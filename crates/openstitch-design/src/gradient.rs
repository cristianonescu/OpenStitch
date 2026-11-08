//! Colour blends and gradient fills (#532, beyond PE-DESIGN).
//!
//! A gradient is sewn as one fill object per colour stop. Every one of them generates the same
//! tatami rows (the shared stitch grid: same shape, angle, density, stitch length and stagger)
//! and keeps only the stitches it owns. Each stitch has exactly one owner: the gradient gives
//! the stops' weights where the stitch lies (they add up to 1), and a dither threshold `r` in
//! 0…1 picks the stop whose running total first passes `r`. The colours together therefore
//! cover the shape as one even layer, however the gradient runs.
//!
//! - **Rows** dither: `r` is the same along a whole row (a golden-ratio sequence by row), so a
//!   colour with weight w takes about every 1/w-th row: a two-colour blend's row spacings are
//!   complementary.
//! - **Ordered**: a 4 × 4 Bayer matrix by row and stitch column (an even, woven look).
//! - **Noise**: a seeded hash by row and column (random, reproducible).
//! - **Bands**: `r` = 0.5, hard edges halfway between stops.
//!
//! Where a colour's weight would leave its rows more than 5 mm apart, it gives that share to
//! the colour next to it: sparse rows would only show as stray lines.
//!
//! Lengths are 0.1 mm.

use crate::generate::Run;
use crate::geometry::Vec2;
use crate::model::{Gradient, GradientCurve, GradientDither, GradientKind};

/// Rows further apart than this (0.1 mm) are not sewn in one colour: its share goes to the
/// neighbouring colour.
pub const SPARSEST_ROWS: f64 = 50.0;

/// Where `p` lies along the gradient, 0…1 (before the curve).
pub fn position(g: &Gradient, p: Vec2) -> f64 {
    let d = g.to - g.from;
    let len2 = d.x * d.x + d.y * d.y;
    if len2 <= 0.0 {
        return 0.0;
    }
    let t = match g.kind {
        GradientKind::Linear => ((p.x - g.from.x) * d.x + (p.y - g.from.y) * d.y) / len2,
        GradientKind::Radial => p.distance(g.from) / len2.sqrt(),
    };
    t.clamp(0.0, 1.0)
}

/// The transition curve applied to a position.
pub fn curve(c: GradientCurve, t: f64) -> f64 {
    match c {
        GradientCurve::Linear => t,
        GradientCurve::EaseIn => t * t,
        GradientCurve::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
        GradientCurve::SCurve => t * t * (3.0 - 2.0 * t),
    }
}

/// The stops' weights at position `t` (after the curve): 1 at a stop, shared linearly between
/// the two stops around `t`. `spacing` is the row spacing: a weight that would leave rows more
/// than [`SPARSEST_ROWS`] apart goes to the other stop.
pub fn weights(stops: &[f64], t: f64, spacing: f64) -> Vec<f64> {
    let n = stops.len();
    let mut w = vec![0.0; n];
    if n == 0 {
        return w;
    }
    if t <= stops[0] {
        w[0] = 1.0;
        return w;
    }
    if t >= stops[n - 1] {
        w[n - 1] = 1.0;
        return w;
    }
    let j = (0..n - 1)
        .find(|&j| t >= stops[j] && t < stops[j + 1])
        .unwrap_or(n - 2);
    let span = stops[j + 1] - stops[j];
    let u = if span > 0.0 {
        (t - stops[j]) / span
    } else {
        1.0
    };
    let least = (spacing / SPARSEST_ROWS).min(0.5);
    let u = if u < least {
        0.0
    } else if 1.0 - u < least {
        1.0
    } else {
        u
    };
    w[j] = 1.0 - u;
    w[j + 1] = u;
    w
}

/// The dither threshold of the stitch in row `row`, stitch column `col`, in 0…1.
pub fn threshold(d: GradientDither, seed: u64, row: i64, col: i64) -> f64 {
    match d {
        GradientDither::Rows => (row as f64 * 0.618_033_988_749_894_9).rem_euclid(1.0),
        GradientDither::Ordered => {
            const BAYER: [[u8; 4]; 4] =
                [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
            let (i, j) = (row.rem_euclid(4) as usize, col.rem_euclid(4) as usize);
            (f64::from(BAYER[i][j]) + 0.5) / 16.0
        }
        GradientDither::Noise => {
            let mut z = seed
                ^ (row as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (col as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            (z >> 11) as f64 / (1u64 << 53) as f64
        }
        GradientDither::Bands => 0.5,
    }
}

/// The stop that owns a stitch with weights `w` and threshold `r`.
pub fn owner(w: &[f64], r: f64) -> usize {
    let mut total = 0.0;
    for (k, &x) in w.iter().enumerate() {
        total += x;
        if r < total {
            return k;
        }
    }
    w.iter().rposition(|&x| x > 0.0).unwrap_or(0)
}

/// The pieces of the shared rows `rows` (tatami rows at geometry angle `angle`, `spacing`
/// apart, `stitch` long) that gradient layer `g.layer` sews: runs of consecutive stitches it
/// owns, each stitch judged at its middle.
pub fn layer_pieces(rows: &[Run], g: &Gradient, angle: f64, spacing: f64, stitch: f64) -> Vec<Run> {
    let rad = -angle.to_radians();
    let mut out = Vec::new();
    for run in rows {
        let mut piece: Run = Vec::new();
        for w in run.windows(2) {
            let mid = w[0].lerp(w[1], 0.5);
            let q = mid.rotated(rad);
            let row = (q.y / spacing.max(1e-9)).round() as i64;
            let col = (q.x / stitch.max(1e-9)).floor() as i64;
            let t = curve(g.curve, position(g, mid));
            let mine = owner(
                &weights(&g.stops, t, spacing),
                threshold(g.dither, g.seed, row, col),
            ) == g.layer;
            if mine {
                if piece.is_empty() {
                    piece.push(w[0]);
                }
                piece.push(w[1]);
            } else if !piece.is_empty() {
                out.push(std::mem::take(&mut piece));
            }
        }
        if piece.len() >= 2 {
            out.push(piece);
        }
    }
    out
}
