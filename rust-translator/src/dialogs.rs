//! Settings window and the keys/usage window.
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC,
    DeleteObject, EndPaint, InvalidateRect, SelectObject, SRCCOPY,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ESCAPE,
    VK_RETURN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateCaret, CreateWindowExW, DefWindowProcW, DestroyCaret, DestroyWindow, GetClientRect,
    GetCursorPos, GetWindowRect, HideCaret, LoadCursorW, RegisterClassW, SetCaretPos, SetCursor,
    SetForegroundWindow, SetTimer, SetWindowPos, ShowCaret, ShowWindow, CS_HREDRAW, CS_VREDRAW,
    IDC_ARROW, IDC_HAND, SWP_NOSIZE, SWP_NOZORDER, SW_SHOW, WM_CHAR, WM_DESTROY, WM_ERASEBKGND,
    WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT,
    WM_SETFOCUS, WM_SETCURSOR, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::config::{self, AppConfig, UsageRow};
use crate::gdiplus::{GpCanvas, TextAlign};
use crate::menu::{self, ItemKind, MenuAction, MenuItem};
use crate::theme::Theme;
use crate::ui::{self, rect, wrap_lines, Field, Hits, Id};
use crate::win32::to_wide;
use crate::{app, with_canvas, App};

const F_LABEL: f32 = 12.0;
const F_SMALL: f32 = 11.0;
const F_ROW: f32 = 12.0;
const F_TITLE: f32 = 12.5;
const ROW_H: f32 = 32.0;

// ------------------------------------------------------------------ state
pub struct SettingsWin {
    pub hwnd: HWND,
    pub hits: Hits,
    pub hover: Option<Id>,
    pub key: Field,
    pub target: Field,
    pub proxy: Field,
    pub timeout: Field,
    pub retries: Field,
    pub model: String,
    pub show_key: bool,
    pub test_msg: String,
    pub testing: bool,
    pub focus: Option<Id>,
    pub drag: Option<(i32, i32)>,
    pub closed: bool,
}

pub struct KeysWin {
    pub hwnd: HWND,
    pub hits: Hits,
    pub hover: Option<Id>,
    pub new_key: Field,
    pub show_key: bool,
    pub selected: Option<usize>,
    pub only_active: bool,
    pub rows: Vec<UsageRow>,
    pub status: String,
    pub focus: Option<Id>,
    pub drag: Option<(i32, i32)>,
    pub list_top: usize,
    pub closed: bool,
}

static TEST: Mutex<Option<Result<(String, String), String>>> = Mutex::new(None);

impl SettingsWin {
    fn field(&self, id: Id) -> Option<&Field> {
        match id {
            Id::SKey => Some(&self.key),
            Id::STarget => Some(&self.target),
            Id::SProxy => Some(&self.proxy),
            Id::STimeout => Some(&self.timeout),
            Id::SRetries => Some(&self.retries),
            _ => None,
        }
    }

    fn field_mut(&mut self, id: Id) -> Option<&mut Field> {
        match id {
            Id::SKey => Some(&mut self.key),
            Id::STarget => Some(&mut self.target),
            Id::SProxy => Some(&mut self.proxy),
            Id::STimeout => Some(&mut self.timeout),
            Id::SRetries => Some(&mut self.retries),
            _ => None,
        }
    }
}

// ------------------------------------------------------------------ window classes
type WndProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

pub fn register_classes() {
    unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let procs: [(Vec<u16>, WndProc); 2] =
            [(to_wide("FTSettings"), settings_proc), (to_wide("FTKeys"), keys_proc)];
        for (name, proc_) in procs {
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.style = CS_HREDRAW | CS_VREDRAW;
            wc.lpfnWndProc = Some(proc_);
            wc.hInstance = hinst;
            wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
            wc.lpszClassName = name.as_ptr();
            RegisterClassW(&wc);
        }
    }
}

fn create_window(class: &str, title: &str, w: i32, h: i32, owner: HWND) -> HWND {
    unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let mut owner_rect: RECT = std::mem::zeroed();
        GetWindowRect(owner, &mut owner_rect);
        let wa = crate::win32::work_area();
        let x = (owner_rect.left + 24).clamp(wa.left, (wa.right - w).max(wa.left));
        let y = (owner_rect.top + 32).clamp(wa.top, (wa.bottom - h).max(wa.top));
        let cls = to_wide(class);
        let name = to_wide(title);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            cls.as_ptr(),
            name.as_ptr(),
            WS_POPUP,
            x,
            y,
            w,
            h,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null_mut(),
        );
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);
        SetTimer(hwnd, 1, 100, None);
        hwnd
    }
}

// ------------------------------------------------------------------ open / close
pub fn open_settings(app: &mut App) {
    if let Some(w) = app.settings.as_ref() {
        unsafe {
            SetForegroundWindow(w.hwnd);
            SetFocus(w.hwnd);
        }
        return;
    }
    let win = SettingsWin {
        hwnd: std::ptr::null_mut(),
        hits: Hits::default(),
        hover: None,
        key: Field::new(Id::SKey, app.cfg.api_key.clone(), true, 200),
        target: Field::new(Id::STarget, app.cfg.target.clone(), false, 12),
        proxy: Field::new(Id::SProxy, app.cfg.proxy.clone(), false, 120),
        timeout: Field::new(Id::STimeout, app.cfg.timeout.to_string(), false, 4),
        retries: Field::new(Id::SRetries, app.cfg.retries.to_string(), false, 2),
        model: app.cfg.model.clone(),
        show_key: false,
        test_msg: String::new(),
        testing: false,
        focus: None,
        drag: None,
        closed: false,
    };
    let hwnd = create_window("FTSettings", "تنظیمات", 420, 352, app.hwnd);
    let mut win = Box::new(win);
    win.hwnd = hwnd;
    app.settings = Some(win);
}

pub fn open_keys(app: &mut App) {
    if let Some(w) = app.keys.as_ref() {
        unsafe {
            SetForegroundWindow(w.hwnd);
            SetFocus(w.hwnd);
        }
        return;
    }
    let win = KeysWin {
        hwnd: std::ptr::null_mut(),
        hits: Hits::default(),
        hover: None,
        new_key: Field::new(Id::KNew, String::new(), true, 200),
        show_key: false,
        selected: None,
        only_active: true,
        rows: Vec::new(),
        status: "کلیدها فقط در config.json همین کامپیوتر ذخیره می‌شوند.".to_string(),
        focus: None,
        drag: None,
        list_top: 0,
        closed: false,
    };
    let hwnd = create_window("FTKeys", "کلیدها و مصرف", 720, 548, app.hwnd);
    let mut win = Box::new(win);
    win.hwnd = hwnd;
    app.keys = Some(win);
    refresh_usage(app);
}

pub fn close_settings(app: &mut App) {
    if let Some(w) = app.settings.as_ref() {
        unsafe {
            DestroyWindow(w.hwnd);
        }
    }
    app.settings = None;
}

pub fn close_keys(app: &mut App) {
    if let Some(w) = app.keys.as_ref() {
        unsafe {
            DestroyWindow(w.hwnd);
        }
    }
    app.keys = None;
}

pub fn close_all(app: &mut App) {
    close_settings(app);
    close_keys(app);
}

/// Called from the main timer: applies async test results and drops closed windows.
pub fn tick(app: &mut App) {
    let tested = TEST.lock().ok().and_then(|mut slot| slot.take());
    if let Some(res) = tested {
        if let Some(w) = app.settings.as_mut() {
            w.testing = false;
            w.test_msg = match res {
                Ok((_src, out)) => format!("✓ اتصال درست است — {}", out.chars().take(40).collect::<String>()),
                Err(e) => format!("✗ {}", e.chars().take(200).collect::<String>()),
            };
            invalidate(w.hwnd);
        }
    }
    if app.settings.as_ref().map(|w| w.closed) == Some(true) {
        app.settings = None;
    }
    if app.keys.as_ref().map(|w| w.closed) == Some(true) {
        app.keys = None;
    }
}

fn invalidate(hwnd: HWND) {
    unsafe {
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }
}

/// Applies a model chosen from the settings window's dropdown.
pub fn pick_settings_model(app: &mut App, model: &str) {
    if let Some(w) = app.settings.as_mut() {
        w.model = model.to_string();
        invalidate(w.hwnd);
    }
}

pub fn refresh_usage(app: &mut App) {
    let keys: Vec<String> = match app.keys.as_ref() {
        Some(w) if w.only_active && !app.cfg.api_key.trim().is_empty() => vec![app.cfg.api_key.clone()],
        _ => app.cfg.api_keys.clone(),
    };
    let rows = config::usage_rows(&keys);
    if let Some(w) = app.keys.as_mut() {
        w.rows = rows;
        invalidate(w.hwnd);
    }
}

// ------------------------------------------------------------------ caret
fn sync_caret(hwnd: HWND, field: Option<&Field>) {
    unsafe {
        let Some(f) = field else {
            DestroyCaret();
            return;
        };
        let (x, y, ch) = with_canvas(|c| (f.caret_x(c), f.caret_y(), f.caret_h()));
        CreateCaret(hwnd, std::ptr::null_mut(), 1, ch);
        SetCaretPos(x as i32, y as i32);
        ShowCaret(hwnd);
    }
}

// ------------------------------------------------------------------ settings window
unsafe extern "system" fn settings_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let app = app();
    if app.settings.as_ref().map(|w| w.hwnd) != Some(hwnd) && msg != WM_PAINT {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    match msg {
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            paint_settings(app, hdc, rc.right as f32, rc.bottom as f32);
            EndPaint(hwnd, &ps);
            0
        }
        WM_ERASEBKGND | WM_TIMER => 0,
        WM_MOUSEMOVE => {
            let (x, y) = mouse_xy(lparam);
            let Some(w) = app.settings.as_mut() else { return 0 };
            if let Some((gx, gy)) = w.drag {
                drag_move(hwnd, gx, gy);
                return 0;
            }
            let hit = w.hits.hit(x, y);
            if hit != w.hover {
                w.hover = hit;
                track_leave(hwnd);
                invalidate(hwnd);
            }
            0
        }
        WM_MOUSELEAVE => {
            if let Some(w) = app.settings.as_mut() {
                w.hover = None;
                invalidate(hwnd);
            }
            0
        }
        WM_LBUTTONDOWN => {
            let (x, y) = mouse_xy(lparam);
            let Some(id) = app.settings.as_ref().and_then(|w| w.hits.hit(x, y)) else { return 0 };
            match id {
                Id::SBar => {
                    if let Some(w) = app.settings.as_mut() {
                        w.drag = Some(drag_start(hwnd));
                    }
                    SetCapture(hwnd);
                }
                Id::SClose | Id::SCancel => {
                    DestroyWindow(hwnd);
                }
                Id::SEye => {
                    if let Some(w) = app.settings.as_mut() {
                        w.show_key = !w.show_key;
                        w.key.masked = !w.show_key;
                        invalidate(hwnd);
                    }
                }
                Id::SModel => {
                    let cur = app.settings.as_ref().map(|w| w.model.clone()).unwrap_or_default();
                    let items: Vec<MenuItem> = config::MODELS
                        .iter()
                        .map(|m| MenuItem {
                            label: m.label.to_string(),
                            kind: ItemKind::Check,
                            checked: cur == m.id,
                            action: Some(MenuAction::PickSettingsModel(m.id.to_string())),
                            accent: false,
                        })
                        .collect();
                    let pt = menu_anchor(app, Id::SModel);
                    menu::open(app, items, pt.0, pt.1);
                }
                Id::STest => run_test(app),
                Id::SSave => save_settings(app),
                _ => {
                    if let Some(w) = app.settings.as_mut() {
                        if w.field_mut(id).is_some() {
                            if let Some(f) = w.field_mut(id) {
                                with_canvas(|c| f.click(c, x));
                            }
                            w.focus = Some(id);
                            sync_caret(hwnd, w.field(id));
                            invalidate(hwnd);
                        }
                    }
                }
            }
            0
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            if let Some(w) = app.settings.as_mut() {
                w.drag = None;
            }
            0
        }
        WM_KEYDOWN => {
            let vk = wparam as i32;
            if vk == VK_ESCAPE as i32 {
                DestroyWindow(hwnd);
                return 0;
            }
            if vk == VK_RETURN as i32 {
                save_settings(app);
                return 0;
            }
            if let Some(w) = app.settings.as_mut() {
                if let Some(id) = w.focus {
                    let handled = w.field_mut(id).map(|f| f.on_key(vk)).unwrap_or(false);
                    if handled {
                        sync_caret(hwnd, w.field(id));
                        invalidate(hwnd);
                    }
                }
            }
            0
        }
        WM_CHAR => {
            if let Some(ch) = char::from_u32(wparam as u32) {
                if ch >= ' ' {
                    if let Some(w) = app.settings.as_mut() {
                        if let Some(id) = w.focus {
                            if let Some(f) = w.field_mut(id) {
                                f.on_char(ch);
                                sync_caret(hwnd, w.field(id));
                                invalidate(hwnd);
                            }
                        }
                    }
                }
            }
            0
        }
        WM_SETFOCUS => {
            if let Some(w) = app.settings.as_ref() {
                sync_caret(hwnd, w.focus.and_then(|id| w.field(id)));
            }
            0
        }
        WM_KILLFOCUS => {
            unsafe {
                HideCaret(hwnd);
            }
            0
        }
        WM_SETCURSOR => {
            set_hand_cursor(app.settings.as_ref().and_then(|w| w.hover).is_some());
            1
        }
        WM_DESTROY => {
            if let Some(w) = app.settings.as_mut() {
                w.closed = true;
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

type SettingsVals = (String, String, String, String, u32, u32);

fn read_settings(app: &App) -> Option<SettingsVals> {
    let w = app.settings.as_ref()?;
    let target = w.target.text.trim().to_lowercase();
    Some((
        w.model.clone(),
        w.key.text.trim().to_string(),
        if target.is_empty() { "fa".to_string() } else { target },
        w.proxy.text.trim().to_string(),
        parse_int(&w.timeout.text, app.cfg.timeout, 5, 120),
        parse_int(&w.retries.text, app.cfg.retries, 0, 5),
    ))
}

fn parse_int(text: &str, default: u32, lo: u32, hi: u32) -> u32 {
    text.trim().parse::<u32>().unwrap_or(default).clamp(lo, hi)
}

fn save_settings(app: &mut App) {
    let Some((model, key, target, proxy, timeout, retries)) = read_settings(app) else { return };
    app.cfg.model = model;
    app.cfg.api_key = key.clone();
    if !key.is_empty() && !app.cfg.api_keys.iter().any(|k| k == &key) {
        app.cfg.api_keys.push(key);
    }
    app.cfg.target = target;
    app.cfg.proxy = proxy;
    app.cfg.timeout = timeout;
    app.cfg.retries = retries;
    app.cfg.save();
    app.last_text = None;
    app.set_status("ذخیره شد");
    close_settings(app);
}

fn run_test(app: &mut App) {
    let Some((model, key, target, proxy, timeout, retries)) = read_settings(app) else { return };
    let cfg = AppConfig {
        model,
        api_key: key,
        target,
        proxy,
        timeout,
        retries,
        ..AppConfig::default()
    };
    if let Some(w) = app.settings.as_mut() {
        w.testing = true;
        w.test_msg = "در حال تست...".to_string();
        invalidate(w.hwnd);
    }
    std::thread::spawn(move || {
        let res = crate::http::translate("Hello, this is a connection test.", &cfg);
        if let Ok(mut slot) = TEST.lock() {
            *slot = Some(res);
        }
    });
}

fn paint_settings(app: &mut App, hdc: windows_sys::Win32::Graphics::Gdi::HDC, w: f32, h: f32) {
    let mem = unsafe { CreateCompatibleDC(hdc) };
    let bmp = unsafe { CreateCompatibleBitmap(hdc, w as i32, h as i32) };
    let old = unsafe { SelectObject(mem, bmp) };
    let t = app.theme;
    if let Some(c) = GpCanvas::from_hdc(mem) {
        if let Some(win) = app.settings.as_mut() {
            draw_settings(win, &c, w, h, &t);
        }
    }
    unsafe {
        BitBlt(hdc, 0, 0, w as i32, h as i32, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
    }
}

fn draw_settings(win: &mut SettingsWin, c: &GpCanvas, w: f32, h: f32, t: &Theme) {
    let hover = win.hover;
    win.hits.clear();
    c.fill_rect(0.0, 0.0, w, h, t.border);
    c.fill_rect(1.0, 1.0, w - 2.0, h - 2.0, t.bg);
    c.fill_rect(1.0, 1.0, w - 2.0, 32.0, t.bar_bg);
    c.fill_rect(1.0, 33.0, w - 2.0, 1.0, t.divider);
    win.hits.add(Id::SBar, rect(1.0, 1.0, w - 2.0, 32.0));
    c.draw_icon("key", 18.0, 17.0, 13.0, t.accent);
    let title = "تنظیمات";
    let tw = c.measure_line_width(title, F_TITLE, true);
    c.draw_text(title, rect(30.0, 1.0, tw + 2.0, 32.0), F_TITLE, true, t.fg, true, TextAlign::Near, true, false);
    let close_r = rect(w - 30.0, 4.0, 26.0, 26.0);
    ui::button(
        c,
        &mut win.hits,
        &ui::Btn::new(Id::SClose, close_r, if hover == Some(Id::SClose) { 0xFFFFFFFF } else { t.fg_muted })
            .icon("close", 11.0)
            .bg(if hover == Some(Id::SClose) { t.close_hover } else { t.bar_bg }),
    );

    let label_right = 148.0;
    let field_x = 158.0;
    let mut y = 45.0;
    let draw_label = |c: &GpCanvas, text: &str, y: f32| {
        let tw = c.measure_line_width(text, F_LABEL, false);
        c.draw_text(
            text,
            rect(label_right - tw, y, tw + 2.0, 24.0),
            F_LABEL,
            false,
            t.fg,
            true,
            TextAlign::Near,
            true,
            false,
        );
    };

    draw_label(c, "کلید Google API", y);
    win.key.rect = rect(field_x, y, 210.0, 24.0);
    win.key.draw(c, t);
    win.hits.add(Id::SKey, win.key.rect);
    let eye_r = rect(field_x + 214.0, y, 26.0, 24.0);
    ui::button(
        c,
        &mut win.hits,
        &ui::Btn::new(Id::SEye, eye_r, if win.show_key { t.accent } else { t.fg_muted })
            .icon(if win.show_key { "eye_off" } else { "eye" }, 13.0)
            .bg(if hover == Some(Id::SEye) { t.hover_bg } else { t.card_bg }),
    );
    y += ROW_H;

    draw_label(c, "مدل", y);
    let model_r = rect(field_x, y, 240.0, 24.0);
    let mlabel = format!("{} ▾", config::find_model(&win.model).label);
    c.fill_round_rect(model_r.x, model_r.y, model_r.width, model_r.height, 4.0, t.card_bg);
    c.draw_round_rect(model_r.x, model_r.y, model_r.width, model_r.height, 4.0, t.border, 1.0);
    c.draw_text(&mlabel, rect(model_r.x + 6.0, y, model_r.width - 12.0, 24.0), F_LABEL, false, t.fg, false, TextAlign::Near, true, false);
    win.hits.add(Id::SModel, model_r);
    y += ROW_H;

    draw_label(c, "زبان مقصد", y);
    win.target.rect = rect(field_x, y, 90.0, 24.0);
    win.target.draw(c, t);
    win.hits.add(Id::STarget, win.target.rect);
    y += ROW_H;

    draw_label(c, "پروکسی (اختیاری)", y);
    win.proxy.rect = rect(field_x, y, 240.0, 24.0);
    win.proxy.draw(c, t);
    win.hits.add(Id::SProxy, win.proxy.rect);
    y += ROW_H;

    draw_label(c, "مهلت پاسخ (ثانیه)", y);
    win.timeout.rect = rect(field_x, y, 60.0, 24.0);
    win.timeout.draw(c, t);
    win.hits.add(Id::STimeout, win.timeout.rect);
    y += ROW_H;

    draw_label(c, "تلاش مجدد", y);
    win.retries.rect = rect(field_x, y, 60.0, 24.0);
    win.retries.draw(c, t);
    win.hits.add(Id::SRetries, win.retries.rect);
    y += ROW_H + 2.0;

    let notes = [
        "هر مدل سهمیه‌ی روزانه‌ی جدا دارد؛ اگر یکی پر شد مدل دیگری را انتخاب کنید.",
        "پروکسی: اگر گوگل دسترسی را رد کرد، مثل http://127.0.0.1:10809",
    ];
    for (i, line) in notes.iter().enumerate() {
        c.draw_text(line, rect(14.0, y + i as f32 * 15.0, w - 28.0, 15.0), F_SMALL, false, t.fg_dim, true, TextAlign::Near, true, false);
    }
    y += 34.0;

    let bw = 92.0;
    let bx = (w - (bw * 3.0 + 16.0)) / 2.0;
    ui::button(
        c,
        &mut win.hits,
        &ui::Btn::new(Id::STest, rect(bx, y, bw, 28.0), t.fg)
            .text("تست اتصال", F_LABEL)
            .bg(if hover == Some(Id::STest) { t.hover_bg } else { t.btn_bg })
            .radius(4.0),
    );
    ui::button(
        c,
        &mut win.hits,
        &ui::Btn::new(Id::SSave, rect(bx + bw + 8.0, y, bw, 28.0), t.accent_fg)
            .text("ذخیره", F_LABEL)
            .bold()
            .bg(if hover == Some(Id::SSave) { t.accent_hover } else { t.accent })
            .radius(4.0),
    );
    ui::button(
        c,
        &mut win.hits,
        &ui::Btn::new(Id::SCancel, rect(bx + (bw + 8.0) * 2.0, y, bw, 28.0), t.fg_muted)
            .text("بستن", F_LABEL)
            .bg(if hover == Some(Id::SCancel) { t.hover_bg } else { t.btn_bg })
            .radius(4.0),
    );
    y += 36.0;

    let msg = win.test_msg.clone();
    if !msg.is_empty() {
        let color = if win.testing {
            t.fg_muted
        } else if msg.starts_with('✓') {
            t.accent_green
        } else {
            t.err_fg
        };
        let lines = with_canvas(|c| wrap_lines(c, &msg, w - 28.0, F_SMALL, false));
        for (i, line) in lines.iter().take(2).enumerate() {
            c.draw_text(line, rect(14.0, y + i as f32 * 15.0, w - 28.0, 15.0), F_SMALL, false, color, true, TextAlign::Near, true, false);
        }
    }
}

// ------------------------------------------------------------------ keys window
unsafe extern "system" fn keys_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let app = app();
    if app.keys.as_ref().map(|w| w.hwnd) != Some(hwnd) && msg != WM_PAINT {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    match msg {
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            paint_keys(app, hdc, rc.right as f32, rc.bottom as f32);
            EndPaint(hwnd, &ps);
            0
        }
        WM_ERASEBKGND | WM_TIMER => 0,
        WM_MOUSEMOVE => {
            let (x, y) = mouse_xy(lparam);
            let Some(w) = app.keys.as_mut() else { return 0 };
            if let Some((gx, gy)) = w.drag {
                drag_move(hwnd, gx, gy);
                return 0;
            }
            let hit = w.hits.hit(x, y);
            if hit != w.hover {
                w.hover = hit;
                track_leave(hwnd);
                invalidate(hwnd);
            }
            0
        }
        WM_MOUSELEAVE => {
            if let Some(w) = app.keys.as_mut() {
                w.hover = None;
                invalidate(hwnd);
            }
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) & 0xFFFF) as i16 as i32;
            let total = app.cfg.api_keys.len();
            if let Some(w) = app.keys.as_mut() {
                if delta < 0 {
                    w.list_top = (w.list_top + 1).min(total.saturating_sub(1));
                } else {
                    w.list_top = w.list_top.saturating_sub(1);
                }
                invalidate(hwnd);
            }
            0
        }
        WM_LBUTTONDOWN => {
            let (x, y) = mouse_xy(lparam);
            let Some(id) = app.keys.as_ref().and_then(|w| w.hits.hit(x, y)) else { return 0 };
            match id {
                Id::KBar => {
                    if let Some(w) = app.keys.as_mut() {
                        w.drag = Some(drag_start(hwnd));
                    }
                    SetCapture(hwnd);
                }
                Id::KClose => {
                    DestroyWindow(hwnd);
                }
                Id::KRow(i) => {
                    if let Some(w) = app.keys.as_mut() {
                        w.selected = Some(i as usize);
                        invalidate(hwnd);
                    }
                }
                Id::KEye => {
                    if let Some(w) = app.keys.as_mut() {
                        w.show_key = !w.show_key;
                        w.new_key.masked = !w.show_key;
                        invalidate(hwnd);
                    }
                }
                Id::KAdd => {
                    add_key(app);
                }
                Id::KActivate => {
                    let key = selected_key(app);
                    match key {
                        Some(k) => {
                            app.set_key(&k);
                            if let Some(w) = app.keys.as_mut() {
                                w.status = "کلید فعال تغییر کرد ✓".to_string();
                            }
                            refresh_usage(app);
                        }
                        None => set_keys_status(app, "ابتدا یک کلید را از فهرست انتخاب کنید."),
                    }
                }
                Id::KDel => {
                    let key = selected_key(app);
                    match key {
                        Some(k) => {
                            app.cfg.api_keys.retain(|x| x != &k);
                            if app.cfg.api_key == k {
                                app.cfg.api_key = app.cfg.api_keys.first().cloned().unwrap_or_default();
                            }
                            app.cfg.save();
                            if let Some(w) = app.keys.as_mut() {
                                w.selected = None;
                                w.status = "کلید حذف شد.".to_string();
                            }
                            refresh_usage(app);
                        }
                        None => set_keys_status(app, "ابتدا یک کلید را از فهرست انتخاب کنید."),
                    }
                }
                Id::KScope => {
                    if let Some(w) = app.keys.as_mut() {
                        w.only_active = !w.only_active;
                    }
                    refresh_usage(app);
                }
                Id::KRefresh => refresh_usage(app),
                _ => {
                    if let Some(w) = app.keys.as_mut() {
                        with_canvas(|c| w.new_key.click(c, x));
                        w.focus = Some(Id::KNew);
                        sync_caret(hwnd, Some(&w.new_key));
                        invalidate(hwnd);
                    }
                }
            }
            0
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            if let Some(w) = app.keys.as_mut() {
                w.drag = None;
            }
            0
        }
        WM_KEYDOWN => {
            let vk = wparam as i32;
            if vk == VK_ESCAPE as i32 {
                DestroyWindow(hwnd);
                return 0;
            }
            if vk == VK_RETURN as i32 {
                add_key(app);
                return 0;
            }
            if let Some(w) = app.keys.as_mut() {
                w.new_key.on_key(vk);
                sync_caret(hwnd, Some(&w.new_key));
                invalidate(hwnd);
            }
            0
        }
        WM_CHAR => {
            if let Some(ch) = char::from_u32(wparam as u32) {
                if ch >= ' ' {
                    if let Some(w) = app.keys.as_mut() {
                        w.new_key.on_char(ch);
                        sync_caret(hwnd, Some(&w.new_key));
                        invalidate(hwnd);
                    }
                }
            }
            0
        }
        WM_SETFOCUS => {
            if let Some(w) = app.keys.as_ref() {
                sync_caret(hwnd, Some(&w.new_key));
            }
            0
        }
        WM_KILLFOCUS => {
            unsafe {
                HideCaret(hwnd);
            }
            0
        }
        WM_SETCURSOR => {
            set_hand_cursor(app.keys.as_ref().and_then(|w| w.hover).is_some());
            1
        }
        WM_DESTROY => {
            if let Some(w) = app.keys.as_mut() {
                w.closed = true;
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn selected_key(app: &App) -> Option<String> {
    let idx = app.keys.as_ref().and_then(|w| w.selected)?;
    app.cfg.api_keys.get(idx).cloned()
}

fn set_keys_status(app: &mut App, text: &str) {
    if let Some(w) = app.keys.as_mut() {
        w.status = text.to_string();
        invalidate(w.hwnd);
    }
}

fn add_key(app: &mut App) {
    let key = app.keys.as_ref().map(|w| w.new_key.text.trim().to_string()).unwrap_or_default();
    if key.is_empty() {
        set_keys_status(app, "کلید جدید را وارد کنید.");
        return;
    }
    if !app.cfg.api_keys.iter().any(|k| k == &key) {
        app.cfg.api_keys.push(key.clone());
    }
    app.cfg.api_key = key;
    app.cfg.save();
    if let Some(w) = app.keys.as_mut() {
        w.new_key.text.clear();
        w.new_key.caret = 0;
        w.status = "کلید افزوده و فعال شد ✓".to_string();
    }
    refresh_usage(app);
}

fn paint_keys(app: &mut App, hdc: windows_sys::Win32::Graphics::Gdi::HDC, w: f32, h: f32) {
    let mem = unsafe { CreateCompatibleDC(hdc) };
    let bmp = unsafe { CreateCompatibleBitmap(hdc, w as i32, h as i32) };
    let old = unsafe { SelectObject(mem, bmp) };
    let t = app.theme;
    let keys = app.cfg.api_keys.clone();
    let active = app.cfg.api_key.clone();
    if let Some(c) = GpCanvas::from_hdc(mem) {
        if let Some(win) = app.keys.as_mut() {
            draw_keys(win, &c, w, h, &t, &keys, &active);
        }
    }
    unsafe {
        BitBlt(hdc, 0, 0, w as i32, h as i32, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
    }
}

fn draw_keys(win: &mut KeysWin, c: &GpCanvas, w: f32, h: f32, t: &Theme, keys: &[String], active: &str) {
    let hover = win.hover;
    win.hits.clear();
    c.fill_rect(0.0, 0.0, w, h, t.border);
    c.fill_rect(1.0, 1.0, w - 2.0, h - 2.0, t.bg);
    c.fill_rect(1.0, 1.0, w - 2.0, 32.0, t.bar_bg);
    c.fill_rect(1.0, 33.0, w - 2.0, 1.0, t.divider);
    win.hits.add(Id::KBar, rect(1.0, 1.0, w - 2.0, 32.0));
    c.draw_icon("key", 18.0, 17.0, 13.0, t.accent);
    let title = "کلیدها و مصرف";
    let tw = c.measure_line_width(title, F_TITLE, true);
    c.draw_text(title, rect(30.0, 1.0, tw + 2.0, 32.0), F_TITLE, true, t.fg, true, TextAlign::Near, true, false);
    let close_r = rect(w - 30.0, 4.0, 26.0, 26.0);
    ui::button(
        c,
        &mut win.hits,
        &ui::Btn::new(Id::KClose, close_r, if hover == Some(Id::KClose) { 0xFFFFFFFF } else { t.fg_muted })
            .icon("close", 11.0)
            .bg(if hover == Some(Id::KClose) { t.close_hover } else { t.bar_bg }),
    );

    // ---- card 1: key management
    let card1 = rect(12.0, 44.0, w - 24.0, 178.0);
    c.fill_round_rect(card1.x, card1.y, card1.width, card1.height, 6.0, t.card_bg);
    c.draw_round_rect(card1.x, card1.y, card1.width, card1.height, 6.0, t.card_border, 1.0);
    c.draw_text("مدیریت کلیدهای API", rect(card1.x + 10.0, card1.y + 6.0, 220.0, 20.0), F_LABEL, true, t.fg, true, TextAlign::Near, true, false);
    let status = win.status.clone();
    if !status.is_empty() {
        let color = if status.contains('✓') { t.accent_green } else { t.accent_amber };
        let sw = c.measure_line_width(&status, F_SMALL, false);
        c.draw_text(&status, rect(card1.x + card1.width - 12.0 - sw, card1.y + 6.0, sw + 2.0, 20.0), F_SMALL, false, color, true, TextAlign::Near, true, false);
    }

    let list = rect(card1.x + 10.0, card1.y + 30.0, card1.width - 20.0, 84.0);
    c.fill_rect(list.x, list.y, list.width, list.height, t.bg);
    c.draw_rect(list.x, list.y, list.width, list.height, t.border, 1.0);
    c.set_clip(list.x, list.y, list.width, list.height);
    let max_list = 4usize;
    for i in 0..max_list {
        let idx = win.list_top + i;
        let Some(key) = keys.get(idx) else { break };
        let row = rect(list.x + 1.0, list.y + 1.0 + i as f32 * 20.0, list.width - 2.0, 20.0);
        if win.selected == Some(idx) {
            c.fill_rect(row.x, row.y, row.width, row.height, t.select_bg);
        } else if hover == Some(Id::KRow(idx as u32)) {
            c.fill_rect(row.x, row.y, row.width, row.height, t.hover_bg);
        }
        let is_active = key == active;
        let label = format!("  {}{}", config::key_label(key), if is_active { "   ✓ فعال" } else { "" });
        c.draw_text(&label, rect(row.x + 4.0, row.y, row.width - 8.0, 20.0), F_ROW, false, if is_active { t.accent } else { t.fg }, true, TextAlign::Near, true, false);
        win.hits.add(Id::KRow(idx as u32), row);
    }
    c.reset_clip();
    if keys.len() > max_list {
        let info = format!("{} کلید — با چرخ ماوس پیمایش کنید", keys.len());
        c.draw_text(&info, rect(list.x + list.width - 246.0, list.y + list.height - 18.0, 240.0, 16.0), 10.0, false, t.fg_dim, true, TextAlign::Near, true, false);
    }

    let hint = "کلید فعال با ✓ نشان داده می‌شود؛ برای تغییر، کلید را انتخاب و «فعال‌سازی» را بزنید.";
    c.draw_text(hint, rect(card1.x + 12.0, card1.y + 120.0, card1.width - 24.0, 16.0), 10.5, false, t.fg_dim, true, TextAlign::Near, true, false);

    // add row
    let row_y = card1.y + 146.0;
    win.new_key.rect = rect(card1.x + 10.0, row_y, 356.0, 26.0);
    win.new_key.draw(c, t);
    win.hits.add(Id::KNew, win.new_key.rect);
    let eye_r = rect(card1.x + 370.0, row_y, 26.0, 26.0);
    ui::button(
        c,
        &mut win.hits,
        &ui::Btn::new(Id::KEye, eye_r, if win.show_key { t.accent } else { t.fg_muted })
            .icon(if win.show_key { "eye_off" } else { "eye" }, 13.0)
            .bg(if hover == Some(Id::KEye) { t.hover_bg } else { t.card_bg }),
    );
    ui::button(
        c,
        &mut win.hits,
        &ui::Btn::new(Id::KAdd, rect(card1.x + 404.0, row_y, 78.0, 26.0), t.accent_fg)
            .icon("plus", 11.0)
            .text("افزودن", F_SMALL)
            .bg(if hover == Some(Id::KAdd) { t.accent_hover } else { t.accent }),
    );
    ui::button(
        c,
        &mut win.hits,
        &ui::Btn::new(Id::KActivate, rect(card1.x + 488.0, row_y, 88.0, 26.0), t.fg)
            .icon("check", 11.0)
            .text("فعال‌سازی", F_SMALL)
            .bg(if hover == Some(Id::KActivate) { t.hover_bg } else { t.btn_bg }),
    );
    ui::button(
        c,
        &mut win.hits,
        &ui::Btn::new(Id::KDel, rect(card1.x + 582.0, row_y, 88.0, 26.0), t.accent_red)
            .icon("trash", 11.0)
            .text("حذف", F_SMALL)
            .bg(if hover == Some(Id::KDel) { t.hover_bg } else { t.btn_bg }),
    );

    // ---- card 2: usage
    let card2 = rect(12.0, 234.0, w - 24.0, h - 234.0 - 12.0);
    c.fill_round_rect(card2.x, card2.y, card2.width, card2.height, 6.0, t.card_bg);
    c.draw_round_rect(card2.x, card2.y, card2.width, card2.height, 6.0, t.card_border, 1.0);
    c.draw_text("مصرف (۶۰ ثانیه / ۲۴ ساعت)", rect(card2.x + 10.0, card2.y + 6.0, 280.0, 20.0), F_LABEL, true, t.fg, true, TextAlign::Near, true, false);

    let sw_x = card2.x + card2.width - 52.0;
    c.draw_toggle_switch(sw_x, card2.y + 8.0, win.only_active, t);
    win.hits.add(Id::KScope, rect(sw_x - 4.0, card2.y + 4.0, 44.0, 24.0));
    let scope_label = "فقط کلید فعال";
    let slw = c.measure_line_width(scope_label, F_SMALL, false);
    c.draw_text(scope_label, rect(sw_x - 10.0 - slw, card2.y + 6.0, slw + 2.0, 20.0), F_SMALL, false, t.fg, true, TextAlign::Near, true, false);
    ui::button(
        c,
        &mut win.hits,
        &ui::Btn::new(Id::KRefresh, rect(sw_x - 88.0 - slw, card2.y + 6.0, 76.0, 24.0), t.fg_muted)
            .icon("refresh", 11.0)
            .text("به‌روزرسانی", 10.5)
            .bg(if hover == Some(Id::KRefresh) { t.hover_bg } else { t.card_bg }),
    );

    // table
    let headers = ["کلید", "مدل", "درخواست/دقیقه", "توکن/دقیقه", "درخواست/روز"];
    let widths = [200.0, 190.0, 92.0, 92.0, 92.0];
    let head_y = card2.y + 34.0;
    let mut hx = card2.x + 12.0;
    for (i, head) in headers.iter().enumerate() {
        let hw = c.measure_line_width(head, F_SMALL, false);
        c.draw_text(head, rect(hx + widths[i] - hw - 4.0, head_y, hw + 6.0, 18.0), F_SMALL, false, t.fg_muted, true, TextAlign::Near, true, false);
        hx += widths[i];
    }
    c.fill_rect(card2.x + 10.0, head_y + 20.0, card2.width - 20.0, 1.0, t.divider);

    let max_rows = (((card2.height - 90.0) / 24.0).floor() as usize).max(1);
    let mut ry = head_y + 26.0;
    for row in win.rows.iter().take(max_rows) {
        let (lr, lt, ld) = row.limits;
        let full = (ld.is_some() && row.rpd as u32 >= ld.unwrap())
            || (lr.is_some() && row.rpm as u32 >= lr.unwrap())
            || (lt.is_some() && row.tpm >= lt.unwrap() as u64);
        let warn = !full
            && ((ld.is_some() && row.rpd as f32 >= ld.unwrap() as f32 * 0.8)
                || (lr.is_some() && row.rpm as f32 >= lr.unwrap() as f32 * 0.8));
        let used = row.rpd > 0 || row.rpm > 0 || row.tpm > 0;
        let color = if full {
            t.accent_red
        } else if warn {
            t.accent_amber
        } else if used {
            t.fg
        } else {
            t.fg_dim
        };
        let cells = [
            config::key_label(&row.key),
            config::find_model(row.model_id).label.to_string(),
            format!("{} / {}", row.rpm, lr.map(|v| v.to_string()).unwrap_or_else(|| "—".into())),
            format!("{} / {}", thousands(row.tpm), lt.map(|v| thousands(v as u64)).unwrap_or_else(|| "—".into())),
            format!("{} / {}", row.rpd, ld.map(|v| v.to_string()).unwrap_or_else(|| "—".into())),
        ];
        let mut cx = card2.x + 12.0;
        for (i, cell) in cells.iter().enumerate() {
            let cw = c.measure_line_width(cell, F_ROW, false);
            let cell_color = if i == 0 { t.key_fg } else { color };
            c.draw_text(cell, rect(cx + widths[i] - cw - 4.0, ry, cw + 6.0, 22.0), F_ROW, false, cell_color, true, TextAlign::Near, true, false);
            cx += widths[i];
        }
        ry += 24.0;
    }
    if win.rows.is_empty() {
        c.draw_text("مصرفی ثبت نشده است.", rect(card2.x + 12.0, ry, 300.0, 22.0), F_ROW, false, t.fg_dim, true, TextAlign::Near, true, false);
    }
    let note = "شمارش محلی و فقط مربوط به همین برنامه است؛ گوگل شمارنده‌ی زنده‌ی سهمیه را برنمی‌گرداند.";
    c.draw_text(note, rect(card2.x + 12.0, card2.y + card2.height - 26.0, card2.width - 24.0, 18.0), 10.5, false, t.fg_dim, true, TextAlign::Near, true, false);
}

fn thousands(v: u64) -> String {
    let s = v.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

// ------------------------------------------------------------------ shared helpers
fn mouse_xy(lparam: LPARAM) -> (f32, f32) {
    (((lparam & 0xFFFF) as i16) as f32, (((lparam >> 16) & 0xFFFF) as i16) as f32)
}

fn drag_start(hwnd: HWND) -> (i32, i32) {
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let mut wr: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut wr);
        (pt.x - wr.left, pt.y - wr.top)
    }
}

fn drag_move(hwnd: HWND, gx: i32, gy: i32) {
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetWindowPos(hwnd, std::ptr::null_mut(), pt.x - gx, pt.y - gy, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
    }
}

fn track_leave(hwnd: HWND) {
    unsafe {
        let mut tme = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        TrackMouseEvent(&mut tme);
    }
}

fn set_hand_cursor(hot: bool) {
    unsafe {
        let cursor = LoadCursorW(std::ptr::null_mut(), if hot { IDC_HAND } else { IDC_ARROW });
        SetCursor(cursor);
    }
}

/// Screen position right under a hit region, for anchoring a popup menu.
fn menu_anchor(app: &App, id: Id) -> (i32, i32) {
    if let Some(w) = app.settings.as_ref() {
        if let Some((_, r)) = w.hits.items.iter().find(|(i, _)| *i == id) {
            let mut pt = POINT { x: r.x as i32, y: (r.y + r.height) as i32 };
            unsafe {
                ClientToScreen(w.hwnd, &mut pt);
            }
            return (pt.x, pt.y);
        }
    }
    (0, 0)
}
