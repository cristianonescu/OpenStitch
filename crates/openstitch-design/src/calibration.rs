//! What sewn thread looks like, measured on real sew-outs (the T1 calibration cards,
//! `examples/sew/calibration/results`, #291).
//!
//! One layer of parallel rows, `r` rows per mm, hides tanh([`COVER_K`] · r) of the fabric: the
//! Black coverage ramp sewn on a Brother NV880E in 40 wt thread measured 16–20 % at 0.5 rows/mm,
//! 35–40 % at 1, 53–55 % at 1.5, 66–68 % at 2, 84–87 % at 3 and 96 % at 4.5 (pink and white
//! cotton), within 3 % of the curve. A flat 0.4 mm thread would hide 80 % at 2 rows/mm and all
//! of it from 2.5: real rows do not lie evenly side by side, so the fabric keeps showing until
//! about 4.5 rows/mm. Drawing each stitch [`covered_width_mm`] wide at its local row density
//! reproduces the measured coverage. Results may change as more machines, threads and fabrics
//! are measured.

use crate::fmath::Det;
use crate::geometry::{Vec2, v2};

/// The measured coverage curve's constant: rows `r` per mm cover tanh(`COVER_K` · r).
pub const COVER_K: f64 = 0.4;

/// Thread width, mm, where rows are sparse (40 wt embroidery thread).
pub const THREAD_MM: f64 = 0.4;

/// Share of the fabric one layer of rows `rows_per_mm` apart hides.
pub fn coverage(rows_per_mm: f64) -> f64 {
    // tanh x = (1 − e^−2x) / (1 + e^−2x), with the crate's platform-independent exp.
    let e = (-2.0 * COVER_K * rows_per_mm.max(0.0)).dexp();
    (1.0 - e) / (1.0 + e)
}

/// Width (mm) to draw a stitch so rows `rows_per_mm` apart hide [`coverage`] of the fabric:
/// 0.4 mm where rows are sparse, about 0.21 mm at 4.5 rows/mm.
pub fn covered_width_mm(rows_per_mm: f64) -> f64 {
    let r = rows_per_mm.max(1e-6);
    (coverage(r) / r).min(THREAD_MM)
}

/// Grid cell of [`RowDensity`], design units (1 mm).
const CELL: f64 = 10.0;
/// Smoothing of [`RowDensity`], design units (1.5 mm).
const SIGMA: f64 = 15.0;
/// Most cells a [`RowDensity`] grid holds; coarser cells are used beyond it.
const MAX_CELLS: usize = 1 << 20;

/// Local rows per mm of one thread colour: thread length per mm² on a 1 mm grid, smoothed over
/// about 3 mm (parallel rows `r` per mm lay `r` mm of thread on every mm²).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowDensity {
    origin: Vec2,
    cell: f64,
    nx: usize,
    ny: usize,
    rows_per_mm: Vec<f64>,
}

impl RowDensity {
    /// The density of `strokes` (polylines of needle points, design units).
    pub fn of(strokes: &[Vec<Vec2>]) -> RowDensity {
        let points = strokes.iter().flatten();
        let (mut min, mut max) = (v2(f64::MAX, f64::MAX), v2(f64::MIN, f64::MIN));
        for p in points {
            min = v2(min.x.min(p.x), min.y.min(p.y));
            max = v2(max.x.max(p.x), max.y.max(p.y));
        }
        if min.x > max.x {
            return RowDensity::default();
        }
        let mut cell = CELL;
        let dims = |cell: f64| {
            (
                ((max.x - min.x) / cell) as usize + 1,
                ((max.y - min.y) / cell) as usize + 1,
            )
        };
        while {
            let (nx, ny) = dims(cell);
            nx.saturating_mul(ny) > MAX_CELLS
        } {
            cell *= 2.0;
        }
        let (nx, ny) = dims(cell);
        let mut grid = vec![0.0; nx * ny];
        for s in strokes {
            for w in s.windows(2) {
                let (a, b) = (w[0], w[1]);
                let len = a.distance(b);
                let n = ((len / cell).ceil() as usize).max(1);
                for k in 0..n {
                    let p = a.lerp(b, (k as f64 + 0.5) / n as f64);
                    let i = (((p.x - min.x) / cell) as usize).min(nx - 1);
                    let j = (((p.y - min.y) / cell) as usize).min(ny - 1);
                    grid[j * nx + i] += len / n as f64;
                }
            }
        }
        let grid = blur(&grid, nx, ny, SIGMA / cell);
        // Design units of thread per square design unit, as mm per mm².
        let rows_per_mm = grid.iter().map(|v| v * 10.0 / (cell * cell)).collect();
        RowDensity {
            origin: min,
            cell,
            nx,
            ny,
            rows_per_mm,
        }
    }

    /// Rows per mm at `p` (design units); 0 outside.
    pub fn at(&self, p: Vec2) -> f64 {
        if self.rows_per_mm.is_empty() {
            return 0.0;
        }
        let i = ((p.x - self.origin.x) / self.cell).floor();
        let j = ((p.y - self.origin.y) / self.cell).floor();
        if i < 0.0 || j < 0.0 || i as usize >= self.nx || j as usize >= self.ny {
            return 0.0;
        }
        self.rows_per_mm[j as usize * self.nx + i as usize]
    }
}

/// Separable Gaussian blur (`sigma` in cells), edges reflected.
fn blur(grid: &[f64], nx: usize, ny: usize, sigma: f64) -> Vec<f64> {
    if sigma <= 0.0 {
        return grid.to_vec();
    }
    let r = (3.0 * sigma).ceil() as isize;
    let kernel: Vec<f64> = (-r..=r)
        .map(|x| (-(x * x) as f64 / (2.0 * sigma * sigma)).dexp())
        .collect();
    let total: f64 = kernel.iter().sum();
    let kernel: Vec<f64> = kernel.iter().map(|k| k / total).collect();
    let reflect = |i: isize, n: usize| -> usize {
        let n = n as isize;
        let mut i = i;
        if n == 1 {
            return 0;
        }
        while i < 0 || i >= n {
            i = if i < 0 { -i - 1 } else { 2 * n - i - 1 };
        }
        i as usize
    };
    let mut tmp = vec![0.0; grid.len()];
    for y in 0..ny {
        for x in 0..nx {
            tmp[y * nx + x] = kernel
                .iter()
                .enumerate()
                .map(|(k, w)| w * grid[y * nx + reflect(x as isize + k as isize - r, nx)])
                .sum();
        }
    }
    let mut out = vec![0.0; grid.len()];
    for y in 0..ny {
        for x in 0..nx {
            out[y * nx + x] = kernel
                .iter()
                .enumerate()
                .map(|(k, w)| w * tmp[reflect(y as isize + k as isize - r, ny) * nx + x])
                .sum();
        }
    }
    out
}

/// Farthest (design units, 0.6 mm) a stitch looks for the row beside it in [`thread_sides`].
const NEIGHBOUR_REACH: f64 = 6.0;
/// Nearer than this (design units) a stitch counts as on the same row, not beside it.
const SAME_ROW: f64 = 0.5;

/// How far (mm) each stitch's thread reaches on either side of its line: (left, right), left
/// being the side 90° anticlockwise from the stitch's direction (#893).
///
/// Each side covers [`coverage`] of half the gap to the row beside it, so two rows together
/// hide `coverage` of the fabric between them, at most [`THREAD_MM`] / 2 a side. With evenly
/// spaced rows that is [`covered_width_mm`] in all, centred on the stitch. Rows stored on the
/// machine's 0.1 mm grid are not even: 0.222 mm rows (4.5 rows/mm) lie 0.2, 0.2, 0.2 and
/// 0.3 mm apart, and a thread [`covered_width_mm`] wide (0.21 mm) left a bare stripe in every
/// 0.3 mm gap, about every 0.9 mm. Sized from the gaps themselves, the uncovered fabric is
/// spread over every gap as on a sew-out. Where no parallel row lies within 0.6 mm (an edge row,
/// sparse rows) the side uses the spacing `density` gives. The search does not stop at the
/// spacing `density` gives: thread running other ways (an underlay, rows converging round a
/// bend) counts in `density`, and with that shorter reach the parallel row beside was missed and
/// every thread drawn as narrow as the crossing rows' density asks, the densest part of a fill
/// lightest (#921).
pub fn thread_sides(strokes: &[Vec<Vec2>], density: &RowDensity) -> Vec<Vec<(f64, f64)>> {
    const CELL: f64 = NEIGHBOUR_REACH;
    let key = |p: Vec2| ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64);
    let mut cells: std::collections::HashMap<(i64, i64), Vec<(usize, usize)>> =
        std::collections::HashMap::new();
    for (i, s) in strokes.iter().enumerate() {
        for (k, w) in s.windows(2).enumerate() {
            let n = ((w[0].distance(w[1]) / (CELL / 2.0)).ceil() as usize).max(1);
            let mut last = None;
            for j in 0..=n {
                let c = key(w[0].lerp(w[1], j as f64 / n as f64));
                if last != Some(c) {
                    cells.entry(c).or_default().push((i, k));
                    last = Some(c);
                }
            }
        }
    }
    let cross = |p: Vec2, q: Vec2| p.x * q.y - p.y * q.x;
    strokes
        .iter()
        .enumerate()
        .map(|(i, s)| {
            s.windows(2)
                .enumerate()
                .map(|(k, w)| {
                    let mid = w[0].lerp(w[1], 0.5);
                    let r = density.at(mid);
                    let half = covered_width_mm(r) / 2.0;
                    let len = w[0].distance(w[1]);
                    if r <= 1e-6 || len < 1e-9 {
                        return (half, half);
                    }
                    let d = (w[1] - w[0]) * (1.0 / len);
                    let normal = v2(-d.y, d.x);
                    let spacing = 10.0 / r;
                    // Not 1.5 spacings: where rows of other directions cross (an underlay, rows
                    // converging round a bend) `r` counts them too, and the parallel row beside
                    // this one lies farther than that spacing (#921).
                    let reach = NEIGHBOUR_REACH;
                    let (mut left, mut right) = (f64::INFINITY, f64::INFINITY);
                    let (cx, cy) = key(mid);
                    let mut seen: Vec<(usize, usize)> = Vec::new();
                    for x in cx - 2..=cx + 2 {
                        for y in cy - 2..=cy + 2 {
                            for &(j, m) in cells.get(&(x, y)).into_iter().flatten() {
                                if (j, m) == (i, k) || seen.contains(&(j, m)) {
                                    continue;
                                }
                                seen.push((j, m));
                                let (a, b) = (strokes[j][m], strokes[j][m + 1]);
                                let e = b - a;
                                let el = e.length();
                                // Rows beside this one run alongside it.
                                if el < 1e-9 || cross(d, e * (1.0 / el)).abs() > 0.5 {
                                    continue;
                                }
                                let den = cross(normal, e);
                                let t = cross(a - mid, e) / den;
                                let u = cross(a - mid, normal) / den;
                                if !(0.0..=1.0).contains(&u) || t.abs() < SAME_ROW {
                                    continue;
                                }
                                if t > 0.0 {
                                    left = left.min(t);
                                } else {
                                    right = right.min(-t);
                                }
                            }
                        }
                    }
                    let side = |gap: f64| {
                        let g = if gap <= reach { gap } else { spacing };
                        (coverage(r) * g / 20.0).min(THREAD_MM / 2.0)
                    };
                    (side(left), side(right))
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_on_the_machine_grid_leave_no_bare_stripe() {
        // #893: 0.222 mm rows (4.5 rows/mm) stored on the 0.1 mm grid lie 0.2, 0.2, 0.2 and
        // 0.3 mm apart. Every gap is covered alike (none bare), and the sides add up to the
        // calibrated width on average.
        let ys: Vec<f64> = (0..120).map(|k| (k as f64 * 2.222).round()).collect();
        let strokes: Vec<Vec<Vec2>> = ys
            .iter()
            .map(|&y| vec![v2(0.0, y), v2(40.0, y), v2(80.0, y), v2(120.0, y)])
            .collect();
        let density = RowDensity::of(&strokes);
        let sides = thread_sides(&strokes, &density);
        let r = density.at(v2(60.0, 130.0));
        assert!((r - 4.5).abs() < 0.2, "{r}");
        let (mut drawn, mut n) = (0.0, 0);
        for row in 30..90 {
            let gap = (ys[row + 1] - ys[row]) / 10.0;
            // Stitch 1 of each row: left is +y (normal of a stitch running +x).
            let above = sides[row][1].0 + sides[row + 1][1].1;
            let bare = gap - above;
            assert!(
                (-1e-9..=gap * (1.0 - coverage(r)) + 0.005).contains(&bare),
                "row {row}: gap {gap} mm, covered {above} mm"
            );
            drawn += sides[row][1].0 + sides[row][1].1;
            n += 1;
        }
        let mean = drawn / n as f64;
        assert!(
            (mean - covered_width_mm(r)).abs() < 0.01,
            "{mean} vs {}",
            covered_width_mm(r)
        );
    }

    #[test]
    fn sparse_rows_keep_their_gaps() {
        // 1 row/mm: each thread as wide as the calibration gives (0.38 mm), the rest fabric.
        let strokes: Vec<Vec<Vec2>> = (0..40)
            .map(|k| vec![v2(0.0, k as f64 * 10.0), v2(200.0, k as f64 * 10.0)])
            .collect();
        let density = RowDensity::of(&strokes);
        let sides = thread_sides(&strokes, &density);
        let (l, r) = sides[20][0];
        let r_mm = density.at(v2(100.0, 200.0));
        assert!((l - r).abs() < 1e-9);
        assert!((l + r - covered_width_mm(r_mm)).abs() < 1e-9);
        assert!(l + r < 0.5, "{}", l + r);
    }

    #[test]
    fn coverage_follows_the_t1_ramp() {
        // The two T1 cards' Black ramp (pink, white): within 4 % of the curve at every density.
        for (r, pink, white) in [
            (0.5, 0.20, 0.16),
            (1.0, 0.40, 0.35),
            (1.5, 0.55, 0.53),
            (2.0, 0.68, 0.66),
            (3.0, 0.87, 0.84),
            (4.5, 0.96, 0.96),
        ] {
            let c = coverage(r);
            assert!(
                (c - pink).abs() < 0.04 && (c - white).abs() < 0.04,
                "{r}: {c}"
            );
            assert!((covered_width_mm(r) * r - c).abs() < 1e-9 || covered_width_mm(r) == THREAD_MM);
        }
        assert!((covered_width_mm(0.01) - THREAD_MM).abs() < 1e-4);
    }

    #[test]
    fn row_density_reads_parallel_rows() {
        // 20 mm square of rows 0.5 mm apart (2 rows/mm): about 2 in the middle.
        let strokes: Vec<Vec<Vec2>> = (0..40)
            .map(|k| vec![v2(0.0, k as f64 * 5.0), v2(200.0, k as f64 * 5.0)])
            .collect();
        let d = RowDensity::of(&strokes);
        let mid = d.at(v2(100.0, 100.0));
        assert!((mid - 2.0).abs() < 0.15, "{mid}");
        assert_eq!(d.at(v2(-50.0, -50.0)), 0.0);
        assert_eq!(RowDensity::of(&[]).at(v2(0.0, 0.0)), 0.0);
    }
}
