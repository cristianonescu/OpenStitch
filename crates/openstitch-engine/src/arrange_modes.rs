//! Arrange Copies beyond PE-DESIGN (#476): copies along a line (`mode: "path"`), copies
//! scattered at random inside a closed shape (`mode: "scatter"`) and the kaleidoscope option of
//! the circle (`mirrorAlternate`). Phase 1: the copies are ordinary objects; they keep no link
//! to the guide. The PE-DESIGN modes (mirror, circle, matrix) are unchanged.

use crate::commands::MM;
use crate::params::{CommandError, Params};
use openstitch_design::geometry::contains;
use openstitch_design::{Contour, Design, Rect, Transform, Vec2, v2};

/// Most copies one path or scatter arrangement places.
pub const MAX_PLACED: i64 = 200;

/// The guide (path) or container (scatter) object's index, taken out of the copied set.
pub(crate) fn guide(d: &Design, p: &Params, idx: &mut Vec<usize>) -> Result<usize, CommandError> {
    let key = if p.string("mode", None)? == "path" {
        "pathId"
    } else {
        "shapeId"
    };
    let id = p.int(key, None, 0, i64::MAX)? as u64;
    let g = d
        .index_of(id)
        .ok_or_else(|| p.not_found(format!("object {id}")))?;
    if d.objects[g].contours.is_empty() {
        return Err(p.invalid(key, "must be a shape or line"));
    }
    if key == "shapeId" && !d.objects[g].contours.iter().any(|c| c.closed) {
        return Err(p.invalid(key, "must be a closed shape"));
    }
    idx.retain(|&i| i != g);
    if idx.is_empty() {
        return Err(p.invalid("ids", "select the motif besides the guide"));
    }
    Ok(g)
}

/// The guide's polyline, flattened: its first contour.
fn polyline(c: &Contour) -> Vec<Vec2> {
    c.path()
}

/// Point at arc length `s` along `pts` and the tangent there, taken over ±1 mm of the line
/// so a flattened curve gives its true direction rather than one chord's (closed lines wrap).
fn at(pts: &[Vec2], s: f64, total: f64, closed: bool) -> (Vec2, Vec2) {
    let h = MM.min(total / 4.0);
    let pos = |x: f64| {
        let x = if closed {
            x.rem_euclid(total)
        } else {
            x.clamp(0.0, total)
        };
        point(pts, x).0
    };
    let d = pos(s + h) - pos(s - h);
    let len = d.length();
    let (p, chord) = point(pts, s);
    (p, if len > 0.0 { d * (1.0 / len) } else { chord })
}

/// Point and chord direction at arc length `s` along `pts`.
fn point(pts: &[Vec2], s: f64) -> (Vec2, Vec2) {
    let mut left = s;
    for w in pts.windows(2) {
        let d = w[1] - w[0];
        let len = d.length();
        if len <= 0.0 {
            continue;
        }
        if left <= len {
            return (w[0] + d * (left / len), d * (1.0 / len));
        }
        left -= len;
    }
    let n = pts.len();
    let d = pts[n - 1] - pts[n - 2];
    (pts[n - 1], d * (1.0 / d.length().max(1e-9)))
}

/// A mirror across the line through `c` along `u` (unit).
pub(crate) fn reflection(c: Vec2, u: Vec2) -> Transform {
    let (cs, sn) = (u.x * u.x - u.y * u.y, 2.0 * u.x * u.y);
    Transform {
        a: cs,
        b: sn,
        c: sn,
        d: -cs,
        tx: c.x - (cs * c.x + sn * c.y),
        ty: c.y - (sn * c.x - cs * c.y),
    }
}

/// `path` mode: one transform per copy, centres evenly spaced by arc length (closed guides
/// wrap evenly, open ones include both ends).
pub(crate) fn path(
    d: &Design,
    p: &Params,
    g: usize,
    b: Rect,
) -> Result<Vec<Transform>, CommandError> {
    let c = &d.objects[g].contours[0];
    let pts = polyline(c);
    let total: f64 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    if pts.len() < 2 || total <= 0.0 {
        return Err(p.invalid("pathId", "the line has no length"));
    }
    let n = p.int("count", Some(10), 1, MAX_PLACED)? as usize;
    let follow = match p.string("orient", Some("follow"))?.as_str() {
        "follow" => true,
        "upright" => false,
        _ => return Err(p.invalid("orient", "must be follow or upright")),
    };
    let side = p.string("side", Some("center"))?;
    if !matches!(side.as_str(), "center" | "above" | "below" | "alternate") {
        return Err(p.invalid("side", "must be center, above, below or alternate"));
    }
    let off = match p.get("offset") {
        Some(_) => p.req("offset", -1000.0, 1000.0)? * MM,
        None if side == "center" => 0.0,
        None => b.height() / 2.0,
    };
    let mirror = p.bool("mirrorAlternate", Some(false))?;
    let step = if c.closed {
        total / n as f64
    } else if n > 1 {
        total / (n - 1) as f64
    } else {
        0.0
    };
    let centre = b.center();
    Ok((0..n)
        .map(|k| {
            let (at_p, t) = at(&pts, step * k as f64, total, c.closed);
            // Above is to the left of the direction of travel on screen (y down).
            let above = v2(t.y, -t.x);
            let sign = match side.as_str() {
                "above" => 1.0,
                "below" => -1.0,
                "alternate" if k % 2 == 1 => -1.0,
                "alternate" => 1.0,
                _ => 0.0,
            };
            let mut tr = Transform::translation(-centre.x, -centre.y);
            if mirror && k % 2 == 1 {
                tr = tr.then(&Transform::scaling(1.0, -1.0, v2(0.0, 0.0)));
            }
            if follow {
                let deg = t.y.atan2(t.x).to_degrees();
                tr = tr.then(&Transform::rotation(deg, v2(0.0, 0.0)));
            }
            let to = at_p + above * (sign * off);
            tr.then(&Transform::translation(to.x, to.y))
        })
        .collect())
}

/// SplitMix64: a small deterministic generator, so a seed always gives the same layout.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn dist_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 <= 0.0 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// The shortest distance between two closed outlines (0 when they cross or one holds the
/// other).
pub(crate) fn outline_gap(a: &[Contour], b: &[Contour]) -> f64 {
    let pa: Vec<Vec2> = a.iter().flat_map(Contour::outline_points).collect();
    let pb: Vec<Vec2> = b.iter().flat_map(Contour::outline_points).collect();
    if pa.iter().any(|&p| contains(p, b)) || pb.iter().any(|&p| contains(p, a)) {
        return 0.0;
    }
    let ea: Vec<(Vec2, Vec2)> = a.iter().flat_map(Contour::edges).collect();
    let eb: Vec<(Vec2, Vec2)> = b.iter().flat_map(Contour::edges).collect();
    let one = |ps: &[Vec2], es: &[(Vec2, Vec2)]| {
        ps.iter()
            .flat_map(|&p| es.iter().map(move |&(x, y)| dist_to_segment(p, x, y)))
            .fold(f64::INFINITY, f64::min)
    };
    one(&pa, &eb).min(one(&pb, &ea))
}

/// `scatter` mode: up to `count` transforms placing the motif (outline `motif`, bounds `b`)
/// inside the container at random, scaled and turned, no two copies closer than `gap` and none
/// crossing the container's outline. Rejection sampling with a fixed seed.
pub(crate) fn scatter(
    d: &Design,
    p: &Params,
    g: usize,
    motif: &[Contour],
    b: Rect,
) -> Result<Vec<Transform>, CommandError> {
    let n = p.int("count", Some(20), 1, MAX_PLACED)? as usize;
    let lo = p.opt("sizeMin", 1.0, 0.05, 20.0)?;
    let hi = p.opt("sizeMax", lo, lo, 20.0)?;
    let gap = p.opt("gap", 1.0, 0.0, 1000.0)? * MM;
    let turn = p.bool("randomRotate", Some(false))?;
    let seed = p.int("seed", Some(1), 0, i64::MAX)? as u64;
    let container = &d.objects[g].contours;
    let Some(area) = Rect::enclosing(container.iter().flat_map(Contour::outline_points)) else {
        return Err(p.invalid("shapeId", "the shape is empty"));
    };
    let mut rng = Rng(seed);
    let centre = b.center();
    let mut placed: Vec<(Transform, Vec<Contour>)> = Vec::new();
    for _ in 0..n * 200 {
        if placed.len() == n {
            break;
        }
        let s = lo + (hi - lo) * rng.next();
        let deg = if turn { 360.0 * rng.next() } else { 0.0 };
        let at = v2(
            area.min_x + area.width() * rng.next(),
            area.min_y + area.height() * rng.next(),
        );
        let t = Transform::translation(-centre.x, -centre.y)
            .then(&Transform::scaling(s, s, v2(0.0, 0.0)))
            .then(&Transform::rotation(deg, v2(0.0, 0.0)))
            .then(&Transform::translation(at.x, at.y));
        let outline: Vec<Contour> = motif.iter().map(|c| c.transformed(&t)).collect();
        let inside = outline
            .iter()
            .flat_map(Contour::outline_points)
            .all(|q| contains(q, container))
            && !container
                .iter()
                .flat_map(Contour::outline_points)
                .any(|q| contains(q, &outline));
        if inside && placed.iter().all(|(_, o)| outline_gap(o, &outline) >= gap) {
            placed.push((t, outline));
        }
    }
    Ok(placed.into_iter().map(|(t, _)| t).collect())
}

/// The motif's outline for scatter's spacing checks: its closed contours, or the bounding box
/// of the selection when it has none (stitch patterns, open lines).
pub(crate) fn motif_outline(d: &Design, idx: &[usize], b: Rect) -> Vec<Contour> {
    let closed: Vec<Contour> = idx
        .iter()
        .flat_map(|&i| d.objects[i].contours.iter().filter(|c| c.closed).cloned())
        .collect();
    if closed.is_empty() {
        vec![Contour::rectangle(b.min_x, b.min_y, b.width(), b.height())]
    } else {
        closed
    }
}
