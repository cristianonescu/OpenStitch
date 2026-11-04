//! Subject isolation by an iterated graph cut (#127's optional foreground step), used by Auto
//! Punch's OpenStitch mode with `isolateSubject`.
//!
//! The method is GrabCut's (Rother, Kolmogorov and Blake, "GrabCut: interactive foreground
//! extraction using iterated graph cuts", SIGGRAPH 2004): a colour Gaussian mixture for each
//! side, a min-cut on the pixel grid between them, repeated until the cut settles. The min-cut
//! is Boykov and Kolmogorov's max-flow ("An experimental comparison of min-cut/max-flow
//! algorithms for energy minimization in vision", PAMI 2004). Both are written here from the
//! papers; no other implementation's code is used.
//!
//! Deterministic on every OS: integer capacities, a fixed visiting order, and only
//! `fmath::Det`'s `dexp` / `dln`, `sqrt` and basic arithmetic.

use crate::palette::Rgb;
use openstitch_design::fmath::Det;
use std::collections::VecDeque;

/// Gaussian components per side.
pub const COMPONENTS: usize = 5;
/// Most cut-and-refit rounds.
pub const ITERATIONS: usize = 8;
/// Smoothness weight γ (GrabCut's 50).
pub const GAMMA: f64 = 50.0;
/// The default band of certain background, as a share of the image's shorter side.
pub const BORDER_SHARE: f64 = 0.02;
/// Ridge added to each covariance's diagonal, RGB units², so flat colours stay invertible.
const RIDGE: f64 = 1.0;
/// Data terms are clamped to this range, nats.
const DATA_MIN: f64 = -20.0;
const DATA_MAX: f64 = 80.0;
/// Capacities are nats times this, rounded.
const SCALE: f64 = 64.0;
/// Terminal capacity of a pixel that is background for certain.
const HARD: i64 = 1 << 40;

/// The region outside which every pixel is background for certain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seed {
    /// A band around the image border, [`BORDER_SHARE`] of the shorter side, at least 2 px.
    Border,
    /// A rectangle `[left, top, right, bottom]` in fractions of the image (0–1).
    Rect([f64; 4]),
}

/// The subject's pixels (`true`), from the modal colour of every working pixel (`None` where
/// transparent, which is always background). `None` when the seed leaves nothing unknown or the
/// cut finds no subject.
pub fn isolate(colors: &[Option<Rgb>], w: usize, h: usize, seed: Seed) -> Option<Vec<bool>> {
    let n = w * h;
    if n == 0 || colors.len() != n {
        return None;
    }
    let (x0, y0, x1, y1) = match seed {
        Seed::Border => {
            let b = ((w.min(h) as f64 * BORDER_SHARE).round() as usize).max(2);
            (b, b, w.saturating_sub(b), h.saturating_sub(b))
        }
        Seed::Rect([l, t, r, b]) => {
            let fx = |f: f64| ((f.clamp(0.0, 1.0) * w as f64).round() as usize).min(w);
            let fy = |f: f64| ((f.clamp(0.0, 1.0) * h as f64).round() as usize).min(h);
            (fx(l.min(r)), fy(t.min(b)), fx(l.max(r)), fy(t.max(b)))
        }
    };
    let z: Vec<[f64; 3]> = colors
        .iter()
        .map(|c| c.map_or([0.0; 3], |c| [c[0] as f64, c[1] as f64, c[2] as f64]))
        .collect();
    // Background for certain: outside the seed, or transparent.
    let hard: Vec<bool> = (0..n)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            colors[i].is_none() || x < x0 || x >= x1 || y < y0 || y >= y1
        })
        .collect();
    if hard.iter().all(|&b| b) || !hard.iter().zip(colors).any(|(&b, c)| b && c.is_some()) {
        // Nothing to decide, or no background colour to learn from.
        return None;
    }
    let links = smoothness(&z, colors, w, h);
    let mut fg: Vec<bool> = hard.iter().map(|&b| !b).collect();
    let mut models: Option<(Gmm, Gmm)> = None;
    for _ in 0..ITERATIONS {
        let opaque = |i: usize, side: bool| colors[i].is_some() && fg[i] == side;
        let fg_px: Vec<usize> = (0..n).filter(|&i| opaque(i, true)).collect();
        let bg_px: Vec<usize> = (0..n).filter(|&i| opaque(i, false)).collect();
        if fg_px.is_empty() || bg_px.is_empty() {
            break;
        }
        let (f, b) = match &models {
            None => (Gmm::split(&z, &fg_px), Gmm::split(&z, &bg_px)),
            Some((f, b)) => (f.refit(&z, &fg_px), b.refit(&z, &bg_px)),
        };
        let mut g = Grid::new(w, h);
        g.cap.copy_from_slice(&links);
        for i in 0..n {
            g.tr[i] = if hard[i] {
                -HARD
            } else {
                // Source side is the subject: a pixel there pays its subject data term (its
                // sink link is cut), one on the sink side its background term.
                let d_fg = f.data(z[i]);
                let d_bg = b.data(z[i]);
                ((d_bg - d_fg) * SCALE).round() as i64
            };
        }
        let next = g.max_flow();
        let changed = next != fg;
        fg = next;
        models = Some((f, b));
        if !changed {
            break;
        }
    }
    fg.iter().any(|&f| f).then_some(fg)
}

/// The 8 neighbour directions; `(d + 4) % 8` is the opposite one.
const DIRS: [(isize, isize); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

fn neighbour(i: usize, d: usize, w: usize, h: usize) -> Option<usize> {
    let (x, y) = ((i % w) as isize + DIRS[d].0, (i / w) as isize + DIRS[d].1);
    (x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h).then(|| y as usize * w + x as usize)
}

fn sq(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Smoothness capacities per pixel and direction: γ · exp(−β ‖zₚ − z_q‖²) / distance, with
/// β = 1 / (2 · mean squared neighbour difference); 0 off the grid or next to a transparent
/// pixel.
fn smoothness(z: &[[f64; 3]], colors: &[Option<Rgb>], w: usize, h: usize) -> Vec<i32> {
    let n = w * h;
    let (mut sum, mut count) = (0.0, 0u64);
    for i in 0..n {
        for d in 0..4 {
            if let Some(j) = neighbour(i, d, w, h)
                && colors[i].is_some()
                && colors[j].is_some()
            {
                sum += sq(z[i], z[j]);
                count += 1;
            }
        }
    }
    let beta = if sum > 0.0 {
        count as f64 / (2.0 * sum)
    } else {
        0.0
    };
    let diag = std::f64::consts::FRAC_1_SQRT_2;
    let mut cap = vec![0i32; n * 8];
    for i in 0..n {
        for d in 0..8 {
            if let Some(j) = neighbour(i, d, w, h)
                && colors[i].is_some()
                && colors[j].is_some()
            {
                let dist = if d % 2 == 1 { diag } else { 1.0 };
                let v = GAMMA * dist * (-beta * sq(z[i], z[j])).dexp();
                cap[i * 8 + d] = (v * SCALE).round() as i32;
            }
        }
    }
    cap
}

/// One Gaussian component: weight, mean, inverse covariance and ln of its determinant.
#[derive(Clone, Debug)]
struct Gauss {
    weight: f64,
    mean: [f64; 3],
    inv: [[f64; 3]; 3],
    ln_det: f64,
}

#[derive(Clone, Debug)]
struct Gmm(Vec<Gauss>);

/// Mean and covariance (with the ridge) of the pixels `px`.
fn moments(z: &[[f64; 3]], px: &[usize]) -> ([f64; 3], [[f64; 3]; 3]) {
    let k = px.len().max(1) as f64;
    let mut m = [0.0; 3];
    for &i in px {
        for c in 0..3 {
            m[c] += z[i][c];
        }
    }
    m.iter_mut().for_each(|v| *v /= k);
    let mut s = [[0.0; 3]; 3];
    for &i in px {
        let d = [z[i][0] - m[0], z[i][1] - m[1], z[i][2] - m[2]];
        for a in 0..3 {
            for b in 0..3 {
                s[a][b] += d[a] * d[b];
            }
        }
    }
    for (a, row) in s.iter_mut().enumerate() {
        for v in row.iter_mut() {
            *v /= k;
        }
        row[a] += RIDGE;
    }
    (m, s)
}

fn gauss(z: &[[f64; 3]], px: &[usize], total: usize) -> Gauss {
    let (mean, s) = moments(z, px);
    let det = s[0][0] * (s[1][1] * s[2][2] - s[1][2] * s[2][1])
        - s[0][1] * (s[1][0] * s[2][2] - s[1][2] * s[2][0])
        + s[0][2] * (s[1][0] * s[2][1] - s[1][1] * s[2][0]);
    let det = det.max(1e-9);
    let mut inv = [[0.0; 3]; 3];
    for (a, row) in inv.iter_mut().enumerate() {
        for (b, v) in row.iter_mut().enumerate() {
            // Cofactor of s[b][a] over the determinant.
            let (r0, r1) = ((b + 1) % 3, (b + 2) % 3);
            let (c0, c1) = ((a + 1) % 3, (a + 2) % 3);
            *v = (s[r0][c0] * s[r1][c1] - s[r0][c1] * s[r1][c0]) / det;
        }
    }
    Gauss {
        weight: px.len() as f64 / total.max(1) as f64,
        mean,
        inv,
        ln_det: det.dln(),
    }
}

impl Gauss {
    /// ln of the weighted density, without the shared (2π)^(−3/2).
    fn ln_p(&self, z: [f64; 3]) -> f64 {
        let d = [
            z[0] - self.mean[0],
            z[1] - self.mean[1],
            z[2] - self.mean[2],
        ];
        let mut m = 0.0;
        for a in 0..3 {
            for b in 0..3 {
                m += d[a] * self.inv[a][b] * d[b];
            }
        }
        self.weight.dln() - 0.5 * self.ln_det - 0.5 * m
    }
}

impl Gmm {
    /// Orchard–Bouman splitting: start with one cluster and split the one whose covariance has
    /// the largest eigenvalue across its mean, along that eigenvector, until there are
    /// [`COMPONENTS`].
    fn split(z: &[[f64; 3]], px: &[usize]) -> Gmm {
        let mut clusters: Vec<Vec<usize>> = vec![px.to_vec()];
        while clusters.len() < COMPONENTS {
            let mut best: Option<(usize, f64, [f64; 3], [f64; 3])> = None;
            for (c, members) in clusters.iter().enumerate() {
                if members.len() < 2 {
                    continue;
                }
                let (m, s) = moments(z, members);
                let (l, v) = principal(s);
                if l > RIDGE + 1e-9 && best.is_none_or(|b| l > b.1) {
                    best = Some((c, l, m, v));
                }
            }
            let Some((c, _, m, v)) = best else { break };
            let (a, b): (Vec<usize>, Vec<usize>) = clusters[c].iter().partition(|&&i| {
                (z[i][0] - m[0]) * v[0] + (z[i][1] - m[1]) * v[1] + (z[i][2] - m[2]) * v[2] <= 0.0
            });
            if a.is_empty() || b.is_empty() {
                break;
            }
            clusters[c] = a;
            clusters.push(b);
        }
        Gmm(clusters.iter().map(|c| gauss(z, c, px.len())).collect())
    }

    /// Gives each pixel its likeliest component, then refits every component that kept pixels.
    fn refit(&self, z: &[[f64; 3]], px: &[usize]) -> Gmm {
        let mut members = vec![Vec::new(); self.0.len()];
        for &i in px {
            let mut best = (0, f64::NEG_INFINITY);
            for (k, g) in self.0.iter().enumerate() {
                let l = g.ln_p(z[i]);
                if l > best.1 {
                    best = (k, l);
                }
            }
            members[best.0].push(i);
        }
        Gmm(members
            .iter()
            .filter(|m| !m.is_empty())
            .map(|m| gauss(z, m, px.len()))
            .collect())
    }

    /// −ln of the mixture's density at `z` (log-sum-exp), clamped.
    fn data(&self, z: [f64; 3]) -> f64 {
        let a: Vec<f64> = self.0.iter().map(|g| g.ln_p(z)).collect();
        let top = a.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if !top.is_finite() {
            return DATA_MAX;
        }
        let s: f64 = a.iter().map(|v| (v - top).dexp()).sum();
        (-(top + s.dln())).clamp(DATA_MIN, DATA_MAX)
    }
}

/// The largest eigenvalue of a symmetric 3×3 matrix and its unit eigenvector, by a fixed
/// 20-step power iteration from (1, 1, 1).
fn principal(s: [[f64; 3]; 3]) -> (f64, [f64; 3]) {
    let mut v = [1.0, 1.0, 1.0];
    let mut l = 0.0;
    for _ in 0..20 {
        let u = [
            s[0][0] * v[0] + s[0][1] * v[1] + s[0][2] * v[2],
            s[1][0] * v[0] + s[1][1] * v[1] + s[1][2] * v[2],
            s[2][0] * v[0] + s[2][1] * v[1] + s[2][2] * v[2],
        ];
        let norm = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
        if norm == 0.0 {
            return (0.0, [1.0, 0.0, 0.0]);
        }
        v = [u[0] / norm, u[1] / norm, u[2] / norm];
        l = norm;
    }
    (l, v)
}

const FREE: u8 = 0;
const SOURCE: u8 = 1;
const SINK: u8 = 2;
/// Parent markers beyond the 8 directions.
const TERMINAL: u8 = 8;
const ORPHAN: u8 = 9;
const NO_PARENT: u8 = 10;

/// The pixel grid as a flow network: residual capacity per pixel and direction, and each
/// pixel's terminal residual (positive: from the source, negative: to the sink).
struct Grid {
    w: usize,
    h: usize,
    cap: Vec<i32>,
    tr: Vec<i64>,
    tree: Vec<u8>,
    parent: Vec<u8>,
    ts: Vec<u32>,
    dist: Vec<u32>,
    time: u32,
}

impl Grid {
    fn new(w: usize, h: usize) -> Grid {
        let n = w * h;
        Grid {
            w,
            h,
            cap: vec![0; n * 8],
            tr: vec![0; n],
            tree: vec![FREE; n],
            parent: vec![NO_PARENT; n],
            ts: vec![0; n],
            dist: vec![0; n],
            time: 0,
        }
    }

    fn next(&self, i: usize, d: u8) -> usize {
        neighbour(i, d as usize, self.w, self.h).expect("parent link stays on the grid")
    }

    /// Residual capacity from `i` to its neighbour in direction `d`.
    fn arc(&self, i: usize, d: usize) -> i32 {
        self.cap[i * 8 + d]
    }

    /// Boykov–Kolmogorov max-flow. Returns the source side (`true`) of the minimum cut.
    fn max_flow(&mut self) -> Vec<bool> {
        let n = self.w * self.h;
        let mut active: VecDeque<usize> = VecDeque::new();
        let mut queued = vec![false; n];
        for (i, q) in queued.iter_mut().enumerate() {
            if self.tr[i] != 0 {
                self.tree[i] = if self.tr[i] > 0 { SOURCE } else { SINK };
                self.parent[i] = TERMINAL;
                self.dist[i] = 1;
                active.push_back(i);
                *q = true;
            }
        }
        let mut orphans: VecDeque<usize> = VecDeque::new();
        while let Some(p) = active.pop_front() {
            queued[p] = false;
            if self.tree[p] == FREE {
                continue;
            }
            // Growth: claim free neighbours until the two trees touch.
            let mut meet = None;
            for d in 0..8 {
                let Some(q) = neighbour(p, d, self.w, self.h) else {
                    continue;
                };
                let opp = (d + 4) % 8;
                let residual = if self.tree[p] == SOURCE {
                    self.arc(p, d)
                } else {
                    self.arc(q, opp)
                };
                if residual <= 0 {
                    continue;
                }
                if self.tree[q] == FREE {
                    self.tree[q] = self.tree[p];
                    self.parent[q] = opp as u8;
                    self.ts[q] = self.ts[p];
                    self.dist[q] = self.dist[p] + 1;
                    if !queued[q] {
                        active.push_back(q);
                        queued[q] = true;
                    }
                } else if self.tree[q] != self.tree[p] {
                    meet = Some(if self.tree[p] == SOURCE {
                        (p, d, q)
                    } else {
                        (q, opp, p)
                    });
                    break;
                }
            }
            let Some((a, d, b)) = meet else {
                continue;
            };
            // p may still have work: it stays active.
            active.push_front(p);
            queued[p] = true;
            self.time += 1;
            self.augment(a, d, b, &mut orphans);
            self.adopt(&mut orphans, &mut active, &mut queued);
        }
        (0..n).map(|i| self.tree[i] == SOURCE).collect()
    }

    /// Pushes the bottleneck along source → … → a → b → … → sink.
    fn augment(&mut self, a: usize, d: usize, b: usize, orphans: &mut VecDeque<usize>) {
        let mut flow = self.arc(a, d) as i64;
        // Source side: from a up to the source, along parent → child arcs.
        let mut i = a;
        while self.parent[i] != TERMINAL {
            let pd = self.parent[i];
            let j = self.next(i, pd);
            flow = flow.min(self.arc(j, (pd as usize + 4) % 8) as i64);
            i = j;
        }
        flow = flow.min(self.tr[i]);
        // Sink side: from b down to the sink, along child → parent arcs.
        let mut i = b;
        while self.parent[i] != TERMINAL {
            let pd = self.parent[i];
            flow = flow.min(self.arc(i, pd as usize) as i64);
            i = self.next(i, pd);
        }
        flow = flow.min(-self.tr[i]);
        let f = flow as i32;
        self.cap[a * 8 + d] -= f;
        self.cap[b * 8 + (d + 4) % 8] += f;
        let mut i = a;
        while self.parent[i] != TERMINAL {
            let pd = self.parent[i] as usize;
            let j = self.next(i, pd as u8);
            self.cap[j * 8 + (pd + 4) % 8] -= f;
            self.cap[i * 8 + pd] += f;
            if self.cap[j * 8 + (pd + 4) % 8] == 0 {
                self.parent[i] = ORPHAN;
                orphans.push_back(i);
            }
            i = j;
        }
        self.tr[i] -= flow;
        if self.tr[i] == 0 {
            self.parent[i] = ORPHAN;
            orphans.push_back(i);
        }
        let mut i = b;
        while self.parent[i] != TERMINAL {
            let pd = self.parent[i] as usize;
            let j = self.next(i, pd as u8);
            self.cap[i * 8 + pd] -= f;
            self.cap[j * 8 + (pd + 4) % 8] += f;
            if self.cap[i * 8 + pd] == 0 {
                self.parent[i] = ORPHAN;
                orphans.push_back(i);
            }
            i = j;
        }
        self.tr[i] += flow;
        if self.tr[i] == 0 {
            self.parent[i] = ORPHAN;
            orphans.push_back(i);
        }
    }

    /// Whether the arc between `q` and the orphan `p` (direction `d` from `p` to `q`) can carry
    /// flow along `q`'s tree.
    fn feeds(&self, p: usize, d: usize, q: usize) -> bool {
        if self.tree[p] == SOURCE {
            self.arc(q, (d + 4) % 8) > 0
        } else {
            self.arc(p, d) > 0
        }
    }

    /// Adoption: each orphan takes the nearest valid parent in its tree, or leaves it.
    fn adopt(
        &mut self,
        orphans: &mut VecDeque<usize>,
        active: &mut VecDeque<usize>,
        queued: &mut [bool],
    ) {
        while let Some(p) = orphans.pop_front() {
            let mut best: Option<(u8, u32)> = None;
            for d in 0..8 {
                let Some(q) = neighbour(p, d, self.w, self.h) else {
                    continue;
                };
                if self.tree[q] != self.tree[p] || !self.feeds(p, d, q) {
                    continue;
                }
                // Walk up from q to its root; it is valid when it reaches a terminal.
                let mut j = q;
                let mut steps = 0u32;
                let found = loop {
                    if self.ts[j] == self.time {
                        break Some(steps + self.dist[j]);
                    }
                    let pj = self.parent[j];
                    steps += 1;
                    if pj == TERMINAL {
                        self.ts[j] = self.time;
                        self.dist[j] = 1;
                        break Some(steps);
                    }
                    if pj == ORPHAN || pj == NO_PARENT {
                        break None;
                    }
                    j = self.next(j, pj);
                };
                if let Some(mut dq) = found {
                    if best.is_none_or(|b| dq < b.1) {
                        best = Some((d as u8, dq));
                    }
                    // Mark the walked path with its distances.
                    let mut j = q;
                    while self.ts[j] != self.time {
                        self.ts[j] = self.time;
                        self.dist[j] = dq;
                        dq = dq.saturating_sub(1);
                        j = self.next(j, self.parent[j]);
                    }
                }
            }
            if let Some((d, dq)) = best {
                self.parent[p] = d;
                self.ts[p] = self.time;
                self.dist[p] = dq + 1;
                continue;
            }
            // No parent: p leaves its tree; its neighbours that could feed it become active,
            // and its children orphans.
            for d in 0..8 {
                let Some(q) = neighbour(p, d, self.w, self.h) else {
                    continue;
                };
                if self.tree[q] != self.tree[p] {
                    continue;
                }
                if self.feeds(p, d, q) && !queued[q] {
                    active.push_back(q);
                    queued[q] = true;
                }
                let pq = self.parent[q];
                if pq < TERMINAL && self.next(q, pq) == p {
                    self.parent[q] = ORPHAN;
                    orphans.push_back(q);
                }
            }
            self.tree[p] = FREE;
            self.parent[p] = NO_PARENT;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Max-flow on a tiny grid equals the cut found by enumerating every labelling.
    #[test]
    fn max_flow_finds_the_minimum_cut() {
        let (w, h) = (3, 3);
        let mut seed = 12345u64;
        for _ in 0..200 {
            let mut g = Grid::new(w, h);
            let mut rnd = |m: i64| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((seed >> 33) as i64) % m
            };
            for i in 0..w * h {
                g.tr[i] = rnd(21) - 10;
                for d in 0..8 {
                    if neighbour(i, d, w, h).is_some() {
                        g.cap[i * 8 + d] = rnd(6) as i32;
                    }
                }
            }
            let (cap0, tr0) = (g.cap.clone(), g.tr.clone());
            let cost = |s: &[bool]| -> i64 {
                let mut c = 0;
                for i in 0..w * h {
                    // Source-side pays the sink link, sink-side the source link.
                    c += if s[i] {
                        (-tr0[i]).max(0)
                    } else {
                        tr0[i].max(0)
                    };
                    for d in 0..8 {
                        if let Some(j) = neighbour(i, d, w, h)
                            && s[i]
                            && !s[j]
                        {
                            c += cap0[i * 8 + d] as i64;
                        }
                    }
                }
                c
            };
            let best = (0..1u32 << (w * h))
                .map(|m| cost(&(0..w * h).map(|i| m >> i & 1 == 1).collect::<Vec<_>>()))
                .min()
                .expect("labellings");
            let cut = g.max_flow();
            assert_eq!(cost(&cut), best);
        }
    }
}
