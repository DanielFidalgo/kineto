use serde::{Deserialize, Serialize};

/// Validated "#RRGGBB" or "#RRGGBBAA" string (validation in Task 3);
/// stored as the string so canonical serialization round-trips bytes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color(pub String);

impl Color {
    pub fn parse_ok(s: &str) -> bool {
        (s.len() == 7 || s.len() == 9)
            && s.starts_with('#')
            && s[1..].chars().all(|c| c.is_ascii_hexdigit())
    }
    /// (r, g, b, a) 0-255; a = 255 for 6-digit form.
    ///
    /// Callers must have validated `self` (load-time validation in
    /// `validate.rs` is the real guard); this only debug-asserts.
    pub fn rgba8(&self) -> (u8, u8, u8, u8) {
        debug_assert!(
            Color::parse_ok(&self.0),
            "Color::rgba8 called on unvalidated color {:?}",
            self.0
        );
        let h = &self.0[1..];
        let b = |i| u8::from_str_radix(&h[i..i + 2], 16).unwrap();
        (b(0), b(2), b(4), if h.len() == 8 { b(6) } else { 255 })
    }
    pub fn is_default_bg(&self) -> bool {
        self.0.eq_ignore_ascii_case("#000000")
    }
}
impl From<&str> for Color {
    fn from(s: &str) -> Self {
        Color(s.to_string())
    }
}
