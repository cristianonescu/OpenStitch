//! Image conversion commands over `openstitch-digitize`: Auto Punch, Cross Stitch, Photo
//! Stitch and the Design Center stages. Approximations of PE-DESIGN 11's wizards, specified
//! from its public guide only. The image is read through the sandbox like every other file.

use crate::commands::{CommandSpec, Effect, MM, ParamDoc, opt, read_file, req, spec, thread};
use crate::params::{CommandError, Params};
use crate::sandbox::Access;
use crate::session::EngineState;
use openstitch_core::Thread;
use openstitch_design::{Contour, DesignObject, LineType};
use openstitch_digitize::autopunch::{self, AutoPunch};
use openstitch_digitize::center::{
    self, FigureOptions, LineImageOptions, OpenLine, RegionSew, SewSettings,
};
use openstitch_digitize::cross::{self, CrossStitch, CrossType};
use openstitch_digitize::photo::{self, ColorSet, PhotoMode, PhotoStitch, PhotoStyle};
use openstitch_digitize::raster::MAX_FILE_BYTES;
use openstitch_digitize::regions::TraceOptions;
use openstitch_digitize::{DigitizeError, Fidelity, Frame, HIGH_COLORS, Mode, Raster};
use serde_json::json;

const IMAGE: ParamDoc = req(
    "path",
    "string",
    "Image file: PNG, JPEG or BMP (relative to the session directory)",
);

fn placement_docs(mut v: Vec<ParamDoc>) -> Vec<ParamDoc> {
    v.extend([
        IMAGE,
        opt(
            "width",
            "mm",
            "Width of the result (default: fit 80 % of the Design Page)",
        ),
        opt("x", "mm", "Left edge (default: centred on the Design Page)"),
        opt("y", "mm", "Top edge (default: centred on the Design Page)"),
    ]);
    v
}

fn failed(p: &Params, e: DigitizeError) -> CommandError {
    p.failed(e.to_string())
}

/// Reads and decodes the image, and places it on the Design Page.
fn load(s: &EngineState, p: &Params) -> Result<(Raster, Frame), CommandError> {
    let (_, data) = read_file(s, p, MAX_FILE_BYTES)?;
    let image = Raster::decode(&data).map_err(|e| failed(p, e))?;
    let (w, h) = (f64::from(image.width()), f64::from(image.height()));
    let page = s.design.page;
    let fit = (0.8 * page.width / MM / w).min(0.8 * page.height / MM / h) * w;
    let width = p.opt("width", fit.clamp(1.0, 2000.0), 1.0, 2000.0)?;
    let height = width * h / w;
    if height > 2000.0 {
        return Err(p.invalid("width", "makes the image taller than 2000 mm"));
    }
    let x = p.opt("x", (page.width / MM - width) / 2.0, -5000.0, 5000.0)?;
    let y = p.opt("y", (page.height / MM - height) / 2.0, -5000.0, 5000.0)?;
    let frame = Frame::new(x, y, width, image.width());
    Ok((image, frame))
}

fn add_all(s: &mut EngineState, objects: Vec<DesignObject>) -> Vec<u64> {
    objects.into_iter().map(|o| s.design.add(o)).collect()
}

fn hexes(threads: &[Thread]) -> Vec<String> {
    threads.iter().map(Thread::hex).collect()
}

/// The `compatMode` parameter, shared with the region attribute of the same name: PE-DESIGN's
/// method (`peDesign`, the default) or OpenStitch's improved one (`openStitch`).
fn compat_mode(p: &Params) -> Result<Mode, CommandError> {
    Ok(
        match choice(p, "compatMode", "peDesign", &["peDesign", "openStitch"])? {
            "openStitch" => Mode::OpenStitch,
            _ => Mode::PeDesign,
        },
    )
}

/// The `fidelity` parameter (B16.1), shared by the image wizards that have an OpenStitch mode:
/// `standard` (the default) or `high`. High needs `compatMode: openStitch`; PE-DESIGN mode has
/// no fidelity.
fn fidelity(p: &Params, mode: Mode) -> Result<Fidelity, CommandError> {
    match (
        choice(p, "fidelity", "standard", &["standard", "high"])?,
        mode,
    ) {
        ("high", Mode::OpenStitch) => Ok(Fidelity::High),
        ("high", Mode::PeDesign) => Err(p.invalid("fidelity", "high needs compatMode openStitch")),
        _ => Ok(Fidelity::Standard),
    }
}

/// The `fidelity` parameter's documentation.
fn fidelity_doc() -> ParamDoc {
    opt(
        "fidelity",
        "standard|high",
        "OpenStitch mode: standard (default) or high; high lets the colour count reach 24 by default and turns on the costlier passes, never raising the needle density (needs compatMode openStitch)",
    )
}

fn choice<'a>(
    p: &Params,
    key: &str,
    default: &'a str,
    allowed: &[&'a str],
) -> Result<&'a str, CommandError> {
    let v = p.string(key, Some(default))?;
    allowed
        .iter()
        .find(|a| a.eq_ignore_ascii_case(&v))
        .copied()
        .ok_or_else(|| p.invalid(key, format!("must be one of {}", allowed.join(", "))))
}

fn count(p: &Params, key: &str, default: i64, lo: i64, hi: i64) -> Result<usize, CommandError> {
    Ok(p.int(key, Some(default), lo, hi)? as usize)
}

fn trace_options(p: &Params, default_simplify: f64) -> Result<TraceOptions, CommandError> {
    Ok(TraceOptions {
        simplify: p.opt("simplify", default_simplify, 0.2, 5.0)?,
        curves: p.bool("curves", Some(true))?,
        ..TraceOptions::default()
    })
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "digitize.autoPunch",
            "Auto Punch (approximation): convert an image's colour areas into fill and satin shapes",
            Effect::Edit,
            placement_docs(vec![
                opt(
                    "colors",
                    "int 0-64",
                    "Thread colours to reduce to; 0 = automatic from the image, as PE-DESIGN fills in Max. Number of Colors (default 0)",
                ),
                opt(
                    "resolution",
                    "int 64-2048",
                    "Working resolution, longest side in pixels (default 400)",
                ),
                opt(
                    "minArea",
                    "mm",
                    "Small-region parameter, mm²: regions of at most max(4, round(minArea / mmPerPx²)) pixels merge into a neighbour (default 1; 3 in OpenStitch mode)",
                ),
                opt(
                    "smooth",
                    "int 0-5",
                    "Extra majority-filter passes on the labels after the 3×3 modal colour census (default 0)",
                ),
                opt(
                    "simplify",
                    "factor",
                    "Outline simplification tolerance, pixels 0.2-5 (default 0.7)",
                ),
                opt(
                    "curves",
                    "bool",
                    "Fit Bézier curves to smooth outlines (default true)",
                ),
                opt(
                    "satinMaxWidth",
                    "mm",
                    "Regions whose stroke width is at most this become satin across the stroke (default 6.5; PE-DESIGN keeps 6.1 mm satin, fills 7.1 mm)",
                ),
                opt(
                    "satinMinWidth",
                    "mm",
                    "Strokes narrower than this are fill, not satin (default 0.7; PE-DESIGN fills 0.5 mm, satins 0.75 mm)",
                ),
                opt(
                    "skipBackground",
                    "bool",
                    "Leave out the colour covering most of the border (default true). OpenStitch mode without this parameter: the background is sewn as part of the picture unless it is the fabric's colour (within ΔE00 5); true leaves it out, false sews it whatever its colour",
                ),
                opt(
                    "fabric",
                    "color",
                    "OpenStitch mode without skipBackground: the fabric colour the background is compared with, \"#rrggbb\", a PEC chart name or number (default white)",
                ),
                opt(
                    "compatMode",
                    "peDesign|openStitch",
                    "peDesign: as PE-DESIGN 11.50 (default); openStitch: colours reduced and matched to threads by appearance (CIELAB k-means, CIEDE2000), regions under 3 mm² merged (docs/research/beyond/image-digitizing.md)",
                ),
                fidelity_doc(),
            ]),
            |s, p| {
                let (image, frame) = load(s, p)?;
                let mode = compat_mode(p)?;
                // OpenStitch mode also cleans up: specks under 3 mm² merge, unless the caller
                // sets the area.
                let min_area = match mode {
                    Mode::PeDesign => 1.0,
                    Mode::OpenStitch => 3.0,
                };
                let mut options = AutoPunch {
                    colors: count(p, "colors", AutoPunch::default().colors as i64, 0, 64)?,
                    resolution: count(p, "resolution", 400, 64, 2048)? as u32,
                    min_area_mm2: p.opt("minArea", min_area, 0.0, 1000.0)?,
                    smooth: count(p, "smooth", 0, 0, 5)? as u32,
                    trace: trace_options(p, 0.7)?,
                    satin_max_mm: p.opt("satinMaxWidth", 6.5, 1.0, 12.0)?,
                    satin_min_mm: p.opt("satinMinWidth", 0.7, 0.0, 6.0)?,
                    skip_background: p.bool("skipBackground", Some(true))?,
                    mode,
                    fidelity: fidelity(p, mode)?,
                    ..AutoPunch::default()
                };
                // OpenStitch mode sews anything in the picture: without skipBackground, only a
                // background of the fabric's colour is left out.
                if mode == Mode::OpenStitch && p.get("skipBackground").is_none() {
                    let f = thread(p, "fabric")?.unwrap_or(Thread::rgb(0xffffff));
                    options.fabric = Some([f.red, f.green, f.blue]);
                }
                let out = autopunch::convert(&image, &frame, &options).map_err(|e| failed(p, e))?;
                let regions = out.objects.len();
                Ok(json!({
                    "ids": add_all(s, out.objects),
                    "colors": hexes(&out.threads),
                    "regions": regions,
                    "background": out.background.as_ref().map(Thread::hex),
                }))
            },
        ),
        spec(
            "digitize.crossStitch",
            "Cross Stitch from an image: one cross per grid cell, one stitch object per colour",
            Effect::Edit,
            placement_docs(vec![
                opt("cellSize", "mm", "Grid cell side, 1-10 (default 2.5)"),
                opt(
                    "colors",
                    "int 0-64",
                    "Thread colours; 0 = automatic from the image, as PE-DESIGN fills in Max. Number of Colors (default 0)",
                ),
                opt(
                    "type",
                    "full|half",
                    "Full crosses or half crosses (default full)",
                ),
                opt(
                    "times",
                    "single|triple",
                    "Each diagonal sewn once or three times (default single)",
                ),
                opt(
                    "skipBackground",
                    "bool",
                    "Leave out the colour covering most of the border (default true)",
                ),
                opt(
                    "compatMode",
                    "peDesign|openStitch",
                    "peDesign: as PE-DESIGN 11.50 (default); openStitch: each cell takes its mean colour in CIELAB, threads by k-means and CIEDE2000, and crosses are filled (docs/research/beyond/image-similarity-plan.md, T14)",
                ),
                opt(
                    "fill",
                    "bool",
                    "OpenStitch mode: cover each cell with parallel diagonals about 0.3 mm apart, at most 2.1 needle points per mm² over the design, instead of one cross; times then has no effect (default true)",
                ),
                opt(
                    "partCrosses",
                    "bool",
                    "OpenStitch mode: a cell whose halves along a diagonal are clearly different colours becomes two part-crosses, three-quarter and quarter (default false)",
                ),
            ]),
            |s, p| {
                let (image, frame) = load(s, p)?;
                let options = CrossStitch {
                    cell_mm: p.opt("cellSize", CrossStitch::CELL_MM, 1.0, 10.0)?,
                    colors: count(p, "colors", CrossStitch::COLORS as i64, 0, 64)?,
                    kind: match choice(p, "type", "full", &["full", "half"])? {
                        "half" => CrossType::Half,
                        _ => CrossType::Full,
                    },
                    triple: choice(p, "times", "single", &["single", "triple"])? == "triple",
                    skip_background: p.bool("skipBackground", Some(true))?,
                    mode: compat_mode(p)?,
                    fill: p.bool("fill", Some(true))?,
                    part_crosses: p.bool("partCrosses", Some(false))?,
                };
                let out = cross::convert(&image, &frame, &options).map_err(|e| failed(p, e))?;
                Ok(json!({
                    "colors": hexes(&out.threads),
                    "cells": out.cells,
                    "columns": out.columns,
                    "rows": out.rows,
                    "background": out.background.as_ref().map(Thread::hex),
                    "ids": add_all(s, out.objects),
                }))
            },
        ),
        spec(
            "digitize.photoStitch",
            "Photo Stitch (approximation): tone-modulated fill rows (style fill, after Photo Stitch 1) or a line screen, one fixed angle per thread (style lines, Photo Stitch 2)",
            Effect::Edit,
            placement_docs(vec![
                opt(
                    "style",
                    "fill|lines",
                    "Fill rows (after Photo Stitch 1) or a line screen (Photo Stitch 2); default fill",
                ),
                opt(
                    "mode",
                    "color|mono",
                    "Several thread colours or one (default color)",
                ),
                opt(
                    "colors",
                    "int 1-32",
                    "Thread colours in colour mode (default 10 for fill, PE-DESIGN's Max. Number of Colors, 24 at High fidelity; 4 for lines, its Color Set)",
                ),
                opt("thread", "color", "Thread in mono mode (default Black)"),
                opt(
                    "fabric",
                    "color",
                    "Fabric colour shown where nothing is sewn (default #ffffff)",
                ),
                opt(
                    "sewPage",
                    "bool",
                    "Style fill: sew the fabric colour as a thread too (PE-DESIGN's Sew Page Color; default true)",
                ),
                opt(
                    "rowSpacing",
                    "mm",
                    "Distance between rows (fill) or the Line interval (lines), 0.2-5 (default 0.25 fill, 2.0 lines)",
                ),
                opt(
                    "stitchLength",
                    "mm",
                    "Fill stitch length (Run Pitch), or the straight run where a line's colour is not needed, 0.3-7 (default 3 fill, 2.5 lines)",
                ),
                opt(
                    "maxDensity",
                    "lines/mm",
                    "Lines: Max. Density, zigzag lines per mm at full colour, 0.5-10 (default 4.5)",
                ),
                opt(
                    "colorSet",
                    "auto|cmyk|rgbk|cryk|bmyk",
                    "Lines, colour mode: the Color Set; a named set is four fixed threads in its order (default auto)",
                ),
                opt(
                    "compatMode",
                    "peDesign|openStitch",
                    "Colour mode: peDesign as PE-DESIGN 11.50 (default); openStitch picks the threads in CIELAB (k-means, CIEDE2000), dithers in CIELAB (fill) or mixes the line screen in linear light (lines)",
                ),
                opt(
                    "direction",
                    "degrees",
                    "Lines: Direction, turns every thread's screen angle (0/45/90/135), -180-180 (default 0)",
                ),
                fidelity_doc(),
            ]),
            |s, p| {
                let (image, frame) = load(s, p)?;
                let compat = compat_mode(p)?;
                let fidelity = fidelity(p, compat)?;
                let style = match choice(p, "style", "fill", &["fill", "lines"])? {
                    "lines" => PhotoStyle::Lines,
                    _ => PhotoStyle::Fill,
                };
                let mode = match choice(p, "mode", "color", &["color", "mono"])? {
                    "mono" => PhotoMode::Mono(thread(p, "thread")?.unwrap_or(Thread::rgb(0))),
                    _ => PhotoMode::Color(count(
                        p,
                        "colors",
                        match (style, fidelity) {
                            (PhotoStyle::Lines, _) => photo::LINE_COLORS,
                            (PhotoStyle::Fill, Fidelity::High) => HIGH_COLORS,
                            (PhotoStyle::Fill, Fidelity::Standard) => photo::COLORS,
                        } as i64,
                        1,
                        32,
                    )?),
                };
                let mut options = PhotoStitch::new(style, mode);
                if let Some(f) = thread(p, "fabric")? {
                    options.fabric = [f.red, f.green, f.blue];
                }
                options.sew_page = p.bool("sewPage", Some(options.sew_page))?;
                options.row_spacing_mm = p.opt("rowSpacing", options.row_spacing_mm, 0.2, 5.0)?;
                options.stitch_mm = p.opt("stitchLength", options.stitch_mm, 0.3, 7.0)?;
                options.max_density = p.opt("maxDensity", options.max_density, 0.5, 10.0)?;
                options.direction_deg = p.opt("direction", options.direction_deg, -180.0, 180.0)?;
                options.color_set = match choice(
                    p,
                    "colorSet",
                    "auto",
                    &["auto", "cmyk", "rgbk", "cryk", "bmyk"],
                )? {
                    "cmyk" => ColorSet::Cmyk,
                    "rgbk" => ColorSet::Rgbk,
                    "cryk" => ColorSet::Cryk,
                    "bmyk" => ColorSet::Bmyk,
                    _ => ColorSet::Auto,
                };
                options.compat = compat;
                options.fidelity = fidelity;
                let out = photo::convert(&image, &frame, &options).map_err(|e| failed(p, e))?;
                Ok(json!({
                    "colors": hexes(&out.threads),
                    "stitches": out.stitches,
                    "ids": add_all(s, out.objects),
                }))
            },
        ),
        spec(
            "digitize.lineImage",
            "Design Center stage 1: the image's line pixels; optionally writes the line image as PNG",
            Effect::File,
            vec![
                IMAGE,
                opt(
                    "threshold",
                    "int 0-255",
                    "Pixels darker than this are lines (default 128)",
                ),
                opt(
                    "resolution",
                    "int 64-2048",
                    "Working resolution, longest side in pixels (default 600)",
                ),
                opt("out", "string", "Write the line image to this PNG file"),
            ],
            |s, p| {
                let (_, data) = read_file(s, p, MAX_FILE_BYTES)?;
                let image = Raster::decode(&data).map_err(|e| failed(p, e))?;
                let li = center::line_image(&image, &line_options(p)?);
                let mut r = json!({
                    "width": li.width, "height": li.height, "linePixels": li.line_pixels(),
                });
                if p.has("out") {
                    let path = s.resolve(&p.string("out", None)?, Access::Write, &p.command)?;
                    let png = li.to_raster().encode_png().map_err(|e| failed(p, e))?;
                    std::fs::write(&path, png).map_err(|e| p.failed(e.to_string()))?;
                    r["out"] = json!(path.display().to_string());
                }
                Ok(r)
            },
        ),
        spec(
            "digitize.figure",
            "Design Center stages 2-3: lines and enclosed regions of the line image, in mm",
            Effect::Read,
            placement_docs(figure_docs(vec![])),
            |s, p| {
                let figure = figure(s, p, Mode::PeDesign)?;
                let points = |c: &Contour| -> Vec<[f64; 2]> {
                    c.path()
                        .iter()
                        .map(|q| [round(q.x / MM), round(q.y / MM)])
                        .collect()
                };
                Ok(json!({
                    "lineWidthMM": round(figure.line_width_mm),
                    "lines": figure.lines.iter().map(|c| json!({"closed": c.closed, "points": points(c)})).collect::<Vec<_>>(),
                    "regions": figure.regions.iter().map(|r| json!({
                        "color": Thread::rgb(u32::from(r.color[0]) << 16 | u32::from(r.color[1]) << 8 | u32::from(r.color[2])).hex(),
                        "thread": r.thread.hex(),
                        "contours": r.contours.iter().map(points).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                }))
            },
        ),
        spec(
            "digitize.designCenter",
            "Design Center (approximation): image → line image → figure → shapes with sew settings",
            Effect::Edit,
            placement_docs(figure_docs(vec![
                opt(
                    "lineSew",
                    "running|triple|zigzag|none",
                    "Line sew type (default running)",
                ),
                opt("lineColor", "color", "Line thread (default Black)"),
                opt(
                    "lineWidth",
                    "mm",
                    "Zigzag width, 1-6 (default: the measured stroke width)",
                ),
                opt(
                    "regionSew",
                    "auto|fill|satin|none",
                    "Region sew type; auto fills regions that are not near white (default auto)",
                ),
                opt(
                    "regionColor",
                    "color",
                    "Region thread (default: each region's image colour)",
                ),
                opt(
                    "compatMode",
                    "peDesign|openStitch",
                    "peDesign: as before (default); openStitch: the threshold by Otsu's method unless set; unless lineSew is set, strokes at least 0.6 mm wide as parallel running lines 0.3 mm apart across 1.2 × the measured stroke width, thinner ones as triple running stitch; region threads by CIEDE2000 (docs/research/beyond/image-similarity-plan.md, Phase 5)",
                ),
            ])),
            |s, p| {
                let mode = compat_mode(p)?;
                let mut figure = figure(s, p, mode)?;
                // OpenStitch mode chooses the line sewing from the strokes unless it is set.
                let auto_line = if mode == Mode::OpenStitch && !p.has("lineSew") {
                    let (lines, kind) = match center::open_stitch_line(&figure) {
                        OpenLine::Parallel { width_mm, passes } => (
                            center::parallel_lines(&figure, width_mm, passes),
                            LineType::Running,
                        ),
                        OpenLine::Triple => {
                            (center::parallel_lines(&figure, 0.0, 1), LineType::Triple)
                        }
                    };
                    figure.lines = lines;
                    Some(kind)
                } else {
                    None
                };
                let settings = SewSettings {
                    line: match choice(
                        p,
                        "lineSew",
                        "running",
                        &["running", "triple", "zigzag", "none"],
                    )? {
                        _ if auto_line.is_some() => auto_line,
                        "triple" => Some(LineType::Triple),
                        "zigzag" => Some(LineType::Zigzag),
                        "none" => None,
                        _ => Some(LineType::Running),
                    },
                    line_thread: thread(p, "lineColor")?.unwrap_or(Thread::rgb(0)),
                    line_width_mm: if p.has("lineWidth") {
                        Some(p.req("lineWidth", 1.0, 6.0)?)
                    } else {
                        None
                    },
                    region: match choice(
                        p,
                        "regionSew",
                        "auto",
                        &["auto", "fill", "satin", "none"],
                    )? {
                        "fill" => RegionSew::Fill,
                        "satin" => RegionSew::Satin,
                        "none" => RegionSew::None,
                        _ => RegionSew::Auto,
                    },
                    region_thread: thread(p, "regionColor")?,
                };
                let objects = center::sew_with(&figure, &settings, mode);
                if objects.is_empty() {
                    return Err(p.failed("nothing to sew with these settings"));
                }
                Ok(json!({
                    "lines": figure.lines.len(),
                    "regions": figure.regions.len(),
                    "ids": add_all(s, objects),
                }))
            },
        ),
    ]
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn figure_docs(mut v: Vec<ParamDoc>) -> Vec<ParamDoc> {
    v.extend([
        opt(
            "threshold",
            "int 0-255",
            "Pixels darker than this are lines (default 128; designCenter in OpenStitch mode: from the image by Otsu's method)",
        ),
        opt(
            "resolution",
            "int 64-2048",
            "Working resolution, longest side in pixels (default 600)",
        ),
        opt(
            "minLineLength",
            "mm",
            "Open lines shorter than this are dropped (default 1)",
        ),
        opt(
            "minRegionArea",
            "mm",
            "Enclosed areas below this many mm² are not regions (default 4)",
        ),
        opt(
            "simplify",
            "factor",
            "Line simplification tolerance, pixels 0.2-5 (default 1)",
        ),
        opt("curves", "bool", "Fit Bézier curves (default true)"),
    ]);
    v
}

fn line_options(p: &Params) -> Result<LineImageOptions, CommandError> {
    Ok(LineImageOptions {
        threshold: p.int("threshold", Some(128), 0, 255)? as f64,
        resolution: count(p, "resolution", 600, 64, 2048)? as u32,
    })
}

/// Stages 1–3; OpenStitch mode takes the threshold from the image (Otsu) unless it is set.
fn figure(s: &EngineState, p: &Params, mode: Mode) -> Result<center::Figure, CommandError> {
    let (image, frame) = load(s, p)?;
    let mut lo = line_options(p)?;
    if mode == Mode::OpenStitch && !p.has("threshold") {
        lo.threshold = center::otsu_threshold(&image, lo.resolution);
    }
    let li = center::line_image(&image, &lo);
    let options = FigureOptions {
        min_line_mm: p.opt("minLineLength", 1.0, 0.0, 100.0)?,
        min_region_mm2: p.opt("minRegionArea", 4.0, 0.0, 10_000.0)?,
        trace: trace_options(p, 1.0)?,
    };
    center::figure(&image, &li, &frame, &options).map_err(|e| failed(p, e))
}
