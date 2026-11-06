//! One-line commands (#496): `convert`, `info`, `optimize`, `digitize` and `formats`. Each builds
//! the engine requests an `exec` script would hold and runs them in a session on the current
//! folder, so the result is the same as the script's. Formats follow the file extensions.
//! `thumbnail` (#557) is the file-manager thumbnail renderer (`openstitch_render::thumbnail`,
//! the same code as `openstitch-thumbnailer` and the macOS / Windows thumbnail handlers): it
//! reads the file and draws its stitches, with no engine session.
//!
//! Exit codes match `exec`: 0 done, 1 usage, 2 a command failed (with a one-line message).

use openstitch_engine::Session;
use serde_json::{Value, json};
use std::path::Path;
use std::process::ExitCode;

pub const USAGE: &str = "one-line commands (formats follow the file extensions):
  stitchtool convert <in> <out> [--json]
      one design file to another format (in: a machine file or an .osd project)
  stitchtool convert <folder> <out-folder> --format EXT [--subfolders] [--json]
      every design in the folder (Design Database batch convert, library.convert; the
      out-folder is made when missing); files already in EXT are copied unchanged, or skipped
      when out-folder is the same folder; existing targets are skipped
  stitchtool info <file> [--json]
      size, stitches, colours, trims, jumps, thread and sewing time (design.info)
  stitchtool optimize <in> <out> [--mode peDesign|minJump] [--by-color] [--json]
      the sewing order (order.optimize; --by-color: order.byColor), then export; a machine
      file is first divided into one object per colour
  stitchtool digitize <image> <out> [--size MM] [--method pe|openstitch] [--colors N] [--json]
      Auto Punch a PNG, JPEG or BMP (digitize.autoPunch; --size is the width in mm)
  stitchtool formats [--json]
      what OpenStitch reads and writes (formats.list)
  stitchtool thumbnail <file> <out.png> [--size PX] [--json]
      the file-manager thumbnail of a machine file or an .osd project: its stitches on a
      transparent PX x PX square (default 256, 16 to 1024), as Finder, Explorer and the Linux
      file managers show it";

/// `thumbnail`'s default and allowed sizes in pixels.
pub const THUMBNAIL_SIZE: u32 = 256;

/// The requests that open `path`: an .osd project replaces the design, any other file is
/// imported as stitches (the engine refuses an unknown extension).
pub fn open_request(path: &str) -> Value {
    let osd = Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("osd"));
    if osd {
        json!({"command": "project.open", "params": {"path": path}})
    } else {
        json!({"command": "stitches.import", "params": {"path": path}})
    }
}

/// Options after the positional arguments: `--flag` alone or `--key value`.
struct Flags<'a> {
    pairs: Vec<(&'a str, Option<&'a str>)>,
}

impl<'a> Flags<'a> {
    /// `switches` take no value; every other `--key` takes one. Anything else is a usage error.
    fn parse(args: &[&'a str], switches: &[&str], valued: &[&str]) -> Option<Flags<'a>> {
        let mut pairs = Vec::new();
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let key = a.strip_prefix("--")?;
            if switches.contains(&key) {
                pairs.push((key, None));
            } else if valued.contains(&key) {
                pairs.push((key, Some(*it.next()?)));
            } else {
                return None;
            }
        }
        Some(Flags { pairs })
    }

    fn has(&self, key: &str) -> bool {
        self.pairs.iter().any(|(k, _)| *k == key)
    }

    fn get(&self, key: &str) -> Option<&'a str> {
        self.pairs
            .iter()
            .rev()
            .find(|(k, _)| *k == key)
            .and_then(|(_, v)| *v)
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::FAILURE
}

/// A failed reply's message on one line.
fn error_text(reply: &Value) -> String {
    let e = &reply["error"];
    let text = match e {
        Value::String(s) => s.clone(),
        Value::Object(o) => o
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| e.to_string()),
        _ => reply.to_string(),
    };
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Runs `steps` in `session`; with `json` each reply is printed as `exec` prints it. Stops at
/// the first failure with a one-line message. The replies' results on success.
fn run(session: &mut Session, steps: &[Value], json: bool) -> Result<Vec<Value>, ExitCode> {
    let mut results: Vec<Value> = Vec::new();
    for step in steps {
        let mut step = step.clone();
        if step["command"] == "stitches.divideByColor" && step["params"]["ids"].is_null() {
            // The object the import before it made.
            let id = results.last().map_or(Value::Null, |r| r["id"].clone());
            step["params"]["ids"] = json!([id]);
        }
        let step = &step;
        let reply = session.handle(step);
        if json {
            println!("{reply}");
        }
        if reply["ok"] != json!(true) {
            let command = step["command"].as_str().unwrap_or("?");
            eprintln!("stitchtool: {command}: {}", error_text(&reply));
            return Err(ExitCode::from(2));
        }
        results.push(reply["result"].clone());
    }
    Ok(results)
}

/// The engine requests for one-line `args` (without the verb's flags that only change the
/// printing), or None for a usage error. Kept apart from running them so a test can compare
/// them with an `exec` script.
pub fn requests(verb: &str, args: &[&str]) -> Option<Vec<Value>> {
    let (pos, rest): (Vec<&str>, Vec<&str>) = {
        let split = args
            .iter()
            .position(|a| a.starts_with("--"))
            .unwrap_or(args.len());
        (args[..split].to_vec(), args[split..].to_vec())
    };
    match (verb, pos.as_slice()) {
        ("convert", [input, output]) => {
            let f = Flags::parse(&rest, &["json", "subfolders"], &["format"])?;
            if Path::new(input).is_dir() {
                let format = f.get("format")?;
                Some(vec![json!({"command": "library.convert", "params": {
                    "paths": [input], "format": format, "output": output,
                    "subfolders": f.has("subfolders"),
                }})])
            } else {
                let mut export = json!({"path": output});
                if let Some(format) = f.get("format") {
                    export["format"] = json!(format);
                }
                Some(vec![
                    open_request(input),
                    json!({"command": "design.export", "params": export}),
                ])
            }
        }
        ("info", [input]) => {
            Flags::parse(&rest, &["json"], &[])?;
            Some(vec![open_request(input), json!({"command": "design.info"})])
        }
        ("optimize", [input, output]) => {
            let f = Flags::parse(&rest, &["json", "by-color"], &["mode"])?;
            let mut steps = vec![open_request(input)];
            if steps[0]["command"] == "stitches.import" {
                // One object per colour, so the order can change. `run` fills in the id the
                // import reported.
                steps.push(json!({"command": "stitches.divideByColor", "params": {}}));
            }
            if f.has("by-color") {
                steps.push(json!({"command": "order.byColor"}));
            } else {
                let mut params = json!({});
                if let Some(mode) = f.get("mode") {
                    params["mode"] = json!(mode);
                }
                steps.push(json!({"command": "order.optimize", "params": params}));
            }
            steps.push(json!({"command": "design.export", "params": {"path": output}}));
            Some(steps)
        }
        ("digitize", [image, output]) => {
            let f = Flags::parse(&rest, &["json"], &["size", "method", "colors"])?;
            let mut params = json!({"path": image});
            if let Some(size) = f.get("size") {
                let mm = size.trim_end_matches("mm").parse::<f64>().ok()?;
                params["width"] = json!(mm);
            }
            if let Some(method) = f.get("method") {
                params["compatMode"] = json!(match method {
                    "pe" | "peDesign" => "peDesign",
                    "openstitch" | "openStitch" => "openStitch",
                    _ => return None,
                });
            }
            if let Some(colors) = f.get("colors") {
                params["colors"] = json!(colors.parse::<u32>().ok()?);
            }
            Some(vec![
                json!({"command": "digitize.autoPunch", "params": params}),
                json!({"command": "design.export", "params": {"path": output}}),
            ])
        }
        ("formats", []) => {
            Flags::parse(&rest, &["json"], &[])?;
            Some(vec![json!({"command": "formats.list"})])
        }
        _ => None,
    }
}

/// `thumbnail <file> <out.png> [--size PX] [--json]`'s file, output and size, or None for a
/// usage error.
pub fn thumbnail_args<'a>(args: &[&'a str]) -> Option<(&'a str, &'a str, u32)> {
    let split = args
        .iter()
        .position(|a| a.starts_with("--"))
        .unwrap_or(args.len());
    let &[input, output] = &args[..split] else {
        return None;
    };
    let f = Flags::parse(&args[split..], &["json"], &["size"])?;
    let size = match f.get("size") {
        Some(s) => s.trim_end_matches("px").parse::<u32>().ok().filter(|n| {
            (openstitch_render::thumbnail::MIN_SIZE..=openstitch_render::thumbnail::MAX_SIZE)
                .contains(n)
        })?,
        None => THUMBNAIL_SIZE,
    };
    Some((input, output, size))
}

/// Writes the thumbnail of `input` to `output` (both relative to `cwd`).
fn thumbnail(args: &[&str], cwd: &Path) -> ExitCode {
    let Some((input, output, size)) = thumbnail_args(args) else {
        return usage();
    };
    let out = cwd.join(output);
    let written = openstitch_render::thumbnail::load(&cwd.join(input))
        .and_then(|p| openstitch_render::thumbnail::png(&p, size))
        .and_then(|png| std::fs::write(&out, png).map_err(|e| format!("{output}: {e}")));
    if let Err(e) = written {
        eprintln!("stitchtool: thumbnail: {e}");
        return ExitCode::from(2);
    }
    if args.contains(&"--json") {
        println!("{}", json!({"path": output, "width": size, "height": size}));
    } else {
        println!("wrote {output} ({size} x {size} px)");
    }
    ExitCode::SUCCESS
}

/// Runs a one-line verb; `cwd` is the session folder relative paths resolve against.
pub fn main(verb: &str, args: &[&str], cwd: &Path) -> ExitCode {
    if verb == "thumbnail" {
        return thumbnail(args, cwd);
    }
    let Some(steps) = requests(verb, args) else {
        return usage();
    };
    let json = args.contains(&"--json");
    if steps[0]["command"] == "library.convert" {
        // `convert dsts/ pes/ --format pes`: the output folder is made when missing.
        let out = cwd.join(steps[0]["params"]["output"].as_str().unwrap_or_default());
        if let Err(e) = std::fs::create_dir_all(&out) {
            eprintln!("stitchtool: cannot make {}: {e}", out.display());
            return ExitCode::from(2);
        }
    }
    let mut session = Session::new(cwd);
    let results = match run(&mut session, &steps, json) {
        Ok(r) => r,
        Err(code) => return code,
    };
    if json {
        return ExitCode::SUCCESS;
    }
    let last = results.last().cloned().unwrap_or(Value::Null);
    match verb {
        "info" => print!("{}", info_text(args[0], &last)),
        "formats" => print!("{}", formats_text(&last)),
        "convert" if steps[0]["command"] == "library.convert" => {
            let n = |s: &str| {
                last["results"]
                    .as_array()
                    .map_or(0, |r| r.iter().filter(|x| x["status"] == s).count())
            };
            let (written, skipped, failed) = (n("written"), n("skipped"), n("failed"));
            println!("{written} written, {skipped} skipped, {failed} failed");
            for r in last["results"].as_array().into_iter().flatten() {
                if r["status"] == "failed" {
                    eprintln!(
                        "{}: {}",
                        r["from"].as_str().unwrap_or("?"),
                        r["reason"].as_str().unwrap_or("?")
                    );
                }
            }
            if failed > 0 {
                return ExitCode::from(2);
            }
        }
        _ => {
            let paths: Vec<&str> = last["paths"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            println!("wrote {}", paths.join(", "));
            for w in last["warnings"].as_array().into_iter().flatten() {
                eprintln!(
                    "warning: {}",
                    w.as_str().map_or_else(|| w.to_string(), str::to_string)
                );
            }
        }
    }
    ExitCode::SUCCESS
}

fn num(v: &Value) -> String {
    match v.as_f64() {
        Some(x) if x.fract() == 0.0 => format!("{x:.0}"),
        Some(x) => format!("{x:.1}"),
        None => "?".into(),
    }
}

/// `design.info`'s reply as a short table.
pub fn info_text(file: &str, info: &Value) -> String {
    let st = &info["stitchStats"];
    let est = &info["estimates"];
    let colours: Vec<&str> = st["colors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let rows = [
        (
            "Size",
            format!("{} x {} mm", num(&st["widthMM"]), num(&st["heightMM"])),
        ),
        ("Stitches", num(&st["stitches"])),
        (
            "Colours",
            format!("{} ({})", colours.len(), colours.join(" ")),
        ),
        ("Colour changes", num(&st["colorChanges"])),
        ("Trims", num(&st["trims"])),
        ("Jumps", num(&st["jumps"])),
        (
            "Thread",
            format!(
                "{} m top, {} m bobbin",
                num(&est["needleThreadM"]),
                num(&est["bobbinThreadM"])
            ),
        ),
        (
            "Sewing time",
            format!(
                "{} min at {} stitches/min",
                num(&est["sewingMinutes"]),
                num(&est["stitchesPerMinute"])
            ),
        ),
    ];
    let mut out = format!("{file}\n");
    for (k, v) in rows {
        out.push_str(&format!("  {k:<15}{v}\n"));
    }
    out
}

/// `formats.list`'s reply as a table.
pub fn formats_text(list: &Value) -> String {
    let mut out = String::from("Format  Read  Write\n");
    for f in list["machine"].as_array().into_iter().flatten() {
        let yes = |k: &str| if f[k] == json!(true) { "yes" } else { "-" };
        out.push_str(&format!(
            "{:<8}{:<6}{}\n",
            f["extension"].as_str().unwrap_or("?").to_uppercase(),
            yes("read"),
            yes("write")
        ));
    }
    let exports: Vec<String> = list["exports"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_uppercase)
        .collect();
    out.push_str(&format!(
        "Also written: {} (stitch lists and G-code); projects: .{}\n",
        exports.join(", "),
        list["project"].as_str().unwrap_or("osd")
    ));
    out
}
