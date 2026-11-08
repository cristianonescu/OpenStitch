//! Thread list files (#482): a design's colours in sewing order as CSV, INF, EDR, COL or TXT
//! (`design.exportThreads`), a list applied to the design's colour blocks in order
//! (`design.applyThreads`), and user thread charts read from and written to those lists
//! (`threads.importList`, `threads.exportList`). File layouts: `openstitch_formats::threadlist`
//! and docs/FILE_FORMATS.md "Thread lists".

use crate::commands::{CommandSpec, Effect, PATH, opt, req, spec};
use crate::params::{CommandError, Params};
use crate::sandbox::Access;
use crate::session::EngineState;
use crate::user::{MAX_CHART_COLORS, MAX_FILE, chart, csv_thread};
use openstitch_core::{Command, StitchPattern, Thread};
use openstitch_design::{Compiler, Design};
use openstitch_formats::threadlist::ListType;
use serde_json::{Value, json};
use std::path::Path;

/// Header of the design thread-list CSV.
pub const CSV_HEADER: &str = "block,brand,code,name,hex,r,g,b,stitches,threadM";

/// What a thread list file is: a list layout, or the CSV (this module's design CSV or a thread
/// chart CSV).
enum Kind {
    List(ListType),
    Csv,
}

fn kind(p: &Params, path: &Path) -> Result<Kind, CommandError> {
    let raw = if p.has("format") {
        p.string("format", None)?
    } else {
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_owned()
    };
    if raw.eq_ignore_ascii_case("csv") {
        return Ok(Kind::Csv);
    }
    ListType::from_extension(&raw)
        .map(Kind::List)
        .ok_or_else(|| p.invalid("format", "use csv, inf, edr, col or txt"))
}

/// CSV field without commas or line breaks (written unquoted, as the thread-chart CSV is).
fn field(s: Option<&str>) -> String {
    s.unwrap_or("").replace([',', '\r', '\n'], " ")
}

/// The design CSV: one row per colour block in sewing order, then a total row.
pub fn design_csv(pattern: &StitchPattern, metres: &[f64]) -> String {
    let mut out = format!("{CSV_HEADER}\n");
    let blocks = pattern.color_blocks();
    let mut total = (0, 0.0);
    for (i, (thread, stitches)) in blocks.iter().enumerate() {
        let t = thread.cloned().unwrap_or_else(|| Thread::rgb(0));
        let n = stitches
            .iter()
            .filter(|s| s.command == Command::Stitch)
            .count();
        let m = metres.get(i).copied().unwrap_or(0.0);
        total = (total.0 + n, total.1 + m);
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{n},{m:.2}\n",
            i + 1,
            field(t.brand.as_deref()),
            field(t.catalog_number.as_deref()),
            field(t.name.as_deref()),
            t.hex(),
            t.red,
            t.green,
            t.blue
        ));
    }
    out.push_str(&format!("total,,,,,,,,{},{:.2}\n", total.0, total.1));
    out
}

/// Threads of a CSV: the design CSV (rows with a block number and a `#rrggbb`), or a
/// thread-chart CSV (`code,brand,description,R,G,B`).
fn read_csv(text: &str) -> Result<Vec<Thread>, String> {
    let mut out = Vec::new();
    for (k, line) in text.lines().enumerate() {
        let line = line.trim_start_matches('\u{feff}').trim();
        let lower = line.to_ascii_lowercase();
        if line.is_empty()
            || line.starts_with('#')
            || lower.starts_with("block,")
            || lower.starts_with("total,")
            || lower.starts_with("code,")
        {
            continue;
        }
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        if f.len() == 10 {
            let mut t =
                Thread::from_hex(f[4]).ok_or(format!("line {}: colour '{}'", k + 1, f[4]))?;
            let text = |s: &str| (!s.is_empty()).then(|| s.to_owned().into());
            t.brand = text(f[1]);
            t.catalog_number = text(f[2]);
            t.name = text(f[3]);
            out.push(t);
        } else {
            out.push(csv_thread(line).map_err(|e| format!("line {}: {e}", k + 1))?);
        }
    }
    Ok(out)
}

fn read_list(s: &mut EngineState, p: &Params) -> Result<Vec<Thread>, CommandError> {
    let path = s.resolve(&p.string("path", None)?, Access::Read, &p.command)?;
    let kind = kind(p, &path)?;
    let meta = crate::fsio::metadata(&path).map_err(|e| p.failed(e.to_string()))?;
    if meta.len() > MAX_FILE {
        return Err(p.failed("the file is too large"));
    }
    let bytes = crate::fsio::read(&path).map_err(|e| p.failed(e.to_string()))?;
    let threads = match kind {
        Kind::List(t) => t.read(&bytes).map_err(|e| p.failed(e.to_string()))?,
        Kind::Csv => read_csv(&String::from_utf8_lossy(&bytes)).map_err(|e| p.failed(e))?,
    };
    if threads.is_empty() {
        return Err(p.failed("the list has no colours"));
    }
    Ok(threads)
}

/// An object's index and each thread it sews with the design colour block it falls in.
type ObjectBlocks = (usize, Vec<(Thread, usize)>);

/// The colour blocks each visible object sews, in sewing order: `(object index, its threads)`,
/// checked against the whole design's blocks. `None` when they do not line up (consecutive
/// objects of one colour share a block; anything else is not a simple mapping).
fn object_blocks(d: &Design, pattern: &StitchPattern) -> Option<Vec<ObjectBlocks>> {
    let compiler = Compiler::for_design(d);
    let mut out = Vec::new();
    let mut seq: Vec<Thread> = Vec::new();
    for (i, o) in d.objects.iter().enumerate() {
        if !o.visible {
            continue;
        }
        let local = compiler.compile_object(d, i);
        let mut mine = Vec::new();
        for t in local.threads {
            if seq.last() != Some(&t) {
                seq.push(t.clone());
            }
            mine.push((t, seq.len() - 1));
        }
        out.push((i, mine));
    }
    (seq == pattern.threads).then_some(out)
}

/// Sets every thread of the design so that colour block `i` sews `list[i]`.
fn apply(d: &mut Design, list: &[Thread]) -> Result<(), String> {
    let pattern = Compiler::for_design(d).compile(d);
    let blocks = pattern.threads.len();
    if pattern.count(Command::Stitch) == 0 {
        return Err("the design has no stitches".into());
    }
    if list.len() != blocks {
        return Err(format!(
            "the list has {} colours and the design {blocks} colour blocks; nothing changed",
            list.len()
        ));
    }
    let map = object_blocks(d, &pattern).ok_or(
        "the design's colour blocks do not map onto its objects one to one; nothing changed",
    )?;
    for (i, mine) in map {
        let new_for = |t: &Thread| -> Result<Option<Thread>, String> {
            let mut found: Option<&Thread> = None;
            for (bt, g) in &mine {
                if bt == t {
                    let n = &list[*g];
                    if found.is_some_and(|f| f != n) {
                        return Err(format!(
                            "object {} sews {} in several colour blocks that the list gives different colours; nothing changed",
                            d.objects[i].id,
                            t.hex()
                        ));
                    }
                    found = Some(n);
                }
            }
            Ok(found.cloned())
        };
        let o = &d.objects[i];
        let region = o
            .region
            .as_ref()
            .map(|r| new_for(&r.thread))
            .transpose()?
            .flatten();
        let line = o
            .line
            .as_ref()
            .map(|l| new_for(&l.thread))
            .transpose()?
            .flatten();
        let block: Option<Vec<Option<Thread>>> = o
            .stitch_block
            .as_ref()
            .map(|b| b.threads.iter().map(new_for).collect::<Result<_, _>>())
            .transpose()?;
        let o = &mut d.objects[i];
        if let (Some(r), Some(t)) = (&mut o.region, region) {
            r.thread = t;
        }
        if let (Some(l), Some(t)) = (&mut o.line, line) {
            l.thread = t;
        }
        if let (Some(b), Some(new)) = (&mut o.stitch_block, block) {
            for (t, n) in b.threads.iter_mut().zip(new) {
                if let Some(n) = n {
                    *t = n;
                }
            }
        }
    }
    // Check the result rather than trust the mapping.
    let after = Compiler::for_design(d).compile(d);
    let hexes = |t: &[Thread]| t.iter().map(Thread::hex).collect::<Vec<_>>();
    if hexes(&after.threads) != hexes(list) {
        return Err("the design's colours could not be set block by block; nothing changed".into());
    }
    Ok(())
}

fn chart_threads(s: &EngineState, p: &Params) -> Result<Vec<Thread>, CommandError> {
    match (p.has("chart"), p.has("builtin")) {
        (true, false) => {
            let n = p.string("chart", None)?;
            s.user
                .chart_index(&n)
                .map(|i| s.user.thread_charts[i].threads.clone())
                .ok_or_else(|| p.not_found(format!("thread chart '{n}'")))
        }
        (false, true) => {
            let id = p.string("builtin", None)?;
            let b = crate::thread_match::builtins()
                .into_iter()
                .find(|b| b.id == id)
                .ok_or_else(|| p.invalid("builtin", "see threads.builtinCharts"))?;
            Ok(b.entries.into_iter().map(|(_, t)| t).collect())
        }
        _ => Err(p.invalid("chart", "give either chart or builtin")),
    }
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "design.exportThreads",
            "Write the design's colours in sewing order as a thread list: CSV (block, brand, code, name, hex, R, G, B, stitches, thread length in m, and a total row), INF, EDR, COL or TXT",
            Effect::File,
            vec![
                PATH,
                opt("format", "csv|inf|edr|col|txt", "Override the extension"),
            ],
            |s, p| {
                let path = s.resolve(&p.string("path", None)?, Access::Write, &p.command)?;
                let kind = kind(p, &path)?;
                let pattern = Compiler::for_design(&s.design).compile(&s.design);
                if pattern.count(Command::Stitch) == 0 {
                    return Err(p.failed("design has no stitches"));
                }
                let bytes = match kind {
                    Kind::Csv => {
                        let e = crate::machine::estimates(&s.design, &pattern);
                        let metres: Vec<f64> = e["threadPerColorM"]
                            .as_array()
                            .map(|a| a.iter().filter_map(Value::as_f64).collect())
                            .unwrap_or_default();
                        design_csv(&pattern, &metres).into_bytes()
                    }
                    Kind::List(t) => t
                        .write(&pattern.threads)
                        .map_err(|e| p.failed(e.to_string()))?,
                };
                std::fs::write(&path, bytes).map_err(|e| p.failed(e.to_string()))?;
                Ok(json!({
                    "path": path.display().to_string(),
                    "colors": pattern.threads.len(),
                }))
            },
        ),
        spec(
            "design.applyThreads",
            "Give the design's colour blocks, in sewing order, the colours of a thread list (CSV, INF, EDR, COL or TXT); a list of another length is refused, nothing is guessed; one undo step",
            Effect::Edit,
            vec![
                PATH,
                opt("format", "csv|inf|edr|col|txt", "Override the extension"),
            ],
            |s, p| {
                let list = read_list(s, p)?;
                apply(&mut s.design, &list).map_err(|e| p.failed(e))?;
                Ok(json!({ "colors": list.len() }))
            },
        ),
        spec(
            "threads.importList",
            "Add the colours of a thread list file (CSV, INF, EDR, COL or TXT) to a user chart",
            Effect::Settings,
            vec![
                req("chart", "string", "Chart"),
                PATH,
                opt("format", "csv|inf|edr|col|txt", "Override the extension"),
            ],
            |s, p| {
                let threads = read_list(s, p)?;
                let c = chart(s, p)?;
                if c.threads.len() + threads.len() > MAX_CHART_COLORS {
                    return Err(p.failed(format!("at most {MAX_CHART_COLORS} colours per chart")));
                }
                let added = threads.len();
                c.threads.extend(threads);
                Ok(json!({ "added": added }))
            },
        ),
        spec(
            "threads.exportList",
            "Write a user chart (chart) or a built-in chart (builtin) as a thread list file: INF, EDR, COL or TXT (CSV: threads.exportCsv)",
            Effect::File,
            vec![
                opt("chart", "string", "User chart name"),
                opt(
                    "builtin",
                    "string",
                    "Built-in chart id (see threads.builtinCharts)",
                ),
                PATH,
                opt("format", "inf|edr|col|txt", "Override the extension"),
            ],
            |s, p| {
                let threads = chart_threads(s, p)?;
                let path = s.resolve(&p.string("path", None)?, Access::Write, &p.command)?;
                let Kind::List(t) = kind(p, &path)? else {
                    return Err(p.invalid("format", "for CSV use threads.exportCsv"));
                };
                let bytes = t.write(&threads).map_err(|e| p.failed(e.to_string()))?;
                std::fs::write(&path, bytes).map_err(|e| p.failed(e.to_string()))?;
                Ok(json!({ "path": path.display().to_string(), "colors": threads.len() }))
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_csv_reads_back_as_its_threads() {
        let mut p = StitchPattern::new();
        let mut red = Thread::rgb(0xFF0000).with_name("Red, bright");
        red.brand = Some("Acme".into());
        p.threads = vec![red, Thread::rgb(0x00FF00)];
        p.stitch_to(0.0, 0.0);
        p.stitch_to(10.0, 0.0);
        p.push(Command::ColorChange);
        p.stitch_to(20.0, 0.0);
        p.push(Command::End);
        let csv = design_csv(&p, &[0.5, 0.25]);
        assert_eq!(
            csv,
            "block,brand,code,name,hex,r,g,b,stitches,threadM\n1,Acme,,Red  bright,#ff0000,255,0,0,2,0.50\n2,,,,#00ff00,0,255,0,1,0.25\ntotal,,,,,,,,3,0.75\n"
        );
        let back = read_csv(&csv).expect("read");
        assert_eq!(
            back.iter().map(Thread::hex).collect::<Vec<_>>(),
            ["#ff0000", "#00ff00"]
        );
        assert_eq!(back[0].name.as_deref(), Some("Red  bright"));
        assert_eq!(back[0].brand.as_deref(), Some("Acme"));
    }
}
