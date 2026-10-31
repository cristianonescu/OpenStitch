//! Rate-constrained fill rows for OpenStitch mode's Photo Stitch 1 (B16.1, plan T2/T3,
//! docs/research/beyond/image-similarity-plan.md).
//!
//! The image is sampled finely along each fill row. Each row becomes the sequence of threads
//! that minimises the squared CIELAB error to the row's colours plus a penalty `λ` for every
//! change of thread, under a minimum run length (so no stitch is shorter than the shortest
//! run), by dynamic programming over (thread, run length so far). A row's remaining error is
//! carried to the next row (¼, ½, ¼), which mixes the threads vertically as the rows sit
//! closer than the eye resolves. Every change of thread costs needle points, so `λ` trades
//! likeness against needle density; [`fit_density`] sets it by bisection to meet a density.
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
}

/// Overlap of a `thread` wide thread over rows `spacing` apart, as [`Rows::strip`].
pub fn strip(thread: f64, spacing: f64) -> f64 {
    ((thread - spacing) / 2.0 / spacing).clamp(0.0, 0.5)
}

/// Each opaque cell's target index, `None` for transparent cells. `cells` is `cols`×`rows`,
/// row-major, in CIELAB; `targets` are the thread (and fabric) colours in CIELAB.
pub fn assign(
    cells: &[Option<Lab>],
    cols: usize,
    rows: usize,
    targets: &[Lab],
    rank: &[usize],
    r: Rows,
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
            let start = |_: usize, _: usize| r.lambda;
            let seq = dp.solve(&want[a..x], &start, &seen);
            for (i, &t) in seq.iter().enumerate() {
                out[y * cols + a + i] = Some(t);
                let s = seen(i, t);
                err[a + i] = [0, 1, 2].map(|k| want[a + i][k] - s[k]);
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
        }
    }

    /// `start(i, t)`: the penalty for a new run of `t` at position `i`; `seen(i, t)`: the colour
    /// target `t` shows there.
    fn solve(
        &mut self,
        want: &[Lab],
        start: &dyn Fn(usize, usize) -> f64,
        seen: &dyn Fn(usize, usize) -> Lab,
    ) -> Vec<usize> {
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
            return vec![best.1; n];
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
            self.cost[idx(t, 1)] = sq(want[0], seen(0, t));
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
                let here = sq(w, seen(x, t));
                let sw = if b1.1 != t { b1 } else { b2 };
                let new_run = sw.0 + start(x, t);
                self.next[idx(t, 1)] = new_run + here;
                self.from[x * k + t] = sw.1.min(255) as u8;
                for r in 2..m {
                    self.next[idx(t, r)] = self.cost[idx(t, r - 1)] + here;
                }
                if m >= 2 {
                    let (grow, stay) = (self.cost[idx(t, m - 1)], self.cost[idx(t, m)]);
                    let s = stay < grow;
                    self.next[idx(t, m)] = if s { stay } else { grow } + here;
                    self.stayed[x * k + t] = s;
                } else {
                    // m == 1: the only state is "a run of one or more".
                    let stay = self.cost[idx(t, 1)];
                    let s = stay <= new_run;
                    self.next[idx(t, 1)] = if s { stay } else { new_run } + here;
                    self.stayed[x * k + t] = s;
                }
            }
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
        seq
    }
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
            },
        ));
        assert!(high.len() < low.len());
        assert_eq!(high.len(), 1);
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
                },
            );
            let n = count(&l);
            (l, n)
        });
        assert!(lambda > 0.0);
        assert!(count(&labels) <= 40.0);
    }
}
