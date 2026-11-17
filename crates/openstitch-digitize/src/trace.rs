//! Contours from pixel masks: marching squares, Douglas–Peucker simplification and optional
//! least-squares cubic Bézier fitting. Pixel coordinates: pixel `(x, y)` covers
//! `[x, x + 1) × [y, y + 1)`; samples sit at pixel centres.

use crate::raster::Frame;
use openstitch_design::fmath::Det;
use openstitch_design::{Bezier, Contour, Vec2, v2};

/// A closed loop in pixel coordinates (first point not repeated).
pub type Loop = Vec<Vec2>;

/// Traces the boundaries of `mask` (`w`×`h`, row-major) with marching squares on the pixel
/// centres. Saddle cells keep diagonal neighbours apart (4-connectivity), so every
/// 4-connected set gets its own outline. Each loop runs with the inside on the same side:
/// outer boundaries have positive [`signed_area`], holes negative. Loops come out in a fixed
/// order (by the first boundary edge in scan order).
pub fn marching_squares(mask: &[bool], w: usize, h: usize) -> Vec<Loop> {
    let (sw, sh) = (w + 2, h + 2);
    let inside = |sx: usize, sy: usize| -> bool {
        sx >= 1 && sy >= 1 && sx <= w && sy <= h && mask[(sy - 1) * w + (sx - 1)]
    };
    const NONE: u32 = u32::MAX;
    let mut next = vec![NONE; 2 * sw * sh];
    let horizontal = |x: usize, y: usize| 2 * (y * sw + x);
    let vertical = |x: usize, y: usize| 2 * (y * sw + x) + 1;
    for cy in 0..sh - 1 {
        for cx in 0..sw - 1 {
            // Corners clockwise from the top-left, and the edge after each corner.
            let c = [
                inside(cx, cy),
                inside(cx + 1, cy),
                inside(cx + 1, cy + 1),
                inside(cx, cy + 1),
            ];
            if c.iter().all(|&b| b) || c.iter().all(|&b| !b) {
                continue;
            }
            let edges = [
                horizontal(cx, cy),
                vertical(cx + 1, cy),
                horizontal(cx, cy + 1),
                vertical(cx, cy),
            ];
            for k in 0..4 {
                if !c[k] && c[(k + 1) % 4] {
                    // Entry: pair it with the next exit clockwise, which cuts off only the
                    // inside corners between them.
                    for j in 1..4 {
                        let e = (k + j) % 4;
                        if c[e] && !c[(e + 1) % 4] {
                            next[edges[k]] = edges[e] as u32;
                            break;
                        }
                    }
                }
            }
        }
    }
    let point = |id: usize| -> Vec2 {
        let cell = id / 2;
        let (x, y) = ((cell % sw) as f64, (cell / sw) as f64);
        // Sample (sx, sy) sits at pixel centre (sx - 0.5, sy - 0.5).
        if id.is_multiple_of(2) {
            v2(x, y - 0.5)
        } else {
            v2(x - 0.5, y)
        }
    };
    let mut loops = Vec::new();
    let mut seen = vec![false; next.len()];
    for start in 0..next.len() {
        if next[start] == NONE || seen[start] {
            continue;
        }
        let mut l = Vec::new();
        let mut e = start;
        while !seen[e] && next[e] != NONE {
            seen[e] = true;
            l.push(point(e));
            e = next[e] as usize;
        }
        if l.len() >= 3 {
            // Linked inside-on-the-left in y-down space; reverse so outer loops are positive.
            l.reverse();
            loops.push(l);
        }
    }
    loops
}

/// Shoelace area of a closed loop; positive for outer boundaries from [`marching_squares`].
pub fn signed_area(l: &[Vec2]) -> f64 {
    let n = l.len();
    (0..n)
        .map(|i| {
            let (a, b) = (l[i], l[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let d = b - a;
    let len2 = d.x * d.x + d.y * d.y;
    if len2 == 0.0 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

/// Douglas–Peucker on `points[first..=last]` (iterative); marks kept indices.
fn dp(points: &[Vec2], first: usize, last: usize, tol: f64, keep: &mut [bool]) {
    let mut stack = vec![(first, last)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (mut worst, mut at) = (0.0, a);
        for i in a + 1..b {
            let d = segment_distance(points[i], points[a], points[b]);
            if d > worst {
                (worst, at) = (d, i);
            }
        }
        if worst > tol {
            keep[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
}

/// Indices of the vertices Douglas–Peucker keeps on a closed loop (at least 3 when the loop
/// has 3 points), in order.
pub fn simplify_closed(l: &[Vec2], tol: f64) -> Vec<usize> {
    let n = l.len();
    if n <= 3 {
        return (0..n).collect();
    }
    let far = (1..n)
        .max_by(|&i, &j| {
            l[i].distance(l[0])
                .total_cmp(&l[j].distance(l[0]))
                .then(j.cmp(&i))
        })
        .unwrap_or(n / 2);
    let mut ring: Vec<Vec2> = l.to_vec();
    ring.push(l[0]);
    let mut keep = vec![false; n + 1];
    keep[0] = true;
    keep[far] = true;
    dp(&ring, 0, far, tol, &mut keep);
    dp(&ring, far, n, tol, &mut keep);
    let mut out: Vec<usize> = (0..n).filter(|&i| keep[i]).collect();
    if out.len() < 3 {
        // A sliver: add the point farthest from the 0–far chord so a triangle survives.
        let third = (1..n)
            .filter(|&i| i != far)
            .max_by(|&i, &j| {
                segment_distance(l[i], l[0], l[far])
                    .total_cmp(&segment_distance(l[j], l[0], l[far]))
                    .then(j.cmp(&i))
            })
            .unwrap_or(1);
        out = vec![0, far, third];
        out.sort_unstable();
    }
    out
}

/// Indices Douglas–Peucker keeps on an open polyline (always the two ends).
pub fn simplify_open(l: &[Vec2], tol: f64) -> Vec<usize> {
    let n = l.len();
    if n <= 2 {
        return (0..n).collect();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    dp(l, 0, n - 1, tol, &mut keep);
    (0..n).filter(|&i| keep[i]).collect()
}

/// Whether the closed polyline `p` (first point not repeated) crosses itself: two
/// non-adjacent segments that properly intersect (#1137). Touching at a shared vertex and
/// collinear overlaps are not counted, as the design checker counts them. Segments are swept
/// in order of their left ends, so long outlines cost far less than every pair.
pub fn crosses_itself(p: &[Vec2]) -> bool {
    let n = p.len();
    if n < 4 {
        return false;
    }
    let seg = |i: usize| (p[i], p[(i + 1) % n]);
    let mut order: Vec<usize> = (0..n).collect();
    let left = |i: usize| {
        let (a, b) = seg(i);
        a.x.min(b.x)
    };
    order.sort_by(|&i, &j| left(i).total_cmp(&left(j)).then(i.cmp(&j)));
    for (k, &i) in order.iter().enumerate() {
        let (a, b) = seg(i);
        let right = a.x.max(b.x);
        let (lo, hi) = (a.y.min(b.y), a.y.max(b.y));
        for &j in &order[k + 1..] {
            let (q, r) = seg(j);
            if q.x.min(r.x) > right {
                break;
            }
            let adjacent = (i + 1) % n == j || (j + 1) % n == i;
            if adjacent || q.y.max(r.y) < lo || q.y.min(r.y) > hi {
                continue;
            }
            let d = (b.x - a.x) * (r.y - q.y) - (b.y - a.y) * (r.x - q.x);
            if d.abs() < 1e-9 {
                continue;
            }
            let t = ((q.x - a.x) * (r.y - q.y) - (q.y - a.y) * (r.x - q.x)) / d;
            let u = ((q.x - a.x) * (b.y - a.y) - (q.y - a.y) * (b.x - a.x)) / d;
            if t > 1e-6 && t < 1.0 - 1e-6 && u > 1e-6 && u < 1.0 - 1e-6 {
                return true;
            }
        }
    }
    false
}

/// A closed design contour from the marching-squares loop `dense` that never crosses itself
/// (#1137): Douglas–Peucker at `tol` and, with `curves`, fitted Béziers, as [`contour`] makes
/// it. Neither guarantees a simple outline: a kept vertex can cut across a narrow neck, and a
/// fitted curve can bulge across a neighbouring stretch. When the flattened outline crosses
/// itself, the curves go first; if the straight outline still crosses, the tolerance halves
/// until it does not. The dense loop itself never crosses (marching squares keeps diagonal
/// neighbours apart), so this ends there at the latest.
pub fn simple_contour(
    dense: &[Vec2],
    tol: f64,
    curves: bool,
    corner_degrees: f64,
    frame: &Frame,
) -> Contour {
    let mut tol = tol;
    let mut curves = curves;
    loop {
        let kept = simplify_closed(dense, tol);
        let c = contour(dense, &kept, true, curves, tol, corner_degrees, frame);
        let mut flat = c.path();
        flat.pop();
        if kept.len() == dense.len() && !curves || !crosses_itself(&flat) {
            return c;
        }
        if curves {
            curves = false;
        } else {
            tol /= 2.0;
            if tol < 0.01 {
                // Every vertex kept.
                let all: Vec<usize> = (0..dense.len()).collect();
                return contour(dense, &all, true, false, tol, corner_degrees, frame);
            }
        }
    }
}

/// Builds a design contour from the dense pixel-space polyline `dense`, keeping vertices
/// `kept` (indices into `dense`). With `curves`, each segment between two kept vertices
/// becomes a cubic Bézier fitted by least squares to the dense points it replaces (tangents
/// from the neighbouring vertices; corners sharper than `corner_degrees` stay sharp) when
/// that fits within `tol` pixels and the points are not straight; otherwise it stays
/// straight.
pub fn contour(
    dense: &[Vec2],
    kept: &[usize],
    closed: bool,
    curves: bool,
    tol: f64,
    corner_degrees: f64,
    frame: &Frame,
) -> Contour {
    let m = kept.len();
    let v: Vec<Vec2> = kept.iter().map(|&i| dense[i]).collect();
    let segments = if closed { m } else { m.saturating_sub(1) };
    let mut fitted: Vec<Option<(Vec2, Vec2)>> = vec![None; segments];
    if curves && m >= 3 {
        let at = |i: isize| -> Option<Vec2> {
            if closed {
                Some(v[i.rem_euclid(m as isize) as usize])
            } else if i < 0 || i >= m as isize {
                None
            } else {
                Some(v[i as usize])
            }
        };
        let cos_corner = corner_degrees.to_radians().dcos();
        // Unit tangent at vertex i, or None at corners and open ends.
        let tangent = |i: usize| -> Option<Vec2> {
            let (p, c, n) = (at(i as isize - 1)?, at(i as isize)?, at(i as isize + 1)?);
            let (a, b) = (unit(c - p)?, unit(n - c)?);
            if a.x * b.x + a.y * b.y < cos_corner {
                return None;
            }
            unit(n - p)
        };
        for (s, slot) in fitted.iter_mut().enumerate() {
            let e = (s + 1) % m;
            let (t0, t1) = (tangent(s), tangent(e));
            if t0.is_none() && t1.is_none() {
                continue;
            }
            let pts = between(dense, kept[s], kept[e], closed);
            *slot = fit_segment(&pts, v[s], v[e], t0, t1, tol);
        }
    }
    let points: Vec<Vec2> = v.iter().map(|p| frame.point(p.x, p.y)).collect();
    let mut c = Contour::new(points, closed);
    if fitted.iter().any(Option::is_some) {
        c.curves = fitted
            .iter()
            .map(|f| {
                f.map(|(c1, c2)| Bezier {
                    c1: frame.point(c1.x, c1.y),
                    c2: frame.point(c2.x, c2.y),
                })
            })
            .collect();
    }
    c
}

fn unit(d: Vec2) -> Option<Vec2> {
    let l = d.length();
    (l > 1e-9).then(|| d * (1.0 / l))
}

/// Dense points from index `a` to `b` inclusive, wrapping on closed loops.
fn between(dense: &[Vec2], a: usize, b: usize, closed: bool) -> Vec<Vec2> {
    if b > a || !closed {
        return dense[a..=b.max(a)].to_vec();
    }
    dense[a..]
        .iter()
        .chain(dense[..=b].iter())
        .copied()
        .collect()
}

fn bezier_at(p0: Vec2, c1: Vec2, c2: Vec2, p3: Vec2, u: f64) -> Vec2 {
    let s = 1.0 - u;
    p0 * (s * s * s) + c1 * (3.0 * s * s * u) + c2 * (3.0 * s * u * u) + p3 * (u * u * u)
}

/// Schneider-style least-squares fit of the two handle lengths along fixed tangent directions
/// (the chord where a tangent is missing), with three rounds of Newton reparameterisation.
/// Returns the control points if the points are not on a straight line (within a quarter
/// pixel) and the curve stays within `tol` (at least one pixel, which absorbs pixel staircase
/// noise) of every point.
fn fit_segment(
    pts: &[Vec2],
    p0: Vec2,
    p3: Vec2,
    t0: Option<Vec2>,
    t1: Option<Vec2>,
    tol: f64,
) -> Option<(Vec2, Vec2)> {
    if pts.len() < 3 {
        return None;
    }
    let chord = p0.distance(p3);
    let dir = unit(p3 - p0)?;
    let t0 = t0.unwrap_or(dir);
    let t1 = t1.map(|t| t * -1.0).unwrap_or(dir * -1.0);
    let dot = |a: Vec2, b: Vec2| a.x * b.x + a.y * b.y;
    // Chord-length parameters.
    let mut u = vec![0.0; pts.len()];
    for i in 1..pts.len() {
        u[i] = u[i - 1] + pts[i].distance(pts[i - 1]);
    }
    let total = u[pts.len() - 1];
    if total <= 0.0 {
        return None;
    }
    for x in &mut u {
        *x /= total;
    }
    let handles = |u: &[f64]| -> (Vec2, Vec2) {
        let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (p, &t) in pts.iter().zip(u) {
            let s = 1.0 - t;
            let (b0, b1, b2, b3) = (s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t);
            let a1 = t0 * b1;
            let a2 = t1 * b2;
            c00 += dot(a1, a1);
            c01 += dot(a1, a2);
            c11 += dot(a2, a2);
            let rest = *p - (p0 * (b0 + b1) + p3 * (b2 + b3));
            x0 += dot(a1, rest);
            x1 += dot(a2, rest);
        }
        let det = c00 * c11 - c01 * c01;
        let (mut a1, mut a2) = if det.abs() > 1e-12 {
            ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det)
        } else {
            (chord / 3.0, chord / 3.0)
        };
        let eps = chord * 1e-3;
        if !(a1.is_finite() && a2.is_finite()) || a1 < eps || a2 < eps {
            (a1, a2) = (chord / 3.0, chord / 3.0);
        }
        (p0 + t0 * a1.min(chord), p3 + t1 * a2.min(chord))
    };
    let (mut c1, mut c2) = handles(&u);
    for _ in 0..3 {
        for (p, t) in pts.iter().zip(u.iter_mut()) {
            let s = 1.0 - *t;
            let q = bezier_at(p0, c1, c2, p3, *t);
            let d1 = (c1 - p0) * (3.0 * s * s)
                + (c2 - c1) * (6.0 * s * *t)
                + (p3 - c2) * (3.0 * *t * *t);
            let d2 = (c2 - c1 * 2.0 + p0) * (6.0 * s) + (p3 - c2 * 2.0 + c1) * (6.0 * *t);
            let num = dot(q - *p, d1);
            let den = dot(d1, d1) + dot(q - *p, d2);
            if den.abs() > 1e-12 {
                *t = (*t - num / den).clamp(0.0, 1.0);
            }
        }
        (c1, c2) = handles(&u);
    }
    let (mut curve_err, mut line_err) = (0.0f64, 0.0f64);
    for (p, &t) in pts.iter().zip(&u) {
        curve_err = curve_err.max(p.distance(bezier_at(p0, c1, c2, p3, t)));
        line_err = line_err.max(segment_distance(*p, p0, p3));
    }
    // Truly straight runs stay straight; elsewhere the curve wins whenever it is within the
    // tolerance, since its tangents join smoothly with the neighbouring segments.
    (line_err > 0.25 && curve_err <= tol.max(1.0)).then_some((c1, c2))
}

#[cfg(test)]
mod crossing_tests {
    use super::*;
    use openstitch_design::v2;

    /// #1137: a hole of Auto Punch's fill of the kit's storybook (`p03-storybook.jpg` at
    /// 199 mm, OpenStitch mode), as marching squares traced it in its mask's pixels. Douglas–
    /// Peucker at the default 0.7 pixel keeps 16 vertices, and the Béziers fitted between them
    /// cross; the design checker then called the storybook "Fix this first".
    const STORYBOOK_HOLE: [(f64, f64); 60] = [
        (41.0, 8.5),
        (41.5, 9.0),
        (42.0, 9.5),
        (42.0, 10.5),
        (41.5, 11.0),
        (41.0, 11.5),
        (41.0, 12.5),
        (41.5, 13.0),
        (42.0, 13.5),
        (41.5, 14.0),
        (41.0, 14.5),
        (40.5, 15.0),
        (40.0, 15.5),
        (40.0, 16.5),
        (40.0, 17.5),
        (40.5, 18.0),
        (41.0, 18.5),
        (41.0, 19.5),
        (41.0, 20.5),
        (41.5, 21.0),
        (42.5, 21.0),
        (43.0, 21.5),
        (43.5, 22.0),
        (44.5, 22.0),
        (45.5, 22.0),
        (46.5, 22.0),
        (47.0, 21.5),
        (47.0, 20.5),
        (47.0, 19.5),
        (47.5, 19.0),
        (48.5, 19.0),
        (49.5, 19.0),
        (50.0, 18.5),
        (50.5, 18.0),
        (51.0, 17.5),
        (50.5, 17.0),
        (50.0, 17.5),
        (49.5, 18.0),
        (49.0, 17.5),
        (48.5, 17.0),
        (47.5, 17.0),
        (47.0, 16.5),
        (47.0, 15.5),
        (46.5, 15.0),
        (46.0, 14.5),
        (46.0, 13.5),
        (45.5, 13.0),
        (45.0, 12.5),
        (45.0, 11.5),
        (45.5, 11.0),
        (46.5, 11.0),
        (47.0, 10.5),
        (47.0, 9.5),
        (47.0, 8.5),
        (46.5, 8.0),
        (45.5, 8.0),
        (44.5, 8.0),
        (43.5, 8.0),
        (42.5, 8.0),
        (41.5, 8.0),
    ];

    #[test]
    fn a_traced_contour_never_crosses_itself() {
        let dense: Vec<Vec2> = STORYBOOK_HOLE.iter().map(|&(x, y)| v2(x, y)).collect();
        assert!(!crosses_itself(&dense), "the traced loop itself is simple");
        // The kit's placement: 199 mm over the 768-pixel image (design units per pixel). The
        // curves cross at 2.25 units per pixel and above, once flattening resolves them.
        let frame = Frame {
            origin: v2(0.0, 0.0),
            scale: 1990.0 / 768.0,
        };
        let kept = simplify_closed(&dense, 0.7);
        let before = contour(&dense, &kept, true, true, 0.7, 55.0, &frame);
        let mut flat = before.path();
        flat.pop();
        assert!(crosses_itself(&flat), "the case still reproduces");
        let after = simple_contour(&dense, 0.7, true, 55.0, &frame);
        let mut flat = after.path();
        flat.pop();
        assert!(!crosses_itself(&flat));
        // Only the curves went: the same 16 vertices, now straight.
        assert_eq!(after.points, before.points);
        assert!(!after.has_curves());
    }

    #[test]
    fn crossing_test_counts_proper_crossings_only() {
        let bow: Vec<Vec2> = [(0.0, 0.0), (4.0, 4.0), (4.0, 0.0), (0.0, 4.0)]
            .iter()
            .map(|&(x, y)| v2(x, y))
            .collect();
        assert!(crosses_itself(&bow));
        let square: Vec<Vec2> = [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]
            .iter()
            .map(|&(x, y)| v2(x, y))
            .collect();
        assert!(!crosses_itself(&square));
        // A hole touching the outline at one vertex (a figure eight's waist) is not a crossing.
        let eight: Vec<Vec2> = [
            (0.0, 0.0),
            (2.0, 2.0),
            (4.0, 0.0),
            (4.0, 4.0),
            (2.0, 2.0),
            (0.0, 4.0),
        ]
        .iter()
        .map(|&(x, y)| v2(x, y))
        .collect();
        assert!(!crosses_itself(&eight));
    }
}
