use std::collections::BTreeMap;
use std::ffi::c_void;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_ACCESS_TYPE_NAMED_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

use crate::config::{find_model, log_debug, record_usage, AppConfig, ModelKind};
use crate::json::{parse_json, JsonValue};
use crate::win32::to_wide;

fn url_encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

pub fn normalize_proxy_for_winhttp(proxy: &str) -> Option<String> {
    let p = proxy.trim().trim_end_matches('/');
    if p.is_empty() {
        return None;
    }
    let host_port = p
        .strip_prefix("http://")
        .or_else(|| p.strip_prefix("https://"))
        .unwrap_or(p);
    if host_port.is_empty() {
        return None;
    }
    Some(format!("http={};https={}", host_port, host_port))
}

enum HttpError {
    Status(u32, String),
    Transport(u32, String),
}

fn post_json_once(
    host: &str,
    path_and_query: &str,
    body: &str,
    proxy: Option<&str>,
    timeout_secs: u32,
) -> Result<String, HttpError> {
    let ua = to_wide("FloatingTranslator/1.0");
    let (access_type, proxy_wide) = match proxy {
        Some(p) => (WINHTTP_ACCESS_TYPE_NAMED_PROXY, Some(to_wide(p))),
        None => (WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, None),
    };
    let proxy_ptr = proxy_wide
        .as_ref()
        .map(|v| v.as_ptr())
        .unwrap_or(std::ptr::null());

    unsafe {
        let h_session = WinHttpOpen(ua.as_ptr(), access_type, proxy_ptr, std::ptr::null(), 0);
        if h_session.is_null() {
            let err = windows_sys::Win32::Foundation::GetLastError();
            return Err(HttpError::Transport(err, format!("WinHttpOpen failed ({})", err)));
        }

        let t_ms = (timeout_secs.clamp(5, 120) * 1000) as i32;
        let _ = WinHttpSetTimeouts(h_session, t_ms, t_ms, t_ms, t_ms);

        let host_w = to_wide(host);
        let h_connect = WinHttpConnect(h_session, host_w.as_ptr(), 443, 0);
        if h_connect.is_null() {
            let err = windows_sys::Win32::Foundation::GetLastError();
            WinHttpCloseHandle(h_session);
            return Err(HttpError::Transport(err, format!("Connect failed ({})", err)));
        }

        let verb_w = to_wide("POST");
        let path_w = to_wide(path_and_query);
        let h_request = WinHttpOpenRequest(
            h_connect,
            verb_w.as_ptr(),
            path_w.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            WINHTTP_FLAG_SECURE,
        );
        if h_request.is_null() {
            let err = windows_sys::Win32::Foundation::GetLastError();
            WinHttpCloseHandle(h_connect);
            WinHttpCloseHandle(h_session);
            return Err(HttpError::Transport(err, format!("OpenRequest failed ({})", err)));
        }

        let headers_w = to_wide("Content-Type: application/json\r\n");
        let body_bytes = body.as_bytes();
        let sent = WinHttpSendRequest(
            h_request,
            headers_w.as_ptr(),
            u32::MAX,
            body_bytes.as_ptr() as *const c_void,
            body_bytes.len() as u32,
            body_bytes.len() as u32,
            0,
        );
        if sent == 0 {
            let err = windows_sys::Win32::Foundation::GetLastError();
            WinHttpCloseHandle(h_request);
            WinHttpCloseHandle(h_connect);
            WinHttpCloseHandle(h_session);
            return Err(HttpError::Transport(err, format!("SendRequest failed ({})", err)));
        }

        if WinHttpReceiveResponse(h_request, std::ptr::null_mut()) == 0 {
            let err = windows_sys::Win32::Foundation::GetLastError();
            WinHttpCloseHandle(h_request);
            WinHttpCloseHandle(h_connect);
            WinHttpCloseHandle(h_session);
            return Err(HttpError::Transport(err, format!("ReceiveResponse failed ({})", err)));
        }

        let mut status: u32 = 0;
        let mut status_len: u32 = std::mem::size_of::<u32>() as u32;
        let _ = WinHttpQueryHeaders(
            h_request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            &mut status as *mut u32 as *mut c_void,
            &mut status_len,
            std::ptr::null_mut(),
        );

        let mut resp_bytes = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let mut read: u32 = 0;
            if WinHttpReadData(h_request, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut read) == 0 {
                break;
            }
            if read == 0 {
                break;
            }
            resp_bytes.extend_from_slice(&buf[..read as usize]);
        }

        WinHttpCloseHandle(h_request);
        WinHttpCloseHandle(h_connect);
        WinHttpCloseHandle(h_session);

        let text = String::from_utf8_lossy(&resp_bytes).into_owned();
        if (200..300).contains(&status) {
            Ok(text)
        } else {
            Err(HttpError::Status(status, text))
        }
    }
}

fn post_json(host: &str, path_and_query: &str, body: &str, cfg: &AppConfig) -> Result<JsonValue, String> {
    let proxy_norm = normalize_proxy_for_winhttp(&cfg.proxy);
    let attempts = cfg.retries.clamp(0, 5) + 1;
    let timeout = cfg.timeout.clamp(5, 120);

    for attempt in 0..attempts {
        match post_json_once(host, path_and_query, body, proxy_norm.as_deref(), timeout) {
            Ok(raw) => {
                return parse_json(&raw).map_err(|e| format!("خطای ترجمه: پاسخ JSON نامعتبر ({})", e));
            }
            Err(HttpError::Status(code, raw)) => {
                let retryable = matches!(code, 408 | 429 | 500 | 502 | 503 | 504);
                if attempt + 1 < attempts && retryable {
                    log_debug(&format!("retry {}/{} after HTTP {}", attempt + 1, attempts - 1, code));
                    thread::sleep(Duration::from_millis(500 * (attempt as u64 + 1)));
                    continue;
                }
                return Err(format_http_error(code, &raw));
            }
            Err(HttpError::Transport(err_code, msg)) => {
                if attempt + 1 < attempts {
                    log_debug(&format!("retry {}/{} after transport {}", attempt + 1, attempts - 1, err_code));
                    thread::sleep(Duration::from_millis(500 * (attempt as u64 + 1)));
                    continue;
                }
                return Err(format_transport_error(err_code, &msg, cfg, attempts));
            }
        }
    }
    Err("خطای ترجمه: تلاش‌ها به پایان رسید".into())
}

fn format_http_error(code: u32, raw: &str) -> String {
    let mut detail = parse_json(raw)
        .ok()
        .and_then(|v| v.get("error")?.get("message")?.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    if code == 429 {
        let hint = "سهمیه‌ی این مدل پر شده (429) — از فهرست مدل بالای پنل مدل دیگری انتخاب کنید.";
        detail = if detail.is_empty() {
            hint.to_string()
        } else {
            format!("{} {}", hint, detail)
        };
    } else if detail.is_empty() {
        detail = match code {
            401 | 403 => format!(
                "گوگل دسترسی این شبکه را رد کرد ({}). معمولاً یعنی IP مسدود است — VPN یا پروکسی روشن کنید و در تنظیمات مقدار پروکسی را بدهید.",
                code
            ),
            400 => "درخواست رد شد (400) — کلید API و زبان مقصد را چک کنید.".to_string(),
            _ => format!("گوگل خطای HTTP {} برگرداند.", code),
        };
    }
    let clean: String = detail.split_whitespace().collect::<Vec<_>>().join(" ");
    let truncated: String = clean.chars().take(280).collect();
    format!("خطای ترجمه: {}", truncated)
}

fn format_transport_error(err_code: u32, _msg: &str, cfg: &AppConfig, attempts: u32) -> String {
    // 12002 = ERROR_WINHTTP_TIMEOUT
    let detail = if err_code == 12002 {
        let base = format!(
            "زمان پاسخ تمام شد ({} ثانیه × {} تلاش) — شبکه یا پروکسی کند است؛ در تنظیمات «مهلت پاسخ» را بیشتر کنید",
            cfg.timeout, attempts
        );
        if cfg.proxy.trim().is_empty() {
            format!("{}.", base)
        } else {
            format!("{} یا پروکسی دیگری بدهید.", base)
        }
    } else if !cfg.proxy.trim().is_empty() {
        format!(
            "اتصال از طریق پروکسی {} برقرار نشد (کد {}) — کلاینت VPN روشن و متصل است؟",
            cfg.proxy.trim(),
            err_code
        )
    } else {
        format!(
            "اتصال به گوگل برقرار نشد (کد {}) — اینترنت یا پروکسی را چک کنید.",
            err_code
        )
    };
    format!("خطای ترجمه: {}", detail)
}

pub fn html_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        if let Some(semi) = rest.find(';') {
            let entity = &rest[1..semi];
            let decoded = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" | "#x27" => Some('\''),
                "nbsp" => Some(' '),
                e if e.starts_with("#x") || e.starts_with("#X") => {
                    u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32)
                }
                e if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
                _ => None,
            };
            if let Some(ch) = decoded {
                out.push(ch);
                rest = &rest[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

fn translate_gemini(text: &str, cfg: &AppConfig, target: &str, model_id: &str) -> Result<(String, String), String> {
    let key = cfg.api_key.trim();
    let path = format!(
        "/v1beta/models/{}:generateContent?key={}",
        url_encode_component(model_id),
        url_encode_component(key)
    );
    let prompt = format!(
        "Translate the text below into ISO language code '{}'. If it is already in that language, translate it into English instead. Reply with ONLY the translation - no notes, no quotes.\n\n{}",
        target, text
    );

    let mut part = BTreeMap::new();
    part.insert("text".into(), JsonValue::String(prompt));
    let mut content = BTreeMap::new();
    content.insert("parts".into(), JsonValue::Array(vec![JsonValue::Object(part)]));
    let mut root = BTreeMap::new();
    root.insert("contents".into(), JsonValue::Array(vec![JsonValue::Object(content)]));
    let body = JsonValue::Object(root).to_compact_string();

    let payload = post_json("generativelanguage.googleapis.com", &path, &body, cfg)?;
    let prompt_tok = payload
        .get("usageMetadata")
        .and_then(|u| u.get("promptTokenCount"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0) as u64;
    let cand_tok = payload
        .get("usageMetadata")
        .and_then(|u| u.get("candidatesTokenCount"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0) as u64;
    record_usage(key, model_id, prompt_tok, cand_tok);

    let out = payload
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
        .and_then(|a| a.first())
        .and_then(|p| p.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "خطای ترجمه: پاسخ نامعتبر از Gemini".to_string())?;

    Ok((String::new(), out))
}

fn translate_google_once(text: &str, key: &str, target: &str, cfg: &AppConfig) -> Result<(String, String), String> {
    let path = format!("/language/translate/v2?key={}", url_encode_component(key));
    let mut root = BTreeMap::new();
    root.insert("q".into(), JsonValue::Array(vec![JsonValue::String(text.to_string())]));
    root.insert("target".into(), JsonValue::String(target.to_string()));
    root.insert("format".into(), JsonValue::String("text".to_string()));
    let body = JsonValue::Object(root).to_compact_string();

    let payload = post_json("translation.googleapis.com", &path, &body, cfg)?;
    record_usage(key, "google-translate", 0, 0);

    let first = payload
        .get("data")
        .and_then(|d| d.get("translations"))
        .and_then(|t| t.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| "خطای ترجمه: پاسخ نامعتبر از Cloud Translation".to_string())?;
    let detected = first
        .get("detectedSourceLanguage")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let translated = first
        .get("translatedText")
        .and_then(|v| v.as_str())
        .map(html_unescape)
        .ok_or_else(|| "خطای ترجمه: متن ترجمه در پاسخ نبود".to_string())?;
    Ok((detected, translated))
}

pub fn translate(text: &str, cfg: &AppConfig) -> Result<(String, String), String> {
    let key = cfg.api_key.trim();
    if key.is_empty() {
        return Err("خطای ترجمه: کلید API تنظیم نشده — راست‌کلیک → تنظیمات".to_string());
    }
    let target = {
        let t = cfg.target.trim().to_lowercase();
        if t.is_empty() { "fa".to_string() } else { t }
    };
    let model = find_model(&cfg.model);
    match model.kind {
        ModelKind::Gemini => translate_gemini(text, cfg, &target, model.id),
        ModelKind::Google => {
            let (src, out) = translate_google_once(text, key, &target, cfg)?;
            let src_base = src.split('-').next().unwrap_or("");
            let tgt_base = target.split('-').next().unwrap_or("");
            if !src_base.is_empty() && src_base == tgt_base {
                let rev_target = if target.starts_with("en") { "fa" } else { "en" };
                return translate_google_once(text, key, rev_target, cfg);
            }
            Ok((src, out))
        }
    }
}
