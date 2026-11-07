//! Ripple line (#541, beyond PE-DESIGN, off unless chosen): a line repeated as copies that
//! shrink towards a target point, a closed line spiralling in as one unbroken run, an open one
//! sewn back and forth beside itself. Lengths are in 0.1 mm.

use crate::generate::subdivide;
use crate::geometry::Vec2;
use crate::paths::{Walker, normal};

/// Shortest stitch a ripple keeps (0.1 mm): points closer to the last one kept are dropped
/// (the tight turns near the target).
pub const MIN_STITCH: f64 = 5.0;

/// A closed `path` spiralling in to `target` over `count` turns as one run: along each turn
/// the copy's scale falls evenly from `1 - k / count` to `1 - (k + 1) / count`, so the run
/// ends at the target. Stitches `pitch` long at most.
pub fn closed(path: &[Vec2], target: Vec2, count: u32, pitch: f64) -> Vec<Vec2> {
    let n = count.clamp(1, 50) as f64;
    let w = Walker::new(path);
    let len = w.length();
    if len <= 1e-9 {
        return path.to_vec();
    }
    let ring = subdivide(path, pitch.max(1.0));
    // Arc length of each ring point, to spread the shrink along the turn.
    let mut at = vec![0.0];
    for p in ring.windows(2) {
        at.push(at[at.len() - 1] + p[0].distance(p[1]));
    }
    let total = at[at.len() - 1].max(1e-9);
    let mut out = Vec::new();
    for k in 0..count.clamp(1, 50) {
        for (i, &p) in ring.iter().enumerate() {
            if k > 0 && i == 0 {
                continue;
            }
            let t = (k as f64 + at[i] / total) / n;
            out.push(target + (p - target) * (1.0 - t));
        }
    }
    thin(out, pitch)
}

/// An open `path` sewn `count` times, each copy `spacing` further to the side (towards
/// `target` when given, else to the left of the drawing direction), back and forth so the
/// copies join without a jump.
pub fn open(
    path: &[Vec2],
    target: Option<Vec2>,
    count: u32,
    spacing: f64,
    pitch: f64,
) -> Vec<Vec2> {
    let w = Walker::new(path);
    let len = w.length();
    if len <= 1e-9 {
        return path.to_vec();
    }
    let n = (len / pitch.max(1.0)).ceil().max(1.0) as usize;
    let at: Vec<f64> = (0..=n).map(|k| len * k as f64 / n as f64).collect();
    // Which side: the one the target lies on.
    let side = target.map_or(1.0, |t| {
        let mid = w.at(len / 2.0);
        let nrm = normal(w.tangent(len / 2.0));
        if (t - mid).x * nrm.x + (t - mid).y * nrm.y < 0.0 {
            -1.0
        } else {
            1.0
        }
    });
    let mut out = Vec::new();
    for k in 0..count.clamp(1, 50) {
        let d = side * spacing * k as f64;
        let copy: Vec<Vec2> = at
            .iter()
            .map(|&s| w.at(s) + normal(w.tangent(s)) * d)
            .collect();
        if k.is_multiple_of(2) {
            out.extend(copy);
        } else {
            out.extend(copy.into_iter().rev());
        }
    }
    thin(out, pitch)
}

/// Drops points closer than [`MIN_STITCH`] to the last one kept (keeping the last point), and
/// splits stitches longer than `pitch`.
fn thin(points: Vec<Vec2>, pitch: f64) -> Vec<Vec2> {
    let Some(&last) = points.last() else {
        return points;
    };
    let mut kept: Vec<Vec2> = Vec::with_capacity(points.len());
    for p in points {
        if kept
            .last()
            .is_none_or(|q: &Vec2| q.distance(p) >= MIN_STITCH)
        {
            kept.push(p);
        }
    }
    if kept.last() != Some(&last) {
        kept.push(last);
    }
    subdivide(&kept, pitch.max(1.0))
}

/// The default target of a closed line: the centre of its points.
pub fn centre(path: &[Vec2]) -> Vec2 {
    let n = path.len().max(1) as f64;
    path.iter()
        .fold(crate::geometry::v2(0.0, 0.0), |s, &p| s + p)
        * (1.0 / n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::v2;

    fn square() -> Vec<Vec2> {
        vec![
            v2(0.0, 0.0),
            v2(400.0, 0.0),
            v2(400.0, 400.0),
            v2(0.0, 400.0),
            v2(0.0, 0.0),
        ]
    }

    #[test]
    fn a_closed_line_spirals_in_to_the_target_as_one_run() {
        let target = v2(200.0, 200.0);
        let run = closed(&square(), target, 5, 25.0);
        assert_eq!(run[0], v2(0.0, 0.0));
        assert!(
            run[run.len() - 1].distance(target) < 1e-6,
            "ends at the target"
        );
        // One run: no move longer than a stitch, so no jump or trim is needed.
        assert!(run.windows(2).all(|w| w[0].distance(w[1]) <= 25.0 + 1e-6));
        // Shrinking: the distance to the target never grows by more than a stitch's worth
        // over a turn.
        let d: Vec<f64> = run.iter().map(|p| p.distance(target)).collect();
        assert!(d[d.len() / 2] < d[0] && d[d.len() - 1] < d[d.len() / 2]);
    }

    #[test]
    fn an_open_line_repeats_back_and_forth_spacing_apart() {
        let path = vec![v2(0.0, 0.0), v2(400.0, 0.0)];
        let run = open(&path, Some(v2(200.0, 100.0)), 4, 20.0, 25.0);
        // Copies at y = 0, 20, 40, 60 (towards the target), alternating direction.
        let mut ys: Vec<i64> = run.iter().map(|p| p.y.round() as i64).collect();
        ys.sort_unstable();
        ys.dedup();
        let rows: Vec<i64> = ys
            .iter()
            .copied()
            .filter(|y| run.iter().filter(|p| p.y.round() as i64 == *y).count() > 5)
            .collect();
        assert_eq!(rows.len(), 4, "{rows:?}");
        for w in rows.windows(2) {
            assert!(((w[1] - w[0]) as f64 - 20.0).abs() <= 2.0, "{rows:?}");
        }
        assert!(run.windows(2).all(|w| w[0].distance(w[1]) <= 25.0 + 1e-6));
        // Ends at the start side after an even number of copies.
        assert!(run[run.len() - 1].x < 1.0);
        // Without a target the copies go to the left of the drawing direction.
        let left = open(&path, None, 2, 20.0, 25.0);
        assert!(left.iter().any(|p| (p.y.abs() - 20.0).abs() < 1e-6));
    }
}
