//! Pixel art's own grid (#994): whether an image is pixel art, a regular grid of flat-colour
//! squares scaled up, and where that grid lies, so Cross Stitch in OpenStitch mode can lay one
//! cross per art pixel (or per a whole number of them) instead of its own grid.
//!
//! The method is this project's own. Colour edges (neighbouring pixels more than
//! [`EDGE_STEP`] apart in summed RGB) are counted per column and per row. Pixel art puts
//! nearly all of them on the lines between its art pixels, so for each axis the detector tries
//! every pitch from [`MIN_PITCH`] pixels up to an eighth of the side, in steps of 0.05 pixel,
//! finds the phase whose window of lines (± [`LINE_TOLERANCE`] of the pitch, at least one
//! pixel) holds the most edges, and scores the share beyond what a window that wide would
//! hold by chance. The largest pitch within 7 % of the best score wins (half the pitch scores
//! as well, since every line of the true grid is also one of its own); the lines found near
//! the predicted ones then refine pitch and phase by least squares. The image is pixel art
//! when both axes score at least [`MIN_SCORE`] after the refinement, the art pixels are
//! roughly square (sides within 25 % of each other) and there are at least [`MIN_CELLS`] of
//! them each way. On the repository's example images the pixel strawberry (g07) scores 0.68
//! and 0.91 and no other image scores above 0.55 on both axes.

use crate::raster::Raster;

/// Summed |ΔR| + |ΔG| + |ΔB| (alpha-weighted) above which two neighbouring pixels are an edge.
pub const EDGE_STEP: u32 = 60;
/// Smallest art pixel, image pixels.
pub const MIN_PITCH: f64 = 8.0;
/// Half-width of a grid line's window, as a share of the pitch (at least one pixel).
pub const LINE_TOLERANCE: f64 = 0.06;
/// Least score, per axis, for pixel art.
pub const MIN_SCORE: f64 = 0.6;
/// Fewest art pixels each way.
pub const MIN_CELLS: usize = 6;

/// One axis of the grid: art pixel `k` spans `[phase + k·pitch, phase + (k+1)·pitch)` in
/// image pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub pitch: f64,
    pub phase: f64,
    /// Share of the axis's colour edges on the grid's lines beyond chance, 0–1.
    pub score: f64,
}

impl Axis {
    /// The art pixels whose centre lies on a `side`-pixel image: `first..first + count`.
    pub fn cells(&self, side: u32) -> (i64, usize) {
        let first = (-self.phase / self.pitch - 0.5).ceil() as i64;
        let last = ((f64::from(side) - self.phase) / self.pitch - 0.5).ceil() as i64;
        (first, (last - first).max(0) as usize)
    }

    /// Left (top) edge of art pixel `k`, image pixels.
    pub fn line(&self, k: i64) -> f64 {
        self.phase + k as f64 * self.pitch
    }
}

/// A detected pixel-art grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelGrid {
    pub x: Axis,
    pub y: Axis,
    /// Art pixels across and down whose centres lie on the image.
    pub columns: usize,
    pub rows: usize,
}

fn colour_step(a: [u8; 4], b: [u8; 4]) -> u32 {
    let w = |p: [u8; 4], k: usize| u32::from(p[k]) * u32::from(p[3]) / 255;
    let rgb: u32 = (0..3).map(|k| w(a, k).abs_diff(w(b, k))).sum();
    rgb + u32::from(a[3].abs_diff(b[3]))
}

/// Edge counts: `cols[x]` for edges between pixel `x - 1` and `x` (sitting at line `x`), and
/// `rows[y]` likewise.
fn edge_profiles(image: &Raster) -> (Vec<f64>, Vec<f64>) {
    let (w, h) = (image.width(), image.height());
    let mut cols = vec![0.0; w as usize + 1];
    let mut rows = vec![0.0; h as usize + 1];
    for y in 0..h {
        for x in 0..w {
            let p = image.pixel(x, y);
            if x > 0 && colour_step(image.pixel(x - 1, y), p) > EDGE_STEP {
                cols[x as usize] += 1.0;
            }
            if y > 0 && colour_step(image.pixel(x, y - 1), p) > EDGE_STEP {
                rows[y as usize] += 1.0;
            }
        }
    }
    (cols, rows)
}

/// Share of `e`'s edge mass within the window of a line of (`pitch`, `phase`), beyond the
/// window's own share of the pitch.
fn score(e: &[f64], pitch: f64, phase: f64) -> f64 {
    let total: f64 = e.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    let tol = (LINE_TOLERANCE * pitch).max(1.0);
    let on: f64 = e
        .iter()
        .enumerate()
        .filter(|&(i, &v)| {
            if v == 0.0 {
                return false;
            }
            let d = (i as f64 - phase).rem_euclid(pitch);
            d.min(pitch - d) <= tol
        })
        .map(|(_, v)| v)
        .sum();
    let chance = ((2.0 * tol + 1.0) / pitch).min(1.0);
    if chance >= 1.0 {
        return 0.0;
    }
    ((on / total - chance) / (1.0 - chance)).max(0.0)
}

/// The best grid along one axis of `side` pixels from its edge profile.
fn axis(e: &[f64], side: u32) -> Option<Axis> {
    let max_pitch = f64::from(side) / MIN_CELLS as f64;
    let total: f64 = e.iter().sum();
    if total <= 0.0 || max_pitch < MIN_PITCH {
        return None;
    }
    let mut tried: Vec<(f64, f64, f64)> = Vec::new();
    let steps =
        ((max_pitch.min(f64::from(side) / 8.0).max(MIN_PITCH) - MIN_PITCH) / 0.05).floor() as usize;
    for s in 0..=steps {
        let pitch = MIN_PITCH + s as f64 * 0.05;
        // Edge mass by phase, in quarter-pixel bins of the pitch.
        let bins = (pitch * 4.0).ceil() as usize;
        let mut hist = vec![0.0; bins];
        for (i, &v) in e.iter().enumerate() {
            if v > 0.0 {
                let b = ((i as f64).rem_euclid(pitch) / pitch * bins as f64) as usize;
                hist[b.min(bins - 1)] += v;
            }
        }
        let tol = (LINE_TOLERANCE * pitch).max(1.0);
        let half = (tol / pitch * bins as f64).ceil() as usize;
        let window = 2 * half + 1;
        if window >= bins {
            continue;
        }
        let mut sum: f64 = (0..window).map(|k| hist[k % bins]).sum();
        let (mut best, mut at) = (sum, 0);
        for start in 1..bins {
            sum += hist[(start + window - 1) % bins] - hist[start - 1];
            if sum > best {
                (best, at) = (sum, start);
            }
        }
        let chance = window as f64 / bins as f64;
        let share = (best / total - chance) / (1.0 - chance);
        let phase = (at + half) as f64 / bins as f64 * pitch;
        tried.push((share, pitch, phase));
    }
    let top = tried.iter().map(|t| t.0).fold(f64::NEG_INFINITY, f64::max);
    let &(_, pitch, phase) = tried
        .iter()
        .filter(|t| t.0 >= 0.93 * top)
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    // Refine: each predicted line's strongest edge within a quarter pitch, weighted least
    // squares of position against line number.
    let (mut sw, mut sk, mut sx, mut skk, mut skx) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let first = (-phase / pitch).ceil() as i64;
    let last = ((e.len() as f64 - phase) / pitch).floor() as i64;
    for k in first..=last {
        let at = phase + k as f64 * pitch;
        let (lo, hi) = (
            (at - pitch / 4.0).ceil().max(0.0) as usize,
            ((at + pitch / 4.0).floor() as usize).min(e.len() - 1),
        );
        if lo > hi {
            continue;
        }
        let (mut v, mut i) = (0.0, lo);
        for (j, &ej) in e.iter().enumerate().take(hi + 1).skip(lo) {
            if ej > v {
                (v, i) = (ej, j);
            }
        }
        if v > 0.0 {
            let kf = k as f64;
            sw += v;
            sk += v * kf;
            sx += v * i as f64;
            skk += v * kf * kf;
            skx += v * kf * i as f64;
        }
    }
    let det = sw * skk - sk * sk;
    let (pitch, phase) = if det.abs() > 1e-9 {
        let p = (sw * skx - sk * sx) / det;
        let c = (sx - p * sk) / sw;
        if (p - pitch).abs() < pitch * 0.1 {
            (p, c.rem_euclid(p))
        } else {
            (pitch, phase)
        }
    } else {
        (pitch, phase)
    };
    Some(Axis {
        pitch,
        phase,
        score: score(e, pitch, phase),
    })
}

/// The image's own pixel grid when it is pixel art (see the module docs), else `None`.
pub fn detect(image: &Raster) -> Option<PixelGrid> {
    let (cols, rows) = edge_profiles(image);
    let x = axis(&cols, image.width())?;
    let y = axis(&rows, image.height())?;
    let ratio = x.pitch / y.pitch;
    if x.score < MIN_SCORE || y.score < MIN_SCORE || !(0.8..=1.25).contains(&ratio) {
        return None;
    }
    let (_, columns) = x.cells(image.width());
    let (_, rows) = y.cells(image.height());
    (columns >= MIN_CELLS && rows >= MIN_CELLS).then_some(PixelGrid {
        x,
        y,
        columns,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 12 × 9 art-pixel picture scaled up `s` times from (`ox`, `oy`), with a little noise.
    fn art(s: u32, ox: u32, oy: u32) -> Raster {
        let colours: [[u8; 4]; 4] = [
            [255, 255, 255, 255],
            [220, 20, 30, 255],
            [30, 160, 60, 255],
            [250, 210, 40, 255],
        ];
        Raster::from_fn(12 * s + ox + 3, 9 * s + oy + 2, |x, y| {
            let (ax, ay) = ((x + s - ox % s) / s, (y + s - oy % s) / s);
            let mut c = colours[((ax * 7 + ay * 3 + ax * ay) % 4) as usize];
            c[0] = c[0].saturating_sub(((x * 31 + y * 17) % 5) as u8);
            c
        })
        .expect("raster")
    }

    #[test]
    fn finds_a_scaled_up_grid_and_its_phase() {
        let g = detect(&art(20, 7, 3)).expect("pixel art");
        assert!((g.x.pitch - 20.0).abs() < 0.05, "{:?}", g.x);
        assert!((g.y.pitch - 20.0).abs() < 0.05, "{:?}", g.y);
        assert!((g.x.phase.rem_euclid(20.0) - 7.0).abs() < 0.6, "{:?}", g.x);
        assert!((g.y.phase.rem_euclid(20.0) - 3.0).abs() < 0.6, "{:?}", g.y);
        assert!(g.columns >= 12 && g.rows >= 9);
    }

    #[test]
    fn a_smooth_picture_is_not_pixel_art() {
        let disc = Raster::from_fn(300, 300, |x, y| {
            let (dx, dy) = (x as f64 - 150.0, y as f64 - 150.0);
            if dx * dx + dy * dy < 100.0 * 100.0 {
                [200, 30, 30, 255]
            } else {
                [255, 255, 255, 255]
            }
        })
        .expect("raster");
        assert_eq!(detect(&disc), None);
        let ramp = Raster::from_fn(300, 200, |x, y| [(x % 256) as u8, (y % 256) as u8, 90, 255])
            .expect("raster");
        assert_eq!(detect(&ramp), None);
    }
}
