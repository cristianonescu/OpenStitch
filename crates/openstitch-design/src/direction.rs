//! Sewing-direction helpers: the curved direction ("round fill") and the region's reference
//! point. Both work by mapping the region into a space where the ordinary generators apply,
//! then mapping the stitches back. Lengths are 0.1 mm.
//!
//! Curved direction: in the frame of the curve's chord (x along the chord), the curve is the
//! graph of y = f(x). The warp (x, y) → (x, y − f(x)) straightens it, so rows that are
//! horizontal in warped space are copies of the curve shifted across the region; they never
//! cross each other.

use crate::fmath::Det;
use crate::generate::Run;
use crate::geometry::{Contour, Transform, Vec2, v2};
use crate::model::RegionType;

/// Region types that can follow a curved direction (PE-DESIGN: fill, programmable fill,
/// piping, motif fill).
pub const CURVE_TYPES: [RegionType; 4] = [
    RegionType::Fill,
    RegionType::ProgrammableFill,
    RegionType::Piping,
    RegionType::MotifFill,
];

/// Region types with a centre point. PE-DESIGN 11.50's Spiral Stitch has none: its region
/// attributes are Density, Run pitch and Start Point only (pe-refs-4 rounds 4–7), and the
/// spiral is the outline offset inwards a row per turn, so it ends where the offsets meet
/// (#354).
pub const CENTER_TYPES: [RegionType; 2] = [RegionType::ConcentricCircle, RegionType::Radial];

/// Region types with a pattern reference point.
pub const REFERENCE_TYPES: [RegionType; 6] = [
    RegionType::ProgrammableFill,
    RegionType::MotifFill,
    RegionType::CrossStitch,
    RegionType::NetFill,
    RegionType::ZigzagNetFill,
    RegionType::DecorativeFill,
];

/// The straightening map of a direction curve.
#[derive(Clone, Debug)]
pub struct Warp {
    /// Chord frame: model → frame.
    to_frame: Transform,
    from_frame: Transform,
    /// The curve in frame coordinates, x strictly increasing.
    graph: Vec<Vec2>,
}

/// Why a curve cannot be a sewing direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveError {
    TooShort,
    TurnsBack,
}

impl Warp {
    pub fn new(curve: &Contour) -> Result<Warp, CurveError> {
        let path = curve.path();
        let (Some(&a), Some(&b)) = (path.first(), path.last()) else {
            return Err(CurveError::TooShort);
        };
        let chord = b - a;
        if chord.length() < 1.0 {
            return Err(CurveError::TooShort);
        }
        let angle = chord.y.datan2(chord.x).to_degrees();
        let to_frame =
            Transform::translation(-a.x, -a.y).then(&Transform::rotation(-angle, v2(0.0, 0.0)));
        let from_frame =
            Transform::rotation(angle, v2(0.0, 0.0)).then(&Transform::translation(a.x, a.y));
        let graph: Vec<Vec2> = path.iter().map(|&p| to_frame.apply(p)).collect();
        if graph.windows(2).any(|w| w[1].x <= w[0].x) {
            return Err(CurveError::TurnsBack);
        }
        Ok(Warp {
            to_frame,
            from_frame,
            graph,
        })
    }

    /// Height of the curve above frame x (constant beyond its ends).
    fn f(&self, x: f64) -> f64 {
        let g = &self.graph;
        let k = g.partition_point(|p| p.x < x);
        if k == 0 {
            return g[0].y;
        }
        if k >= g.len() {
            return g[g.len() - 1].y;
        }
        let (p, q) = (g[k - 1], g[k]);
        p.y + (q.y - p.y) * (x - p.x) / (q.x - p.x)
    }

    pub fn forward(&self, p: Vec2) -> Vec2 {
        let q = self.to_frame.apply(p);
        v2(q.x, q.y - self.f(q.x))
    }

    pub fn back(&self, p: Vec2) -> Vec2 {
        self.from_frame.apply(v2(p.x, p.y + self.f(p.x)))
    }

    /// Closed contours mapped into warped space. Every edge is split where it crosses a vertex
    /// of the curve, so the warped outline is exact (f is linear between those crossings).
    pub fn forward_contours(&self, contours: &[Contour]) -> Vec<Contour> {
        contours
            .iter()
            .map(|c| {
                let path: Vec<Vec2> = c.path().iter().map(|&p| self.to_frame.apply(p)).collect();
                let mut pts: Vec<Vec2> = Vec::new();
                for w in path.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    pts.push(a);
                    let (lo, hi) = (a.x.min(b.x), a.x.max(b.x));
                    let mut cuts: Vec<f64> = self
                        .graph
                        .iter()
                        .map(|g| g.x)
                        .filter(|&x| x > lo && x < hi)
                        .map(|x| (x - a.x) / (b.x - a.x))
                        .collect();
                    cuts.sort_by(f64::total_cmp);
                    pts.extend(cuts.into_iter().map(|t| a.lerp(b, t)));
                }
                if let Some(&last) = path.last() {
                    pts.push(last);
                }
                if c.closed && pts.len() > 1 {
                    pts.pop();
                }
                let warped = pts
                    .into_iter()
                    .map(|q| v2(q.x, q.y - self.f(q.x)))
                    .collect();
                Contour::new(warped, c.closed)
            })
            .collect()
    }

    /// Runs mapped back to model space. Needle points along rows are at most a stitch apart
    /// already, so they follow the curve; longer connections stay straight stitches.
    pub fn back_runs(&self, runs: Vec<Run>) -> Vec<Run> {
        runs.into_iter()
            .map(|r| r.into_iter().map(|p| self.back(p)).collect())
            .collect()
    }
}

/// Spacing of the samples along a guide (0.1 mm) from which [`guided_rows`] offsets its rows.
const GUIDE_STEP: f64 = 5.0;
/// Largest turn of the guide's normal between samples (radians, about 6°).
const GUIDE_FAN: f64 = 0.1;

/// The guide `curve` as dense samples (about [`GUIDE_STEP`] apart) with their left normals,
/// extended `reach` straight on past both ends in the end segments' directions.
fn guide_samples(curve: &Contour, reach: f64) -> Option<Vec<(Vec2, Vec2)>> {
    let mut path: Vec<Vec2> = curve.path();
    path.dedup_by(|a, b| a.distance(*b) < 1e-6);
    if path.len() < 2 {
        return None;
    }
    let unit = |d: Vec2| {
        let l = d.length();
        (l > 1e-9).then(|| d * (1.0 / l))
    };
    let (t0, t1) = (
        unit(path[1] - path[0])?,
        unit(path[path.len() - 1] - path[path.len() - 2])?,
    );
    let mut ext = vec![path[0] - t0 * reach];
    ext.extend(path.iter().copied());
    ext.push(path[path.len() - 1] + t1 * reach);
    let mut dense = crate::generate::subdivide(&ext, GUIDE_STEP);
    dense.dedup_by(|a, b| a.distance(*b) < 1e-6);
    let n = dense.len();
    let tangents: Vec<Vec2> = dense
        .windows(2)
        .map(|w| unit(w[1] - w[0]).unwrap_or(t0))
        .collect();
    let normal = |t: Vec2| v2(t.y, -t.x);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let before = tangents[i.saturating_sub(1)];
        let after = tangents[i.min(n - 2)];
        let turn = (before.x * after.y - before.y * after.x)
            .datan2(before.x * after.x + before.y * after.y);
        if turn.abs() <= GUIDE_FAN {
            let t = unit(before + after).unwrap_or(after);
            out.push((dense[i], normal(t)));
            continue;
        }
        // A sharp vertex: its normals fan round from one side's to the other's, so the rows on
        // the outside of the bend are round arcs, not chords cutting inside them.
        let steps = (turn.abs() / GUIDE_FAN).ceil() as usize;
        let a0 = before.y.datan2(before.x);
        for k in 0..=steps {
            let a = a0 + turn * k as f64 / steps as f64;
            let (sin, cos) = a.dsin_cos();
            out.push((dense[i], normal(v2(cos, sin))));
        }
    }
    Some(out)
}

/// Signed distance from `p` to the polyline of `samples` (positive on the normals' side).
fn signed_distance(samples: &[(Vec2, Vec2)], p: Vec2) -> f64 {
    let mut best = (f64::INFINITY, 0.0);
    for w in samples.windows(2) {
        let (a, b) = (w[0].0, w[1].0);
        let ab = b - a;
        let len2 = ab.x * ab.x + ab.y * ab.y;
        let t = if len2 > 0.0 {
            (((p - a).x * ab.x + (p - a).y * ab.y) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = a + ab * t;
        let d = p.distance(q);
        if d < best.0 {
            let side = ab.y * (p - a).x - ab.x * (p - a).y;
            best = (d, if side >= 0.0 { d } else { -d });
        }
    }
    best.1
}

/// Guided fill (#471): rows that are the guide `curve` offset by whole numbers of `spacing`,
/// clipped to the region (`closed` minus `holes`), in sewing order, each split into stitches of
/// `stitch` with the needle grid moved `shift` of a stitch from row to row. Row-to-row distance
/// is the spacing everywhere along the guide, and rows never cross: each row is the set of
/// points at one distance from the guide (the guide carried straight on past its ends, so the
/// rows go on straight there). On the convex side of a bend the rows round off; on the
/// concave side they close into a corner, the part nearer the guide than the row's distance
/// being left out. `None` when the curve is too short to guide.
pub fn guided_rows(
    closed: &[Contour],
    holes: &[Contour],
    curve: &Contour,
    spacing: f64,
    stitch: f64,
    shift: f64,
) -> Option<Vec<Run>> {
    if !(spacing > 0.0 && stitch > 0.0) {
        return None;
    }
    let (lines, _) = guided_lines(closed, holes, curve, spacing)?;
    let rows: Vec<Vec<Vec<Vec2>>> = lines
        .into_iter()
        .enumerate()
        .map(|(row, (_, pieces))| {
            pieces
                .into_iter()
                .map(|piece| {
                    needle_points(
                        &piece,
                        stitch,
                        (row as f64 * shift).rem_euclid(1.0),
                        spacing / 3.0,
                    )
                })
                .filter(|r| r.len() >= 2)
                .collect()
        })
        .collect();
    let chord = curve.path();
    let (a, z) = (chord[0], chord[chord.len() - 1]);
    let angle = (z.y - a.y).datan2(z.x - a.x).to_degrees();
    Some(crate::regions::order_rows(rows, angle))
}

/// A guide carried straight on past its ends, as dense samples with their normals, and each
/// sample's arc length along it: the frame (along, across) the guided rows and tiles use.
pub struct Guide {
    samples: Vec<(Vec2, Vec2)>,
    along: Vec<f64>,
}

impl Guide {
    /// `p` in the guide's frame: (arc length along the guide to `p`'s nearest point, signed
    /// distance from the guide, positive on the normals' side).
    pub fn frame(&self, p: Vec2) -> (f64, f64) {
        let mut best = (f64::INFINITY, 0.0, 0.0);
        for (i, w) in self.samples.windows(2).enumerate() {
            let (a, b) = (w[0].0, w[1].0);
            let ab = b - a;
            let len2 = ab.x * ab.x + ab.y * ab.y;
            let t = if len2 > 0.0 {
                (((p - a).x * ab.x + (p - a).y * ab.y) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = a + ab * t;
            let d = p.distance(q);
            if d < best.0 {
                let side = ab.y * (p - a).x - ab.x * (p - a).y;
                let s = self.along[i] + (self.along[i + 1] - self.along[i]) * t;
                best = (d, s, if side >= 0.0 { d } else { -d });
            }
        }
        (best.1, best.2)
    }
}

/// The rows of [`guided_rows`] before their needle points: for each row (the guide offset by a
/// whole number of `spacing`), its distance from the guide and its pieces inside the region,
/// as polylines; and the guide. `None` when the curve is too short to guide.
#[allow(clippy::type_complexity)]
pub fn guided_lines(
    closed: &[Contour],
    holes: &[Contour],
    curve: &Contour,
    spacing: f64,
) -> Option<(Vec<(f64, Vec<Vec<Vec2>>)>, Guide)> {
    if spacing <= 0.0 {
        return None;
    }
    let pts: Vec<Vec2> = closed.iter().flat_map(Contour::outline_points).collect();
    let b = crate::geometry::Rect::enclosing(pts.iter().copied())?;
    let reach = b.width().dhypot(b.height())
        + curve
            .path()
            .iter()
            .map(|&p| p.distance(b.center()))
            .fold(0.0, f64::max);
    let guide = guide_samples(curve, reach)?;
    let (lo, hi) = pts
        .iter()
        .map(|&p| signed_distance(&guide, p))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), d| {
            (l.min(d), h.max(d))
        });
    let (k0, k1) = ((lo / spacing).ceil() as i64, (hi / spacing).floor() as i64);
    if k1 < k0 || k1 - k0 > 20_000 {
        return None;
    }
    let tol = 0.05 * spacing + 1e-6;
    let mut rows: Vec<(f64, Vec<Vec<Vec2>>)> = Vec::new();
    for k in k0..=k1 {
        let d = k as f64 * spacing;
        // The guide's samples carried d along their normals; where that lands nearer the guide
        // than d (inside a bend), the row cuts straight across to where it comes out again.
        let raw: Vec<Vec2> = guide.iter().map(|&(g, n)| g + n * d).collect();
        let keep: Vec<bool> = raw
            .iter()
            .map(|&q| d == 0.0 || signed_distance(&guide, q).abs() >= d.abs() - tol)
            .collect();
        let line = untangled(without_loops(&raw, &keep));
        rows.push((d, crate::paths::clip(&line, closed, holes)));
    }
    let mut along = vec![0.0];
    for w in guide.windows(2) {
        along.push(along[along.len() - 1] + w[0].0.distance(w[1].0));
    }
    Some((
        rows,
        Guide {
            samples: guide,
            along,
        },
    ))
}

/// Samples on each side of a stretch nearer the guide than its row in which [`without_loops`]
/// looks for the loop's crossing.
const LOOP_MARGIN: usize = 40;

/// The offset `raw` with the loops it makes inside bends cut out: each stretch whose samples
/// are nearer the guide than the row (`keep` false) lies on a loop; the loop runs from the
/// first segment that crosses a later one to that later one, and is replaced by the crossing,
/// the row's corner. A stretch with no crossing found is left out with its ends joined.
fn without_loops(raw: &[Vec2], keep: &[bool]) -> Vec<Vec2> {
    let n = raw.len();
    let margin = LOOP_MARGIN;
    // Stretches of left-out samples; those a few samples apart are one loop (its arms hover
    // within the tolerance of the row near the crossing).
    let mut stretches: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < n {
        if keep[i] {
            i += 1;
            continue;
        }
        let first = i;
        while i < n && !keep[i] {
            i += 1;
        }
        match stretches.last_mut() {
            Some(last) if first - last.1 <= margin => last.1 = i,
            _ => stretches.push((first, i)),
        }
    }
    let mut cuts: Vec<(usize, usize, Vec2)> = Vec::new();
    for (first, i) in stretches {
        let (lo, hi) = (first.saturating_sub(margin), (i + margin).min(n - 1));
        // The widest crossing: a segment near the stretch's start with one near its end (the
        // cusps inside the loop cross themselves too, in small loops of their own).
        let mut found: Option<(usize, usize, Vec2)> = None;
        for a in lo..(first + margin).min(hi) {
            for b in ((a + 2).max(i.saturating_sub(margin))..hi).rev() {
                if found.is_some_and(|(fa, fb, _)| b - a <= fb - fa) {
                    break;
                }
                if let Some(c) = crossing(raw[a], raw[a + 1], raw[b], raw[b + 1]) {
                    found = Some((a, b, c));
                    break;
                }
            }
        }
        cuts.push(found.unwrap_or((first.saturating_sub(1), i, raw[first.saturating_sub(1)])));
    }
    let mut out = Vec::with_capacity(n);
    let mut k = 0;
    for (a, b, c) in cuts {
        if a + 1 < k {
            continue;
        }
        out.extend_from_slice(&raw[k..=a]);
        out.push(c);
        k = b + 1;
    }
    if k < n {
        out.extend_from_slice(&raw[k..]);
    }
    out.dedup_by(|a, b| a.distance(*b) < 1e-9);
    out
}

/// Samples within which [`untangled`] looks for a row crossing itself.
const UNTANGLE_WINDOW: usize = 240;

/// `line` with every loop it still makes cut out at its crossing (a row, a set of points at one
/// distance from the guide, never crosses itself; what does is left over from sampling the
/// cusps inside a bend): from the start, each segment that crosses a later one within
/// [`UNTANGLE_WINDOW`] samples is joined to the farthest such one at the crossing.
fn untangled(mut line: Vec<Vec2>) -> Vec<Vec2> {
    for _ in 0..8 {
        let n = line.len();
        let mut out = Vec::with_capacity(n);
        let mut changed = false;
        let mut a = 0;
        while a + 1 < n {
            out.push(line[a]);
            let far = (a + UNTANGLE_WINDOW).min(n - 1);
            let hit = (a + 2..far)
                .rev()
                .find_map(|b| crossing(line[a], line[a + 1], line[b], line[b + 1]).map(|c| (b, c)));
            if let Some((b, c)) = hit {
                out.push(c);
                a = b + 1;
                changed = true;
            } else {
                a += 1;
            }
        }
        if a < n {
            out.push(line[n - 1]);
        }
        out.dedup_by(|p, q| p.distance(*q) < 1e-9);
        line = out;
        if !changed {
            break;
        }
    }
    line
}

/// Where segment a-b crosses segment c-d.
fn crossing(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<Vec2> {
    let (r, s) = (b - a, d - c);
    let den = r.x * s.y - r.y * s.x;
    if den.abs() < 1e-12 {
        return None;
    }
    let q = c - a;
    let t = (q.x * s.y - q.y * s.x) / den;
    let u = (q.x * r.y - q.y * r.x) / den;
    // Touching at an end is not crossing (consecutive and repeated points).
    const E: f64 = 1e-9;
    (t > E && t < 1.0 - E && u > E && u < 1.0 - E).then(|| a + r * t)
}

/// A row turning by more than this (radians, 15°) at a vertex keeps that vertex as a needle
/// point, so a stitch never cuts across a corner into the next row.
const CORNER_TURN: f64 = 0.26;

/// Needle points along `line`: its ends and corners, and every `l` of its length from `phase`
/// of a stitch on, leaving out points within a quarter stitch of an end or corner. Where the
/// row bends so much that a stitch would stray more than `sag` from it (and so cross the next
/// row), the stitch is halved until it does not.
fn needle_points(line: &[Vec2], l: f64, phase: f64, sag: f64) -> Run {
    let n = line.len();
    let mut fixed: Vec<f64> = vec![0.0];
    let mut at = 0.0;
    let mut cum = vec![0.0];
    for i in 1..n {
        at += line[i - 1].distance(line[i]);
        cum.push(at);
        if i + 1 < n {
            let (u, v) = (line[i] - line[i - 1], line[i + 1] - line[i]);
            let turn = (u.x * v.y - u.y * v.x).datan2(u.x * v.x + u.y * v.y).abs();
            if turn > CORNER_TURN {
                fixed.push(at);
            }
        }
    }
    let total = at;
    fixed.push(total);
    let mut marks: Vec<f64> = fixed.clone();
    let mut next = phase * l;
    while next < total {
        if fixed.iter().all(|&f| (next - f).abs() > 0.25 * l) {
            marks.push(next);
        }
        next += l;
    }
    marks.sort_by(f64::total_cmp);
    marks.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let at = |m: f64| {
        let k = cum.partition_point(|&c| c < m).clamp(1, n - 1);
        let seg = cum[k] - cum[k - 1];
        let t = if seg > 0.0 {
            ((m - cum[k - 1]) / seg).clamp(0.0, 1.0)
        } else {
            1.0
        };
        line[k - 1].lerp(line[k], t)
    };
    // How far the row strays from the stitch between lengths a and b.
    let stray = |a: f64, b: f64| {
        let (p, q) = (at(a), at(b));
        let d = q - p;
        let len = d.length();
        (0..n)
            .filter(|&i| cum[i] > a && cum[i] < b)
            .map(|i| {
                let v = line[i] - p;
                if len > 0.0 {
                    (v.x * d.y - v.y * d.x).abs() / len
                } else {
                    v.length()
                }
            })
            .fold(0.0, f64::max)
    };
    let mut fine = vec![marks[0]];
    for w in marks.windows(2) {
        let mut stack = vec![(w[0], w[1])];
        while let Some((a, b)) = stack.pop() {
            if b - a > 1.0 && stray(a, b) > sag {
                let m = (a + b) / 2.0;
                stack.push((m, b));
                stack.push((a, m));
            } else {
                fine.push(b);
            }
        }
    }
    fine.into_iter().map(at).collect()
}

/// `contours` moved by `d`.
pub fn shifted(contours: &[Contour], d: Vec2) -> Vec<Contour> {
    let t = Transform::translation(d.x, d.y);
    contours.iter().map(|c| c.transformed(&t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Bezier;

    fn arch() -> Contour {
        let mut c = Contour::new(vec![v2(-100.0, 0.0), v2(300.0, 0.0)], false);
        c.set_curve(
            0,
            Some(Bezier {
                c1: v2(0.0, -150.0),
                c2: v2(200.0, -150.0),
            }),
        );
        c
    }

    #[test]
    fn warp_round_trips_and_straightens_the_curve() {
        let w = Warp::new(&arch()).expect("warp");
        for p in [
            v2(0.0, 0.0),
            v2(100.0, -50.0),
            v2(250.0, 80.0),
            v2(-200.0, 5.0),
        ] {
            assert!(w.back(w.forward(p)).distance(p) < 1e-9, "{p:?}");
        }
        for p in arch().path() {
            assert!(w.forward(p).y.abs() < 1e-9);
        }
    }

    #[test]
    fn guided_rows_are_offsets_that_go_on_straight_past_the_guide() {
        // #471: a 10 mm arch in the middle of a 60 x 30 mm rectangle. Every needle point lies
        // a whole number of 1 mm row spacings from the guide carried straight on past its
        // ends; beyond the ends (here falling at 45 degrees) the rows are straight lines
        // parallel to the end segments.
        let guide = Contour::new(
            vec![v2(250.0, 150.0), v2(300.0, 100.0), v2(350.0, 150.0)],
            false,
        );
        let rect = Contour::rectangle(0.0, 0.0, 600.0, 300.0);
        let rows =
            guided_rows(std::slice::from_ref(&rect), &[], &guide, 10.0, 40.0, 0.3).expect("rows");
        let samples = guide_samples(&guide, 2000.0).expect("samples");
        let mut n = 0;
        for run in &rows {
            for &p in run {
                let d = signed_distance(&samples, p) / 10.0;
                assert!((d - d.round()).abs() < 0.02, "{p:?} at {d} spacings");
                n += 1;
            }
            // Left of the guide's start the row is straight, along the start direction (45°
            // down to the left): no needle point strays from the line through its ends.
            let left: Vec<Vec2> = run.iter().copied().filter(|p| p.x < 200.0).collect();
            if left.len() >= 3 {
                let (a, b) = (left[0], left[left.len() - 1]);
                for &p in &left {
                    let v = p - a;
                    let off = (v.x * (b - a).y - v.y * (b - a).x).abs() / a.distance(b);
                    assert!(off < 0.5, "{p:?} off the straight row by {off}");
                }
                let dir = b - a;
                assert!(
                    (dir.x.abs() - dir.y.abs()).abs() < 0.02 * dir.length(),
                    "{dir:?}"
                );
            }
        }
        assert!(n > 200, "{n} needle points");
        // The region is covered: rows a spacing apart reach every part of it.
        assert!(rows.len() > 25, "{} rows", rows.len());
    }

    #[test]
    fn rejected_curves() {
        let short = Contour::new(vec![v2(0.0, 0.0), v2(0.5, 0.0)], false);
        assert_eq!(Warp::new(&short).err(), Some(CurveError::TooShort));
        let back = Contour::new(vec![v2(0.0, 0.0), v2(100.0, 0.0), v2(50.0, 10.0)], false);
        assert_eq!(Warp::new(&back).err(), Some(CurveError::TurnsBack));
        // Any chord direction works.
        let diagonal = Contour::new(vec![v2(0.0, 0.0), v2(10.0, 30.0), v2(0.0, 100.0)], false);
        assert!(Warp::new(&diagonal).is_ok());
    }

    use crate::model::RegionSewing;
    use openstitch_core::Thread;

    fn layers(c: &Contour, r: &RegionSewing) -> Vec<Vec2> {
        crate::region::layers(std::slice::from_ref(c), r, &[])
            .into_iter()
            .flatten()
            .flatten()
            .collect()
    }

    fn sewing(kind: RegionType) -> RegionSewing {
        RegionSewing::new(kind, Thread::rgb(0xff0000))
    }

    #[test]
    fn concentric_rings_are_centred_on_the_centre_point() {
        let square = Contour::rectangle(0.0, 0.0, 400.0, 400.0);
        let mut r = sewing(RegionType::ConcentricCircle);
        let c = v2(120.0, 260.0);
        r.center = Some(c);
        let spacing = 10.0 / r.density;
        assert!(layers(&square, &r).len() > 100);
        // The whole circles (the first run; PE-DESIGN sews the cut ones after a trim) have
        // radii that are multiples of the spacing, needle points on the 0.1 mm grid: distances
        // to the centre fall into narrow bands.
        let runs = crate::region::layers(std::slice::from_ref(&square), &r, &[])
            .pop()
            .expect("top");
        let points = &runs[0];
        let off = points
            .iter()
            .filter(|p| {
                let k = p.distance(c) / spacing;
                (k - k.round()).abs() * spacing > 1.0
            })
            .count();
        assert!(off * 10 < points.len(), "{off} of {}", points.len());
    }

    #[test]
    fn radial_lines_meet_at_the_centre_point() {
        let square = Contour::rectangle(0.0, 0.0, 400.0, 400.0);
        let mut r = sewing(RegionType::Radial);
        let c = v2(100.0, 100.0);
        r.center = Some(c);
        let runs = crate::region::layers(std::slice::from_ref(&square), &r, &[])
            .pop()
            .expect("top");
        // Every straight piece of a run points at the centre.
        let mut checked = 0;
        for run in &runs {
            for w in run.windows(2) {
                let (a, b) = (w[0] - c, w[1] - c);
                let cross = a.x * b.y - a.y * b.x;
                if a.length() > 20.0
                    && b.length() > 20.0
                    && w[0].distance(w[1]) > 5.0
                    && (cross / (a.length() * b.length())).abs() < 0.02
                {
                    checked += 1;
                }
            }
        }
        assert!(checked > 200, "{checked}");
    }

    #[test]
    fn reference_point_moves_the_pattern_grid() {
        let square = Contour::rectangle(3.3, 7.1, 300.7, 290.3);
        let mut r0 = sewing(RegionType::ProgrammableFill);
        // The tile grid lines up with the region's axes at 0°. Without a reference point it
        // starts at the region's top-left corner (3.3, 7.1), as in PE-DESIGN.
        r0.angle = 0.0;
        r0.pattern = Some("diamonds".into());
        let (pw, ph) = r0.pattern_size_mm();
        let corner = v2(3.3, 7.1);
        let mut whole = r0.clone();
        whole.reference = Some(corner + v2(pw * 20.0, ph * 20.0));
        let mut half = r0.clone();
        half.reference = Some(corner + v2(pw * 5.0, 0.0));
        let base = layers(&square, &r0);
        let same = layers(&square, &whole);
        let moved = layers(&square, &half);
        // Share of `a`'s needle points that `b` also has.
        let shared = |a: &[Vec2], b: &[Vec2]| {
            a.iter()
                .filter(|p| b.iter().any(|q| q.distance(**p) < 1e-3))
                .count() as f64
                / a.len() as f64
        };
        // Whole tiles: the same pattern (up to a point flipping at the outline).
        assert!(shared(&base, &same) > 0.99, "{}", shared(&base, &same));
        // Half a tile: a different pattern.
        assert!(shared(&base, &moved) < 0.8, "{}", shared(&base, &moved));
    }

    #[test]
    fn curved_direction_rows_follow_the_curve() {
        // A band along the arch: straightened, it is a rectangle, so every needle point is
        // inside it (a straight top edge would leave notches the rows jump across).
        let arc = arch().path();
        let mut band: Vec<Vec2> = arc.iter().map(|p| v2(p.x, p.y - 60.0)).collect();
        band.extend(arc.iter().rev().map(|p| v2(p.x, p.y + 60.0)));
        let rect = Contour::new(band, true);
        let mut r = sewing(RegionType::Fill);
        r.direction_curve = Some(arch());
        // The top rows only: under sewing runs across them.
        r.underlay = false;
        let spacing = 10.0 / r.density;
        let points = layers(&rect, &r);
        assert!(points.len() > 200);
        // Every needle point lies on a row: a whole number of spacings from the curve (its
        // offsets since #471; before, its vertical copies).
        let guide = guide_samples(&arch(), 2000.0).expect("samples");
        let phase = |p: &Vec2| (signed_distance(&guide, *p) / spacing).rem_euclid(1.0);
        // Rows lie whole spacings from the curve, so the phase is 0; travel between rows
        // inside the band is the rest.
        let on = points
            .iter()
            .filter(|p| {
                let f = phase(p);
                f.min(1.0 - f) < 0.02
            })
            .count();
        assert!(
            on as f64 > 0.85 * points.len() as f64,
            "{on} of {} points on the curved rows",
            points.len()
        );
        // All inside the band (pull compensation 0), to 0.03 mm.
        let w = Warp::new(&arch()).expect("warp");
        assert!(points.iter().all(|p| {
            let q = w.forward(*p);
            q.y.abs() <= 60.3 && q.x >= -0.3 && q.x <= 400.3
        }));
        // Different from the straight fill.
        let mut straight = r.clone();
        straight.direction_curve = None;
        assert_ne!(layers(&rect, &straight), points);
        // Types without curved direction ignore the curve.
        let mut satin = sewing(RegionType::Satin);
        satin.direction_curve = Some(arch());
        let mut plain = satin.clone();
        plain.direction_curve = None;
        assert_eq!(layers(&rect, &satin), layers(&rect, &plain));
    }
}
