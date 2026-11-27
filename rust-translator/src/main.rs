#![windows_subsystem = "windows"]
//! مترجم شناور — floating selection translator (Rust/Win32 port of app.py).

mod config;
mod dialogs;
mod gdiplus;
mod http;
mod json;
mod menu;
mod theme;
mod ui;
mod win32;

use std::cell::UnsafeCell;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC,
    DeleteObject, EndPaint, GetDC, InvalidateRect, ReleaseDC, SelectObject, UpdateWindow, SRCCOPY,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_C,
    VK_CONTROL, VK_ESCAPE, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos,
    GetMessageW, GetWindowRect, KillTimer, LoadCursorW, PostQuitMessage, RegisterClassW, SetCursor,
    SetTimer, SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, HWND_TOPMOST,
    IDC_ARROW, IDC_HAND, IDC_IBEAM, IDC_SIZENWSE, MSG, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SW_SHOW, WM_ACTIVATE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_RBUTTONDOWN, WM_SETCURSOR, WM_TIMER, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use config::{AppConfig, ModelKind, MAX_CHARS};
use gdiplus::{GdiplusGuard, GpCanvas, RectF, SpinnerState, TextAlign};
use menu::{MenuAction, MenuItem};
use theme::Theme;
use ui::{is_rtl_text, rect, wrap_lines, Field, Hits, Id};
use win32::{
    apply_window_corners, apply_window_opacity, clipboard_seq, clipboard_text, set_clipboard,
    to_wide, MousePoller,
};

const TIMER_UI: usize = 1;
const TIMER_MOUSE: usize = 2;
const UI_MS: u32 = 40;
const MOUSE_MS: u32 = 15;
const COPY_SETTLE_MS: u64 = 180;

// font sizes (px) and metrics
const F_TITLE: f32 = 12.0;
const F_CHIP: f32 = 11.5;
const F_SRC: f32 = 12.0;
const F_OUT: f32 = 16.0;
const F_STATUS: f32 = 11.0;
const SRC_LH: f32 = 17.0;
const OUT_LH: f32 = 22.0;

pub struct App {
    pub hwnd: HWND,
    pub cfg: AppConfig,
    pub theme: Theme,
    pub hits: Hits,
    pub hover: Option<Id>,
    pub status: String,
    pub src_text: String,
    pub src_lines: Vec<String>,
    pub out_text: String,
    pub out_lines: Vec<String>,
    pub out_source: Vec<String>,
    pub out_err: bool,
    pub last_out: Option<String>,
    pub last_ok: bool,
    pub last_text: Option<String>,
    pub seq: u64,
    pub spinner: SpinnerState,
    pub spinner_angle: f32,
    pub spin: [f32; 3],
    pub copy_flash: bool,
    pub scroll: f32,
    pub sel_anchor: Option<u32>,
    pub sel_start: Option<u32>,
    pub sel_end: Option<u32>,
    pub selecting: bool,
    pub drag: Option<(i32, i32)>,
    pub resize: Option<((i32, i32), (i32, i32))>,
    pub watcher: MousePoller,
    pub clip_seq: u32,
    pub pending: Vec<(Instant, Pending)>,
    pub menu: Option<Box<menu::Menu>>,
    pub settings: Option<Box<dialogs::SettingsWin>>,
    pub keys: Option<Box<dialogs::KeysWin>>,
    pub out_rect: RectF,
    pub dirty: bool,
}

pub enum Pending {
    CtrlCAgain,
    ReadClipboard(bool),
    CopyIconRestore,
    OpenSettings,
    KeepTopmost,
}

struct AppCell(UnsafeCell<Option<App>>);
unsafe impl Sync for AppCell {}
static APP: AppCell = AppCell(UnsafeCell::new(None));

pub fn app() -> &'static mut App {
    unsafe { (*APP.0.get()).as_mut().expect("app not ready") }
}

fn app_ready() -> bool {
    unsafe { (*APP.0.get()).is_some() }
}

// ------------------------------------------------------------------ async results
static RESULT: Mutex<Option<(u64, Result<(String, String), String>)>> = Mutex::new(None);

fn spawn_job(seq: u64, text: String, cfg: AppConfig) {
    std::thread::spawn(move || {
        let res = http::translate(&text, &cfg);
        if let Ok(mut slot) = RESULT.lock() {
            *slot = Some((seq, res));
        }
    });
}

// ------------------------------------------------------------------ helpers
fn mouse_xy(lparam: LPARAM) -> (f32, f32) {
    (((lparam & 0xFFFF) as i16) as f32, (((lparam >> 16) & 0xFFFF) as i16) as f32)
}

pub fn with_canvas<R>(f: impl FnOnce(&GpCanvas) -> R) -> R {
    let hdc = unsafe { GetDC(std::ptr::null_mut()) };
    let c = GpCanvas::from_hdc(hdc).expect("gdi+ canvas");
    let out = f(&c);
    drop(c);
    unsafe { ReleaseDC(std::ptr::null_mut(), hdc) };
    out
}

fn invalidate(hwnd: HWND) {
    unsafe {
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }
}

/// Text wrapped to the given width; measured with a throwaway screen canvas.
fn wrap_cached(text: &str, width: f32, font: f32, bold: bool) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    with_canvas(|c| wrap_lines(c, text, width, font, bold))
}

fn label_at(c: &GpCanvas, text: &str, x: f32, y: f32, h: f32, size: f32, bold: bool, color: u32, rtl: bool) -> f32 {
    let tw = c.measure_line_width(text, size, bold);
    c.draw_text(text, rect(x, y, tw.max(1.0), h), size, bold, color, rtl, TextAlign::Near, true, false);
    tw
}

// ------------------------------------------------------------------ app state
impl App {
    fn new(hwnd: HWND, cfg: AppConfig) -> Self {
        let theme = Theme::for_name(&cfg.theme);
        let watcher = MousePoller::new(cfg.watch);
        let mut app = Self {
            hwnd,
            cfg,
            theme,
            hits: Hits::default(),
            hover: None,
            status: "آماده".to_string(),
            src_text: "متنی را انتخاب کنید...".to_string(),
            src_lines: Vec::new(),
            out_text: String::new(),
            out_lines: Vec::new(),
            out_source: Vec::new(),
            out_err: false,
            last_out: None,
            last_ok: false,
            last_text: None,
            seq: 0,
            spinner: SpinnerState::Idle,
            spinner_angle: 0.0,
            spin: [0.0; 3],
            copy_flash: false,
            scroll: 0.0,
            sel_anchor: None,
            sel_start: None,
            sel_end: None,
            selecting: false,
            drag: None,
            resize: None,
            watcher,
            clip_seq: 0,
            pending: Vec::new(),
            menu: None,
            settings: None,
            keys: None,
            out_rect: rect(0.0, 0.0, 0.0, 0.0),
            dirty: true,
        };
        app.src_lines = wrap_cached(&app.src_text, 380.0, F_SRC, false);
        if app.cfg.api_key.trim().is_empty() {
            app.pending.push((Instant::now() + Duration::from_millis(300), Pending::OpenSettings));
        }
        app.pending.push((Instant::now() + Duration::from_millis(4000), Pending::KeepTopmost));
        app
    }

    fn rtl(&self) -> bool {
        self.cfg.effective_rtl()
    }

    fn set_theme(&mut self, name: &str) {
        self.cfg.theme = name.to_string();
        self.theme = Theme::for_name(name);
        self.cfg.save();
        self.dirty = true;
    }

    fn set_status(&mut self, s: &str) {
        self.status = s.to_string();
        self.dirty = true;
    }

    fn set_model(&mut self, id: &str) {
        if !config::MODELS.iter().any(|m| m.id == id) {
            return;
        }
        self.cfg.model = id.to_string();
        self.cfg.save();
        let label = config::find_model(id).label;
        if self.last_text.is_some() && !self.last_ok {
            let text = self.last_text.clone().unwrap();
            self.translate_text(&text);
        } else {
            self.last_text = None;
            self.set_status(&format!("مدل: {}", label));
        }
        self.dirty = true;
    }

    fn set_key(&mut self, key: &str) {
        if !self.cfg.api_keys.iter().any(|k| k == key) {
            return;
        }
        self.cfg.api_key = key.to_string();
        self.cfg.save();
        if self.last_text.is_some() && !self.last_ok {
            let text = self.last_text.clone().unwrap();
            self.translate_text(&text);
        } else {
            self.last_text = None;
            self.set_status(&format!("کلید فعال: {}", config::key_label(key)));
        }
        self.dirty = true;
    }

    fn toggle_lock(&mut self) {
        self.cfg.locked = !self.cfg.locked;
        self.cfg.save();
        let locked = self.cfg.locked;
        self.set_status(if locked { "جابه‌جایی قفل است" } else { "جابه‌جایی آزاد است" });
    }

    fn toggle_rtl(&mut self) {
        let next = !self.rtl();
        self.cfg.rtl = Some(next);
        self.cfg.save();
        if let Some(out) = self.last_out.clone() {
            self.set_out(&out, self.out_err);
        }
        self.set_status(if next { "جهت متن: راست‌به‌چپ" } else { "جهت متن: چپ‌به‌راست" });
    }

    fn set_watch(&mut self, val: bool) {
        self.cfg.watch = val;
        self.watcher.enabled = val;
        self.cfg.save();
        self.set_status(if val { "ترجمه‌ی خودکار: روشن" } else { "ترجمه‌ی خودکار: خاموش" });
    }

    fn set_autocopy(&mut self, val: bool) {
        self.cfg.autocopy = val;
        self.cfg.save();
        self.set_status(if val { "کپی خودکار: روشن" } else { "کپی خودکار: خاموش" });
    }

    fn set_opacity(&mut self, val: f32) {
        self.cfg.opacity = val.clamp(0.3, 1.0);
        self.cfg.save();
        apply_window_opacity(self.hwnd, self.cfg.opacity);
        self.dirty = true;
    }

    fn set_out(&mut self, text: &str, err: bool) {
        self.out_text = text.to_string();
        self.out_err = err;
        self.last_out = if text.is_empty() { None } else { Some(text.to_string()) };
        self.scroll = 0.0;
        self.sel_start = None;
        self.sel_end = None;
        self.rewrap_out();
        self.dirty = true;
    }

    fn rewrap_out(&mut self) {
        let width = if self.out_rect.width > 20.0 { self.out_rect.width - 16.0 } else { 388.0 };
        let rtl = self.rtl();
        let lines = wrap_cached(&self.out_text, width, F_OUT, false);
        // logical lines, used when copying the selection
        let source = wrap_cached(&self.out_text, width, F_OUT, false);
        let _ = rtl;
        self.out_lines = lines;
        self.out_source = source;
    }

    fn translate_text(&mut self, raw: &str) {
        let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        let text: String = collapsed.chars().take(MAX_CHARS).collect();
        if text.is_empty() || (self.last_text.as_deref() == Some(text.as_str()) && self.last_ok) {
            return;
        }
        config::log_debug(&format!("clipboard: {} chars", text.chars().count()));
        self.last_text = Some(text.clone());
        self.last_ok = false;
        self.seq += 1;
        self.src_text = text.clone();
        self.src_lines = wrap_cached(&text, 380.0, F_SRC, false);
        self.set_status("در حال ترجمه...");
        self.spinner = SpinnerState::Loading;
        self.set_out("", false);
        spawn_job(self.seq, text, self.cfg.clone());
        self.dirty = true;
    }

    fn from_clipboard(&mut self) {
        let text = clipboard_text();
        if text.trim().is_empty() {
            self.set_status("کلیپ‌بورد خالی است");
            return;
        }
        self.last_text = None;
        self.translate_text(&text);
    }

    fn copy_translation(&mut self) {
        let Some(out) = self.last_out.clone() else {
            self.set_status("ترجمه‌ای برای کپی نیست");
            return;
        };
        if !self.last_ok {
            self.set_status("ترجمه‌ای برای کپی نیست");
            return;
        }
        if set_clipboard(&out) {
            self.set_status("ترجمه کپی شد ✓");
            self.copy_flash = true;
            self.pending.push((Instant::now() + Duration::from_millis(1500), Pending::CopyIconRestore));
        } else {
            self.set_status("کپی ناموفق بود — کلیپ‌بورد قفل است؟");
        }
        self.dirty = true;
    }

    /// Ctrl+C: logical text of the selected lines, or the whole translation.
    fn copy_selection_or_all(&mut self) {
        if let (Some(a), Some(b)) = (self.sel_start, self.sel_end) {
            let (lo, hi) = (a.min(b) as usize, a.max(b) as usize);
            let joined = self
                .out_source
                .iter()
                .skip(lo)
                .take(hi - lo + 1)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            let joined = joined.trim().to_string();
            if !joined.is_empty() {
                let ok = set_clipboard(&joined);
                self.set_status(if ok { "کپی شد ✓" } else { "کپی ناموفق بود" });
                return;
            }
        }
        self.copy_translation();
    }

    fn grab_selection(&mut self) {
        config::log_debug("selection gesture -> Ctrl+C");
        self.clip_seq = clipboard_seq();
        win32::send_ctrl_c();
        let now = Instant::now();
        self.pending.push((now + Duration::from_millis(120), Pending::CtrlCAgain));
        self.pending
            .push((now + Duration::from_millis(COPY_SETTLE_MS + 120), Pending::ReadClipboard(true)));
    }

    fn read_clipboard(&mut self, retry: bool) {
        if clipboard_seq() == self.clip_seq {
            if retry {
                self.pending.push((
                    Instant::now() + Duration::from_millis(200),
                    Pending::ReadClipboard(false),
                ));
            } else {
                config::log_debug("clipboard sequence unchanged -> copy never happened");
                self.set_status("متن کپی نشد — دوباره انتخاب کنید");
            }
            return;
        }
        let text = clipboard_text();
        self.translate_text(&text);
    }

    fn apply_result(&mut self, seq: u64, res: Result<(String, String), String>) {
        if seq != self.seq {
            return;
        }
        match res {
            Ok((_src, out)) => {
                self.last_ok = true;
                self.set_out(&out, false);
                self.spinner = SpinnerState::Ok;
                if self.cfg.autocopy && set_clipboard(&out) {
                    self.set_status("ترجمه شد و کپی شد ✓");
                } else {
                    self.set_status("ترجمه شد");
                }
            }
            Err(err) => {
                config::log_debug(&format!("translate error: {}", err));
                self.last_ok = false;
                self.set_out(&err, true);
                self.spinner = SpinnerState::Err;
                self.set_status("ناموفق");
            }
        }
        self.dirty = true;
    }
}

// ------------------------------------------------------------------ menu actions
pub fn apply_menu_action(app: &mut App, action: MenuAction) {
    match action {
        MenuAction::SetModel(m) => app.set_model(&m),
        MenuAction::SetKey(k) => app.set_key(&k),
        MenuAction::OpenKeys => dialogs::open_keys(app),
        MenuAction::OpenSettings => dialogs::open_settings(app),
        MenuAction::FromClipboard => app.from_clipboard(),
        MenuAction::CopyTranslation => app.copy_translation(),
        MenuAction::ToggleWatch(v) => app.set_watch(v),
        MenuAction::ToggleAutocopy(v) => app.set_autocopy(v),
        MenuAction::ToggleLock => app.toggle_lock(),
        MenuAction::ToggleRtl => app.toggle_rtl(),
        MenuAction::ToggleTheme(light) => app.set_theme(if light { "light" } else { "dark" }),
        MenuAction::SetOpacity(v) => app.set_opacity(v),
        MenuAction::Exit => unsafe {
            DestroyWindow(app.hwnd);
        },
        MenuAction::TestConnection => {}
        MenuAction::PickSettingsModel(m) => dialogs::pick_settings_model(app, &m),
    }
}

fn model_menu_items(app: &App) -> Vec<MenuItem> {
    config::MODELS
        .iter()
        .map(|m| MenuItem {
            label: m.label.to_string(),
            kind: menu::ItemKind::Check,
            checked: m.id == app.cfg.model,
            action: Some(MenuAction::SetModel(m.id.to_string())),
            accent: false,
        })
        .collect()
}

fn key_menu_items(app: &App) -> Vec<MenuItem> {
    let mut items: Vec<MenuItem> = app
        .cfg
        .api_keys
        .iter()
        .map(|k| MenuItem {
            label: config::key_label(k),
            kind: menu::ItemKind::Check,
            checked: k == &app.cfg.api_key,
            action: Some(MenuAction::SetKey(k.clone())),
            accent: false,
        })
        .collect();
    if !items.is_empty() {
        items.push(MenuItem::separator());
    }
    items.push(MenuItem::action("کلیدها و مصرف...", MenuAction::OpenKeys));
    items
}

fn context_menu_items(app: &App) -> Vec<MenuItem> {
    let mut items = vec![
        MenuItem::action("تنظیمات...", MenuAction::OpenSettings),
        MenuItem::action("ترجمه‌ی کلیپ‌بورد", MenuAction::FromClipboard),
        MenuItem::action("کپی ترجمه", MenuAction::CopyTranslation),
        MenuItem::separator(),
        MenuItem::check("ترجمه‌ی خودکار انتخاب", app.cfg.watch, MenuAction::ToggleWatch(!app.cfg.watch)),
        MenuItem::check("کپی خودکار پس از هر ترجمه", app.cfg.autocopy, MenuAction::ToggleAutocopy(!app.cfg.autocopy)),
        MenuItem::check("قفل پنجره", app.cfg.locked, MenuAction::ToggleLock),
        MenuItem::check("راست‌به‌چپ (RTL)", app.rtl(), MenuAction::ToggleRtl),
        MenuItem::check("حالت روشن", app.cfg.theme != "dark", MenuAction::ToggleTheme(app.cfg.theme == "dark")),
        MenuItem::separator(),
    ];
    for (label, val) in [("شفافیت ۷۰٪", 0.7f32), ("شفافیت ۸۵٪", 0.85), ("شفافیت ۱۰۰٪", 1.0)] {
        items.push(MenuItem {
            label: label.to_string(),
            kind: menu::ItemKind::Check,
            checked: (app.cfg.opacity - val).abs() < 0.03,
            action: Some(MenuAction::SetOpacity(val)),
            accent: false,
        });
    }
    items.push(MenuItem::separator());
    items.push(MenuItem::action("کلیدها و مصرف...", MenuAction::OpenKeys));
    items.push(MenuItem {
        label: "خروج".to_string(),
        kind: menu::ItemKind::Action,
        checked: false,
        action: Some(MenuAction::Exit),
        accent: true,
    });
    items
}

fn open_menu_at(app: &mut App, id: Id, screen_offset: (i32, i32)) {
    let (items, anchor) = match id {
        Id::Model => (model_menu_items(app), app.hits.items.iter().find(|(i, _)| *i == Id::Model).map(|(_, r)| *r)),
        Id::Key => (key_menu_items(app), app.hits.items.iter().find(|(i, _)| *i == Id::Key).map(|(_, r)| *r)),
        _ => return,
    };
    let Some(r) = anchor else { return };
    let mut pt = POINT { x: r.x as i32, y: (r.y + r.height) as i32 };
    unsafe {
        ClientToScreen(app.hwnd, &mut pt);
    }
    let _ = screen_offset;
    menu::open(app, items, pt.x, pt.y);
}

// ------------------------------------------------------------------ painting
fn paint_main(app: &mut App, hdc: windows_sys::Win32::Graphics::Gdi::HDC, w: f32, h: f32) {
    let mem = unsafe { CreateCompatibleDC(hdc) };
    let bmp = unsafe { CreateCompatibleBitmap(hdc, w as i32, h as i32) };
    let old = unsafe { SelectObject(mem, bmp) };
    if let Some(c) = GpCanvas::from_hdc(mem) {
        draw_main(app, &c, w, h);
    }
    unsafe {
        BitBlt(hdc, 0, 0, w as i32, h as i32, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
    }
}

fn draw_main(app: &mut App, c: &GpCanvas, w: f32, h: f32) {
    let t = app.theme;
    let border = 1.0;
    let bar_h = 30.0;
    let foot_h = 22.0;
    let pad = 8.0;
    app.hits.clear();

    c.fill_rect(0.0, 0.0, w, h, t.border);
    c.fill_rect(border, border, w - 2.0, h - 2.0, t.bg);
    c.fill_rect(border, border, w - 2.0, bar_h, t.bar_bg);
    c.fill_rect(border, border + bar_h, w - 2.0, 1.0, t.divider);
    app.hits.add(Id::Bar, rect(border, border, w - 2.0, bar_h));

    // ---- title bar: title + chips on the left
    let mut x = border + 4.0;
    x += label_at(c, "مترجم شناور", x, border, bar_h, F_TITLE, true, t.fg_muted, true) + 6.0;

    // lock
    let lock_r = rect(x, border + 3.0, 22.0, 24.0);
    ui::button(
        c,
        &mut app.hits,
        &ui::Btn::new(Id::Lock, lock_r, if app.cfg.locked { t.accent_amber } else { t.fg_muted })
            .icon(if app.cfg.locked { "lock" } else { "unlock" }, 12.0),
    );
    if app.hover == Some(Id::Lock) {
        c.fill_round_rect(lock_r.x, lock_r.y, lock_r.width, lock_r.height, 5.0, t.hover_bg);
        c.draw_icon(
            if app.cfg.locked { "lock" } else { "unlock" },
            lock_r.x + 11.0,
            lock_r.y + 12.0,
            12.0,
            t.fg,
        );
    }
    x += 22.0 + 2.0;

    // model chip
    let model_label = format!("{} ▾", config::find_model(&app.cfg.model).short);
    let mw = c.measure_line_width(&model_label, F_CHIP, false) + 10.0;
    let model_r = rect(x, border + 3.0, mw, 24.0);
    ui::chip(
        c,
        &mut app.hits,
        Id::Model,
        model_r,
        &model_label,
        t.accent,
        if app.hover == Some(Id::Model) { Some(t.hover_bg) } else { None },
        F_CHIP,
        false,
    );
    x += mw + 4.0;

    // active key chip
    let key_label = format!("{} ▾", config::key_label(&app.cfg.api_key));
    let kw = c.measure_line_width(&key_label, F_CHIP, false) + 10.0;
    let key_r = rect(x, border + 3.0, kw, 24.0);
    ui::chip(
        c,
        &mut app.hits,
        Id::Key,
        key_r,
        &key_label,
        t.key_fg,
        if app.hover == Some(Id::Key) { Some(t.hover_bg) } else { None },
        F_CHIP,
        false,
    );
    x += kw + 4.0;

    // RTL / LTR chip
    let rtl_label = if app.rtl() { "RTL" } else { "LTR" };
    let rtl_fg = if app.rtl() { t.accent } else { t.fg_muted };
    let rw = c.measure_line_width(rtl_label, F_CHIP, false) + 10.0;
    let rtl_r = rect(x, border + 3.0, rw, 24.0);
    ui::chip(
        c,
        &mut app.hits,
        Id::Rtl,
        rtl_r,
        rtl_label,
        rtl_fg,
        if app.hover == Some(Id::Rtl) { Some(t.hover_bg) } else { None },
        F_CHIP,
        false,
    );

    // ---- title bar: action buttons on the right
    let mut rx = w - border - 2.0;
    let close_r = rect(rx - 24.0, border + 3.0, 24.0, 24.0);
    ui::button(
        c,
        &mut app.hits,
        &ui::Btn::new(Id::Close, close_r, if app.hover == Some(Id::Close) { 0xFFFFFFFF } else { t.fg_muted })
            .icon("close", 11.0)
            .bg(if app.hover == Some(Id::Close) { t.close_hover } else { t.bar_bg }),
    );
    rx -= 24.0;

    let gear_r = rect(rx - 24.0, border + 3.0, 24.0, 24.0);
    ui::button(
        c,
        &mut app.hits,
        &ui::Btn::new(Id::Gear, gear_r, if app.hover == Some(Id::Gear) { t.fg } else { t.fg_muted })
            .icon("gear", 13.0)
            .bg(if app.hover == Some(Id::Gear) { t.hover_bg } else { t.bar_bg })
            .angle(app.spin[0]),
    );
    rx -= 24.0;

    let theme_r = rect(rx - 24.0, border + 3.0, 24.0, 24.0);
    ui::button(
        c,
        &mut app.hits,
        &ui::Btn::new(Id::Theme, theme_r, if app.hover == Some(Id::Theme) { t.fg } else { t.fg_muted })
            .icon(if app.cfg.theme == "dark" { "sun" } else { "moon" }, 13.0)
            .bg(if app.hover == Some(Id::Theme) { t.hover_bg } else { t.bar_bg })
            .angle(app.spin[1]),
    );
    rx -= 24.0 + 2.0;

    let copy_r = rect(rx - 48.0, border + 3.0, 48.0, 24.0);
    let copy_fg = if app.copy_flash {
        t.accent_green
    } else if app.last_ok {
        t.accent
    } else {
        t.fg_muted
    };
    ui::button(
        c,
        &mut app.hits,
        &ui::Btn::new(Id::Copy, copy_r, if app.hover == Some(Id::Copy) { t.fg } else { copy_fg })
            .icon(if app.copy_flash { "check" } else { "copy" }, 12.0)
            .text("کپی", 11.0)
            .bg(if app.hover == Some(Id::Copy) { t.hover_bg } else { t.bar_bg }),
    );

    // ---- source preview
    let max_lines = (((h - 125.0) / 18.0) as i32).clamp(2, 5) as usize;
    let show_src = app.src_lines.len().min(max_lines).max(1);
    let src_y = border + bar_h + 1.0 + 6.0;
    let src_w = w - pad * 2.0 - 2.0;
    let src_h = show_src as f32 * SRC_LH;
    c.set_clip(pad, src_y, src_w, src_h);
    let src_rtl = is_rtl_text(&app.src_text);
    for (i, line) in app.src_lines.iter().take(show_src).enumerate() {
        c.draw_text(
            line,
            rect(pad + 2.0, src_y + i as f32 * SRC_LH, src_w - 4.0, SRC_LH),
            F_SRC,
            false,
            t.fg_muted,
            src_rtl,
            TextAlign::Near,
            true,
            false,
        );
    }
    c.reset_clip();

    // ---- footer (grip, spinner, status, watch badge)
    let foot_y = h - border - 4.0 - foot_h;
    let grip_x = w - pad - 16.0;
    let grip_r = rect(grip_x, foot_y + 3.0, 16.0, 16.0);
    ui::button(
        c,
        &mut app.hits,
        &ui::Btn::new(Id::Grip, grip_r, if app.hover == Some(Id::Grip) { t.fg_muted } else { t.fg_dim })
            .icon("grip", 12.0),
    );
    let spin_x = grip_x - 4.0 - 14.0;
    let spin_r = rect(spin_x, foot_y + 4.0, 14.0, 14.0);
    c.draw_spinner(spin_r.x + 7.0, spin_r.y + 7.0, app.spinner, app.spinner_angle, &t);
    let status_right = spin_x - 4.0;
    let sw = c.measure_line_width(&app.status, F_STATUS, false);
    c.draw_text(
        &app.status,
        rect(status_right - sw - 2.0, foot_y, sw + 2.0, foot_h),
        F_STATUS,
        false,
        t.fg_dim,
        true,
        TextAlign::Near,
        true,
        false,
    );
    if !app.cfg.watch {
        let badge = "ترجمه خودکار خاموش";
        let bw = label_at(c, badge, pad, foot_y, foot_h, F_STATUS, false, t.accent_amber, true);
        app.hits.add(Id::Watch, rect(pad, foot_y, bw + 4.0, foot_h));
    }

    // ---- output card
    let card_y = src_y + src_h + 2.0;
    let card_h = (foot_y - 4.0 - card_y).max(20.0);
    let card = rect(pad, card_y, w - pad * 2.0, card_h);
    c.fill_rect(card.x, card.y, card.width, card.height, t.card_bg);
    c.draw_rect(card.x, card.y, card.width, card.height, t.card_border, 1.0);
    app.hits.add(Id::Out, card);
    app.out_rect = rect(card.x + 1.0 + 8.0, card.y + 1.0 + 6.0, card.width - 2.0 - 16.0, card.height - 2.0 - 12.0);

    let inner = app.out_rect;
    let total = app.out_lines.len() as f32;
    let max_scroll = (total * OUT_LH - inner.height).max(0.0);
    if app.scroll > max_scroll {
        app.scroll = max_scroll;
    }
    if app.scroll < 0.0 {
        app.scroll = 0.0;
    }
    let text_color = if app.out_err { t.err_fg } else { t.fg };
    let rtl = app.rtl();
    c.set_clip(inner.x, inner.y, inner.width, inner.height);
    for (i, line) in app.out_lines.iter().enumerate() {
        let y = inner.y + i as f32 * OUT_LH - app.scroll;
        if y + OUT_LH < inner.y || y > inner.y + inner.height {
            continue;
        }
        if let (Some(a), Some(b)) = (app.sel_start, app.sel_end) {
            let (lo, hi) = (a.min(b), a.max(b));
            if i as u32 >= lo && i as u32 <= hi {
                c.fill_rect(inner.x, y, inner.width, OUT_LH, t.select_bg);
            }
        }
        c.draw_text(line, rect(inner.x, y, inner.width, OUT_LH), F_OUT, false, text_color, rtl, TextAlign::Near, true, false);
    }
    c.reset_clip();
}

// ------------------------------------------------------------------ ticking
fn tick(app: &mut App) {
    let now = Instant::now();
    let mut due: Vec<Pending> = Vec::new();
    let mut i = 0;
    while i < app.pending.len() {
        if app.pending[i].0 <= now {
            due.push(app.pending.remove(i).1);
        } else {
            i += 1;
        }
    }
    for p in due {
        match p {
            Pending::CtrlCAgain => win32::send_ctrl_c(),
            Pending::ReadClipboard(retry) => app.read_clipboard(retry),
            Pending::CopyIconRestore => {
                app.copy_flash = false;
                app.dirty = true;
            }
            Pending::OpenSettings => dialogs::open_settings(app),
            Pending::KeepTopmost => {
                if app.settings.is_none() && app.keys.is_none() {
                    unsafe {
                        SetWindowPos(
                            app.hwnd,
                            HWND_TOPMOST,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
                app.pending
                    .push((now + Duration::from_millis(4000), Pending::KeepTopmost));
            }
        }
    }

    if let Ok(mut slot) = RESULT.lock() {
        if let Some((seq, res)) = slot.take() {
            drop(slot);
            app.apply_result(seq, res);
        }
    }

    if app.spinner == SpinnerState::Loading {
        app.spinner_angle = (app.spinner_angle + 30.0) % 360.0;
        app.dirty = true;
    }
    let hover_spin = [Id::Gear, Id::Theme, Id::Close];
    let mut spinning = false;
    for (i, id) in hover_spin.iter().enumerate() {
        if app.hover == Some(*id) {
            app.spin[i] = (app.spin[i] + 24.0) % 360.0;
            spinning = true;
        }
    }
    if spinning {
        app.dirty = true;
    }

    dialogs::tick(app);

    if app.dirty {
        app.dirty = false;
        invalidate(app.hwnd);
    }
}

fn set_cursor_for(app: &App) -> bool {
    let cursor = match app.hover {
        Some(Id::Out) => unsafe { LoadCursorW(std::ptr::null_mut(), IDC_IBEAM) },
        Some(Id::Grip) => unsafe { LoadCursorW(std::ptr::null_mut(), IDC_SIZENWSE) },
        Some(Id::Lock) | Some(Id::Model) | Some(Id::Key) | Some(Id::Rtl) | Some(Id::Copy)
        | Some(Id::Theme) | Some(Id::Gear) | Some(Id::Close) | Some(Id::Watch) => {
            unsafe { LoadCursorW(std::ptr::null_mut(), IDC_HAND) }
        }
        _ => unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
    };
    unsafe {
        SetCursor(cursor);
    }
    true
}

// ------------------------------------------------------------------ window proc
unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if !app_ready() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let app = app();
    match msg {
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            paint_main(app, hdc, rc.right as f32, rc.bottom as f32);
            EndPaint(hwnd, &ps);
            0
        }
        WM_ERASEBKGND => 1,
        WM_TIMER => {
            match wparam {
                TIMER_UI => tick(app),
                TIMER_MOUSE => {
                    if app.watcher.poll() {
                        app.grab_selection();
                    }
                }
                _ => {}
            }
            0
        }
        WM_MOUSEMOVE => {
            let (x, y) = mouse_xy(lparam);
            if let Some((gx, gy)) = app.drag {
                let mut pt = POINT { x: 0, y: 0 };
                GetCursorPos(&mut pt);
                SetWindowPos(hwnd, std::ptr::null_mut(), pt.x - gx, pt.y - gy, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
                return 0;
            }
            if let Some(((rx, ry), (rw, rh))) = app.resize {
                let mut pt = POINT { x: 0, y: 0 };
                GetCursorPos(&mut pt);
                let nw = (rw + (pt.x - rx)).clamp(340, 800);
                let nh = (rh + (pt.y - ry)).clamp(170, 600);
                SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, nw, nh, SWP_NOMOVE | SWP_NOZORDER);
                return 0;
            }
            if app.selecting {
                set_selection(app, y);
                return 0;
            }
            let hit = app.hits.hit(x, y);
            if hit != app.hover {
                app.hover = hit;
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut tme);
                app.dirty = true;
            }
            0
        }
        WM_MOUSELEAVE => {
            if app.hover.is_some() {
                app.hover = None;
                app.dirty = true;
            }
            0
        }
        WM_LBUTTONDOWN => {
            let (x, y) = mouse_xy(lparam);
            let Some(id) = app.hits.hit(x, y) else { return 0 };
            match id {
                Id::Bar => {
                    if !app.cfg.locked {
                        let mut pt = POINT { x: 0, y: 0 };
                        GetCursorPos(&mut pt);
                        let mut wr: RECT = std::mem::zeroed();
                        GetWindowRect(hwnd, &mut wr);
                        app.drag = Some((pt.x - wr.left, pt.y - wr.top));
                        SetCapture(hwnd);
                    }
                }
                Id::Lock => app.toggle_lock(),
                Id::Model => open_menu_at(app, Id::Model, (0, 0)),
                Id::Key => open_menu_at(app, Id::Key, (0, 0)),
                Id::Rtl => app.toggle_rtl(),
                Id::Copy => app.copy_translation(),
                Id::Theme => {
                    let next = if app.cfg.theme == "dark" { "light" } else { "dark" };
                    app.set_theme(next);
                    let s = format!("تم: {}", if next == "dark" { "تاریک" } else { "روشن" });
                    app.set_status(&s);
                }
                Id::Gear => dialogs::open_settings(app),
                Id::Close => {
                    DestroyWindow(hwnd);
                }
                Id::Grip => {
                    let mut pt = POINT { x: 0, y: 0 };
                    GetCursorPos(&mut pt);
                    let mut wr: RECT = std::mem::zeroed();
                    GetWindowRect(hwnd, &mut wr);
                    app.resize = Some(((pt.x, pt.y), (wr.right - wr.left, wr.bottom - wr.top)));
                    SetCapture(hwnd);
                }
                Id::Watch => {
                    let v = !app.cfg.watch;
                    app.set_watch(v);
                }
                Id::Out => {
                    app.selecting = true;
                    app.sel_anchor = Some(line_at_y(app, y));
                    app.sel_start = app.sel_anchor;
                    app.sel_end = app.sel_anchor;
                    SetCapture(hwnd);
                    app.dirty = true;
                }
                _ => {}
            }
            0
        }
        WM_LBUTTONUP => {
            if app.drag.is_some() {
                app.drag = None;
                ReleaseCapture();
                let mut wr: RECT = std::mem::zeroed();
                GetWindowRect(hwnd, &mut wr);
                app.cfg.x = wr.left;
                app.cfg.y = wr.top;
                app.cfg.save();
                app.set_status("موقعیت ذخیره شد");
            }
            if let Some(_) = app.resize {
                app.resize = None;
                ReleaseCapture();
                let mut wr: RECT = std::mem::zeroed();
                GetWindowRect(hwnd, &mut wr);
                app.cfg.width = wr.right - wr.left;
                app.cfg.height = wr.bottom - wr.top;
                app.cfg.save();
                app.rewrap_out();
                app.dirty = true;
            }
            if app.selecting {
                app.selecting = false;
                ReleaseCapture();
                app.dirty = true;
            }
            0
        }
        WM_RBUTTONDOWN => {
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            let items = context_menu_items(app);
            menu::open(app, items, pt.x, pt.y);
            0
        }
        WM_MOUSEWHEEL => {
            let delta = (((wparam >> 16) & 0xFFFF) as i16 as i32) as f32;
            app.scroll -= delta / 120.0 * OUT_LH * 2.0;
            app.dirty = true;
            0
        }
        WM_KEYDOWN => {
            let ctrl = GetKeyState(VK_CONTROL as i32) < 0;
            let shift = GetKeyState(VK_SHIFT as i32) < 0;
            if wparam as i32 == VK_ESCAPE as i32 {
                menu::close(app);
            } else if ctrl && wparam as i32 == VK_C as i32 {
                if shift {
                    app.copy_translation();
                } else {
                    app.copy_selection_or_all();
                }
            }
            0
        }
        WM_SETCURSOR => {
            set_cursor_for(app);
            1
        }
        WM_ACTIVATE => {
            if (wparam & 0xFFFF) as u32 == 0 {
                menu::close(app);
            }
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_UI);
            KillTimer(hwnd, TIMER_MOUSE);
            app.cfg.save();
            menu::close(app);
            dialogs::close_all(app);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn line_at_y(app: &App, y: f32) -> u32 {
    let rel = y - app.out_rect.y + app.scroll;
    let idx = (rel / OUT_LH).floor().max(0.0) as u32;
    idx.min(app.out_lines.len().saturating_sub(1) as u32)
}

fn set_selection(app: &mut App, y: f32) {
    app.sel_end = Some(line_at_y(app, y));
    app.dirty = true;
}

// ------------------------------------------------------------------ entry point
fn main() {
    // A GUI process has nowhere to print; keep panics on disk instead of dying silently.
    std::panic::set_hook(Box::new(|info| {
        let _ = std::fs::write(
            config::app_dir().join("panic.log"),
            format!("{}\n{:?}\n", info, std::backtrace::Backtrace::capture()),
        );
    }));
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--selftest") {
        selftest();
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--translate") {
        let text = args.get(pos + 1).cloned().unwrap_or_else(|| "Hello, this is a connection test.".to_string());
        let cfg = AppConfig::load();
        let log = config::app_dir().join("translate.log");
        let _ = std::fs::write(&log, format!("start model={} keys={} proxy={}", cfg.model, cfg.api_keys.len(), cfg.proxy));
        let msg = match http::translate(&text, &cfg) {
            Ok((_src, out)) => format!("OK {} => {}", cfg.model, out),
            Err(e) => format!("ERR {}", e),
        };
        let _ = std::fs::write(&log, &msg);
        std::process::exit(0);
    }
    if args.iter().any(|a| a == "--debug") {
        config::set_debug(true);
    }
    unsafe {
        let _gdiplus = GdiplusGuard::init();
        let hinst = GetModuleHandleW(std::ptr::null());

        let name = to_wide("FloatingTranslator");
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.style = CS_HREDRAW | CS_VREDRAW;
        wc.lpfnWndProc = Some(main_proc);
        wc.hInstance = hinst;
        wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        wc.lpszClassName = name.as_ptr();
        RegisterClassW(&wc);
        menu::register_class();
        dialogs::register_classes();

        let cfg = AppConfig::load();
        let wa = win32::work_area();
        let w = cfg.width.clamp(340, 900);
        let h = cfg.height.clamp(170, 700);
        let x = cfg.x.clamp(wa.left, (wa.right - w).max(wa.left));
        let y = cfg.y.clamp(wa.top, (wa.bottom - h).max(wa.top));

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            name.as_ptr(),
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
        if hwnd.is_null() {
            return;
        }
        *APP.0.get() = Some(App::new(hwnd, cfg));
        if args.iter().any(|a| a == "--demo") {
            demo(app());
        }
        if args.iter().any(|a| a == "--settings") {
            dialogs::open_settings(app());
        }
        if args.iter().any(|a| a == "--keys") {
            dialogs::open_keys(app());
        }
        apply_window_opacity(hwnd, app().cfg.opacity);
        apply_window_corners(hwnd);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetTimer(hwnd, TIMER_UI, UI_MS, None);
        SetTimer(hwnd, TIMER_MOUSE, MOUSE_MS, None);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

// ------------------------------------------------------------------ demo (dev only)
fn demo(app: &mut App) {
    let src = "This is a sample sentence used to check line wrapping, direction and copy.";
    let out = "این یک متن نمونه است؛ جهت راست‌به‌چپ، شکستن خط و کپی انتخاب باید درست کار کند. \
عدد 1234 و کلمه PDF داخل متن انگلیسی است تا ترتیب دوجهته آزمایش شود. \
خط دوم متن برای بررسی پیمایش عمودی و رنگ خط انتخاب‌شده اضافه شده است.";
    app.src_text = src.to_string();
    app.src_lines = wrap_cached(src, 380.0, F_SRC, false);
    app.last_ok = true;
    app.spinner = SpinnerState::Ok;
    app.set_status("ترجمه شد");
    app.set_out(out, false);
}

// ------------------------------------------------------------------ selftest
fn selftest() {
    let _gdiplus = GdiplusGuard::init();
    let mut out = String::new();
    let mut ok = true;
    let mut check = |name: &str, cond: bool| {
        out.push_str(&format!("{} {}\n", if cond { "ok  " } else { "FAIL" }, name));
        if !cond {
            ok = false;
        }
    };

    // word wrapping keeps every word and never exceeds the width
    with_canvas(|c| {
        let text = "این یک متن نمونه است که باید در چند خط شکسته شود و هیچ کلمه‌ای گم نشود.";
        let lines = wrap_lines(c, text, 200.0, F_OUT, false);
        let joined = lines.join(" ");
        check("wrap keeps all words", joined.split_whitespace().count() == text.split_whitespace().count());
        check("wrap produces multiple lines", lines.len() > 1);
        check(
            "wrap respects width",
            lines.iter().all(|l| c.measure_line_width(l, F_OUT, false) <= 210.0),
        );
    });

    check("rtl detection", is_rtl_text("سلام") && !is_rtl_text("hello"));

    let cfg = AppConfig::default();
    check("default model exists", config::find_model(&cfg.model).id == cfg.model);
    check("rtl default from target", cfg.effective_rtl());
    check("gemini model is gemini kind", config::find_model("gemini-4-hydrogen").kind == ModelKind::Gemini);
    check("google model is google kind", config::find_model("google-translate").kind == ModelKind::Google);

    let json = "{\"a\":[1,2,{\"b\":\"x\\ny\"}],\"c\":true,\"d\":null}";
    let parsed = json::parse_json(json).ok();
    check("json round trip", parsed.map(|v| v.to_compact_string() == json).unwrap_or(false));

    let field = Field::new(Id::SKey, "abc".to_string(), false, 10);
    check("field caret at end", field.caret == 3);

    drop(check);
    out.push_str(&format!("\n{}\n", if ok { "ALL OK" } else { "FAILURES" }));
    print!("{}", out);
    let _ = std::fs::write(config::app_dir().join("selftest.log"), &out);
    std::process::exit(if ok { 0 } else { 1 });
}
