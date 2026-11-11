//! Tartan (plaid) fill (#539, beyond PE-DESIGN).
//!
//! A tartan is sewn as one fill object per colour of its sett, like a gradient fill. Every one
//! of them lays out the same two families of rows, each at half the fill's density so together
//! they cover like one fill:
//!
//! - **warp** rows run along the vertical stripes (across the fill direction), and
//! - **weft** rows along the horizontal ones (along the fill direction),
//!
//! in the fill's direction frame, starting at the shape's corner. The sett repeats across the
//! shape both ways; a symmetric sett is mirrored about its first and last stripes (R8 G24 W4
//! sews R8 G24 W4 G24, R8 …). Each object keeps the rows inside its own colour's stripes, warp
//! first, then weft; where a warp stripe crosses a weft stripe their colours mix as in a woven
//! plaid. Lengths are 0.1 mm.

use crate::generate::{self, RegionParams, Run, Stagger};
use crate::geometry::{Contour, Rect, Vec2};
use crate::model::Tartan;

/// The repeat of `t`: (colour index, width in 0.1 mm), mirrored when symmetric.
pub fn repeat(t: &Tartan) -> Vec<(usize, f64)> {
    let mut out: Vec<(usize, f64)> = t
        .sett
        .iter()
        .map(|s| (s.color, (s.width * 10.0).max(1.0)))
        .collect();
    if t.symmetric && out.len() >= 3 {
        let back: Vec<(usize, f64)> = out[1..out.len() - 1].iter().rev().copied().collect();
        out.extend(back);
    }
    out
}

/// The colour index of the stripe at `pos` (0.1 mm from the start of the repeat).
pub fn colour_at(repeat: &[(usize, f64)], pos: f64) -> usize {
    let total: f64 = repeat.iter().map(|s| s.1).sum();
    if repeat.is_empty() || total <= 0.0 {
        return 0;
    }
    let mut p = pos.rem_euclid(total);
    for &(c, w) in repeat {
        if p < w {
            return c;
        }
        p -= w;
    }
    repeat[repeat.len() - 1].0
}

/// The warp and weft rows colour `t.layer` sews on `closed` minus `holes`, rows at geometry
/// angle `angle` (weft) and square to it (warp), each family `2 × spacing` apart.
pub fn layers(
    closed: &[Contour],
    holes: &[Contour],
    t: &Tartan,
    angle: f64,
    spacing: f64,
    stitch: f64,
    stagger: Stagger,
) -> [Vec<Run>; 2] {
    let rep = repeat(t);
    let rad = -angle.to_radians();
    let Some(b) = Rect::enclosing(
        closed
            .iter()
            .flat_map(Contour::outline_points)
            .map(|p| p.rotated(rad)),
    ) else {
        return [Vec::new(), Vec::new()];
    };
    let family = |row_angle: f64, across: fn(Vec2, &Rect) -> f64| -> Vec<Run> {
        let p = RegionParams {
            angle: row_angle,
            row_spacing: spacing * 2.0,
            stitch_length: stitch,
            pull_compensation: 0.0,
        };
        // Rows may come joined into longer runs: each stitch is judged at its middle, and runs
        // of consecutive stitches in this colour's stripes are kept.
        let mut out: Vec<Run> = Vec::new();
        for run in generate::fill_shifted(closed, &p, holes, stagger) {
            let mut piece: Run = Vec::new();
            for w in run.windows(2) {
                let mid = w[0].lerp(w[1], 0.5).rotated(rad);
                if colour_at(&rep, across(mid, &b)) == t.layer {
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
    };
    // Warp rows run along the vertical stripes: their colour is where they lie across x.
    let warp = family(angle + 90.0, |q, b| q.x - b.min_x);
    // Weft rows run along the horizontal stripes: their colour is where they lie across y.
    let weft = family(angle, |q, b| q.y - b.min_y);
    [warp, weft]
}
