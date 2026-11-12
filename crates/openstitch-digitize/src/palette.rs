//! Colour reduction: a 3×3 modal colour pre-filter and a colour census capped at
//! [`CENSUS_COLORS`] tracked colours (the prepass Auto Punch and Cross Stitch share, #127 #132),
//! median cut to N colours, each mapped to the nearest Brother PEC chart thread (equal-weight
//! squared-Euclidean distance, `openstitch-core`, #119), plus Floyd–Steinberg dithering.

use crate::perceptual::{self, Lab};
use crate::raster::Raster;
use openstitch_core::{PEC_CHART, Thread, color_distance, nearest_index, pec_basic_chart};
use openstitch_design::fmath::Det;
use std::collections::HashMap;

pub type Rgb = [u8; 3];

/// Reduced palette: `means` are the median-cut box colours (what pixels are matched against),
/// `threads` the distinct PEC threads they map to, `thread_of[i]` the thread of mean `i`.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub means: Vec<Rgb>,
    pub thread_of: Vec<usize>,
    pub threads: Vec<Thread>,
    /// PEC chart number (1–64) of each thread.
    pub chart: Vec<usize>,
    /// OpenStitch mode: pixels are matched to `means` by CIEDE2000 instead of RGB distance.
    pub perceptual: bool,
    /// OpenStitch mode: the nearest mean and nearest thread of colours met in advance (an Auto
    /// Punch census's modal colours), by packed colour, so each is matched once, not per pixel.
    known: Vec<(u32, usize, usize)>,
}

fn rgb_of(t: &Thread) -> Rgb {
    let v = t.rgb_value();
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

fn thread(c: Rgb) -> Thread {
    Thread::rgb(u32::from(c[0]) << 16 | u32::from(c[1]) << 8 | u32::from(c[2]))
}

/// PEC chart number and thread nearest to `c`.
pub fn nearest_pec(c: Rgb) -> (usize, Thread) {
    let i = nearest_index(&thread(c), pec_basic_chart()).unwrap_or(20);
    (i, PEC_CHART[i].clone().unwrap_or_else(|| thread(c)))
}

/// The thread of `source` nearest to `c` by CIEDE2000 (B16.1 T5); an empty `source` is the PEC
/// chart, matched as [`nearest_pec`] does.
pub fn nearest_in(c: Rgb, source: &[Thread]) -> Thread {
    if source.is_empty() {
        return nearest_pec(c).1;
    }
    let labs: Vec<Lab> = source.iter().map(|t| perceptual::lab(rgb_of(t))).collect();
    source[perceptual::nearest(&labs, perceptual::lab(c))].clone()
}

/// Most distinct colours the census tracks; later colours join their nearest tracked one.
pub const CENSUS_COLORS: usize = 100;

/// OpenStitch mode: two palette colours at least this far apart (CIEDE2000) get different
/// threads.
pub const DISTINCT_DE: f64 = 10.0;

/// RGB distance within which a census colour counts under a more frequent tracked colour.
/// 32 matches PE-DESIGN's automatic colour count on the round-5 images best (g01 4, g13 10,
/// g14 2, g15 5, the photos 10); 16 keeps edge shades as colours, 48 merges distinct ones.
pub const MERGE_DIST: u32 = 32;

/// The colour census of an image: the 3×3 modal colour of every opaque pixel (alpha ≥ 128),
/// counted into at most [`CENSUS_COLORS`] tracked colours, in the order they were first met.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Census {
    /// Tracked colours with their pixel counts, first met first.
    pub colors: Vec<(Rgb, u64)>,
    /// The modal colour of every pixel, `None` where it is transparent.
    pub modal: Vec<Option<Rgb>>,
    /// The modal colour of every pixel with its shades kept ([`MODAL_DIST`]): the colour Auto
    /// Punch labels the pixel by (#443); `None` where it is transparent.
    pub labels: Vec<Option<Rgb>>,
}

impl Census {
    /// Opaque pixels counted.
    pub fn pixels(&self) -> u64 {
        self.colors.iter().map(|c| c.1).sum()
    }

    /// The automatic colour count (PE-DESIGN's Auto Punch and Cross Stitch fill in "Max. Number
    /// of Colors" from the image: 2 for black bars on white, 5 for four flat shapes on white):
    /// the tracked colours holding at least [`AUTO_SHARE`] of the opaque pixels, at least 1 and
    /// at most [`AUTO_MAX`].
    pub fn auto_colors(&self) -> usize {
        self.auto_colors_up_to(AUTO_MAX)
    }

    /// [`Census::auto_colors`] with another ceiling than [`AUTO_MAX`] (OpenStitch mode's High
    /// fidelity, [`crate::Fidelity::auto_max`]).
    pub fn auto_colors_up_to(&self, max: usize) -> usize {
        let total = self.pixels() as f64;
        self.colors
            .iter()
            .filter(|c| c.1 as f64 >= total * AUTO_SHARE)
            .count()
            .clamp(1, max.max(1))
    }

    /// OpenStitch mode's automatic count (#962): [`Census::auto_colors_up_to`] plus the small
    /// colours of a flat picture that are far from every colour with a real share
    /// ([`distinct_small`]), however unsaturated, and drawn as a solid patch, not a seam: at
    /// least [`SOLID_SHARE`] of their pixels have all four neighbours in the same colour. g02's
    /// basket brown holds 0.47 % of the pixels, just under [`AUTO_SHARE`], at chroma about 35,
    /// and without a thread of its own it was sewn in the balloon's red; the anti-aliased edges
    /// of g08's letters are as far from its colours but one or two pixels wide. `width` is the
    /// census image's width in pixels.
    pub fn auto_colors_distinct_up_to(&self, max: usize, width: usize) -> usize {
        let extra = distinct_small(&self.colors)
            .into_iter()
            .filter(|&c| self.solid_share(c, width) >= SOLID_SHARE)
            .count();
        (self.auto_colors_up_to(max) + extra).clamp(1, max.max(1))
    }

    /// The share of the pixels counted under tracked colour `c` (their nearest tracked colour)
    /// whose four neighbours are counted under it too.
    fn solid_share(&self, c: Rgb, width: usize) -> f64 {
        if width == 0 || !self.modal.len().is_multiple_of(width) {
            return 0.0;
        }
        let height = self.modal.len() / width;
        let of = |p: Option<Rgb>| {
            p.is_some_and(|p| {
                self.colors
                    .iter()
                    .min_by_key(|k| sq_dist(p, k.0))
                    .is_some_and(|k| k.0 == c)
            })
        };
        let mine: Vec<bool> = self.modal.iter().map(|&p| of(p)).collect();
        let (mut all, mut solid) = (0usize, 0usize);
        for y in 0..height {
            for x in 0..width {
                if !mine[y * width + x] {
                    continue;
                }
                all += 1;
                if x > 0
                    && y > 0
                    && x + 1 < width
                    && y + 1 < height
                    && [
                        y * width + x - 1,
                        y * width + x + 1,
                        (y - 1) * width + x,
                        (y + 1) * width + x,
                    ]
                    .iter()
                    .all(|&i| mine[i])
                {
                    solid += 1;
                }
            }
        }
        if all == 0 {
            0.0
        } else {
            solid as f64 / all as f64
        }
    }
}

/// The least share of a small colour's pixels with all four neighbours in it for OpenStitch
/// mode's automatic count to give it a thread (#962): a solid patch, not an anti-aliased seam.
const SOLID_SHARE: f64 = 0.3;

/// The census colours of a flat picture (at most [`PROTECT_FLAT`]) smaller than
/// [`AUTO_SHARE`] (but at least a tenth of it) and at least [`PROTECT_DE`] from every colour
/// with a real share, at most [`PROTECT_MAX`], largest first: [`protected`] without its chroma
/// rule (#962, OpenStitch mode).
fn distinct_small(hist: &[(Rgb, u64)]) -> Vec<Rgb> {
    small_colours(hist, 0.0)
}

/// A colour this saturated (CIELAB chroma) may keep its own box in PE-compatible mode however
/// small it is ([`protected`], #447).
const PROTECT_CHROMA: f64 = 50.0;
/// ...when every colour with at least [`AUTO_SHARE`] of the pixels is at least this far from
/// it (CIE76).
const PROTECT_DE: f64 = 30.0;
/// At most this many colours are kept that way.
const PROTECT_MAX: usize = 3;
/// ...and only in a census of at most this many colours (flat graphics, #447).
const PROTECT_FLAT: usize = 40;

/// The census colours PE-compatible mode keeps a box for however small they are (#447): at
/// most [`PROTECT_MAX`], smaller than [`AUTO_SHARE`], saturated, and far from every colour that
/// has a real share. Largest first.
fn protected(hist: &[(Rgb, u64)]) -> Vec<Rgb> {
    small_colours(hist, PROTECT_CHROMA)
}

/// [`protected`] and [`distinct_small`]: the small, far colours at least `chroma` saturated.
fn small_colours(hist: &[(Rgb, u64)], chroma: f64) -> Vec<Rgb> {
    let total: u64 = hist.iter().map(|c| c.1).sum();
    // Flat graphics only: a photo's census is full of shades, and its small saturated ones
    // are highlights PE-DESIGN merges (p07's pinks).
    if total == 0 || hist.len() > PROTECT_FLAT {
        return Vec::new();
    }
    let share = |c: &(Rgb, u64)| c.1 as f64 / total as f64;
    let big: Vec<Lab> = hist
        .iter()
        .filter(|c| share(c) >= AUTO_SHARE)
        .map(|c| crate::perceptual::lab(c.0))
        .collect();
    let mut kept: Vec<(Rgb, u64)> = hist
        .iter()
        .filter(|c| share(c) < AUTO_SHARE && share(c) >= AUTO_SHARE / 10.0)
        .filter(|c| {
            let l = crate::perceptual::lab(c.0);
            l[1].dhypot(l[2]) >= chroma
                && big.iter().all(|b| {
                    ((b[0] - l[0]).powi(2) + (b[1] - l[1]).powi(2) + (b[2] - l[2]).powi(2)).sqrt()
                        >= PROTECT_DE
                })
        })
        .copied()
        .collect();
    kept.sort_by_key(|c| std::cmp::Reverse(c.1));
    kept.truncate(PROTECT_MAX);
    kept.into_iter().map(|c| c.0).collect()
}

/// A median-cut box whose nearest PEC thread is taken takes the nearest unused one when that
/// is at most this many times as far (squared RGB): 2 keeps p03's and d01's 10 colours apart
/// (PE-DESIGN 10) and leaves g11 as it was; 4 also gave g11 a Flesh Pink for a grey (#447).
const UNUSED_RATIO: i64 = 2;

/// Smallest share of the opaque pixels a colour needs to count in [`Census::auto_colors`].
pub const AUTO_SHARE: f64 = 0.005;
/// Most colours [`Census::auto_colors`] picks: PE-DESIGN fills in at most 10 (round 5: 10 for
/// the twelve-colour mosaic `g13` and for every photo).
pub const AUTO_MAX: usize = 10;

/// Thread indices of `palette` in PE-DESIGN's colour order (round-4 captures
/// `autopunch-default.pes` and `autopunch-owner-shapes-moved.pes`, the same in Cross Stitch):
/// by the reduced colour's channel sum R + G + B, highest first (white 765, yellow 480, blue
/// 290, red 280, green 250 on the owner's shapes, wherever each colour sits and whatever its
/// area); ties to the lower index. A thread reached from several reduced colours takes the
/// brightest of them.
pub fn brightness_order(palette: &Palette, threads: impl IntoIterator<Item = usize>) -> Vec<usize> {
    let sum = |t: usize| -> u32 {
        palette
            .means
            .iter()
            .zip(&palette.thread_of)
            .filter(|(_, k)| **k == t)
            .map(|(m, _)| m.iter().map(|&v| u32::from(v)).sum::<u32>())
            .max()
            .unwrap_or(0)
    };
    let mut order: Vec<usize> = threads.into_iter().collect();
    order.sort_by_key(|&t| (std::cmp::Reverse(sum(t)), t));
    order
}

/// RGB distance within which a window pixel counts as a shade of the centre's colour when the
/// census labels a pixel ([`Census::modal`]). The shade runs of a flat colour drawn in slightly
/// different shades differ by up to 14 (`examples/images/g08-lettering.png`).
pub const MODAL_DIST: u32 = 16;

/// The commonest colour of a 3×3 window counts as flat when at least this many of its pixels
/// have it exactly (a row of three across an edge).
pub const FLAT_RUN: u8 = 3;

/// Modal colour of the 3×3 window around `(x, y)`: the colour most of the window's opaque
/// pixels have. Ties keep the centre's own colour when it is among them, otherwise the
/// smallest packed colour. `None` for a transparent centre.
///
/// With `keep_shades`, the centre also keeps its colour when it and the window's shades of it
/// (within [`MODAL_DIST`]) are at least as many as the most common colour (#443). A flat colour
/// drawn in runs of slightly different shades (g08: the red bar's edge row alternates
/// 243,68,82 and 244,79,91, the cream above it 254,245,237 and 252,250,240) otherwise splits
/// its votes where two runs meet, and its edge pixel there took the other side's colour: one-
/// pixel notches along straight edges, which Auto satin fanned its rows round. The rule only
/// ever keeps a centre the plain modal would replace, so a one-pixel line between edge shades
/// stays as it did (the leaf's veins, `g10-leaf-transparent.png`). It applies only against a
/// flat colour, one at least [`FLAT_RUN`] window pixels have exactly: in a photograph's
/// gradients no colour repeats that often, and the plain modal smooths them as before (keeping
/// every centre there lowered the storybook picture's and the mosaic's likeness).
fn modal_at(image: &Raster, x: u32, y: u32, keep_shades: bool) -> Option<Rgb> {
    let [r, g, b, a] = image.pixel(x, y);
    if a < 128 {
        return None;
    }
    let centre = [r, g, b];
    let mut counts: Vec<(Rgb, u8)> = Vec::with_capacity(9);
    let mut shades = 0u8;
    for ny in y.saturating_sub(1)..(y + 2).min(image.height()) {
        for nx in x.saturating_sub(1)..(x + 2).min(image.width()) {
            let [r, g, b, a] = image.pixel(nx, ny);
            if a < 128 {
                continue;
            }
            let c = [r, g, b];
            shades += u8::from(sq_dist(c, centre) <= MODAL_DIST * MODAL_DIST);
            match counts.iter_mut().find(|k| k.0 == c) {
                Some(k) => k.1 += 1,
                None => counts.push((c, 1)),
            }
        }
    }
    let most = counts.iter().map(|k| k.1).max().unwrap_or(0);
    if keep_shades && most >= FLAT_RUN && shades >= most {
        return Some(centre);
    }
    let mut tied: Vec<Rgb> = counts.iter().filter(|k| k.1 == most).map(|k| k.0).collect();
    if tied.contains(&centre) {
        return Some(centre);
    }
    tied.sort_by_key(|&c| pack(c));
    tied.first().copied().or(Some(centre))
}

/// Squared Euclidean RGB distance (the census's own merge metric).
fn sq_dist(a: Rgb, b: Rgb) -> u32 {
    (0..3)
        .map(|k| {
            let d = i32::from(a[k]) - i32::from(b[k]);
            (d * d) as u32
        })
        .sum()
}

/// The colour census of `image` (#127): every opaque pixel takes its 3×3 modal colour, and the
/// modal colours are counted into at most [`CENSUS_COLORS`] tracked colours. Colours are taken
/// most frequent first (ties to the first met): one within [`MERGE_DIST`] of a tracked colour
/// counts under the nearest tracked one, any other is tracked while there is room and counted
/// under the nearest tracked one after that. Tracked colours are listed in the order first met.
///
/// Taking the colours by frequency, not in scan order, matters on anti-aliased images (round 5,
/// `examples/images/g01-emblem.png`): their many small edge shades filled all 100 slots by the
/// 32nd row, so the emblem's orange and yellow counted as grey. PE-DESIGN keeps all four
/// colours (4 automatic colours), as this census does. Deterministic for a given image.
///
/// The colours are counted from the exact modal colour (each colour voting alone), which keeps
/// PE-DESIGN's automatic colour counts (the edge shades it keeps as regions on the small
/// emblem `d02`); [`Census::labels`] also holds the one that keeps the centre's shades, which
/// Auto Punch labels its regions by, so a flat colour drawn in slightly different shades keeps
/// straight edges (#443).
pub fn census(image: &Raster) -> Census {
    let mut seen: HashMap<Rgb, usize> = HashMap::new();
    let mut all: Vec<(Rgb, u64)> = Vec::new();
    let mut modal = Vec::with_capacity(image.width() as usize * image.height() as usize);
    let mut labels = Vec::with_capacity(modal.capacity());
    for y in 0..image.height() {
        for x in 0..image.width() {
            labels.push(modal_at(image, x, y, true));
            let m = modal_at(image, x, y, false);
            modal.push(m);
            let Some(c) = m else { continue };
            let i = *seen.entry(c).or_insert_with(|| {
                all.push((c, 0));
                all.len() - 1
            });
            all[i].1 += 1;
        }
    }
    // Most frequent first: a colour within MERGE_DIST of a tracked colour counts under the
    // nearest one; otherwise it is tracked while there is room, else counted under the nearest.
    let mut rank: Vec<usize> = (0..all.len()).collect();
    rank.sort_by_key(|&i| (std::cmp::Reverse(all[i].1), i));
    // (first-met index, colour, count)
    let mut tracked: Vec<(usize, Rgb, u64)> = Vec::new();
    for &i in &rank {
        let (c, n) = all[i];
        let mut best = (u32::MAX, 0);
        for (j, k) in tracked.iter().enumerate() {
            let d = sq_dist(c, k.1);
            if d < best.0 {
                best = (d, j);
            }
        }
        if best.0 <= MERGE_DIST * MERGE_DIST || tracked.len() >= CENSUS_COLORS {
            tracked[best.1].2 += n;
        } else {
            tracked.push((i, c, n));
        }
    }
    tracked.sort_by_key(|k| k.0);
    let colors = tracked.into_iter().map(|(_, c, n)| (c, n)).collect();
    Census {
        colors,
        modal,
        labels,
    }
}

impl Palette {
    /// Median cut of `colors` (a pixel list; order does not matter) into at most `n` boxes,
    /// then PEC matching. Deterministic: ties go to the lowest box / smallest packed colour.
    pub fn build(colors: &[Rgb], n: usize) -> Palette {
        Self::from_histogram(&histogram(colors), n, false)
    }

    /// The palette of a colour census: its tracked colours, weighted by their counts, median
    /// cut into at most `n` boxes, then PEC matching (#127).
    pub fn from_census(census: &Census, n: usize) -> Palette {
        let mut hist = census.colors.clone();
        hist.sort_by_key(|c| pack(c.0));
        // A small saturated colour unlike every colour with a real share keeps its own box, as
        // PE-DESIGN keeps it at every count (#447: a 0.17 % red patch has its own thread at
        // 10, 6 and 4 colours, round 9); median cut alone gives it no box of its own.
        let kept = protected(&hist);
        if kept.is_empty() || n <= kept.len() {
            return Self::from_histogram(&hist, n, true);
        }
        let rest: Vec<(Rgb, u64)> = hist
            .iter()
            .filter(|c| !kept.contains(&c.0))
            .copied()
            .collect();
        let mut p = Self::from_histogram(&rest, n - kept.len(), true);
        for c in kept {
            let (i, t) = nearest_pec(c);
            let k = p.chart.iter().position(|&x| x == i).unwrap_or_else(|| {
                p.chart.push(i);
                p.threads.push(t);
                p.chart.len() - 1
            });
            p.means.push(c);
            p.thread_of.push(k);
        }
        p
    }

    /// Median cut of a sorted colour histogram, then PEC matching.
    fn from_histogram(hist: &[(Rgb, u64)], n: usize, distinct: bool) -> Palette {
        let means = median_cut_histogram(hist, n.max(1));
        let mut threads: Vec<Thread> = Vec::new();
        let mut chart: Vec<usize> = Vec::new();
        let mut thread_of = Vec::with_capacity(means.len());
        for &m in &means {
            let (mut i, mut t) = nearest_pec(m);
            // PE-compatible Auto Punch and Cross Stitch (`distinct`, the census palettes):
            // a box whose nearest thread another box took already takes its nearest unused
            // thread, when that is at most UNUSED_RATIO times as far (squared RGB): PE-DESIGN
            // keeps as many threads as it proposes colours (#447: p03 10, where two boxes
            // matching one thread left 8 and lost the fox's orange).
            if distinct && chart.contains(&i) {
                let r = UNUSED_RATIO;
                let me = thread(m);
                let best = PEC_CHART[i]
                    .as_ref()
                    .map_or(i64::MAX, |c| color_distance(&me, c));
                if let Some((j, c)) = pec_basic_chart()
                    .iter()
                    .enumerate()
                    .filter_map(|(j, c)| c.as_ref().map(|c| (j, c)))
                    .filter(|(j, _)| !chart.contains(j))
                    .min_by_key(|(_, c)| color_distance(&me, c))
                    && color_distance(&me, c) <= best.saturating_mul(r).max(best + 1)
                {
                    i = j;
                    t = c.clone();
                }
            }
            let k = chart.iter().position(|&c| c == i).unwrap_or_else(|| {
                chart.push(i);
                threads.push(t);
                chart.len() - 1
            });
            thread_of.push(k);
        }
        Palette {
            means,
            thread_of,
            threads,
            chart,
            perceptual: false,
            known: Vec::new(),
        }
    }

    /// OpenStitch mode's palette of a colour census (docs/research/beyond/image-similarity-plan.md,
    /// T4). The census colours, weighted by their counts, choose at most `n` threads from the PEC
    /// chart by backward elimination ([`perceptual::select_threads`]). The median-cut box colours
    /// seed a weighted k-means in CIELAB as before, and each centre takes the chosen thread that
    /// sews its members with the least weighted CIEDE2000 (ties to the lower chart number). Thin
    /// dark lines on white keep a thread of their own whenever dropping it costs the image more
    /// than any other thread. Pixels are matched to the centres by CIEDE2000, so a colour sharing
    /// the background's thread (a line's light edge) is still a colour of its own.
    pub fn from_census_perceptual(census: &Census, n: usize) -> Palette {
        Self::from_census_perceptual_in(census, n, &[])
    }

    /// [`Palette::from_census_perceptual`] choosing from `source` instead of the PEC chart
    /// (B16.1 T5: other brands' charts, the user's own threads); an empty `source` is the PEC
    /// chart. A thread from another chart records the PEC chart number nearest it in
    /// [`Palette::chart`], the number a PES/PEC file gives it.
    pub fn from_census_perceptual_in(census: &Census, n: usize, source: &[Thread]) -> Palette {
        let (chart, labs) = candidates(source);
        let colors: Vec<(Lab, f64)> = census
            .colors
            .iter()
            .map(|&(c, w)| (perceptual::lab(c), w as f64))
            .collect();
        let kept = perceptual::select_threads(&colors, &labs, n);
        let kept_labs: Vec<Lab> = kept.iter().map(|&k| labs[k]).collect();
        let mut palette = Palette {
            means: Vec::new(),
            thread_of: Vec::new(),
            threads: Vec::new(),
            chart: Vec::new(),
            perceptual: true,
            known: Vec::new(),
        };
        // The candidate each palette thread came from.
        let mut taken: Vec<usize> = Vec::new();
        for (&(c, _), &(l, _)) in census.colors.iter().zip(&colors) {
            let i = kept[nearest_de(&kept_labs, l)];
            let k = taken.iter().position(|&x| x == i).unwrap_or_else(|| {
                taken.push(i);
                palette.chart.push(chart[i].0);
                palette.threads.push(chart[i].1.clone());
                taken.len() - 1
            });
            palette.means.push(c);
            palette.thread_of.push(k);
        }
        let mut met: Vec<u32> = census
            .modal
            .iter()
            .chain(&census.labels)
            .flatten()
            .map(|&c| pack(c))
            .collect();
        met.sort_unstable();
        met.dedup();
        let mean_labs: Vec<Lab> = palette.means.iter().map(|&m| perceptual::lab(m)).collect();
        let thread_labs: Vec<Lab> = taken.iter().map(|&i| labs[i]).collect();
        palette.known = met
            .into_iter()
            .map(|p| {
                let l = perceptual::lab([(p >> 16) as u8, (p >> 8) as u8, p as u8]);
                (p, nearest_de(&mean_labs, l), nearest_de(&thread_labs, l))
            })
            .collect();
        palette
    }

    /// OpenStitch mode's palette of a pixel list for Photo Stitch 2's line screen: the
    /// median-cut box colours seed a weighted k-means in CIELAB, and each centre takes the PEC
    /// thread nearest it by CIEDE2000. A centre clearly distinct from another ([`DISTINCT_DE`])
    /// never shares its thread: it takes its nearest thread not yet taken. The screen mixes each
    /// thread with the fabric, so its threads are chosen by Photo Stitch 2's own rework (T13),
    /// not by [`Palette::select_perceptual`].
    pub fn build_perceptual(colors: &[Rgb], n: usize) -> Palette {
        Self::build_perceptual_in(colors, n, &[])
    }

    /// [`Palette::build_perceptual`] matching the centres to `source`'s threads instead of the
    /// PEC chart (B16.1 T5); an empty `source` is the PEC chart. A thread from another chart
    /// records the PEC chart number nearest it in [`Palette::chart`], as in
    /// [`Palette::from_census_perceptual_in`].
    pub fn build_perceptual_in(colors: &[Rgb], n: usize, source: &[Thread]) -> Palette {
        Self::from_histogram_perceptual(&histogram(colors), n, source)
    }

    fn from_histogram_perceptual(hist: &[(Rgb, u64)], n: usize, source: &[Thread]) -> Palette {
        let seeds: Vec<Lab> = median_cut_histogram(hist, n.max(1))
            .into_iter()
            .map(perceptual::lab)
            .collect();
        let colors: Vec<(Lab, f64)> = hist
            .iter()
            .map(|&(c, w)| (perceptual::lab(c), w as f64))
            .collect();
        let centres = perceptual::kmeans(&colors, &seeds, 20);
        let (chart, labs) = candidates(source);
        let mut palette = Palette {
            means: Vec::new(),
            thread_of: Vec::new(),
            threads: Vec::new(),
            chart: Vec::new(),
            perceptual: true,
            known: Vec::new(),
        };
        // The candidate each palette thread came from, and which centres took it, to keep
        // distinct colours apart.
        let mut taken: Vec<usize> = Vec::new();
        let mut takers: Vec<Vec<Lab>> = Vec::new();
        for c in centres {
            let mut ranked: Vec<(f64, usize)> = labs
                .iter()
                .enumerate()
                .map(|(j, &t)| (perceptual::de2000(c, t), j))
                .collect();
            ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let free = |j: usize| match taken.iter().position(|&x| x == j) {
                None => true,
                Some(k) => takers[k]
                    .iter()
                    .all(|&o| perceptual::de2000(o, c) < DISTINCT_DE),
            };
            let Some(j) = ranked
                .iter()
                .find(|&&(_, j)| free(j))
                .or(ranked.first())
                .map(|&(_, j)| j)
            else {
                continue;
            };
            let k = taken.iter().position(|&x| x == j).unwrap_or_else(|| {
                taken.push(j);
                palette.chart.push(chart[j].0);
                palette.threads.push(chart[j].1.clone());
                takers.push(Vec::new());
                taken.len() - 1
            });
            takers[k].push(c);
            palette.means.push(perceptual::rgb(c));
            palette.thread_of.push(k);
        }
        palette
    }

    /// OpenStitch mode's palette of a pixel list (Photo Stitch 1, T4): at most `n` threads
    /// chosen from the PEC chart by backward elimination ([`perceptual::select_threads`]) over
    /// the pixels' histogram. Each chosen thread is a palette colour of its own, and its mean is
    /// the weighted mean of the colours nearest to it. Photo Stitch 1 dithers onto the threads
    /// themselves.
    pub fn select_perceptual(colors: &[Rgb], n: usize) -> Palette {
        Self::select_perceptual_with(colors, n, perceptual::SelectCost::Nearest, &[])
    }

    /// [`Palette::select_perceptual`] choosing from `source` instead of the PEC chart (T5), as
    /// [`Palette::from_census_perceptual_in`].
    pub fn select_perceptual_in(colors: &[Rgb], n: usize, source: &[Thread]) -> Palette {
        Self::select_perceptual_with(colors, n, perceptual::SelectCost::Nearest, source)
    }

    /// [`Palette::select_perceptual`] with the selection's cost chosen by the caller: nearest
    /// thread, or the best two-thread mix for rows that mix threads by error carry
    /// ([`perceptual::SelectCost`]), choosing from `source` (empty: the PEC chart). On Photo
    /// Stitch 1's 3 mm cells the nearest thread scores higher (nine photos: 57.1 against 54.2
    /// with the mix), so that is the default there.
    pub fn select_perceptual_with(
        colors: &[Rgb],
        n: usize,
        cost: perceptual::SelectCost,
        source: &[Thread],
    ) -> Palette {
        let (chart, labs) = candidates(source);
        let hist = histogram(colors);
        let rgbs: Vec<Rgb> = chart.iter().map(|(_, t)| rgb_of(t)).collect();
        let weighted: Vec<(Rgb, f64)> = hist.iter().map(|&(c, w)| (c, w as f64)).collect();
        let kept = match cost {
            perceptual::SelectCost::Nearest => {
                perceptual::select_threads(&perceptual::select_colors(&hist), &labs, n)
            }
            perceptual::SelectCost::Mix => perceptual::select_threads_by(&weighted, &rgbs, n, cost),
        };
        let colors = perceptual::select_colors(&hist);
        let kept_labs: Vec<Lab> = kept.iter().map(|&k| labs[k]).collect();
        let mut sum = vec![([0.0; 3], 0.0); kept.len()];
        for &(c, w) in &colors {
            let s = &mut sum[nearest_de(&kept_labs, c)];
            for (total, v) in s.0.iter_mut().zip(c) {
                *total += v * w;
            }
            s.1 += w;
        }
        Palette {
            means: sum
                .iter()
                .zip(&kept_labs)
                .map(|((s, w), &t)| perceptual::rgb(if *w > 0.0 { s.map(|v| v / w) } else { t }))
                .collect(),
            thread_of: (0..kept.len()).collect(),
            threads: kept.iter().map(|&k| chart[k].1.clone()).collect(),
            chart: kept.iter().map(|&k| chart[k].0).collect(),
            perceptual: true,
            known: Vec::new(),
        }
    }

    /// Thread index for `c` (ties to the lower index): the thread of the box mean nearest to it,
    /// or in OpenStitch mode the thread nearest to it by CIEDE2000.
    pub fn classify(&self, c: Rgb) -> usize {
        if self.perceptual {
            return match self.known.binary_search_by_key(&pack(c), |k| k.0) {
                Ok(i) => self.known[i].2,
                Err(_) => self.match_thread(c),
            };
        }
        self.thread_of
            .get(self.nearest_mean(c))
            .copied()
            .unwrap_or(0)
    }

    /// OpenStitch mode: the thread nearest `c` by CIEDE2000 (ties to the lower index).
    fn match_thread(&self, c: Rgb) -> usize {
        let labs: Vec<Lab> = self
            .threads
            .iter()
            .map(|t| perceptual::lab(rgb_of(t)))
            .collect();
        nearest_de(&labs, perceptual::lab(c))
    }

    /// OpenStitch mode: the mean nearest `c` by CIEDE2000 (ties to the lower index).
    fn match_mean(&self, c: Rgb) -> usize {
        let labs: Vec<Lab> = self.means.iter().map(|&m| perceptual::lab(m)).collect();
        nearest_de(&labs, perceptual::lab(c))
    }

    /// Index of the box mean nearest to `c` (ties to the lower index): by CIEDE2000 in
    /// OpenStitch mode, by RGB distance otherwise.
    pub fn nearest_mean(&self, c: Rgb) -> usize {
        if self.perceptual {
            return match self.known.binary_search_by_key(&pack(c), |k| k.0) {
                Ok(i) => self.known[i].1,
                Err(_) => self.match_mean(c),
            };
        }
        let t = thread(c);
        let mut best = (i64::MAX, 0);
        for (i, &m) in self.means.iter().enumerate() {
            let d = color_distance(&t, &thread(m));
            if d < best.0 {
                best = (d, i);
            }
        }
        best.1
    }
}

/// The PEC chart's threads with their chart numbers, and their CIELAB colours.
fn pec_labs() -> (Vec<(usize, Thread)>, Vec<Lab>) {
    let chart: Vec<(usize, Thread)> = pec_basic_chart()
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.clone().map(|t| (i, t)))
        .collect();
    let labs = chart
        .iter()
        .map(|(_, t)| perceptual::lab(rgb_of(t)))
        .collect();
    (chart, labs)
}

/// The threads a palette chooses from with their PEC chart numbers, and their CIELAB colours:
/// the PEC chart when `source` is empty, otherwise `source`, each thread with the number of the
/// PEC thread nearest it.
fn candidates(source: &[Thread]) -> (Vec<(usize, Thread)>, Vec<Lab>) {
    if source.is_empty() {
        return pec_labs();
    }
    let chart: Vec<(usize, Thread)> = source
        .iter()
        .map(|t| (nearest_pec(rgb_of(t)).0, t.clone()))
        .collect();
    let labs = chart
        .iter()
        .map(|(_, t)| perceptual::lab(rgb_of(t)))
        .collect();
    (chart, labs)
}

/// Index of the colour in `targets` nearest `c` by CIEDE2000 (ties to the lower index).
fn nearest_de(targets: &[Lab], c: Lab) -> usize {
    let mut best = (f64::INFINITY, 0);
    for (i, &t) in targets.iter().enumerate() {
        let d = perceptual::de2000(c, t);
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1
}

fn pack(c: Rgb) -> u32 {
    u32::from(c[0]) << 16 | u32::from(c[1]) << 8 | u32::from(c[2])
}

/// Distinct colours of a pixel list with their counts, sorted by packed value.
fn histogram(colors: &[Rgb]) -> Vec<(Rgb, u64)> {
    let mut packed: Vec<u32> = colors.iter().map(|&c| pack(c)).collect();
    packed.sort_unstable();
    let mut hist: Vec<(Rgb, u64)> = Vec::new();
    for p in packed {
        let c = [(p >> 16) as u8, (p >> 8) as u8, p as u8];
        match hist.last_mut() {
            Some((last, count)) if *last == c => *count += 1,
            _ => hist.push((c, 1)),
        }
    }
    hist
}

/// Weighted median cut (variance-driven: the box with the largest squared error splits at
/// its mean). Returns up to `n` mean colours (fewer if the image has fewer distinct colours).
pub fn median_cut(colors: &[Rgb], n: usize) -> Vec<Rgb> {
    median_cut_histogram(&histogram(colors), n)
}

/// [`median_cut`] of a colour histogram sorted by packed value.
fn median_cut_histogram(hist: &[(Rgb, u64)], n: usize) -> Vec<Rgb> {
    if hist.is_empty() {
        return Vec::new();
    }
    let mut boxes: Vec<Vec<(Rgb, u64)>> = vec![hist.to_vec()];
    while boxes.len() < n {
        // Split the box with the largest squared error, along its highest-variance channel,
        // at the weighted mean; ties go to the lower box and channel.
        let mut pick: Option<(usize, usize, f64, f64)> = None;
        for (b, cs) in boxes.iter().enumerate() {
            if cs.len() < 2 {
                continue;
            }
            let w: f64 = cs.iter().map(|c| c.1 as f64).sum();
            let mut sse = 0.0;
            let mut best_ch = (0, -1.0, 0.0);
            for ch in 0..3 {
                let mean = cs
                    .iter()
                    .map(|c| f64::from(c.0[ch]) * c.1 as f64)
                    .sum::<f64>()
                    / w;
                let var: f64 = cs
                    .iter()
                    .map(|c| (f64::from(c.0[ch]) - mean).powi(2) * c.1 as f64)
                    .sum();
                sse += var;
                if var > best_ch.1 {
                    best_ch = (ch, var, mean);
                }
            }
            if pick.is_none_or(|(_, _, e, _)| sse > e) {
                pick = Some((b, best_ch.0, sse, best_ch.2));
            }
        }
        let Some((b, ch, sse, mean)) = pick else {
            break;
        };
        if sse <= 0.0 {
            break;
        }
        let mut cs = std::mem::take(&mut boxes[b]);
        cs.sort_by_key(|c| (c.0[ch], pack(c.0)));
        let cut = cs
            .iter()
            .position(|c| f64::from(c.0[ch]) > mean)
            .unwrap_or(cs.len() - 1)
            .clamp(1, cs.len() - 1);
        let upper = cs.split_off(cut);
        boxes[b] = cs;
        boxes.push(upper);
    }
    boxes
        .iter()
        .map(|cs| {
            let w: u64 = cs.iter().map(|c| c.1).sum();
            let mean = |ch: usize| {
                let s: u64 = cs.iter().map(|c| u64::from(c.0[ch]) * c.1).sum();
                ((s + w / 2) / w) as u8
            };
            [mean(0), mean(1), mean(2)]
        })
        .collect()
}

/// Floyd–Steinberg error diffusion (serpentine) of a `w`×`h` grid onto `targets`. Cells that
/// are `None` (transparent) are skipped and receive no error. Returns the target index per
/// cell, `None` for skipped cells. Ties go to the lower target index.
pub fn dither(
    cells: &[Option<[f64; 3]>],
    w: usize,
    h: usize,
    targets: &[[f64; 3]],
) -> Vec<Option<usize>> {
    diffuse(cells, w, h, targets, |c| c.map(|v| v.clamp(0.0, 255.0)))
}

/// OpenStitch mode's [`dither`]: the cells and targets (sRGB) are taken to CIELAB, and both the
/// nearest target and the diffused error are measured there, so neighbouring threads mix by
/// how they look rather than by RGB.
pub fn dither_perceptual(
    cells: &[Option<[f64; 3]>],
    w: usize,
    h: usize,
    targets: &[[f64; 3]],
) -> Vec<Option<usize>> {
    let to_lab = |c: [f64; 3]| perceptual::lab(c.map(|v| v.round().clamp(0.0, 255.0) as u8));
    let cells: Vec<Option<[f64; 3]>> = cells.iter().map(|c| c.map(to_lab)).collect();
    let targets: Vec<[f64; 3]> = targets.iter().map(|&t| to_lab(t)).collect();
    diffuse(&cells, w, h, &targets, |c| {
        [c[0].clamp(0.0, 100.0), c[1], c[2]]
    })
}

/// OpenStitch mode's dithering, only where the image is tonal (B16.1 T6): cells in flat areas
/// take their nearest target and neither give nor take diffused error, so flat colours and the
/// edges between them are sewn plainly, without the dithering's noise; tonal cells (gradients,
/// texture) dither in CIELAB as [`dither_perceptual`]. `window` is the neighbourhood half-size
/// in cells, `(columns, rows)`, the caller's choice for its cell shape ([`tonal_map`]).
pub fn dither_tonal(
    cells: &[Option<[f64; 3]>],
    w: usize,
    h: usize,
    targets: &[[f64; 3]],
    window: (usize, usize),
) -> Vec<Option<usize>> {
    let to_lab = |c: [f64; 3]| perceptual::lab(c.map(|v| v.round().clamp(0.0, 255.0) as u8));
    let cells: Vec<Option<[f64; 3]>> = cells.iter().map(|c| c.map(to_lab)).collect();
    let targets: Vec<[f64; 3]> = targets.iter().map(|&t| to_lab(t)).collect();
    let tonal = tonal_map(&cells, w, h, window);
    diffuse_where(&cells, w, h, &targets, Some(&tonal), |c| {
        [c[0].clamp(0.0, 100.0), c[1], c[2]]
    })
}

/// Largest CIELAB distance (CIE76) from a cell's own colour that still counts as the same flat
/// colour in [`tonal_map`]. Chosen by the owner (2026-10-31) from the similarity score on nine
/// photos at Photo Stitch 1's 3 mm cells: dithering everywhere 55.8, a threshold of 4 60.7,
/// 12 63.8, no dithering 64.4 (smooth gradients then band).
pub const FLAT_DE: f64 = 12.0;
/// Share of a cell's neighbourhood that must be the same flat colour for the cell to be flat.
pub const FLAT_SHARE: f64 = 0.5;

/// Which cells are tonal (B16.1 T6): a cell is flat when at least [`FLAT_SHARE`] of the opaque
/// cells within `window` (half-size in cells, `(columns, rows)`) lie within [`FLAT_DE`] of its
/// own colour, as in a flat area or beside a sharp edge between flat areas; otherwise it is
/// tonal. `cells` are CIELAB. Transparent cells are not tonal. Deterministic.
pub fn tonal_map(cells: &[Option<Lab>], w: usize, h: usize, window: (usize, usize)) -> Vec<bool> {
    let (rx, ry) = window;
    let mut out = vec![false; cells.len()];
    for y in 0..h {
        for x in 0..w {
            let Some(c) = cells[y * w + x] else { continue };
            let (mut near, mut all) = (0usize, 0usize);
            for ny in y.saturating_sub(ry)..(y + ry + 1).min(h) {
                for nx in x.saturating_sub(rx)..(x + rx + 1).min(w) {
                    if let Some(o) = cells[ny * w + nx] {
                        all += 1;
                        let d = (0..3).map(|k| (o[k] - c[k]).powi(2)).sum::<f64>();
                        if d <= FLAT_DE * FLAT_DE {
                            near += 1;
                        }
                    }
                }
            }
            out[y * w + x] = (near as f64) < FLAT_SHARE * all as f64;
        }
    }
    out
}

/// Serpentine Floyd–Steinberg over `cells` in whatever space they and `targets` share;
/// `clamp` keeps a cell's accumulated colour in range before it is matched.
fn diffuse(
    cells: &[Option<[f64; 3]>],
    w: usize,
    h: usize,
    targets: &[[f64; 3]],
    clamp: impl Fn([f64; 3]) -> [f64; 3],
) -> Vec<Option<usize>> {
    diffuse_where(cells, w, h, targets, None, clamp)
}

/// [`diffuse`], with error diffused only between cells `tonal` marks (all when `None`); other
/// cells take the target nearest their own colour.
fn diffuse_where(
    cells: &[Option<[f64; 3]>],
    w: usize,
    h: usize,
    targets: &[[f64; 3]],
    tonal: Option<&[bool]>,
    clamp: impl Fn([f64; 3]) -> [f64; 3],
) -> Vec<Option<usize>> {
    let dithers = |i: usize| tonal.is_none_or(|t| t[i]);
    let mut work: Vec<[f64; 3]> = cells.iter().map(|c| c.unwrap_or([0.0; 3])).collect();
    let mut out = vec![None; cells.len()];
    if targets.is_empty() {
        return out;
    }
    for y in 0..h {
        let forward = y % 2 == 0;
        for k in 0..w {
            let x = if forward { k } else { w - 1 - k };
            let i = y * w + x;
            if cells[i].is_none() {
                continue;
            }
            let c = if dithers(i) {
                clamp(work[i])
            } else {
                clamp(cells[i].unwrap_or([0.0; 3]))
            };
            let mut best = (f64::INFINITY, 0);
            for (t, tc) in targets.iter().enumerate() {
                let d = weighted(c, *tc);
                if d < best.0 {
                    best = (d, t);
                }
            }
            out[i] = Some(best.1);
            if !dithers(i) {
                continue;
            }
            let e = [0, 1, 2].map(|ch| c[ch] - targets[best.1][ch]);
            let dir: isize = if forward { 1 } else { -1 };
            let mut spread = |dx: isize, dy: usize, f: f64| {
                let nx = x as isize + dx;
                let ny = y + dy;
                if nx < 0 || nx >= w as isize || ny >= h {
                    return;
                }
                let j = ny * w + nx as usize;
                if cells[j].is_some() && dithers(j) {
                    for ch in 0..3 {
                        work[j][ch] += e[ch] * f;
                    }
                }
            };
            spread(dir, 0, 7.0 / 16.0);
            spread(-dir, 1, 3.0 / 16.0);
            spread(0, 1, 5.0 / 16.0);
            spread(dir, 1, 1.0 / 16.0);
        }
    }
    out
}

/// Equal-weight squared Euclidean distance on floating-point colours (the dithering error is
/// fractional), the same metric as `openstitch_core::color_distance`.
fn weighted(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (r, g, bl) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    r * r + g * g + bl * bl
}

pub fn rgb_f(t: &Thread) -> [f64; 3] {
    [f64::from(t.red), f64::from(t.green), f64::from(t.blue)]
}

#[cfg(test)]
mod protect_tests {
    use super::*;

    /// The colours of #447's calibration image (447-patches.png, 600 x 400 px) with their pixel
    /// counts, the white background left out as Auto Punch leaves it out.
    fn patches() -> Vec<(Rgb, u64)> {
        vec![
            ([40, 40, 40], 8000),
            ([80, 80, 80], 8000),
            ([120, 120, 120], 8000),
            ([160, 160, 160], 8000),
            ([200, 200, 200], 8000),
            ([230, 230, 230], 8000),
            ([110, 95, 45], 16000),
            ([60, 90, 170], 10000),
            ([70, 140, 60], 10000),
            ([230, 140, 40], 2500),
            ([210, 40, 30], 400),
            ([140, 80, 150], 8000),
            ([200, 150, 90], 14400),
            ([206, 156, 96], 14400),
            ([40, 120, 120], 10800),
            ([46, 126, 116], 10800),
            ([225, 190, 160], 12000),
        ]
    }

    #[test]
    fn a_small_saturated_colour_keeps_its_own_thread_as_in_pe_design() {
        // #447, round 9: PE-DESIGN keeps the 0.17 % red patch's own thread (Red) at 10, 6 and 4
        // colours; median cut alone merged it into a neighbour at every count.
        let census = Census {
            colors: patches(),
            modal: Vec::new(),
            labels: Vec::new(),
        };
        let red = nearest_pec([210, 40, 30]).0;
        for n in [10, 6, 4] {
            let p = Palette::from_census(&census, n);
            assert!(p.chart.contains(&red), "{n} colours: {:?}", p.threads);
            assert!(p.means.len() <= n, "{n} colours: {} boxes", p.means.len());
        }
        // A photo's census (more than PROTECT_FLAT colours) keeps median cut as it was.
        let mut many = patches();
        many.extend((0..60u8).map(|k| ([k * 4, 200 - k, 100], 2000)));
        assert!(protected(&many).is_empty());
        // Nothing small and saturated: as before.
        let plain: Vec<(Rgb, u64)> = patches()
            .into_iter()
            .filter(|c| c.0 != [210, 40, 30])
            .collect();
        assert!(protected(&plain).is_empty());
    }
}

#[cfg(test)]
mod unused_thread_tests {
    use super::*;

    #[test]
    fn two_boxes_near_one_thread_keep_two_threads() {
        // #447: two colours whose nearest PEC thread is the same each keep a thread when the
        // second's nearest unused one is not much further (PE-DESIGN keeps as many threads as
        // it proposes colours); a colour far from every other thread still shares.
        let red = rgb_of(PEC_CHART[5].as_ref().expect("Red"));
        // Red's nearest neighbour on the chart, and a colour 45 % of the way to it: still
        // nearest Red, its neighbour (1.5 times as far, squared) the nearest unused thread.
        let other = pec_basic_chart()
            .iter()
            .enumerate()
            .filter(|(j, c)| *j != 5 && c.is_some())
            .map(|(_, c)| rgb_of(c.as_ref().expect("thread")))
            .min_by_key(|c| color_distance(&thread(*c), &thread(red)))
            .expect("a neighbour");
        let near: Rgb = std::array::from_fn(|k| {
            (f64::from(red[k]) * 0.55 + f64::from(other[k]) * 0.45).round() as u8
        });
        assert_eq!(nearest_pec(near).0, nearest_pec(red).0, "both nearest Red");
        let hist = vec![(red, 1000), (near, 900)];
        let p = Palette::from_histogram(&hist, 2, true);
        assert_eq!(p.means.len(), 2);
        assert_eq!(p.threads.len(), 2, "{:?}", p.threads);
        // One colour: one thread, as before.
        let one = Palette::from_histogram(&[(red, 1000)], 1, true);
        // Photo Stitch's palette (`build`) shares a thread as before.
        assert_eq!(Palette::from_histogram(&hist, 2, false).threads.len(), 1);
        assert_eq!(one.threads.len(), 1);
    }
}

#[cfg(test)]
mod nearest_in_tests {
    use super::*;

    #[test]
    fn nearest_in_takes_the_source_thread_and_keeps_the_pec_chart_by_default() {
        let mine = [
            Thread::rgb(0xffffff),
            Thread::rgb(0xed171f),
            Thread::rgb(0x000000),
        ];
        assert_eq!(nearest_in([240, 240, 240], &mine).rgb_value(), 0xffffff);
        assert_eq!(
            nearest_in([255, 255, 255], &[]),
            nearest_pec([255, 255, 255]).1
        );
    }
}

#[cfg(test)]
mod distinct_small_tests {
    use super::*;

    /// A 200 × 200 picture: white with a large blue disc's worth of blue, and `extra` drawn in
    /// brown ([142, 91, 47], g02's basket: chroma about 35, ΔE 47 from the red).
    fn picture(extra: impl Fn(u32, u32) -> bool) -> Census {
        let image = Raster::from_fn(200, 200, |x, y| {
            if extra(x, y) {
                [142, 91, 47, 255]
            } else if x < 100 {
                [30, 60, 200, 255]
            } else if y < 40 {
                [229, 76, 44, 255]
            } else {
                [255, 255, 255, 255]
            }
        })
        .expect("raster");
        census(&image)
    }

    /// #962: OpenStitch mode's automatic count gives a small, unsaturated colour far from every
    /// other its own thread when it is a solid patch (a 13 × 13 px square, 0.42 % of the
    /// pixels, under AUTO_SHARE), not when it is a two-pixel seam of about the same size.
    #[test]
    fn a_small_distinct_patch_counts_and_a_seam_does_not() {
        let patch = picture(|x, y| (150..163).contains(&x) && (120..133).contains(&y));
        assert_eq!(patch.auto_colors_up_to(AUTO_MAX), 3, "the share rule alone");
        assert_eq!(patch.auto_colors_distinct_up_to(AUTO_MAX, 200), 4);
        let seam = picture(|x, y| (150..152).contains(&x) && (100..190).contains(&y));
        assert_eq!(seam.auto_colors_distinct_up_to(AUTO_MAX, 200), 3);
    }
}
