use std::ffi::c_void;

use windows_sys::Win32::Graphics::Gdi::HDC;

use crate::theme::Theme;
use crate::win32::to_wide;

#[repr(C)]
struct GdiplusStartupInput {
    gdiplus_version: u32,
    debug_event_callback: *const c_void,
    suppress_background_thread: i32,
    suppress_external_codecs: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[link(name = "gdiplus")]
extern "system" {
    fn GdiplusStartup(
        token: *mut usize,
        input: *const GdiplusStartupInput,
        output: *mut c_void,
    ) -> i32;
    fn GdiplusShutdown(token: usize);

    fn GdipCreateFromHDC(hdc: HDC, graphics: *mut *mut c_void) -> i32;
    fn GdipDeleteGraphics(graphics: *mut c_void) -> i32;
    fn GdipSetSmoothingMode(graphics: *mut c_void, mode: i32) -> i32;
    fn GdipSetTextRenderingHint(graphics: *mut c_void, mode: i32) -> i32;
    fn GdipSetClipRect(
        graphics: *mut c_void,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        combine_mode: i32,
    ) -> i32;
    fn GdipResetClip(graphics: *mut c_void) -> i32;

    fn GdipCreateSolidFill(color: u32, brush: *mut *mut c_void) -> i32;
    fn GdipDeleteBrush(brush: *mut c_void) -> i32;

    fn GdipCreatePen1(color: u32, width: f32, unit: i32, pen: *mut *mut c_void) -> i32;
    fn GdipSetPenStartCap(pen: *mut c_void, cap: i32) -> i32;
    fn GdipSetPenEndCap(pen: *mut c_void, cap: i32) -> i32;
    fn GdipSetPenLineJoin(pen: *mut c_void, join: i32) -> i32;
    fn GdipDeletePen(pen: *mut c_void) -> i32;

    fn GdipCreatePath(fill_mode: i32, path: *mut *mut c_void) -> i32;
    fn GdipDeletePath(path: *mut c_void) -> i32;
    fn GdipAddPathArc(
        path: *mut c_void,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        start_angle: f32,
        sweep_angle: f32,
    ) -> i32;
    fn GdipClosePathFigure(path: *mut c_void) -> i32;

    fn GdipFillPath(graphics: *mut c_void, brush: *mut c_void, path: *mut c_void) -> i32;
    fn GdipDrawPath(graphics: *mut c_void, pen: *mut c_void, path: *mut c_void) -> i32;
    fn GdipFillRectangle(
        graphics: *mut c_void,
        brush: *mut c_void,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> i32;
    fn GdipDrawRectangle(
        graphics: *mut c_void,
        pen: *mut c_void,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> i32;
    fn GdipFillEllipse(
        graphics: *mut c_void,
        brush: *mut c_void,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> i32;
    fn GdipDrawEllipse(
        graphics: *mut c_void,
        pen: *mut c_void,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> i32;
    fn GdipDrawArc(
        graphics: *mut c_void,
        pen: *mut c_void,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        start_angle: f32,
        sweep_angle: f32,
    ) -> i32;
    fn GdipDrawLine(
        graphics: *mut c_void,
        pen: *mut c_void,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    ) -> i32;

    fn GdipCreateFontFamilyFromName(
        name: *const u16,
        font_collection: *mut c_void,
        family: *mut *mut c_void,
    ) -> i32;
    fn GdipDeleteFontFamily(family: *mut c_void) -> i32;
    fn GdipCreateFont(
        family: *mut c_void,
        em_size: f32,
        style: i32,
        unit: i32,
        font: *mut *mut c_void,
    ) -> i32;
    fn GdipDeleteFont(font: *mut c_void) -> i32;

    fn GdipCreateStringFormat(format_attributes: i32, language: u16, format: *mut *mut c_void) -> i32;
    fn GdipDeleteStringFormat(format: *mut c_void) -> i32;
    fn GdipSetStringFormatAlign(format: *mut c_void, align: i32) -> i32;
    fn GdipSetStringFormatLineAlign(format: *mut c_void, align: i32) -> i32;
    fn GdipSetStringFormatFlags(format: *mut c_void, flags: i32) -> i32;
    fn GdipSetStringFormatTrimming(format: *mut c_void, trimming: i32) -> i32;

    fn GdipDrawString(
        graphics: *mut c_void,
        string: *const u16,
        length: i32,
        font: *mut c_void,
        layout_rect: *const RectF,
        string_format: *mut c_void,
        brush: *mut c_void,
    ) -> i32;
    fn GdipMeasureString(
        graphics: *mut c_void,
        string: *const u16,
        length: i32,
        font: *mut c_void,
        layout_rect: *const RectF,
        string_format: *mut c_void,
        bounding_box: *mut RectF,
        codepoints_fitted: *mut i32,
        lines_filled: *mut i32,
    ) -> i32;
}

const SMOOTHING_MODE_ANTI_ALIAS: i32 = 4;
const TEXT_RENDERING_CLEARTYPE: i32 = 5;
const UNIT_PIXEL: i32 = 2;
const LINE_CAP_ROUND: i32 = 2;
const LINE_JOIN_ROUND: i32 = 2;
const STRING_FORMAT_FLAGS_RTL: i32 = 0x0000_0001;
const STRING_FORMAT_FLAGS_NO_WRAP: i32 = 0x0000_1000;
const STRING_TRIMMING_ELLIPSIS_CHAR: i32 = 3;

pub struct GdiplusGuard {
    token: usize,
}

impl GdiplusGuard {
    pub fn init() -> Self {
        let mut token = 0usize;
        let input = GdiplusStartupInput {
            gdiplus_version: 1,
            debug_event_callback: std::ptr::null(),
            suppress_background_thread: 0,
            suppress_external_codecs: 0,
        };
        unsafe {
            GdiplusStartup(&mut token, &input, std::ptr::null_mut());
        }
        Self { token }
    }
}

impl Drop for GdiplusGuard {
    fn drop(&mut self) {
        if self.token != 0 {
            unsafe {
                GdiplusShutdown(self.token);
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum TextAlign {
    Near,
    Center,
    Far,
}

pub struct GpCanvas {
    g: *mut c_void,
}

impl GpCanvas {
    pub fn from_hdc(hdc: HDC) -> Option<Self> {
        let mut g: *mut c_void = std::ptr::null_mut();
        unsafe {
            if GdipCreateFromHDC(hdc, &mut g) != 0 || g.is_null() {
                return None;
            }
            GdipSetSmoothingMode(g, SMOOTHING_MODE_ANTI_ALIAS);
            GdipSetTextRenderingHint(g, TEXT_RENDERING_CLEARTYPE);
        }
        Some(Self { g })
    }

    pub fn set_clip(&self, x: f32, y: f32, w: f32, h: f32) {
        unsafe {
            GdipSetClipRect(self.g, x, y, w, h, 0);
        }
    }

    pub fn reset_clip(&self) {
        unsafe {
            GdipResetClip(self.g);
        }
    }

    pub fn fill_rect(&self, x: f32, y: f32, w: f32, h: f32, color: u32) {
        unsafe {
            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(color, &mut brush) == 0 {
                GdipFillRectangle(self.g, brush, x, y, w, h);
                GdipDeleteBrush(brush);
            }
        }
    }

    pub fn draw_rect(&self, x: f32, y: f32, w: f32, h: f32, color: u32, stroke: f32) {
        unsafe {
            let mut pen = std::ptr::null_mut();
            if GdipCreatePen1(color, stroke, UNIT_PIXEL, &mut pen) == 0 {
                GdipDrawRectangle(self.g, pen, x, y, w, h);
                GdipDeletePen(pen);
            }
        }
    }

    fn build_round_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> *mut c_void {
        let mut path = std::ptr::null_mut();
        let d = (r * 2.0).min(w).min(h).max(0.1);
        unsafe {
            if GdipCreatePath(0, &mut path) != 0 {
                return std::ptr::null_mut();
            }
            GdipAddPathArc(path, x, y, d, d, 180.0, 90.0);
            GdipAddPathArc(path, x + w - d, y, d, d, 270.0, 90.0);
            GdipAddPathArc(path, x + w - d, y + h - d, d, d, 0.0, 90.0);
            GdipAddPathArc(path, x, y + h - d, d, d, 90.0, 90.0);
            GdipClosePathFigure(path);
        }
        path
    }

    pub fn fill_round_rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32, color: u32) {
        unsafe {
            let path = Self::build_round_rect_path(x, y, w, h, r);
            if path.is_null() {
                return;
            }
            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(color, &mut brush) == 0 {
                GdipFillPath(self.g, brush, path);
                GdipDeleteBrush(brush);
            }
            GdipDeletePath(path);
        }
    }

    pub fn draw_round_rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32, color: u32, stroke: f32) {
        unsafe {
            let path = Self::build_round_rect_path(x, y, w, h, r);
            if path.is_null() {
                return;
            }
            let mut pen = std::ptr::null_mut();
            if GdipCreatePen1(color, stroke, UNIT_PIXEL, &mut pen) == 0 {
                GdipDrawPath(self.g, pen, path);
                GdipDeletePen(pen);
            }
            GdipDeletePath(path);
        }
    }

    pub fn fill_ellipse(&self, x: f32, y: f32, w: f32, h: f32, color: u32) {
        unsafe {
            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(color, &mut brush) == 0 {
                GdipFillEllipse(self.g, brush, x, y, w, h);
                GdipDeleteBrush(brush);
            }
        }
    }

    pub fn draw_ellipse(&self, x: f32, y: f32, w: f32, h: f32, color: u32, stroke: f32) {
        unsafe {
            let mut pen = std::ptr::null_mut();
            if GdipCreatePen1(color, stroke, UNIT_PIXEL, &mut pen) == 0 {
                GdipDrawEllipse(self.g, pen, x, y, w, h);
                GdipDeletePen(pen);
            }
        }
    }

    pub fn draw_arc(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        start_angle: f32,
        sweep_angle: f32,
        color: u32,
        stroke: f32,
    ) {
        unsafe {
            let mut pen = std::ptr::null_mut();
            if GdipCreatePen1(color, stroke, UNIT_PIXEL, &mut pen) == 0 {
                GdipSetPenStartCap(pen, LINE_CAP_ROUND);
                GdipSetPenEndCap(pen, LINE_CAP_ROUND);
                GdipDrawArc(self.g, pen, x, y, w, h, start_angle, sweep_angle);
                GdipDeletePen(pen);
            }
        }
    }

    pub fn draw_line(&self, x1: f32, y1: f32, x2: f32, y2: f32, color: u32, stroke: f32) {
        unsafe {
            let mut pen = std::ptr::null_mut();
            if GdipCreatePen1(color, stroke, UNIT_PIXEL, &mut pen) == 0 {
                GdipSetPenStartCap(pen, LINE_CAP_ROUND);
                GdipSetPenEndCap(pen, LINE_CAP_ROUND);
                GdipSetPenLineJoin(pen, LINE_JOIN_ROUND);
                GdipDrawLine(self.g, pen, x1, y1, x2, y2);
                GdipDeletePen(pen);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &self,
        text: &str,
        rect: RectF,
        size: f32,
        bold: bool,
        color: u32,
        rtl: bool,
        align: TextAlign,
        v_center: bool,
        wrap: bool,
    ) {
        if text.is_empty() {
            return;
        }
        let fam_name = to_wide("Segoe UI");
        let wide = to_wide(text);
        let len = (wide.len() - 1) as i32;
        unsafe {
            let mut family = std::ptr::null_mut();
            if GdipCreateFontFamilyFromName(fam_name.as_ptr(), std::ptr::null_mut(), &mut family) != 0 {
                return;
            }
            let mut font = std::ptr::null_mut();
            let style = if bold { 1 } else { 0 };
            if GdipCreateFont(family, size, style, UNIT_PIXEL, &mut font) == 0 {
                let mut fmt = std::ptr::null_mut();
                if GdipCreateStringFormat(0, 0, &mut fmt) == 0 {
                    let mut flags = 0;
                    if rtl {
                        flags |= STRING_FORMAT_FLAGS_RTL;
                    }
                    if !wrap {
                        flags |= STRING_FORMAT_FLAGS_NO_WRAP;
                    }
                    GdipSetStringFormatFlags(fmt, flags);
                    if !wrap {
                        GdipSetStringFormatTrimming(fmt, STRING_TRIMMING_ELLIPSIS_CHAR);
                    }
                    let a = match align {
                        TextAlign::Near => 0,
                        TextAlign::Center => 1,
                        TextAlign::Far => 2,
                    };
                    GdipSetStringFormatAlign(fmt, a);
                    GdipSetStringFormatLineAlign(fmt, if v_center { 1 } else { 0 });

                    let mut brush = std::ptr::null_mut();
                    if GdipCreateSolidFill(color, &mut brush) == 0 {
                        GdipDrawString(self.g, wide.as_ptr(), len, font, &rect, fmt, brush);
                        GdipDeleteBrush(brush);
                    }
                    GdipDeleteStringFormat(fmt);
                }
                GdipDeleteFont(font);
            }
            GdipDeleteFontFamily(family);
        }
    }

    pub fn measure_text_height(&self, text: &str, width: f32, size: f32, bold: bool, rtl: bool) -> f32 {
        if text.is_empty() {
            return size * 1.4;
        }
        let fam_name = to_wide("Segoe UI");
        let wide = to_wide(text);
        let len = (wide.len() - 1) as i32;
        let mut h = size * 1.4;
        unsafe {
            let mut family = std::ptr::null_mut();
            if GdipCreateFontFamilyFromName(fam_name.as_ptr(), std::ptr::null_mut(), &mut family) != 0 {
                return h;
            }
            let mut font = std::ptr::null_mut();
            let style = if bold { 1 } else { 0 };
            if GdipCreateFont(family, size, style, UNIT_PIXEL, &mut font) == 0 {
                let mut fmt = std::ptr::null_mut();
                if GdipCreateStringFormat(0, 0, &mut fmt) == 0 {
                    if rtl {
                        GdipSetStringFormatFlags(fmt, STRING_FORMAT_FLAGS_RTL);
                    }
                    let layout = RectF {
                        x: 0.0,
                        y: 0.0,
                        width: width.max(20.0),
                        height: 10000.0,
                    };
                    let mut bbox = RectF { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
                    if GdipMeasureString(
                        self.g,
                        wide.as_ptr(),
                        len,
                        font,
                        &layout,
                        fmt,
                        &mut bbox,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    ) == 0
                    {
                        h = bbox.height;
                    }
                    GdipDeleteStringFormat(fmt);
                }
                GdipDeleteFont(font);
            }
            GdipDeleteFontFamily(family);
        }
        h
    }

    /// Width of a single unwrapped line (used for word wrapping).
    pub fn measure_line_width(&self, text: &str, size: f32, bold: bool) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let fam_name = to_wide("Segoe UI");
        let wide = to_wide(text);
        let len = (wide.len() - 1) as i32;
        let mut w = 0.0f32;
        unsafe {
            let mut family = std::ptr::null_mut();
            if GdipCreateFontFamilyFromName(fam_name.as_ptr(), std::ptr::null_mut(), &mut family) != 0 {
                return w;
            }
            let mut font = std::ptr::null_mut();
            if GdipCreateFont(family, size, if bold { 1 } else { 0 }, UNIT_PIXEL, &mut font) == 0 {
                let mut fmt = std::ptr::null_mut();
                if GdipCreateStringFormat(0, 0, &mut fmt) == 0 {
                    GdipSetStringFormatFlags(fmt, STRING_FORMAT_FLAGS_NO_WRAP);
                    let layout = RectF { x: 0.0, y: 0.0, width: 1.0e6, height: 1.0e6 };
                    let mut bbox = RectF { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
                    if GdipMeasureString(
                        self.g,
                        wide.as_ptr(),
                        len,
                        font,
                        &layout,
                        fmt,
                        &mut bbox,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    ) == 0
                    {
                        w = bbox.width;
                    }
                    GdipDeleteStringFormat(fmt);
                }
                GdipDeleteFont(font);
            }
            GdipDeleteFontFamily(family);
        }
        w
    }

    pub fn draw_icon(&self, name: &str, cx: f32, cy: f32, size: f32, color: u32) {
        let s = size;
        let stroke = (s / 8.5).max(1.3);
        match name {
            "lock" | "unlock" => {
                let bw = s * 0.68;
                let bh = s * 0.46;
                let bx = cx - bw / 2.0;
                let by = cy - bh * 0.1;
                self.draw_round_rect(bx, by, bw, bh, 2.0, color, stroke);
                let aw = s * 0.42;
                let ah = s * 0.42;
                let ax = cx - aw / 2.0;
                let ay = by - ah * 0.55;
                if name == "lock" {
                    self.draw_arc(ax, ay, aw, ah, 180.0, 180.0, color, stroke);
                    self.draw_line(ax, ay + ah / 2.0, ax, by, color, stroke);
                    self.draw_line(ax + aw, ay + ah / 2.0, ax + aw, by, color, stroke);
                } else {
                    self.draw_arc(ax, ay - 1.5, aw, ah, 180.0, 150.0, color, stroke);
                    self.draw_line(ax, ay - 1.5 + ah / 2.0, ax, by, color, stroke);
                }
            }
            "sun" => {
                let r = s * 0.21;
                self.draw_ellipse(cx - r, cy - r, r * 2.0, r * 2.0, color, stroke);
                for i in 0..8 {
                    let ang = (i as f32) * std::f32::consts::FRAC_PI_4;
                    let r1 = s * 0.34;
                    let r2 = s * 0.47;
                    self.draw_line(
                        cx + r1 * ang.cos(),
                        cy + r1 * ang.sin(),
                        cx + r2 * ang.cos(),
                        cy + r2 * ang.sin(),
                        color,
                        stroke,
                    );
                }
            }
            "moon" => {
                let r = s * 0.38;
                self.draw_arc(cx - r, cy - r, r * 2.0, r * 2.0, 35.0, 250.0, color, stroke);
                self.draw_arc(
                    cx - r * 0.45,
                    cy - r * 1.15,
                    r * 1.5,
                    r * 1.5,
                    125.0,
                    125.0,
                    color,
                    stroke,
                );
            }
            "gear" => {
                let r_in = s * 0.16;
                let r_out = s * 0.31;
                self.draw_ellipse(cx - r_in, cy - r_in, r_in * 2.0, r_in * 2.0, color, stroke);
                self.draw_ellipse(cx - r_out, cy - r_out, r_out * 2.0, r_out * 2.0, color, stroke);
                for i in 0..8 {
                    let ang = (i as f32) * std::f32::consts::FRAC_PI_4;
                    self.draw_line(
                        cx + r_out * ang.cos(),
                        cy + r_out * ang.sin(),
                        cx + (s * 0.44) * ang.cos(),
                        cy + (s * 0.44) * ang.sin(),
                        color,
                        stroke * 1.15,
                    );
                }
            }
            "copy" => {
                let b = s * 0.48;
                self.draw_round_rect(
                    cx - s * 0.12,
                    cy - s * 0.12,
                    b,
                    b,
                    1.8,
                    color,
                    stroke,
                );
                self.draw_line(cx - s * 0.36, cy + s * 0.16, cx - s * 0.36, cy - s * 0.36, color, stroke);
                self.draw_line(cx - s * 0.36, cy - s * 0.36, cx + s * 0.16, cy - s * 0.36, color, stroke);
            }
            "clipboard" => {
                let w = s * 0.62;
                let h = s * 0.72;
                self.draw_round_rect(cx - w / 2.0, cy - h / 2.0 + 1.0, w, h, 2.0, color, stroke);
                self.draw_round_rect(cx - s * 0.16, cy - h / 2.0 - 1.5, s * 0.32, s * 0.16, 1.0, color, stroke);
            }
            "check" => {
                self.draw_line(
                    cx - s * 0.32,
                    cy + s * 0.02,
                    cx - s * 0.06,
                    cy + s * 0.28,
                    color,
                    stroke * 1.2,
                );
                self.draw_line(
                    cx - s * 0.06,
                    cy + s * 0.28,
                    cx + s * 0.34,
                    cy - s * 0.24,
                    color,
                    stroke * 1.2,
                );
            }
            "close" => {
                let d = s * 0.28;
                self.draw_line(cx - d, cy - d, cx + d, cy + d, color, stroke * 1.1);
                self.draw_line(cx - d, cy + d, cx + d, cy - d, color, stroke * 1.1);
            }
            "grip" => {
                for (dx, dy) in [
                    (0.32, -0.32),
                    (0.0, 0.0),
                    (0.32, 0.0),
                    (-0.32, 0.32),
                    (0.0, 0.32),
                    (0.32, 0.32),
                ] {
                    self.fill_ellipse(cx + dx * s - 1.1, cy + dy * s - 1.1, 2.2, 2.2, color);
                }
            }
            "eye" | "eye_off" => {
                let w = s * 0.78;
                let h = s * 0.5;
                self.draw_arc(cx - w / 2.0, cy - h, w, h * 2.0, 200.0, 140.0, color, stroke * 0.9);
                self.draw_arc(cx - w / 2.0, cy - h, w, h * 2.0, 20.0, 140.0, color, stroke * 0.9);
                self.fill_ellipse(cx - s * 0.11, cy - s * 0.11, s * 0.22, s * 0.22, color);
                if name == "eye_off" {
                    self.draw_line(
                        cx - s * 0.38,
                        cy + s * 0.34,
                        cx + s * 0.38,
                        cy - s * 0.34,
                        color,
                        stroke,
                    );
                }
            }
            "key" => {
                let r = s * 0.17;
                self.draw_ellipse(cx - s * 0.34, cy - r, r * 2.0, r * 2.0, color, stroke);
                self.draw_line(cx - s * 0.14, cy, cx + s * 0.38, cy, color, stroke);
                self.draw_line(cx + s * 0.22, cy, cx + s * 0.22, cy + s * 0.2, color, stroke);
                self.draw_line(cx + s * 0.38, cy, cx + s * 0.38, cy + s * 0.2, color, stroke);
            }
            "plus" => {
                self.draw_line(cx, cy - s * 0.32, cx, cy + s * 0.32, color, stroke * 1.1);
                self.draw_line(cx - s * 0.32, cy, cx + s * 0.32, cy, color, stroke * 1.1);
            }
            "trash" => {
                let w = s * 0.5;
                self.draw_line(cx - s * 0.4, cy - s * 0.28, cx + s * 0.4, cy - s * 0.28, color, stroke);
                self.draw_line(cx - s * 0.14, cy - s * 0.28, cx - s * 0.14, cy - s * 0.4, color, stroke * 0.9);
                self.draw_line(cx + s * 0.14, cy - s * 0.28, cx + s * 0.14, cy - s * 0.4, color, stroke * 0.9);
                self.draw_line(cx - s * 0.14, cy - s * 0.4, cx + s * 0.14, cy - s * 0.4, color, stroke * 0.9);
                self.draw_round_rect(cx - w / 2.0, cy - s * 0.22, w, s * 0.6, 1.8, color, stroke * 0.9);
            }
            "refresh" => {
                let r = s * 0.34;
                self.draw_arc(cx - r, cy - r, r * 2.0, r * 2.0, 40.0, 250.0, color, stroke);
                self.draw_line(cx + r * 0.72, cy - r * 0.72, cx + r * 1.05, cy - r * 0.25, color, stroke);
                self.draw_line(cx + r * 0.72, cy - r * 0.72, cx + r * 0.2, cy - r * 0.95, color, stroke);
            }
            _ => {}
        }
    }

    pub fn draw_toggle_switch(&self, x: f32, y: f32, on: bool, t: &Theme) {
        let w = 32.0;
        let h = 16.0;
        let bg = if on { t.accent } else { t.switch_off };
        self.fill_round_rect(x, y, w, h, h / 2.0, bg);
        let kx = if on { x + w - 14.0 } else { x + 2.0 };
        self.fill_ellipse(kx, y + 2.0, 12.0, 12.0, t.knob);
    }

    pub fn draw_spinner(&self, cx: f32, cy: f32, state: SpinnerState, angle: f32, t: &Theme) {
        match state {
            SpinnerState::Idle => {}
            SpinnerState::Loading => {
                let r = 5.5;
                self.draw_arc(cx - r, cy - r, r * 2.0, r * 2.0, angle, 240.0, t.accent, 1.8);
            }
            SpinnerState::Ok => {
                self.draw_icon("check", cx, cy, 11.0, t.accent_green);
            }
            SpinnerState::Err => {
                self.draw_icon("close", cx, cy, 9.0, t.accent_red);
            }
        }
    }
}

impl Drop for GpCanvas {
    fn drop(&mut self) {
        if !self.g.is_null() {
            unsafe {
                GdipDeleteGraphics(self.g);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SpinnerState {
    Idle,
    Loading,
    Ok,
    Err,
}
