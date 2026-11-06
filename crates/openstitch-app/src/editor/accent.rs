//! The accent colour (#559, beyond PE-DESIGN): the highlight used for the selection outline and
//! handles on the canvas, selected buttons, focus and links. `None` keeps each theme's own
//! colours (the look before #559). Text on an accent background stays readable: white text needs
//! a 4.5:1 contrast (WCAG AA), so a light custom colour is darkened for those backgrounds.
//!
//! Nothing here depends on egui.

/// The presets Options ▸ Accent colour offers: key, label and colour.
pub const PRESETS: &[(&str, &str, [u8; 3])] = &[
    ("blue", "Blue", [0x1e, 0x88, 0xe5]),
    ("orange", "Orange", [0xef, 0x6c, 0x00]),
    ("green", "Green", [0x2e, 0x9d, 0x4f]),
    ("magenta", "Magenta", [0xd8, 0x1b, 0x80]),
    ("purple", "Purple", [0x7e, 0x57, 0xc2]),
    ("teal", "Teal", [0x00, 0x89, 0x7b]),
];

/// The smallest contrast between text and its background (WCAG AA, normal text).
pub const MIN_CONTRAST: f64 = 4.5;

/// `#rrggbb`.
pub fn to_hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// A preset key, or `#rrggbb` / `rrggbb`.
pub fn parse(s: &str) -> Option<[u8; 3]> {
    let s = s.trim();
    if let Some((_, _, c)) = PRESETS.iter().find(|(k, _, _)| k.eq_ignore_ascii_case(s)) {
        return Some(*c);
    }
    let h = s.strip_prefix('#').unwrap_or(s);
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

/// The setting's value: the preset key when it is one, else `#rrggbb`; "default" for none.
pub fn key(c: Option<[u8; 3]>) -> String {
    match c {
        None => "default".into(),
        Some(c) => PRESETS
            .iter()
            .find(|p| p.2 == c)
            .map_or_else(|| to_hex(c), |p| p.0.to_string()),
    }
}

fn channel(v: u8) -> f64 {
    let c = f64::from(v) / 255.0;
    if c <= 0.039_28 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Relative luminance (WCAG 2).
pub fn luminance(c: [u8; 3]) -> f64 {
    0.2126 * channel(c[0]) + 0.7152 * channel(c[1]) + 0.0722 * channel(c[2])
}

/// Contrast ratio between two colours, 1–21.
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `c`, darkened just enough for white text on it to reach [`MIN_CONTRAST`].
pub fn for_white_text(c: [u8; 3]) -> [u8; 3] {
    let mut out = c;
    let mut f = 1.0;
    while contrast(out, [255; 3]) < MIN_CONTRAST && f > 0.0 {
        f -= 0.02;
        out = c.map(|v| (f64::from(v) * f).round() as u8);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_presets_and_hex() {
        assert_eq!(parse("Orange"), Some([0xef, 0x6c, 0x00]));
        assert_eq!(parse("#FF0080"), Some([255, 0, 128]));
        assert_eq!(parse("00ff00"), Some([0, 255, 0]));
        assert!(parse("#12345").is_none() && parse("red!").is_none());
        assert_eq!(key(None), "default");
        assert_eq!(key(parse("teal")), "teal");
        assert_eq!(key(Some([1, 2, 3])), "#010203");
    }

    #[test]
    fn white_text_on_the_accent_keeps_aa_contrast() {
        assert!((contrast([0; 3], [255; 3]) - 21.0).abs() < 1e-9);
        for c in PRESETS
            .iter()
            .map(|p| p.2)
            .chain([[255, 255, 0], [255; 3], [0x80, 0xc0, 0xff]])
        {
            let safe = for_white_text(c);
            assert!(contrast(safe, [255; 3]) >= MIN_CONTRAST, "{c:?} → {safe:?}");
        }
        // A colour that already passes is kept.
        assert_eq!(for_white_text([0x10, 0x30, 0x80]), [0x10, 0x30, 0x80]);
    }
}
