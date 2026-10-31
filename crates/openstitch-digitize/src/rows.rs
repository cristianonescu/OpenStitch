//! Rate-constrained fill rows for OpenStitch mode's Photo Stitch 1 (B16.1, plan T2/T3,
//! docs/research/beyond/image-similarity-plan.md).
//!
//! The image is sampled finely along each fill row. Each row becomes the sequence of threads
//! that minimises the squared CIELAB error to the row's colours plus a penalty `λ` for every
//! change of thread, under a minimum run length (so no stitch is shorter than the shortest
//! run), by dynamic programming over (thread, run length so far). A row's remaining error is
//! carried partly along the row's path ([`Rows::along`]) and the rest to the next row (¼, ½, ¼),
//! which mixes the threads as the eye blurs neighbouring runs and rows. Every change of thread
//! costs needle points, so `λ` trades likeness against needle density: [`fit_density`] sets one
//! `λ` by bisection to meet a mean density over the design (T2), and [`fit_cap`] then varies it
//! per tile so no window of the design exceeds a needle cap while the budget goes where the
//! image needs it (T3).
//!
//! Deterministic: plain `f64` arithmetic in a fixed order, single-threaded; ties go to the
//! lower thread index; the bisection runs a fixed number of steps.

use crate::perceptual::Lab;

/// One row assignment's settings.
#[derive(Clone, Copy, Debug)]
pub struct Rows {
    /// Fewest samples in a run of one thread (a row segment shorter than this is one run).
    pub min_run: usize,
    /// Penalty per change of thread, in squared CIELAB units.
    pub lambda: f64,
    /// How much of a row's band a neighbouring row's thread covers at each edge, 0–0.5: the
    /// thread is wider than the row spacing, so where two rows meet the one sewn later shows
    /// (`(thread − spacing) / 2 / spacing`).
    pub strip: f64,
    /// Share of a sample's error carried on along the row's path (the rest goes to the next
    /// row), 0–1: Floyd–Steinberg's 7/16 lets neighbouring runs average out.
    pub along: f64,
}

/// Overlap of a `thread` wide thread over rows `spacing` apart, as [`Rows::strip`].
pub fn strip(thread: f64, spacing: f64) -> f64 {
    ((thread - spacing) / 2.0 / spacing).clamp(0.0, 0.5)
}

/// A change-of-thread penalty that varies over the image: one multiplier of [`Rows::lambda`]
/// per tile of `tile`×`tile` cells.
#[derive(Clone, Debug)]
pub struct Field {
    pub tile: usize,
    /// Tiles across.
    pub across: usize,
    pub scale: Vec<f64>,
}

impl Field {
    /// The same penalty everywhere.
    pub fn uniform() -> Field {
        Field {
            tile: usize::MAX,
            across: 1,
            scale: vec![1.0],
        }
    }

    /// `tile`-cell tiles over a `cols`×`rows` grid, every multiplier `v`.
    pub fn tiles(cols: usize, rows: usize, tile: usize, v: f64) -> Field {
        let tile = tile.max(1);
        let across = cols.div_ceil(tile);
        Field {
            tile,
            across,
            scale: vec![v; across * rows.div_ceil(tile)],
        }
    }

    fn at(&self, x: usize, y: usize) -> f64 {
        self.scale[(y / self.tile) * self.across + x / self.tile]
    }
}

/// Each opaque cell's target index, `None` for transparent cells. `cells` is `cols`×`rows`,
/// row-major, in CIELAB; `targets` are the thread (and fabric) colours in CIELAB; `rank` is
/// each target's place in the sewing order.
pub fn assign(
    cells: &[Option<Lab>],
    cols: usize,
    rows: usize,
    targets: &[Lab],
    rank: &[usize],
    r: Rows,
) -> Vec<Option<usize>> {
    assign_in(cells, cols, rows, targets, rank, r, &Field::uniform())
}

/// [`assign`] with the penalty scaled by `field` where a run starts.
pub fn assign_in(
    cells: &[Option<Lab>],
    cols: usize,
    rows: usize,
    targets: &[Lab],
    rank: &[usize],
    r: Rows,
    field: &Field,
) -> Vec<Option<usize>> {
    let mut out = vec![None; cells.len()];
    if targets.is_empty() {
        return out;
    }
    let mut carry = vec![[0.0; 3]; cols];
    let mut want = vec![[0.0; 3]; cols];
    let mut err = vec![[0.0; 3]; cols];
    let mut dp = Dp::new(targets.len(), r.min_run.max(1));
    let later = |a: usize, b: usize| if rank[b] > rank[a] { b } else { a };
    let mut up: Vec<Option<usize>> = vec![None; cols];
    for y in 0..rows {
        let row = &cells[y * cols..(y + 1) * cols];
        for x in 0..cols {
            want[x] = match row[x] {
                Some(c) => {
                    let w = [0, 1, 2].map(|k| c[k] + carry[x][k]);
                    [w[0].clamp(0.0, 100.0), w[1], w[2]]
                }
                None => [0.0; 3],
            };
            err[x] = [0.0; 3];
        }
        // Each stretch of opaque cells is a sequence of its own.
        let mut x = 0;
        while x < cols {
            if row[x].is_none() {
                x += 1;
                continue;
            }
            let a = x;
            while x < cols && row[x].is_some() {
                x += 1;
            }
            // What each target shows in this row's band: its own colour, but where the row
            // above was sewn later its thread covers the band's top strip.
            let seen = |i: usize, t: usize| match up[a + i] {
                Some(u) if r.strip > 0.0 => {
                    let v = later(u, t);
                    [0, 1, 2].map(|k| (1.0 - r.strip) * targets[t][k] + r.strip * targets[v][k])
                }
                _ => targets[t],
            };
            let start = |i: usize, _: usize| r.lambda * field.at(a + i, y);
            let (seq, res) = dp.solve(&want[a..x], &start, &seen, r.along);
            for (i, &t) in seq.iter().enumerate() {
                out[y * cols + a + i] = Some(t);
                err[a + i] = res[i];
                // This row's thread covers the bottom strip of the band above when sewn later:
                // the band above shifts by that much, which the rows below make up.
                if let Some(u) = up[a + i] {
                    let v = later(u, t);
                    for k in 0..3 {
                        err[a + i][k] -= r.strip * (targets[v][k] - targets[u][k]);
                    }
                }
            }
        }
        for x in 0..cols {
            let l = err[x.saturating_sub(1)];
            let rr = err[(x + 1).min(cols - 1)];
            carry[x] = [0, 1, 2].map(|k| 0.5 * err[x][k] + 0.25 * l[k] + 0.25 * rr[k]);
            if row[x].is_none() {
                carry[x] = [0.0; 3];
            }
        }
        for x in 0..cols {
            up[x] = out[y * cols + x];
        }
    }
    out
}

/// The `λ` (bisected over `[0, hi]` in a fixed number of steps) whose assignment's needle
/// count, by `points`, comes closest to `budget` without exceeding it; `hi` when even that
/// exceeds it.
pub fn fit_density(
    budget: f64,
    hi: f64,
    steps: usize,
    mut run: impl FnMut(f64) -> (Vec<Option<usize>>, f64),
) -> (Vec<Option<usize>>, f64) {
    let (zero, n0) = run(0.0);
    if n0 <= budget {
        return (zero, 0.0);
    }
    let (mut lo, mut hi) = (0.0, hi);
    let (mut best, nhi) = run(hi);
    if nhi > budget {
        return (best, hi);
    }
    let mut best_lambda = hi;
    for _ in 0..steps {
        let mid = 0.5 * (lo + hi);
        let (labels, n) = run(mid);
        if n <= budget {
            hi = mid;
            best = labels;
            best_lambda = mid;
        } else {
            lo = mid;
        }
    }
    (best, best_lambda)
}

/// How [`fit_cap`] holds the needle density.
#[derive(Clone, Copy, Debug)]
pub struct Cap {
    /// Most needle points in any `window`×`window` cells.
    pub points: f64,
    /// Most needle points over the whole grid.
    pub total: f64,
    /// Window side, cells.
    pub window: usize,
    /// Penalty tile side, cells.
    pub tile: usize,
    /// Adjustment rounds (fixed, for determinism).
    pub rounds: usize,
}

/// Holds the needle points under `cap.points` in every window of `cap.window`×`cap.window`
/// cells and under `cap.total` over the grid, while spending them where the image needs them:
/// the penalty is adjusted per tile for a fixed number of rounds, raised in tiles under a window
/// over its cap (everywhere while the total is over), and lowered where every window around and
/// the total are well under theirs. If a window is still over after the rounds, the penalty keeps rising in its
/// tiles, for at most as many rounds again, and finally everywhere. `needles` gives each
/// cell's needle points for an assignment.
pub fn fit_cap(
    cols: usize,
    rows: usize,
    cap: Cap,
    mut run: impl FnMut(&Field) -> Vec<Option<usize>>,
    needles: impl Fn(&[Option<usize>]) -> Vec<u32>,
) -> Vec<Option<usize>> {
    let w = cap.window.min(cols).min(rows).max(1);
    // A design smaller than the window: the cap scales to its area.
    let limit = cap.points * (w * w) as f64 / (cap.window * cap.window).max(1) as f64;
    let mut field = Field::tiles(cols, rows, cap.tile, 1.0);
    let (tile, across) = (field.tile, field.across);
    let tiles_y = rows.div_ceil(tile);
    let floor = 1.0 / 64.0;
    let mut worst = vec![0.0f64; field.scale.len()];
    let mut sum = vec![0u64; (cols + 1) * (rows + 1)];
    let mut labels = run(&field);
    for round in 0..cap.rounds * 3 {
        let n = needles(&labels);
        for y in 0..rows {
            let mut line = 0u64;
            for x in 0..cols {
                line += u64::from(n[y * cols + x]);
                sum[(y + 1) * (cols + 1) + x + 1] = sum[y * (cols + 1) + x + 1] + line;
            }
        }
        let at = |x: usize, y: usize| sum[y * (cols + 1) + x];
        // The whole grid's share of its cap is every tile's floor.
        let total = at_total(&sum, cols, rows) as f64 / cap.total;
        worst.fill(total);
        let mut over = total > 1.0;
        for y0 in 0..=rows - w {
            for x0 in 0..=cols - w {
                let pts = (at(x0 + w, y0 + w) + at(x0, y0)) - (at(x0 + w, y0) + at(x0, y0 + w));
                let ratio = pts as f64 / limit;
                over |= ratio > 1.0;
                for ty in y0 / tile..=((y0 + w - 1) / tile).min(tiles_y - 1) {
                    for tx in x0 / tile..=((x0 + w - 1) / tile).min(across - 1) {
                        let v = &mut worst[ty * across + tx];
                        *v = v.max(ratio);
                    }
                }
            }
        }
        // Tuning rounds raise and lower; later rounds only raise until no window is over,
        // the last third everywhere.
        let tuning = round < cap.rounds;
        if !tuning && !over {
            break;
        }
        let everywhere = round >= 2 * cap.rounds;
        for (s, &r) in field.scale.iter_mut().zip(&worst) {
            if r > 1.0 || everywhere {
                *s = s.max(floor) * (1.0 + 2.0 * (r - 1.0).max(0.25));
            } else if tuning && r < 0.92 {
                *s = (*s * 0.75).max(floor);
            }
        }
        labels = run(&field);
    }
    labels
}

/// The needle points of a prefix-sum grid's whole extent.
fn at_total(sum: &[u64], cols: usize, rows: usize) -> u64 {
    sum[rows * (cols + 1) + cols]
}

/// The run-length-constrained Viterbi over one row segment, with its buffers kept between
/// calls.
struct Dp {
    k: usize,
    m: usize,
    /// Cost so far per (thread, run length 1..=m, m meaning m or more).
    cost: Vec<f64>,
    next: Vec<f64>,
    /// Per position and thread: the thread a new run switched from.
    from: Vec<u8>,
    /// Per position and thread: whether the "m or more" state came from itself (else from m−1).
    stayed: Vec<bool>,
    /// Per state: the error its best path carries to the next position (see [`Rows::along`]).
    err: Vec<Lab>,
    err_next: Vec<Lab>,
}

impl Dp {
    fn new(k: usize, m: usize) -> Self {
        Dp {
            k,
            m,
            cost: vec![0.0; k * m],
            next: vec![0.0; k * m],
            from: Vec::new(),
            stayed: Vec::new(),
            err: vec![[0.0; 3]; k * m],
            err_next: vec![[0.0; 3]; k * m],
        }
    }

    /// `start(i, t)`: the penalty for a new run of `t` at position `i`; `seen(i, t)`: the colour
    /// target `t` shows there. `along` of each position's error is carried to the next one on
    /// the same path. Returns the threads and each position's error after that share.
    fn solve(
        &mut self,
        want: &[Lab],
        start: &dyn Fn(usize, usize) -> f64,
        seen: &dyn Fn(usize, usize) -> Lab,
        along: f64,
    ) -> (Vec<usize>, Vec<Lab>) {
        let (k, n) = (self.k, want.len());
        let sq = |c: Lab, t: Lab| {
            let d = [c[0] - t[0], c[1] - t[1], c[2] - t[2]];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
        };
        // Too short for a full run: one thread for the whole segment.
        if n < self.m {
            let mut best = (f64::INFINITY, 0);
            for t in 0..k {
                let c: f64 = want
                    .iter()
                    .enumerate()
                    .map(|(i, &w)| sq(w, seen(i, t)))
                    .sum();
                if c < best.0 {
                    best = (c, t);
                }
            }
            let seq = vec![best.1; n];
            let res = residuals(want, &seq, seen, along);
            return (seq, res);
        }
        let m = self.m;
        let idx = |t: usize, r: usize| t * m + (r - 1);
        self.from.clear();
        self.from.resize(n * k, 0);
        self.stayed.clear();
        self.stayed.resize(n * k, false);
        for t in 0..k {
            for r in 1..=m {
                self.cost[idx(t, r)] = f64::INFINITY;
            }
            let e = sub(want[0], seen(0, t));
            self.cost[idx(t, 1)] = dot(e);
            self.err[idx(t, 1)] = e;
        }
        for (x, &w) in want.iter().enumerate().skip(1) {
            // The two cheapest finished runs, for a change of thread.
            let (mut b1, mut b2) = ((f64::INFINITY, usize::MAX), (f64::INFINITY, usize::MAX));
            for t in 0..k {
                let c = self.cost[idx(t, m)];
                if c < b1.0 {
                    b2 = b1;
                    b1 = (c, t);
                } else if c < b2.0 {
                    b2 = (c, t);
                }
            }
            for t in 0..k {
                let st = seen(x, t);
                // The error at x on a path whose previous state is `from`.
                let at = |dp: &Dp, from: usize| {
                    let e = dp.err[from];
                    let w2 = [0, 1, 2].map(|c| w[c] + along * e[c]);
                    sub(w2, st)
                };
                let sw = if b1.1 != t { b1 } else { b2 };
                if sw.1 == usize::MAX {
                    self.next[idx(t, 1)] = f64::INFINITY;
                    self.err_next[idx(t, 1)] = [0.0; 3];
                } else {
                    let e = at(self, idx(sw.1, m));
                    self.next[idx(t, 1)] = sw.0 + start(x, t) + dot(e);
                    self.err_next[idx(t, 1)] = e;
                }
                self.from[x * k + t] = sw.1.min(255) as u8;
                for r in 2..m {
                    let e = at(self, idx(t, r - 1));
                    self.next[idx(t, r)] = self.cost[idx(t, r - 1)] + dot(e);
                    self.err_next[idx(t, r)] = e;
                }
                if m >= 2 {
                    let (eg, es) = (at(self, idx(t, m - 1)), at(self, idx(t, m)));
                    let grow = self.cost[idx(t, m - 1)] + dot(eg);
                    let stay = self.cost[idx(t, m)] + dot(es);
                    let s = stay < grow;
                    self.next[idx(t, m)] = if s { stay } else { grow };
                    self.err_next[idx(t, m)] = if s { es } else { eg };
                    self.stayed[x * k + t] = s;
                } else {
                    // m == 1: the only state is "a run of one or more".
                    let es = at(self, idx(t, 1));
                    let stay = self.cost[idx(t, 1)] + dot(es);
                    let s = stay <= self.next[idx(t, 1)];
                    if s {
                        self.next[idx(t, 1)] = stay;
                        self.err_next[idx(t, 1)] = es;
                    }
                    self.stayed[x * k + t] = s;
                }
            }
            std::mem::swap(&mut self.err, &mut self.err_next);
            std::mem::swap(&mut self.cost, &mut self.next);
        }
        let mut t = 0;
        for c in 1..k {
            if self.cost[idx(c, m)] < self.cost[idx(t, m)] {
                t = c;
            }
        }
        let mut r = m;
        let mut seq = vec![0; n];
        for x in (0..n).rev() {
            seq[x] = t;
            if x == 0 {
                break;
            }
            if m == 1 {
                if !self.stayed[x * k + t] {
                    t = usize::from(self.from[x * k + t]);
                }
            } else if r == 1 {
                t = usize::from(self.from[x * k + t]);
                r = m;
            } else if r == m {
                if !self.stayed[x * k + t] {
                    r = m - 1;
                }
            } else {
                r -= 1;
            }
        }
        let res = residuals(want, &seq, seen, along);
        (seq, res)
    }
}

fn sub(a: Lab, b: Lab) -> Lab {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(e: Lab) -> f64 {
    e[0] * e[0] + e[1] * e[1] + e[2] * e[2]
}

/// Each position's error along `seq`, `along` of it carried on to the next position, and what
/// is left of it after that share.
fn residuals(
    want: &[Lab],
    seq: &[usize],
    seen: &dyn Fn(usize, usize) -> Lab,
    along: f64,
) -> Vec<Lab> {
    let mut e = [0.0; 3];
    let mut out = Vec::with_capacity(seq.len());
    for (i, (&w, &t)) in want.iter().zip(seq).enumerate() {
        let w2 = [0, 1, 2].map(|c| w[c] + along * e[c]);
        e = sub(w2, seen(i, t));
        out.push(e.map(|v| (1.0 - along) * v));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(seq: &[Option<usize>]) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for &s in seq {
            let s = s.expect("opaque");
            match out.last_mut() {
                Some((t, n)) if *t == s => *n += 1,
                _ => out.push((s, 1)),
            }
        }
        out
    }

    #[test]
    fn runs_hold_the_minimum_length() {
        // Alternating black and white every sample: no run may be shorter than 4.
        let cols = 40;
        let cells: Vec<Option<Lab>> = (0..cols)
            .map(|x| {
                Some(if x % 2 == 0 {
                    [0.0; 3]
                } else {
                    [100.0, 0.0, 0.0]
                })
            })
            .collect();
        let targets = [[0.0; 3], [100.0, 0.0, 0.0]];
        let out = assign(
            &cells,
            cols,
            1,
            &targets,
            &[0, 1, 2],
            Rows {
                min_run: 4,
                lambda: 0.0,
                strip: 0.0,
                along: 0.0,
            },
        );
        let r = runs(&out);
        assert!(r.iter().all(|&(_, n)| n >= 4), "{r:?}");
    }

    #[test]
    fn a_step_changes_thread_where_the_colour_changes() {
        let cols = 20;
        let cells: Vec<Option<Lab>> = (0..cols)
            .map(|x| Some(if x < 7 { [0.0; 3] } else { [100.0, 0.0, 0.0] }))
            .collect();
        let targets = [[0.0; 3], [100.0, 0.0, 0.0]];
        let out = assign(
            &cells,
            cols,
            1,
            &targets,
            &[0, 1, 2],
            Rows {
                min_run: 4,
                lambda: 10.0,
                strip: 0.0,
                along: 0.0,
            },
        );
        assert_eq!(runs(&out), vec![(0, 7), (1, 13)]);
    }

    #[test]
    fn a_larger_penalty_changes_thread_less() {
        let cols = 64;
        let cells: Vec<Option<Lab>> = (0..cols)
            .map(|x| Some([((x * 37) % 100) as f64, 0.0, 0.0]))
            .collect();
        let targets = [[0.0; 3], [50.0, 0.0, 0.0], [100.0, 0.0, 0.0]];
        let low = runs(&assign(
            &cells,
            cols,
            1,
            &targets,
            &[0, 1, 2],
            Rows {
                min_run: 2,
                lambda: 0.0,
                strip: 0.0,
                along: 0.0,
            },
        ));
        let high = runs(&assign(
            &cells,
            cols,
            1,
            &targets,
            &[0, 1, 2],
            Rows {
                min_run: 2,
                lambda: 1e5,
                strip: 0.0,
                along: 0.0,
            },
        ));
        assert!(high.len() < low.len());
        assert_eq!(high.len(), 1);
    }

    #[test]
    fn carrying_the_error_along_mixes_within_the_row() {
        // One row of mid-grey between black and white threads, runs of at least 2: alone each
        // sample is as far from either, so with the error carried along the row the threads
        // alternate and average to the grey.
        let cols = 40;
        let cells: Vec<Option<Lab>> = vec![Some([50.0, 0.0, 0.0]); cols];
        let targets = [[0.0; 3], [100.0, 0.0, 0.0]];
        let rows = |along| Rows {
            min_run: 2,
            lambda: 0.0,
            strip: 0.0,
            along,
        };
        let mixed = assign(&cells, cols, 1, &targets, &[0, 1], rows(7.0 / 16.0));
        let white = mixed.iter().filter(|t| **t == Some(1)).count();
        assert!((15..=25).contains(&white), "{white} of {cols} white");
        assert!(runs(&mixed).len() >= 8);
    }

    #[test]
    fn the_cap_holds_in_every_window() {
        // A noisy 64 × 64 image whose left half wants many changes of thread: one needle point
        // per run start, at most 30 in any 16 × 16 window.
        let (cols, rows) = (64, 64);
        let cells: Vec<Option<Lab>> = (0..cols * rows)
            .map(|i| {
                let (x, y) = (i % cols, i / cols);
                let v = if x < 32 {
                    ((x * 37 + y * 11) % 100) as f64
                } else {
                    50.0
                };
                Some([v, 0.0, 0.0])
            })
            .collect();
        let targets = [[0.0; 3], [50.0, 0.0, 0.0], [100.0, 0.0, 0.0]];
        let base = Rows {
            min_run: 2,
            lambda: 10.0,
            strip: 0.0,
            along: 0.0,
        };
        let needles = |d: &[Option<usize>]| {
            let mut n = vec![0u32; d.len()];
            for (i, t) in d.iter().enumerate() {
                if i % cols == 0 || d[i - 1] != *t {
                    n[i] += 1;
                }
            }
            n
        };
        let cap = Cap {
            points: 30.0,
            total: f64::INFINITY,
            window: 16,
            tile: 8,
            rounds: 8,
        };
        let out = fit_cap(
            cols,
            rows,
            cap,
            |field| assign_in(&cells, cols, rows, &targets, &[0, 1, 2], base, field),
            needles,
        );
        let n = needles(&out);
        for y0 in 0..=rows - 16 {
            for x0 in 0..=cols - 16 {
                let pts: u32 = (y0..y0 + 16)
                    .map(|y| n[y * cols + x0..y * cols + x0 + 16].iter().sum::<u32>())
                    .sum();
                assert!(
                    pts <= 30,
                    "{pts} needle points in the window at ({x0}, {y0})"
                );
            }
        }
    }

    #[test]
    fn transparent_cells_split_a_row() {
        let cells = vec![
            Some([0.0; 3]),
            Some([0.0; 3]),
            None,
            Some([100.0, 0.0, 0.0]),
        ];
        let targets = [[0.0; 3], [100.0, 0.0, 0.0]];
        let out = assign(
            &cells,
            4,
            1,
            &targets,
            &[0, 1, 2],
            Rows {
                min_run: 4,
                lambda: 0.0,
                strip: 0.0,
                along: 0.0,
            },
        );
        assert_eq!(out, vec![Some(0), Some(0), None, Some(1)]);
    }

    #[test]
    fn the_density_fit_meets_the_budget() {
        let cols = 64;
        let cells: Vec<Option<Lab>> = (0..cols * 8)
            .map(|i| Some([((i * 37) % 100) as f64, 0.0, 0.0]))
            .collect();
        let targets = [[0.0; 3], [50.0, 0.0, 0.0], [100.0, 0.0, 0.0]];
        let count =
            |l: &[Option<usize>]| l.chunks(cols).map(|row| runs(row).len()).sum::<usize>() as f64;
        let (labels, lambda) = fit_density(40.0, 1e6, 30, |lambda| {
            let l = assign(
                &cells,
                cols,
                8,
                &targets,
                &[0, 1, 2],
                Rows {
                    min_run: 2,
                    lambda,
                    strip: 0.0,
                    along: 0.0,
                },
            );
            let n = count(&l);
            (l, n)
        });
        assert!(lambda > 0.0);
        assert!(count(&labels) <= 40.0);
    }
}
