//! GIMP / Inkscape palettes (`.gpl`) as user thread charts (#483): `threads.importPalette`
//! reads one into a new chart, `threads.exportPalette` writes a user or built-in chart as one.
//!
//! The format, as GIMP's and Inkscape's manuals describe it: a `GIMP Palette` first line,
//! optional `Name:` and `Columns:` lines, `#` comments, then one colour per line: red, green and
//! blue as 0–255 integers separated by white space, then an optional name. OpenStitch reads a
//! name whose first word is all digits ("1800 Ruby") as thread code 1800 named Ruby, and
//! writes a thread with a code that way, so a chart goes out and back unchanged (colour, code,
//! name, order). The format has no brand, so a brand is not written.

use crate::commands::{CommandSpec, Effect, PATH, opt, req, spec};
use crate::params::Params;
use crate::sandbox::Access;
use crate::session::EngineState;
use crate::user::{MAX_CHART_COLORS, MAX_CHARTS, MAX_FILE, MAX_NAME, ThreadChart};
use openstitch_core::Thread;
use serde_json::{Value, json};
use std::borrow::Cow;

/// A palette read from a `.gpl` file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Palette {
    /// The `Name:` line, if any.
    pub name: Option<String>,
    pub threads: Vec<Thread>,
    /// Lines that could not be read: 1-based line number and why.
    pub errors: Vec<(usize, String)>,
}

fn clip(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME)
        .collect::<String>()
        .trim()
        .to_string()
}

/// One colour line: `R G B [name]`.
fn colour_line(line: &str) -> Result<Thread, String> {
    let mut rest = line.trim_start();
    let mut rgb = [0u8; 3];
    for c in &mut rgb {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (num, tail) = rest.split_at(end);
        *c = num
            .parse::<u8>()
            .map_err(|_| format!("'{num}' is not a colour value 0-255"))?;
        rest = tail.trim_start();
    }
    let mut t = Thread::rgb(u32::from(rgb[0]) << 16 | u32::from(rgb[1]) << 8 | u32::from(rgb[2]));
    let name = clip(rest);
    let (first, after) = match name.split_once(char::is_whitespace) {
        Some((a, b)) => (a, b.trim()),
        None => (name.as_str(), ""),
    };
    if !first.is_empty() && first.chars().all(|c| c.is_ascii_digit()) {
        t.catalog_number = Some(Cow::Owned(first.to_string()));
        t.name = (!after.is_empty()).then(|| Cow::Owned(after.to_string()));
    } else {
        t.name = (!name.is_empty()).then(|| Cow::Owned(name.clone()));
    }
    Ok(t)
}

/// Reads a `.gpl` palette. Fails only when the file is not a palette at all (no `GIMP Palette`
/// first line); bad colour lines are collected in [`Palette::errors`].
pub fn parse_gpl(text: &str) -> Result<Palette, String> {
    let mut lines = text.lines().enumerate();
    let header = lines
        .by_ref()
        .map(|(_, l)| l.trim_start_matches('\u{feff}').trim())
        .find(|l| !l.is_empty());
    if header != Some("GIMP Palette") {
        return Err("not a GIMP palette: the first line must be 'GIMP Palette'".into());
    }
    let mut p = Palette::default();
    for (k, line) in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(n) = line.strip_prefix("Name:") {
            let n = clip(n);
            if !n.is_empty() {
                p.name = Some(n);
            }
            continue;
        }
        if line.starts_with("Columns:") {
            continue;
        }
        match colour_line(line) {
            Ok(t) => p.threads.push(t),
            Err(e) => p.errors.push((k + 1, e)),
        }
    }
    Ok(p)
}

/// One name field written for a palette line: control characters dropped, edges trimmed.
fn field(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// A palette file of `threads` named `name`: GIMP's layout (`%3d %3d %3d\tname`), LF line ends.
pub fn write_gpl<'a>(name: &str, threads: impl IntoIterator<Item = &'a Thread>) -> String {
    let mut out = format!("GIMP Palette\nName: {}\nColumns: 0\n#\n", field(name));
    for t in threads {
        let label = [t.catalog_number.as_deref(), t.name.as_deref()]
            .into_iter()
            .flatten()
            .map(field)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!(
            "{:3} {:3} {:3}\t{label}\n",
            t.red, t.green, t.blue
        ));
    }
    out
}

fn import_palette(s: &mut EngineState, p: &Params) -> crate::CommandResult {
    let path = s.resolve(&p.string("path", None)?, Access::Read, &p.command)?;
    let meta = std::fs::metadata(&path).map_err(|e| p.failed(e.to_string()))?;
    if meta.len() > MAX_FILE {
        return Err(p.failed("the file is too large"));
    }
    let bytes = std::fs::read(&path).map_err(|e| p.failed(e.to_string()))?;
    let palette = parse_gpl(&String::from_utf8_lossy(&bytes)).map_err(|e| p.failed(e))?;
    let partial = p.bool("partial", Some(false))?;
    if !palette.errors.is_empty() && !partial {
        let lines: Vec<String> = palette
            .errors
            .iter()
            .map(|(n, e)| format!("line {n}: {e}"))
            .collect();
        return Err(p.failed(format!(
            "{} (nothing imported; partial: true imports the other lines)",
            lines.join("; ")
        )));
    }
    if palette.threads.len() > MAX_CHART_COLORS {
        return Err(p.failed(format!("at most {MAX_CHART_COLORS} colours per chart")));
    }
    let stem = path
        .file_stem()
        .map(|s| clip(&s.to_string_lossy()))
        .unwrap_or_default();
    let name = if p.has("name") {
        clip(&p.string("name", None)?)
    } else {
        palette.name.clone().unwrap_or(stem)
    };
    if name.is_empty() {
        return Err(p.invalid("name", "the palette has no name: give one"));
    }
    // The same rule as threads.createChart: a name in use is refused, never replaced.
    if s.user.chart_index(&name).is_some() {
        return Err(p.failed(format!("a chart named '{name}' exists; give another name")));
    }
    if s.user.thread_charts.len() >= MAX_CHARTS {
        return Err(p.failed(format!("at most {MAX_CHARTS} charts")));
    }
    let added = palette.threads.len();
    s.user.thread_charts.push(ThreadChart {
        name: name.clone(),
        threads: palette.threads,
    });
    Ok(json!({
        "chart": name,
        "added": added,
        "skipped": palette
            .errors
            .iter()
            .map(|(n, e)| json!({ "line": n, "reason": e }))
            .collect::<Vec<Value>>(),
    }))
}

fn export_palette(s: &mut EngineState, p: &Params) -> crate::CommandResult {
    let (name, threads): (String, Vec<Thread>) = match (p.has("chart"), p.has("builtin")) {
        (true, false) => {
            let n = p.string("chart", None)?;
            let c = s
                .user
                .chart_index(&n)
                .map(|i| &s.user.thread_charts[i])
                .ok_or_else(|| p.not_found(format!("thread chart '{n}'")))?;
            (c.name.clone(), c.threads.clone())
        }
        (false, true) => {
            let id = p.string("builtin", None)?;
            let b = crate::thread_match::builtins()
                .into_iter()
                .find(|b| b.id == id)
                .ok_or_else(|| p.invalid("builtin", "see threads.builtinCharts"))?;
            let threads = b.entries.into_iter().map(|(_, t)| t).collect();
            (b.name.to_string(), threads)
        }
        _ => return Err(p.invalid("chart", "give either chart or builtin")),
    };
    let path = s.resolve(&p.string("path", None)?, Access::Write, &p.command)?;
    std::fs::write(&path, write_gpl(&name, &threads)).map_err(|e| p.failed(e.to_string()))?;
    Ok(json!({ "path": path.display().to_string(), "colors": threads.len() }))
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "threads.importPalette",
            "Import a GIMP / Inkscape palette (.gpl) as a new user thread chart; a name starting with a number (\"1800 Ruby\") gives the thread code. Lines that cannot be read are reported with their numbers and nothing is imported unless partial",
            Effect::Settings,
            vec![
                PATH,
                opt(
                    "name",
                    "string",
                    "Chart name (default: the palette's Name: line, else the file name); a name in use is refused",
                ),
                opt(
                    "partial",
                    "bool",
                    "Import the readable lines of a file with bad lines (default false)",
                ),
            ],
            import_palette,
        ),
        spec(
            "threads.exportPalette",
            "Write a user chart (chart) or a built-in chart (builtin) as a GIMP / Inkscape palette (.gpl): one line per colour, \"R G B<tab>code name\"",
            Effect::File,
            vec![
                opt("chart", "string", "User chart name"),
                opt(
                    "builtin",
                    "string",
                    "Built-in chart id (see threads.builtinCharts)",
                ),
                req(
                    "path",
                    "string",
                    "Palette file to write (relative to the session directory)",
                ),
            ],
            export_palette,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(rgb: u32, code: Option<&str>, name: Option<&str>) -> Thread {
        let mut t = Thread::rgb(rgb);
        t.catalog_number = code.map(|c| Cow::Owned(c.to_string()));
        t.name = name.map(|n| Cow::Owned(n.to_string()));
        t
    }

    /// The layout GIMP writes: header, Name, Columns, a `#` line, `%3d %3d %3d\tname`.
    #[test]
    fn gimp_layout_with_name_columns_comments_and_tabs() {
        let p = parse_gpl(
            "GIMP Palette\nName: Spools\nColumns: 4\n#\n# my threads\n255   0   0\t1800 Ruby\n  0 128 255\tSky blue\n 10  20  30\n",
        )
        .expect("palette");
        assert_eq!(p.name.as_deref(), Some("Spools"));
        assert!(p.errors.is_empty());
        assert_eq!(
            p.threads,
            [
                thread(0xff0000, Some("1800"), Some("Ruby")),
                thread(0x0080ff, None, Some("Sky blue")),
                thread(0x0a141e, None, None),
            ]
        );
    }

    /// Inkscape's layout: no Name line in some, spaces instead of tabs, CRLF, a BOM, a code
    /// with no name.
    #[test]
    fn inkscape_layout_crlf_bom_and_code_only() {
        let p = parse_gpl("\u{feff}GIMP Palette\r\n#\r\n0 0 0 Black\r\n255 255 255 2001\r\n\r\n")
            .expect("palette");
        assert_eq!(p.name, None);
        assert_eq!(
            p.threads,
            [
                thread(0, None, Some("Black")),
                thread(0xffffff, Some("2001"), None),
            ]
        );
    }

    #[test]
    fn bad_lines_are_reported_with_their_numbers() {
        let p = parse_gpl("GIMP Palette\n1 2 3 ok\n256 0 0 too much\n1 2\nx 1 2\n4 5 6\n")
            .expect("palette");
        assert_eq!(p.threads.len(), 2);
        let lines: Vec<usize> = p.errors.iter().map(|e| e.0).collect();
        assert_eq!(lines, [3, 4, 5]);
        for bad in ["", "Palette\n1 2 3", "JASC-PAL\n0100\n"] {
            assert!(parse_gpl(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn export_then_import_is_identical() {
        let threads = vec![
            thread(0xff0000, Some("1800"), Some("Ruby Red")),
            thread(0x00ff00, None, Some("Leaf")),
            thread(0x0000ff, Some("7"), None),
            thread(0x123456, None, None),
        ];
        let text = write_gpl("Mine", &threads);
        assert!(
            text.starts_with(
                "GIMP Palette\nName: Mine\nColumns: 0\n#\n255   0   0\t1800 Ruby Red\n"
            )
        );
        let back = parse_gpl(&text).expect("palette");
        assert_eq!(back.name.as_deref(), Some("Mine"));
        assert_eq!(back.threads, threads);
        assert!(back.errors.is_empty());
    }
}
