//! Flow fill from the picture (#538, beyond PE-DESIGN, Auto Punch's OpenStitch mode only): each
//! fill region's rows follow the direction of the picture's own strokes, fur or petals.
//!
//! - The direction comes from the picture's **structure tensor**: the products of the
//!   luminance gradient, smoothed over [`SMOOTH_MM`]. Its major axis is the gradient's (across
//!   an edge or stroke); the rows run along the minor one, along the strokes.
//! - Per region the tensor is summed over the region's own pixels on a coarse grid of
//!   [`CELL_MM`] cells: the field the rows follow is one angle per cell
//!   ([`openstitch_design::flow::FlowField::Grid`]), not per pixel. A cell whose direction is
//!   weak or mixed (coherence under [`MIN_COHERENCE`]) keeps the region's own fill angle.
//! - The rows themselves are `openstitch_design::flow`'s evenly spaced streamlines, sewn back and
//!   forth inside the region; nothing here decides a move between rows or regions.
//!
//! The owner decided on 2026-11-08 that this may be built despite Brother's US 7,996,103
//! (docs/research/beyond/patents.md): it computes no connector test (coverage, colour
//! difference or crossing angle), which is the patent claim's third element.

use crate::raster::{Frame, Raster};
use openstitch_design::flow::FlowField;
use openstitch_design::fmath::Det;

/// The field's cells, mm.
pub const CELL_MM: f64 = 1.0;
/// The tensor's smoothing (the Gaussian's sigma), mm.
pub const SMOOTH_MM: f64 = 1.5;
/// A cell whose direction is weaker or more mixed than this keeps the region's fill angle:
/// `(λ1 - λ2) / (λ1 + λ2)` of the cell's summed tensor.
pub const MIN_COHERENCE: f64 = 0.25;

/// The picture's smoothed structure tensor, per pixel: `(Jxx, Jxy, Jyy)`.
pub struct Tensor {
    w: usize,
    h: usize,
    j: Vec<[f64; 3]>,
}

impl Tensor {
    /// The tensor of `image` for pixels `mm_per_px` wide.
    pub fn of(image: &Raster, mm_per_px: f64) -> Tensor {
        let (w, h) = (image.width() as usize, image.height() as usize);
        let lum: Vec<f64> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let [r, g, b, a] = image.pixel(x as u32, y as u32);
                // Transparent pixels read as white, as the conversion treats them.
                let t = f64::from(a) / 255.0;
                let l = 0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b);
                l * t + 255.0 * (1.0 - t)
            })
            .collect();
        let at = |x: usize, y: usize| lum[y * w + x];
        let mut j = vec![[0.0; 3]; w * h];
        for y in 0..h {
            for x in 0..w {
                let gx = (at((x + 1).min(w - 1), y) - at(x.saturating_sub(1), y)) / 2.0;
                let gy = (at(x, (y + 1).min(h - 1)) - at(x, y.saturating_sub(1))) / 2.0;
                j[y * w + x] = [gx * gx, gx * gy, gy * gy];
            }
        }
        let sigma = if mm_per_px > 0.0 {
            (SMOOTH_MM / mm_per_px).max(0.5)
        } else {
            1.0
        };
        Tensor {
            w,
            h,
            j: blur(&j, w, h, sigma),
        }
    }

    /// The field of a region: `mask` is the region's pixels in the `mw` × `mh` box at `offset`
    /// of the picture; `fill_deg` (a stored region angle) is kept where the picture has no clear
    /// direction. The grid's cells are [`CELL_MM`] square, placed by `frame`.
    pub fn region_field(
        &self,
        mask: &[bool],
        mw: usize,
        mh: usize,
        offset: (usize, usize),
        frame: &Frame,
        fill_deg: f64,
    ) -> FlowField {
        let mm = frame.mm_per_px();
        let cell_px = if mm > 0.0 {
            (CELL_MM / mm).max(1.0)
        } else {
            1.0
        };
        let cols = ((mw as f64 / cell_px).ceil() as usize).max(1);
        let rows = ((mh as f64 / cell_px).ceil() as usize).max(1);
        let mut sums = vec![[0.0; 3]; cols * rows];
        for y in 0..mh {
            for x in 0..mw {
                if !mask[y * mw + x] {
                    continue;
                }
                let (px, py) = (x + offset.0, y + offset.1);
                if px >= self.w || py >= self.h {
                    continue;
                }
                let c = ((y as f64 / cell_px) as usize).min(rows - 1) * cols
                    + ((x as f64 / cell_px) as usize).min(cols - 1);
                let t = self.j[py * self.w + px];
                for k in 0..3 {
                    sums[c][k] += t[k];
                }
            }
        }
        let angles = sums
            .iter()
            .map(|&[xx, xy, yy]| row_angle(xx, xy, yy).unwrap_or(fill_deg))
            .collect();
        FlowField::Grid {
            origin: frame.point(offset.0 as f64, offset.1 as f64),
            cell: cell_px * frame.scale,
            cols,
            angles,
        }
    }
}

/// The stored row angle (degrees counter-clockwise on screen, 0 ≤ a < 180) of a summed tensor:
/// along its minor axis; `None` where the direction is weak or mixed.
pub fn row_angle(xx: f64, xy: f64, yy: f64) -> Option<f64> {
    let trace = xx + yy;
    if trace <= 1e-9 {
        return None;
    }
    let spread = ((xx - yy) * (xx - yy) + 4.0 * xy * xy).sqrt();
    if spread / trace < MIN_COHERENCE {
        return None;
    }
    // The gradient's axis (design y down), then a right angle on: along the strokes.
    let across = 0.5 * (2.0 * xy).datan2(xx - yy);
    let along = across.to_degrees() + 90.0;
    // Stored angles turn the other way (`openstitch_design::region::geometry_angle`).
    Some((-along).rem_euclid(180.0))
}

/// Each of the tensor's three planes blurred by a Gaussian of `sigma` pixels (two passes).
fn blur(j: &[[f64; 3]], w: usize, h: usize, sigma: f64) -> Vec<[f64; 3]> {
    let r = (3.0 * sigma).ceil() as isize;
    let kernel: Vec<f64> = (-r..=r)
        .map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).dexp())
        .collect();
    let total: f64 = kernel.iter().sum();
    let kernel: Vec<f64> = kernel.iter().map(|k| k / total).collect();
    let pass = |src: &[[f64; 3]], horizontal: bool| -> Vec<[f64; 3]> {
        let mut out = vec![[0.0; 3]; w * h];
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0; 3];
                for (k, wt) in kernel.iter().enumerate() {
                    let d = k as isize - r;
                    let (sx, sy) = if horizontal {
                        ((x as isize + d).clamp(0, w as isize - 1) as usize, y)
                    } else {
                        (x, (y as isize + d).clamp(0, h as isize - 1) as usize)
                    };
                    let v = src[sy * w + sx];
                    for c in 0..3 {
                        acc[c] += v[c] * wt;
                    }
                }
                out[y * w + x] = acc;
            }
        }
        out
    };
    pass(&pass(j, true), false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_design::flow::FlowField;
    use openstitch_design::v2;

    /// A 400 × 400 picture (0.1 mm pixels, 40 mm) of stripes `stripe(x, y)` (0…1) round the
    /// centre.
    fn picture(stripe: impl Fn(f64, f64) -> f64) -> Raster {
        Raster::from_fn(400, 400, |x, y| {
            let v = (40.0 + 180.0 * stripe(f64::from(x) - 200.0, f64::from(y) - 200.0)) as u8;
            [v, v, v, 255]
        })
        .expect("size")
    }

    /// Mean angle (degrees) between the region field's cells and `truth` (a geometry direction
    /// at a point, design units) over a ring 6-18 mm round the centre.
    fn error(image: &Raster, truth: impl Fn(f64, f64) -> (f64, f64)) -> f64 {
        let frame = Frame::new(0.0, 0.0, 40.0, 400);
        let t = Tensor::of(image, frame.mm_per_px());
        let ring = |x: usize, y: usize| {
            let r = ((x as f64 - 200.0).powi(2) + (y as f64 - 200.0).powi(2)).sqrt();
            (60.0..=180.0).contains(&r)
        };
        let mask: Vec<bool> = (0..400 * 400).map(|i| ring(i % 400, i / 400)).collect();
        let field = t.region_field(&mask, 400, 400, (0, 0), &frame, 0.0);
        let FlowField::Grid {
            cols, ref angles, ..
        } = field
        else {
            panic!("a grid")
        };
        assert_eq!(cols, 40, "1 mm cells over 40 mm");
        let (mut sum, mut n) = (0.0, 0.0);
        for (i, a) in angles.iter().enumerate() {
            let (cx, cy) = (
                (i % cols) as f64 * 10.0 + 5.0,
                (i / cols) as f64 * 10.0 + 5.0,
            );
            if !ring(cx as usize, cy as usize) {
                continue;
            }
            let d = field.direction(v2(cx, cy)).expect("a direction");
            let (tx, ty) = truth(cx - 200.0, cy - 200.0);
            let l = tx.dhypot(ty);
            let cos = ((d.x * tx + d.y * ty) / l).abs().min(1.0);
            sum += cos.dacos().to_degrees();
            n += 1.0;
            assert!(a.is_finite());
        }
        assert!(n > 300.0, "{n} cells");
        sum / n
    }

    #[test]
    fn rings_give_a_swirl_and_spokes_a_radial_field() {
        // Rings 2 mm apart: the strokes circle the centre, so the rows do (a swirl).
        let rings = picture(|x, y| 0.5 + 0.5 * (x.dhypot(y) / 20.0 * std::f64::consts::TAU).dsin());
        let swirl = error(&rings, |x, y| (-y, x));
        assert!(swirl < 15.0, "swirl: mean error {swirl:.2}°");
        // 36 spokes: the strokes radiate, so the rows do.
        let spokes = picture(|x, y| 0.5 + 0.5 * (y.datan2(x) * 36.0).dsin());
        let radial = error(&spokes, |x, y| (x, y));
        assert!(radial < 15.0, "radial: mean error {radial:.2}°");
    }

    #[test]
    fn a_flat_picture_keeps_the_fill_angle() {
        let flat = picture(|_, _| 0.5);
        let frame = Frame::new(0.0, 0.0, 40.0, 400);
        let t = Tensor::of(&flat, frame.mm_per_px());
        let field = t.region_field(&vec![true; 400 * 400], 400, 400, (0, 0), &frame, 30.0);
        let FlowField::Grid { angles, .. } = field else {
            panic!("a grid")
        };
        assert!(angles.iter().all(|a| *a == 30.0));
    }
}
