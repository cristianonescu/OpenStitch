//! Flow fill from the picture (#538, beyond PE-DESIGN, Auto Punch's OpenStitch mode only): each
//! fill region's rows follow the direction of the picture's own strokes, fur or petals.
//!
//! - The direction comes from the picture's **structure tensor**: the products of the
//!   luminance gradient, smoothed over [`SMOOTH_MM`] inside each region. Its major axis is the
//!   gradient's (across an edge or stroke); the rows run along the minor one, along the strokes.
//! - Per region the tensor is summed over the region's own pixels on a coarse grid of
//!   [`CELL_MM`] cells: the field the rows follow is one angle per cell
//!   ([`openstitch_design::flow::FlowField::Grid`]), not per pixel.
//! - Only the picture's strokes inside a region count, not its outline (#894): pixels within
//!   [`RIM_MM`] of the region's edge are left out, since there the gradient is the colour
//!   change to the next region and would bend the rows along the outline, and the smoothing is
//!   done over the region's own cells, so a strong outline does not bleed inwards. A cell is
//!   **textured** when at least [`MIN_CELL_SHARE`] of it is counted, its mean gradient is at
//!   least [`MIN_STROKE`] luminance per mm (JPEG noise and gentle shading are weaker) and its
//!   coherence is at least [`MIN_COHERENCE`]; any other cell keeps the region's fill angle.
//! - A region with fewer than [`MIN_TEXTURED`] of its cells textured has no strokes to follow
//!   (a flat logo shape, a noisy photo area): it keeps straight rows, no field at all.
//! - Measured on the ten Auto Punch gallery cases (#894): with these rules no case scores below
//!   its straight-row result in dE00 or MS-SSIM; flat logos keep straight rows.
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
/// The tensor's smoothing inside a region (the Gaussian's sigma over its cells), mm.
pub const SMOOTH_MM: f64 = 1.5;
/// A cell whose direction is more mixed than this keeps the region's fill angle:
/// `(λ1 - λ2) / (λ1 + λ2)` of the cell's summed tensor.
pub const MIN_COHERENCE: f64 = 0.4;
/// Pixels this close to a region's edge are left out of its field, mm.
pub const RIM_MM: f64 = 0.5;
/// The share of a cell that must be counted (inside the region, past the rim) for it to steer.
pub const MIN_CELL_SHARE: f64 = 0.25;
/// The weakest mean gradient a textured cell has, luminance (0-255) per mm.
pub const MIN_STROKE: f64 = 50.0;
/// The share of a region's counted cells that must be textured for it to get a field.
pub const MIN_TEXTURED: f64 = 0.3;

/// The picture's structure tensor, per pixel: `(Jxx, Jxy, Jyy)`, unsmoothed (each region
/// smooths its own part).
pub struct Tensor {
    w: usize,
    h: usize,
    /// In (luminance per mm)².
    j: Vec<[f64; 3]>,
    mm_per_px: f64,
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
        let per_mm = if mm_per_px > 0.0 {
            1.0 / mm_per_px
        } else {
            1.0
        };
        for y in 0..h {
            for x in 0..w {
                let gx = (at((x + 1).min(w - 1), y) - at(x.saturating_sub(1), y)) / 2.0 * per_mm;
                let gy = (at(x, (y + 1).min(h - 1)) - at(x, y.saturating_sub(1))) / 2.0 * per_mm;
                j[y * w + x] = [gx * gx, gx * gy, gy * gy];
            }
        }
        Tensor { w, h, j, mm_per_px }
    }

    /// The field of a region: `mask` is the region's pixels in the `mw` × `mh` box at `offset`
    /// of the picture; `fill_deg` (a stored region angle) is kept where the picture has no clear
    /// direction. The grid's cells are [`CELL_MM`] square, placed by `frame`. `None` where the
    /// region has too few textured cells: it keeps straight rows.
    pub fn region_field(
        &self,
        mask: &[bool],
        mw: usize,
        mh: usize,
        offset: (usize, usize),
        frame: &Frame,
        fill_deg: f64,
    ) -> Option<FlowField> {
        let mm = frame.mm_per_px();
        let cell_px = if mm > 0.0 {
            (CELL_MM / mm).max(1.0)
        } else {
            1.0
        };
        let cols = ((mw as f64 / cell_px).ceil() as usize).max(1);
        let rows = ((mh as f64 / cell_px).ceil() as usize).max(1);
        // Pixels at least `rim` from the region's edge (chessboard distance).
        let rim = if self.mm_per_px > 0.0 {
            (RIM_MM / self.mm_per_px).round() as usize
        } else {
            0
        };
        let depth = depth(mask, mw, mh, rim);
        let mut sums = vec![[0.0; 3]; cols * rows];
        let mut counts = vec![0usize; cols * rows];
        for y in 0..mh {
            for x in 0..mw {
                if !mask[y * mw + x] || depth[y * mw + x] < rim {
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
                counts[c] += 1;
            }
        }
        let (sums, pixels) = smooth_cells(&sums, &counts, cols, rows, SMOOTH_MM / CELL_MM);
        let full = (cell_px * cell_px * MIN_CELL_SHARE).max(1.0) as usize;
        let min_energy = MIN_STROKE * MIN_STROKE;
        let (mut counted, mut textured) = (0usize, 0usize);
        let angles: Vec<f64> = sums
            .iter()
            .zip(&counts)
            .zip(&pixels)
            .map(|((&[xx, xy, yy], &n), &weight)| {
                if n < full || weight <= 0.0 {
                    return fill_deg;
                }
                counted += 1;
                let energy = (xx + yy) / weight;
                match row_angle(xx, xy, yy) {
                    Some(a) if energy >= min_energy => {
                        textured += 1;
                        a
                    }
                    _ => fill_deg,
                }
            })
            .collect();
        if textured == 0 || (textured as f64) < MIN_TEXTURED * counted as f64 {
            return None;
        }
        Some(FlowField::Grid {
            origin: frame.point(offset.0 as f64, offset.1 as f64),
            cell: cell_px * frame.scale,
            cols,
            angles,
        })
    }
}

/// Each pixel's chessboard distance to the nearest pixel outside `mask` (or the box's edge),
/// counted up to `cap` (more is reported as `cap`).
fn depth(mask: &[bool], w: usize, h: usize, cap: usize) -> Vec<usize> {
    let mut d: Vec<usize> = mask.iter().map(|&m| if m { cap } else { 0 }).collect();
    if cap == 0 {
        return d;
    }
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if d[i] == 0 {
                continue;
            }
            let edge = x == 0 || y == 0 || x == w - 1 || y == h - 1;
            let mut v = if edge { 1 } else { d[i] };
            if !edge {
                for (dx, dy) in [(-1i64, 0i64), (-1, -1), (0, -1), (1, -1)] {
                    let n = (y as i64 + dy) as usize * w + (x as i64 + dx) as usize;
                    v = v.min(d[n] + 1);
                }
            }
            d[i] = v;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if d[i] == 0 || x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                continue;
            }
            let mut v = d[i];
            for (dx, dy) in [(1i64, 0i64), (1, 1), (0, 1), (-1, 1)] {
                let n = (y as i64 + dy) as usize * w + (x as i64 + dx) as usize;
                v = v.min(d[n] + 1);
            }
            d[i] = v;
        }
    }
    d
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

/// The cells' tensor sums and pixel counts blurred by a Gaussian of `sigma` cells (two passes),
/// over the region's own cells only: a cell outside it adds nothing.
fn smooth_cells(
    sums: &[[f64; 3]],
    counts: &[usize],
    cols: usize,
    rows: usize,
    sigma: f64,
) -> (Vec<[f64; 3]>, Vec<f64>) {
    let r = (3.0 * sigma).ceil() as isize;
    let kernel: Vec<f64> = (-r..=r)
        .map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).dexp())
        .collect();
    // Four planes: the three products and the pixel count.
    let planes: Vec<[f64; 4]> = sums
        .iter()
        .zip(counts)
        .map(|(t, &n)| [t[0], t[1], t[2], n as f64])
        .collect();
    let pass = |src: &[[f64; 4]], horizontal: bool| -> Vec<[f64; 4]> {
        let mut out = vec![[0.0; 4]; cols * rows];
        for y in 0..rows {
            for x in 0..cols {
                let mut acc = [0.0; 4];
                for (k, wt) in kernel.iter().enumerate() {
                    let d = k as isize - r;
                    let (sx, sy) = if horizontal {
                        (x as isize + d, y as isize)
                    } else {
                        (x as isize, y as isize + d)
                    };
                    if sx < 0 || sy < 0 || sx >= cols as isize || sy >= rows as isize {
                        continue;
                    }
                    let v = src[sy as usize * cols + sx as usize];
                    for c in 0..4 {
                        acc[c] += v[c] * wt;
                    }
                }
                out[y * cols + x] = acc;
            }
        }
        out
    };
    let blurred = pass(&pass(&planes, true), false);
    (
        blurred.iter().map(|v| [v[0], v[1], v[2]]).collect(),
        blurred.iter().map(|v| v[3]).collect(),
    )
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
        let field = t
            .region_field(&mask, 400, 400, (0, 0), &frame, 0.0)
            .expect("a field");
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
    fn a_flat_shape_keeps_straight_rows_though_its_outline_has_an_edge() {
        // A solid dark disc 30 mm across on white: only its outline has a gradient.
        let disc = picture(|x, y| if x.dhypot(y) < 150.0 { 0.0 } else { 1.0 });
        let frame = Frame::new(0.0, 0.0, 40.0, 400);
        let t = Tensor::of(&disc, frame.mm_per_px());
        let mask: Vec<bool> = (0..400 * 400)
            .map(|i| ((i % 400) as f64 - 200.0).dhypot((i / 400) as f64 - 200.0) < 150.0)
            .collect();
        assert!(
            t.region_field(&mask, 400, 400, (0, 0), &frame, 30.0)
                .is_none()
        );
    }

    #[test]
    fn a_flat_picture_keeps_straight_rows() {
        let flat = picture(|_, _| 0.5);
        let frame = Frame::new(0.0, 0.0, 40.0, 400);
        let t = Tensor::of(&flat, frame.mm_per_px());
        let mask = vec![true; 400 * 400];
        assert!(
            t.region_field(&mask, 400, 400, (0, 0), &frame, 30.0)
                .is_none()
        );
    }
}
