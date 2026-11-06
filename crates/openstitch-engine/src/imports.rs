//! Importing artwork: SVG vector paths as editable Bézier shapes (`import.svg`), WMF/EMF
//! drawings the same way (`import.metafile`, parsed by [`crate::metafile`]), and a raster
//! image as a background reference layer (`background.*`, with the `digitize` feature, which
//! brings the size-limited image decoder).
//!
//! SVG is parsed by `usvg` (resvg project, MIT / Apache-2.0) without text, fonts or compressed
//! SVG, and with external and embedded `<image>` references disabled, so a file can never make
//! the engine read outside the sandbox. usvg resolves CSS, units, `viewBox` and transforms and
//! turns rectangles, circles, ellipses, lines and polygons into paths. Each visible path becomes
//! one shape object: subpaths become contours (quadratic segments are raised to cubics, so
//! curves stay exact), a fill becomes a fill region in its colour, a stroke a running line.
//! Clip paths and masks cut what they apply to (a mask keeps the area where it is at least half
//! opaque), a fill or stroke whose opacity is below `minOpacity` is left out, a gradient gives
//! its colour half way along or, opt-in, bands of its colours, and a pattern paint is black.
//! The result lists everything left out (text, images, transparent or clipped-away shapes) and
//! approximated (#487). SVG's `nonzero` fill rule is approximated by OpenStitch's even-odd rule.

use crate::commands::{CommandSpec, Effect, MM, opt, read_file, req, spec};
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use openstitch_core::Thread;
use openstitch_design::boolean::{self, BoolOp};
use openstitch_design::{
    Bezier, Contour, DesignObject, LineSewing, LineType, Rect, RegionSewing, RegionType, Transform,
    Vec2, v2,
};
use serde_json::{Value, json};

/// Largest SVG file accepted.
pub const MAX_SVG_BYTES: usize = 16 << 20;
/// Most shape objects one import may add.
pub const MAX_SVG_PATHS: usize = 5_000;
/// Most vertices one import may add.
pub const MAX_SVG_POINTS: usize = 500_000;

/// One SVG unit (CSS pixel, 96 per inch) in 0.1 mm.
const PX: f64 = 254.0 / 96.0;

/// A shape read from an SVG, in 0.1 mm with the SVG's own origin.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SvgShape {
    pub contours: Vec<Contour>,
    pub fill: Option<Thread>,
    pub stroke: Option<Thread>,
    /// The ids of the element and of the named groups around it, outermost first (for the
    /// stitch settings in names, #455).
    pub names: Vec<String>,
    /// The stroke's dash pattern (dash, gap, …) and offset in 0.1 mm, if it is dashed.
    pub dashes: Option<(Vec<f64>, f64)>,
    /// `paint-order` puts the stroke before the fill.
    pub stroke_first: bool,
}

/// Why an SVG could not be imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SvgError {
    Parse(String),
    TooLarge(String),
}

impl std::fmt::Display for SvgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "not a usable SVG: {e}"),
            Self::TooLarge(e) => write!(f, "SVG too large: {e}"),
        }
    }
}

fn options() -> usvg::Options<'static> {
    usvg::Options {
        resources_dir: None,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    }
}

fn rgb_thread(c: usvg::Color) -> Thread {
    Thread::rgb(u32::from(c.red) << 16 | u32::from(c.green) << 8 | u32::from(c.blue))
}

/// The colour of a gradient's stops at `t` (0–1), interpolated in sRGB between the stops
/// around it (SVG's default `color-interpolation`); outside the stops, the end stop's colour.
fn stop_colour(stops: &[usvg::Stop], t: f64) -> Thread {
    let at = |s: &usvg::Stop| f64::from(s.offset().get());
    let Some(first) = stops.first() else {
        return Thread::rgb(0);
    };
    if t <= at(first) {
        return rgb_thread(first.color());
    }
    for w in stops.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if t <= at(b) {
            let span = at(b) - at(a);
            let f = if span > 0.0 { (t - at(a)) / span } else { 1.0 };
            let mix =
                |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * f).round() as u8;
            let (ca, cb) = (a.color(), b.color());
            return Thread::rgb(
                u32::from(mix(ca.red, cb.red)) << 16
                    | u32::from(mix(ca.green, cb.green)) << 8
                    | u32::from(mix(ca.blue, cb.blue)),
            );
        }
    }
    rgb_thread(stops[stops.len() - 1].color())
}

/// The one thread a paint gives: its colour, a gradient's colour half way along, or, for a
/// pattern (not read), black.
fn paint_thread(paint: &usvg::Paint) -> Thread {
    match paint {
        usvg::Paint::Color(c) => rgb_thread(*c),
        usvg::Paint::LinearGradient(g) => stop_colour(g.stops(), 0.5),
        usvg::Paint::RadialGradient(g) => stop_colour(g.stops(), 0.5),
        usvg::Paint::Pattern(_) => Thread::rgb(0),
    }
}

/// How `import.svg` turns a gradient fill into threads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gradients {
    /// One fill in the gradient's colour half way along (the default).
    OneColour,
    /// That many fills (2–8), bands along a linear gradient or rings of a radial one, each in
    /// the gradient's colour at its middle.
    Bands(usize),
}

/// Options of an SVG import.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgOptions {
    pub gradients: Gradients,
    /// A fill or stroke whose opacity (its own times its groups') is below this is left out.
    pub min_opacity: f64,
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            gradients: Gradients::OneColour,
            min_opacity: 0.1,
        }
    }
}

/// What an import left out (`skipped`) or changed to fit embroidery (`approximated`), by kind,
/// with the SVG ids of the elements (or of their nearest named group) where there are any.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SvgReport {
    pub skipped: std::collections::BTreeMap<&'static str, (usize, Vec<String>)>,
    pub approximated: std::collections::BTreeMap<&'static str, (usize, Vec<String>)>,
}

/// Most ids listed per kind.
const MAX_REPORT_IDS: usize = 50;

impl SvgReport {
    fn add(
        map: &mut std::collections::BTreeMap<&'static str, (usize, Vec<String>)>,
        kind: &'static str,
        id: &str,
    ) {
        let e = map.entry(kind).or_default();
        e.0 += 1;
        if !id.is_empty() && e.1.len() < MAX_REPORT_IDS && !e.1.iter().any(|x| x == id) {
            e.1.push(id.to_string());
        }
    }

    fn skip(&mut self, kind: &'static str, id: &str) {
        Self::add(&mut self.skipped, kind, id);
    }

    fn approx(&mut self, kind: &'static str, id: &str) {
        Self::add(&mut self.approximated, kind, id);
    }

    fn list(map: &std::collections::BTreeMap<&'static str, (usize, Vec<String>)>) -> Value {
        Value::Array(
            map.iter()
                .map(|(k, (n, ids))| json!({ "kind": k, "count": n, "ids": ids }))
                .collect(),
        )
    }

    /// `{skipped: [{kind, count, ids}], approximated: [...]}`.
    pub fn to_json(&self) -> Value {
        json!({
            "skipped": Self::list(&self.skipped),
            "approximated": Self::list(&self.approximated),
        })
    }
}

/// An SVG's shapes and what was left out or approximated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SvgImport {
    pub shapes: Vec<SvgShape>,
    pub report: SvgReport,
}

/// Parses `data` into shapes, in document order, with the default options.
pub fn parse_svg(data: &[u8]) -> Result<Vec<SvgShape>, SvgError> {
    parse_svg_with(data, &SvgOptions::default()).map(|i| i.shapes)
}

/// Parses `data` into shapes, in document order: clip paths and masks cut the shapes they
/// apply to, fills and strokes below `min_opacity` are left out, gradients become one colour or
/// bands, and the report lists everything left out or approximated (#487).
pub fn parse_svg_with(data: &[u8], opts: &SvgOptions) -> Result<SvgImport, SvgError> {
    if data.len() > MAX_SVG_BYTES {
        return Err(SvgError::TooLarge(format!("{} bytes", data.len())));
    }
    let mut report = SvgReport::default();
    let resolved = css_variables(data, &mut report);
    let data = resolved.as_deref().unwrap_or(data);
    let tree =
        usvg::Tree::from_data(data, &options()).map_err(|e| SvgError::Parse(e.to_string()))?;
    let mut out = Walk {
        opts,
        import: SvgImport::default(),
        points: 0,
    };
    out.import.report = report;
    unsupported_elements(data, &mut out.import.report);
    let ctx = Ctx {
        opacity: 1.0,
        clip: None,
        id: String::new(),
        names: Vec::new(),
    };
    out.group(tree.root(), &ctx)?;
    Ok(out.import)
}

/// CSS custom properties, which usvg does not read: every `var(--name)` or
/// `var(--name, fallback)` is replaced by the value a `--name: value` declaration anywhere in
/// the file gives (the first one; selectors and scopes are not followed), else by the fallback.
/// One with neither is left for usvg (which then uses SVG's default) and listed. `None` when
/// the file has no `var(`.
fn css_variables(data: &[u8], report: &mut SvgReport) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(data).ok()?;
    if !text.contains("var(") {
        return None;
    }
    let mut vars: Vec<(&str, &str)> = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("--") {
        let after = &rest[i + 2..];
        let name_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(after.len());
        let name = &after[..name_len];
        let tail = after[name_len..].trim_start();
        if !name.is_empty()
            && let Some(v) = tail.strip_prefix(':')
        {
            let end = v.find([';', '}', '"', '\'', '<']).unwrap_or(v.len());
            let value = v[..end].trim();
            if !value.is_empty() && !vars.iter().any(|(n, _)| *n == name) {
                vars.push((name, value));
            }
        }
        rest = &after[name_len..];
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("var(") {
        out.push_str(&rest[..i]);
        let inner = &rest[i + 4..];
        let Some(close) = inner.find(')') else {
            out.push_str(&rest[i..]);
            rest = "";
            break;
        };
        let (name, fallback) = match inner[..close].split_once(',') {
            Some((n, f)) => (n.trim(), Some(f.trim())),
            None => (inner[..close].trim(), None),
        };
        let value = name
            .strip_prefix("--")
            .and_then(|n| vars.iter().find(|(v, _)| *v == n).map(|(_, x)| *x))
            .or(fallback);
        match value {
            Some(v) => out.push_str(v),
            None => {
                report.approx("cssVariable", name);
                out.push_str(&rest[i..i + 4 + close + 1]);
            }
        }
        rest = &inner[close + 1..];
    }
    out.push_str(rest);
    Some(out.into_bytes())
}

/// `<text>`, `<image>` and `<foreignObject>` elements that would show: usvg is built without
/// text and with images refused, so they are counted from the XML itself.
fn unsupported_elements(data: &[u8], report: &mut SvgReport) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let opt = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    let Ok(doc) = roxmltree::Document::parse_with_options(text, opt) else {
        return;
    };
    for n in doc.descendants().filter(roxmltree::Node::is_element) {
        let kind = match n.tag_name().name() {
            "text" => "text",
            "image" => "image",
            "foreignObject" => "foreignObject",
            _ => continue,
        };
        let hidden = n.ancestors().skip(1).any(|a| {
            matches!(
                a.tag_name().name(),
                "defs" | "clipPath" | "mask" | "pattern" | "marker" | "symbol" | "text"
            )
        });
        if !hidden {
            report.skip(kind, n.attribute("id").unwrap_or(""));
        }
    }
}

/// What the groups around a node pass down to it.
#[derive(Clone)]
struct Ctx {
    /// The product of the groups' opacities.
    opacity: f64,
    /// The area clip paths and masks leave visible, in 0.1 mm; `None` = everything.
    clip: Option<std::rc::Rc<Vec<Contour>>>,
    /// The id of the nearest named element, for the report.
    id: String,
    /// The ids of the named groups around the node, outermost first.
    names: Vec<String>,
}

struct Walk<'a> {
    opts: &'a SvgOptions,
    import: SvgImport,
    points: usize,
}

/// `a ∩ b` (`None` is everything).
fn intersect(a: Option<&[Contour]>, b: Vec<Contour>) -> Vec<Contour> {
    match a {
        None => b,
        Some(a) => boolean::contours(&boolean::combine(a, &b, BoolOp::Intersection)),
    }
}

/// The paths of `g` and its subgroups with the transform each is drawn with and the group
/// opacity above it.
fn paths_of<'a>(
    g: &'a usvg::Group,
    ts: usvg::Transform,
    opacity: f64,
    out: &mut Vec<(&'a usvg::Path, usvg::Transform, f64)>,
) {
    for node in g.children() {
        match node {
            usvg::Node::Group(c) => paths_of(c, ts, opacity * f64::from(c.opacity().get()), out),
            usvg::Node::Path(p) if p.is_visible() => {
                out.push((p, ts.pre_concat(p.abs_transform()), opacity));
            }
            _ => {}
        }
    }
}

/// The area a clip path leaves visible, for an element drawn with `ts`: the union of its
/// shapes, cut by its own clip path.
fn clip_region(cp: &usvg::ClipPath, ts: usvg::Transform) -> Vec<Contour> {
    let mut paths = Vec::new();
    paths_of(cp.root(), ts.pre_concat(cp.transform()), 1.0, &mut paths);
    let parts: Vec<Vec<Contour>> = paths
        .iter()
        .map(|(p, t, _)| contours(p.data(), t, true, false))
        .collect();
    let mut region = boolean::contours(&boolean::union_all(parts.iter().map(Vec::as_slice)));
    if let Some(inner) = cp.clip_path() {
        region = intersect(Some(&region), clip_region(inner, ts));
    }
    region
}

/// The area a mask leaves visible, for an element drawn with `ts`: its shapes in document
/// order, each adding its area when it is at least half opaque (luminance times opacity for a
/// luminance mask, opacity for an alpha mask) and removing it otherwise; inside the mask's
/// rectangle; cut by its own mask. Strokes in a mask are not read.
fn mask_region(m: &usvg::Mask, ts: usvg::Transform) -> Vec<Contour> {
    let mut paths = Vec::new();
    paths_of(m.root(), ts, 1.0, &mut paths);
    let mut acc: Vec<Contour> = Vec::new();
    for (p, t, opacity) in paths {
        let Some(fill) = p.fill() else { continue };
        let alpha = opacity * f64::from(fill.opacity().get());
        let value = match m.kind() {
            usvg::MaskType::Alpha => alpha,
            usvg::MaskType::Luminance => {
                let c = paint_thread(fill.paint());
                let l = (0.2125 * f64::from(c.red)
                    + 0.7154 * f64::from(c.green)
                    + 0.0721 * f64::from(c.blue))
                    / 255.0;
                l * alpha
            }
        };
        let shape = contours(p.data(), &t, true, false);
        let op = if value >= 0.5 {
            BoolOp::Union
        } else {
            BoolOp::Difference
        };
        acc = boolean::contours(&boolean::combine(&acc, &shape, op));
    }
    let r = m.rect();
    let rect = tiny_skia_rect(r.x(), r.y(), r.width(), r.height(), &ts);
    acc = intersect(Some(&acc), vec![rect]);
    if let Some(inner) = m.mask() {
        acc = intersect(Some(&acc), mask_region(inner, ts));
    }
    acc
}

/// A rectangle in SVG units drawn with `t`, as a closed contour in 0.1 mm.
fn tiny_skia_rect(x: f32, y: f32, w: f32, h: f32, t: &usvg::Transform) -> Contour {
    let a = Affine::of(t);
    let pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
        .map(|(px, py)| a.apply(f64::from(px), f64::from(py)));
    Contour::new(pts.to_vec(), true)
}

/// An SVG transform in f64, mapping SVG units to 0.1 mm.
#[derive(Clone, Copy)]
struct Affine([f64; 6]);

impl Affine {
    fn of(t: &usvg::Transform) -> Affine {
        Affine([t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].map(f64::from))
    }

    fn apply(&self, x: f64, y: f64) -> Vec2 {
        let [sx, ky, kx, sy, tx, ty] = self.0;
        v2((sx * x + kx * y + tx) * PX, (ky * x + sy * y + ty) * PX)
    }

    /// The inverse, from 0.1 mm back to SVG units.
    fn invert(&self, p: Vec2) -> Option<(f64, f64)> {
        let [sx, ky, kx, sy, tx, ty] = self.0;
        let det = sx * sy - kx * ky;
        if det.abs() < 1e-12 {
            return None;
        }
        let (x, y) = (p.x / PX - tx, p.y / PX - ty);
        Some(((sy * x - kx * y) / det, (-ky * x + sx * y) / det))
    }
}

/// A gradient fill as `n` bands: along a linear gradient's axis, or rings around a radial
/// gradient's centre (its focal point is not used). Each band is the shape cut to the part
/// where the gradient runs through that band, in the gradient's colour at the band's middle;
/// empty bands are left out. Beyond the gradient's ends the end bands go on (pad).
fn gradient_bands(
    paint: &usvg::Paint,
    shape: &[Contour],
    path_ts: &usvg::Transform,
    n: usize,
) -> Option<Vec<(Vec<Contour>, Thread)>> {
    let (stops, gts) = match paint {
        usvg::Paint::LinearGradient(g) => (g.stops(), g.transform()),
        usvg::Paint::RadialGradient(g) => (g.stops(), g.transform()),
        _ => return None,
    };
    let a = Affine::of(&path_ts.pre_concat(gts));
    // The shape in gradient space.
    let local: Vec<(f64, f64)> = shape
        .iter()
        .flat_map(Contour::outline_points)
        .map(|p| a.invert(p))
        .collect::<Option<_>>()?;
    let mut bands = Vec::with_capacity(n);
    for k in 0..n {
        let (lo, hi) = (k as f64 / n as f64, (k + 1) as f64 / n as f64);
        let band: Vec<Contour> = match paint {
            usvg::Paint::LinearGradient(g) => {
                let (x1, y1) = (f64::from(g.x1()), f64::from(g.y1()));
                let (ux, uy) = (f64::from(g.x2()) - x1, f64::from(g.y2()) - y1);
                let len2 = ux * ux + uy * uy;
                if len2 < 1e-12 {
                    return None;
                }
                let (wx, wy) = (-uy / len2.sqrt(), ux / len2.sqrt());
                let t = |&(x, y): &(f64, f64)| ((x - x1) * ux + (y - y1) * uy) / len2;
                let s = |&(x, y): &(f64, f64)| (x - x1) * wx + (y - y1) * wy;
                let tmin = local.iter().map(t).fold(0.0, f64::min) - 1.0;
                let tmax = local.iter().map(t).fold(1.0, f64::max) + 1.0;
                let smin = local.iter().map(s).fold(f64::INFINITY, f64::min) - 1.0;
                let smax = local.iter().map(s).fold(f64::NEG_INFINITY, f64::max) + 1.0;
                let (t0, t1) = (
                    if k == 0 { tmin } else { lo },
                    if k + 1 == n { tmax } else { hi },
                );
                let at = |t: f64, s: f64| a.apply(x1 + ux * t + wx * s, y1 + uy * t + wy * s);
                vec![Contour::new(
                    vec![at(t0, smin), at(t1, smin), at(t1, smax), at(t0, smax)],
                    true,
                )]
            }
            usvg::Paint::RadialGradient(g) => {
                let (cx, cy, r) = (f64::from(g.cx()), f64::from(g.cy()), f64::from(g.r().get()));
                let far = local
                    .iter()
                    .map(|&(x, y)| (x - cx).hypot(y - cy))
                    .fold(r, f64::max)
                    * 1.5;
                let circle = |radius: f64| {
                    let pts = (0..72)
                        .map(|i| {
                            let ang = i as f64 * std::f64::consts::TAU / 72.0;
                            a.apply(cx + radius * ang.cos(), cy + radius * ang.sin())
                        })
                        .collect();
                    Contour::new(pts, true)
                };
                let outer = circle(if k + 1 == n { far } else { hi * r });
                if k == 0 {
                    vec![outer]
                } else {
                    boolean::contours(&boolean::combine(
                        &[outer],
                        &[circle(lo * r)],
                        BoolOp::Difference,
                    ))
                }
            }
            _ => return None,
        };
        let part = boolean::contours(&boolean::combine(shape, &band, BoolOp::Intersection));
        if !part.is_empty() {
            bands.push((part, stop_colour(stops, (lo + hi) / 2.0)));
        }
    }
    Some(bands)
}

/// The parts of an open or closed stroke inside `clip` (even-odd), as open contours of
/// straight segments (curves flattened). A stroke wholly inside is returned unchanged.
fn clip_stroke(c: &Contour, clip: &[Contour]) -> Vec<Contour> {
    let edges: Vec<(Vec2, Vec2)> = clip
        .iter()
        .flat_map(|k| {
            let p = k.path();
            let mut e: Vec<(Vec2, Vec2)> = p.windows(2).map(|w| (w[0], w[1])).collect();
            if let (Some(&a), Some(&b)) = (p.last(), p.first())
                && a.distance(b) > 1e-9
            {
                e.push((a, b));
            }
            e
        })
        .collect();
    let flat: Vec<Contour> = clip.iter().map(|k| Contour::new(k.path(), true)).collect();
    let path = c.path();
    let mut runs: Vec<Vec<Vec2>> = Vec::new();
    let mut current: Vec<Vec2> = Vec::new();
    let mut crossed = false;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = b - a;
        let mut ts = vec![0.0, 1.0];
        for &(p, q) in &edges {
            let e = q - p;
            let den = d.x * e.y - d.y * e.x;
            if den.abs() < 1e-12 {
                continue;
            }
            let t = ((p.x - a.x) * e.y - (p.y - a.y) * e.x) / den;
            let u = ((p.x - a.x) * d.y - (p.y - a.y) * d.x) / den;
            if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                ts.push(t);
                crossed = true;
            }
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        for s in ts.windows(2) {
            let (p, q) = (a + d * s[0], a + d * s[1]);
            if openstitch_design::geometry::contains(p + (q - p) * 0.5, &flat) {
                if current.last().is_none_or(|l| l.distance(p) > 1e-6) {
                    if current.len() >= 2 {
                        runs.push(std::mem::take(&mut current));
                    }
                    current = vec![p];
                }
                current.push(q);
            } else if current.len() >= 2 {
                runs.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
    }
    if current.len() >= 2 {
        runs.push(current);
    }
    if !crossed && runs.len() == 1 {
        return vec![c.clone()];
    }
    runs.into_iter()
        .filter(|r| r.len() >= 2)
        .map(|r| Contour::new(r, false))
        .collect()
}

impl Walk<'_> {
    fn push(&mut self, shape: SvgShape) -> Result<(), SvgError> {
        self.points += shape.contours.iter().map(|c| c.points.len()).sum::<usize>();
        if self.import.shapes.len() >= MAX_SVG_PATHS || self.points > MAX_SVG_POINTS {
            return Err(SvgError::TooLarge(format!(
                "more than {MAX_SVG_PATHS} paths or {MAX_SVG_POINTS} points"
            )));
        }
        self.import.shapes.push(shape);
        Ok(())
    }

    fn group(&mut self, group: &usvg::Group, ctx: &Ctx) -> Result<(), SvgError> {
        for node in group.children() {
            match node {
                usvg::Node::Group(g) => {
                    let mut c = ctx.clone();
                    c.opacity *= f64::from(g.opacity().get());
                    if !g.id().is_empty() {
                        c.id = g.id().to_string();
                        c.names.push(g.id().to_string());
                    }
                    let ts = g.abs_transform();
                    if let Some(cp) = g.clip_path() {
                        let r =
                            intersect(c.clip.as_deref().map(Vec::as_slice), clip_region(cp, ts));
                        c.clip = Some(std::rc::Rc::new(r));
                    }
                    if let Some(m) = g.mask() {
                        let r = intersect(c.clip.as_deref().map(Vec::as_slice), mask_region(m, ts));
                        c.clip = Some(std::rc::Rc::new(r));
                        self.import.report.approx("mask", &c.id);
                    }
                    if !g.filters().is_empty() {
                        self.import.report.approx("filter", &c.id);
                    }
                    self.group(g, &c)?;
                }
                usvg::Node::Path(path) => {
                    let start = self.import.shapes.len();
                    self.path(path, ctx)?;
                    self.annotate(path, ctx, start);
                }
                usvg::Node::Image(i) => self.import.report.skip("image", i.id()),
                usvg::Node::Text(t) => self.import.report.skip("text", t.id()),
            }
        }
        Ok(())
    }

    /// Gives the shapes `path` added (from `start`) its names, dash pattern and paint order;
    /// with `paint-order: stroke`, its stroke-only pieces go before its fills.
    fn annotate(&mut self, path: &usvg::Path, ctx: &Ctx, start: usize) {
        let mut names = ctx.names.clone();
        if !path.id().is_empty() {
            names.push(path.id().to_string());
        }
        // Dash lengths are in the path's units: scaled by its transform's mean scale.
        let t = path.abs_transform();
        let scale = f64::from((t.sx * t.sy - t.kx * t.ky).abs()).sqrt() * PX;
        let dashes = path.stroke().and_then(|s| {
            s.dasharray().map(|d| {
                (
                    d.iter()
                        .map(|&v| f64::from(v) * scale)
                        .collect::<Vec<f64>>(),
                    f64::from(s.dashoffset()) * scale,
                )
            })
        });
        let stroke_first = path.paint_order() == usvg::PaintOrder::StrokeAndFill;
        let added = &mut self.import.shapes[start..];
        for shape in added.iter_mut() {
            shape.names = names.clone();
            shape.dashes = dashes.clone();
            shape.stroke_first = stroke_first;
        }
        if stroke_first {
            added.sort_by_key(|s| s.fill.is_some());
        }
    }

    fn path(&mut self, path: &usvg::Path, ctx: &Ctx) -> Result<(), SvgError> {
        let id = if path.id().is_empty() {
            ctx.id.clone()
        } else {
            path.id().to_string()
        };
        let report = &mut self.import.report;
        if !path.is_visible() {
            if path.fill().is_some() || path.stroke().is_some() {
                report.skip("hidden", &id);
            }
            return Ok(());
        }
        let min = self.opts.min_opacity;
        let mut seen = |o: f64| {
            let keep = ctx.opacity * o >= min;
            if !keep {
                report.skip("transparent", &id);
            }
            keep
        };
        let fill = path.fill().filter(|f| seen(f64::from(f.opacity().get())));
        let stroke = path.stroke().filter(|s| seen(f64::from(s.opacity().get())));
        if fill.is_none() && stroke.is_none() {
            return Ok(());
        }
        let ts = path.abs_transform();
        let contours = contours(path.data(), &ts, fill.is_some(), stroke.is_some());
        if contours.is_empty() {
            return Ok(());
        }
        for (paint, _) in fill
            .map(|f| (f.paint(), ()))
            .into_iter()
            .chain(stroke.map(|s| (s.paint(), ())))
        {
            match paint {
                usvg::Paint::Color(_) => {}
                usvg::Paint::Pattern(_) => report.approx("pattern", &id),
                _ => report.approx("gradient", &id),
            }
        }
        let closed = contours.iter().all(|c| c.closed);
        // Fills: one, or gradient bands.
        let mut fills: Vec<(Vec<Contour>, Thread)> = Vec::new();
        if let Some(f) = fill {
            let bands = match self.opts.gradients {
                Gradients::Bands(n) if closed => gradient_bands(f.paint(), &contours, &ts, n),
                _ => None,
            };
            fills = bands.unwrap_or_else(|| vec![(contours.clone(), paint_thread(f.paint()))]);
        }
        let stroke_thread = stroke.map(|s| paint_thread(s.paint()));
        let Some(clip) = ctx.clip.clone() else {
            // Not clipped: one shape as before, or the bands and then the stroke.
            if fills.len() <= 1 {
                return self.push(SvgShape {
                    contours,
                    fill: fills.pop().map(|f| f.1),
                    stroke: stroke_thread,
                    ..SvgShape::default()
                });
            }
            for (c, t) in fills {
                self.push(SvgShape {
                    contours: c,
                    fill: Some(t),
                    stroke: None,
                    ..SvgShape::default()
                })?;
            }
            if let Some(t) = stroke_thread {
                self.push(SvgShape {
                    contours,
                    fill: None,
                    stroke: Some(t),
                    ..SvgShape::default()
                })?;
            }
            return Ok(());
        };
        // Clipped: fills cut to the visible area, strokes to their visible pieces.
        let mut kept = false;
        for (c, t) in fills {
            if closed {
                let cut = boolean::contours(&boolean::combine(&c, &clip, BoolOp::Intersection));
                let area = |c: &[Contour]| -> f64 {
                    boolean::simplify(c)
                        .iter()
                        .map(|r| boolean::region_area(r))
                        .sum()
                };
                let whole = (area(&c) - area(&cut)).abs() < 1.0;
                let c = if whole { c } else { cut };
                if !c.is_empty() {
                    kept = true;
                    self.push(SvgShape {
                        contours: c,
                        fill: Some(t),
                        stroke: None,
                        ..SvgShape::default()
                    })?;
                }
            } else {
                // An open fill sews as a running line: cut it as a stroke.
                let pieces: Vec<Contour> = c.iter().flat_map(|k| clip_stroke(k, &clip)).collect();
                if !pieces.is_empty() {
                    kept = true;
                    self.push(SvgShape {
                        contours: pieces,
                        fill: Some(t),
                        stroke: None,
                        ..SvgShape::default()
                    })?;
                }
            }
        }
        if let Some(t) = stroke_thread {
            let pieces: Vec<Contour> = contours
                .iter()
                .flat_map(|k| clip_stroke(k, &clip))
                .collect();
            if !pieces.is_empty() {
                kept = true;
                self.push(SvgShape {
                    contours: pieces,
                    fill: None,
                    stroke: Some(t),
                    ..SvgShape::default()
                })?;
            }
        }
        if !kept {
            self.import.report.skip("clipped", &id);
        }
        Ok(())
    }
}

/// Subpaths as contours in 0.1 mm. Filled paths close every subpath with an area, as SVG fills
/// do; a straight two-point subpath stays an open stroke.
fn contours(
    data: &usvg::tiny_skia_path::Path,
    t: &usvg::Transform,
    filled: bool,
    stroked: bool,
) -> Vec<Contour> {
    use usvg::tiny_skia_path::PathSegment;
    let map = |p: usvg::tiny_skia_path::Point| {
        let (x, y) = (f64::from(p.x), f64::from(p.y));
        v2(
            (f64::from(t.sx) * x + f64::from(t.kx) * y + f64::from(t.tx)) * PX,
            (f64::from(t.ky) * x + f64::from(t.sy) * y + f64::from(t.ty)) * PX,
        )
    };
    let mut out = Vec::new();
    let mut current: Option<Contour> = None;
    let mut last = Vec2::default();
    let finish = |c: Option<Contour>, closed: bool, out: &mut Vec<Contour>| {
        let Some(mut c) = c else { return };
        // A straight subpath of two points has no area: a fill cannot show it, so it stays an
        // open stroke (a `<line>`, whose default fill is black, #341) or is dropped.
        let flat = c.points.len() < 3 && !c.curves.iter().any(Option::is_some);
        if flat && !stroked {
            return;
        }
        let closed = !flat && (closed || filled);
        if closed {
            // A final vertex on top of the first is the closing segment's end.
            if c.points.len() > 1 && c.points[c.points.len() - 1].distance(c.points[0]) < 1e-6 {
                c.points.pop();
            } else {
                c.curves.push(None);
            }
        }
        c.closed = closed;
        if !c.curves.iter().any(Option::is_some) {
            c.curves.clear();
        }
        let enough = if closed {
            c.points.len() >= 3 || (c.points.len() >= 2 && !c.curves.is_empty())
        } else {
            c.points.len() >= 2
        };
        if enough && c.points.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            out.push(c);
        }
    };
    for segment in data.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                finish(current.take(), false, &mut out);
                last = map(p);
                current = Some(Contour::new(vec![last], false));
            }
            PathSegment::LineTo(p) => {
                if let Some(c) = &mut current {
                    last = map(p);
                    c.curves.push(None);
                    c.points.push(last);
                }
            }
            PathSegment::QuadTo(q, p) => {
                if let Some(c) = &mut current {
                    let (q, end) = (map(q), map(p));
                    let c1 = last + (q - last) * (2.0 / 3.0);
                    let c2 = end + (q - end) * (2.0 / 3.0);
                    c.curves.push(Some(Bezier { c1, c2 }));
                    c.points.push(end);
                    last = end;
                }
            }
            PathSegment::CubicTo(a, b, p) => {
                if let Some(c) = &mut current {
                    last = map(p);
                    c.curves.push(Some(Bezier {
                        c1: map(a),
                        c2: map(b),
                    }));
                    c.points.push(last);
                }
            }
            PathSegment::Close => {
                finish(current.take(), true, &mut out);
            }
        }
    }
    finish(current.take(), false, &mut out);
    out
}

fn bounds(shapes: &[SvgShape]) -> Option<Rect> {
    Rect::enclosing(
        shapes
            .iter()
            .flat_map(|s| s.contours.iter())
            .flat_map(Contour::outline_points),
    )
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    let mut v = vec![
        spec(
            "import.svg",
            "Import an SVG's paths as editable shapes (fill → fill region, stroke → running line), grouped",
            Effect::Edit,
            vec![
                req(
                    "path",
                    "string",
                    "SVG file (relative to the session directory)",
                ),
                opt(
                    "width",
                    "mm",
                    "Scale the artwork to this width (default: the SVG's own size)",
                ),
                opt("x", "mm", "Left edge (default: centred on the Design Page)"),
                opt("y", "mm", "Top edge (default: centred on the Design Page)"),
                opt(
                    "exactColors",
                    "bool",
                    "Keep the SVG's own RGB instead of the nearest Brother chart thread (default false: PE-DESIGN's behaviour)",
                ),
                opt(
                    "gradients",
                    "\"one\" | \"bands\"",
                    "A gradient fill as one colour (its colour half way along; default) or as bands of its colours",
                ),
                opt(
                    "bands",
                    "2–8",
                    "Bands per gradient with gradients: bands (default 3)",
                ),
                opt(
                    "minOpacity",
                    "0–1",
                    "Fills and strokes less opaque than this are left out and listed (default 0.1)",
                ),
                opt(
                    "settings",
                    "bool",
                    "Read stitch settings from layer and object names (\"fill a=45 s=0.4\" or \"fill angle=45 spacing=0.4\", \"satin\", \"run r=3\" or \"run repeats=3\", \"stroke.satin w=3\", \"skip\"; short and long key names alike; docs/FILE_FORMATS.md), split dashed strokes into one line per dash and sew paint-order: stroke outlines first (default false)",
                ),
            ],
            |s, p| {
                let (_, data) = read_file(s, p, MAX_SVG_BYTES)?;
                let gradients = match p.string("gradients", Some("one"))?.as_str() {
                    "one" => Gradients::OneColour,
                    "bands" => Gradients::Bands(p.int("bands", Some(3), 2, 8)? as usize),
                    _ => return Err(p.invalid("gradients", "must be \"one\" or \"bands\"")),
                };
                let opts = SvgOptions {
                    gradients,
                    min_opacity: p.opt("minOpacity", 0.1, 0.0, 1.0)?,
                };
                let import = parse_svg_with(&data, &opts).map_err(|e| p.failed(e.to_string()))?;
                let report = import.report.to_json();
                let mut out = if p.bool("settings", Some(false))? {
                    use crate::svg_settings as names;
                    let mut named = names::Report::default();
                    let prepared =
                        names::prepare(import.shapes, &names::names_by_id(&data), &mut named);
                    let (shapes, sets): (Vec<SvgShape>, Vec<names::Settings>) =
                        prepared.into_iter().unzip();
                    let mut out = import_shapes(s, p, shapes, "the SVG")?;
                    let ids: Vec<u64> = out["ids"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_u64)
                        .collect();
                    let placed: Vec<(u64, names::Settings)> = ids.into_iter().zip(sets).collect();
                    names::apply(s, &placed, &mut named);
                    out["settings"] = json!({
                        "applied": named.applied,
                        "skipped": named.skipped,
                        "dashed": named.dashed,
                        "unknown": named.unknown,
                        "invalid": named.invalid,
                    });
                    out
                } else {
                    import_shapes(s, p, import.shapes, "the SVG")?
                };
                out["skipped"] = report["skipped"].clone();
                out["approximated"] = report["approximated"].clone();
                Ok(out)
            },
        ),
        spec(
            "import.metafile",
            "Import a WMF or EMF vector image's drawing as editable shapes (brush → fill region, pen → running line), grouped",
            Effect::Edit,
            vec![
                req(
                    "path",
                    "string",
                    "WMF (placeable or standard) or EMF file (relative to the session directory)",
                ),
                opt(
                    "width",
                    "mm",
                    "Scale the artwork to this width (default: the metafile's own size)",
                ),
                opt("x", "mm", "Left edge (default: centred on the Design Page)"),
                opt("y", "mm", "Top edge (default: centred on the Design Page)"),
                opt(
                    "exactColors",
                    "bool",
                    "Keep the metafile's own RGB instead of the nearest Brother chart thread (default false, as import.svg)",
                ),
            ],
            |s, p| {
                let (_, data) = read_file(s, p, crate::metafile::MAX_METAFILE_BYTES)?;
                let shapes =
                    crate::metafile::parse_metafile(&data).map_err(|e| p.failed(e.to_string()))?;
                import_shapes(s, p, shapes, "the metafile")
            },
        ),
    ];
    #[cfg(feature = "digitize")]
    v.extend(background_commands());
    v
}

/// Matches colours to the thread chart unless `exactColors`, then places the shapes.
fn import_shapes(
    s: &mut EngineState,
    p: &Params,
    mut shapes: Vec<SvgShape>,
    what: &str,
) -> crate::CommandResult {
    // PE-DESIGN 11.50 gives every imported fill and stroke the nearest BROTHER
    // EMBROIDERY chart thread (thread-match-swatches); `exactColors` keeps the RGB.
    if !p.bool("exactColors", Some(false))? {
        for shape in &mut shapes {
            for t in [&mut shape.fill, &mut shape.stroke].into_iter().flatten() {
                *t = openstitch_core::nearest_brother_thread(t);
            }
        }
    }
    let Some(b) = bounds(&shapes) else {
        return Err(p.failed(format!("{what} has no visible filled or stroked shapes")));
    };
    place(s, p, shapes, b)
}

fn place(s: &mut EngineState, p: &Params, shapes: Vec<SvgShape>, b: Rect) -> crate::CommandResult {
    let natural = b.width().max(1e-6);
    let width = p.opt("width", (natural / MM).clamp(0.1, 2000.0), 0.1, 2000.0)? * MM;
    let scale = width / natural;
    let height = b.height() * scale;
    if height > 2000.0 * MM {
        return Err(p.invalid("width", "makes the artwork taller than 2000 mm"));
    }
    let page = s.design.page;
    let x = p.opt("x", (page.width - width) / 2.0 / MM, -5000.0, 5000.0)? * MM;
    let y = p.opt("y", (page.height - height) / 2.0 / MM, -5000.0, 5000.0)? * MM;
    let t = Transform::translation(-b.min_x, -b.min_y)
        .then(&Transform::scaling(scale, scale, v2(0.0, 0.0)))
        .then(&Transform::translation(x, y));
    let group = (shapes.len() > 1).then(|| s.design.new_group_id());
    let mut ids = Vec::with_capacity(shapes.len());
    for shape in shapes {
        let closed = shape.contours.iter().all(|c| c.closed);
        let region = shape
            .fill
            .clone()
            .filter(|_| closed)
            .map(|t| RegionSewing::new(RegionType::Fill, t));
        let line = shape
            .stroke
            .or_else(|| (!closed).then(|| shape.fill.clone()).flatten())
            .map(|t| LineSewing::new(LineType::Running, t));
        let contours = shape.contours.iter().map(|c| c.transformed(&t)).collect();
        let mut object = DesignObject::shape(contours, region, line);
        object.group = group;
        ids.push(s.design.add(object));
    }
    Ok(json!({ "ids": ids, "group": group }))
}

#[cfg(feature = "digitize")]
fn background_commands() -> Vec<CommandSpec> {
    use crate::sandbox::Access;
    use openstitch_design::{BackgroundImage, BackgroundLayer, ImageAdjust};
    use openstitch_digitize::Raster;
    use openstitch_digitize::raster::MAX_FILE_BYTES;

    /// The background's layer as `background.setLayer` reports it.
    fn layer_json(b: &openstitch_design::BackgroundImage) -> serde_json::Value {
        use openstitch_design::BackgroundLayer;
        match b.layer {
            BackgroundLayer::Bottom => json!({ "position": "bottom", "opacity": b.opacity }),
            BackgroundLayer::Top => json!({ "position": "top", "opacity": b.opacity }),
            BackgroundLayer::AfterObject { after } => {
                json!({ "position": "afterObject", "afterId": after, "opacity": b.opacity })
            }
        }
    }

    fn opacity(p: &Params, default: f64) -> Result<f64, CommandError> {
        p.opt("opacity", default, 0.0, 1.0)
    }

    fn background<'a>(
        s: &'a mut EngineState,
        p: &Params,
    ) -> Result<&'a mut BackgroundImage, CommandError> {
        s.design
            .background
            .as_mut()
            .ok_or_else(|| p.failed("no background image"))
    }

    /// The background image's pixels as shown on the Design Page.
    fn shown(s: &EngineState, p: &Params) -> Result<Raster, CommandError> {
        let b = s
            .design
            .background
            .as_ref()
            .ok_or_else(|| p.failed("no background image"))?;
        let path = s.resolve(&b.path, Access::Read, &p.command)?;
        let meta = std::fs::metadata(&path)
            .map_err(|_| p.failed(format!("cannot read {}", path.display())))?;
        if meta.len() > MAX_FILE_BYTES as u64 {
            return Err(p.failed("background image too large"));
        }
        let data = std::fs::read(&path)
            .map_err(|_| p.failed(format!("cannot read {}", path.display())))?;
        let image = Raster::decode(&data).map_err(|e| p.failed(e.to_string()))?;
        Ok(image.with_adjust(&b.adjust))
    }

    vec![
        spec(
            "background.set",
            "Place a PNG, JPEG or BMP image behind the design as a tracing reference (not sewn)",
            Effect::Edit,
            vec![
                req(
                    "path",
                    "string",
                    "Image file (relative to the session directory)",
                ),
                opt(
                    "width",
                    "mm",
                    "Width on the page (default: fit the Design Page)",
                ),
                opt("x", "mm", "Left edge (default: centred on the Design Page)"),
                opt("y", "mm", "Top edge (default: centred on the Design Page)"),
                opt("opacity", "0–1", "Default 0.5"),
            ],
            |s, p| {
                let (path, data) = read_file(s, p, MAX_FILE_BYTES)?;
                let image = Raster::decode(&data).map_err(|e| p.failed(e.to_string()))?;
                let (pw, ph) = (f64::from(image.width()), f64::from(image.height()));
                let page = s.design.page;
                let fit = (page.width / pw).min(page.height / ph) * pw / MM;
                let width = p.opt("width", fit.clamp(1.0, 2000.0), 1.0, 2000.0)? * MM;
                let height = width * ph / pw;
                if height > 2000.0 * MM {
                    return Err(p.invalid("width", "makes the image taller than 2000 mm"));
                }
                let x = p.opt("x", (page.width - width) / 2.0 / MM, -5000.0, 5000.0)? * MM;
                let y = p.opt("y", (page.height - height) / 2.0 / MM, -5000.0, 5000.0)? * MM;
                // Stored absolute, so the reference survives opening the project elsewhere.
                let path = std::fs::canonicalize(&path).unwrap_or(path);
                let background = BackgroundImage {
                    path: path.display().to_string(),
                    pixel_width: image.width(),
                    pixel_height: image.height(),
                    x,
                    y,
                    width,
                    height,
                    opacity: opacity(p, 0.5)?,
                    visible: true,
                    adjust: ImageAdjust::default(),
                    layer: BackgroundLayer::default(),
                };
                let out = json!({
                    "path": background.path,
                    "pixels": [image.width(), image.height()],
                    "widthMM": width / MM,
                    "heightMM": height / MM,
                });
                s.design.background = Some(background);
                Ok(out)
            },
        ),
        spec(
            "background.show",
            "Show or hide the background image, or change its opacity",
            Effect::Edit,
            vec![
                opt("visible", "bool", "Show (default: unchanged)"),
                opt("opacity", "0–1", "Opacity (default: unchanged)"),
            ],
            |s, p| {
                let Some(b) = s.design.background.as_mut() else {
                    return Err(p.failed("no background image"));
                };
                let visible = p.bool("visible", Some(b.visible))?;
                let o = opacity(p, b.opacity)?;
                b.visible = visible;
                b.opacity = o;
                Ok(json!({ "visible": visible, "opacity": o }))
            },
        ),
        spec(
            "background.setLayer",
            "Where the background image is drawn (beyond PE-DESIGN): bottom (behind the design, the default), top (over it, with its opacity) or afterObject (above that object, below the ones sewn after it). It never sews and machine files are unchanged",
            Effect::Edit,
            vec![
                req(
                    "position",
                    "bottom|top|afterObject",
                    "Where to draw the image",
                ),
                opt(
                    "afterId",
                    "int",
                    "afterObject: the object the image sits above",
                ),
                opt("opacity", "0–1", "Opacity (default: unchanged)"),
            ],
            |s, p| {
                let layer = match p.string("position", None)?.as_str() {
                    "bottom" => BackgroundLayer::Bottom,
                    "top" => BackgroundLayer::Top,
                    "afterObject" => {
                        let id = p.int("afterId", None, 0, i64::MAX)? as u64;
                        if s.design.index_of(id).is_none() {
                            return Err(p.not_found(format!("object {id}")));
                        }
                        BackgroundLayer::AfterObject { after: id }
                    }
                    _ => return Err(p.invalid("position", "use bottom, top or afterObject")),
                };
                if p.has("afterId") && !matches!(layer, BackgroundLayer::AfterObject { .. }) {
                    return Err(p.invalid("afterId", "only with position afterObject"));
                }
                let b = background(s, p)?;
                let o = opacity(p, b.opacity)?;
                b.layer = layer;
                b.opacity = o;
                Ok(layer_json(b))
            },
        ),
        spec(
            "background.rotate",
            "Turn the background image 90° about its centre (Image ▸ Rotate Right / Left)",
            Effect::Edit,
            vec![req(
                "direction",
                "\"right\" | \"left\"",
                "Right = clockwise",
            )],
            |s, p| {
                let right = match p.string("direction", None)?.as_str() {
                    "right" => true,
                    "left" => false,
                    _ => return Err(p.invalid("direction", "must be \"right\" or \"left\"")),
                };
                let b = background(s, p)?;
                let [l, t, r, bt] = b.adjust.crop;
                // The crop belongs to the turned image, so its sides turn with it.
                b.adjust.crop = if right { [bt, l, t, r] } else { [t, r, bt, l] };
                b.adjust.rotation = (b.adjust.rotation + if right { 1 } else { 3 }) % 4;
                let (cx, cy) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
                (b.width, b.height) = (b.height, b.width);
                (b.x, b.y) = (cx - b.width / 2.0, cy - b.height / 2.0);
                Ok(json!({ "rotation": u32::from(b.adjust.rotation) * 90 }))
            },
        ),
        spec(
            "background.modify",
            "Move, scale, crop or adjust the colours of the background image (Image ▸ Modify Image); the image file is not changed",
            Effect::Edit,
            vec![
                opt("x", "mm", "Left edge (default: unchanged)"),
                opt("y", "mm", "Top edge (default: unchanged)"),
                opt(
                    "width",
                    "mm",
                    "Width; the height follows the image's proportions (default: unchanged)",
                ),
                opt(
                    "crop",
                    "[left, top, right, bottom] %",
                    "Cut off each side of the turned image, 0–90 % of its width or height; at least 5 % stays. The kept part stays where it was",
                ),
                opt("brightness", "-100–100", "0 = as in the file"),
                opt("contrast", "-100–100", "0 = as in the file"),
                opt("saturation", "-100–100", "-100 = greys; 0 = as in the file"),
            ],
            |s, p| {
                let crop = match p.get("crop") {
                    None => None,
                    Some(_) => {
                        let v = p.array("crop")?;
                        let mut c = [0.0; 4];
                        if v.len() != 4 {
                            return Err(p.invalid("crop", "must be [left, top, right, bottom]"));
                        }
                        for (i, x) in v.iter().enumerate() {
                            let n = x
                                .as_f64()
                                .filter(|n| (0.0..=ImageAdjust::MAX_CROP * 100.0).contains(n))
                                .ok_or_else(|| p.invalid("crop", "each side must be 0–90 %"))?;
                            c[i] = n / 100.0;
                        }
                        let keep = 1.0 - ImageAdjust::MIN_KEPT + 1e-9;
                        if c[0] + c[2] > keep || c[1] + c[3] > keep {
                            return Err(p.invalid("crop", "must keep at least 5 % of the image"));
                        }
                        Some(c)
                    }
                };
                let level = |key: &str, now: f64| p.opt(key, now, -100.0, 100.0);
                let b = background(s, p)?;
                let brightness = level("brightness", b.adjust.brightness)?;
                let contrast = level("contrast", b.adjust.contrast)?;
                let saturation = level("saturation", b.adjust.saturation)?;
                let mut n = b.clone();
                if let Some(c) = crop {
                    // Scale stays: the whole turned image would cover `full`.
                    let [l, t, r, bt] = n.adjust.crop;
                    let fw = n.width / (1.0 - l - r);
                    let fh = n.height / (1.0 - t - bt);
                    let (fx, fy) = (n.x - l * fw, n.y - t * fh);
                    n.x = fx + c[0] * fw;
                    n.y = fy + c[1] * fh;
                    n.width = fw * (1.0 - c[0] - c[2]);
                    n.height = fh * (1.0 - c[1] - c[3]);
                    n.adjust.crop = c;
                }
                if p.has("width") {
                    let w = p.opt("width", n.width / MM, 1.0, 2000.0)? * MM;
                    n.height *= w / n.width;
                    n.width = w;
                }
                if n.height > 2000.0 * MM || n.width > 2000.0 * MM {
                    return Err(p.invalid("width", "makes the image larger than 2000 mm"));
                }
                n.x = p.opt("x", n.x / MM, -5000.0, 5000.0)? * MM;
                n.y = p.opt("y", n.y / MM, -5000.0, 5000.0)? * MM;
                n.adjust.brightness = brightness;
                n.adjust.contrast = contrast;
                n.adjust.saturation = saturation;
                let out = json!({
                    "x": n.x / MM, "y": n.y / MM, "widthMM": n.width / MM, "heightMM": n.height / MM,
                    "crop": n.adjust.crop.map(|c| c * 100.0),
                    "brightness": brightness, "contrast": contrast, "saturation": saturation,
                });
                *b = n;
                Ok(out)
            },
        ),
        spec(
            "background.save",
            "Save the background image as shown (turned, cropped, colours adjusted) to a PNG, BMP or JPEG file, chosen by the extension",
            Effect::File,
            vec![req(
                "path",
                "string",
                "Image file to write: .png, .bmp or .jpg (relative to the session directory)",
            )],
            |s, p| {
                let target = s.resolve(&p.string("path", None)?, Access::Write, &p.command)?;
                let ext = target
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or_default()
                    .to_string();
                let image = shown(s, p)?;
                let bytes = image
                    .encode_as(&ext)
                    .map_err(|e| p.invalid("path", e.to_string()))?;
                std::fs::write(&target, bytes).map_err(|e| p.failed(e.to_string()))?;
                Ok(json!({
                    "path": target.display().to_string(),
                    "pixels": [image.width(), image.height()],
                }))
            },
        ),
        spec(
            "background.clear",
            "Remove the background image",
            Effect::Edit,
            vec![],
            |s, p| {
                if s.design.background.take().is_none() {
                    return Err(p.failed("no background image"));
                }
                Ok(json!({}))
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_circle_and_path_become_contours() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96" viewBox="0 0 96 96">
            <rect x="0" y="0" width="48" height="24" fill="#ff0000"/>
            <circle cx="48" cy="48" r="24" fill="none" stroke="#0000ff"/>
            <path d="M 0 96 Q 48 48 96 96" fill="none" stroke="green"/>
            <g transform="translate(10 10)"><path d="M0 0 L10 0 L10 10 Z M2 2 L8 2 L8 8 Z" fill="#00ff00"/></g>
            <text x="0" y="0">ignored</text>
            <image href="/etc/passwd" width="10" height="10"/>
        </svg>"##;
        let shapes = parse_svg(svg).expect("parse");
        assert_eq!(shapes.len(), 4);
        // 48 px = 12.7 mm = 127 units.
        let r = &shapes[0];
        assert_eq!(r.fill.as_ref().map(Thread::hex).as_deref(), Some("#ff0000"));
        assert!(r.stroke.is_none());
        assert_eq!(r.contours.len(), 1);
        assert!(r.contours[0].closed);
        let rb = Rect::enclosing(r.contours[0].points.iter().copied()).expect("bounds");
        assert!((rb.width() - 127.0).abs() < 1e-6 && (rb.height() - 63.5).abs() < 1e-6);
        // The circle is four exact cubic segments, stroked only.
        let c = &shapes[1];
        assert!(c.fill.is_none());
        assert_eq!(
            c.stroke.as_ref().map(Thread::hex).as_deref(),
            Some("#0000ff")
        );
        assert!(c.contours[0].closed && c.contours[0].has_curves());
        // The quadratic is raised to one cubic; open stroke.
        let q = &shapes[2].contours[0];
        assert!(!q.closed);
        assert_eq!(q.segment_count(), 1);
        assert!(q.curve(0).is_some());
        // Transformed group: two closed subpaths (outer and hole).
        let g = &shapes[3];
        assert_eq!(g.contours.len(), 2);
        assert!((g.contours[0].points[0].x - 10.0 * PX).abs() < 1e-6);
    }

    #[test]
    fn line_elements_import_as_open_strokes() {
        // #341: `<line>` has SVG's default black fill, which used to close and drop it.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100">
          <line x1="50" y1="10" x2="90" y2="10" stroke="#000000"/>
          <line x1="10" y1="10" x2="40" y2="50" stroke="#000000"/>
          <line x1="10" y1="90" x2="10" y2="60" stroke="#ff0000" stroke-width="2"/>
          <path d="M 60 90 L 95 90 Z" stroke="#000000" fill="none"/>
          <path d="M 0 0 L 5 5" fill="#00ff00"/>
        </svg>"##;
        let shapes = parse_svg(svg).expect("parse");
        // The fill-only straight subpath shows nothing in SVG and is not imported.
        assert_eq!(shapes.len(), 4);
        for s in &shapes {
            assert_eq!(s.contours.len(), 1);
            assert!(!s.contours[0].closed && s.contours[0].points.len() == 2);
            assert!(s.stroke.is_some());
        }
        assert_eq!(
            shapes[2].stroke.as_ref().map(Thread::hex).as_deref(),
            Some("#ff0000")
        );
    }

    #[test]
    fn css_variables_are_resolved_with_fallbacks_and_unknown_ones_listed() {
        let mut r = SvgReport::default();
        let svg = br#"<svg><style>:root{--a: #ff0000; --b-2:blue}</style><rect style="fill:var(--a)"/><rect fill="var(--b-2)"/><rect fill="var(--c, #00ff00)"/><rect fill="var(--d)"/></svg>"#;
        let out = String::from_utf8(css_variables(svg, &mut r).expect("vars")).expect("utf8");
        assert!(out.contains(r#"style="fill:#ff0000""#), "{out}");
        assert!(out.contains(r#"fill="blue""#) && out.contains(r##"fill="#00ff00""##));
        assert!(out.contains(r#"fill="var(--d)""#));
        assert_eq!(r.approximated["cssVariable"], (1, vec!["--d".to_string()]));
        assert_eq!(css_variables(b"<svg/>", &mut r), None);
    }

    #[test]
    fn hostile_svgs_fail_cleanly() {
        for bad in [
            &b""[..],
            b"not xml",
            b"<svg",
            b"<svg xmlns='http://www.w3.org/2000/svg'><path d='M 0 0 L nan nan'/></svg>",
            b"<?xml version='1.0'?><!DOCTYPE x [<!ENTITY a 'aaaaaaaaaa'><!ENTITY b '&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;'><!ENTITY c '&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;'>]><svg xmlns='http://www.w3.org/2000/svg'><text>&c;</text></svg>",
        ] {
            let _ = parse_svg(bad);
        }
        let mut many = String::from("<svg xmlns='http://www.w3.org/2000/svg'>");
        for _ in 0..=MAX_SVG_PATHS {
            many.push_str("<rect width='1' height='1'/>");
        }
        many.push_str("</svg>");
        assert!(matches!(
            parse_svg(many.as_bytes()),
            Err(SvgError::TooLarge(_))
        ));
    }
}
