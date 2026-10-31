//! Photo Stitch 2 as PE-DESIGN 11.50 sews it: a line screen (#129).
//!
//! Each thread sews one set of parallel lines across the whole image, `interval` apart, at its
//! own fixed screen angle (0°, 45°, 90°, 135° in sewing order, turned by the Direction setting).
//! Where the thread's share of the local colour is near zero its line is a straight running
//! stitch; where the share is higher the line becomes a zigzag across it, 90 % of the interval
//! deep, whose stitches come closer together as the share grows, up to Max. Density. A thread's
//! lines are sewn end to end, serpentine, with one jump in.
//!
//! Mono shares follow PE's tone ramp ([`mono_share`]); a named Color Set
//! (`photo::ColorSet`) gives four fixed threads in its order.
//!
//! The angles are a fixed set, the same over the whole image: there is no per-pixel direction
//! field and no per-connector coverage or colour test (docs/research/beyond/patents.md). The
//! measurements behind the numbers are in docs/research/photo-stitch-2-halftone.md (round-4
//! capture `photostitch2-color-owner-shapes.pes`).

use crate::raster::{Frame, Raster};
use openstitch_core::{Command, Stitch};
use openstitch_design::fmath::Det;

/// Coverage grid cell side, mm.
pub const GRID_MM: f64 = 0.5;
/// Screen angles in sewing order, degrees counter-clockwise on screen; a fifth thread and later
/// ones take the same angles shifted by 22.5°.
pub const ANGLES: [f64; 4] = [0.0, 45.0, 90.0, 135.0];
/// Zigzag depth as a share of the line interval (PE-DESIGN: 1.8 mm on a 2.0 mm interval).
pub const DEPTH_SHARE: f64 = 0.9;
/// Below this share of the local colour a thread's line is a straight running stitch.
pub const MIN_SHARE: f64 = 0.08;
/// Needle points per mm of line at full share, per line/mm of Max. Density: PE-DESIGN sews
/// about 6 per mm at 4.5 lines/mm (≈ 300 needle points per cm² in a thread's own colour area
/// on a 2.0 mm interval).
pub const POINTS_PER_DENSITY: f64 = 4.0 / 3.0;

/// Mono: darkness below this sews only the straight run (PE-DESIGN's Photo Stitch 2 Mono on
/// the round-4 grey capture: no zigzag in the band below 17 % darkness).
pub const MONO_LIGHT: f64 = 0.17;
/// Mono: darkness from which the zigzag is at full density.
pub const MONO_FULL: f64 = 0.86;

/// The mono thread's share for a darkness 0–1: a linear ramp from [`MONO_LIGHT`] to
/// [`MONO_FULL`]. Fitted to PE-DESIGN's Photo Stitch 2 Mono on the round-4 grey capture
/// (`photostitch2-mono-grey-tonal.pes`): 4671 stitches against its 4665, every grey band's
/// needle density within 6 % of PE's but 150–179 (59 against 72 per cm²).
pub fn mono_share(darkness: f64) -> f64 {
    ((darkness - MONO_LIGHT) / (MONO_FULL - MONO_LIGHT)).clamp(0.0, 1.0)
}

/// The screen angle of the thread sewn `layer`-th, turned by `direction`.
pub fn angle(layer: usize, direction: f64) -> f64 {
    let base = ANGLES[layer % ANGLES.len()] + 22.5 * (layer / ANGLES.len()) as f64;
    (base + direction).rem_euclid(180.0)
}

/// Each thread's share of the local colour on a grid over the image (`None` where the image is
/// transparent).
pub struct Shares {
    cols: usize,
    rows: usize,
    /// Cell side in design units.
    cell: f64,
    threads: usize,
    /// `cols × rows` cells of `threads` shares each.
    share: Vec<Option<Vec<f64>>>,
}

impl Shares {
    /// Colour mode: each cell's colour is matched as the fabric plus a non-negative mix of the
    /// threads (shares summing to at most 1), by least squares.
    pub fn colour(image: &Raster, frame: &Frame, threads: &[[f64; 3]], fabric: [f64; 3]) -> Self {
        Self::colour_in(image, frame, threads, fabric, false)
    }

    /// [`Shares::colour`], or OpenStitch mode's when `linear`: the mix is solved in linear
    /// light (thread and fabric seen side by side average their light, not their sRGB values)
    /// and the shares are not raised to PE-DESIGN's full coverage ([`saturate`]).
    pub fn colour_in(
        image: &Raster,
        frame: &Frame,
        threads: &[[f64; 3]],
        fabric: [f64; 3],
        linear: bool,
    ) -> Self {
        let lin = |c: [f64; 3]| -> [f64; 3] {
            if linear {
                c.map(|v| {
                    let v = v / 255.0;
                    255.0
                        * if v <= 0.04045 {
                            v / 12.92
                        } else {
                            ((v + 0.055) / 1.055).dpowf(2.4)
                        }
                })
            } else {
                c
            }
        };
        let lthreads: Vec<[f64; 3]> = threads.iter().map(|&t| lin(t)).collect();
        let lfabric = lin(fabric);
        let mut cache: Vec<([u8; 3], Vec<f64>)> = Vec::new();
        Self::build(image, frame, threads.len(), |rgb| {
            if let Some((_, s)) = cache.iter().find(|(c, _)| *c == rgb) {
                return s.clone();
            }
            let m = mix(lin(rgb.map(f64::from)), &lthreads, lfabric);
            // PE-DESIGN's full coverage for saturated colours is its own habit; OpenStitch
            // mode keeps the mix as solved.
            let s = if linear { m } else { saturate(rgb, m, threads) };
            if cache.len() < 4096 {
                cache.push((rgb, s.clone()));
            }
            s
        })
    }

    /// Mono: the one thread's share is the cell's darkness after the full-range contrast
    /// stretch Photo Stitch's mono mode uses (`photo::auto_levels`).
    pub fn mono(image: &Raster, frame: &Frame) -> Self {
        let mut s = Self::build(image, frame, 1, |rgb| {
            vec![f64::from(crate::raster::luma601(rgb[0], rgb[1], rgb[2]))]
        });
        let mut greys: Vec<u8> = s.share.iter().flatten().map(|v| v[0] as u8).collect();
        crate::photo::auto_levels(&mut greys);
        for (cell, g) in s.share.iter_mut().flatten().zip(greys) {
            cell[0] = mono_share(1.0 - f64::from(g) / 255.0);
        }
        s
    }

    fn build(
        image: &Raster,
        frame: &Frame,
        threads: usize,
        mut f: impl FnMut([u8; 3]) -> Vec<f64>,
    ) -> Self {
        let width = f64::from(image.width()) * frame.scale;
        let height = f64::from(image.height()) * frame.scale;
        let mut cell = GRID_MM * 10.0;
        // Never more grid cells than the conversion's sample limit.
        while (width / cell).ceil() * (height / cell).ceil() > crate::photo::MAX_SAMPLES as f64 {
            cell *= 1.5;
        }
        let cols = ((width / cell).ceil() as usize).max(1);
        let rows = ((height / cell).ceil() as usize).max(1);
        let grid = image.resample(cols as u32, rows as u32);
        let mut share = Vec::with_capacity(cols * rows);
        for y in 0..rows as u32 {
            for x in 0..cols as u32 {
                let [r, g, b, a] = grid.pixel(x, y);
                share.push((a >= 128).then(|| f([r, g, b])));
            }
        }
        Shares {
            cols,
            rows,
            cell: width / cols as f64,
            threads,
            share,
        }
    }

    /// OpenStitch mode (B16.1 T13): each thread's *coverage* of each cell, for threads sewn in
    /// `order` (later layers lie on top). A layer covering `c_k` of a cell shows where no later
    /// layer covers it, so the cell looks like
    /// `fabric + Σ_k c_k Π_{j later} (1 − c_j) (thread_k − fabric)` in linear light. Every line
    /// is sewn at least as a straight run, which covers [`Screen::straight`] of the cell, so
    /// `c_k` is at least that. The coverages minimise the difference from the cell's colour in
    /// cube-root (lightness-like) linear light by coordinate descent (a coarse then a fine grid
    /// per layer, top layer first, three sweeps), with the needle points they need held to `cap`
    /// per mm² ([`Screen::points`]).
    pub fn layered(
        image: &Raster,
        frame: &Frame,
        threads: &[[f64; 3]],
        fabric: [f64; 3],
        order: &[usize],
        s: &Screen,
        cap: f64,
    ) -> Self {
        let lin = |c: [f64; 3]| c.map(|v| linear_of(v / 255.0));
        let lt: Vec<[f64; 3]> = threads.iter().map(|&t| lin(t)).collect();
        let lf = lin(fabric);
        let floor = s.straight();
        let n = threads.len();
        let mut cache: Vec<([u8; 3], Vec<f64>)> = Vec::new();
        Self::build(image, frame, n, |rgb| {
            if let Some((_, c)) = cache.iter().find(|(k, _)| *k == rgb) {
                return c.clone();
            }
            let want = lin(rgb.map(f64::from)).map(|v| v.dcbrt());
            let cost = |c: &[f64]| {
                let mut col = lf;
                // Bottom layer first: each later layer covers `c` of what is below it.
                for &k in order {
                    for q in 0..3 {
                        col[q] += c[k] * (lt[k][q] - col[q]);
                    }
                }
                let e: f64 = (0..3).map(|q| (col[q].max(0.0).dcbrt() - want[q]).powi(2)).sum();
                let over = (s.points(c) - cap).max(0.0);
                e + 10.0 * over
            };
            let mut c = vec![floor; n];
            let top = MAX_COVER;
            // Top layer first: what lies on top decides most, and a lower layer then spends
            // needle points only where it still shows.
            for _ in 0..3 {
                for &k in order.iter().rev() {
                    let mut best = (cost(&c), c[k]);
                    for i in 0..=20 {
                        c[k] = floor + (top - floor) * f64::from(i) / 20.0;
                        let v = cost(&c);
                        if v < best.0 - 1e-12 {
                            best = (v, c[k]);
                        }
                    }
                    let mid = best.1;
                    for i in -4..=4 {
                        c[k] = (mid + f64::from(i) * (top - floor) / 160.0).clamp(floor, top);
                        let v = cost(&c);
                        if v < best.0 - 1e-12 {
                            best = (v, c[k]);
                        }
                    }
                    c[k] = best.1;
                }
            }
            if cache.len() < 4096 {
                cache.push((rgb, c.clone()));
            }
            c
        })
    }

    /// Thread `t`'s total share over the image (how much of it the screen sews).
    pub fn usage(&self, t: usize) -> f64 {
        self.share
            .iter()
            .flatten()
            .map(|s| s.get(t).copied().unwrap_or(0.0))
            .sum()
    }

    /// Thread `t`'s share at a point relative to the image's top-left corner (design units),
    /// `None` outside the image or where it is transparent.
    fn at(&self, x: f64, y: f64, t: usize) -> Option<f64> {
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let (cx, cy) = ((x / self.cell) as usize, (y / self.cell) as usize);
        if cx >= self.cols || cy >= self.rows || t >= self.threads {
            return None;
        }
        self.share[cy * self.cols + cx].as_ref().map(|s| s[t])
    }
}

/// HSV saturation of a colour, 0–1.
fn saturation(c: [f64; 3]) -> f64 {
    let (max, min) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    if max <= 0.0 { 0.0 } else { (max - min) / max }
}

/// Saturation from which a colour's chromatic threads sum to full coverage.
pub const FULL_SATURATION: f64 = 0.8;

/// PE-DESIGN's coverage for a colourful pixel: its chromatic threads' shares sum to
/// `min(1, saturation / 0.8)` where the least-squares mix gives less, scaled up in proportion;
/// black and grey threads keep their share. On PE-DESIGN's ramp captures
/// (`photostitch2-ramp-cmyk/-rgbk.pes`) and the owner-shapes Color Sets this narrows the
/// difference from least squares alone: mean share error 0.069 → 0.064 on the ramp, 0.107 →
/// 0.093 on the shapes. Pale colours stay light, since their target is low.
pub fn saturate(rgb: [u8; 3], mut shares: Vec<f64>, threads: &[[f64; 3]]) -> Vec<f64> {
    let chromatic: Vec<usize> = (0..threads.len())
        .filter(|&k| saturation(threads[k]) > 0.2)
        .collect();
    let sum: f64 = chromatic.iter().map(|&k| shares[k]).sum();
    let target = (saturation(rgb.map(f64::from)) / FULL_SATURATION).min(1.0);
    if sum > 1e-6 && target > sum {
        for &k in &chromatic {
            shares[k] = (shares[k] * target / sum).min(1.0);
        }
    }
    shares
}

/// Non-negative shares `c` (summing to at most 1) minimising
/// `|colour − (fabric + Σ c_t (thread_t − fabric))|²`, by projected gradient descent.
pub fn mix(colour: [f64; 3], threads: &[[f64; 3]], fabric: [f64; 3]) -> Vec<f64> {
    let n = threads.len();
    let a: Vec<[f64; 3]> = threads
        .iter()
        .map(|t| [0, 1, 2].map(|k| (t[k] - fabric[k]) / 255.0))
        .collect();
    let b = [0, 1, 2].map(|k| (colour[k] - fabric[k]) / 255.0);
    // Step from the largest possible curvature (the trace of AᵀA).
    let trace: f64 = a.iter().map(|v| v.iter().map(|x| x * x).sum::<f64>()).sum();
    if trace <= 1e-12 {
        return vec![0.0; n];
    }
    let step = 1.0 / trace;
    let mut c = vec![0.0; n];
    for _ in 0..300 {
        let mut r = [0.0; 3];
        for (ct, at) in c.iter().zip(&a) {
            for k in 0..3 {
                r[k] += ct * at[k];
            }
        }
        for k in 0..3 {
            r[k] -= b[k];
        }
        for (ct, at) in c.iter_mut().zip(&a) {
            let g: f64 = (0..3).map(|k| at[k] * r[k]).sum();
            *ct = (*ct - step * g).max(0.0);
        }
        project_capped(&mut c);
    }
    c
}

/// Projects non-negative shares onto `Σ c ≤ 1` (onto the simplex when the sum exceeds 1).
fn project_capped(c: &mut [f64]) {
    let sum: f64 = c.iter().sum();
    if sum <= 1.0 {
        return;
    }
    let mut sorted: Vec<f64> = c.to_vec();
    sorted.sort_by(|a, b| b.total_cmp(a));
    let (mut acc, mut theta) = (0.0, 0.0);
    for (i, v) in sorted.iter().enumerate() {
        acc += v;
        let t = (acc - 1.0) / (i + 1) as f64;
        if *v - t > 0.0 {
            theta = t;
        }
    }
    for v in c.iter_mut() {
        *v = (*v - theta).max(0.0);
    }
}

/// Line-screen settings in design units (0.1 mm).
#[derive(Clone, Copy, Debug)]
pub struct Screen {
    /// Distance between lines.
    pub interval: f64,
    /// Straight running stitch length where the share is below [`MIN_SHARE`].
    pub run: f64,
    /// Max. Density, lines per mm.
    pub max_density: f64,
    /// OpenStitch mode: shares are each layer's coverage ([`Shares::layered`]) and a zigzag's
    /// stitches are spaced for it ([`Screen::advance`]), instead of PE-DESIGN's Max. Density
    /// ramp.
    pub coverage: bool,
}

impl Screen {
    /// Coverage of a straight run: a [`THREAD_MM`]-wide line every interval.
    pub fn straight(&self) -> f64 {
        (THREAD_MM * 10.0 / self.interval).min(1.0)
    }

    /// Advance along the line, design units, at which a zigzag covers `c` of its cell. The
    /// zigzag's stitches overlap more as they close up, so coverage saturates:
    /// `c = 1 − exp(−ZIGZAG_REACH / advance)` (fitted to 0.4 mm thread drawn on a 2 mm interval,
    /// 90 % deep; #129's calibrated coverage waits for a sew-out). At most [`MAX_COVER`].
    pub fn advance(&self, c: f64) -> f64 {
        let c = c.clamp(1e-6, MAX_COVER);
        let reach = ZIGZAG_REACH_MM * 10.0 * self.interval / 20.0;
        (reach / -(1.0 - c).dln()).min(self.run)
    }

    /// Needle points per mm² the layers' coverages `c` need: a zigzag stitch every
    /// [`Screen::advance`] along each line, or a straight run's stitch every `run`, lines an
    /// interval apart.
    pub fn points(&self, c: &[f64]) -> f64 {
        let interval = self.interval / 10.0;
        c.iter()
            .map(|&v| {
                let step = if v > self.straight() + 1e-9 {
                    self.advance(v)
                } else {
                    self.run
                };
                1.0 / (step / 10.0) / interval
            })
            .sum()
    }
}

/// [`Screen::advance`]'s saturation length at a 2 mm interval, mm.
pub const ZIGZAG_REACH_MM: f64 = 0.5;
/// Most coverage a zigzag is asked for (full coverage needs ever closer stitches).
pub const MAX_COVER: f64 = 0.95;

/// Thread width, mm, for the coverage model.
pub const THREAD_MM: f64 = 0.4;

/// One thread's lines at `angle_deg`: serpentine over the image, one jump in (and one more after
/// each stretch of transparency).
pub fn layer(shares: &Shares, t: usize, angle_deg: f64, frame: &Frame, s: &Screen) -> Vec<Stitch> {
    let width = shares.cols as f64 * shares.cell;
    let height = shares.rows as f64 * shares.cell;
    let a = angle_deg.to_radians();
    // Along the line (y down on screen, so a counter-clockwise angle turns towards −y), and
    // across it.
    let u = (a.dcos(), -a.dsin());
    let n = (a.dsin(), a.dcos());
    let corners = [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)];
    let proj = |p: (f64, f64)| p.0 * n.0 + p.1 * n.1;
    let lo = corners
        .iter()
        .map(|&p| proj(p))
        .fold(f64::INFINITY, f64::min);
    let hi = corners
        .iter()
        .map(|&p| proj(p))
        .fold(f64::NEG_INFINITY, f64::max);
    let depth = DEPTH_SHARE * s.interval;
    // Advance per stitch at full share, design units.
    let full = 10.0 / (s.max_density.max(0.5) * POINTS_PER_DENSITY);
    let step = shares.cell * 0.5;
    let mut out: Vec<Stitch> = Vec::new();
    let mut pen: Option<(f64, f64)> = None;
    let mut k = 0usize;
    loop {
        let offset = lo + s.interval * (k as f64 + 0.5);
        if offset >= hi {
            break;
        }
        // The line {offset·n + τ·u} clipped to the image rectangle.
        let Some((t0, t1)) = clip(offset, n, u, width, height) else {
            k += 1;
            continue;
        };
        let (from, to, dir) = if k.is_multiple_of(2) {
            (t0, t1, 1.0)
        } else {
            (t1, t0, -1.0)
        };
        let point = |tau: f64, side: f64| {
            let base = (offset * n.0 + tau * u.0, offset * n.1 + tau * u.1);
            (base.0 + side * n.0, base.1 + side * n.1)
        };
        let mut tau = from;
        let mut side = 0.5;
        let mut since = 0.0;
        let mut down = false;
        let len = (to - from).abs();
        let mut walked = 0.0;
        while walked <= len + 1e-9 {
            let base = point(tau, 0.0);
            let share = shares.at(base.0, base.1, t);
            let Some(share) = share else {
                down = false;
                walked += step;
                tau += dir * step;
                continue;
            };
            let place = |p: (f64, f64),
                         out: &mut Vec<Stitch>,
                         pen: &mut Option<(f64, f64)>,
                         down: &mut bool| {
                let at = (frame.origin.x + p.0, frame.origin.y + p.1);
                let near = pen.is_some_and(|q| (q.0 - at.0).dhypot(q.1 - at.1) <= 3.0 * s.interval);
                if !*down && !near {
                    out.push(Stitch::new(at.0, at.1, Command::Jump));
                }
                out.push(Stitch::new(at.0, at.1, Command::Stitch));
                *pen = Some(at);
                *down = true;
            };
            if !down {
                place(base, &mut out, &mut pen, &mut down);
                since = 0.0;
            }
            let zigzag = if s.coverage {
                share > s.straight() + 1e-9
            } else {
                share >= MIN_SHARE
            };
            if zigzag {
                // Zigzag across the line, closer stitches for a higher share.
                let advance = if s.coverage {
                    s.advance(share)
                } else {
                    (full / share.min(1.0)).min(s.run)
                };
                walked += advance;
                tau += dir * advance;
                if walked > len {
                    break;
                }
                place(point(tau, side * depth), &mut out, &mut pen, &mut down);
                side = -side;
                since = 0.0;
            } else {
                // Straight run: a needle point every `run` exactly, or where the zigzag starts.
                let inc = step.min(s.run - since).max(1e-6);
                walked += inc;
                tau += dir * inc;
                since += inc;
                if walked > len {
                    break;
                }
                let next = point(tau, 0.0);
                let next_share = shares.at(next.0, next.1, t).unwrap_or(0.0);
                let next_zig = if s.coverage {
                    next_share > s.straight() + 1e-9
                } else {
                    next_share >= MIN_SHARE
                };
                if since >= s.run - 1e-9 || next_zig {
                    place(next, &mut out, &mut pen, &mut down);
                    since = 0.0;
                }
            }
        }
        // The line's end point, on the centre line.
        if down {
            let end = point(to, 0.0);
            if shares
                .at(end.0.min(width - 1e-6), end.1.min(height - 1e-6), t)
                .is_some()
            {
                let at = (frame.origin.x + end.0, frame.origin.y + end.1);
                if pen.is_none_or(|q| (q.0 - at.0).dhypot(q.1 - at.1) > 1e-6) {
                    out.push(Stitch::new(at.0, at.1, Command::Stitch));
                    pen = Some(at);
                }
            }
        }
        k += 1;
    }
    out
}

/// The parameter range `[t0, t1]` of the line `offset·n + τ·u` inside `[0, w] × [0, h]`.
fn clip(offset: f64, n: (f64, f64), u: (f64, f64), w: f64, h: f64) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (f64::NEG_INFINITY, f64::INFINITY);
    let p0 = (offset * n.0, offset * n.1);
    for (p, d, max) in [(p0.0, u.0, w), (p0.1, u.1, h)] {
        if d.abs() < 1e-12 {
            if p < 0.0 || p > max {
                return None;
            }
            continue;
        }
        let (a, b) = ((0.0 - p) / d, (max - p) / d);
        let (a, b) = if a < b { (a, b) } else { (b, a) };
        t0 = t0.max(a);
        t1 = t1.min(b);
    }
    (t1 - t0 > 1e-6).then_some((t0, t1))
}

/// OpenStitch mode's Auto Select (B16.1 T13): `n` chart threads for the line screen, chosen for
/// how well they *mix* to the image's colours over the `fabric`, not how near each is to one
/// colour. Each sampled colour (at most [`SELECT_SAMPLES`], evenly through `colors`) is scored
/// against every pair of chart threads by the CIEDE2000 of its best mix of the pair and the
/// fabric ([`mix2`], linear light); a set's cost is the mean over the colours of their best pair
/// within it. Threads are added greedily, then each is swapped for any better chart thread,
/// twice. Ties go to the lower chart number. Returns the chosen chart indices, ascending.
pub fn select_threads(colors: &[[u8; 3]], fabric: [f64; 3], n: usize) -> Vec<usize> {
    use crate::perceptual::{de2000, lab};
    let lin = |c: [f64; 3]| c.map(|v| 255.0 * linear_of(v / 255.0));
    let chart: Vec<(usize, [f64; 3])> = openstitch_core::PEC_CHART
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.as_ref().map(|t| (i, [t.red, t.green, t.blue].map(f64::from))))
        .filter(|(_, c)| {
            let to = |c: [f64; 3]| lab(c.map(|v| v as u8));
            de2000(to(*c), to(fabric)) > 3.0
        })
        .collect();
    let m = chart.len();
    let n = n.min(m);
    if n == 0 || colors.is_empty() {
        return Vec::new();
    }
    let stride = colors.len().div_ceil(SELECT_SAMPLES).max(1);
    let px: Vec<[f64; 3]> = colors.iter().step_by(stride).map(|c| c.map(f64::from)).collect();
    let px_lab: Vec<_> = px.iter().map(|c| lab(c.map(|v| v as u8))).collect();
    let lf = lin(fabric);
    let lt: Vec<[f64; 3]> = chart.iter().map(|(_, c)| lin(*c)).collect();
    let to_lab = |l: [f64; 3]| {
        lab(l.map(|v| (255.0 * srgb_of(v / 255.0)).round().clamp(0.0, 255.0) as u8))
    };
    // err[(i * m + j) * p + k]: colour k's error mixed from threads i ≤ j and the fabric.
    let p = px.len();
    let mut err = vec![f64::INFINITY; m * m * p];
    for i in 0..m {
        for j in i..m {
            for (k, c) in px.iter().enumerate() {
                let (a, b) = mix2(lin(*c), lt[i], lt[j], lf);
                let mixed = [0, 1, 2].map(|q| lf[q] + a * (lt[i][q] - lf[q]) + b * (lt[j][q] - lf[q]));
                let e = de2000(to_lab(mixed), px_lab[k]);
                err[(i * m + j) * p + k] = e;
                err[(j * m + i) * p + k] = e;
            }
        }
    }
    let cost = |set: &[usize]| -> f64 {
        (0..p)
            .map(|k| {
                let mut best = f64::INFINITY;
                for (x, &i) in set.iter().enumerate() {
                    for &j in &set[x..] {
                        best = best.min(err[(i * m + j) * p + k]);
                    }
                }
                best
            })
            .sum::<f64>()
            / p as f64
    };
    let mut set: Vec<usize> = Vec::new();
    while set.len() < n {
        let mut best = (f64::INFINITY, 0);
        for c in 0..m {
            if set.contains(&c) {
                continue;
            }
            let mut trial = set.clone();
            trial.push(c);
            let v = cost(&trial);
            if v < best.0 {
                best = (v, c);
            }
        }
        set.push(best.1);
    }
    for _ in 0..2 {
        for pos in 0..set.len() {
            let mut cur = cost(&set);
            for c in 0..m {
                if set.contains(&c) {
                    continue;
                }
                let mut trial = set.clone();
                trial[pos] = c;
                let v = cost(&trial);
                if v < cur - 1e-12 {
                    cur = v;
                    set = trial;
                }
            }
        }
    }
    let mut out: Vec<usize> = set.iter().map(|&i| chart[i].0).collect();
    out.sort_unstable();
    out
}

/// Most colours [`select_threads`] scores.
pub const SELECT_SAMPLES: usize = 400;

/// Shares `(a, b)` of threads `t1`, `t2` over `fabric` (both non-negative, `a + b ≤ 1`) whose
/// mix is nearest `colour` in least squares, exactly (the 2 × 2 normal equations, else the best
/// point on the feasible triangle's edges).
fn mix2(colour: [f64; 3], t1: [f64; 3], t2: [f64; 3], fabric: [f64; 3]) -> (f64, f64) {
    let u = [0, 1, 2].map(|k| t1[k] - fabric[k]);
    let v = [0, 1, 2].map(|k| t2[k] - fabric[k]);
    let w = [0, 1, 2].map(|k| colour[k] - fabric[k]);
    let d = |x: [f64; 3], y: [f64; 3]| x[0] * y[0] + x[1] * y[1] + x[2] * y[2];
    let resid = |a: f64, b: f64| {
        let r = [0, 1, 2].map(|k| a * u[k] + b * v[k] - w[k]);
        d(r, r)
    };
    let mut cands: Vec<(f64, f64)> = Vec::with_capacity(4);
    let (uu, vv, uv, uw, vw) = (d(u, u), d(v, v), d(u, v), d(u, w), d(v, w));
    let det = uu * vv - uv * uv;
    if det.abs() > 1e-9 {
        cands.push(((uw * vv - vw * uv) / det, (vw * uu - uw * uv) / det));
    }
    // Edges: b = 0, a = 0, a + b = 1.
    let clamp01 = |x: f64| x.clamp(0.0, 1.0);
    cands.push((if uu > 1e-12 { clamp01(uw / uu) } else { 0.0 }, 0.0));
    cands.push((0.0, if vv > 1e-12 { clamp01(vw / vv) } else { 0.0 }));
    let s = [0, 1, 2].map(|k| u[k] - v[k]);
    let r0 = [0, 1, 2].map(|k| w[k] - v[k]);
    let a = if d(s, s) > 1e-12 { clamp01(d(r0, s) / d(s, s)) } else { 0.0 };
    cands.push((a, 1.0 - a));
    let mut best = (f64::INFINITY, (0.0, 0.0));
    for (a, b) in cands {
        if a < -1e-12 || b < -1e-12 || a + b > 1.0 + 1e-12 {
            continue;
        }
        let r = resid(a, b);
        if r < best.0 {
            best = (r, (a.max(0.0), b.max(0.0)));
        }
    }
    best.1
}

fn linear_of(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).dpowf(2.4)
    }
}

fn srgb_of(v: f64) -> f64 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.dpowf(1.0 / 2.4) - 0.055
    }
}
