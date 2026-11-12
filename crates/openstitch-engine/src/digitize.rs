//! Image conversion commands over `openstitch-digitize`: Auto Punch, Cross Stitch, Photo
//! Stitch and the Design Center stages. Approximations of PE-DESIGN 11's wizards, specified
//! from its public guide only. The image is read through the sandbox like every other file.

use crate::commands::{CommandSpec, Effect, MM, ParamDoc, opt, read_file, req, spec, thread};
use crate::params::{CommandError, Params};
use crate::sandbox::Access;
use crate::session::EngineState;
use openstitch_core::Thread;
use openstitch_design::{Contour, DesignObject, LineType};
use openstitch_digitize::autopunch::{self, AngleRule, AutoPunch, Override, Stitch};
use openstitch_digitize::center::{
    self, FigureOptions, LineImageOptions, OpenLine, RegionSew, SewSettings,
};
use openstitch_digitize::cross::{self, CrossStitch, CrossType};
use openstitch_digitize::grabcut::{self, Marks, Seed, Stroke};
use openstitch_digitize::photo::{self, ColorSet, PhotoMode, PhotoStitch, PhotoStyle, Travel};
use openstitch_digitize::raster::MAX_FILE_BYTES;
use openstitch_digitize::regions::TraceOptions;
use openstitch_digitize::{DigitizeError, Fidelity, Frame, HIGH_COLORS, Mode, Raster};
use serde_json::{Value, json};

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

/// The threads chosen, with code, brand and PEC number (#798).
fn described(threads: &[Thread]) -> Vec<Value> {
    threads.iter().map(crate::thread_match::describe).collect()
}

/// The `compatMode` parameter, shared with the region attribute of the same name: PE-DESIGN's
/// method (`peDesign`, the default) or OpenStitch's improved one (`openStitch`).
/// Auto Punch's `isolateSubject` and `subjectRect`: the graph cut's seed, or `None` when off.
fn subject_seed(p: &Params) -> Result<Option<Seed>, CommandError> {
    let on = p.bool("isolateSubject", Some(false))?;
    if !p.has("subjectRect") {
        return Ok(on.then_some(Seed::Border));
    }
    let bad = || {
        p.invalid(
            "subjectRect",
            "must be [left, top, right, bottom], each 0-1",
        )
    };
    let a = p.array("subjectRect")?;
    if a.len() != 4 {
        return Err(bad());
    }
    let mut r = [0.0; 4];
    for (v, x) in r.iter_mut().zip(a) {
        *v = x
            .as_f64()
            .filter(|f| (0.0..=1.0).contains(f))
            .ok_or_else(bad)?;
    }
    if r[2] <= r[0] || r[3] <= r[1] {
        return Err(p.invalid("subjectRect", "right and bottom must exceed left and top"));
    }
    if !on {
        return Err(p.invalid("subjectRect", "needs isolateSubject: true"));
    }
    Ok(Some(Seed::Rect(r)))
}

/// Auto Punch's `subjectPoint` (#490): a click on the subject, `[x, y]` in fractions of the
/// image; it needs `isolateSubject: true` and takes the place of `subjectRect`.
fn subject_point(p: &Params) -> Result<Option<[f64; 2]>, CommandError> {
    if !p.has("subjectPoint") {
        return Ok(None);
    }
    if !p.bool("isolateSubject", Some(false))? {
        return Err(p.invalid("subjectPoint", "needs isolateSubject: true"));
    }
    if p.has("subjectRect") {
        return Err(p.invalid("subjectPoint", "takes the place of subjectRect; give one"));
    }
    point_param(p, "subjectPoint").map(Some)
}

/// An `[x, y]` parameter in fractions of the image (0-1).
fn point_param(p: &Params, key: &str) -> Result<[f64; 2], CommandError> {
    let bad = || p.invalid(key, "must be [x, y], each 0-1 (fractions of the image)");
    let a = p.array(key)?;
    let f = |v: &Value| v.as_f64().filter(|f| (0.0..=1.0).contains(f));
    match &a[..] {
        [x, y] => Ok([f(x).ok_or_else(bad)?, f(y).ok_or_else(bad)?]),
        _ => Err(bad()),
    }
}

/// `digitize.segment`'s strokes: as Auto Punch's, without `isolateSubject`.
fn segment_strokes(p: &Params, key: &str) -> Result<Vec<Stroke>, CommandError> {
    if !p.has(key) {
        return Ok(Vec::new());
    }
    let mut values = p.values().clone();
    values.insert("isolateSubject".into(), json!(true));
    strokes(&Params::new(&p.command, Some(&Value::Object(values)))?, key)
}

/// Most strokes, and points per stroke, in `subjectStrokes` or `backgroundStrokes`.
const MAX_STROKES: usize = 200;
const MAX_STROKE_POINTS: usize = 2000;

/// Auto Punch's `subjectStrokes` or `backgroundStrokes` (#350): `[{points: [[x, y], ...],
/// width}]` in fractions of the image; they need `isolateSubject: true`.
fn strokes(p: &Params, key: &str) -> Result<Vec<Stroke>, CommandError> {
    if !p.has(key) {
        return Ok(Vec::new());
    }
    if !p.bool("isolateSubject", Some(false))? {
        return Err(p.invalid(key, "needs isolateSubject: true"));
    }
    let bad = || {
        p.invalid(
            key,
            format!(
                "must be at most {MAX_STROKES} strokes {{points: [[x, y], ...], width}}, with \
                 1-{MAX_STROKE_POINTS} points, x and y 0-1 and width 0.002-0.25"
            ),
        )
    };
    let list = p.array(key)?;
    if list.len() > MAX_STROKES {
        return Err(bad());
    }
    list.iter()
        .map(|s| {
            let width = s["width"]
                .as_f64()
                .filter(|w| (0.002..=0.25).contains(w))
                .ok_or_else(bad)?;
            let points = s["points"].as_array().ok_or_else(bad)?;
            if points.is_empty() || points.len() > MAX_STROKE_POINTS {
                return Err(bad());
            }
            let points = points
                .iter()
                .map(|q| {
                    let xy = q.as_array().filter(|a| a.len() == 2).ok_or_else(bad)?;
                    let f = |v: &Value| v.as_f64().filter(|f| (0.0..=1.0).contains(f));
                    Ok([f(&xy[0]).ok_or_else(bad)?, f(&xy[1]).ok_or_else(bad)?])
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Stroke { points, width })
        })
        .collect()
}

/// Most Auto Punch overrides one command takes.
const MAX_OVERRIDES: usize = 5000;

/// Auto Punch's `overrides` (#489): `[{region, type?, angle?}]`.
fn overrides(p: &Params) -> Result<Vec<Override>, CommandError> {
    if !p.has("overrides") {
        return Ok(Vec::new());
    }
    let bad = || {
        p.invalid(
            "overrides",
            format!(
                "must be at most {MAX_OVERRIDES} entries {{region, type?: fill|satin|line|skip, \
                 angle?: degrees -180-180}}, each with a type or an angle"
            ),
        )
    };
    let list = p.array("overrides")?;
    if list.len() > MAX_OVERRIDES {
        return Err(bad());
    }
    list.iter()
        .map(|o| {
            let region = o["region"].as_u64().ok_or_else(bad)? as usize;
            let stitch = match o.get("type") {
                None | Some(Value::Null) => None,
                Some(t) => Some(t.as_str().and_then(Stitch::parse).ok_or_else(bad)?),
            };
            let angle = match o.get("angle") {
                None | Some(Value::Null) => None,
                Some(a) => Some(
                    a.as_f64()
                        .filter(|a| (-180.0..=180.0).contains(a))
                        .ok_or_else(bad)?,
                ),
            };
            if stitch.is_none() && angle.is_none() {
                return Err(bad());
            }
            Ok(Override {
                region,
                stitch,
                angle,
            })
        })
        .collect()
}

/// Auto Punch's objects as an SVG document in mm (#489), with how many filled paths (regions)
/// and stroked paths (lines) it holds. Outlines are flattened to straight segments; a region's
/// holes are its further contours, filled even-odd.
fn outlines_svg(objects: &[DesignObject]) -> (String, usize, usize) {
    use std::fmt::Write as _;
    let mm = |v: f64| round(v / MM);
    let all = objects
        .iter()
        .flat_map(|o| o.contours.iter())
        .flat_map(|c| c.path());
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for q in all {
        (x0, y0, x1, y1) = (x0.min(q.x), y0.min(q.y), x1.max(q.x), y1.max(q.y));
    }
    if x0 > x1 {
        (x0, y0, x1, y1) = (0.0, 0.0, 0.0, 0.0);
    }
    let (w, h) = (mm(x1 - x0).max(0.01), mm(y1 - y0).max(0.01));
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"{} {} {w} {h}\">\n",
        mm(x0),
        mm(y0)
    );
    let (mut fills, mut lines) = (0, 0);
    for o in objects {
        let mut d = String::new();
        for c in &o.contours {
            for (i, q) in c.path().iter().enumerate() {
                let _ = write!(
                    d,
                    "{}{} {} ",
                    if i == 0 { "M" } else { "L" },
                    mm(q.x),
                    mm(q.y)
                );
            }
            if c.closed {
                d.push_str("Z ");
            }
        }
        let d = d.trim_end();
        if d.is_empty() {
            continue;
        }
        if let Some(r) = &o.region {
            fills += 1;
            let _ = writeln!(
                svg,
                "  <path fill=\"{}\" fill-rule=\"evenodd\" stroke=\"none\" d=\"{d}\"/>",
                r.thread.hex()
            );
        } else if let Some(l) = &o.line {
            lines += 1;
            let _ = writeln!(
                svg,
                "  <path fill=\"none\" stroke=\"{}\" stroke-width=\"0.4\" d=\"{d}\"/>",
                l.thread.hex()
            );
        }
    }
    svg.push_str("</svg>\n");
    (svg, fills, lines)
}

/// Auto Punch's region plan as JSON (#489).
fn plan_json(out: &autopunch::Converted, options: &AutoPunch) -> Value {
    let round = |v: f64| (v * 100.0).round() / 100.0;
    Value::Array(
        out.regions
            .iter()
            .map(|r| {
                json!({
                    "region": r.id,
                    "color": r.thread.hex(),
                    "thread": r.thread.name,
                    // The code on the region's spool, its brand and PEC number (#798).
                    "code": openstitch_core::thread_code(&r.thread),
                    "brand": r.thread.brand,
                    "pecIndex": openstitch_core::pec_index(&r.thread),
                    "areaMm2": round(r.area_mm2),
                    "widthMm": round(r.width_mm),
                    "type": r.stitch.name(),
                    "reason": r.reason.code(),
                    "why": r.why(options),
                    "angle": r.angle.map(round),
                })
            })
            .collect(),
    )
}

fn compat_mode(p: &Params) -> Result<Mode, CommandError> {
    Ok(
        match choice(p, "compatMode", "peDesign", &["peDesign", "openStitch"])? {
            "openStitch" => Mode::OpenStitch,
            _ => Mode::PeDesign,
        },
    )
}

/// The threads OpenStitch mode chooses from (B16.1 T5), from the `threads` parameter: `brother`
/// (the PEC chart), `all` (every chart built into OpenStitch, other brands' included), a
/// built-in chart's id (`thread.chartList`, or `newBrothread`, `madeiraPolyneon` and the other
/// [`openstitch_core::brands::brand_charts`]), or the name of one of the user's thread charts
/// ("My Threads"). Without `threads`, OpenStitch mode takes the user's default chart
/// (`threads.defaultChart`: New Brothread unless they chose another, #810), as the editor's
/// wizard does; PE-DESIGN mode keeps the PEC chart.
/// Empty means the PEC chart. Anything but `brother` needs `compatMode: openStitch`.
fn thread_source(s: &EngineState, p: &Params, mode: Mode) -> Result<Vec<Thread>, CommandError> {
    let name = if p.has("threads") {
        p.string("threads", None)?
    } else if mode == Mode::PeDesign {
        "brother".to_string()
    } else {
        match s.user.default_chart() {
            crate::user::ChartRef::Builtin(id) | crate::user::ChartRef::Chart(id) => id,
        }
    };
    if name == "brother" {
        return Ok(Vec::new());
    }
    if mode == Mode::PeDesign {
        return Err(p.invalid("threads", "a thread source needs compatMode openStitch"));
    }
    let builtin = crate::user::builtin_charts();
    let brands = openstitch_core::brands::brand_charts();
    let charts: Vec<Vec<Thread>> = if name == "all" {
        builtin
            .iter()
            .map(|c| c.2.iter().flatten().cloned().collect())
            .chain(brands.iter().map(|c| c.2.to_vec()))
            .collect()
    } else if let Some(c) = builtin.iter().find(|c| c.0 == name) {
        vec![c.2.iter().flatten().cloned().collect()]
    } else if let Some(c) = brands.iter().find(|c| c.0 == name) {
        vec![c.2.to_vec()]
    } else if let Some(c) = s.user.thread_charts.iter().find(|c| c.name == name) {
        vec![c.threads.clone()]
    } else {
        return Err(p.not_found(format!("thread chart '{name}'")));
    };
    // Each colour once per brand, first chart first.
    let mut out: Vec<Thread> = Vec::new();
    for t in charts.into_iter().flatten() {
        if !out
            .iter()
            .any(|o| o.rgb_value() == t.rgb_value() && o.brand == t.brand)
        {
            out.push(t);
        }
    }
    Ok(out)
}

/// The threads of `chosen` not in the user's chart named by the `owned` parameter (B16.1 T5:
/// High fidelity may ask for threads the user does not own, with this shopping list). A thread
/// is owned when the chart has one of the same brand and code, or of the same colour. `None`
/// when `owned` is not given.
fn to_buy(s: &EngineState, p: &Params, chosen: &[Thread]) -> Result<Option<Value>, CommandError> {
    if !p.has("owned") {
        return Ok(None);
    }
    let name = p.string("owned", None)?;
    let owned = s
        .user
        .thread_charts
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| p.not_found(format!("thread chart '{name}'")))?;
    let has = |t: &Thread| {
        owned.threads.iter().any(|o| {
            o.rgb_value() == t.rgb_value()
                || (o.brand.is_some()
                    && o.brand == t.brand
                    && o.catalog_number.is_some()
                    && o.catalog_number == t.catalog_number)
        })
    };
    Ok(Some(Value::Array(
        chosen
            .iter()
            .filter(|t| !has(t))
            .map(|t| {
                json!({
                    "color": t.hex(),
                    "brand": t.brand,
                    "code": t.catalog_number,
                    "name": t.name,
                })
            })
            .collect(),
    )))
}

/// The `threads` and `owned` parameters' documentation.
fn thread_source_docs() -> [ParamDoc; 2] {
    [
        opt(
            "threads",
            "brother|all|chart",
            "OpenStitch mode: the threads to choose from: the user's default chart when left out (threads.defaultChart, New Brothread unless changed), brother (the PEC chart), all (every built-in chart, Madeira Polyneon included), a built-in chart id (newBrothread and madeiraPolyneon among them), or one of the user's thread charts by name. PE-DESIGN mode always uses the PEC chart",
        ),
        opt(
            "owned",
            "chart",
            "OpenStitch mode: the user's thread chart of threads they own; the result lists the chosen threads not in it (toBuy)",
        ),
    ]
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

/// Auto Punch's parameters (shared by `digitize.autoPunch` and `digitize.exportOutlines`).
fn auto_punch_docs() -> Vec<ParamDoc> {
    let mut v: Vec<ParamDoc> = placement_docs(vec![
                opt(
                    "colors",
                    "int 0-64",
                    "Thread colours to reduce to; 0 = automatic from the image, as PE-DESIGN fills in Max. Number of Colors (default 0)",
                ),
                opt(
                    "resolution",
                    "int 64-2048",
                    "Working resolution, longest side in pixels (default 400, flat graphics of at most 5 colours traced at twice that; 800 in OpenStitch mode)",
                ),
                opt(
                    "minArea",
                    "mm",
                    "Small-region parameter, mm²: regions of at most max(4, round(minArea / mmPerPx²)) pixels merge into a neighbour (default 1; 2 in OpenStitch mode)",
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
                    "Regions whose stroke width is at most this become satin across the stroke (default 6.5; PE-DESIGN keeps 6.1 mm satin, fills 7.1 mm; OpenStitch mode 2.5, and only for strokes at least three times as long as wide)",
                ),
                opt(
                    "satinMinWidth",
                    "mm",
                    "Strokes narrower than this are fill, not satin (default 0.7; PE-DESIGN fills 0.5 mm, satins 0.75 mm)",
                ),
                opt(
                    "outlineMaxWidth",
                    "mm",
                    "OpenStitch mode: strokes of the darkest colour (L* ≤ 25) at most this wide become satin outlines along their skeleton, sewn last over the regions beside them; 0 turns this off (default 2.5)",
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
                    "isolateSubject",
                    "bool",
                    "OpenStitch mode: find the subject by an iterated graph cut (GrabCut's colour mixtures and min-cut, #127) and leave out everything around it, whatever its colour, in place of skipBackground and fabric; the border is the background it learns from unless subjectRect is given; a cut that finds no subject is an error (default false; PE-DESIGN-compatible mode ignores it)",
                ),
                opt(
                    "subjectRect",
                    "[left, top, right, bottom]",
                    "With isolateSubject: the box around the subject, in fractions of the image (0-1); everything outside it is background (default: a band 2 % of the shorter side around the border)",
                ),
                opt(
                    "subjectStrokes",
                    "[{points: [[x, y], ...], width}]",
                    "With isolateSubject: brush strokes over parts that are subject for certain (#350), such as a bowl the colour of its table; x and y are fractions of the image's width and height (0-1), width a fraction of its shorter side (0.002-0.25); win over backgroundStrokes and subjectRect; at most 200 strokes of 2000 points",
                ),
                opt(
                    "backgroundStrokes",
                    "[{points: [[x, y], ...], width}]",
                    "With isolateSubject: brush strokes over parts that are background for certain, in the same form as subjectStrokes",
                ),
                opt(
                    "compatMode",
                    "peDesign|openStitch",
                    "peDesign: as PE-DESIGN 11.50 (default); openStitch: colours reduced and matched to threads by appearance (CIELAB k-means, CIEDE2000), an 800 px working raster, regions under 2 mm² merged, parts under 1 mm wide (ropes, whiskers) sewn as running lines but never at the cost of a colour stop for under 40 mm of one thread's lines (docs/research/beyond/image-digitizing.md)",
                ),
                fidelity_doc(),
                opt(
                    "dryRun",
                    "bool",
                    "Return the region plan (plan) without adding anything to the design (default false); the regions are numbered as a run with the same options numbers them",
                ),
                opt(
                    "overrides",
                    "[{region, type?, angle?}]",
                    "Per-region choices replacing Auto Punch's own, by plan region number: type fill|satin|line|skip (line: a running line along the region's skeleton; skip: left out) and/or angle, the fill or satin direction in degrees; regions that are lines (thin lines, outlines) can only be skipped (#489)",
                ),
                opt(
                    "angleRule",
                    "pe|varyNeighbours|leastTravel",
                    "OpenStitch mode: how fills choose their direction: pe (default, PE-DESIGN's rule: of 90/45/135/0°, the one with the most rows), varyNeighbours (a fill of at least 20 mm² within 20° of a touching fill as large, sewn before it, is turned by 30-90° as far from it as it can be, so fitted panels read apart) or leastTravel (of every 15°, the direction whose sewn path is shortest, so least travel between a region's sections; PE's direction kept unless another saves at least 1 mm of thread) (#489; needs compatMode openStitch)",
                ),
            ])
        .into_iter()
        .chain(thread_source_docs())
        .collect();
    v.push(opt(
        "subjectPoint",
        "[x, y]",
        "With isolateSubject: one click on the subject, in fractions of the image (0-1): the cut learns the background from the border, the click is subject for certain, and only the part of the subject holding it is kept (#490, no model); in place of subjectRect",
    ));
    v.push(opt(
        "order",
        "[\"#rrggbb\", ...]",
        "The colours to sew first, in this order (the plan's color values); the others follow in PE-DESIGN's order (brightest first); colours this run does not sew are passed over and listed in orderIgnored. Region numbers follow the sewing order, so overrides from a run with another order no longer match (#489)",
    ));
    v.push(opt(
        "regionMap",
        "bool",
        "Also return regionMap {width, height, runs}: the plan region under each cell of a grid at most 256 long, run-length coded row by row as [region, count], -1 for none (background, thin lines, outlines): what the image dialog's click-to-fix looks up (default false, #489)",
    ));
    v.push(opt(
        "underlap",
        "mm 0-1.5",
        "OpenStitch mode: each fill reaches this far under the touching regions sewn after it, so no fabric shows along the seams between colours (default 0, off; 0.8 is typical) (#469; needs compatMode openStitch)",
    ));
    v.push(opt(
        "splitBranches",
        "bool",
        "OpenStitch mode: a satin stroke that branches (a Y, K or X, a star, a forked line) is sewn as one satin column per arm along its own centre line, the columns overlapping 0.3 mm where they meet, instead of one satin across the whole shape; only a stroke whose columns meet at a junction (a ring or a bent stroke stays one satin), at least 8 widths long (area over the square of its widest width) whose columns cover it to 0.9 (overlap over union) (painted regions that branch keep one fill or satin). Default true in OpenStitch mode; false sews every stroke as one (#702; PE-DESIGN mode refuses true)",
    ));
    v.push(opt(
        "flowFill",
        "bool",
        "OpenStitch mode: each fill's rows follow the direction of the picture's own strokes (fur, petals, hair) on a 1 mm grid, as evenly spaced curved rows that never cross, instead of one straight angle; a region whose picture has no clear strokes inside it (a flat shape, noise) keeps straight rows, as does a fill whose angle is set (default false; the region's flowField can be changed afterwards with object.setRegion) (#538; needs compatMode openStitch)",
    ));
    v.push(opt(
        "preset",
        "logo|patch|lineArt|illustration|photo|pixelArt",
        "OpenStitch mode: a purpose preset that sets several options at once, each only where the call does not set it (digitize.presets lists them); implies compatMode openStitch (#489)",
    ));
    v
}

/// Auto Punch's `order` (#489): `["#rrggbb", ...]`, the colours to sew first.
fn colour_order(p: &Params) -> Result<Vec<u32>, CommandError> {
    if !p.has("order") {
        return Ok(Vec::new());
    }
    let bad = || p.invalid("order", "must be a list of at most 64 colours \"#rrggbb\"");
    let list = p.array("order")?;
    if list.len() > 64 {
        return Err(bad());
    }
    list.iter()
        .map(|v| {
            let h = v
                .as_str()
                .and_then(|s| s.strip_prefix('#'))
                .ok_or_else(bad)?;
            if h.len() != 6 {
                return Err(bad());
            }
            u32::from_str_radix(h, 16).map_err(|_| bad())
        })
        .collect()
}

/// Longest side of the region map Auto Punch returns with `regionMap` (#489).
const REGION_MAP_SIDE: usize = 256;

/// Auto Punch's region map (#489): the plan region under each cell of a grid at most
/// [`REGION_MAP_SIDE`] long (nearest working pixel), run-length coded row by row as
/// `[region, count]` pairs, `-1` where no region is.
fn region_map_json(out: &autopunch::Converted) -> Value {
    let (w, h) = out.map_size;
    let long = w.max(h).max(1);
    let (mw, mh) = if long <= REGION_MAP_SIDE {
        (w, h)
    } else {
        (
            (w * REGION_MAP_SIDE).div_ceil(long).max(1),
            (h * REGION_MAP_SIDE).div_ceil(long).max(1),
        )
    };
    let mut runs: Vec<[i64; 2]> = Vec::new();
    for y in 0..mh {
        for x in 0..mw {
            let (sx, sy) = (
                (x * w / mw.max(1)).min(w - 1),
                (y * h / mh.max(1)).min(h - 1),
            );
            let r = out.region_map[sy * w + sx];
            let v = if r == u32::MAX { -1 } else { i64::from(r) };
            match runs.last_mut() {
                Some(last) if last[0] == v => last[1] += 1,
                _ => runs.push([v, 1]),
            }
        }
    }
    json!({"width": mw, "height": mh, "runs": runs})
}

/// Auto Punch's `splitBranches` (#702): OpenStitch mode only, on by default there.
fn split_branches(p: &Params, mode: Mode) -> Result<bool, CommandError> {
    let on = p.bool("splitBranches", Some(mode == Mode::OpenStitch))?;
    if on && mode == Mode::PeDesign {
        return Err(p.invalid("splitBranches", "needs compatMode openStitch"));
    }
    Ok(on)
}

/// Auto Punch's `flowFill` (#538): OpenStitch mode only.
fn flow_fill(p: &Params, mode: Mode) -> Result<bool, CommandError> {
    let on = p.bool("flowFill", Some(false))?;
    if on && mode == Mode::PeDesign {
        return Err(p.invalid("flowFill", "needs compatMode openStitch"));
    }
    Ok(on)
}

/// Auto Punch's `underlap` (#469): mm, OpenStitch mode only.
fn underlap(p: &Params, mode: Mode) -> Result<f64, CommandError> {
    let v = p.opt("underlap", 0.0, 0.0, autopunch::MAX_UNDERLAP_MM)?;
    if v > 0.0 && mode == Mode::PeDesign {
        return Err(p.invalid("underlap", "needs compatMode openStitch"));
    }
    Ok(v)
}

/// Purpose presets for Auto Punch (#489): named bundles over its own options, OpenStitch mode
/// only. Each sets options the call leaves unset.
pub const PRESETS: [(&str, &str); 6] = [
    (
        "logo",
        "Logos and flat graphics: touching fills turned apart, nothing smaller than 2 mm²",
    ),
    (
        "patch",
        "Patches and badges: the whole picture sewn, background included, rows laid for the least travel",
    ),
    (
        "lineArt",
        "Line drawings: contour lines up to 4 mm sewn as outlines on top",
    ),
    (
        "illustration",
        "Illustrations and cartoons: outlines on top, touching fills turned apart",
    ),
    (
        "photo",
        "Photos: high fidelity colours, specks under 4 mm² merged, satin from 1 mm, rows laid for the least travel",
    ),
    ("pixelArt", "Pixel art: straight edges"),
];

/// The options preset `name` sets.
fn preset_values(name: &str) -> Option<Value> {
    Some(match name {
        // Each preset sets only what its purpose needs: a smoothing pass (logo, photo) or a
        // smaller small-region area (lineArt, pixelArt) left specks and short ends that sewed
        // 7-64 stitches in place on the gallery and B16 images (sew_check, #690).
        "logo" => json!({"angleRule": "varyNeighbours", "minArea": 2.0}),
        "patch" => json!({"skipBackground": false, "angleRule": "leastTravel"}),
        "lineArt" => json!({"outlineMaxWidth": 4.0}),
        "illustration" => json!({"angleRule": "varyNeighbours", "outlineMaxWidth": 2.5}),
        // Satin from 1 mm: High fidelity's narrow strokes sewed as hairline zigzags (p07: 49
        // stitches under 1 mm wide in a row).
        "photo" => {
            json!({"fidelity": "high", "minArea": 4.0, "angleRule": "leastTravel", "satinMinWidth": 1.0})
        }
        "pixelArt" => json!({"curves": false}),
        _ => return None,
    })
}

/// `p` with its `preset`'s options filled in where the call leaves them unset, and compatMode
/// openStitch; `p` itself when it names no preset.
fn with_preset(p: &Params) -> Result<Option<Params>, CommandError> {
    if !p.has("preset") {
        return Ok(None);
    }
    let name = p.string("preset", None)?;
    let names: Vec<&str> = PRESETS.iter().map(|k| k.0).collect();
    let Some(Value::Object(set)) = preset_values(&name) else {
        return Err(p.invalid("preset", format!("must be one of {}", names.join(", "))));
    };
    if p.string("compatMode", Some("openStitch"))? != "openStitch" {
        return Err(p.invalid("preset", "a preset needs compatMode openStitch"));
    }
    let mut values = p.values().clone();
    values.insert("compatMode".into(), json!("openStitch"));
    for (k, v) in set {
        values.entry(k).or_insert(v);
    }
    Params::new(&p.command, Some(&Value::Object(values))).map(Some)
}

/// Auto Punch's image, placement and options from its parameters (shared by
/// `digitize.autoPunch` and `digitize.exportOutlines`).
fn auto_punch_options(
    s: &EngineState,
    p: &Params,
) -> Result<(Raster, Frame, AutoPunch), CommandError> {
    let preset = with_preset(p)?;
    let p = preset.as_ref().unwrap_or(p);
    let (image, frame) = load(s, p)?;
    let mode = compat_mode(p)?;
    let angle_rule = match (
        choice(
            p,
            "angleRule",
            "pe",
            &["pe", "varyNeighbours", "leastTravel"],
        )?,
        mode,
    ) {
        ("pe", _) => AngleRule::Pe,
        ("varyNeighbours", Mode::OpenStitch) => AngleRule::VaryNeighbours,
        ("leastTravel", Mode::OpenStitch) => AngleRule::LeastTravel,
        (rule, _) => {
            return Err(p.invalid("angleRule", format!("{rule} needs compatMode openStitch")));
        }
    };
    // OpenStitch mode works at 800 px, so thin lines and small details keep their
    // shape, and merges regions under 2 mm², so no speck is sewn (B16.1 T11), unless
    // the caller sets them.
    // Its dark contour lines are satin lines along their skeleton, satin is kept
    // for strokes up to 2.5 mm and wider shapes are filled (B16.1 T12).
    let (resolution, min_area, outline_max, satin_max) = match mode {
        Mode::PeDesign => (400, 1.0, 0.0, 6.5),
        Mode::OpenStitch => (800, 2.0, 2.5, 2.5),
    };
    let mut options = AutoPunch {
        colors: count(p, "colors", AutoPunch::default().colors as i64, 0, 64)?,
        resolution: count(p, "resolution", resolution, 64, 2048)? as u32,
        min_area_mm2: p.opt("minArea", min_area, 0.0, 1000.0)?,
        smooth: count(p, "smooth", 0, 0, 5)? as u32,
        trace: trace_options(p, 0.7)?,
        satin_max_mm: p.opt("satinMaxWidth", satin_max, 1.0, 12.0)?,
        satin_min_mm: p.opt("satinMinWidth", 0.7, 0.0, 6.0)?,
        skip_background: p.bool("skipBackground", Some(true))?,
        outline_max_mm: p.opt("outlineMaxWidth", outline_max, 0.0, 12.0)?,
        mode,
        threads: thread_source(s, p, mode)?,
        fidelity: fidelity(p, mode)?,
        isolate: subject_seed(p)?,
        marks: Marks {
            subject: strokes(p, "subjectStrokes")?,
            background: strokes(p, "backgroundStrokes")?,
        },
        subject_point: subject_point(p)?,
        overrides: overrides(p)?,
        angle_rule,
        underlap_mm: underlap(p, mode)?,
        flow_fill: flow_fill(p, mode)?,
        split_branches: split_branches(p, mode)?,
        order: colour_order(p)?,
        ..AutoPunch::default()
    };
    // OpenStitch mode sews anything in the picture: without skipBackground, only a
    // background of the fabric's colour is left out.
    if mode == Mode::OpenStitch && p.get("skipBackground").is_none() {
        let f = thread(p, "fabric")?.unwrap_or(Thread::rgb(0xffffff));
        options.fabric = Some([f.red, f.green, f.blue]);
    }
    Ok((image, frame, options))
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "digitize.autoPunch",
            "Auto Punch (approximation): convert an image's colour areas into fill and satin shapes",
            Effect::Edit,
            auto_punch_docs(),
            |s, p| {
                let (image, frame, options) = auto_punch_options(s, p)?;
                let out = autopunch::convert(&image, &frame, &options).map_err(|e| failed(p, e))?;
                let regions = out.objects.len();
                let buy = to_buy(s, p, &out.threads)?;
                let plan = plan_json(&out, &options);
                let ignored: Vec<String> =
                    out.order_ignored.iter().map(|v| format!("#{v:06x}")).collect();
                let map = p
                    .bool("regionMap", Some(false))?
                    .then(|| region_map_json(&out));
                let dry = p.bool("dryRun", Some(false))?;
                let mut r = json!({
                    "colors": hexes(&out.threads),
                    "threads": described(&out.threads),
                    "regions": regions,
                    "background": out.background.as_ref().map(Thread::hex),
                    "plan": plan,
                });
                if !dry {
                    r["ids"] = json!(add_all(s, out.objects));
                }
                if let Some(b) = buy {
                    r["toBuy"] = b;
                }
                if let Some(m) = map {
                    r["regionMap"] = m;
                }
                if !ignored.is_empty() {
                    r["orderIgnored"] = json!(ignored);
                }
                Ok(r)
            },
        ),
        spec(
            "digitize.presets",
            "Auto Punch's purpose presets (#489): each one's name, what it is for and the options it sets",
            Effect::Read,
            vec![],
            |_, _| {
                Ok(Value::Array(
                    PRESETS
                        .iter()
                        .map(|(name, about)| {
                            json!({"name": name, "about": about, "options": preset_values(name)})
                        })
                        .collect(),
                ))
            },
        ),
        spec(
            "digitize.exportOutlines",
            "Save Auto Punch's traced outlines as an SVG file (#489): one filled path per sewn region in its thread colour, one stroked path per line, in mm; import.svg reads it back as one object each",
            Effect::File,
            {
                let mut v = auto_punch_docs();
                v.push(req(
                    "out",
                    "string",
                    "SVG file to write (relative to the session directory)",
                ));
                v
            },
            |s, p| {
                let (image, frame, options) = auto_punch_options(s, p)?;
                let target = s.resolve(&p.string("out", None)?, Access::Write, &p.command)?;
                let out = autopunch::convert(&image, &frame, &options).map_err(|e| failed(p, e))?;
                let (svg, fills, lines) = outlines_svg(&out.objects);
                std::fs::write(&target, svg).map_err(|e| p.failed(e.to_string()))?;
                Ok(json!({
                    "out": target.display().to_string(),
                    "fills": fills,
                    "lines": lines,
                    "colors": hexes(&out.threads),
                    "threads": described(&out.threads),
                }))
            },
        ),
        spec(
            "digitize.segment",
            "One-click subject cut-out (#490, no model): the subject's mask from a click on it; preview for Auto Punch's isolateSubject with subjectPoint, optionally written as a PNG (subject black)",
            Effect::File,
            vec![
                IMAGE,
                req(
                    "point",
                    "[x, y]",
                    "The click on the subject, in fractions of the image (0-1)",
                ),
                opt(
                    "resolution",
                    "int 64-2048",
                    "Working resolution, longest side in pixels (default 800, as Auto Punch's OpenStitch mode)",
                ),
                opt(
                    "subjectStrokes",
                    "[{points: [[x, y], ...], width}]",
                    "Brush strokes over parts that are subject for certain, as Auto Punch takes them",
                ),
                opt(
                    "backgroundStrokes",
                    "[{points: [[x, y], ...], width}]",
                    "Brush strokes over parts that are background for certain",
                ),
                opt("out", "string", "Write the mask to this PNG file"),
            ],
            |s, p| {
                let (_, data) = read_file(s, p, MAX_FILE_BYTES)?;
                let image = Raster::decode(&data).map_err(|e| failed(p, e))?;
                let point = point_param(p, "point")?;
                let work = image.fit_mode(count(p, "resolution", 800, 64, 2048)? as u32);
                let (w, h) = (work.width() as usize, work.height() as usize);
                let marks = Marks {
                    subject: segment_strokes(p, "subjectStrokes")?,
                    background: segment_strokes(p, "backgroundStrokes")?,
                };
                let census = openstitch_digitize::palette::census(&work);
                let mask = grabcut::isolate_at(&census.modal, w, h, point, &marks)
                    .ok_or_else(|| p.failed("no background to learn from: the image has no opaque border"))?;
                let n = mask.iter().filter(|&&m| m).count();
                let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
                for (i, _) in mask.iter().enumerate().filter(|(_, m)| **m) {
                    let (x, y) = (i % w, i / w);
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
                }
                let mut r = json!({
                    "width": w,
                    "height": h,
                    "subjectPixels": n,
                    "share": round(n as f64 / (w * h).max(1) as f64),
                    "box": (n > 0).then(|| [
                        round(x0 as f64 / w as f64),
                        round(y0 as f64 / h as f64),
                        round(x1 as f64 / w as f64),
                        round(y1 as f64 / h as f64),
                    ]),
                });
                if p.has("out") {
                    let path = s.resolve(&p.string("out", None)?, Access::Write, &p.command)?;
                    let png = Raster::from_fn(w as u32, h as u32, |x, y| {
                        if mask[y as usize * w + x as usize] {
                            [0, 0, 0, 255]
                        } else {
                            [255, 255, 255, 255]
                        }
                    })
                    .and_then(|m| m.encode_png())
                    .map_err(|e| failed(p, e))?;
                    std::fs::write(&path, png).map_err(|e| p.failed(e.to_string()))?;
                    r["out"] = json!(path.display().to_string());
                }
                Ok(r)
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
                    "threads": described(&out.threads),
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
                opt(
                    "travel",
                    "fewerTrims|allTrims",
                    "OpenStitch mode, fill: fewerTrims sews short visible travel stitches (up to 2 mm) between a thread's runs instead of trimming (about half the trims); allTrims travels only under later layers and trims elsewhere (best likeness); default fewerTrims at Standard fidelity, allTrims at High",
                ),
                opt(
                    "densityMean",
                    "points/mm²",
                    "OpenStitch mode, fill: most needle points per mm² over the design, 0.5-10 (default 2.1)",
                ),
                opt(
                    "densityWindow",
                    "points/mm²",
                    "OpenStitch mode, fill: most needle points per mm² in any 10 × 10 mm window, 0.5-10 (default 3.0)",
                ),
            ]
            .into_iter()
            .chain(thread_source_docs())
            .collect()),
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
                options.threads = thread_source(s, p, compat)?;
                // The user's choice; by default fewer trims at Standard, the best likeness at
                // High (the owner, 2026-10-31).
                let travel_default = match fidelity {
                    Fidelity::High => "allTrims",
                    Fidelity::Standard => "fewerTrims",
                };
                options.travel =
                    match choice(p, "travel", travel_default, &["fewerTrims", "allTrims"])? {
                        "allTrims" => Travel::AllTrims,
                        _ => Travel::FewerTrims,
                    };
                options.density_mean = p.opt("densityMean", options.density_mean, 0.5, 10.0)?;
                options.density_window =
                    p.opt("densityWindow", options.density_window, 0.5, 10.0)?;
                let out = photo::convert(&image, &frame, &options).map_err(|e| failed(p, e))?;
                let buy = to_buy(s, p, &out.threads)?;
                let mut r = json!({
                    "colors": hexes(&out.threads),
                    "threads": described(&out.threads),
                    "stitches": out.stitches,
                    "ids": add_all(s, out.objects),
                });
                if let Some(b) = buy {
                    r["toBuy"] = b;
                }
                Ok(r)
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
            ]
            .into_iter()
            .chain(edge_docs())
            .collect(),
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
                    "joinLines",
                    "bool",
                    "Running and triple lines that meet are sewn as one path, walking back over lines already sewn instead of trimming between them (default true; false: one piece per line, a trim after each)",
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
                    join_lines: p.bool("joinLines", Some(true))?,
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
    v.extend(edge_docs());
    v
}

fn line_options(p: &Params) -> Result<LineImageOptions, CommandError> {
    let edges = match choice(p, "lines", "dark", &["dark", "edges"])? {
        "edges" => Some(p.opt("edgeStrength", center::EDGE_STRENGTH, 1.0, 100.0)?),
        _ if p.has("edgeStrength") => {
            return Err(p.invalid("edgeStrength", "needs lines: edges"));
        }
        _ => None,
    };
    if edges.is_some() && p.has("threshold") {
        return Err(p.invalid("threshold", "lines: edges takes edgeStrength instead"));
    }
    Ok(LineImageOptions {
        threshold: p.int("threshold", Some(128), 0, 255)? as f64,
        resolution: count(p, "resolution", 600, 64, 2048)? as u32,
        edges,
    })
}

/// The `lines` and `edgeStrength` parameters' documentation (#490).
fn edge_docs() -> [ParamDoc; 2] {
    [
        opt(
            "lines",
            "dark|edges",
            "What the line image's lines are: dark (default, PE-DESIGN's: pixels darker than threshold) or edges (a line drawing from a photo: the picture's edges, one pixel wide, found without any model, #490)",
        ),
        opt(
            "edgeStrength",
            "1-100",
            "With lines: edges, the weakest edge kept, % of the strongest (default 20; lower keeps fainter edges)",
        ),
    ]
}

/// Stages 1–3; OpenStitch mode takes the threshold from the image (Otsu) unless it is set.
fn figure(s: &EngineState, p: &Params, mode: Mode) -> Result<center::Figure, CommandError> {
    let (image, frame) = load(s, p)?;
    let mut lo = line_options(p)?;
    if mode == Mode::OpenStitch && !p.has("threshold") && lo.edges.is_none() {
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
