//! Millimetres or inches (#519): one setting for every length the editor shows and accepts.
//! Values stay in millimetres everywhere (the model, the commands, `.osd`); this module only
//! formats them for display and parses what the user types. A field accepts a bare number in
//! the chosen unit, or a number with its own unit: `2"`, `2 in`, `1 1/2"`, `50.8 mm`, `5 cm`.
//! Stitch settings (spacing, stitch length, compensation) stay in mm in both modes and do not
//! use this module.
//!
//! The setting is the Design Database's "Select System Unit" (PE-DESIGN's name), so both modes
//! share it.

pub use openstitch_library::catalog::Units;

pub const MM_PER_INCH: f64 = 25.4;

/// The preference key: `mm` or `inch`.
pub fn key(u: Units) -> &'static str {
    match u {
        Units::Mm => "mm",
        Units::Inch => "inch",
    }
}

pub fn from_key(k: &str) -> Option<Units> {
    match k {
        "mm" => Some(Units::Mm),
        "inch" | "in" => Some(Units::Inch),
        _ => None,
    }
}

/// The unit shown after a number: " mm" or `"`.
pub fn suffix(u: Units) -> &'static str {
    match u {
        Units::Mm => " mm",
        Units::Inch => "\"",
    }
}

/// Decimals shown: 0.1 mm, or 0.01 inch.
pub fn decimals(u: Units) -> usize {
    match u {
        Units::Mm => 1,
        Units::Inch => 2,
    }
}

/// `mm` in the display unit.
pub fn to_unit(u: Units, mm: f64) -> f64 {
    match u {
        Units::Mm => mm,
        Units::Inch => mm / MM_PER_INCH,
    }
}

/// A display-unit value back in mm.
pub fn from_unit(u: Units, v: f64) -> f64 {
    match u {
        Units::Mm => v,
        Units::Inch => v * MM_PER_INCH,
    }
}

/// The number alone, rounded for display: `12.5` or `0.49`.
pub fn number(u: Units, mm: f64) -> String {
    let v = to_unit(u, mm);
    let d = decimals(u);
    let s = format!("{v:.d$}");
    // "-0.0" reads oddly; a value that rounds to zero shows as zero.
    if s.trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        format!("{:.d$}", 0.0)
    } else {
        s
    }
}

/// A length with its unit: `12.5 mm` or `0.49"`.
pub fn length(u: Units, mm: f64) -> String {
    format!("{}{}", number(u, mm), suffix(u))
}

/// Width × height with the unit once: `130.0 × 180.0 mm` or `5.12 × 7.09"`.
pub fn size(u: Units, w_mm: f64, h_mm: f64) -> String {
    format!("{} × {}{}", number(u, w_mm), number(u, h_mm), suffix(u))
}

/// A hoop's size in both units, the chosen one first: `130 × 180 mm (5.12 × 7.09")`.
/// Hoops are sold by both, so both are always shown.
pub fn hoop_size(u: Units, w_mm: f64, h_mm: f64) -> String {
    let mm = format!("{} × {} mm", trim(w_mm, 1), trim(h_mm, 1));
    let inch = format!(
        "{} × {}\"",
        trim(w_mm / MM_PER_INCH, 2),
        trim(h_mm / MM_PER_INCH, 2)
    );
    match u {
        Units::Mm => format!("{mm} ({inch})"),
        Units::Inch => format!("{inch} ({mm})"),
    }
}

/// `v` with at most `d` decimals and no trailing zeros.
fn trim(v: f64, d: usize) -> String {
    let s = format!("{v:.d$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// Parses a typed length to mm. A bare number is in `u`; a unit typed with it wins
/// (`"`, `in`, `inch`, `inches`, `mm`, `cm`). Inches also take fractions: `1/2"`, `1 1/2"`,
/// `1-1/2"`. Returns `None` for anything else or a non-finite result.
pub fn parse(text: &str, u: Units) -> Option<f64> {
    let t = text.trim().to_ascii_lowercase();
    let units: [(&str, Units, f64); 7] = [
        ("inches", Units::Inch, 1.0),
        ("inch", Units::Inch, 1.0),
        ("in", Units::Inch, 1.0),
        ("\"", Units::Inch, 1.0),
        ("″", Units::Inch, 1.0),
        ("mm", Units::Mm, 1.0),
        ("cm", Units::Mm, 10.0),
    ];
    let (body, unit, factor) = units
        .iter()
        .find_map(|&(s, unit, f)| t.strip_suffix(s).map(|b| (b.trim(), unit, f)))
        .unwrap_or((t.as_str(), u, 1.0));
    let v = number_or_fraction(body)? * factor;
    let mm = from_unit(unit, v);
    mm.is_finite().then_some(mm)
}

/// `12.5`, `-3`, `1/2`, `1 1/2`, `1-1/2` (and `-1 1/2`).
fn number_or_fraction(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(v) = s.parse::<f64>() {
        return Some(v);
    }
    let (sign, s) = match s.strip_prefix('-') {
        Some(rest) => (-1.0, rest.trim_start()),
        None => (1.0, s),
    };
    let fraction = |f: &str| -> Option<f64> {
        let (a, b) = f.split_once('/')?;
        let (a, b) = (a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?);
        (b != 0.0 && a >= 0.0).then(|| a / b)
    };
    let whole_and_fraction = s
        .split_once(' ')
        .or_else(|| s.split_once('-'))
        .and_then(|(w, f)| {
            let w = w.trim().parse::<f64>().ok().filter(|w| *w >= 0.0)?;
            Some(w + fraction(f)?)
        });
    whole_and_fraction.or_else(|| fraction(s)).map(|v| sign * v)
}

/// Grid spacings offered in inch mode: 1/8", 1/4", 1/2", 1" (in mm).
pub const GRID_INCHES_MM: [f64; 4] = [3.175, 6.35, 12.7, 25.4];

/// The grid presets for the chosen unit (mm values).
pub fn grid_spacings(u: Units) -> &'static [f64] {
    match u {
        Units::Mm => &super::workspace::GridSettings::SPACINGS_MM,
        Units::Inch => &GRID_INCHES_MM,
    }
}

/// A grid spacing's menu label.
pub fn grid_label(mm: f64) -> String {
    let inches = mm / MM_PER_INCH;
    match [0.125, 0.25, 0.5, 1.0]
        .iter()
        .position(|f| (inches - f).abs() < 1e-9)
    {
        Some(0) => "1/8 inch".into(),
        Some(1) => "1/4 inch".into(),
        Some(2) => "1/2 inch".into(),
        Some(_) => "1 inch".into(),
        None => format!("{mm} mm"),
    }
}

/// Ruler steps in inches: (step in inches, minor ticks per labelled tick). Fractions of an
/// inch tick at 1/16, 1/8, 1/4 and 1/2 with whole inches labelled; further out, whole inches.
const INCH_STEPS: [(f64, i64); 9] = [
    (0.0625, 16),
    (0.125, 8),
    (0.25, 4),
    (0.5, 2),
    (1.0, 1),
    (2.0, 5),
    (5.0, 2),
    (10.0, 5),
    (50.0, 2),
];

/// Ruler ticks along one axis between model coordinates `from` and `to` (0.1 mm) in the chosen
/// unit. Millimetres keep [`super::workspace::ruler_ticks`]; inches tick at 1/8, 1/4, 1/2 and 1
/// with whole inches labelled.
pub fn ruler_ticks(
    u: Units,
    from: f64,
    to: f64,
    scale: f64,
    min_points: f64,
) -> Vec<super::workspace::Tick> {
    if u == Units::Mm {
        return super::workspace::ruler_ticks(from, to, scale, min_points);
    }
    if !(scale > 0.0 && from.is_finite() && to.is_finite() && to > from) {
        return Vec::new();
    }
    let unit = MM_PER_INCH * 10.0;
    let Some(&(step_in, major_every)) = INCH_STEPS
        .iter()
        .find(|&&(s, _)| s * unit * scale >= min_points)
    else {
        return Vec::new();
    };
    let step = step_in * unit;
    let first = (from / step).ceil() as i64;
    let last = (to / step).floor() as i64;
    if last - first > 10_000 {
        return Vec::new();
    }
    (first..=last)
        .map(|k| {
            let at = k as f64 * step;
            let major = k.rem_euclid(major_every) == 0;
            super::workspace::Tick {
                at,
                major,
                label: major.then(|| trim(k as f64 * step_in, 2)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_in_both_units() {
        assert_eq!(length(Units::Mm, 50.8), "50.8 mm");
        assert_eq!(length(Units::Inch, 50.8), "2.00\"");
        assert_eq!(size(Units::Inch, 130.0, 180.0), "5.12 × 7.09\"");
        assert_eq!(size(Units::Mm, 130.0, 180.0), "130.0 × 180.0 mm");
        assert_eq!(number(Units::Mm, -0.01), "0.0");
        assert_eq!(
            hoop_size(Units::Mm, 130.0, 180.0),
            "130 × 180 mm (5.12 × 7.09\")"
        );
        assert_eq!(
            hoop_size(Units::Inch, 101.6, 101.6),
            "4 × 4\" (101.6 × 101.6 mm)"
        );
    }

    #[test]
    fn parses_suffixes_fractions_and_bare_numbers() {
        // The acceptance check: `2"` and `50.8 mm` give the same length in either mode.
        for u in [Units::Mm, Units::Inch] {
            assert_eq!(parse("2\"", u), Some(50.8));
            assert_eq!(parse("50.8 mm", u), Some(50.8));
            assert_eq!(parse("2 in", u), Some(50.8));
            assert_eq!(parse("2 Inches", u), Some(50.8));
            assert_eq!(parse("5.08cm", u), Some(50.8));
            for t in ["1 1/2\"", "1-1/2 in", "1.5in"] {
                assert!((parse(t, u).expect(t) - 38.1).abs() < 1e-9, "{t}");
            }
            assert_eq!(parse("1/4\"", u), Some(6.35));
            assert_eq!(parse("-1/2\"", u), Some(-12.7));
        }
        assert_eq!(parse("2", Units::Inch), Some(50.8));
        assert_eq!(parse("2", Units::Mm), Some(2.0));
        assert_eq!(parse("-3.5", Units::Mm), Some(-3.5));
        for bad in ["", "abc", "1/0\"", "2 ft", "mm", "1 /", "inf", "NaN mm"] {
            assert_eq!(parse(bad, Units::Mm), None, "{bad}");
        }
    }

    #[test]
    fn display_round_trip_keeps_the_typed_value_exact() {
        // A size typed in inches is stored as the exact mm value; only display rounds.
        for typed in ["4", "5", "7", "0.5", "1 3/8"] {
            let mm = parse(typed, Units::Inch).expect("parses");
            let back = parse(&number(Units::Inch, mm), Units::Inch).expect("parses");
            assert!((back - mm).abs() < 0.13, "{typed}: {mm} vs {back}");
        }
        assert_eq!(parse("4", Units::Inch), Some(4.0 * 25.4));
        assert_eq!(from_unit(Units::Inch, to_unit(Units::Inch, 130.0)), 130.0);
    }

    #[test]
    fn sizes_typed_in_inches_save_the_same_osd_as_millimetres() {
        use openstitch_engine::Session;
        use serde_json::json;
        let dir = openstitch_testutil::TempDir::new("units-osd");
        let save = |w: f64, x: f64, name: &str| {
            let mut s = Session::new(dir.path().to_path_buf());
            s.execute("machine.set", Some(&json!({ "width": w, "height": 150 })))
                .expect("page");
            s.execute(
                "shape.rectangle",
                Some(&json!({ "x": x, "y": 10, "width": 20, "height": 20 })),
            )
            .expect("rectangle");
            let path = dir.path().join(name);
            s.execute("project.save", Some(&json!({ "path": path })))
                .expect("save");
            std::fs::read(path).expect("read")
        };
        let inch = save(
            parse("4", Units::Inch).expect("4 in"),
            parse("1 1/4", Units::Inch).expect("1 1/4 in"),
            "inch.osd",
        );
        let mm = save(
            parse("4\"", Units::Mm).expect("4\""),
            parse("1.25 in", Units::Mm).expect("1.25 in"),
            "mm.osd",
        );
        assert_eq!(inch, mm, "the unit only changes what is shown");
    }

    #[test]
    fn inch_rulers_tick_on_fractions_and_label_whole_inches() {
        // 1 point per 0.1 mm: 1/8" = 3.175 mm = 31.75 points.
        let t = ruler_ticks(Units::Inch, 0.0, 1016.0, 1.0, 6.0);
        assert!((t[1].at - 0.0625 * 254.0).abs() < 1e-9, "1/16\" ticks fit");
        let labels: Vec<_> = t.iter().filter_map(|t| t.label.clone()).collect();
        assert_eq!(labels, ["0", "1", "2", "3", "4"]);
        // Zoomed out (1 inch is 2.5 points): ticks every 5 inches, labelled every 10.
        let t = ruler_ticks(Units::Inch, 0.0, 10_000.0, 0.01, 6.0);
        assert!((t[1].at - 5.0 * 254.0).abs() < 1e-9);
        assert_eq!(t[1].label, None);
        assert!(t.iter().any(|t| t.label.as_deref() == Some("10")));
        assert_eq!(
            ruler_ticks(Units::Mm, 0.0, 500.0, 1.0, 6.0),
            crate::editor::workspace::ruler_ticks(0.0, 500.0, 1.0, 6.0)
        );
        assert!(ruler_ticks(Units::Inch, 10.0, 0.0, 1.0, 6.0).is_empty());
    }

    #[test]
    fn grid_presets_follow_the_unit() {
        assert_eq!(grid_spacings(Units::Inch), &[3.175, 6.35, 12.7, 25.4]);
        assert!(grid_spacings(Units::Mm).contains(&10.0));
        let labels: Vec<_> = GRID_INCHES_MM.iter().map(|&s| grid_label(s)).collect();
        assert_eq!(labels, ["1/8 inch", "1/4 inch", "1/2 inch", "1 inch"]);
        assert_eq!(grid_label(5.0), "5 mm");
        assert_eq!(from_key(key(Units::Inch)), Some(Units::Inch));
        assert_eq!(from_key("feet"), None);
    }
}
