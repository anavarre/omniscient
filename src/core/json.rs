//! A tiny allocation-light JSON object writer for the hot path (one object
//! per visible row on every change). `serde_json::Value` would build a map
//! and a String per key first; this appends straight into one buffer.

use std::fmt::Write;

pub struct Obj {
    buf: String,
    first: bool,
}

pub fn escape_into(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl Obj {
    pub fn new() -> Obj {
        Obj::with_capacity(256)
    }

    pub fn with_capacity(n: usize) -> Obj {
        let mut buf = String::with_capacity(n);
        buf.push('{');
        Obj { buf, first: true }
    }

    fn key(&mut self, k: &str) -> &mut String {
        if !self.first {
            self.buf.push(',');
        }
        self.first = false;
        escape_into(&mut self.buf, k);
        self.buf.push(':');
        &mut self.buf
    }

    pub fn str(mut self, k: &str, v: &str) -> Obj {
        let buf = self.key(k);
        escape_into(buf, v);
        self
    }

    pub fn opt_str(self, k: &str, v: Option<&str>) -> Obj {
        match v {
            Some(v) => self.str(k, v),
            None => self.raw(k, "null"),
        }
    }

    pub fn int(mut self, k: &str, v: i64) -> Obj {
        let _ = write!(self.key(k), "{v}");
        self
    }

    pub fn opt_int(self, k: &str, v: Option<i64>) -> Obj {
        match v {
            Some(v) => self.int(k, v),
            None => self.raw(k, "null"),
        }
    }

    pub fn bool(self, k: &str, v: bool) -> Obj {
        self.raw(k, if v { "true" } else { "false" })
    }

    /// `v` must already be valid JSON.
    pub fn raw(mut self, k: &str, v: &str) -> Obj {
        self.key(k).push_str(v);
        self
    }

    pub fn finish(mut self) -> String {
        self.buf.push('}');
        self.buf
    }
}

/// `[item, item]` from already-encoded items.
pub fn array(items: impl Iterator<Item = String>) -> String {
    let mut out = String::from("[");
    for (i, item) in items.enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&item);
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_valid_json() {
        let s = Obj::new()
            .str("a", "q\"uo\\te\n\u{1}")
            .int("n", -3)
            .opt_int("none", None)
            .bool("t", true)
            .raw("list", &array(["1".to_string(), "2".to_string()].into_iter()))
            .finish();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["a"], "q\"uo\\te\n\u{1}");
        assert_eq!(v["n"], -3);
        assert!(v["none"].is_null());
        assert_eq!(v["list"][1], 2);
    }
}
