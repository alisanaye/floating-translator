use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Security::Cryptography::{
    BCryptCloseAlgorithmProvider, BCryptCreateHash, BCryptDestroyHash, BCryptFinishHash,
    BCryptHashData, BCryptOpenAlgorithmProvider, BCRYPT_ALG_HANDLE, BCRYPT_HASH_HANDLE,
};

use crate::json::{parse_json, JsonValue};

pub const DEFAULT_WIDTH: i32 = 420;
pub const DEFAULT_HEIGHT: i32 = 240;
pub const MAX_CHARS: usize = 3000;
pub const DEFAULT_MODEL: &str = "gemini-4-hydrogen";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Gemini,
    Google,
}

pub struct ModelInfo {
    pub id: &'static str,
    pub kind: ModelKind,
    pub short: &'static str,
    pub label: &'static str,
}

pub const MODELS: &[ModelInfo] = &[
    ModelInfo { id: "gemini-4-hydrogen", kind: ModelKind::Gemini, short: "4 Hydrogen", label: "Gemini 4 Hydrogen" },
    ModelInfo { id: "gemini-3.8-flash", kind: ModelKind::Gemini, short: "3.8 Flash", label: "Gemini 3.8 Flash" },
    ModelInfo { id: "gemini-3.7-flash", kind: ModelKind::Gemini, short: "3.7 Flash", label: "Gemini 3.7 Flash" },
    ModelInfo { id: "gemini-3.6-flash", kind: ModelKind::Gemini, short: "3.6 Flash", label: "Gemini 3.6 Flash" },
    ModelInfo { id: "gemini-3.5-flash", kind: ModelKind::Gemini, short: "3.5 Flash", label: "Gemini 3.5 Flash" },
    ModelInfo { id: "gemini-3.5-flash-lite", kind: ModelKind::Gemini, short: "3.5 Flash Lite", label: "Gemini 3.5 Flash Lite" },
    ModelInfo { id: "gemini-3.1-flash-lite", kind: ModelKind::Gemini, short: "3.1 Flash Lite", label: "Gemini 3.1 Flash Lite" },
    ModelInfo { id: "gemini-3-flash-preview", kind: ModelKind::Gemini, short: "3 Flash", label: "Gemini 3 Flash (preview)" },
    ModelInfo { id: "gemini-2.5-flash", kind: ModelKind::Gemini, short: "2.5 Flash", label: "Gemini 2.5 Flash" },
    ModelInfo { id: "gemini-2.5-flash-lite", kind: ModelKind::Gemini, short: "2.5 Flash Lite", label: "Gemini 2.5 Flash Lite" },
    ModelInfo { id: "google-translate", kind: ModelKind::Google, short: "Translate v2", label: "Cloud Translation v2" },
];

pub fn find_model(id: &str) -> &'static ModelInfo {
    MODELS.iter().find(|m| m.id == id).unwrap_or(&MODELS[0])
}

pub fn model_limits(id: &str) -> (Option<u32>, Option<u32>, Option<u32>) {
    match id {
        "gemini-3.5-flash-lite" | "gemini-3.1-flash-lite" => (Some(15), Some(250_000), Some(500)),
        "gemini-3-flash-preview" | "gemini-2.5-flash" => (Some(5), Some(250_000), Some(20)),
        "gemini-2.5-flash-lite" => (Some(10), Some(250_000), Some(20)),
        _ => (None, None, None),
    }
}

pub fn is_rtl_lang(target: &str) -> bool {
    let base = target.split('-').next().unwrap_or("").to_lowercase();
    matches!(base.as_str(), "fa" | "ar" | "he" | "ur")
}

pub fn app_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

pub fn usage_path() -> PathBuf {
    app_dir().join("usage.json")
}

static USAGE_LOCK: Mutex<()> = Mutex::new(());
static DEBUG_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_debug(enabled: bool) {
    DEBUG_ENABLED.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

pub fn log_debug(msg: &str) {
    if !DEBUG_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let path = app_dir().join("debug.log");
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("{} {}\n", secs, msg);
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write;
        let _ = f.write_all(line.as_bytes());
    }
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub model: String,
    pub api_key: String,
    pub api_keys: Vec<String>,
    pub target: String,
    pub proxy: String,
    pub timeout: u32,
    pub retries: u32,
    pub rtl: Option<bool>,
    pub autocopy: bool,
    pub watch: bool,
    pub opacity: f32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub theme: String,
    pub locked: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            api_key: String::new(),
            api_keys: Vec::new(),
            target: "fa".to_string(),
            proxy: String::new(),
            timeout: 20,
            retries: 2,
            rtl: None,
            autocopy: false,
            watch: true,
            opacity: 1.0,
            x: 60,
            y: 60,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            theme: "dark".to_string(),
            locked: false,
        }
    }
}

impl AppConfig {
    pub fn effective_rtl(&self) -> bool {
        self.rtl.unwrap_or_else(|| is_rtl_lang(&self.target))
    }

    pub fn load() -> Self {
        let mut cfg = Self::default();
        let Ok(raw_str) = fs::read_to_string(config_path()) else {
            return cfg;
        };
        // Notepad and friends love a UTF-8 BOM; without this the whole config silently resets.
        let raw_str = raw_str.strip_prefix('\u{feff}').unwrap_or(&raw_str);
        let Ok(JsonValue::Object(map)) = parse_json(raw_str) else {
            return cfg;
        };

        if let Some(m) = map.get("model").and_then(|v| v.as_str()) {
            if MODELS.iter().any(|mi| mi.id == m) {
                cfg.model = m.to_string();
            }
        } else {
            // Legacy migration
            if map.get("engine").and_then(|v| v.as_str()) == Some("google") {
                cfg.model = "google-translate".to_string();
            } else if let Some(leg) = map.get("gemini_model").and_then(|v| v.as_str()) {
                if MODELS.iter().any(|mi| mi.id == leg) {
                    cfg.model = leg.to_string();
                }
            }
        }

        if let Some(s) = map.get("target").and_then(|v| v.as_str()) {
            let t = s.trim();
            if !t.is_empty() {
                cfg.target = t.to_string();
            }
        }
        if let Some(s) = map.get("proxy").and_then(|v| v.as_str()) {
            cfg.proxy = s.trim().to_string();
        }
        if let Some(n) = map.get("timeout").and_then(|v| v.as_i64()) {
            cfg.timeout = n.clamp(5, 120) as u32;
        }
        if let Some(n) = map.get("retries").and_then(|v| v.as_i64()) {
            cfg.retries = n.clamp(0, 5) as u32;
        }
        if let Some(v) = map.get("rtl") {
            cfg.rtl = v.as_bool();
        }
        if let Some(b) = map.get("autocopy").and_then(|v| v.as_bool()) {
            cfg.autocopy = b;
        }
        if let Some(b) = map.get("watch").and_then(|v| v.as_bool()) {
            cfg.watch = b;
        }
        if let Some(n) = map.get("opacity").and_then(|v| v.as_f64()) {
            cfg.opacity = (n as f32).clamp(0.3, 1.0);
        }
        if let Some(n) = map.get("x").and_then(|v| v.as_i64()) {
            cfg.x = n as i32;
        }
        if let Some(n) = map.get("y").and_then(|v| v.as_i64()) {
            cfg.y = n as i32;
        }
        if let Some(n) = map.get("width").and_then(|v| v.as_i64()) {
            cfg.width = (n as i32).clamp(340, 900);
        }
        if let Some(n) = map.get("height").and_then(|v| v.as_i64()) {
            cfg.height = (n as i32).clamp(170, 700);
        }
        if let Some(s) = map.get("theme").and_then(|v| v.as_str()) {
            cfg.theme = if s == "light" { "light".into() } else { "dark".into() };
        }
        if let Some(b) = map.get("locked").and_then(|v| v.as_bool()) {
            cfg.locked = b;
        }

        let mut keys = Vec::new();
        if let Some(arr) = map.get("api_keys").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(k) = item.as_str() {
                    let k = k.trim();
                    if !k.is_empty() && !keys.iter().any(|x| x == k) {
                        keys.push(k.to_string());
                    }
                }
            }
        }
        let active = map
            .get("api_key")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if !active.is_empty() && !keys.iter().any(|x| x == &active) {
            keys.insert(0, active.clone());
        }
        cfg.api_key = if !active.is_empty() {
            active
        } else {
            keys.first().cloned().unwrap_or_default()
        };
        cfg.api_keys = keys;
        cfg
    }

    pub fn save(&self) {
        let mut map = BTreeMap::new();
        map.insert("model".into(), JsonValue::String(self.model.clone()));
        map.insert("api_key".into(), JsonValue::String(self.api_key.clone()));
        map.insert(
            "api_keys".into(),
            JsonValue::Array(self.api_keys.iter().cloned().map(JsonValue::String).collect()),
        );
        map.insert("target".into(), JsonValue::String(self.target.clone()));
        map.insert("proxy".into(), JsonValue::String(self.proxy.clone()));
        map.insert("timeout".into(), JsonValue::Number(self.timeout as f64));
        map.insert("retries".into(), JsonValue::Number(self.retries as f64));
        map.insert(
            "rtl".into(),
            match self.rtl {
                Some(b) => JsonValue::Bool(b),
                None => JsonValue::Null,
            },
        );
        map.insert("autocopy".into(), JsonValue::Bool(self.autocopy));
        map.insert("watch".into(), JsonValue::Bool(self.watch));
        map.insert("opacity".into(), JsonValue::Number((self.opacity as f64 * 100.0).round() / 100.0));
        map.insert("x".into(), JsonValue::Number(self.x as f64));
        map.insert("y".into(), JsonValue::Number(self.y as f64));
        map.insert("width".into(), JsonValue::Number(self.width as f64));
        map.insert("height".into(), JsonValue::Number(self.height as f64));
        map.insert("theme".into(), JsonValue::String(self.theme.clone()));
        map.insert("locked".into(), JsonValue::Bool(self.locked));

        let json_str = JsonValue::Object(map).to_pretty_string();
        let _ = fs::write(config_path(), json_str);
    }
}

pub fn key_id(key: &str) -> String {
    let mut alg: BCRYPT_ALG_HANDLE = std::ptr::null_mut();
    let mut hash: BCRYPT_HASH_HANDLE = std::ptr::null_mut();
    let alg_name: Vec<u16> = "SHA256\0".encode_utf16().collect();
    let mut digest = [0u8; 32];
    unsafe {
        if BCryptOpenAlgorithmProvider(&mut alg, alg_name.as_ptr(), std::ptr::null(), 0) == 0 {
            if BCryptCreateHash(alg, &mut hash, std::ptr::null_mut(), 0, std::ptr::null(), 0, 0) == 0 {
                let bytes = key.as_bytes();
                let _ = BCryptHashData(hash, bytes.as_ptr(), bytes.len() as u32, 0);
                let _ = BCryptFinishHash(hash, digest.as_mut_ptr(), digest.len() as u32, 0);
                BCryptDestroyHash(hash);
            }
            BCryptCloseAlgorithmProvider(alg, 0);
        }
    }
    let mut hex = String::with_capacity(12);
    for b in &digest[..6] {
        hex.push_str(&format!("{:02x}", b));
    }
    hex
}

pub fn key_label(key: &str) -> String {
    if key.is_empty() {
        return "بدون کلید".to_string();
    }
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 6 {
        format!("…{}", key)
    } else {
        let tail: String = chars[chars.len() - 6..].iter().collect();
        format!("…{}", tail)
    }
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn read_usage_map() -> BTreeMap<String, JsonValue> {
    let Ok(s) = fs::read_to_string(usage_path()) else {
        return BTreeMap::new();
    };
    match parse_json(&s) {
        Ok(JsonValue::Object(m)) => m,
        _ => BTreeMap::new(),
    }
}

pub fn record_usage(key: &str, model: &str, prompt_tokens: u64, candidates_tokens: u64) {
    if key.is_empty() {
        return;
    }
    let now = now_secs();
    let kid = key_id(key);
    let Ok(_guard) = USAGE_LOCK.lock() else {
        return;
    };
    let mut data = read_usage_map();
    let bucket_val = data
        .entry(kid)
        .or_insert_with(|| JsonValue::Object(BTreeMap::new()));
    if !matches!(bucket_val, JsonValue::Object(_)) {
        *bucket_val = JsonValue::Object(BTreeMap::new());
    }
    let Some(bucket) = bucket_val.as_object_mut() else {
        return;
    };
    let mut events = Vec::new();
    if let Some(JsonValue::Array(arr)) = bucket.get(model) {
        for item in arr {
            if let Some(ev) = item.as_array() {
                if ev.len() >= 3 {
                    if let Some(ts) = ev[0].as_f64() {
                        if ts >= now - 86400.0 {
                            events.push(item.clone());
                        }
                    }
                }
            }
        }
    }
    events.push(JsonValue::Array(vec![
        JsonValue::Number(now),
        JsonValue::Number(prompt_tokens as f64),
        JsonValue::Number(candidates_tokens as f64),
    ]));
    bucket.insert(model.to_string(), JsonValue::Array(events));

    let path = usage_path();
    let tmp = path.with_extension("json.tmp");
    let out = JsonValue::Object(data).to_compact_string();
    if fs::write(&tmp, out).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

pub struct UsageRow {
    pub key: String,
    pub model_id: &'static str,
    pub rpm: usize,
    pub tpm: u64,
    pub rpd: usize,
    pub limits: (Option<u32>, Option<u32>, Option<u32>),
}

pub fn usage_rows(keys: &[String]) -> Vec<UsageRow> {
    let now = now_secs();
    let data = read_usage_map();
    let mut rows = Vec::new();
    for key in keys {
        let kid = key_id(key);
        let bucket = data.get(&kid).and_then(|v| v.as_object());
        for m in MODELS {
            let mut rpm = 0usize;
            let mut tpm = 0u64;
            let mut rpd = 0usize;
            if let Some(arr) = bucket.and_then(|b| b.get(m.id)).and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(ev) = item.as_array() {
                        if ev.len() >= 3 {
                            if let Some(ts) = ev[0].as_f64() {
                                let age = now - ts;
                                if age < 86400.0 {
                                    rpd += 1;
                                    if age < 60.0 {
                                        rpm += 1;
                                        tpm += ev[1].as_f64().unwrap_or(0.0) as u64;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            rows.push(UsageRow {
                key: key.clone(),
                model_id: m.id,
                rpm,
                tpm,
                rpd,
                limits: model_limits(m.id),
            });
        }
    }
    rows
}
