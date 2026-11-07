//! The motif band (#544, beyond PE-DESIGN): a motif repeated along a satin column instead of
//! the satin, each copy stretched across the column's local width, so it narrows and widens
//! with a tapered column. Skeleton satin and Manual Punch regions and satin lines can sew it;
//! off unless chosen.

use crate::generate::Run;
use crate::{Vec2, v2};
use serde::{Deserialize, Serialize};

/// The motif band of a column.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotifBand {
    /// The motif's id (a bundled or a user motif, `patterns.list`).
    pub motif: String,
    /// Each copy's length along the column, mm; `None`: the column's mean width, so the
    /// copies stay about square.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile: Option<f64>,
    /// Every other copy mirrored across the column.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirror: bool,
}

/// The shortest copy along the column, 0.1 mm.
const MIN_TILE: f64 = 10.0;

/// The column `rungs` ([left, right] in sewing order, 0.1 mm) sewn as copies of the motif
/// `unit` (its paths in the motif's own units: x along, y across) end to end, a whole number of
/// them stretched to fill the column, each `tile` long (0.1 mm; `None`: the mean width). The
/// motif's extent across is mapped onto the column from edge to edge at every point, so it
/// follows the local width. Stitches at most `pitch` long; empty for fewer than two rungs.
pub fn sew(
    rungs: &[[Vec2; 2]],
    unit: &[Vec<Vec2>],
    tile: Option<f64>,
    mirror: bool,
    pitch: f64,
) -> Run {
    let points: Vec<Vec2> = unit.iter().flatten().copied().collect();
    if rungs.len() < 2 || points.len() < 2 {
        return Vec::new();
    }
    let col = Column::new(rungs);
    let total = col.length();
    if total <= 0.0 {
        return Vec::new();
    }
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in &points {
        x0 = x0.min(p.x);
        x1 = x1.max(p.x);
        y0 = y0.min(p.y);
        y1 = y1.max(p.y);
    }
    let along = |x: f64| {
        if x1 - x0 > 1e-9 {
            (x - x0) / (x1 - x0)
        } else {
            0.5
        }
    };
    let across = |y: f64| {
        if y1 - y0 > 1e-9 {
            (y - y0) / (y1 - y0)
        } else {
            0.5
        }
    };
    let want = tile.unwrap_or_else(|| col.mean_width()).max(MIN_TILE);
    let n = (total / want).round().max(1.0);
    let len = total / n;
    let pitch = pitch.max(1.0);
    let mut run: Run = Vec::new();
    for k in 0..n as usize {
        let start = k as f64 * len;
        let flip = mirror && k % 2 == 1;
        let uv: Vec<(f64, f64)> = points
            .iter()
            .map(|p| {
                let v = across(p.y);
                (start + along(p.x) * len, if flip { 1.0 - v } else { v })
            })
            .collect();
        for w in uv.windows(2) {
            let (a, b) = (col.at(w[0].0, w[0].1), col.at(w[1].0, w[1].1));
            let steps = (a.distance(b) / pitch).ceil().max(1.0) as usize;
            if run.is_empty() {
                run.push(a);
            }
            for j in 1..=steps {
                let t = j as f64 / steps as f64;
                run.push(col.at(
                    w[0].0 + (w[1].0 - w[0].0) * t,
                    w[0].1 + (w[1].1 - w[0].1) * t,
                ));
            }
        }
    }
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
    crate::generate::drop_close(&run, crate::generate::MOTIF_MIN_STITCH, pitch)
}

/// A column measured along its centre line.
struct Column<'a> {
    rungs: &'a [[Vec2; 2]],
    /// The distance along the centre line to each rung.
    at: Vec<f64>,
}

impl<'a> Column<'a> {
    fn new(rungs: &'a [[Vec2; 2]]) -> Self {
        let mut at = vec![0.0];
        for w in rungs.windows(2) {
            let (a, b) = (mid(w[0]), mid(w[1]));
            at.push(at[at.len() - 1] + a.distance(b));
        }
        Column { rungs, at }
    }

    fn length(&self) -> f64 {
        self.at[self.at.len() - 1]
    }

    fn mean_width(&self) -> f64 {
        self.rungs.iter().map(|r| r[0].distance(r[1])).sum::<f64>() / self.rungs.len() as f64
    }

    /// The point `s` along the centre line and `v` across, 0 the left edge and 1 the right.
    fn at(&self, s: f64, v: f64) -> Vec2 {
        let s = s.clamp(0.0, self.length());
        let j = self
            .at
            .partition_point(|&a| a <= s)
            .clamp(1, self.at.len() - 1);
        let (a, b) = (self.at[j - 1], self.at[j]);
        let t = if b - a > 1e-12 {
            (s - a) / (b - a)
        } else {
            0.0
        };
        let (r0, r1) = (self.rungs[j - 1], self.rungs[j]);
        let l = r0[0].lerp(r1[0], t);
        let r = r0[1].lerp(r1[1], t);
        l.lerp(r, v)
    }
}

fn mid(r: [Vec2; 2]) -> Vec2 {
    v2((r[0].x + r[1].x) / 2.0, (r[0].y + r[1].y) / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A straight column along x, 60 mm long, tapering from 2 mm to 8 mm wide.
    fn taper() -> Vec<[Vec2; 2]> {
        (0..=60)
            .map(|k| {
                let x = k as f64 * 10.0;
                let h = width(x) / 2.0;
                [v2(x, -h), v2(x, h)]
            })
            .collect()
    }

    fn width(x: f64) -> f64 {
        20.0 + 60.0 * x / 600.0
    }

    fn zigzag() -> Vec<Vec<Vec2>> {
        vec![vec![
            v2(0.0, 0.0),
            v2(0.25, 1.0),
            v2(0.75, -1.0),
            v2(1.0, 0.0),
        ]]
    }

    #[test]
    fn the_motif_follows_the_local_width_of_a_tapered_column() {
        let run = sew(&taper(), &zigzag(), Some(60.0), false, 25.0);
        assert!(run.len() > 20, "{}", run.len());
        // Every needle point inside the column, 0.2 mm at most past its local edge.
        for p in &run {
            assert!(p.y.abs() <= width(p.x) / 2.0 + 2.0, "{p:?}");
        }
        // Each copy reaches both edges where its peaks are: the band follows the width.
        let n = 10;
        for k in 0..n {
            let peak = (k as f64 + 0.25) * 60.0;
            let dip = (k as f64 + 0.75) * 60.0;
            let near = |x: f64, y: f64| run.iter().any(|p| p.distance(v2(x, y)) <= 2.0);
            assert!(near(peak, width(peak) / 2.0), "copy {k}: peak at {peak}");
            assert!(near(dip, -width(dip) / 2.0), "copy {k}: dip at {dip}");
        }
    }

    #[test]
    fn copies_fill_the_column_and_mirror_alternately() {
        let plain = sew(&taper(), &zigzag(), None, false, 25.0);
        let first = *plain.first().expect("start");
        let last = *plain.last().expect("end");
        assert!(
            first.x.abs() < 1e-6 && last.x > 600.0 - 1e-6,
            "{first:?} {last:?}"
        );
        let mirrored = sew(&taper(), &zigzag(), Some(60.0), true, 25.0);
        // The second copy's peak is on the other edge.
        let peak = 1.25 * 60.0;
        assert!(
            mirrored
                .iter()
                .any(|p| p.distance(v2(peak, -width(peak) / 2.0)) <= 2.0)
        );
        assert!(sew(&taper()[..1], &zigzag(), None, false, 25.0).is_empty());
    }
}
