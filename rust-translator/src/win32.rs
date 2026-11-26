use std::ffi::c_void;
use std::time::Instant;

use windows_sys::Win32::Foundation::{BOOL, HWND, POINT, RECT};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    SetClipboardData,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, GetAsyncKeyState, KEYEVENTF_KEYUP, VK_C, VK_CONTROL, VK_LBUTTON,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, GetWindowThreadProcessId, SetLayeredWindowAttributes,
    SystemParametersInfoW, WindowFromPoint, LWA_ALPHA, SM_CXSCREEN, SM_CYSCREEN, SPI_GETWORKAREA,
};

const CF_UNICODETEXT: u32 = 13;
const TRIGGER_DRAG_PX: i32 = 8;
const DOUBLE_CLICK_MS: u128 = 450;
const DOUBLE_CLICK_PX: i32 = 6;

pub fn to_wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

pub fn from_wide_ptr(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe {
        let mut len = 0usize;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        String::from_utf16_lossy(slice)
    }
}

pub fn clipboard_seq() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

pub fn set_clipboard(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let wide = to_wide(text);
    let byte_len = wide.len() * 2;
    unsafe {
        let hmem = GlobalAlloc(GMEM_MOVEABLE, byte_len);
        if hmem.is_null() {
            return false;
        }
        let ptr = GlobalLock(hmem) as *mut u16;
        if ptr.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        GlobalUnlock(hmem);

        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return false;
        }
        EmptyClipboard();
        let res = SetClipboardData(CF_UNICODETEXT, hmem);
        CloseClipboard();
        !res.is_null()
    }
}

pub fn clipboard_text() -> String {
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return String::new();
        }
        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_null() {
            CloseClipboard();
            return String::new();
        }
        let ptr = GlobalLock(handle) as *const u16;
        let text = if ptr.is_null() {
            String::new()
        } else {
            let s = from_wide_ptr(ptr);
            GlobalUnlock(handle);
            s
        };
        CloseClipboard();
        text
    }
}

pub fn cursor_pos() -> (i32, i32) {
    let mut pt = POINT { x: 0, y: 0 };
    unsafe {
        GetCursorPos(&mut pt);
    }
    (pt.x, pt.y)
}

pub fn is_our_window(pt: (i32, i32)) -> bool {
    unsafe {
        let hwnd = WindowFromPoint(POINT { x: pt.0, y: pt.1 });
        if hwnd.is_null() {
            return false;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        pid == GetCurrentProcessId()
    }
}

pub fn send_ctrl_c() {
    unsafe {
        keybd_event(VK_CONTROL as u8, 0, 0, 0);
        keybd_event(VK_C as u8, 0, 0, 0);
        keybd_event(VK_C as u8, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_CONTROL as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}

pub fn is_lbutton_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0 }
}

pub struct MousePoller {
    down: Option<(i32, i32)>,
    last_pt: Option<(i32, i32)>,
    last_up: Option<Instant>,
    armed: bool,
    pub enabled: bool,
}

impl MousePoller {
    pub fn new(enabled: bool) -> Self {
        Self {
            down: None,
            last_pt: None,
            last_up: None,
            armed: false,
            enabled,
        }
    }

    pub fn poll(&mut self) -> bool {
        let pt = cursor_pos();
        let pressed = is_lbutton_down();
        self.feed(pt, pressed, Instant::now())
    }

    pub fn feed(&mut self, pt: (i32, i32), pressed: bool, now: Instant) -> bool {
        let mut triggered = false;
        if pressed && self.down.is_none() {
            self.down = Some(pt);
            self.armed = !is_our_window(pt);
        } else if !pressed {
            if let Some(down) = self.down.take() {
                if self.enabled && self.armed && !is_our_window(pt) {
                    let moved = (pt.0 - down.0).abs().max((pt.1 - down.1).abs());
                    let is_drag = moved >= TRIGGER_DRAG_PX;
                    let is_dbl = match (self.last_up, self.last_pt) {
                        (Some(lup), Some(lpt)) => {
                            now.duration_since(lup).as_millis() < DOUBLE_CLICK_MS
                                && (pt.0 - lpt.0).abs().max((pt.1 - lpt.1).abs()) <= DOUBLE_CLICK_PX
                        }
                        _ => false,
                    };
                    if is_drag || is_dbl {
                        triggered = true;
                    }
                }
                self.last_pt = Some(pt);
                self.last_up = Some(now);
            }
        }
        triggered
    }
}

pub fn screen_size() -> (i32, i32) {
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

pub fn work_area() -> RECT {
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };
    unsafe {
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rc as *mut RECT as *mut c_void, 0) == 0 {
            let (w, h) = screen_size();
            rc.right = w;
            rc.bottom = h;
        }
    }
    rc
}

pub fn apply_window_opacity(hwnd: HWND, opacity: f32) {
    let alpha = (opacity.clamp(0.3, 1.0) * 255.0).round() as u8;
    unsafe {
        SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
    }
}

pub fn apply_window_corners(hwnd: HWND) {
    // Windows 11 DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE = 33, DWMWCP_ROUND = 2)
    unsafe {
        let lib_name = to_wide("dwmapi.dll");
        let hmod = LoadLibraryW(lib_name.as_ptr());
        if hmod.is_null() {
            return;
        }
        let proc = GetProcAddress(hmod, b"DwmSetWindowAttribute\0".as_ptr());
        if let Some(func) = proc {
            type DwmSetAttrFn = unsafe extern "system" fn(HWND, u32, *const c_void, u32) -> i32;
            let f: DwmSetAttrFn = std::mem::transmute(func);
            let pref: u32 = 2; // DWMWCP_ROUND
            let _ = f(hwnd, 33, &pref as *const u32 as *const c_void, 4);
            // Also enable dark mode titlebar for child dialogs if applicable (DWMWA_USE_IMMERSIVE_DARK_MODE = 20)
            let dark: BOOL = 1;
            let _ = f(hwnd, 20, &dark as *const BOOL as *const c_void, 4);
        }
    }
}

#[allow(dead_code)] // kept for when the dialogs get a native frame
pub fn apply_dark_titlebar(hwnd: HWND, dark: bool) {
    unsafe {
        let lib_name = to_wide("dwmapi.dll");
        let hmod = LoadLibraryW(lib_name.as_ptr());
        if hmod.is_null() {
            return;
        }
        let proc = GetProcAddress(hmod, b"DwmSetWindowAttribute\0".as_ptr());
        if let Some(func) = proc {
            type DwmSetAttrFn = unsafe extern "system" fn(HWND, u32, *const c_void, u32) -> i32;
            let f: DwmSetAttrFn = std::mem::transmute(func);
            let val: BOOL = if dark { 1 } else { 0 };
            let _ = f(hwnd, 20, &val as *const BOOL as *const c_void, 4);
        }
    }
}

// Deliberately not called: staying DPI-unaware keeps coordinates identical to Tk's scaling.
#[allow(dead_code)]
pub fn enable_dpi_awareness() {
    unsafe {
        let user32_name = to_wide("user32.dll");
        let hmod = GetModuleHandleW(user32_name.as_ptr());
        if !hmod.is_null() {
            if let Some(proc) = GetProcAddress(hmod, b"SetProcessDpiAwarenessContext\0".as_ptr()) {
                type SetDpiCtxFn = unsafe extern "system" fn(isize) -> BOOL;
                let f: SetDpiCtxFn = std::mem::transmute(proc);
                // DPI_AWARENESS_CONTEXT_SYSTEM_AWARE = -2 (keeps pixel coordinates consistent with 96 DPI scaling)
                // Or PER_MONITOR_AWARE_V2 = -4
                let _ = f(-4);
            }
        }
    }
}
