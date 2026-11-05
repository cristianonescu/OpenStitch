//! Smooth Shape (#478): fewer nodes on an outline, each kept within a tolerance of the
//! original. Runs of straight segments keep the vertices Douglas–Peucker keeps; runs of curves
//! between corners are sampled and refitted with as few cubic Béziers as stay within the
//! tolerance (least squares with Newton reparametrisation, split at the worst point), and the
//! refit is used only when it has fewer segments than the run had. Corners and the ends of an
//! open contour stay. Manual Punch columns lose the one direction line whose removal moves the
//! sides least; satin skeletons keep the centre-line points Douglas–Peucker keeps. Own
//! algorithm. Lengths are 0.1 mm.

use crate::column;
use crate::fmath::Det;
use crate::geometry::{Bezier, Contour, FLATNESS, Vec2, v2};
use crate::skeleton::Skeleton;

/// Default tolerance, mm.
pub const DEFAULT_TOLERANCE_MM: f64 = 0.2;
/// Tangent turns sharper than this (degrees) at a vertex between curves are corners.
const CORNER_DEGREES: f64 = 30.0;
/// Most samples one curved run is refitted from.
const MAX_SAMPLES: usize = 4_000;

/// Distance from `p` to segment a–b.
pub fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let d = b - a;
    let len2 = d.x * d.x + d.y * d.y;
    if len2 == 0.0 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

/// Flags of the vertices Douglas–Peucker keeps on the open run `pts` (both ends kept).
fn douglas_peucker(pts: &[Vec2], tolerance: f64) -> Vec<bool> {
    let n = pts.len();
    let mut kept = vec![false; n];
    if n == 0 {
        return kept;
    }
    kept[0] = true;
    kept[n - 1] = true;
    let mut stack = vec![(0, n - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (mut worst, mut at) = (0.0, a);
        for (i, &p) in pts.iter().enumerate().take(b).skip(a + 1) {
            let d = segment_distance(p, pts[a], pts[b]);
            if d > worst {
                (worst, at) = (d, i);
            }
        }
        if worst > tolerance {
            kept[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
    kept
}

fn unit(d: Vec2) -> Option<Vec2> {
    let l = d.length();
    (l > 1e-9).then(|| d * (1.0 / l))
}

fn dot(a: Vec2, b: Vec2) -> f64 {
    a.x * b.x + a.y * b.y
}

/// Direction the contour leaves vertex `i` along segment `i`.
fn out_tangent(c: &Contour, seg: usize) -> Option<Vec2> {
    let (a, b) = c.segment_ends(seg);
    match c.curve(seg) {
        Some(z) => unit(z.c1 - a)
            .or_else(|| unit(z.c2 - a))
            .or_else(|| unit(b - a)),
        None => unit(b - a),
    }
}

/// Direction the contour arrives at the end of segment `seg`.
fn in_tangent(c: &Contour, seg: usize) -> Option<Vec2> {
    let (a, b) = c.segment_ends(seg);
    match c.curve(seg) {
        Some(z) => unit(b - z.c2)
            .or_else(|| unit(b - z.c1))
            .or_else(|| unit(b - a)),
        None => unit(b - a),
    }
}

/// Whether the vertex between segments `before` and `after` must stay: where a straight run
/// meets a curved one, or a corner between curves.
fn is_break(c: &Contour, before: usize, after: usize) -> bool {
    let (cb, ca) = (c.curve(before).is_some(), c.curve(after).is_some());
    if cb != ca {
        return true;
    }
    if !cb {
        return false;
    }
    match (in_tangent(c, before), out_tangent(c, after)) {
        (Some(i), Some(o)) => dot(i, o) < CORNER_DEGREES.to_radians().dcos(),
        _ => true,
    }
}

/// One run of segments between kept vertices, written back as vertices (without the run's
/// last vertex) and the curves of their outgoing segments.
type Piece = (Vec<Vec2>, Vec<Option<Bezier>>);

/// The run of segments `segs` (consecutive, in order) of `c`, simplified.
fn simplify_run(c: &Contour, segs: &[usize], tolerance: f64) -> Piece {
    let original: Piece = (
        segs.iter().map(|&s| c.segment_ends(s).0).collect(),
        segs.iter().map(|&s| c.curve(s)).collect(),
    );
    if segs.len() < 2 {
        return original;
    }
    if c.curve(segs[0]).is_none() {
        // Straight: Douglas–Peucker on the vertices.
        let mut pts: Vec<Vec2> = segs.iter().map(|&s| c.segment_ends(s).0).collect();
        pts.push(c.segment_ends(*segs.last().expect("2 or more")).1);
        let kept = douglas_peucker(&pts, tolerance);
        let mut v: Vec<Vec2> = pts
            .iter()
            .zip(&kept)
            .filter(|(_, k)| **k)
            .map(|(p, _)| *p)
            .collect();
        v.pop();
        let n = v.len();
        return (v, vec![None; n]);
    }
    // Curved: refit from samples, ends and end tangents kept.
    let mut samples: Vec<Vec2> = vec![c.segment_ends(segs[0]).0];
    for &s in segs {
        samples.extend(c.segment_polyline(s).into_iter().skip(1));
    }
    let samples = resample(&samples, tolerance);
    let (Some(t1), Some(t2)) = (
        out_tangent(c, segs[0]),
        in_tangent(c, *segs.last().expect("2 or more")).map(|t| t * -1.0),
    ) else {
        return original;
    };
    // The curves stay within the tolerance less the flattening error of the samples.
    let budget = (tolerance - FLATNESS).max(tolerance * 0.5);
    let mut fitted = Vec::new();
    fit_cubic(&samples, t1, t2, budget, 0, &mut fitted);
    if fitted.len() >= segs.len() {
        return original;
    }
    (
        fitted.iter().map(|b| b[0]).collect(),
        fitted
            .iter()
            .map(|b| Some(Bezier { c1: b[1], c2: b[2] }))
            .collect(),
    )
}

/// Points along the polyline `pts` about `tolerance / 2` apart (ends kept), at most
/// [`MAX_SAMPLES`].
fn resample(pts: &[Vec2], tolerance: f64) -> Vec<Vec2> {
    let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    let step = (tolerance / 2.0).max(total / MAX_SAMPLES as f64).max(0.05);
    let count = ((total / step).ceil() as usize).max(1);
    let walker = crate::paths::Walker::new(pts);
    (0..=count)
        .map(|k| walker.at(total * k as f64 / count as f64))
        .collect()
}

fn bezier(b: &[Vec2; 4], t: f64) -> Vec2 {
    let s = 1.0 - t;
    b[0] * (s * s * s) + b[1] * (3.0 * s * s * t) + b[2] * (3.0 * s * t * t) + b[3] * (t * t * t)
}

/// Least-squares cubic through `pts` with the end tangents `t1` (leaving the start) and `t2`
/// (pointing back from the end), at parameters `u`.
fn generate(pts: &[Vec2], u: &[f64], t1: Vec2, t2: Vec2) -> [Vec2; 4] {
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (p, &t) in pts.iter().zip(u) {
        let s = 1.0 - t;
        let (b0, b1, b2, b3) = (s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t);
        let (a1, a2) = (t1 * b1, t2 * b2);
        c00 += dot(a1, a1);
        c01 += dot(a1, a2);
        c11 += dot(a2, a2);
        let tmp = *p - (first * (b0 + b1) + last * (b2 + b3));
        x0 += dot(a1, tmp);
        x1 += dot(a2, tmp);
    }
    let det = c00 * c11 - c01 * c01;
    let chord = first.distance(last);
    let (mut l1, mut l2) = if det.abs() > 1e-12 {
        ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det)
    } else {
        (0.0, 0.0)
    };
    // Wildly wrong handles fall back to a third of the chord (Wu/Barsky heuristic).
    if l1 < chord * 1e-6 || l2 < chord * 1e-6 || l1 > chord * 4.0 || l2 > chord * 4.0 {
        l1 = chord / 3.0;
        l2 = chord / 3.0;
    }
    [first, first + t1 * l1, last + t2 * l2, last]
}

/// Largest distance from `pts` to the curve at their parameters, and where.
fn max_error(pts: &[Vec2], b: &[Vec2; 4], u: &[f64]) -> (f64, usize) {
    let mut worst = (0.0, pts.len() / 2);
    for (i, (p, &t)) in pts.iter().zip(u).enumerate() {
        let d = p.distance(bezier(b, t));
        if d > worst.0 {
            worst = (d, i);
        }
    }
    worst
}

/// One Newton step towards each point's nearest parameter.
fn reparameterise(pts: &[Vec2], b: &[Vec2; 4], u: &[f64]) -> Vec<f64> {
    let d1 = [
        (b[1] - b[0]) * 3.0,
        (b[2] - b[1]) * 3.0,
        (b[3] - b[2]) * 3.0,
    ];
    let d2 = [(d1[1] - d1[0]) * 2.0, (d1[2] - d1[1]) * 2.0];
    pts.iter()
        .zip(u)
        .map(|(p, &t)| {
            let s = 1.0 - t;
            let q = bezier(b, t);
            let q1 = d1[0] * (s * s) + d1[1] * (2.0 * s * t) + d1[2] * (t * t);
            let q2 = d2[0] * s + d2[1] * t;
            let diff = q - *p;
            let num = dot(diff, q1);
            let den = dot(q1, q1) + dot(diff, q2);
            if den.abs() < 1e-12 {
                t
            } else {
                (t - num / den).clamp(0.0, 1.0)
            }
        })
        .collect()
}

/// Cubic Béziers through `pts` within `error`, appended to `out` (Schneider's scheme).
fn fit_cubic(pts: &[Vec2], t1: Vec2, t2: Vec2, error: f64, depth: u32, out: &mut Vec<[Vec2; 4]>) {
    let n = pts.len();
    let (first, last) = (pts[0], pts[n - 1]);
    if n <= 2 {
        let d = first.distance(last) / 3.0;
        out.push([first, first + t1 * d, last + t2 * d, last]);
        return;
    }
    let mut u = chord_parameters(pts);
    let mut b = generate(pts, &u, t1, t2);
    let (mut err, mut split) = max_error(pts, &b, &u);
    if err <= error {
        out.push(b);
        return;
    }
    if err <= error * 4.0 {
        for _ in 0..8 {
            u = reparameterise(pts, &b, &u);
            b = generate(pts, &u, t1, t2);
            (err, split) = max_error(pts, &b, &u);
            if err <= error {
                out.push(b);
                return;
            }
        }
    }
    if depth >= 24 {
        out.push(b);
        return;
    }
    let split = split.clamp(1, n - 2);
    let centre = unit(pts[split - 1] - pts[split + 1])
        .or_else(|| unit(pts[split - 1] - pts[split]))
        .unwrap_or(t2);
    fit_cubic(&pts[..=split], t1, centre, error, depth + 1, out);
    fit_cubic(&pts[split..], centre * -1.0, t2, error, depth + 1, out);
}

fn chord_parameters(pts: &[Vec2]) -> Vec<f64> {
    let mut u = vec![0.0];
    for w in pts.windows(2) {
        let last = *u.last().expect("first");
        u.push(last + w[0].distance(w[1]));
    }
    let total = *u.last().expect("first");
    if total > 0.0 {
        for t in &mut u {
            *t /= total;
        }
    }
    u
}

/// `c` with fewer nodes, every point within `tolerance` (0.1 mm) of the original. Returns `c`
/// unchanged when nothing can go.
pub fn contour(c: &Contour, tolerance: f64) -> Contour {
    let segs = c.segment_count();
    if segs < 2 || tolerance.is_nan() || tolerance <= 0.0 {
        return c.clone();
    }
    let n = c.points.len();
    // Vertices that stay: the ends of an open contour and every break.
    let mut breaks: Vec<usize> = Vec::new();
    for v in 0..n {
        let before = if v > 0 {
            Some(v - 1)
        } else if c.closed {
            Some(segs - 1)
        } else {
            None
        };
        let after = (v < segs).then_some(v);
        match (before, after) {
            (Some(b), Some(a)) if !is_break(c, b, a) => {}
            _ => breaks.push(v),
        }
    }
    // A closed contour without breaks starts its one run at vertex 0; a closed straight
    // polygon also needs a second kept vertex (the farthest from the first) so its run is not
    // collapsed onto one point.
    if c.closed && breaks.is_empty() {
        breaks.push(0);
        if !c.has_curves() {
            let far = (1..n)
                .max_by(|&i, &j| {
                    c.points[i]
                        .distance(c.points[0])
                        .total_cmp(&c.points[j].distance(c.points[0]))
                })
                .unwrap_or(n / 2);
            breaks.push(far);
        }
    }
    let mut points = Vec::new();
    let mut curves = Vec::new();
    let runs = breaks.len() - usize::from(!c.closed);
    for r in 0..runs {
        let from = breaks[r];
        let to = if r + 1 < breaks.len() {
            breaks[r + 1]
        } else {
            breaks[0] + n
        };
        let run: Vec<usize> = (from..to).map(|s| s % n).collect();
        // A run mixing straight and curved segments cannot occur (breaks split them).
        let (v, cv) = simplify_run(c, &run, tolerance);
        points.extend(v);
        curves.extend(cv);
    }
    if !c.closed {
        points.push(*c.points.last().expect("2 or more points"));
    }
    let minimum = if c.closed {
        if curves.iter().any(Option::is_some) {
            2
        } else {
            3
        }
    } else {
        2
    };
    if points.len() < minimum || points.len() >= n {
        return c.clone();
    }
    let mut out = Contour::new(points, c.closed);
    if curves.iter().any(Option::is_some) {
        out.curves = curves;
    }
    out
}

/// How far the sides move when direction line `i` (not the first or last) is left out.
fn rung_deviation(rungs: &[[Vec2; 2]], i: usize) -> f64 {
    (0..2)
        .map(|k| segment_distance(rungs[i][k], rungs[i - 1][k], rungs[i + 1][k]))
        .fold(0.0, f64::max)
}

/// Manual Punch: the direction lines without the one whose removal moves the sides least, if
/// that is within `tolerance` and leaves a valid column of at least two lines.
pub fn column(rungs: &[[Vec2; 2]], curved: bool, tolerance: f64) -> Option<Vec<[Vec2; 2]>> {
    if rungs.len() <= 2 {
        return None;
    }
    let mut order: Vec<(f64, usize)> = (1..rungs.len() - 1)
        .map(|i| (rung_deviation(rungs, i), i))
        .filter(|(d, _)| *d <= tolerance)
        .collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    order.into_iter().find_map(|(_, i)| {
        let mut out = rungs.to_vec();
        out.remove(i);
        let ok = if curved {
            column::validate(&column::curved_rungs(&out)).is_ok()
        } else {
            column::validate(&out).is_ok()
        };
        ok.then_some(out)
    })
}

/// Satin skeleton: the centre-line points Douglas–Peucker keeps within `tolerance`, a point
/// also kept where its width differs from the width interpolated between its neighbours by
/// more than the tolerance (widths are mm, points 0.1 mm).
pub fn skeleton(s: &Skeleton, tolerance: f64) -> Option<Skeleton> {
    let n = s.points.len();
    if n <= 2 {
        return None;
    }
    let mut kept = douglas_peucker(&s.points, tolerance);
    // Keep width changes: compare each dropped point's width with the line between the kept
    // points around it, and keep it when they differ by more than the tolerance.
    let tol_mm = tolerance / 10.0;
    loop {
        let mut added = false;
        let mut prev = 0;
        for next in 1..n {
            if !kept[next] {
                continue;
            }
            let span: f64 = s.points[prev..=next]
                .windows(2)
                .map(|w| w[0].distance(w[1]))
                .sum();
            let mut along = 0.0;
            let (mut worst, mut at) = (tol_mm, None);
            for i in prev + 1..next {
                along += s.points[i - 1].distance(s.points[i]);
                let f = if span > 0.0 { along / span } else { 0.0 };
                let expected = s.widths[prev] + (s.widths[next] - s.widths[prev]) * f;
                let d = (s.widths[i] - expected).abs();
                if d > worst {
                    (worst, at) = (d, Some(i));
                }
            }
            if let Some(i) = at {
                kept[i] = true;
                added = true;
            }
            prev = next;
        }
        if !added {
            break;
        }
    }
    let out = Skeleton {
        points: (0..n).filter(|&i| kept[i]).map(|i| s.points[i]).collect(),
        widths: (0..n).filter(|&i| kept[i]).map(|i| s.widths[i]).collect(),
    };
    (out.points.len() < n && out.validate().is_ok()).then_some(out)
}

/// Whether `a` and `b` are the same outline within `tolerance` (same points and curves).
pub fn same_outline(a: &Contour, b: &Contour, tolerance: f64) -> bool {
    let close = |p: Vec2, q: Vec2| p.distance(q) <= tolerance;
    a.closed == b.closed
        && a.points.len() == b.points.len()
        && a.points.iter().zip(&b.points).all(|(p, q)| close(*p, *q))
        && (0..a.segment_count()).all(|i| match (a.curve(i), b.curve(i)) {
            (None, None) => true,
            (Some(x), Some(y)) => close(x.c1, y.c1) && close(x.c2, y.c2),
            _ => false,
        })
}

/// Largest distance from a point of `a` to the outline `b` (both flattened), 0.1 mm.
pub fn directed_distance(a: &Contour, b: &Contour) -> f64 {
    let pa = dense(a);
    let pb = b.path();
    pa.iter()
        .map(|&p| {
            pb.windows(2)
                .map(|w| segment_distance(p, w[0], w[1]))
                .fold(f64::INFINITY, f64::min)
        })
        .fold(0.0, f64::max)
}

/// Hausdorff distance between two outlines (flattened), 0.1 mm.
pub fn hausdorff(a: &Contour, b: &Contour) -> f64 {
    directed_distance(a, b).max(directed_distance(b, a))
}

/// The flattened outline with points at most 0.05 mm apart.
fn dense(c: &Contour) -> Vec<Vec2> {
    let path = c.path();
    let mut out = Vec::new();
    for w in path.windows(2) {
        let steps = (w[0].distance(w[1]) / 0.5).ceil().max(1.0) as usize;
        for k in 0..steps {
            out.push(w[0].lerp(w[1], k as f64 / steps as f64));
        }
    }
    out.extend(path.last().copied());
    if out.is_empty() {
        out.push(v2(0.0, 0.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmath::Det;

    /// A traced circle: 360 vertices with ±0.05 mm noise.
    fn traced() -> Contour {
        let pts = (0..360)
            .map(|i| {
                let t = (i as f64).to_radians();
                let r = 200.0 + if i % 2 == 0 { 0.5 } else { -0.5 };
                v2(r * t.dcos(), r * t.dsin())
            })
            .collect();
        Contour::new(pts, true)
    }

    #[test]
    fn a_traced_outline_loses_nodes_within_the_tolerance() {
        let c = traced();
        let s = contour(&c, 2.0);
        assert!(s.points.len() < c.points.len() / 3, "{}", s.points.len());
        assert!(hausdorff(&c, &s) <= 2.0 + 1e-9, "{}", hausdorff(&c, &s));
        // A second pass removes little.
        let again = contour(&s, 2.0);
        assert!(again.points.len() * 10 >= s.points.len() * 9);
    }

    #[test]
    fn curved_runs_are_refitted_and_corners_kept() {
        // An open run of 12 short curves along a sine wave, then a corner.
        let mut c = Contour::new(
            (0..=12)
                .map(|i| v2(i as f64 * 10.0, (i as f64 * 0.5).dsin() * 30.0))
                .collect(),
            false,
        );
        let n = c.segment_count();
        for i in 0..n {
            let (a, b) = c.segment_ends(i);
            let (x0, x1) = (a.x, b.x);
            let d = |x: f64| v2(1.0, (x / 20.0).dcos() * 1.5);
            let h = (x1 - x0) / 3.0;
            c.set_curve(
                i,
                Some(Bezier {
                    c1: a + d(x0) * h,
                    c2: b - d(x1) * h,
                }),
            );
        }
        c.points.push(v2(120.0, 100.0));
        c.curves.push(None);
        let s = contour(&c, 2.0);
        assert!(s.points.len() < c.points.len(), "{}", s.points.len());
        assert_eq!(s.points.first(), c.points.first());
        assert_eq!(s.points.last(), c.points.last());
        assert!(s.points.contains(&c.points[12]), "the corner stays");
        assert!(hausdorff(&c, &s) <= 2.0 + 1e-6, "{}", hausdorff(&c, &s));
    }

    #[test]
    fn a_square_keeps_its_corners() {
        let c = Contour::rectangle(0.0, 0.0, 100.0, 50.0);
        assert_eq!(contour(&c, 2.0), c);
        let mut noisy = Contour::new(
            vec![
                v2(0.0, 0.0),
                v2(50.0, 0.5),
                v2(100.0, 0.0),
                v2(100.0, 50.0),
                v2(0.0, 50.0),
            ],
            true,
        );
        noisy = contour(&noisy, 2.0);
        assert_eq!(noisy.points.len(), 4);
    }

    #[test]
    fn columns_lose_one_line_per_pass_and_stay_valid() {
        let rungs: Vec<[Vec2; 2]> = (0..6)
            .map(|i| [v2(i as f64 * 20.0, 0.0), v2(i as f64 * 20.0, 30.0)])
            .collect();
        let once = column(&rungs, false, 2.0).expect("one goes");
        assert_eq!(once.len(), 5);
        assert!(column::validate(&once).is_ok());
        let mut r = rungs;
        while let Some(next) = column(&r, false, 2.0) {
            assert_eq!(next.len(), r.len() - 1);
            r = next;
        }
        assert_eq!(r.len(), 2);
        // A bent column keeps the bend.
        let bent = vec![
            [v2(0.0, 0.0), v2(0.0, 30.0)],
            [v2(50.0, 40.0), v2(50.0, 70.0)],
            [v2(100.0, 0.0), v2(100.0, 30.0)],
        ];
        assert!(column(&bent, false, 2.0).is_none());
    }

    #[test]
    fn skeletons_keep_the_bends_and_width_changes() {
        let s = Skeleton {
            points: (0..=10)
                .map(|i| v2(i as f64 * 10.0, 0.0))
                .chain([v2(100.0, 50.0)])
                .collect(),
            widths: (0..=10)
                .map(|i| if i == 5 { 6.0 } else { 3.0 })
                .chain([3.0])
                .collect(),
        };
        let out = skeleton(&s, 2.0).expect("simplified");
        assert!(out.points.contains(&v2(50.0, 0.0)), "width change kept");
        assert!(out.points.contains(&v2(100.0, 0.0)), "bend kept");
        assert!(out.points.len() < s.points.len());
    }
}
