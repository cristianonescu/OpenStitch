//! Knife (#477): one open cut line splits every object it crosses. Regions are cut by
//! subtracting a hairline stroke of the cut line (0.01 mm wide) and taking the connected
//! pieces, whose vertices on the cut are then put back onto the line, so neighbouring pieces
//! share the cut edge; curves away from the cut stay curves (see [`crate::boolean`]). Lines are
//! split at their crossings (a closed outline cut once becomes one open line starting there).
//! A Manual Punch column is cut between the two direction lines around each crossing, with
//! the cut itself as a new direction line ending one half and starting the next; a satin
//! skeleton is cut at the crossing of its centre line, the width interpolated. Own code.
//! Lengths are 0.1 mm.

use crate::boolean::{self, BoolOp, Region};
use crate::column;
use crate::geometry::{Contour, Vec2};
use crate::skeleton::Skeleton;
use i_overlay::mesh::float::stroke::offset::StrokeOffset;
use i_overlay::mesh::float::style::StrokeStyle;

/// Width of the stroke cut out of regions, 0.1 mm (0.01 mm).
const SLIT: f64 = 0.1;
/// Samples per curved segment when looking for crossings.
const CURVE_SAMPLES: usize = 64;

/// Where segments a–b and c–d cross: the parameters along each (0..1), if they do.
pub fn crossing(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<(f64, f64)> {
    let r = b - a;
    let s = d - c;
    let den = r.x * s.y - r.y * s.x;
    if den.abs() < 1e-12 {
        return None;
    }
    let q = c - a;
    let t = (q.x * s.y - q.y * s.x) / den;
    let u = (q.x * r.y - q.y * r.x) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some((t, u))
}

fn segments(line: &[Vec2]) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
    line.windows(2).map(|w| (w[0], w[1]))
}

/// Whether the cut line touches any edge of `contours`.
pub fn crosses(contours: &[Contour], line: &[Vec2]) -> bool {
    contours.iter().any(|c| {
        c.path()
            .windows(2)
            .any(|e| segments(line).any(|(a, b)| crossing(e[0], e[1], a, b).is_some()))
    })
}

/// Distance from `p` to the polyline `line`, and the nearest point on it.
pub fn nearest(p: Vec2, line: &[Vec2]) -> (f64, Vec2) {
    let mut best = (f64::INFINITY, p);
    for (a, b) in segments(line) {
        let d = b - a;
        let len2 = d.x * d.x + d.y * d.y;
        let t = if len2 == 0.0 {
            0.0
        } else {
            (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
        };
        let q = a + d * t;
        let dist = p.distance(q);
        if dist < best.0 {
            best = (dist, q);
        }
    }
    best
}

/// The region of `contours` (even-odd) cut along `line`: its connected pieces, when there are
/// two or more; `None` when the line does not cut it through.
pub fn region(contours: &[Contour], line: &[Vec2]) -> Option<Vec<Region>> {
    if line.len() < 2 || !crosses(contours, line) {
        return None;
    }
    let path: Vec<[f64; 2]> = line.iter().map(|p| [p.x, p.y]).collect();
    let stroke = path.stroke(StrokeStyle::new(SLIT), false);
    let slit: Vec<Contour> = stroke
        .into_iter()
        .flatten()
        .filter(|c| c.len() >= 3)
        .map(|c| Contour::new(c.into_iter().map(|[x, y]| Vec2 { x, y }).collect(), true))
        .collect();
    if slit.is_empty() {
        return None;
    }
    let mut pieces = boolean::combine(contours, &slit, BoolOp::Difference);
    if pieces.len() < 2 {
        return None;
    }
    // Vertices on the slit go back onto the cut line: neighbouring pieces share the edge.
    for r in &mut pieces {
        for c in r.iter_mut() {
            for p in &mut c.points {
                let (d, q) = nearest(*p, line);
                if d <= SLIT {
                    *p = q;
                }
            }
        }
    }
    Some(pieces)
}

/// Parameters (segment, t) along `c` where the cut line crosses it, in order along `c`.
fn crossings(c: &Contour, line: &[Vec2]) -> Vec<(usize, f64)> {
    let mut out = Vec::new();
    for i in 0..c.segment_count() {
        let (a, b) = c.segment_ends(i);
        let samples: Vec<(f64, Vec2)> = match c.curve(i) {
            None => vec![(0.0, a), (1.0, b)],
            Some(z) => (0..=CURVE_SAMPLES)
                .map(|k| {
                    let t = k as f64 / CURVE_SAMPLES as f64;
                    let s = 1.0 - t;
                    let p = a * (s * s * s)
                        + z.c1 * (3.0 * s * s * t)
                        + z.c2 * (3.0 * s * t * t)
                        + b * (t * t * t);
                    (t, p)
                })
                .collect(),
        };
        let mut here: Vec<f64> = Vec::new();
        for w in samples.windows(2) {
            for (p, q) in segments(line) {
                if let Some((u, _)) = crossing(w[0].1, w[1].1, p, q) {
                    here.push(w[0].0 + (w[1].0 - w[0].0) * u);
                }
            }
        }
        here.sort_by(f64::total_cmp);
        here.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        out.extend(here.into_iter().map(|t| (i, t)));
    }
    out
}

/// `c` cut at its crossings with `line` into open pieces, in order along `c`; `None` when the
/// line does not cross it. A crossing at an open contour's very end does not cut.
pub fn path(c: &Contour, line: &[Vec2]) -> Option<Vec<Contour>> {
    let cuts = crossings(c, line);
    cut_at(c, &cuts)
}

/// `c` cut at the points `cuts` (segment, parameter), in order along it.
fn cut_at(c: &Contour, cuts: &[(usize, f64)]) -> Option<Vec<Contour>> {
    let n = c.segment_count();
    // Ignore cuts at an open contour's ends; snap cuts at a vertex onto it.
    let mut marks: Vec<(usize, f64)> = cuts
        .iter()
        .copied()
        .filter(|&(i, t)| c.closed || !((i == 0 && t < 1e-6) || (i + 1 == n && t > 1.0 - 1e-6)))
        .collect();
    if marks.is_empty() {
        return None;
    }
    // Insert a vertex at every cut, from the last so earlier indices stay valid.
    let mut work = c.clone();
    let mut at: Vec<usize> = Vec::new();
    marks.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    // Several cuts on one segment: the later one is inserted first, which leaves the earlier
    // part of the segment as parameters 0..t_later (a straight line and de Casteljau both
    // reparametrise linearly), so earlier cuts are rescaled by it.
    let mut last: Option<(usize, f64)> = None;
    for &(seg, t0) in marks.iter().rev() {
        let t = match last {
            Some((s, later)) if s == seg && later > 0.0 => t0 / later,
            _ => t0,
        };
        if t <= 1e-6 {
            at.push(seg);
        } else if t >= 1.0 - 1e-6 {
            at.push((seg + 1) % work.points.len().max(1));
        } else {
            let index = work.insert_point(seg, t);
            for v in &mut at {
                if *v >= index {
                    *v += 1;
                }
            }
            at.push(index);
            last = Some((seg, t0));
        }
    }
    at.sort_unstable();
    at.dedup();
    let pts = work.points.len();
    let curve = |i: usize| work.curve(i % pts);
    let piece = |from: usize, to: usize| {
        // Vertices from..=to (wrapping for closed contours).
        let idx: Vec<usize> = if to > from {
            (from..=to).collect()
        } else {
            (from..pts).chain(0..=to).collect()
        };
        let mut p = Contour::new(idx.iter().map(|&i| work.points[i % pts]).collect(), false);
        let segs: Vec<_> = idx[..idx.len() - 1].iter().map(|&i| curve(i)).collect();
        if segs.iter().any(Option::is_some) {
            p.curves = segs;
        }
        p
    };
    let mut out = Vec::new();
    if work.closed {
        for k in 0..at.len() {
            let from = at[k];
            let to = at[(k + 1) % at.len()];
            let p = if at.len() == 1 {
                // Once round, back to the cut.
                let idx: Vec<usize> = (from..pts).chain(0..=from).collect();
                let mut q =
                    Contour::new(idx.iter().map(|&i| work.points[i % pts]).collect(), false);
                let segs: Vec<_> = idx[..idx.len() - 1].iter().map(|&i| curve(i)).collect();
                if segs.iter().any(Option::is_some) {
                    q.curves = segs;
                }
                q
            } else {
                piece(from, to)
            };
            out.push(p);
        }
    } else {
        let mut from = 0;
        for &v in &at {
            out.push(piece(from, v));
            from = v;
        }
        out.push(piece(from, pts - 1));
    }
    let out: Vec<Contour> = out.into_iter().filter(|p| p.points.len() >= 2).collect();
    (!out.is_empty() && (out.len() >= 2 || c.closed)).then_some(out)
}

/// The open or closed line `c` cut at the point nearest `p`, when that is within `reach`
/// (click to split).
pub fn path_at(c: &Contour, p: Vec2, reach: f64) -> Option<Vec<Contour>> {
    let mut best: Option<(f64, usize, f64)> = None;
    for i in 0..c.segment_count() {
        let poly = c.segment_polyline(i);
        let total: f64 = poly.windows(2).map(|w| w[0].distance(w[1])).sum();
        let mut along = 0.0;
        for w in poly.windows(2) {
            let d = w[1] - w[0];
            let len = d.length();
            let len2 = len * len;
            let t = if len2 == 0.0 {
                0.0
            } else {
                (((p.x - w[0].x) * d.x + (p.y - w[0].y) * d.y) / len2).clamp(0.0, 1.0)
            };
            let dist = p.distance(w[0] + d * t);
            // Arc-length fraction stands in for the curve parameter: close enough to cut
            // within the click's reach.
            let f = if total > 0.0 {
                (along + len * t) / total
            } else {
                0.0
            };
            if best.is_none_or(|b| dist < b.0) {
                best = Some((dist, i, f));
            }
            along += len;
        }
    }
    let (d, seg, t) = best?;
    if d > reach {
        return None;
    }
    cut_at(c, &[(seg, t)])
}

/// A Manual Punch column cut where the line crosses both of its sides between two direction
/// lines: the halves in order, the cut a direction line of both. `None` when the line crosses
/// no part of the column from side to side.
pub fn column_cut(rungs: &[[Vec2; 2]], curved: bool, line: &[Vec2]) -> Option<Vec<Vec<[Vec2; 2]>>> {
    if rungs.len() < 2 {
        return None;
    }
    // Curved Blocks are cut on their dense rungs, so the cut follows the curved sides.
    let dense = if curved {
        column::curved_rungs(rungs)
    } else {
        rungs.to_vec()
    };
    // Which original line each dense rung belongs after (for curved blocks).
    let owner: Vec<usize> = if curved {
        let mut o = Vec::with_capacity(dense.len());
        let mut k = 0;
        for r in &dense {
            if k + 1 < rungs.len() && *r == rungs[k + 1] {
                k += 1;
            }
            o.push(k);
        }
        o
    } else {
        (0..dense.len()).collect()
    };
    let mut cuts: Vec<(usize, [Vec2; 2])> = Vec::new();
    for j in 0..dense.len() - 1 {
        let side = |k: usize| {
            segments(line).find_map(|(a, b)| {
                crossing(dense[j][k], dense[j + 1][k], a, b)
                    .map(|(t, _)| dense[j][k].lerp(dense[j + 1][k], t))
            })
        };
        if let (Some(l), Some(r)) = (side(0), side(1)) {
            // A cut along a rung touches the quads on both sides of it: count it once.
            let same = cuts
                .last()
                .is_some_and(|c| c.1[0].distance(l) < 1e-6 && c.1[1].distance(r) < 1e-6);
            if !same {
                cuts.push((owner[j], [l, r]));
            }
        }
    }
    if cuts.is_empty() {
        return None;
    }
    let mut halves = Vec::new();
    let mut current: Vec<[Vec2; 2]> = Vec::new();
    let mut next = 0;
    for (after, cut) in cuts {
        while next <= after {
            current.push(rungs[next]);
            next += 1;
        }
        current.push(cut);
        halves.push(std::mem::replace(&mut current, vec![cut]));
    }
    current.extend(rungs[next..].iter().copied());
    halves.push(current);
    let valid = |h: &Vec<[Vec2; 2]>| {
        h.len() >= 2
            && if curved {
                column::validate(&column::curved_rungs(h)).is_ok()
            } else {
                column::validate(h).is_ok()
            }
    };
    halves.iter().all(valid).then_some(halves)
}

/// A satin skeleton cut where the line crosses its centre line, the width there interpolated.
pub fn skeleton_cut(s: &Skeleton, line: &[Vec2]) -> Option<Vec<Skeleton>> {
    let c = Contour::new(s.points.clone(), false);
    let mut cuts = crossings(&c, line);
    cuts.retain(|&(i, t)| !((i == 0 && t < 1e-6) || (i + 2 == s.points.len() && t > 1.0 - 1e-6)));
    if cuts.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    let mut points = vec![s.points[0]];
    let mut widths = vec![s.widths[0]];
    let mut k = 0;
    for i in 0..s.points.len() - 1 {
        while k < cuts.len() && cuts[k].0 == i {
            let t = cuts[k].1;
            let p = s.points[i].lerp(s.points[i + 1], t);
            let w = s.widths[i] + (s.widths[i + 1] - s.widths[i]) * t;
            points.push(p);
            widths.push(w);
            out.push(Skeleton {
                points: std::mem::replace(&mut points, vec![p]),
                widths: std::mem::replace(&mut widths, vec![w]),
            });
            k += 1;
        }
        points.push(s.points[i + 1]);
        widths.push(s.widths[i + 1]);
    }
    out.push(Skeleton { points, widths });
    // A cut exactly at a centre-line point leaves no zero-length step.
    for sk in &mut out {
        let keep: Vec<bool> = std::iter::once(true)
            .chain(sk.points.windows(2).map(|w| w[1].distance(w[0]) >= 1e-9))
            .collect();
        let mut k = keep.iter();
        sk.points.retain(|_| *k.next().expect("same length"));
        let mut k = keep.iter();
        sk.widths.retain(|_| *k.next().expect("same length"));
    }
    out.iter().all(|k| k.validate().is_ok()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::region_area;
    use crate::geometry::{Bezier, v2};

    fn total(r: &[Region]) -> f64 {
        r.iter().map(|x| region_area(x)).sum()
    }

    #[test]
    fn a_square_cut_in_two_keeps_its_area() {
        let sq = [Contour::rectangle(0.0, 0.0, 100.0, 100.0)];
        let pieces = region(&sq, &[v2(-10.0, 30.0), v2(110.0, 70.0)]).expect("cut");
        assert_eq!(pieces.len(), 2);
        assert!(
            (total(&pieces) - 10_000.0).abs() < 20.0,
            "{}",
            total(&pieces)
        );
        // Both halves end on the cut line exactly.
        let on_line = |p: Vec2| nearest(p, &[v2(-10.0, 30.0), v2(110.0, 70.0)]).0 < 1e-9;
        for r in &pieces {
            assert_eq!(r[0].points.iter().filter(|p| on_line(**p)).count(), 2);
        }
    }

    #[test]
    fn concave_shapes_with_holes_and_partial_cuts() {
        // A U shape cut across both arms: three pieces.
        let u = [Contour::new(
            vec![
                v2(0.0, 0.0),
                v2(30.0, 0.0),
                v2(30.0, 70.0),
                v2(70.0, 70.0),
                v2(70.0, 0.0),
                v2(100.0, 0.0),
                v2(100.0, 100.0),
                v2(0.0, 100.0),
            ],
            true,
        )];
        let p = region(&u, &[v2(-10.0, 40.0), v2(110.0, 40.0)]).expect("cut");
        assert_eq!(p.len(), 3);
        assert!((total(&p) - total(&boolean::simplify(&u))).abs() < 30.0);
        // A ring cut across: two C shapes.
        let ring = [
            Contour::rectangle(0.0, 0.0, 100.0, 100.0),
            Contour::rectangle(30.0, 30.0, 40.0, 40.0),
        ];
        let p = region(&ring, &[v2(50.0, -10.0), v2(50.0, 110.0)]).expect("cut");
        assert_eq!(p.len(), 2);
        assert!((total(&p) - 8_400.0).abs() < 30.0);
        // A line that stops inside does not cut.
        assert!(
            region(
                &[Contour::rectangle(0.0, 0.0, 100.0, 100.0)],
                &[v2(-10.0, 50.0), v2(50.0, 50.0)]
            )
            .is_none()
        );
        assert!(
            region(
                &[Contour::rectangle(0.0, 0.0, 100.0, 100.0)],
                &[v2(200.0, 50.0), v2(300.0, 50.0)]
            )
            .is_none()
        );
    }

    #[test]
    fn open_and_closed_lines() {
        let open = Contour::new(vec![v2(0.0, 0.0), v2(100.0, 0.0), v2(100.0, 100.0)], false);
        let p = path(&open, &[v2(50.0, -10.0), v2(50.0, 10.0)]).expect("cut");
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].points, vec![v2(0.0, 0.0), v2(50.0, 0.0)]);
        assert_eq!(p[1].points[0], v2(50.0, 0.0));
        let closed = Contour::rectangle(0.0, 0.0, 100.0, 100.0);
        let once = path(&closed, &[v2(50.0, -10.0), v2(50.0, 10.0)]).expect("cut");
        assert_eq!(once.len(), 1);
        assert!(!once[0].closed);
        assert_eq!(once[0].points.first(), once[0].points.last());
        let twice = path(&closed, &[v2(50.0, -10.0), v2(50.0, 110.0)]).expect("cut");
        assert_eq!(twice.len(), 2);
        // A curved segment is cut on the curve.
        let mut arc = Contour::new(vec![v2(0.0, 0.0), v2(100.0, 0.0)], false);
        arc.set_curve(
            0,
            Some(Bezier {
                c1: v2(0.0, 60.0),
                c2: v2(100.0, 60.0),
            }),
        );
        let p = path(&arc, &[v2(50.0, -10.0), v2(50.0, 100.0)]).expect("cut");
        assert_eq!(p.len(), 2);
        assert!((p[0].points[1].x - 50.0).abs() < 1e-6);
        assert!((p[0].points[1].y - 45.0).abs() < 1e-6);
        assert!(path(&open, &[v2(200.0, 0.0), v2(300.0, 0.0)]).is_none());
        // Click to split.
        let p = path_at(&open, v2(30.0, 1.0), 3.0).expect("near");
        assert_eq!(p[0].points.last(), Some(&v2(30.0, 0.0)));
        assert!(path_at(&open, v2(30.0, 20.0), 3.0).is_none());
    }

    #[test]
    fn columns_are_cut_between_their_lines_with_a_shared_line() {
        let rungs: Vec<[Vec2; 2]> = (0..4)
            .map(|i| [v2(i as f64 * 30.0, 0.0), v2(i as f64 * 30.0, 40.0)])
            .collect();
        let halves = column_cut(&rungs, false, &[v2(45.0, -10.0), v2(45.0, 50.0)]).expect("cut");
        assert_eq!(halves.len(), 2);
        assert_eq!(halves[0].len(), 3);
        assert_eq!(halves[1].len(), 3);
        assert_eq!(halves[0].last(), halves[1].first(), "shared cut line");
        assert_eq!(halves[0][..2], rungs[..2]);
        assert_eq!(halves[1][1..], rungs[2..]);
        // Crossing one side only does not cut.
        assert!(column_cut(&rungs, false, &[v2(45.0, -10.0), v2(45.0, 20.0)]).is_none());
        let curved = column_cut(&rungs, true, &[v2(45.0, -10.0), v2(45.0, 50.0)]).expect("cut");
        assert_eq!(curved.len(), 2);
        assert_eq!(curved[0].last(), curved[1].first());
    }

    #[test]
    fn skeletons_are_cut_at_the_centre_line() {
        let s = Skeleton {
            points: vec![v2(0.0, 0.0), v2(100.0, 0.0)],
            widths: vec![2.0, 4.0],
        };
        let parts = skeleton_cut(&s, &[v2(25.0, -20.0), v2(25.0, 20.0)]).expect("cut");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].points.last(), Some(&v2(25.0, 0.0)));
        assert!((parts[1].widths[0] - 2.5).abs() < 1e-9);
    }
}
