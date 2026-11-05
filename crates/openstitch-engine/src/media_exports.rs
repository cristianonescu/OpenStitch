//! Export extras, part 2 (#488): a picture of the design (`design.exportPicture`), a
//! stitch-out video as an animated PNG (`design.exportVideo`, no video codec needed) and a
//! ZIP of several exports (`design.exportBundle`). Pictures and frames are the flat preview
//! (`crate::preview`); the bundle runs the same export commands the user would, so every file
//! in it is byte for byte the file an individual export writes.

use crate::commands::{CommandSpec, Effect, PATH, opt, req, spec, stats};
use crate::params::{CommandError, Params};
use crate::preview::{self, View};
use crate::sandbox::Access;
use crate::session::EngineState;
use openstitch_core::{Command, StitchPattern};
use openstitch_design::{Compiler, Design};
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Most frames a video may have (about 5 minutes at 10 frames per second).
pub const MAX_FRAMES: usize = 3_000;

/// Frames for `stitches` sewn at `speed` stitches per second, `fps` frames per second:
/// `ceil(stitches / speed × fps)`, at least one.
pub fn frame_count(stitches: usize, speed: f64, fps: f64) -> usize {
    ((stitches as f64 / speed * fps).ceil() as usize).max(1)
}

/// Index of the record after the `n`-th stitch (the stream's length when there are fewer).
fn after_stitch(pattern: &StitchPattern, n: usize) -> usize {
    let mut seen = 0;
    pattern
        .stitches
        .iter()
        .position(|s| {
            if s.command == Command::Stitch {
                seen += 1;
            }
            seen > n
        })
        .unwrap_or(pattern.stitches.len())
}

/// Records `from..to` to draw on top of what records `..from` drew: from the last stitch
/// before `from` (so the next stitch joins it, and a trim or colour change in between still
/// cuts), with the threads from that stitch's colour block on.
fn delta(pattern: &StitchPattern, from: usize, to: usize) -> StitchPattern {
    let start = pattern.stitches[..from]
        .iter()
        .rposition(|s| s.command == Command::Stitch)
        .unwrap_or(0);
    let block = pattern.stitches[..start]
        .iter()
        .filter(|s| s.command == Command::ColorChange)
        .count();
    StitchPattern {
        stitches: pattern.stitches[start..to].to_vec(),
        threads: pattern.threads.get(block..).unwrap_or_default().to_vec(),
        metadata: Default::default(),
    }
}

/// The smallest rectangle `(x, y, w, h)` where two same-sized RGBA bitmaps differ.
fn changed(a: &[u8], b: &[u8], width: usize) -> Option<(usize, usize, usize, usize)> {
    let row = width * 4;
    let rows: Vec<usize> = (0..a.len() / row)
        .filter(|&y| a[y * row..(y + 1) * row] != b[y * row..(y + 1) * row])
        .collect();
    let (&y0, &y1) = (rows.first()?, rows.last()?);
    let (mut x0, mut x1) = (width, 0);
    for y in y0..=y1 {
        for x in 0..width {
            let i = y * row + x * 4;
            if a[i..i + 4] != b[i..i + 4] {
                x0 = x0.min(x);
                x1 = x1.max(x);
            }
        }
    }
    Some((x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

/// An animated PNG of the design sewing itself: `frame_count` frames shown `1/fps` s each,
/// the stitches appearing as the machine would sew them at `speed` stitches per second; every
/// frame in the same view as the whole design. Each frame draws only the stitches added since
/// the one before, onto it, and stores only the rectangle that changed; the last frame is the
/// whole design drawn at once, the same pixels as its preview picture. Played once.
pub fn apng(
    design: &Design,
    pattern: &StitchPattern,
    max_pixels: u32,
    fps: f64,
    speed: f64,
) -> Result<(Vec<u8>, usize), String> {
    let total = pattern.count(Command::Stitch);
    let frames = frame_count(total, speed, fps);
    if frames > MAX_FRAMES {
        return Err(format!(
            "{frames} frames (at most {MAX_FRAMES}): raise the speed or lower the frame rate"
        ));
    }
    let view = View::fit(design, pattern, max_pixels);
    let width = view.width as usize;
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, view.width, view.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Balanced);
        let err = |e: png::EncodingError| format!("PNG encoding failed: {e}");
        encoder.set_animated(frames as u32, 1).map_err(err)?;
        encoder
            .set_frame_delay(1, fps.round().clamp(1.0, 60.0) as u16)
            .map_err(err)?;
        let mut writer = encoder.write_header().map_err(err)?;
        writer.set_blend_op(png::BlendOp::Source).map_err(err)?;
        writer.set_dispose_op(png::DisposeOp::None).map_err(err)?;
        let mut pixmap = preview::draw(design, &StitchPattern::new(), &view, false)?;
        let mut shown = pixmap.data().to_vec();
        let mut drawn = 0;
        for k in 1..=frames {
            let last = k == frames;
            if last {
                pixmap = preview::draw(design, pattern, &view, false)?;
            } else {
                let n = ((k as f64 * speed / fps).ceil() as usize).min(total);
                let to = after_stitch(pattern, n);
                if to > drawn {
                    preview::stroke_runs(&mut pixmap, &delta(pattern, drawn, to), &view, false);
                    drawn = to;
                }
            }
            // The first and last frames whole; the others only where they changed.
            let rect = if k == 1 || last {
                Some((0, 0, width, view.height as usize))
            } else {
                changed(&shown, pixmap.data(), width)
            };
            let (x, y, w, h) = rect.unwrap_or((0, 0, 1, 1));
            let data = pixmap.data();
            let mut rgb = Vec::with_capacity(w * h * 3);
            for row in y..y + h {
                for col in x..x + w {
                    let i = (row * width + col) * 4;
                    rgb.extend_from_slice(&data[i..i + 3]);
                }
            }
            writer.reset_frame_position().map_err(err)?;
            writer
                .set_frame_dimension(w as u32, h as u32)
                .map_err(err)?;
            writer.set_frame_position(x as u32, y as u32).map_err(err)?;
            writer.write_image_data(&rgb).map_err(err)?;
            shown.copy_from_slice(pixmap.data());
        }
        writer.finish().map_err(err)?;
    }
    Ok((out, frames))
}

fn compiled(s: &EngineState, p: &Params) -> Result<StitchPattern, CommandError> {
    let pattern = Compiler::for_design(&s.design).compile(&s.design);
    if pattern.count(Command::Stitch) == 0 {
        return Err(p.failed("design has no stitches"));
    }
    Ok(pattern)
}

/// The command `id` run with `params`, as the user would run it.
fn run(s: &mut EngineState, id: &str, params: Value) -> Result<Value, CommandError> {
    let spec = crate::commands::all()
        .into_iter()
        .find(|c| c.id == id)
        .expect("a registered command");
    let p = Params::new(id, Some(&params))?;
    (spec.run)(s, &p)
}

/// A scratch folder beside `zip` for the bundle's parts, removed when dropped.
struct Parts(PathBuf);

impl Drop for Parts {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn bundle(s: &mut EngineState, p: &Params, zip_path: &Path) -> Result<Value, CommandError> {
    let formats: Vec<String> = match p.get("formats") {
        None => vec!["pes".into(), "dst".into(), "jef".into()],
        Some(Value::Array(a)) if !a.is_empty() && a.len() <= 30 => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_ascii_lowercase)
                    .ok_or_else(|| p.invalid("formats", "a list of extensions"))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(p.invalid("formats", "a list of 1–30 extensions")),
    };
    let thread_list = match p.get("threadList") {
        None | Some(Value::Bool(true)) => Some("csv".to_owned()),
        Some(Value::Bool(false)) => None,
        Some(Value::String(f)) => Some(f.to_ascii_lowercase()),
        Some(_) => return Err(p.invalid("threadList", "true, false or a list format")),
    };
    let picture = p.bool("picture", Some(true))?;
    let stem = zip_path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("design")
        .to_owned();
    let dir = zip_path.with_file_name(format!(".{stem}.bundle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| p.failed(e.to_string()))?;
    let parts = Parts(dir);
    let mut entries: Vec<String> = Vec::new();
    let path = |name: &str| parts.0.join(name).display().to_string();
    for f in &formats {
        let name = format!("{stem}.{f}");
        let out = run(
            s,
            "design.export",
            json!({ "path": path(&name), "format": f, "split": true }),
        )
        .map_err(|e| p.failed(format!("{f}: {e}")))?;
        for written in out["paths"].as_array().into_iter().flatten() {
            if let Some(file) = written.as_str().and_then(|w| Path::new(w).file_name()) {
                entries.push(file.to_string_lossy().into_owned());
            }
        }
    }
    if let Some(f) = &thread_list {
        let name = format!("{stem} threads.{f}");
        run(
            s,
            "design.exportThreads",
            json!({ "path": path(&name), "format": f }),
        )
        .map_err(|e| p.failed(format!("thread list: {e}")))?;
        entries.push(name);
    }
    if picture {
        let name = format!("{stem}.png");
        run(s, "design.exportPicture", json!({ "path": path(&name) }))
            .map_err(|e| p.failed(format!("picture: {e}")))?;
        entries.push(name);
    }
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    for name in &entries {
        let bytes = std::fs::read(parts.0.join(name)).map_err(|e| p.failed(e.to_string()))?;
        zip.start_file(name.as_str(), options)
            .map_err(|e| p.failed(e.to_string()))?;
        zip.write_all(&bytes).map_err(|e| p.failed(e.to_string()))?;
    }
    let bytes = zip
        .finish()
        .map_err(|e| p.failed(e.to_string()))?
        .into_inner();
    std::fs::write(zip_path, bytes).map_err(|e| p.failed(e.to_string()))?;
    Ok(json!({ "path": zip_path.display().to_string(), "files": entries }))
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "design.exportPicture",
            "Write a PNG picture of the design (the stitches in their thread colours on the Design Page), the longest side `size` pixels",
            Effect::File,
            vec![
                PATH,
                opt(
                    "size",
                    "int",
                    "Longest side in pixels, 64–4096 (default 1024)",
                ),
            ],
            |s, p| {
                let path = s.resolve(&p.string("path", None)?, Access::Write, &p.command)?;
                let size = p.int("size", Some(1024), 64, 4096)? as u32;
                let pattern = compiled(s, p)?;
                let png =
                    preview::png(&s.design, &pattern, size, false).map_err(|e| p.failed(e))?;
                std::fs::write(&path, png).map_err(|e| p.failed(e.to_string()))?;
                Ok(json!({ "path": path.display().to_string() }))
            },
        ),
        spec(
            "design.exportVideo",
            "Write a stitch-out video of the design sewing itself as an animated PNG (.png or .apng): frames = ceil(stitches / speed × fps), the last frame the design's picture",
            Effect::File,
            vec![
                PATH,
                opt("fps", "number", "Frames per second, 1–60 (default 10)"),
                opt(
                    "speed",
                    "number",
                    "Stitches sewn per second of video, 10–20000 (default 300)",
                ),
                opt(
                    "size",
                    "int",
                    "Longest side in pixels, 64–1920 (default 1280: 720p for a wide design)",
                ),
            ],
            |s, p| {
                let path = s.resolve(&p.string("path", None)?, Access::Write, &p.command)?;
                let fps = p.opt("fps", 10.0, 1.0, 60.0)?;
                let speed = p.opt("speed", 300.0, 10.0, 20_000.0)?;
                let size = p.int("size", Some(1280), 64, 1920)? as u32;
                let pattern = compiled(s, p)?;
                let (bytes, frames) =
                    apng(&s.design, &pattern, size, fps, speed).map_err(|e| p.failed(e))?;
                std::fs::write(&path, bytes).map_err(|e| p.failed(e.to_string()))?;
                Ok(json!({
                    "path": path.display().to_string(),
                    "frames": frames,
                    "seconds": frames as f64 / fps,
                    "stats": stats(&pattern),
                }))
            },
        ),
        spec(
            "design.exportBundle",
            "Write a ZIP of the design in several machine formats (default PES, DST, JEF), its thread list (CSV by default) and a picture, each file the same as its individual export",
            Effect::File,
            vec![
                req(
                    "path",
                    "string",
                    "The .zip file (relative to the session directory)",
                ),
                opt(
                    "formats",
                    "[string]",
                    "Extensions to include (default pes, dst, jef)",
                ),
                opt(
                    "threadList",
                    "bool | csv|inf|edr|col|txt",
                    "The thread list (default true: CSV)",
                ),
                opt("picture", "bool", "A PNG picture (default true)"),
            ],
            |s, p| {
                let path = s.resolve(&p.string("path", None)?, Access::Write, &p.command)?;
                bundle(s, p, &path)
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drawing the stream in steps, each step on top of the last, ends where drawing it at
    /// once does, within anti-aliasing noise where strokes overlap.
    #[test]
    fn drawing_step_by_step_matches_drawing_at_once() {
        let mut d = Design::default();
        d.page.width = 400.0;
        d.page.height = 300.0;
        let mut p = StitchPattern::new();
        p.threads = vec![
            openstitch_core::Thread::rgb(0xFF0000),
            openstitch_core::Thread::rgb(0x0000FF),
        ];
        for i in 0..120 {
            p.stitch_to(20.0 + f64::from(i % 2) * 120.0, 20.0 + f64::from(i) * 1.5);
        }
        p.push(Command::Trim);
        p.jump_to(250.0, 40.0);
        p.push(Command::ColorChange);
        for i in 0..90 {
            p.stitch_to(250.0 + f64::from(i % 2) * 100.0, 40.0 + f64::from(i) * 2.0);
        }
        p.push(Command::End);
        let view = View::fit(&d, &p, 400);
        let whole = preview::draw(&d, &p, &view, false).expect("draw");
        for step in [1, 7, 37] {
            let mut pixmap = preview::draw(&d, &StitchPattern::new(), &view, false).expect("blank");
            let (mut drawn, mut n) = (0, 0);
            while drawn < p.stitches.len() {
                n += step;
                let to = after_stitch(&p, n);
                if to > drawn {
                    preview::stroke_runs(&mut pixmap, &delta(&p, drawn, to), &view, false);
                    drawn = to;
                }
            }
            let off = pixmap
                .data()
                .chunks(4)
                .zip(whole.data().chunks(4))
                .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 64))
                .count();
            let pixels = (view.width * view.height) as usize;
            assert!(
                off * 200 < pixels,
                "step {step}: {off} of {pixels} pixels differ"
            );
        }
    }

    #[test]
    fn changed_finds_the_smallest_rectangle() {
        let (w, h) = (5, 4);
        let a = vec![255; w * h * 4];
        assert_eq!(changed(&a, &a, w), None);
        let mut b = a.clone();
        for (x, y) in [(1, 1), (3, 2)] {
            b[(y * w + x) * 4] = 0;
        }
        assert_eq!(changed(&a, &b, w), Some((1, 1, 3, 2)));
    }

    #[test]
    fn frame_count_rounds_up() {
        assert_eq!(frame_count(30_000, 300.0, 10.0), 1000);
        assert_eq!(frame_count(1001, 100.0, 10.0), 101);
        assert_eq!(frame_count(0, 100.0, 10.0), 1);
    }
}
