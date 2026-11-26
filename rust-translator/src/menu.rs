//! Themed popup menus (context menu + model/key dropdowns).
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, EndPaint,
    InvalidateRect, SelectObject, SRCCOPY,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetSystemMetrics, LoadCursorW,
    RegisterClassW, SetForegroundWindow, CS_HREDRAW, CS_VREDRAW, IDC_ARROW, SM_CXSCREEN,
    SM_CYSCREEN, WM_ACTIVATE, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_PAINT,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE, WNDCLASSW,
};

use crate::gdiplus::{GpCanvas, TextAlign};
use crate::ui::{rect, Hits, Id};
use crate::win32::to_wide;
use crate::App;

const ITEM_H: f32 = 26.0;
const SEP_H: f32 = 9.0;
const PAD: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Action,
    Check,
    Separator,
}

#[derive(Clone, PartialEq)]
pub enum MenuAction {
    SetModel(String),
    SetKey(String),
    OpenKeys,
    OpenSettings,
    FromClipboard,
    CopyTranslation,
    ToggleWatch(bool),
    ToggleAutocopy(bool),
    ToggleLock,
    ToggleRtl,
    ToggleTheme(bool),
    SetOpacity(f32),
    Exit,
    TestConnection,
    /// Model picked from the settings window's dropdown (applies on save).
    PickSettingsModel(String),
}

pub struct MenuItem {
    pub label: String,
    pub kind: ItemKind,
    pub checked: bool,
    pub action: Option<MenuAction>,
    pub accent: bool,
}

impl MenuItem {
    pub fn action(label: &str, action: MenuAction) -> Self {
        Self { label: label.to_string(), kind: ItemKind::Action, checked: false, action: Some(action), accent: false }
    }

    pub fn check(label: &str, checked: bool, action: MenuAction) -> Self {
        Self { label: label.to_string(), kind: ItemKind::Check, checked, action: Some(action), accent: false }
    }

    pub fn separator() -> Self {
        Self { label: String::new(), kind: ItemKind::Separator, checked: false, action: None, accent: false }
    }
}

pub struct Menu {
    pub hwnd: HWND,
    pub items: Vec<MenuItem>,
    pub hits: Hits,
    pub hover: Option<usize>,
    pub width: f32,
    pub height: f32,
}

const CLASS: &str = "FTMenu";

fn class_name() -> Vec<u16> {
    to_wide(CLASS)
}

pub fn register_class() {
    unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let name = class_name();
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.style = CS_HREDRAW | CS_VREDRAW;
        wc.lpfnWndProc = Some(menu_proc);
        wc.hInstance = hinst;
        wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        wc.lpszClassName = name.as_ptr();
        RegisterClassW(&wc);
    }
}

/// Opens a menu at (x, y); measured and clamped to the screen.
pub fn open(app: &mut App, items: Vec<MenuItem>, x: i32, y: i32) {
    close(app);
    let hdc = unsafe { windows_sys::Win32::Graphics::Gdi::GetDC(std::ptr::null_mut()) };
    let (mut w, mut h) = (140.0f32, PAD * 2.0);
    if let Some(c) = GpCanvas::from_hdc(hdc) {
        for it in &items {
            match it.kind {
                ItemKind::Separator => h += SEP_H,
                _ => {
                    h += ITEM_H;
                    let tw = c.measure_line_width(&it.label, 13.0, false);
                    w = w.max(tw + 62.0);
                }
            }
        }
    }
    unsafe {
        windows_sys::Win32::Graphics::Gdi::ReleaseDC(std::ptr::null_mut(), hdc);
    }
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let px = x.min((sw as f32 - w - 4.0).max(4.0) as i32).max(4);
    let py = y.min((sh as f32 - h - 4.0).max(4.0) as i32).max(4);
    let (wi, hi) = (w.ceil() as i32, h.ceil() as i32);
    let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
    let name = class_name();
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            name.as_ptr(),
            name.as_ptr(),
            WS_POPUP | WS_VISIBLE,
            px,
            py,
            wi,
            hi,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null_mut(),
        )
    };
    app.menu = Some(Box::new(Menu { hwnd, items, hits: Hits::default(), hover: None, width: w, height: h }));
    unsafe {
        SetForegroundWindow(hwnd);
    }
}

pub fn close(app: &mut App) {
    if let Some(m) = app.menu.take() {
        unsafe {
            DestroyWindow(m.hwnd);
        }
    }
}

unsafe extern "system" fn menu_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let app = crate::app();
    match msg {
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            paint(app, hdc, rc.right as f32, rc.bottom as f32);
            EndPaint(hwnd, &ps);
            0
        }
        WM_MOUSEMOVE => {
            let (x, y) = mouse_xy(lparam);
            let hit = app.menu.as_ref().and_then(|m| m.hits.hit(x, y));
            let index = hit.and_then(|id| match id {
                Id::MenuRow(i) => Some(i as usize),
                _ => None,
            });
            if app.menu.as_ref().map(|m| m.hover) != Some(index) {
                if let Some(m) = app.menu.as_mut() {
                    m.hover = index;
                }
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            0
        }
        WM_LBUTTONDOWN => {
            let (x, y) = mouse_xy(lparam);
            if let Some(m) = app.menu.as_ref() {
                if let Some(id) = m.hits.hit(x, y) {
                    if let Id::MenuRow(i) = id {
                        if let Some(item) = m.items.get(i as usize) {
                            let action = item.action.clone();
                            close(app);
                            if let Some(a) = action {
                                crate::apply_menu_action(app, a);
                            }
                            return 0;
                        }
                    }
                }
            }
            close(app);
            0
        }
        WM_KEYDOWN => {
            if wparam as i32 == VK_ESCAPE as i32 {
                close(app);
            }
            0
        }
        WM_ACTIVATE => {
            // lost activation (click outside) -> dismiss
            if (wparam & 0xFFFF) as u32 == 0 && app.menu.as_ref().map(|m| m.hwnd) == Some(hwnd) {
                close(app);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn mouse_xy(lparam: LPARAM) -> (f32, f32) {
    let x = (lparam & 0xFFFF) as i16 as f32;
    let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
    (x, y)
}

fn paint(app: &mut App, hdc: windows_sys::Win32::Graphics::Gdi::HDC, w: f32, h: f32) {    let mem = unsafe { CreateCompatibleDC(hdc) };
    let bmp = unsafe { CreateCompatibleBitmap(hdc, w as i32, h as i32) };
    let old = unsafe { SelectObject(mem, bmp) };
    let t = app.theme;
    if let Some(m) = app.menu.as_mut() {
        m.hits.clear();
        if let Some(c) = GpCanvas::from_hdc(mem) {
            c.fill_rect(0.0, 0.0, w, h, t.border);
            c.fill_rect(1.0, 1.0, w - 2.0, h - 2.0, t.card_bg);
            let mut y = PAD + 1.0;
            for (i, item) in m.items.iter().enumerate() {
                if item.kind == ItemKind::Separator {
                    c.fill_rect(PAD + 1.0, y + SEP_H / 2.0, w - PAD * 2.0 - 2.0, 1.0, t.divider);
                    y += SEP_H;
                    continue;
                }
                let r = rect(1.0, y, w - 2.0, ITEM_H);
                if m.hover == Some(i) {
                    c.fill_rect(1.0, y, w - 2.0, ITEM_H, t.hover_bg);
                }
                let fg = if item.accent { t.accent } else { t.fg };
                // RTL: "near" is the right edge, so labels hug the right and checks sit at the left.
                let label_right = if item.checked { w - 30.0 } else { w - 14.0 };
                c.draw_text(&item.label, rect(9.0, y, label_right - 9.0, ITEM_H), 13.0, false, fg, true, TextAlign::Near, true, false);
                if item.checked {
                    c.draw_icon("check", w - 18.0, y + ITEM_H / 2.0, 12.0, t.accent);
                }
                m.hits.add(Id::MenuRow(i as u32), r);
                y += ITEM_H;
            }
        }
    }
    unsafe {
        BitBlt(hdc, 0, 0, w as i32, h as i32, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
    }
}

