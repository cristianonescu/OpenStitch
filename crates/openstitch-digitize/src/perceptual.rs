//! Perceptual colour for the "OpenStitch" image mode (docs/research/beyond/image-digitizing.md):
//! sRGB to CIELAB (D65), the CIEDE2000 colour difference, and a weighted k-means in CIELAB.
//! PE-DESIGN mode never uses this module; its colour arithmetic stays in RGB (#119, #127).

use crate::palette::Rgb;

/// CIELAB coordinates (L*, a*, b*), D65 white.
pub use openstitch_core::colour::Lab;
/// sRGB/CIELAB conversions and CIEDE2000, shared with the engine's thread matching (#484).
pub use openstitch_core::colour::{de2000, lab, lab_linear, linear_rgb, rgb};

/// Squared Euclidean distance in CIELAB (the k-means objective).
fn sq(p: Lab, q: Lab) -> f64 {
    (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)
}

/// Weighted k-means (Lloyd) in CIELAB, started from `seeds`: each colour joins its nearest
/// centre, each centre moves to its members' weighted mean, for at most `rounds` rounds or
/// until nothing moves. An empty centre keeps its place. Deterministic.
pub fn kmeans(colors: &[(Lab, f64)], seeds: &[Lab], rounds: usize) -> Vec<Lab> {
    let mut centres = seeds.to_vec();
    let mut owner = vec![usize::MAX; colors.len()];
    for _ in 0..rounds {
        let mut moved = false;
        for (i, (c, _)) in colors.iter().enumerate() {
            let best = nearest(&centres, *c);
            if owner[i] != best {
                owner[i] = best;
                moved = true;
            }
        }
        if !moved {
            break;
        }
        let mut sum = vec![([0.0; 3], 0.0); centres.len()];
        for (i, (c, w)) in colors.iter().enumerate() {
            let s = &mut sum[owner[i]];
            for (total, v) in s.0.iter_mut().zip(c) {
                *total += v * w;
            }
            s.1 += w;
        }
        for (centre, (s, w)) in centres.iter_mut().zip(sum) {
            if w > 0.0 {
                *centre = s.map(|v| v / w);
            }
        }
    }
    centres
}

/// Index of the centre nearest `c` in CIELAB (ties to the lower index).
pub fn nearest(centres: &[Lab], c: Lab) -> usize {
    let mut best = (f64::INFINITY, 0);
    for (i, &m) in centres.iter().enumerate() {
        let d = sq(m, c);
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1
}

/// Thread selection by backward elimination (B16.1 T4): starting from every thread of `chart`,
/// repeatedly drop the thread whose loss raises the colours' weighted CIEDE2000 to their
/// nearest remaining thread least, until `n` remain. A thread no colour is nearest to costs
/// nothing to drop, so unused threads go first; any still unused at the end are left out, so an
/// image with fewer colours than `n` gets fewer threads. Returns the kept indices into `chart`,
/// ascending. Deterministic: costs are summed in the colours' order and ties drop the later
/// chart thread.
///
/// This replaces snapping k-means centres to their nearest thread, where several centres
/// collapse onto one thread (ten asked, seven distinct on the portrait) and a thread is never
/// chosen for how well it serves the image as a whole.
pub fn select_threads(colors: &[(Lab, f64)], chart: &[Lab], n: usize) -> Vec<usize> {
    let m = chart.len();
    if m == 0 {
        return Vec::new();
    }
    let n = n.max(1);
    let dist: Vec<Vec<f64>> = colors
        .iter()
        .map(|(c, _)| chart.iter().map(|&t| de2000(*c, t)).collect())
        .collect();
    let mut alive = vec![true; m];
    let mut left = m;
    // Each colour's nearest and second-nearest live thread (ties to the lower index).
    let rank = |d: &[f64], alive: &[bool]| {
        let mut best = (f64::INFINITY, usize::MAX);
        let mut second = (f64::INFINITY, usize::MAX);
        for (j, &v) in d.iter().enumerate() {
            if !alive[j] {
                continue;
            }
            if v < best.0 {
                second = best;
                best = (v, j);
            } else if v < second.0 {
                second = (v, j);
            }
        }
        (best, second)
    };
    let mut near: Vec<_> = dist.iter().map(|d| rank(d, &alive)).collect();
    while left > n {
        let mut cost = vec![0.0; m];
        for ((best, second), (_, w)) in near.iter().zip(colors) {
            if second.1 != usize::MAX {
                cost[best.1] += w * (second.0 - best.0);
            }
        }
        let mut drop = (f64::INFINITY, 0);
        for j in (0..m).filter(|&j| alive[j]) {
            if cost[j] <= drop.0 {
                drop = (cost[j], j);
            }
        }
        alive[drop.1] = false;
        left -= 1;
        for (k, d) in dist.iter().enumerate() {
            if near[k].0.1 == drop.1 || near[k].1.1 == drop.1 {
                near[k] = rank(d, &alive);
            }
        }
    }
    let mut used = vec![false; m];
    for ((best, _), (_, w)) in near.iter().zip(colors) {
        if *w > 0.0 && best.1 != usize::MAX {
            used[best.1] = true;
        }
    }
    (0..m).filter(|&j| alive[j] && used[j]).collect()
}

/// Most distinct colours [`select_threads`] weighs: a larger colour list is binned on a coarser
/// RGB grid (one bit per channel fewer at a time) until it fits, each bin standing at its
/// members' weighted mean.
pub const SELECT_COLORS: usize = 4096;

/// A colour histogram (sorted by packed RGB) reduced to at most [`SELECT_COLORS`] CIELAB colours
/// with their weights, for [`select_threads`]. Deterministic: bins are visited in packed order.
pub fn select_colors(hist: &[(Rgb, u64)]) -> Vec<(Lab, f64)> {
    let colors: Vec<(Rgb, f64)> = hist.iter().map(|&(c, w)| (c, w as f64)).collect();
    rebin(&colors, SELECT_COLORS)
        .into_iter()
        .map(|(c, w)| (lab(c), w))
        .collect()
}

/// How [`select_threads_by`] measures a colour against the chosen threads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectCost {
    /// CIEDE2000 to the nearest chosen thread ([`select_threads`]): for tools that sew each
    /// area in one thread (Auto Punch, flat areas).
    Nearest,
    /// CIEDE2000 to the best mix of two chosen threads in linear light: for tools whose rows
    /// mix threads by error carry (Photo Stitch 1), so the threads span the image's colours,
    /// extremes included.
    Mix,
}

/// Most candidate threads [`SelectCost::Mix`] weighs: each colour's nearest few threads, the
/// most used first.
pub const MIX_CANDIDATES: usize = 96;
/// Most colours [`SelectCost::Mix`] weighs (the colour list is binned further to fit).
pub const MIX_COLORS: usize = 1024;

/// Chooses at most `n` of `chart`'s threads for `colors` (sRGB with weights) by `cost`:
/// [`SelectCost::Nearest`] is [`select_threads`]; [`SelectCost::Mix`] picks threads greedily,
/// each step the one that lowers the colours' weighted CIEDE2000 to their best two-thread mix
/// most, then swaps chosen threads for others while that lowers it (two passes; skipped when it
/// would take too long). Returns indices into `chart`, ascending. Deterministic: fixed visiting
/// orders, ties to the lower index.
pub fn select_threads_by(
    colors: &[(Rgb, f64)],
    chart: &[Rgb],
    n: usize,
    cost: SelectCost,
) -> Vec<usize> {
    let labs: Vec<Lab> = chart.iter().map(|&c| lab(c)).collect();
    if cost == SelectCost::Nearest {
        let colors: Vec<(Lab, f64)> = colors.iter().map(|&(c, w)| (lab(c), w)).collect();
        return select_threads(&colors, &labs, n);
    }
    let colors = rebin(colors, MIX_COLORS);
    let n = n.max(1);
    if chart.is_empty() || colors.is_empty() {
        return Vec::new();
    }
    // Candidates: each colour's four nearest threads, the most used first, ties by index.
    let mut use_of = vec![0.0; chart.len()];
    for &(c, w) in &colors {
        let l = lab(c);
        let mut d: Vec<(f64, usize)> = labs
            .iter()
            .enumerate()
            .map(|(j, &t)| (de2000(l, t), j))
            .collect();
        d.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for &(_, j) in d.iter().take(4) {
            use_of[j] += w;
        }
    }
    let mut cand: Vec<usize> = (0..chart.len()).filter(|&j| use_of[j] > 0.0).collect();
    cand.sort_by(|&a, &b| use_of[b].total_cmp(&use_of[a]).then(a.cmp(&b)));
    cand.truncate(MIX_CANDIDATES);
    cand.sort_unstable();
    let m = cand.len();
    let lin: Vec<[f64; 3]> = cand.iter().map(|&j| linear_rgb(chart[j])).collect();
    let px: Vec<([f64; 3], Lab, f64)> = colors
        .iter()
        .map(|&(c, w)| (linear_rgb(c), lab(c), w))
        .collect();
    // pair[i * m + j][k]: colour k's CIEDE2000 to the best mix of candidates i and j.
    let mut pair = vec![Vec::new(); m * m];
    for i in 0..m {
        for j in i..m {
            let (a, b) = (lin[i], lin[j]);
            let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            let e: Vec<f64> = px
                .iter()
                .map(|(p, l, _)| {
                    let t = if dd > 0.0 {
                        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1] + (p[2] - a[2]) * d[2]) / dd)
                            .clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    de2000(
                        *l,
                        lab_linear([a[0] + t * d[0], a[1] + t * d[1], a[2] + t * d[2]]),
                    )
                })
                .collect();
            pair[j * m + i] = e.clone();
            pair[i * m + j] = e;
        }
    }
    let total = |set: &[usize]| -> f64 {
        px.iter()
            .enumerate()
            .map(|(k, (_, _, w))| {
                let mut best = f64::INFINITY;
                for (x, &i) in set.iter().enumerate() {
                    for &j in &set[x..] {
                        best = best.min(pair[i * m + j][k]);
                    }
                }
                w * best
            })
            .sum()
    };
    let mut chosen: Vec<usize> = Vec::new();
    let mut best = vec![f64::INFINITY; px.len()];
    for _ in 0..n.min(m) {
        let mut pick = (f64::INFINITY, usize::MAX);
        for c in (0..m).filter(|c| !chosen.contains(c)) {
            let score: f64 = (0..px.len())
                .map(|k| {
                    let mut e = best[k].min(pair[c * m + c][k]);
                    for &j in &chosen {
                        e = e.min(pair[c * m + j][k]);
                    }
                    px[k].2 * e
                })
                .sum();
            if score < pick.0 {
                pick = (score, c);
            }
        }
        let c = pick.1;
        for (k, b) in best.iter_mut().enumerate() {
            *b = b.min(pair[c * m + c][k]);
            for &j in &chosen {
                *b = b.min(pair[c * m + j][k]);
            }
        }
        chosen.push(c);
    }
    let k = chosen.len();
    if k * k * m * px.len() <= 200_000_000 {
        for _ in 0..2 {
            let mut current = total(&chosen);
            for pos in 0..k {
                for c in 0..m {
                    if chosen.contains(&c) {
                        continue;
                    }
                    let mut trial = chosen.clone();
                    trial[pos] = c;
                    let v = total(&trial);
                    if v < current {
                        chosen = trial;
                        current = v;
                    }
                }
            }
        }
    }
    let mut out: Vec<usize> = chosen.into_iter().map(|c| cand[c]).collect();
    out.sort_unstable();
    out
}

/// A weighted sRGB colour list binned on a coarser grid (one bit per channel fewer at a time)
/// until at most `limit` remain, each bin at its members' weighted mean; kept as it is when it
/// already fits. Deterministic: bins in packed order.
fn rebin(colors: &[(Rgb, f64)], limit: usize) -> Vec<(Rgb, f64)> {
    let mut bits = 8;
    loop {
        let shift = 8 - bits;
        let key = |c: Rgb| {
            (u32::from(c[0] >> shift) << 16)
                | (u32::from(c[1] >> shift) << 8)
                | u32::from(c[2] >> shift)
        };
        let mut keyed: Vec<(u32, Rgb, f64)> = colors.iter().map(|&(c, w)| (key(c), c, w)).collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut bins: Vec<(u32, [f64; 3], f64)> = Vec::new();
        for (k, c, w) in keyed {
            match bins.last_mut() {
                Some(b) if b.0 == k => {
                    for (total, &v) in b.1.iter_mut().zip(&c) {
                        *total += f64::from(v) * w;
                    }
                    b.2 += w;
                }
                _ => bins.push((k, c.map(|v| f64::from(v) * w), w)),
            }
        }
        if bins.len() <= limit || bits == 1 {
            return bins
                .into_iter()
                .filter(|b| b.2 > 0.0)
                .map(|(_, s, w)| (s.map(|v| (v / w).round().clamp(0.0, 255.0) as u8), w))
                .collect();
        }
        bits -= 1;
    }
}
