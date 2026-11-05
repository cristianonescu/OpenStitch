//! Export extras (#488) that `design.export` takes besides the machine formats: stitch lists
//! (`csv`, `txt`) and G-code (`gcode`), and two export-time arrangements for every format,
//! `repeat` (copies side by side) and `framePark` (where the frame waits at a stop). None of
//! them changes the design. File layouts: `openstitch_formats::{stitchlist, gcode}`.

use crate::commands::MM;
use crate::params::{CommandError, Params};
use openstitch_core::{Command, Stitch, StitchPattern};
use openstitch_design::Design;
use serde_json::Value;

/// Formats `design.export` writes that are not machine files.
pub const EXTRA_FORMATS: [&str; 3] = ["csv", "txt", "gcode"];

/// Most copies on one axis.
pub const MAX_REPEAT: i64 = 20;

/// The bytes of a stitch list or G-code export, or `None` for any other format.
pub fn encode(
    format: &str,
    pattern: &StitchPattern,
    p: &Params,
) -> Result<Option<Vec<u8>>, CommandError> {
    use openstitch_formats::{gcode, stitchlist};
    Ok(match format {
        "csv" => Some(stitchlist::write_csv(pattern)),
        "txt" => Some(stitchlist::write_txt(pattern)),
        "gcode" => {
            let mut o = gcode::Options::default();
            if let Some(v) = p.get("gcode") {
                let text = |key: &str| -> Result<Option<String>, CommandError> {
                    match v.get(key) {
                        None => Ok(None),
                        Some(Value::String(s)) if s.len() <= 100 && !s.contains(['\n', '\r']) => {
                            Ok(Some(s.clone()))
                        }
                        Some(_) => Err(p.invalid("gcode", format!("{key}: one line of text"))),
                    }
                };
                o.needle = text("needle")?.unwrap_or_default();
                o.trim = text("trim")?.unwrap_or_default();
                o.profile = text("profile")?.unwrap_or_default();
                if let Some(f) = v.get("feed") {
                    o.feed = f
                        .as_f64()
                        .filter(|f| (1.0..=100_000.0).contains(f))
                        .ok_or_else(|| p.invalid("gcode", "feed: 1–100000 mm/min"))?;
                }
            }
            Some(gcode::write(pattern, &o))
        }
        _ => None,
    })
}

/// The colour blocks of a pattern: the records between colour changes, without them and
/// without the end.
fn blocks(pattern: &StitchPattern) -> Vec<Vec<Stitch>> {
    let mut out = vec![Vec::new()];
    for s in &pattern.stitches {
        match s.command {
            Command::ColorChange => out.push(Vec::new()),
            Command::End => break,
            _ => out.last_mut().expect("one block").push(*s),
        }
    }
    out
}

/// `repeat: {columns, rows, gapMm}`: the design `columns` × `rows` times, `gapMm` apart
/// between the copies' needle points, the whole centred on the Design Page. Each colour
/// block is sewn on every copy before the next colour, with a trim between copies, so the
/// copies cost no extra colour changes. Refused when the layout is larger than the hoop
/// (either way round), naming the size it needs.
fn repeat(
    d: &Design,
    p: &Params,
    v: &Value,
    pattern: &StitchPattern,
) -> Result<StitchPattern, CommandError> {
    let int = |key: &str| -> Result<i64, CommandError> {
        match v.get(key) {
            None => Ok(1),
            Some(n) => n
                .as_i64()
                .filter(|n| (1..=MAX_REPEAT).contains(n))
                .ok_or_else(|| p.invalid("repeat", format!("{key}: 1–{MAX_REPEAT}"))),
        }
    };
    let (columns, rows) = (int("columns")?, int("rows")?);
    let gap = match v.get("gapMm") {
        None => 0.0,
        Some(g) => g
            .as_f64()
            .filter(|g| (0.0..=100.0).contains(g))
            .ok_or_else(|| p.invalid("repeat", "gapMm: 0–100"))?,
    } * MM;
    let b = pattern.bounds();
    let (w, h) = (b.width(), b.height());
    let total = (
        columns as f64 * w + (columns - 1) as f64 * gap,
        rows as f64 * h + (rows - 1) as f64 * gap,
    );
    let (hw, hh) = crate::machine::hoop_mm(d);
    let (hw, hh) = (f64::from(hw) * MM, f64::from(hh) * MM);
    let tol = 0.5;
    if !((total.0 <= hw + tol && total.1 <= hh + tol)
        || (total.0 <= hh + tol && total.1 <= hw + tol))
    {
        return Err(p.failed(format!(
            "the repeat needs {:.1} × {:.1} mm, larger than the {:.0} × {:.0} mm hoop",
            total.0 / MM,
            total.1 / MM,
            hw / MM,
            hh / MM
        )));
    }
    // The layout's top left, so that it is centred on the Design Page.
    let c = d.page.center();
    let origin = (c.x - total.0 / 2.0 - b.min_x, c.y - total.1 / 2.0 - b.min_y);
    let mut out = StitchPattern {
        threads: pattern.threads.clone(),
        metadata: pattern.metadata.clone(),
        stitches: Vec::new(),
    };
    for (k, block) in blocks(pattern).iter().enumerate() {
        if k > 0 {
            out.push(Command::ColorChange);
        }
        for r in 0..rows {
            for col in 0..columns {
                let dx = origin.0 + col as f64 * (w + gap);
                let dy = origin.1 + r as f64 * (h + gap);
                if (r, col) != (0, 0) && block.iter().any(|s| s.command == Command::Stitch) {
                    out.push(Command::Trim);
                }
                for s in block {
                    out.stitches
                        .push(Stitch::new(s.x + dx, s.y + dy, s.command));
                }
            }
        }
    }
    out.push(Command::End);
    Ok(out)
}

/// `framePark: {xMm, yMm}`: at every stop the thread is cut and the frame moves to that point
/// (mm from the Design Page's centre, +y down), and comes back after the stop.
fn frame_park(
    d: &Design,
    p: &Params,
    v: &Value,
    pattern: &StitchPattern,
) -> Result<StitchPattern, CommandError> {
    let coord = |key: &str| -> Result<f64, CommandError> {
        v.get(key)
            .and_then(Value::as_f64)
            .filter(|c| c.abs() <= 1000.0)
            .ok_or_else(|| p.invalid("framePark", format!("{key}: −1000–1000 mm")))
    };
    let c = d.page.center();
    let park = (c.x + coord("xMm")? * MM, c.y + coord("yMm")? * MM);
    let mut out = StitchPattern {
        stitches: Vec::with_capacity(pattern.stitches.len()),
        ..pattern.clone()
    };
    let mut at = (0.0, 0.0);
    for s in &pattern.stitches {
        if s.command == Command::Stop {
            out.stitches.push(Stitch::new(at.0, at.1, Command::Trim));
            out.stitches
                .push(Stitch::new(park.0, park.1, Command::Jump));
            out.stitches
                .push(Stitch::new(park.0, park.1, Command::Stop));
            out.stitches.push(Stitch::new(at.0, at.1, Command::Jump));
            continue;
        }
        if matches!(s.command, Command::Stitch | Command::Jump) {
            at = (s.x, s.y);
        }
        out.stitches.push(*s);
    }
    Ok(out)
}

/// Applies `repeat`, then `framePark`, when given.
pub fn arrange(
    d: &Design,
    p: &Params,
    pattern: StitchPattern,
) -> Result<StitchPattern, CommandError> {
    let mut pattern = pattern;
    if let Some(v) = p.get("repeat") {
        pattern = repeat(d, p, v, &pattern)?;
    }
    if let Some(v) = p.get("framePark") {
        pattern = frame_park(d, p, v, &pattern)?;
    }
    Ok(pattern)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_core::Thread;
    use serde_json::json;

    #[test]
    fn frame_park_cuts_moves_out_and_back_at_each_stop() {
        let d = Design::default();
        let mut pattern = StitchPattern::new();
        pattern.threads = vec![Thread::rgb(0)];
        pattern.stitch_to(100.0, 100.0);
        pattern.stitch_to(120.0, 100.0);
        pattern.push(Command::Stop);
        pattern.stitch_to(140.0, 100.0);
        pattern.push(Command::End);
        let p = Params::new(
            "design.export",
            Some(&json!({"framePark": {"xMm": 0, "yMm": -50}})),
        )
        .expect("params");
        let out = arrange(&d, &p, pattern).expect("arrange");
        let c = d.page.center();
        let got: Vec<(f64, f64, Command)> =
            out.stitches.iter().map(|s| (s.x, s.y, s.command)).collect();
        assert_eq!(
            got,
            [
                (100.0, 100.0, Command::Stitch),
                (120.0, 100.0, Command::Stitch),
                (120.0, 100.0, Command::Trim),
                (c.x, c.y - 500.0, Command::Jump),
                (c.x, c.y - 500.0, Command::Stop),
                (120.0, 100.0, Command::Jump),
                (140.0, 100.0, Command::Stitch),
                (140.0, 100.0, Command::End),
            ]
        );
        let bad = Params::new(
            "design.export",
            Some(&json!({"framePark": {"xMm": 5000, "yMm": 0}})),
        )
        .expect("params");
        assert!(arrange(&d, &bad, out).is_err());
    }
}
