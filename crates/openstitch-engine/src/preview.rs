//! Draws generated stitches as a PNG with a pure-Rust rasterizer (tiny-skia, encoded by the
//! `png` crate): the Design Page outline, then each sewn run in its thread colour, optionally
//! with travel (jumps and stitches after a cut) dashed in grey.

use openstitch_core::{Command, StitchPattern};
use openstitch_design::Design;
pub use tiny_skia::Pixmap;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Rect, Stroke, StrokeDash, Transform,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunKind {
    Sewn,
    Travel,
}

/// A polyline of consecutive needle moves of one kind in one non-empty colour block.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub block: usize,
    pub kind: RunKind,
    pub points: Vec<(f64, f64)>,
}

/// Splits the stream into runs, into sewn and travel runs: a stitch right after a
/// trim or colour change is travel (the thread was cut), and block numbers skip empty blocks.
pub fn runs(pattern: &StitchPattern) -> Vec<Run> {
    let stitches = &pattern.stitches;
    let mut raw = Vec::with_capacity(stitches.len());
    let mut non_empty = std::collections::BTreeSet::new();
    let mut current = 0usize;
    for s in stitches {
        raw.push(current);
        match s.command {
            Command::ColorChange => current += 1,
            Command::End => {}
            _ => {
                non_empty.insert(current);
            }
        }
    }
    let compact: std::collections::HashMap<usize, usize> =
        non_empty.iter().enumerate().map(|(i, b)| (*b, i)).collect();

    let mut runs: Vec<Run> = Vec::new();
    let mut last: Option<(f64, f64)> = None;
    let mut thread_cut = true; // nothing is attached before the first stitch
    for (i, s) in stitches.iter().enumerate() {
        match s.command {
            Command::Stitch | Command::Jump => {
                let p = (s.x, s.y);
                let kind = if s.command == Command::Stitch && !thread_cut {
                    RunKind::Sewn
                } else {
                    RunKind::Travel
                };
                if let (Some(from), Some(&block)) = (last, compact.get(&raw[i])) {
                    match runs.last_mut() {
                        Some(run)
                            if run.block == block
                                && run.kind == kind
                                && run.points.last() == Some(&from) =>
                        {
                            run.points.push(p);
                        }
                        _ => runs.push(Run {
                            block,
                            kind,
                            points: vec![from, p],
                        }),
                    }
                }
                if s.command == Command::Stitch {
                    thread_cut = false;
                }
                last = Some(p);
            }
            Command::Trim | Command::ColorChange => thread_cut = true,
            Command::Stop | Command::End => {}
        }
    }
    runs
}

/// Where a picture shows the design: the frame's top left (0.1 mm), the scale to pixels and
/// the bitmap size. Fitted once to the whole design, it keeps every frame of a stitch-out
/// video in the same place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub min_x: f64,
    pub min_y: f64,
    pub scale: f64,
    pub width: u32,
    pub height: u32,
}

impl View {
    /// The Design Page and the stitches, with a 2 % margin, the longest side `max_pixels`.
    pub fn fit(design: &Design, pattern: &StitchPattern, max_pixels: u32) -> View {
        let (mut min_x, mut min_y, mut max_x, mut max_y) =
            (0.0, 0.0, design.page.width, design.page.height);
        if pattern.count(Command::Stitch) > 0 {
            let b = pattern.bounds();
            min_x = f64::min(min_x, b.min_x);
            min_y = f64::min(min_y, b.min_y);
            max_x = f64::max(max_x, b.max_x);
            max_y = f64::max(max_y, b.max_y);
        }
        let margin = (max_x - min_x).max(max_y - min_y) * 0.02;
        let (min_x, min_y) = (min_x - margin, min_y - margin);
        let (frame_w, frame_h) = (max_x + margin - min_x, max_y + margin - min_y);
        let scale = f64::from(max_pixels) / frame_w.max(frame_h).max(1.0);
        View {
            min_x,
            min_y,
            scale,
            width: ((frame_w * scale).round() as u32).max(1),
            height: ((frame_h * scale).round() as u32).max(1),
        }
    }
}

/// PNG bytes whose longest side is `max_pixels`.
pub fn png(
    design: &Design,
    pattern: &StitchPattern,
    max_pixels: u32,
    show_travel: bool,
) -> Result<Vec<u8>, String> {
    let view = View::fit(design, pattern, max_pixels);
    encode(&draw(design, pattern, &view, show_travel)?)
}

/// View ▸ Show settings for a rendered picture (#517): the filter and the two overlays.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShowArgs {
    pub filter: openstitch_design::view::ViewFilter,
    pub needle_points: bool,
    pub trim_marks: bool,
}

/// The arguments the picture tools share (`preview`, `ui.render`, `view_render`): `layers`,
/// `colorBy`, `only`, `solo`, `needlePoints`, `trimMarks`; each one given replaces the value in
/// `defaults`.
pub fn show_args(args: &serde_json::Value, defaults: ShowArgs) -> Result<ShowArgs, String> {
    use openstitch_design::view::{ColorBy, Layers};
    let mut s = defaults;
    if let Some(v) = args.get("layers") {
        s.filter.layers = v
            .as_str()
            .and_then(Layers::parse)
            .ok_or("layers takes both, underlay or top")?;
    }
    if let Some(v) = args.get("colorBy") {
        s.filter.color_by = v
            .as_str()
            .and_then(ColorBy::parse)
            .ok_or("colorBy takes thread, order or mono")?;
    }
    if let Some(v) = args.get("only") {
        s.filter.only = match v {
            serde_json::Value::Null => None,
            v => Some(
                serde_json::from_value::<Vec<u64>>(v.clone())
                    .map_err(|_| "only takes a list of object ids")?,
            ),
        };
    }
    if let Some(v) = args.get("solo") {
        s.filter.solo = match v {
            serde_json::Value::Null => None,
            v => Some(
                v.as_u64()
                    .filter(|n| *n <= 10_000)
                    .ok_or("solo takes a colour block number (0-based)")? as usize,
            ),
        };
    }
    for (key, flag) in [
        ("needlePoints", &mut s.needle_points),
        ("trimMarks", &mut s.trim_marks),
    ] {
        if let Some(v) = args.get(key) {
            *flag = v
                .as_bool()
                .ok_or_else(|| format!("{key} takes true or false"))?;
        }
    }
    Ok(s)
}

/// The design's picture as `show` sets it: the filtered pattern, needle points and trim marks.
/// `pattern` and `spans` are the compiled design ([`openstitch_design::Compiler::compile_traced`]).
pub fn png_shown(
    design: &Design,
    pattern: &StitchPattern,
    spans: &[(u64, std::ops::Range<usize>)],
    max_pixels: u32,
    show_travel: bool,
    show: &ShowArgs,
) -> Result<Vec<u8>, String> {
    use openstitch_design::view;
    let flags = if show.filter.layers == view::Layers::Both {
        Vec::new()
    } else {
        view::underlay_flags(design, pattern, spans)
    };
    let shown = view::apply(pattern, &show.filter, &flags, spans, [255, 255, 255]);
    let overlays = Overlays {
        needle_points: show.needle_points,
        trims: if show.trim_marks {
            view::trim_points(pattern)
                .iter()
                .map(|p| (p.x, p.y))
                .collect()
        } else {
            Vec::new()
        },
    };
    png_with(design, &shown, max_pixels, show_travel, &overlays)
}

/// View ▸ Show overlays drawn over the stitches (#517).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlays {
    /// A dot at every needle drop.
    pub needle_points: bool,
    /// Cut marks at these positions (0.1 mm): where the machine trims.
    pub trims: Vec<(f64, f64)>,
}

/// [`png`] with `overlays` drawn over the stitches.
pub fn png_with(
    design: &Design,
    pattern: &StitchPattern,
    max_pixels: u32,
    show_travel: bool,
    overlays: &Overlays,
) -> Result<Vec<u8>, String> {
    let view = View::fit(design, pattern, max_pixels);
    let mut pixmap = draw(design, pattern, &view, show_travel)?;
    draw_overlays(&mut pixmap, pattern, &view, overlays);
    encode(&pixmap)
}

/// Needle points in their thread colour and red cut marks at the trims.
pub fn draw_overlays(pixmap: &mut Pixmap, pattern: &StitchPattern, view: &View, o: &Overlays) {
    let transform = transform(view);
    let px = (1.0 / view.scale) as f32;
    let mut paint = Paint::default();
    if o.needle_points {
        let threads: Vec<_> = pattern.color_blocks().into_iter().map(|(t, _)| t).collect();
        let r = (1.6 * px).max(1.5);
        let mut block = 0;
        for s in &pattern.stitches {
            match s.command {
                Command::ColorChange => block += 1,
                Command::Stitch => {
                    let t = threads.get(block).copied().flatten();
                    paint.set_color_rgba8(
                        t.map_or(0, |t| t.red / 2),
                        t.map_or(0, |t| t.green / 2),
                        t.map_or(0, |t| t.blue / 2),
                        255,
                    );
                    if let Some(c) = PathBuilder::from_circle(s.x as f32, s.y as f32, r) {
                        pixmap.fill_path(&c, &paint, FillRule::Winding, transform, None);
                    }
                }
                _ => {}
            }
        }
    }
    paint.set_color_rgba8(0xd3, 0x2f, 0x2f, 255);
    let mark = Stroke {
        width: 1.5 * px,
        ..Stroke::default()
    };
    for &(x, y) in &o.trims {
        let (x, y) = (x as f32, y as f32);
        let mut pb = PathBuilder::new();
        pb.push_circle(x, y, 4.0 * px);
        pb.move_to(x - 5.0 * px, y - 5.0 * px);
        pb.line_to(x + 5.0 * px, y + 5.0 * px);
        if let Some(p) = pb.finish() {
            pixmap.stroke_path(&p, &paint, &mark, transform, None);
        }
    }
}

/// The design drawn into a white bitmap of `view`'s size.
pub fn draw(
    design: &Design,
    pattern: &StitchPattern,
    view: &View,
    show_travel: bool,
) -> Result<Pixmap, String> {
    let View {
        scale,
        width,
        height,
        ..
    } = *view;
    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| format!("cannot create a {width}x{height} bitmap"))?;
    pixmap.fill(Color::WHITE);
    let transform = transform(view);
    let hairline = (1.0 / scale) as f32;
    let mut paint = Paint::default();

    if let Some(page) = Rect::from_xywh(
        0.0,
        0.0,
        design.page.width as f32,
        design.page.height as f32,
    ) {
        paint.set_color_rgba8(191, 191, 191, 255);
        let stroke = Stroke {
            width: hairline,
            ..Stroke::default()
        };
        pixmap.stroke_path(
            &PathBuilder::from_rect(page),
            &paint,
            &stroke,
            transform,
            None,
        );
    }

    stroke_runs(&mut pixmap, pattern, view, show_travel);
    Ok(pixmap)
}

/// Design coordinates (0.1 mm, +y down, like the bitmap) to the view's pixels.
fn transform(view: &View) -> Transform {
    Transform::from_row(
        view.scale as f32,
        0.0,
        0.0,
        view.scale as f32,
        (-view.min_x * view.scale) as f32,
        (-view.min_y * view.scale) as f32,
    )
}

/// Draws the pattern's runs onto `pixmap` (no background, no page outline): what [`draw`]
/// adds to a blank page, and what a stitch-out video adds frame by frame.
pub fn stroke_runs(pixmap: &mut Pixmap, pattern: &StitchPattern, view: &View, show_travel: bool) {
    let scale = view.scale;
    let transform = transform(view);
    let hairline = (1.0 / scale) as f32;
    let mut paint = Paint::default();
    let threads: Vec<_> = pattern.color_blocks().into_iter().map(|(t, _)| t).collect();
    let thread_width = (1.2 / scale).max(4.0) as f32; // ~0.4 mm, at least 1.2 px
    let sewn = Stroke {
        width: thread_width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    let travel = Stroke {
        width: hairline,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        dash: StrokeDash::new(vec![4.0 * hairline, 3.0 * hairline], 0.0),
        ..Stroke::default()
    };
    for run in runs(pattern) {
        if run.kind == RunKind::Travel && !show_travel {
            continue;
        }
        let mut pb = PathBuilder::new();
        let mut points = run.points.iter();
        let Some(&(x, y)) = points.next() else {
            continue;
        };
        pb.move_to(x as f32, y as f32);
        for &(x, y) in points {
            pb.line_to(x as f32, y as f32);
        }
        let Some(path) = pb.finish() else { continue };
        let stroke = match run.kind {
            RunKind::Sewn => {
                let t = threads.get(run.block).copied().flatten();
                paint.set_color_rgba8(
                    t.map_or(0, |t| t.red),
                    t.map_or(0, |t| t.green),
                    t.map_or(0, |t| t.blue),
                    255,
                );
                &sewn
            }
            RunKind::Travel => {
                paint.set_color_rgba8(140, 140, 140, 255);
                &travel
            }
        };
        pixmap.stroke_path(&path, &paint, stroke, transform, None);
    }
}

/// The bitmap's RGB bytes (the background is opaque, so premultiplied RGBA equals straight RGB).
pub fn rgb(pixmap: &Pixmap) -> Vec<u8> {
    pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect()
}

/// Opaque RGB PNG.
fn encode(pixmap: &Pixmap) -> Result<Vec<u8>, String> {
    let rgb = rgb(pixmap);
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, pixmap.width(), pixmap.height());
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    writer
        .write_image_data(&rgb)
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    writer
        .finish()
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    Ok(out)
}
