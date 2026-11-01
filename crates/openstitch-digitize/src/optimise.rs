//! Metric-driven local search over OpenStitch mode's Photo Stitch 1 rows (B16.1 T9,
//! docs/research/beyond/image-similarity-plan.md; direct binary search, Analoui and Allebach
//! 1992).
//!
//! The rows ([`crate::rows`]) choose each row's threads against the colours sample by sample.
//! The eye sees the design blurred: this pass moves run boundaries and recolours runs wherever
//! that brings the *blurred* design closer to the blurred image. Each cell shows its thread,
//! with a strip of a later-sewn neighbour row's thread at each edge, in linear light; the
//! difference from the image is taken to opponent colour channels (S-CIELAB's, Zhang and Wandell
//! 1996), blurred per channel by the eye's spatial filter at 40 cm (its narrow Gaussians), and
//! weighted per cell by how strongly lightness responds there (CIELAB's `Y^(−2/3)` slope); the
//! wide Gaussians, which hold the local mean colour, are kept on a coarse grid of
//! [`COARSE`]-cell squares. A move is kept when it lowers the sum of both weighted squared
//! errors.
//!
//! A move never adds needle points to its row: a boundary moves only while both runs stay at
//! least the minimum run and their needle points do not grow, and a run is recoloured to another
//! thread only when its points (merged with a neighbour of that thread) do not grow.
//!
//! Deterministic: rows and runs are visited in a fixed order for a fixed number of passes, all
//! sums run in a fixed order, the Gaussians are built once through `fmath::Det`, ties keep the
//! current labels, single-threaded.

use openstitch_design::fmath::Det;

/// Linear sRGB to the S-CIELAB opponent channels (through CIE XYZ).
fn opponent(c: [f64; 3]) -> [f64; 3] {
    let x = 0.4124 * c[0] + 0.3576 * c[1] + 0.1805 * c[2];
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let z = 0.0193 * c[0] + 0.1192 * c[1] + 0.9505 * c[2];
    [
        0.279 * x + 0.72 * y - 0.107 * z,
        -0.449 * x + 0.29 * y - 0.077 * z,
        0.086 * x - 0.59 * y + 0.501 * z,
    ]
}

/// The eye's narrow Gaussians per opponent channel at 40 cm, as (spread in mm, weight):
/// S-CIELAB's luminance (0.0283°, 0.133°) and chroma (0.0392°, 0.0536°) spreads, 1° = 6.98 mm.
const SPREADS: [&[(f64, f64)]; 3] = [
    &[(0.198, 0.921), (0.928, 0.105)],
    &[(0.274, 1.0)],
    &[(0.374, 1.0)],
];

/// The coarse grid's cell, in cells.
pub const COARSE: usize = 4;
/// The eye's wide Gaussians per opponent channel, mm (S-CIELAB's 0.133°, 0.494°, 0.386°).
const WIDE: [f64; 3] = [0.928, 3.45, 2.69];
/// The coarse term's weight against the fine one.
pub const COARSE_WEIGHT: f64 = 1.0;
/// How many threads nearest the image a run is tried in, besides its neighbours'.
pub const NEAREST: usize = 2;
/// Kernels are cut at this many of their widest spread.
pub const REACH: f64 = 2.5;

/// A local search's inputs.
pub struct Search<'a> {
    pub cols: usize,
    pub rows: usize,
    /// Cell size, mm.
    pub cell_mm: f64,
    /// The image per cell, linear sRGB 0–1 (`None` where transparent).
    pub image: &'a [Option<[f64; 3]>],
    /// Each target's colour, linear sRGB 0–1.
    pub targets: &'a [[f64; 3]],
    /// Each target's place in the sewing order.
    pub rank: &'a [usize],
    /// A neighbour row's overlap per band edge ([`crate::rows::Rows::strip`]).
    pub strip: f64,
    pub min_run: usize,
    /// Fixed number of passes.
    pub passes: usize,
}

/// Improves `labels` in place by the local search; `points(cells)` is a run's needle points.
pub fn optimise(s: &Search, labels: &mut [Option<usize>], points: impl Fn(usize) -> usize) {
    let (cols, rows) = (s.cols, s.rows);
    let tgt: Vec<[f64; 3]> = s.targets.iter().map(|&c| opponent(c)).collect();
    let img: Vec<Option<[f64; 3]>> = s.image.iter().map(|c| c.map(opponent)).collect();
    // Lightness sensitivity per cell, mean 1.
    let mut weight: Vec<f64> = s
        .image
        .iter()
        .map(|c| {
            c.map_or(0.0, |c| {
                let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                (y.max(0.0) + 0.01).dpowf(-2.0 / 3.0)
            })
        })
        .collect();
    let mean =
        weight.iter().sum::<f64>() / weight.iter().filter(|w| **w > 0.0).count().max(1) as f64;
    if mean > 0.0 {
        weight.iter_mut().for_each(|w| *w /= mean);
    }
    let kernels: Vec<Vec<f64>> = SPREADS
        .iter()
        .map(|parts| kernel(parts, s.cell_mm))
        .collect();
    let radius = kernels.iter().map(|k| k.len() / 2).max().unwrap_or(0);
    let later = |a: usize, b: usize| if s.rank[b] > s.rank[a] { b } else { a };
    let at = |l: &[Option<usize>], x: usize, y: usize| l[y * cols + x];
    // What a cell shows, opponent channels.
    let shown = |l: &[Option<usize>], x: usize, y: usize| -> Option<[f64; 3]> {
        let t = at(l, x, y)?;
        let up = if y > 0 { at(l, x, y - 1) } else { None };
        let down = if y + 1 < rows { at(l, x, y + 1) } else { None };
        let edge = |n: Option<usize>| n.map_or(tgt[t], |n| tgt[later(n, t)]);
        let (a, b) = (edge(up), edge(down));
        Some([0, 1, 2].map(|q| (1.0 - 2.0 * s.strip) * tgt[t][q] + s.strip * (a[q] + b[q])))
    };
    // The difference field, then its blur per channel (separable, zero outside the image).
    let mut diff = vec![[0.0; 3]; cols * rows];
    for y in 0..rows {
        for x in 0..cols {
            if let (Some(v), Some(i)) = (shown(labels, x, y), img[y * cols + x]) {
                diff[y * cols + x] = [0, 1, 2].map(|q| v[q] - i[q]);
            }
        }
    }
    let mut err = blur(&diff, cols, rows, &kernels);
    // The wide part of the eye's filter on a coarse grid of COARSE × COARSE cells: it keeps the
    // local mean colour, which the narrow part alone lets drift.
    let (ccols, crows) = (cols.div_ceil(COARSE), rows.div_ceil(COARSE));
    let wide: Vec<Vec<f64>> = WIDE
        .iter()
        .map(|&spread| kernel(&[(spread, 1.0)], s.cell_mm * COARSE as f64))
        .collect();
    let cr = wide.iter().map(|k| k.len() / 2).max().unwrap_or(0);
    let mut cdiff = vec![[0.0; 3]; ccols * crows];
    let mut cweight = vec![0.0; ccols * crows];
    let norm = 1.0 / (COARSE * COARSE) as f64;
    for y in 0..rows {
        for x in 0..cols {
            let c = (y / COARSE) * ccols + x / COARSE;
            for q in 0..3 {
                cdiff[c][q] += diff[y * cols + x][q] * norm;
            }
            cweight[c] += weight[y * cols + x] * norm;
        }
    }
    let mut cerr = blur(&cdiff, ccols, crows, &wide);
    let cgain = |cerr: &[[f64; 3]], delta: &[(usize, usize, [f64; 3])]| -> f64 {
        // The change per coarse cell.
        let mut cd: Vec<(usize, usize, [f64; 3])> = Vec::new();
        for &(x, y, v) in delta {
            let (cx, cy) = (x / COARSE, y / COARSE);
            match cd.iter_mut().find(|e| e.0 == cx && e.1 == cy) {
                Some(e) => (0..3).for_each(|q| e.2[q] += v[q] * norm),
                None => cd.push((cx, cy, v.map(|v| v * norm))),
            }
        }
        let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0, usize::MAX, 0);
        for &(x, y, _) in &cd {
            x0 = x0.min(x.saturating_sub(cr));
            x1 = x1.max((x + cr + 1).min(ccols));
            y0 = y0.min(y.saturating_sub(cr));
            y1 = y1.max((y + cr + 1).min(crows));
        }
        let mut total = 0.0;
        for y in y0..y1 {
            for x in x0..x1 {
                let w = cweight[y * ccols + x];
                if w == 0.0 {
                    continue;
                }
                let mut d = [0.0; 3];
                for &(cx, cy, v) in &cd {
                    let (dx, dy) = (x.abs_diff(cx), y.abs_diff(cy));
                    for q in 0..3 {
                        let h = wide[q].len() / 2;
                        if dx <= h && dy <= h {
                            d[q] += wide[q][h + dx] * wide[q][h + dy] * v[q];
                        }
                    }
                }
                let e = cerr[y * ccols + x];
                total += w * (0..3).map(|q| 2.0 * e[q] * d[q] + d[q] * d[q]).sum::<f64>();
            }
        }
        COARSE_WEIGHT * total
    };
    let capply = |cerr: &mut [[f64; 3]], delta: &[(usize, usize, [f64; 3])]| {
        for &(x, y, v) in delta {
            let (cx, cy) = (x / COARSE, y / COARSE);
            for q in 0..3 {
                let h = wide[q].len() / 2;
                for j in cy.saturating_sub(h)..(cy + h + 1).min(crows) {
                    let ky = wide[q][h + j.abs_diff(cy)];
                    for i in cx.saturating_sub(h)..(cx + h + 1).min(ccols) {
                        cerr[j * ccols + i][q] += wide[q][h + i.abs_diff(cx)] * ky * v[q] * norm;
                    }
                }
            }
        }
    };
    // Change in weighted error if the cells in `delta` change by their vectors.
    let gain = |err: &[[f64; 3]], delta: &[(usize, usize, [f64; 3])]| -> f64 {
        let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0, usize::MAX, 0);
        for &(x, y, _) in delta {
            x0 = x0.min(x.saturating_sub(radius));
            x1 = x1.max((x + radius + 1).min(cols));
            y0 = y0.min(y.saturating_sub(radius));
            y1 = y1.max((y + radius + 1).min(rows));
        }
        let mut total = 0.0;
        for y in y0..y1 {
            for x in x0..x1 {
                let w = weight[y * cols + x];
                if w == 0.0 {
                    continue;
                }
                let mut d = [0.0; 3];
                for &(cx, cy, v) in delta {
                    let (dx, dy) = (x.abs_diff(cx), y.abs_diff(cy));
                    for q in 0..3 {
                        let h = kernels[q].len() / 2;
                        if dx <= h && dy <= h {
                            d[q] += kernels[q][h + dx] * kernels[q][h + dy] * v[q];
                        }
                    }
                }
                let e = err[y * cols + x];
                total += w * (0..3).map(|q| 2.0 * e[q] * d[q] + d[q] * d[q]).sum::<f64>();
            }
        }
        total
    };
    let apply = |err: &mut [[f64; 3]], delta: &[(usize, usize, [f64; 3])]| {
        for &(cx, cy, v) in delta {
            for q in 0..3 {
                let h = kernels[q].len() / 2;
                for y in cy.saturating_sub(h)..(cy + h + 1).min(rows) {
                    let ky = kernels[q][h + y.abs_diff(cy)];
                    for x in cx.saturating_sub(h)..(cx + h + 1).min(cols) {
                        err[y * cols + x][q] += kernels[q][h + x.abs_diff(cx)] * ky * v[q];
                    }
                }
            }
        }
    };
    // The cells whose shown colour changes when cells `(x, y)` of row `y` take thread `t`:
    // those cells and the cells above and below them (their strips).
    let delta_of = |l: &mut [Option<usize>], xs: &[usize], y: usize, t: usize| {
        let mut cells: Vec<(usize, usize)> = Vec::with_capacity(xs.len() * 3);
        for &x in xs {
            if y > 0 {
                cells.push((x, y - 1));
            }
            cells.push((x, y));
            if y + 1 < rows {
                cells.push((x, y + 1));
            }
        }
        let before: Vec<Option<[f64; 3]>> = cells.iter().map(|&(x, y)| shown(l, x, y)).collect();
        let old: Vec<Option<usize>> = xs.iter().map(|&x| l[y * cols + x]).collect();
        for &x in xs {
            l[y * cols + x] = Some(t);
        }
        let mut delta = Vec::with_capacity(cells.len());
        for (&(x, cy), b) in cells.iter().zip(before) {
            if let (Some(b), Some(a)) = (b, shown(l, x, cy)) {
                let v = [0, 1, 2].map(|q| a[q] - b[q]);
                if v.iter().any(|v| v.abs() > 1e-15) {
                    delta.push((x, cy, v));
                }
            }
        }
        for (&x, o) in xs.iter().zip(old) {
            l[y * cols + x] = o;
        }
        delta
    };
    // Threads worth trying on the run of `t` over cells `a..b` of row `y`: its neighbours'
    // (left, right, above, below) and the [`NEAREST`] nearest to the image there, ascending.
    let candidates = |l: &[Option<usize>], a: usize, b: usize, y: usize, t: usize| {
        let mut ks: Vec<usize> = Vec::with_capacity(4 + NEAREST);
        let mid = (a + b) / 2;
        let mut push = |k: Option<usize>| {
            if let Some(k) = k
                && k != t
                && !ks.contains(&k)
            {
                ks.push(k);
            }
        };
        push((a > 0).then(|| l[y * cols + a - 1]).flatten());
        push((b < cols).then(|| l[y * cols + b]).flatten());
        push((y > 0).then(|| l[(y - 1) * cols + mid]).flatten());
        push((y + 1 < rows).then(|| l[(y + 1) * cols + mid]).flatten());
        let mut want = [0.0; 3];
        for i in a..b {
            if let Some(c) = img[y * cols + i] {
                for q in 0..3 {
                    want[q] += c[q];
                }
            }
        }
        let mut by_dist: Vec<(f64, usize)> = tgt
            .iter()
            .enumerate()
            .map(|(k, c)| {
                (
                    (0..3)
                        .map(|q| (c[q] * (b - a) as f64 - want[q]).powi(2))
                        .sum(),
                    k,
                )
            })
            .collect();
        by_dist.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.cmp(&q.1)));
        for &(_, k) in by_dist.iter().take(NEAREST + 1) {
            push(Some(k));
        }
        ks.sort_unstable();
        ks
    };
    for _ in 0..s.passes {
        let mut changed = false;
        for y in 0..rows {
            let mut x = 0;
            while x < cols {
                let Some(t) = labels[y * cols + x] else {
                    x += 1;
                    continue;
                };
                let a = x;
                while x < cols && labels[y * cols + x] == Some(t) {
                    x += 1;
                }
                let len = x - a;
                // The next run in the row, if it starts right here.
                let next = (x < cols).then(|| labels[y * cols + x]).flatten();
                let next_len = next.map_or(0, |u| {
                    (x..cols)
                        .take_while(|&i| labels[y * cols + i] == Some(u))
                        .count()
                });
                let mut best: Option<(f64, Vec<usize>, usize)> = None;
                let mut consider = |cand: (f64, Vec<usize>, usize)| {
                    if cand.0 < -1e-12 && best.as_ref().is_none_or(|b| cand.0 < b.0 - 1e-15) {
                        best = Some(cand);
                    }
                };
                if let Some(u) = next {
                    let before = points(len) + points(next_len);
                    // The boundary one cell left: this run's last cell joins the next run.
                    if len > s.min_run && points(len - 1) + points(next_len + 1) <= before {
                        let d = delta_of(labels, &[x - 1], y, u);
                        consider((gain(&err, &d) + cgain(&cerr, &d), vec![x - 1], u));
                    }
                    // One cell right: the next run's first cell joins this one.
                    if next_len > s.min_run && points(len + 1) + points(next_len - 1) <= before {
                        let d = delta_of(labels, &[x], y, t);
                        consider((gain(&err, &d) + cgain(&cerr, &d), vec![x], t));
                    }
                }
                // Recolour the whole run: the same needle points, or fewer where it merges with
                // a neighbour of that thread.
                let cells: Vec<usize> = (a..x).collect();
                for k in candidates(labels, a, x, y, t) {
                    let d = delta_of(labels, &cells, y, k);
                    consider((gain(&err, &d) + cgain(&cerr, &d), cells.clone(), k));
                }
                if let Some((_, xs, k)) = best {
                    let d = delta_of(labels, &xs, y, k);
                    apply(&mut err, &d);
                    capply(&mut cerr, &d);
                    for &c in &xs {
                        labels[y * cols + c] = Some(k);
                    }
                    changed = true;
                    // Revisit from this run's start: its extent changed.
                    x = a;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// A 1-D kernel over `cell_mm` cells: the weighted sum of Gaussians `(spread mm, weight)`,
/// truncated at [`REACH`] of the widest spread, normalised to sum 1.
fn kernel(parts: &[(f64, f64)], cell_mm: f64) -> Vec<f64> {
    let widest = parts.iter().map(|p| p.0).fold(0.0, f64::max) / cell_mm;
    let h = (REACH * widest).ceil().max(1.0) as usize;
    let mut k: Vec<f64> = (0..=2 * h)
        .map(|i| {
            let d = i as f64 - h as f64;
            parts
                .iter()
                .map(|&(spread, w)| {
                    let sg = (spread / cell_mm).max(0.3);
                    w * (-(d * d) / (2.0 * sg * sg)).dexp() / sg
                })
                .sum()
        })
        .collect();
    let total: f64 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= total);
    k
}

/// Each channel of `field` blurred by its kernel (rows, then columns), zero outside.
fn blur(field: &[[f64; 3]], cols: usize, rows: usize, kernels: &[Vec<f64>]) -> Vec<[f64; 3]> {
    let mut out = vec![[0.0; 3]; field.len()];
    let mut tmp = vec![0.0; field.len()];
    for (q, k) in kernels.iter().enumerate() {
        let h = k.len() / 2;
        for y in 0..rows {
            for x in 0..cols {
                let mut acc = 0.0;
                for i in x.saturating_sub(h)..(x + h + 1).min(cols) {
                    acc += k[h + i.abs_diff(x)] * field[y * cols + i][q];
                }
                tmp[y * cols + x] = acc;
            }
        }
        for y in 0..rows {
            for x in 0..cols {
                let mut acc = 0.0;
                for j in y.saturating_sub(h)..(y + h + 1).min(rows) {
                    acc += k[h + j.abs_diff(y)] * tmp[j * cols + x];
                }
                out[y * cols + x][q] = acc;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_points(l: &[Option<usize>], cols: usize, points: &dyn Fn(usize) -> usize) -> usize {
        let mut n = 0;
        for row in l.chunks(cols) {
            let mut x = 0;
            while x < cols {
                let t = row[x];
                let a = x;
                while x < cols && row[x] == t {
                    x += 1;
                }
                n += points(x - a);
            }
        }
        n
    }

    /// A grey made of black and white runs whose boundaries sit badly: the search moves them so
    /// the blurred rows come closer to the grey, without adding needle points or breaking the
    /// minimum run, and twice gives the same result.
    #[test]
    fn the_search_improves_without_adding_points() {
        let (cols, rows) = (48, 24);
        let image = vec![Some([0.2; 3]); cols * rows];
        let targets = [[0.0; 3], [1.0; 3]];
        // Long white runs, short black ones: far too light for a 0.2 grey.
        let start: Vec<Option<usize>> = (0..cols * rows)
            .map(|i| Some(usize::from(i % cols % 12 >= 4)))
            .collect();
        let search = Search {
            cols,
            rows,
            cell_mm: 0.25,
            image: &image,
            targets: &targets,
            rank: &[1, 0],
            strip: 0.0,
            min_run: 4,
            passes: 3,
        };
        let points = |n: usize| n.div_ceil(12) + 1;
        let mut a = start.clone();
        optimise(&search, &mut a, points);
        let mut b = start.clone();
        optimise(&search, &mut b, points);
        assert_eq!(a, b);
        assert!(row_points(&a, cols, &points) <= row_points(&start, cols, &points));
        let black = |l: &[Option<usize>]| l.iter().filter(|t| **t == Some(0)).count();
        assert!(black(&a) > black(&start), "the grey darkens");
        for row in a.chunks(cols) {
            let mut x = 0;
            while x < cols {
                let t = row[x];
                let s = x;
                while x < cols && row[x] == t {
                    x += 1;
                }
                assert!(
                    x - s >= 4 || s == 0 || x == cols,
                    "a run of {} at {s}",
                    x - s
                );
            }
        }
    }
}
