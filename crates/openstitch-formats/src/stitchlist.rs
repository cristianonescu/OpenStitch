//! Stitch lists (#488): every record of a stitch stream as a CSV or TXT row, for checking a
//! file in a spreadsheet, teaching, or feeding other tools. Write only (and a parser for the
//! tests and for tools that read them back).
//!
//! Columns: `index, command, x_mm, y_mm, dx_mm, dy_mm, color, brand, code`: one row per record
//! in stream order (stitch, jump, trim, stop, colorChange, end); x and y absolute in mm to 0.01 mm as
//! the design holds them (+y down), dx and dy the move from the previous record (the first
//! from (0, 0)) between the rounded positions, so the sums of dx and dy are exactly the last
//! record's position; `color` is the colour
//! block (1 for the first), `brand` and `code` that block's thread. CSV separates with commas
//! and writes a header row; TXT aligns the same columns with spaces after a `#` header line.

use openstitch_core::{Command, StitchPattern};

pub const COLUMNS: [&str; 9] = [
    "index", "command", "x_mm", "y_mm", "dx_mm", "dy_mm", "color", "brand", "code",
];

/// The camelCase name of a command, as the engine's JSON uses it.
pub fn command_name(c: Command) -> &'static str {
    match c {
        Command::Stitch => "stitch",
        Command::Jump => "jump",
        Command::Trim => "trim",
        Command::Stop => "stop",
        Command::ColorChange => "colorChange",
        Command::End => "end",
    }
}

fn rows(pattern: &StitchPattern) -> Vec<[String; 9]> {
    let mut out = Vec::with_capacity(pattern.stitches.len());
    // Positions in hundredths of a millimetre, rounded once, so the deltas sum exactly.
    let hundredths = |v: f64| (v * 10.0).round() as i64;
    let mm = |v: i64| {
        format!(
            "{}{}.{:02}",
            if v < 0 { "-" } else { "" },
            v.abs() / 100,
            v.abs() % 100
        )
    };
    let (mut px, mut py) = (0_i64, 0_i64);
    let mut block = 0;
    let clean = |s: Option<&str>| s.unwrap_or("").replace([',', '\t', '\r', '\n', ' '], "_");
    for (i, s) in pattern.stitches.iter().enumerate() {
        let t = pattern.threads.get(block);
        out.push([
            i.to_string(),
            command_name(s.command).to_owned(),
            mm(hundredths(s.x)),
            mm(hundredths(s.y)),
            mm(hundredths(s.x) - px),
            mm(hundredths(s.y) - py),
            (block + 1).to_string(),
            clean(t.and_then(|t| t.brand.as_deref())),
            clean(t.and_then(|t| t.catalog_number.as_deref())),
        ]);
        (px, py) = (hundredths(s.x), hundredths(s.y));
        if s.command == Command::ColorChange {
            block += 1;
        }
    }
    out
}

pub fn write_csv(pattern: &StitchPattern) -> Vec<u8> {
    let mut s = COLUMNS.join(",");
    s.push('\n');
    for r in rows(pattern) {
        s.push_str(&r.join(","));
        s.push('\n');
    }
    s.into_bytes()
}

pub fn write_txt(pattern: &StitchPattern) -> Vec<u8> {
    let widths = [7, 12, 9, 9, 9, 9, 6, 0, 0];
    let line = |r: &[String]| {
        let mut l = String::new();
        for (v, w) in r.iter().zip(widths) {
            l.push_str(&format!("{v:<w$} "));
        }
        l.trim_end().to_owned()
    };
    let header: Vec<String> = COLUMNS.iter().map(|c| (*c).to_owned()).collect();
    let mut s = format!("# {}\n", line(&header));
    for r in rows(pattern) {
        s.push_str(&line(&r));
        s.push('\n');
    }
    s.into_bytes()
}

/// One parsed row: index, command name, x, y, dx, dy (mm), colour block.
pub type Row = (usize, String, f64, f64, f64, f64, usize);

/// Reads a stitch list written by [`write_csv`] or [`write_txt`]. Rows that do not parse are an
/// error with their line number.
pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("index") {
            continue;
        }
        let f: Vec<&str> = if line.contains(',') {
            line.split(',').map(str::trim).collect()
        } else {
            line.split_whitespace().collect()
        };
        let num = |i: usize| -> Result<f64, String> {
            f.get(i)
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| format!("line {}: column {}", n + 1, COLUMNS[i]))
        };
        out.push((
            num(0)? as usize,
            f.get(1)
                .ok_or(format!("line {}: command", n + 1))?
                .to_string(),
            num(2)?,
            num(3)?,
            num(4)?,
            num(5)?,
            num(6)? as usize,
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_core::Thread;

    fn sample() -> StitchPattern {
        let mut p = StitchPattern::new();
        let mut red = Thread::rgb(0xFF0000);
        red.brand = Some("Acme Rayon".into());
        red.catalog_number = Some("1,2".into());
        p.threads = vec![red, Thread::rgb(0x0000FF)];
        p.stitch_to(10.0, 20.0);
        p.stitch_to(35.0, -5.0);
        p.push(Command::Trim);
        p.jump_to(100.0, 100.0);
        p.push(Command::ColorChange);
        p.stitch_to(101.0, 99.0);
        p.push(Command::End);
        p
    }

    #[test]
    fn rows_sum_to_the_end_point_and_keep_every_record() {
        for bytes in [write_csv(&sample()), write_txt(&sample())] {
            let rows = parse(&String::from_utf8(bytes).expect("utf8")).expect("parse");
            assert_eq!(rows.len(), sample().stitches.len());
            let (sx, sy) = rows.iter().fold((0.0, 0.0), |a, r| (a.0 + r.4, a.1 + r.5));
            let last = rows.last().expect("rows");
            assert!((sx - last.2).abs() < 1e-6 && (sy - last.3).abs() < 1e-6);
            assert_eq!((last.2, last.3), (10.1, 9.9));
            let names: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
            assert_eq!(
                names,
                [
                    "stitch",
                    "stitch",
                    "trim",
                    "jump",
                    "colorChange",
                    "stitch",
                    "end"
                ]
            );
            assert_eq!(
                rows.iter().map(|r| r.6).collect::<Vec<_>>(),
                [1, 1, 1, 1, 1, 2, 2]
            );
        }
    }

    #[test]
    fn csv_has_a_header_and_no_stray_separators() {
        let csv = String::from_utf8(write_csv(&sample())).expect("utf8");
        let mut lines = csv.lines();
        assert_eq!(lines.next(), Some(COLUMNS.join(",").as_str()));
        assert_eq!(
            lines.next(),
            Some("0,stitch,1.00,2.00,1.00,2.00,1,Acme_Rayon,1_2")
        );
        assert!(csv.lines().all(|l| l.split(',').count() == 9));
        assert!(parse("index,command\n0,stitch,x").is_err());
    }
}
