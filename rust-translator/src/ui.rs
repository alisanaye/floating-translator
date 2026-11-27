//! Small shared widget layer: hit regions, themed buttons, toggle switches, text fields.
use crate::gdiplus::{GpCanvas, RectF, TextAlign};
use crate::theme::Theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Id {
    // main window
    Bar,
    Lock,
    Model,
    Key,
    Rtl,
    Copy,
    Theme,
    Gear,
    Close,
    Src,
    Out,
    Grip,
    Watch,
    // settings window
    SBar,
    SClose,
    SKey,
    SEye,
    SModel,
    STarget,
    SProxy,
    STimeout,
    SRetries,
    STest,
    SSave,
    SCancel,
    // keys window
    KBar,
    KClose,
    KNew,
    KEye,
    KAdd,
    KActivate,
    KDel,
    KRow(u32),
    KScope,
    KRefresh,
    KListRow(u32),
    // popup menu
    MenuRow(u32),
}

/// Hit regions collected while painting; used to route mouse input.
#[derive(Default)]
pub struct Hits {
    pub items: Vec<(Id, RectF)>,
}

impl Hits {
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn add(&mut self, id: Id, r: RectF) {
        self.items.push((id, r));
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<Id> {
        self.items
            .iter()
            .rev()
            .find(|(_, r)| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
            .map(|(id, _)| *id)
    }
}

pub fn rect(x: f32, y: f32, w: f32, h: f32) -> RectF {
    RectF { x, y, width: w, height: h }
}

pub struct Btn<'a> {
    pub id: Id,
    pub rect: RectF,
    pub icon: Option<&'a str>,
    pub text: Option<&'a str>,
    pub fg: u32,
    pub bg: Option<u32>,
    pub radius: f32,
    pub icon_size: f32,
    pub font: f32,
    pub bold: bool,
    pub angle: f32,
}

impl<'a> Btn<'a> {
    pub fn new(id: Id, rect: RectF, fg: u32) -> Self {
        Self {
            id,
            rect,
            icon: None,
            text: None,
            fg,
            bg: None,
            radius: 5.0,
            icon_size: 12.0,
            font: 13.0,
            bold: false,
            angle: 0.0,
        }
    }

    pub fn icon(mut self, name: &'a str, size: f32) -> Self {
        self.icon = Some(name);
        self.icon_size = size;
        self
    }

    pub fn text(mut self, text: &'a str, font: f32) -> Self {
        self.text = Some(text);
        self.font = font;
        self
    }

    pub fn bg(mut self, color: u32) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn radius(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }

    pub fn angle(mut self, a: f32) -> Self {
        self.angle = a;
        self
    }
}

/// Draws a button (optionally with a background) and registers its hit region.
/// Icon sits at the right edge, label to its left — same as the Python `VectorButton`.
pub fn button(c: &GpCanvas, hits: &mut Hits, b: &Btn) {
    let r = b.rect;
    if let Some(bg) = b.bg {
        if b.radius > 0.0 {
            c.fill_round_rect(r.x, r.y, r.width, r.height, b.radius, bg);
        } else {
            c.fill_rect(r.x, r.y, r.width, r.height, bg);
        }
    }
    let cy = r.y + r.height / 2.0;
    match (b.icon, b.text) {
        (Some(icon), Some(text)) => {
            let ix = r.x + r.width - 12.0;
            c.draw_icon(icon, ix, cy, b.icon_size, b.fg);
            let tr = rect(r.x + 2.0, r.y, r.width - 24.0, r.height);
            c.draw_text(text, tr, b.font, b.bold, b.fg, true, TextAlign::Far, true, false);
        }
        (Some(icon), None) => {
            c.draw_icon(icon, r.x + r.width / 2.0, cy, b.icon_size, b.fg);
        }
        (None, Some(text)) => {
            c.draw_text(text, r, b.font, b.bold, b.fg, true, TextAlign::Center, true, false);
        }
        (None, None) => {}
    }
    hits.add(b.id, r);
}

/// Left-aligned text button (used for RTL / model / key chips in the title bar).
pub fn chip(
    c: &GpCanvas,
    hits: &mut Hits,
    id: Id,
    r: RectF,
    text: &str,
    fg: u32,
    bg: Option<u32>,
    font: f32,
    bold: bool,
) {
    if let Some(bg) = bg {
        c.fill_round_rect(r.x, r.y, r.width, r.height, 4.0, bg);
    }
    c.draw_text(text, r, font, bold, fg, false, TextAlign::Near, true, false);
    hits.add(id, r);
}

pub struct Field {
    pub id: Id,
    pub text: String,
    pub caret: usize,
    pub masked: bool,
    pub max: usize,
    pub rect: RectF,
    pub font: f32,
}

impl Field {
    pub fn new(id: Id, text: String, masked: bool, max: usize) -> Self {
        let caret = text.len();
        Self { id, text, caret, masked, max, rect: rect(0.0, 0.0, 0.0, 0.0), font: 13.0 }
    }

    fn shown(&self) -> String {
        if self.masked {
            "\u{2022}".repeat(self.text.chars().count())
        } else {
            self.text.clone()
        }
    }

    pub fn draw(&self, c: &GpCanvas, t: &Theme) {
        let r = self.rect;
        c.fill_round_rect(r.x, r.y, r.width, r.height, 4.0, t.bg);
        c.draw_round_rect(r.x, r.y, r.width, r.height, 4.0, t.border, 1.0);
        let inner = rect(r.x + 6.0, r.y, r.width - 12.0, r.height);
        c.set_clip(inner.x, inner.y, inner.width, inner.height);
        let text = self.shown();
        // right-align the visible tail when the text is wider than the field
        let w = c.measure_line_width(&text, self.font, false);
        let dx = if w > inner.width { inner.x + inner.width - w } else { 0.0 };
        let draw_rect = rect(inner.x + dx, inner.y, w.max(inner.width), inner.height);
        let color = if self.text.is_empty() { t.fg_dim } else { t.fg };
        c.draw_text(&text, draw_rect, self.font, false, color, false, TextAlign::Near, true, false);
        c.reset_clip();
    }

    pub fn click(&mut self, c: &GpCanvas, x: f32) {
        let inner_x = self.rect.x + 6.0;
        let inner_w = self.rect.width - 12.0;
        let text = self.shown();
        let w = c.measure_line_width(&text, self.font, false);
        let dx = if w > inner_w { inner_w - w } else { 0.0 };
        let mut best = 0usize;
        let mut best_d = f32::MAX;
        for i in 0..=self.text.chars().count() {
            let prefix: String = text.chars().take(i).collect();
            let px = inner_x + dx + c.measure_line_width(&prefix, self.font, false);
            let d = (px - x).abs();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        self.caret = self.byte_of_char(best);
    }

    fn byte_of_char(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    fn char_of_byte(&self, byte: usize) -> usize {
        self.text[..byte.min(self.text.len())].chars().count()
    }

    pub fn caret_x(&self, c: &GpCanvas) -> f32 {
        let text = self.shown();
        let chars = self.char_of_byte(self.caret);
        let prefix: String = text.chars().take(chars).collect();
        let inner_x = self.rect.x + 6.0;
        let inner_w = self.rect.width - 12.0;
        let w = c.measure_line_width(&text, self.font, false);
        let dx = if w > inner_w { inner_w - w } else { 0.0 };
        inner_x + dx + c.measure_line_width(&prefix, self.font, false)
    }

    pub fn caret_y(&self) -> f32 {
        self.rect.y + 3.0
    }

    pub fn caret_h(&self) -> i32 {
        (self.rect.height - 6.0).max(8.0) as i32
    }

    /// ASCII only: every field is a key / language code / port / proxy URL.
    /// shortcut: no IME or RTL input in fields, add if a translated field is ever needed.
    pub fn on_char(&mut self, ch: char) {
        self.insert_char(ch);
    }

    /// Insert printable ASCII characters from pasted text at the caret, up to `self.max`.
    pub fn paste(&mut self, text: &str) -> bool {
        let mut changed = false;
        for ch in text.trim().chars() {
            if self.text.chars().count() >= self.max {
                break;
            }
            if self.insert_char(ch) {
                changed = true;
            }
        }
        changed
    }

    fn insert_char(&mut self, ch: char) -> bool {
        if !(' '..='~').contains(&ch) || self.text.chars().count() >= self.max {
            return false;
        }
        self.text.insert(self.caret, ch);
        self.caret += ch.len_utf8();
        true
    }

    pub fn on_key(&mut self, vk: i32) -> bool {
        const VK_BACK: i32 = 0x08;
        const VK_DELETE: i32 = 0x2E;
        const VK_LEFT: i32 = 0x25;
        const VK_RIGHT: i32 = 0x27;
        const VK_HOME: i32 = 0x24;
        const VK_END: i32 = 0x23;
        match vk {
            VK_BACK => {
                if let Some(prev) = self.text[..self.caret].chars().next_back() {
                    self.caret -= prev.len_utf8();
                    self.text.remove(self.caret);
                }
                true
            }
            VK_DELETE => {
                if self.caret < self.text.len() {
                    self.text.remove(self.caret);
                }
                true
            }
            VK_LEFT => {
                if let Some(prev) = self.text[..self.caret].chars().next_back() {
                    self.caret -= prev.len_utf8();
                }
                true
            }
            VK_RIGHT => {
                if self.caret < self.text.len() {
                    let ch = self.text[self.caret..].chars().next().unwrap();
                    self.caret += ch.len_utf8();
                }
                true
            }
            VK_HOME => {
                self.caret = 0;
                true
            }
            VK_END => {
                self.caret = self.text.len();
                true
            }
            _ => false,
        }
    }
}

/// Word-wrap into logical lines (bidi ordering is left to GDI+/Uniscribe at draw time).
pub fn wrap_lines(c: &GpCanvas, text: &str, width: f32, font: f32, bold: bool) -> Vec<String> {
    let space = c.measure_line_width(" ", font, bold);
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line: Vec<&str> = Vec::new();
        let mut used = 0.0f32;
        for word in para.split_whitespace() {
            let need = c.measure_line_width(word, font, bold);
            if !line.is_empty() && used + space + need > width {
                out.push(line.join(" "));
                line.clear();
                used = 0.0;
            }
            used += (if line.is_empty() { 0.0 } else { space }) + need;
            line.push(word);
        }
        out.push(line.join(" "));
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// First strong character decides the source line direction (same rule as app.py).
pub fn is_rtl_text(text: &str) -> bool {
    for ch in text.chars() {
        let c = ch as u32;
        // Hebrew, Arabic, Syriac, Thaana, NKo, Samaritan, Arabic supplement/extended
        if (0x0590..=0x08FF).contains(&c) || (0xFB1D..=0xFDFF).contains(&c) || (0xFE70..=0xFEFF).contains(&c) {
            return true;
        }
        if ch.is_ascii_alphabetic() {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_paste_inserts_ascii_and_respects_max() {
        let mut f = Field::new(Id::SKey, String::new(), true, 10);
        assert!(f.paste("  AIza123\r\n"));
        assert_eq!(f.text, "AIza123");
        assert_eq!(f.caret, 7);

        // Truncates at max (10) and ignores non-ASCII
        assert!(f.paste("456789"));
        assert_eq!(f.text, "AIza123456");
        assert_eq!(f.caret, 10);
        assert!(!f.paste("x"));
    }

    #[test]
    fn field_paste_inserts_at_caret() {
        let mut f = Field::new(Id::SKey, "ac".to_string(), false, 10);
        f.caret = 1;
        assert!(f.paste("b"));
        assert_eq!(f.text, "abc");
        assert_eq!(f.caret, 2);
    }
}
