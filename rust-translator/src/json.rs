use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JsonValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            JsonValue::Number(n) => Some(*n as i64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<JsonValue>> {
        match self {
            JsonValue::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        match self {
            JsonValue::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut BTreeMap<String, JsonValue>> {
        match self {
            JsonValue::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        self.as_object()?.get(key)
    }

    pub fn to_compact_string(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out, None, 0);
        out
    }

    pub fn to_pretty_string(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out, Some(2), 0);
        out
    }

    fn write_to(&self, out: &mut String, indent: Option<usize>, depth: usize) {
        match self {
            JsonValue::Null => out.push_str("null"),
            JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            JsonValue::Number(n) => {
                if n.is_finite() {
                    if n.fract() == 0.0 && *n >= (i64::MIN as f64) && *n <= (i64::MAX as f64) {
                        out.push_str(&format!("{}", *n as i64));
                    } else {
                        out.push_str(&format!("{}", n));
                    }
                } else {
                    out.push_str("null");
                }
            }
            JsonValue::String(s) => write_escaped_str(s, out),
            JsonValue::Array(arr) => {
                if arr.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                for (i, item) in arr.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    if let Some(ind) = indent {
                        out.push('\n');
                        for _ in 0..(depth + 1) * ind {
                            out.push(' ');
                        }
                    }
                    item.write_to(out, indent, depth + 1);
                }
                if let Some(ind) = indent {
                    out.push('\n');
                    for _ in 0..depth * ind {
                        out.push(' ');
                    }
                }
                out.push(']');
            }
            JsonValue::Object(map) => {
                if map.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    if let Some(ind) = indent {
                        out.push('\n');
                        for _ in 0..(depth + 1) * ind {
                            out.push(' ');
                        }
                    }
                    write_escaped_str(k, out);
                    out.push(':');
                    if indent.is_some() {
                        out.push(' ');
                    }
                    v.write_to(out, indent, depth + 1);
                }
                if let Some(ind) = indent {
                    out.push('\n');
                    for _ in 0..depth * ind {
                        out.push(' ');
                    }
                }
                out.push('}');
            }
        }
    }
}

fn write_escaped_str(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[derive(Debug)]
pub struct JsonError(pub String);

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSON error: {}", self.0)
    }
}

pub fn parse_json(input: &str) -> Result<JsonValue, JsonError> {
    let bytes = input.as_bytes();
    let mut parser = Parser { bytes, pos: 0 };
    parser.skip_ws();
    let val = parser.parse_value()?;
    parser.skip_ws();
    if parser.pos < parser.bytes.len() {
        return Err(JsonError(format!("unexpected trailing bytes at {}", parser.pos)));
    }
    Ok(val)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, JsonError> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => {
                self.expect_bytes(b"null")?;
                Ok(JsonValue::Null)
            }
            Some(b't') => {
                self.expect_bytes(b"true")?;
                Ok(JsonValue::Bool(true))
            }
            Some(b'f') => {
                self.expect_bytes(b"false")?;
                Ok(JsonValue::Bool(false))
            }
            Some(b'"') => Ok(JsonValue::String(self.parse_string()?)),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(c) => Err(JsonError(format!("unexpected char '{}' at {}", c as char, self.pos))),
            None => Err(JsonError("unexpected EOF".into())),
        }
    }

    fn expect_bytes(&mut self, expected: &[u8]) -> Result<(), JsonError> {
        if self.bytes[self.pos..].starts_with(expected) {
            self.pos += expected.len();
            Ok(())
        } else {
            Err(JsonError(format!("expected {:?} at {}", expected, self.pos)))
        }
    }

    fn parse_string(&mut self) -> Result<String, JsonError> {
        if self.next() != Some(b'"') {
            return Err(JsonError("expected '\"'".into()));
        }
        let mut out = String::new();
        while let Some(b) = self.next() {
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = self.next().ok_or_else(|| JsonError("EOF in escape".into()))?;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\x08'),
                        b'f' => out.push('\x0c'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let code = self.parse_hex4()?;
                            if (0xD800..=0xDBFF).contains(&code) {
                                // UTF-16 surrogate pair
                                if self.bytes.get(self.pos..self.pos + 2) == Some(b"\\u") {
                                    self.pos += 2;
                                    let low = self.parse_hex4()?;
                                    if (0xDC00..=0xDFFF).contains(&low) {
                                        let combined = 0x10000
                                            + (((code as u32 - 0xD800) << 10) | (low as u32 - 0xDC00));
                                        if let Some(ch) = char::from_u32(combined) {
                                            out.push(ch);
                                            continue;
                                        }
                                    }
                                }
                                out.push(char::REPLACEMENT_CHARACTER);
                            } else if let Some(ch) = char::from_u32(code as u32) {
                                out.push(ch);
                            } else {
                                out.push(char::REPLACEMENT_CHARACTER);
                            }
                        }
                        other => return Err(JsonError(format!("bad escape \\{}", other as char))),
                    }
                }
                c if c < 0x80 => out.push(c as char),
                first => {
                    let start = self.pos - 1;
                    let width = if first & 0xE0 == 0xC0 {
                        2
                    } else if first & 0xF0 == 0xE0 {
                        3
                    } else if first & 0xF8 == 0xF0 {
                        4
                    } else {
                        return Err(JsonError("invalid UTF-8 lead byte".into()));
                    };
                    if start + width > self.bytes.len() {
                        return Err(JsonError("truncated UTF-8".into()));
                    }
                    let s = std::str::from_utf8(&self.bytes[start..start + width])
                        .map_err(|_| JsonError("invalid UTF-8".into()))?;
                    out.push_str(s);
                    self.pos = start + width;
                }
            }
        }
        Err(JsonError("unterminated string".into()))
    }

    fn parse_hex4(&mut self) -> Result<u16, JsonError> {
        if self.pos + 4 > self.bytes.len() {
            return Err(JsonError("truncated \\u escape".into()));
        }
        let hex_str = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4])
            .map_err(|_| JsonError("invalid hex".into()))?;
        self.pos += 4;
        u16::from_str_radix(hex_str, 16).map_err(|_| JsonError("invalid hex digits".into()))
    }

    fn parse_number(&mut self) -> Result<JsonValue, JsonError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while let Some(b'0'..=b'9') = self.peek() {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while let Some(b'0'..=b'9') = self.peek() {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            while let Some(b'0'..=b'9') = self.peek() {
                self.pos += 1;
            }
        }
        let s = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| JsonError("invalid number".into()))?;
        let n: f64 = s.parse().map_err(|_| JsonError(format!("bad number: {}", s)))?;
        Ok(JsonValue::Number(n))
    }

    fn parse_array(&mut self) -> Result<JsonValue, JsonError> {
        self.next(); // '['
        let mut arr = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.next();
            return Ok(JsonValue::Array(arr));
        }
        loop {
            arr.push(self.parse_value()?);
            self.skip_ws();
            match self.next() {
                Some(b',') => continue,
                Some(b']') => return Ok(JsonValue::Array(arr)),
                _ => return Err(JsonError("expected ',' or ']'".into())),
            }
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, JsonError> {
        self.next(); // '{'
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.next();
            return Ok(JsonValue::Object(map));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            if self.next() != Some(b':') {
                return Err(JsonError("expected ':'".into()));
            }
            let val = self.parse_value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.next() {
                Some(b',') => continue,
                Some(b'}') => return Ok(JsonValue::Object(map)),
                _ => return Err(JsonError("expected ',' or '}'".into())),
            }
        }
    }
}
