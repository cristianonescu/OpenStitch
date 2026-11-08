//! Production aids beyond PE-DESIGN, all off by default so PE-compatible output is unchanged:
//! a basting box sewn first (#509), machine stops after objects and at jumps (#512) and a
//! nearest-start rule applied when the design is compiled (#547). The compiler
//! ([`crate::compile`]) reads these settings; the fill and satin generators are not touched:
//! the start rule only sets an object's entry point, as `object.setEntryExit` would.

use crate::boolean;
use crate::geometry::{Contour, Rect, Vec2, v2};
use crate::machine::HoopShape;
use crate::model::{Design, DesignObject, ObjectKind};
use openstitch_core::Thread;
use serde::{Deserialize, Serialize};

/// The design's production settings. Omitted from `.osd` while all off.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Production {
    /// A tacking run sewn before everything else (#509); `None` = off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basting: Option<Basting>,
    /// For machines without a thread cutter: a stop before every jump the machine would
    /// otherwise sew as a loose thread (#512).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stop_at_jumps: bool,
    /// Where objects start (#547).
    #[serde(default, skip_serializing_if = "AutoStart::is_off")]
    pub auto_start: AutoStart,
    /// Moves into an object of the same thread that lie inside fills sewn later are sewn as
    /// short running stitches hidden under them instead of a jump and trim (#545).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub travel_under: bool,
    /// Every shape's region leaves out the stitching later fills and satins hide, keeping
    /// this overlap (mm) under their edges (#511); text only with its own setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove_hidden: Option<f64>,
}

impl Production {
    pub fn is_default(&self) -> bool {
        *self == Production::default()
    }
}

/// The basting run's settings (lengths in mm).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Basting {
    pub shape: BastingShape,
    /// Distance from the design, mm.
    pub margin: f64,
    /// Running stitch length, mm.
    pub stitch: f64,
    /// Its own thread (a separate colour block); `None` sews it in the first thread, with no
    /// added colour change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<Thread>,
}

impl Default for Basting {
    fn default() -> Self {
        Basting {
            shape: BastingShape::Box,
            margin: 5.0,
            stitch: 6.0,
            thread: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BastingShape {
    /// The design's bounding box grown by the margin, kept inside the hoop.
    #[default]
    Box,
    /// The outline of all objects grown by the margin.
    Outline,
}

impl BastingShape {
    pub fn id(self) -> &'static str {
        match self {
            BastingShape::Box => "box",
            BastingShape::Outline => "outline",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [BastingShape::Box, BastingShape::Outline]
            .into_iter()
            .find(|s| s.id() == id)
    }
}

/// Where each object starts when the design is compiled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoStart {
    /// Each object starts at its own entry point (PE-DESIGN's behaviour).
    #[default]
    Off,
    /// A shape without an entry point of its own starts nearest to where the needle stopped:
    /// a region or closed line at the nearest point of its outline, an open line at its nearer
    /// end. Text and stitch patterns keep their own start.
    Nearest,
}

impl AutoStart {
    pub fn is_off(&self) -> bool {
        *self == AutoStart::Off
    }

    pub fn id(self) -> &'static str {
        match self {
            AutoStart::Off => "off",
            AutoStart::Nearest => "nearest",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [AutoStart::Off, AutoStart::Nearest]
            .into_iter()
            .find(|s| s.id() == id)
    }
}

/// Model units (0.1 mm) per mm.
const MM: f64 = 10.0;
/// The basting run keeps this far inside the hoop's sewing field (0.1 mm).
const HOOP_INSET: f64 = 5.0;
/// The longest stitch the basting run sews (0.1 mm), under the 12.1 mm machine limit.
const MAX_STITCH: f64 = 120.0;

/// The hoop's sewing field in model coordinates: its shape and its box, centred on the Design
/// Page (the page itself without a hoop).
pub fn hoop_field(d: &Design) -> (HoopShape, Rect) {
    let c = d.page.center();
    let (w, h) = d
        .machine
        .hoop_size()
        .map(|(w, h)| (w * MM, h * MM))
        .unwrap_or((d.page.width, d.page.height));
    let shape = if d.machine.hoop_size().is_some() {
        d.machine.hoop_shape()
    } else {
        HoopShape::Rectangle
    };
    let r = Rect {
        min_x: c.x - w / 2.0,
        min_y: c.y - h / 2.0,
        max_x: c.x + w / 2.0,
        max_y: c.y + h / 2.0,
    };
    (shape, r)
}

/// Whether `p` lies in the hoop field shrunk by `inset`.
fn in_field(shape: HoopShape, f: &Rect, p: Vec2, inset: f64) -> bool {
    shape.contains(
        f.width() - 2.0 * inset,
        f.height() - 2.0 * inset,
        p.x - f.min_x - inset,
        p.y - f.min_y - inset,
        1e-6,
    )
}

/// `p` pulled into the hoop field shrunk by `inset`: clamped for a rectangle, moved towards
/// the centre onto the ellipse for an oval or round hoop.
fn into_field(shape: HoopShape, f: &Rect, p: Vec2, inset: f64) -> Vec2 {
    let (lo_x, lo_y, hi_x, hi_y) = (
        f.min_x + inset,
        f.min_y + inset,
        f.max_x - inset,
        f.max_y - inset,
    );
    match shape {
        HoopShape::Rectangle => v2(p.x.clamp(lo_x, hi_x), p.y.clamp(lo_y, hi_y)),
        HoopShape::Oval | HoopShape::Round => {
            let (cx, cy) = ((lo_x + hi_x) / 2.0, (lo_y + hi_y) / 2.0);
            let (a, b) = ((hi_x - lo_x) / 2.0, (hi_y - lo_y) / 2.0);
            let (dx, dy) = (p.x - cx, p.y - cy);
            let e = (dx / a).powi(2) + (dy / b).powi(2);
            if e <= 1.0 {
                p
            } else {
                let k = 1.0 / e.sqrt();
                v2(cx + dx * k, cy + dy * k)
            }
        }
    }
}

/// The objects the basting run goes round: every visible object's outline area.
fn sewn_bounds(d: &Design) -> Option<Rect> {
    Rect::enclosing(
        d.objects
            .iter()
            .filter(|o| o.visible)
            .filter_map(DesignObject::bounds)
            .flat_map(|b| [v2(b.min_x, b.min_y), v2(b.max_x, b.max_y)]),
    )
}

/// The basting path: closed loops (first point repeated at the end), as needle points at the
/// set stitch length. Empty when the design has nothing to go round.
pub fn basting_runs(d: &Design, b: &Basting) -> Vec<Vec<Vec2>> {
    let (shape, field) = hoop_field(d);
    let stitch = (b.stitch * MM).clamp(10.0, MAX_STITCH);
    let margin = b.margin * MM;
    let loops: Vec<Vec<Vec2>> = match b.shape {
        BastingShape::Box => {
            let Some(bounds) = sewn_bounds(d) else {
                return Vec::new();
            };
            vec![box_loop(shape, &field, &bounds, margin)]
        }
        BastingShape::Outline => outline_loops(d, margin)
            .into_iter()
            .map(|l| {
                densify(&l, 10.0)
                    .into_iter()
                    .map(|p| into_field(shape, &field, p, HOOP_INSET))
                    .collect()
            })
            .collect(),
    };
    loops
        .iter()
        .map(|l| running(l, stitch))
        .filter(|r| r.len() >= 3)
        .collect()
}

/// The box round `bounds` at the largest margin up to `margin` (in 0.5 mm steps) whose corners
/// all lie in the hoop field; when even the bare box does not fit, the box at `margin` with
/// its corners cut along the field's edge.
fn box_loop(shape: HoopShape, field: &Rect, bounds: &Rect, margin: f64) -> Vec<Vec2> {
    let corners = |m: f64| {
        vec![
            v2(bounds.min_x - m, bounds.min_y - m),
            v2(bounds.max_x + m, bounds.min_y - m),
            v2(bounds.max_x + m, bounds.max_y + m),
            v2(bounds.min_x - m, bounds.max_y + m),
        ]
    };
    let mut m = margin;
    while m >= 0.0 {
        let c = corners(m);
        if c.iter().all(|&p| in_field(shape, field, p, HOOP_INSET)) {
            let mut l = c.clone();
            l.push(c[0]);
            return l;
        }
        m -= 5.0;
    }
    let mut c = corners(margin);
    c.push(c[0]);
    densify(&c, 10.0)
        .into_iter()
        .map(|p| into_field(shape, field, p, HOOP_INSET))
        .collect()
}

/// The outer outlines of all visible objects grown by `margin`: closed contours as drawn,
/// open lines and stitch patterns as their bounding box.
fn outline_loops(d: &Design, margin: f64) -> Vec<Vec<Vec2>> {
    let mut parts: Vec<Vec<Contour>> = Vec::new();
    for o in d.objects.iter().filter(|o| o.visible) {
        let mut part: Vec<Contour> = Vec::new();
        if o.kind != ObjectKind::Stitches {
            let closed: Vec<Contour> = o.contours.iter().filter(|c| c.closed).cloned().collect();
            part.extend(
                boolean::simplify(&closed)
                    .into_iter()
                    .filter_map(|r| r.into_iter().next()),
            );
        }
        let open = o.kind == ObjectKind::Stitches || o.contours.iter().any(|c| !c.closed);
        if open {
            let pts: Vec<Vec2> = if o.kind == ObjectKind::Stitches {
                o.points()
            } else {
                o.contours
                    .iter()
                    .filter(|c| !c.closed)
                    .flat_map(Contour::outline_points)
                    .collect()
            };
            if let Some(b) = Rect::enclosing(pts) {
                part.push(Contour::rectangle(
                    b.min_x,
                    b.min_y,
                    b.width().max(1.0),
                    b.height().max(1.0),
                ));
            }
        }
        if !part.is_empty() {
            parts.push(part);
        }
    }
    let union = boolean::union_all(parts.iter().map(|p| &p[..]));
    let outer: Vec<Contour> = union.iter().filter_map(|r| r.first().cloned()).collect();
    let grown = if margin > 0.0 {
        boolean::offset(&outer, margin, true)
    } else {
        boolean::simplify(&outer)
    };
    grown
        .iter()
        .filter_map(|r| r.first())
        .map(Contour::path)
        .filter(|p| p.len() >= 3)
        .collect()
}

/// `path` with extra points so no segment is longer than `step`.
fn densify(path: &[Vec2], step: f64) -> Vec<Vec2> {
    let mut out = Vec::new();
    for w in path.windows(2) {
        let n = ((w[0].distance(w[1]) / step).ceil() as usize).max(1);
        for k in 0..n {
            out.push(w[0].lerp(w[1], k as f64 / n as f64));
        }
    }
    out.extend(path.last());
    out
}

/// Needle points along `path` about `stitch` apart: the path is split at its corners (turns
/// over 30°), which stay needle points, and each piece is divided into equal stitches.
pub fn running(path: &[Vec2], stitch: f64) -> Vec<Vec2> {
    let mut pts: Vec<Vec2> = Vec::with_capacity(path.len());
    for &p in path {
        if pts.last().is_none_or(|q: &Vec2| q.distance(p) > 0.05) {
            pts.push(p);
        }
    }
    if pts.len() < 2 {
        return pts;
    }
    let corner = |i: usize| {
        let (a, b, c) = (pts[i - 1], pts[i], pts[i + 1]);
        let (u, v) = (b - a, c - b);
        let len = |w: Vec2| (w.x * w.x + w.y * w.y).sqrt();
        let cos = (u.x * v.x + u.y * v.y) / (len(u) * len(v));
        cos < 0.866
    };
    let mut out = vec![pts[0]];
    let mut piece = vec![pts[0]];
    for i in 1..pts.len() {
        piece.push(pts[i]);
        if i + 1 == pts.len() || corner(i) {
            out.extend(divide(&piece, stitch).into_iter().skip(1));
            piece = vec![pts[i]];
        }
    }
    out
}

/// Points dividing the polyline `piece` into equal lengths about `stitch` long, ends included.
fn divide(piece: &[Vec2], stitch: f64) -> Vec<Vec2> {
    let lens: Vec<f64> = piece.windows(2).map(|w| w[0].distance(w[1])).collect();
    let total: f64 = lens.iter().sum();
    let n = ((total / stitch).round() as usize)
        .max((total / MAX_STITCH).ceil() as usize)
        .max(1);
    let step = total / n as f64;
    let mut out = vec![piece[0]];
    let (mut seg, mut at) = (0, 0.0);
    for k in 1..n {
        let target = step * k as f64;
        while seg < lens.len() - 1 && at + lens[seg] < target {
            at += lens[seg];
            seg += 1;
        }
        let t = if lens[seg] > 0.0 {
            ((target - at) / lens[seg]).clamp(0.0, 1.0)
        } else {
            0.0
        };
        out.push(piece[seg].lerp(piece[seg + 1], t));
    }
    out.push(*piece.last().expect("non-empty"));
    out
}

/// `object` with its entry point set by the nearest-start rule from needle position `from`,
/// or `None` when the rule leaves it alone (text, stitch patterns, an entry point already set).
/// A region without its own exit point also ends nearest `toward`, where the next object is
/// (#547): the move out of it shrinks as the move into it does.
pub fn nearest_start(
    object: &DesignObject,
    from: Vec2,
    toward: Option<Vec2>,
) -> Option<DesignObject> {
    if object.kind != ObjectKind::Shape {
        return None;
    }
    let mut o = object.clone();
    if let Some(r) = &mut o.region {
        if r.entry.is_some() {
            return None;
        }
        let closed = || object.contours.iter().filter(|c| c.closed);
        r.entry = Some(nearest_on(closed(), from)?);
        // Not a motif band: its tiles follow the column, and an exit moved there turned the
        // band round (motif-band: travel 37 -> 62 mm).
        if r.exit.is_none()
            && r.motif_band.is_none()
            && let Some(t) = toward
        {
            r.exit = nearest_on(closed(), t);
        }
        return Some(o);
    }
    let l = o.line.as_mut()?;
    if l.entry.is_some() {
        return None;
    }
    let c = object.contours.first()?;
    l.entry = Some(if c.closed {
        nearest_on(std::iter::once(c), from)?
    } else {
        let p = c.path();
        let (a, b) = (*p.first()?, *p.last()?);
        if b.distance(from) < a.distance(from) {
            b
        } else {
            a
        }
    });
    Some(o)
}

/// Where the needle heads after `object`: the point of `next` (the next object sewn) nearest
/// to `object`'s middle (#547).
pub fn heading(object: &DesignObject, next: &DesignObject) -> Option<Vec2> {
    let b = crate::geometry::Rect::enclosing(object.contours.iter().flat_map(|c| c.path()))?;
    nearest_on(
        next.contours.iter(),
        v2((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0),
    )
}

/// The point of `contours` nearest to `p`.
fn nearest_on<'a>(contours: impl Iterator<Item = &'a Contour>, p: Vec2) -> Option<Vec2> {
    contours
        .map(|c| crate::knife::nearest(p, &c.path()))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, q)| q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_start_ends_a_region_toward_the_next_object() {
        // #547: a 20 mm square entered from the left ends nearest the point above it; a
        // region with its own exit keeps it, and with no next object none is set.
        let sq = Contour::new(
            vec![
                v2(0.0, 0.0),
                v2(200.0, 0.0),
                v2(200.0, 200.0),
                v2(0.0, 200.0),
            ],
            true,
        );
        let fill = crate::model::RegionSewing::new(
            crate::model::RegionType::Fill,
            openstitch_core::Thread::rgb(0xff0000),
        );
        let mut o = DesignObject::shape(vec![sq], Some(fill), None);
        let started = nearest_start(&o, v2(-50.0, 100.0), Some(v2(100.0, -80.0))).expect("set");
        let r = started.region.expect("region");
        assert_eq!(r.entry, Some(v2(0.0, 100.0)));
        assert_eq!(r.exit, Some(v2(100.0, 0.0)));
        let none = nearest_start(&o, v2(-50.0, 100.0), None).expect("set");
        assert_eq!(none.region.and_then(|r| r.exit), None, "no next object");
        o.region.as_mut().expect("region").exit = Some(v2(200.0, 200.0));
        let own = nearest_start(&o, v2(-50.0, 100.0), Some(v2(100.0, -80.0))).expect("set");
        assert_eq!(own.region.and_then(|r| r.exit), Some(v2(200.0, 200.0)));
    }

    #[test]
    fn running_keeps_corners_and_length() {
        let sq = vec![
            v2(0.0, 0.0),
            v2(500.0, 0.0),
            v2(500.0, 500.0),
            v2(0.0, 500.0),
            v2(0.0, 0.0),
        ];
        let r = running(&sq, 60.0);
        for c in &sq {
            assert!(r.iter().any(|p| p.distance(*c) < 1e-6), "corner {c:?} kept");
        }
        for w in r.windows(2) {
            let d = w[0].distance(w[1]);
            assert!((d - 60.0).abs() <= 6.0, "stitch {d}");
        }
    }

    #[test]
    fn into_field_reaches_the_ellipse() {
        let f = Rect {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1000.0,
            max_y: 600.0,
        };
        let p = into_field(HoopShape::Oval, &f, v2(1000.0, 600.0), 0.0);
        assert!(in_field(HoopShape::Oval, &f, p, -0.01));
        assert!(!in_field(HoopShape::Oval, &f, v2(990.0, 590.0), 0.0));
    }
}
