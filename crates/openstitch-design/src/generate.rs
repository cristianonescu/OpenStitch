//! Independent, deterministic stitch-generation algorithms. Lengths are 0.1 mm.

use crate::geometry::{Contour, Span, Vec2, from_rotated, scan_rows, v2};

/// Needle points sewn one after another; moving between runs is travel.
pub type Run = Vec<Vec2>;

/// Parts of `len` no longer than `max`: `len / max` rounded up, except that a length a
/// whole number of `max` long within float noise (1e-9 of a part) is that many parts (#567:
/// a 10 mm edge at 2.5 mm came out as 4 or 5 stitches depending on where the shape stood).
pub fn parts(len: f64, max: f64) -> usize {
    ((len / max.max(1e-6) - 1e-9).ceil() as usize).max(1)
}

/// The shortest needle step a motif keeps (0.1 mm): closer points stitch in place
/// (`tools/sew_check.py`'s 0.3 mm rule, #617).
pub const MOTIF_MIN_STITCH: f64 = 3.0;

/// `run` without the points closer than `min` to the last point kept, where leaving one out
/// makes no stitch longer than `max` (the pitch the run was split to). The first and last
/// points stay (the end replaces the last kept point when they are too close), so the run still
/// starts and ends where it did and joins its neighbours (#617).
pub fn drop_close(run: &[Vec2], min: f64, max: f64) -> Vec<Vec2> {
    let Some((&end, body)) = run.split_last() else {
        return Vec::new();
    };
    let mut out: Vec<Vec2> = Vec::with_capacity(run.len());
    for (i, &p) in body.iter().enumerate() {
        let next = body.get(i + 1).copied().unwrap_or(end);
        match out.last() {
            Some(&k) if k.distance(p) < min && k.distance(next) <= max => {}
            _ => out.push(p),
        }
    }
    if out.len() > 1
        && out.last().is_some_and(|k| k.distance(end) < min)
        && out[out.len() - 2].distance(end) <= max
    {
        out.pop();
    }
    if out.last().is_none_or(|k| k.distance(end) > 1e-9) {
        out.push(end);
    }
    out
}

/// Keeps every vertex and splits each edge into equal parts no longer than `max_len`.
pub fn subdivide(path: &[Vec2], max_len: f64) -> Vec<Vec2> {
    let Some(&first) = path.first() else {
        return Vec::new();
    };
    let mut out = vec![first];
    for w in path.windows(2) {
        let n = parts(w[0].distance(w[1]), max_len);
        out.extend((1..=n).map(|k| w[0].lerp(w[1], k as f64 / n as f64)));
    }
    out
}

/// Needle points along a contour: every vertex kept, straight segments split into equal
/// stitches no longer than `pitch`, curved segments split by halving the curve's parameter
/// until no chord is longer than `pitch` (see [`halve_curve`]).
pub fn along(contour: &Contour, pitch: f64) -> Vec<Vec2> {
    if !contour.has_curves() {
        return subdivide(&contour.path(), pitch);
    }
    let mut out: Vec<Vec2> = contour.points.first().copied().into_iter().collect();
    for i in 0..contour.segment_count() {
        let (a, b) = contour.segment_ends(i);
        match contour.curve(i) {
            None => out.extend(subdivide(&[a, b], pitch).into_iter().skip(1)),
            Some(c) => out.extend(halve_curve(a, c.c1, c.c2, b, pitch).into_iter().skip(1)),
        }
    }
    out
}

/// Most halvings of a curve's parameter when sampling it for stitches (2^16 chords): a
/// safety stop far beyond any curve a design page holds (an 800 mm circle's quarter arc at a
/// 2 mm pitch takes 9 halvings), so a chord longer than the pitch never survives on a long or
/// tight curve. Until 2026-10-29 the stop was 2^8 chords, which left 2.6 mm stitches on that
/// circle (#121); no PE-DESIGN reference reaches that far, its page being at most 360 mm.
const MAX_CURVE_HALVINGS: u32 = 16;

/// Farthest a curve may bulge from a running stitch's chord (0.1 mm), measured at the chord's
/// middle parameter: a curve shorter than one stitch (a 2 mm scallop or wave of a decorative
/// frame under a 2 mm pitch) was sewn as its chord, flattening the ornament into a straight
/// edge (#464). A curve of radius r bulges pitch² / 8r from its chord, so at the 2-2.5 mm
/// pitches this only halves curves tighter than about 2.5 mm in radius; PE-DESIGN's 60 mm
/// circle (bulge 0.01 mm) keeps its 32 chords per quarter.
const MAX_CURVE_SAG: f64 = 3.0;

/// Distance from `m` to the segment `p`-`q`.
fn sag(m: Vec2, p: Vec2, q: Vec2) -> f64 {
    let d = q - p;
    let l2 = d.x * d.x + d.y * d.y;
    let t = if l2 > 0.0 {
        (((m - p).x * d.x + (m - p).y * d.y) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    m.distance(p + d * t)
}

/// Needle points on the cubic Bézier `a`, `c1`, `c2`, `b` at equal parameter steps, the
/// parameter halved until every chord is no longer than `pitch` and no part of the curve
/// bulges more than [`MAX_CURVE_SAG`] from its chord: PE-DESIGN 11.50 sews a
/// 60 mm circle's running stitch (pitch 2 mm) as 4 × 32 chords of 1.44-1.60 mm whose needle
/// points lie at the quarter arcs' parameters k/32 (compat text-fit-default), not at equal
/// arc length and not on a flatness-tolerance polyline resampled at the pitch (#121: that
/// would move every needle point off PE's). Outlines, by contrast, are flattened adaptively to
/// [`crate::geometry::FLATNESS`], where no reference pins the sampling.
pub fn halve_curve(a: Vec2, c1: Vec2, c2: Vec2, b: Vec2, pitch: f64) -> Vec<Vec2> {
    let at = |t: f64| {
        let u = 1.0 - t;
        a * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + b * (t * t * t)
    };
    let pitch = pitch.max(1e-6);
    let mut halvings = 0;
    loop {
        let n = 1usize << halvings;
        let pts: Vec<Vec2> = (0..=n).map(|k| at(k as f64 / n as f64)).collect();
        let fits = |k: usize| {
            let (p, q) = (pts[k], pts[k + 1]);
            p.distance(q) <= pitch + 1e-9
                && sag(at((k as f64 + 0.5) / n as f64), p, q) <= MAX_CURVE_SAG
        };
        if halvings >= MAX_CURVE_HALVINGS || (0..n).all(fits) {
            return pts;
        }
        halvings += 1;
    }
}

/// Running stitch: vertices kept, segments split to `pitch`; `passes` > 1 sews back and forth.
pub fn running(contour: &Contour, pitch: f64, passes: u32) -> Run {
    let forward = along(contour, pitch);
    if forward.len() < 2 {
        return forward;
    }
    let mut run = forward.clone();
    for pass in 1..passes.max(1) {
        if pass % 2 == 1 {
            run.extend(forward.iter().rev().skip(1).copied());
        } else {
            run.extend(forward.iter().skip(1).copied());
        }
    }
    run
}

/// Triple stitch: every stitch forward, back, forward.
pub fn triple(contour: &Contour, pitch: f64) -> Run {
    let pts = along(contour, pitch);
    let Some(&first) = pts.first() else {
        return Vec::new();
    };
    let mut run = vec![first];
    for w in pts.windows(2) {
        run.extend([w[1], w[0], w[1]]);
    }
    run
}

/// Zigzag line of total `width`, alternating sides every `spacing` along the path.
pub fn zigzag(contour: &Contour, width: f64, spacing: f64) -> Run {
    let path = contour.path();
    if path.len() < 2 || spacing.is_nan() || spacing <= 0.0 {
        return path;
    }
    let mut run = Vec::new();
    let mut side = 1.0;
    let mut carried = 0.0;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        if len <= 0.0 {
            continue;
        }
        let dir = (b - a) * (1.0 / len);
        let normal = v2(-dir.y, dir.x);
        let mut t = carried;
        while t <= len {
            run.push(a + dir * t + normal * (side * width / 2.0));
            side = -side;
            t += spacing;
        }
        carried = t - len;
    }
    run
}

#[derive(Clone, Copy, Debug)]
pub struct RegionParams {
    pub angle: f64,
    pub row_spacing: f64,
    pub stitch_length: f64,
    pub pull_compensation: f64,
}

/// Tatami fill: rows at `angle`, needle points on a `stitch_length` grid shifted a third of a
/// stitch per row so penetrations do not line up.
pub fn fill(contours: &[Contour], p: &RegionParams, holes: &[Contour]) -> Vec<Run> {
    fill_shifted(contours, p, holes, Stagger::Third)
}

/// How the needle grid of a tatami fill moves from row to row (the stagger that keeps the
/// penetrations from lining up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stagger {
    /// A third of a stitch per row, anchored at the origin: OpenStitch's placement before
    /// Fill Frequency existed, kept for `.osd` files without a Frequency.
    Third,
    /// PE-DESIGN's Frequency: the grid moves this fraction of a stitch per row; row k's grid
    /// lies (k + 1) times it from the first row's start (regular brick).
    Regular(f64),
    /// OpenStitch's own random stagger: row k's grid lies at a pseudo-random fraction of a
    /// stitch, drawn from this seed and k, so the output is reproducible and no two rows of
    /// a region line up by rule. PE-DESIGN offers nothing like it (its Fill pane has the
    /// Frequency field only).
    Random(u64),
    /// OpenStitch's stepped texture (#537): the grid takes `n` evenly spaced offsets (the
    /// middles of `n` equal steps of a stitch) in turn,
    /// each held for `repeat` rows, so the needle holes draw diagonals, steps or blocks.
    Steps { n: u32, repeat: u32 },
    /// OpenStitch's curved texture (#537): row k's grid lies `amplitude` (fraction of a stitch)
    /// times the curve at k / `period`, drawing waves, chevrons or scallops into the fill.
    Curve {
        kind: StaggerCurve,
        amplitude: f64,
        period: u32,
    },
}

/// The curve of a [`Stagger::Curve`] texture, from 0 to 1 and back over one period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaggerCurve {
    /// A smooth sine wave.
    Wave,
    /// Straight up and down (zigzag, herringbone, chevron).
    Triangle,
    /// A half sine that drops back at once (scallops).
    Arc,
}

impl StaggerCurve {
    /// The curve at `t` (fraction of a period), 0 to 1.
    pub fn at(self, t: f64) -> f64 {
        let t = t.rem_euclid(1.0);
        match self {
            StaggerCurve::Wave => (1.0 - crate::fmath::Det::dcos(std::f64::consts::TAU * t)) / 2.0,
            StaggerCurve::Triangle => 1.0 - (1.0 - 2.0 * t).abs(),
            StaggerCurve::Arc => crate::fmath::Det::dsin(std::f64::consts::PI * t),
        }
    }
}

impl Stagger {
    /// Offset of row `row` for the textures, fraction of a stitch.
    fn texture(self, row: usize) -> Option<f64> {
        match self {
            Stagger::Steps { n, repeat } => {
                let n = n.max(1) as usize;
                // Centred in each step, so no step's grid sits on the first row's start.
                Some((((row / repeat.max(1) as usize) % n) as f64 + 0.5) / n as f64)
            }
            Stagger::Curve {
                kind,
                amplitude,
                period,
            } => Some((amplitude * kind.at(row as f64 / period.max(1) as f64)).rem_euclid(1.0)),
            _ => None,
        }
    }

    /// Offset of row `row`'s needle grid, fraction of a stitch, from the page origin.
    pub fn phase(self, row: usize) -> f64 {
        match self {
            Stagger::Third => (row % 3) as f64 / 3.0,
            Stagger::Regular(s) => (row as f64 * s).fract(),
            Stagger::Random(seed) => random_fraction(seed, row),
            t => t.texture(row).unwrap_or(0.0),
        }
    }

    /// Offset of row `k`'s grid from the first row's start (PE-DESIGN's anchor, see
    /// [`grid_anchor`]), fraction of a stitch; `None` for the origin-anchored [`Stagger::Third`].
    pub fn anchored(self, k: usize) -> Option<f64> {
        match self {
            Stagger::Third => None,
            Stagger::Regular(f) => Some(((k as f64 + 1.0) * f).fract()),
            Stagger::Random(seed) => Some(random_fraction(seed, k)),
            t => t.texture(k),
        }
    }
}

/// A pseudo-random fraction in [0, 1) for row `row` of a fill seeded with `seed`: one
/// SplitMix64 step (see [`crate::paths::Rng`]) of the seed mixed with the row number, so
/// every row draws its own value and the same seed always gives the same rows.
fn random_fraction(seed: u64, row: usize) -> f64 {
    let mixed = seed ^ (row as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    crate::paths::Rng::new(mixed).next_f64()
}

/// Offset of row `row`'s needle grid, fraction of a stitch (see [`Stagger::phase`]).
pub fn row_phase(row: usize, stagger: Stagger) -> f64 {
    stagger.phase(row)
}

/// [`fill`] whose needle grid moves from row to row as `stagger` says (see [`Stagger`]).
pub fn fill_shifted(
    contours: &[Contour],
    p: &RegionParams,
    holes: &[Contour],
    stagger: Stagger,
) -> Vec<Run> {
    let l = p.stitch_length;
    let mut runs = sew_rows(contours, p, holes, |span, row, forward| {
        let mut xs = tatami_xs(span, stagger.phase(row) * l, l);
        if !forward {
            xs.reverse();
        }
        xs
    });
    runs.iter_mut().for_each(join_short_stitches);
    runs
}

/// A stitch shorter than this is a short stitch for [`join_short_stitches`], 0.1 mm. A fill row
/// 0.3-0.5 mm long becomes a stitch under 0.3 mm (`tools/sew_check.py`'s "stitching in place"
/// bound) once rounded to the PEC's 0.1 mm grid, so the bound is 0.5 mm, not 0.3.
const SHORT_STITCH: f64 = 5.0;
/// Most short stitches in a row a fill keeps: PE-DESIGN 11.50's drawn fills have at most six
/// under 0.5 mm in a row (u-gradation-blend-blue; five in hole-sewing-pentagon-star's tips,
/// three or fewer in the other 72 fill references in `compat/reference`), and six is
/// `tools/sew_check.py`'s limit.
const MAX_SHORT_RUN: usize = 6;

/// Joins a row of more than [`MAX_SHORT_RUN`] stitches under [`SHORT_STITCH`] (#303): fill rows
/// clipped to a sliver of the outline about a stitch's width across (an Auto Punch region's
/// notch, 0.3-0.5 mm) are each a stitch in place, and the needle sews in one spot until the
/// thread breaks or nests. Within such a row of stitches, a needle point closer than
/// [`SHORT_STITCH`] to the last one kept is left out, so the sliver is sewn in a zigzag of
/// stitches at least that long; the run's own last point stays. PE-DESIGN 11.50 sews such
/// slivers as they are (its Auto Punch of p03-storybook has 11 stitches under 0.3 mm in a row,
/// rows 0.2 mm long; round-5 reference), so this goes beyond it; its drawn fills stay within
/// [`MAX_SHORT_RUN`] and are left as they are.
pub fn join_short_stitches(run: &mut Run) {
    let n = run.len();
    if n < MAX_SHORT_RUN + 2 {
        return;
    }
    let short = |i: usize| run[i].distance(run[i + 1]) < SHORT_STITCH;
    let mut out: Run = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        // Stitches i → i + 1, …, j - 1 → j are all short.
        let mut j = i;
        while j + 1 < n && short(j) {
            j += 1;
        }
        if j - i <= MAX_SHORT_RUN {
            out.extend_from_slice(&run[i..=j.max(i)]);
            i = j.max(i) + 1;
            continue;
        }
        out.push(run[i]);
        let first = out.len() - 1;
        for &q in &run[i + 1..=j] {
            if out.last().is_some_and(|&k| k.distance(q) >= SHORT_STITCH) {
                out.push(q);
            }
        }
        if j + 1 == n && out.last() != Some(&run[j]) {
            // The run ends here: its last needle point stays (the lock and the next object's
            // travel start from it).
            if out.len() - 1 > first {
                out.pop();
            }
            out.push(run[j]);
        }
        i = j + 1;
    }
    *run = out;
}

/// Shortest row span [`fill_edge_to_edge`] sews, 0.1 mm.
const MIN_EDGE_ROW_SPAN: f64 = 3.0;

/// [`fill_shifted`] with PE-DESIGN's row placement: the first row lies on one edge of the
/// region and the last on the other (a 40 mm square at 4.5 rows/mm has 181 rows), instead of
/// starting half a row spacing in. Rows on an edge are sampled just inside it. With an
/// `entry` point (design space) the rows chain from the row end nearest it, so the fill starts
/// at the entry instead of travelling along the outline to the first row.
pub fn fill_edge_to_edge(
    contours: &[Contour],
    p: &RegionParams,
    holes: &[Contour],
    stagger: Stagger,
    entry: Option<Vec2>,
    sloped: bool,
) -> Vec<Run> {
    fill_edge_to_edge_travel(contours, p, holes, stagger, entry, sloped, false)
}

/// [`fill_edge_to_edge`] whose travel from the entry point to the first row, between its
/// halves and back walks along the outline when `along` is set (#628), instead of PE-DESIGN's
/// straight run across the region.
pub fn fill_edge_to_edge_travel(
    contours: &[Contour],
    p: &RegionParams,
    holes: &[Contour],
    stagger: Stagger,
    entry: Option<Vec2>,
    sloped: bool,
    along: bool,
) -> Vec<Run> {
    let mut runs = edge_rows_travel(contours, p, holes, stagger, entry, sloped, along, false);
    runs.iter_mut().for_each(join_short_stitches);
    runs
}

/// [`fill_edge_to_edge_travel`] with a region in several sections sewn so that a section
/// joining others comes after them, each of those from its far end towards the one it joins
/// (#469, `RunningStitchPath::InsideHidden`): the travel between them then runs where thread
/// is still to be sewn. A region whose rows are all one span is sewn as before.
pub fn fill_edge_to_edge_travel_hidden(
    contours: &[Contour],
    p: &RegionParams,
    holes: &[Contour],
    stagger: Stagger,
    entry: Option<Vec2>,
    sloped: bool,
) -> Vec<Run> {
    let mut runs = edge_rows_travel(contours, p, holes, stagger, entry, sloped, false, true);
    runs.iter_mut().for_each(join_short_stitches);
    runs
}

/// [`fill_edge_to_edge`] before [`join_short_stitches`].
#[cfg(test)]
pub(crate) fn edge_to_edge_rows(
    contours: &[Contour],
    p: &RegionParams,
    holes: &[Contour],
    stagger: Stagger,
    entry: Option<Vec2>,
    sloped: bool,
) -> Vec<Run> {
    edge_rows_travel(contours, p, holes, stagger, entry, sloped, false, false)
}

// The fill entry points pass their few switches straight through (#469 added `hidden`).
#[allow(clippy::too_many_arguments)]
fn edge_rows_travel(
    contours: &[Contour],
    p: &RegionParams,
    holes: &[Contour],
    stagger: Stagger,
    entry: Option<Vec2>,
    sloped: bool,
    along: bool,
    hidden: bool,
) -> Vec<Run> {
    let l = p.stitch_length;
    let start = entry.map(|e| e.rotated(-p.angle.to_radians()));
    let anchor = grid_anchor(contours, p, holes);
    if let (Some(s), Some(a)) = (start, anchor)
        && let Some(run) = pe_halves(
            &edge_row_spans(contours, p, holes, true),
            p,
            stagger,
            s,
            a,
            sloped,
            along.then_some(contours),
        )
    {
        return vec![run];
    }
    // Regions with several spans per row keep the older order; with a Frequency their rows
    // take PE-DESIGN's spacing (stretched to the far edge, as in its pentagon) and grid (row k
    // at (k + 1)·shift from the rotated bounds' minimum, points down to 0.3 mm from the ends).
    let spans = edge_row_spans(contours, p, holes, stagger.anchored(0).is_some());
    let min_x = anchor;
    // The needle grid moves on in sewing order, as in PE-DESIGN 11: a fill sewn from its last
    // row (an entry at the bottom of a rectangle) counts the rows from there.
    let last = spans.iter().map(|s| s.row).max().unwrap_or(0);
    let from_last = match (start, spans.first(), spans.last()) {
        (Some(st), Some(f), Some(e)) => (st.y - f.y).abs() > (st.y - e.y).abs(),
        _ => false,
    };
    // Main's own walk (#469): which row it sews when and which way, so a reordered region
    // keeps every row's direction and with it the needle-point texture.
    let walk = hidden.then(|| greedy_walk(&spans, start));
    let order = walk
        .as_ref()
        .and_then(|(seq, _)| connectors_last(&spans, seq));
    let directions = walk.as_ref().map(|(_, forward)| forward.as_slice());
    chain_in(
        &spans,
        p,
        |span, row, forward| {
            let mut xs = match (stagger.anchored(row), min_x) {
                (Some(f), Some(min_x)) => {
                    let phase = min_x + f * l;
                    let mut xs = vec![*span.start()];
                    xs.extend(pe_grid(span, phase, l));
                    xs.push(*span.end());
                    xs
                }
                _ => {
                    let row = if from_last { last - row } else { row };
                    tatami_xs(span, stagger.phase(row) * l, l)
                }
            };
            if !forward {
                xs.reverse();
            }
            xs
        },
        Chain::Brick,
        start,
        order.as_deref(),
        directions.filter(|_| order.is_some()),
    )
}

/// The rows in the order [`chain`] sews them (nearest unsewn end first) and the way each is
/// sewn (`true` from its start), as `chain` would with [`Chain::Brick`] from `start` (#469).
fn greedy_walk(spans: &[RowSpan], start: Option<Vec2>) -> (Vec<usize>, Vec<bool>) {
    let mut seq = Vec::with_capacity(spans.len());
    let mut forward = vec![true; spans.len()];
    let Some(first) = spans.first() else {
        return (seq, forward);
    };
    let mut used = vec![false; spans.len()];
    let mut position = start.unwrap_or(v2(*first.range.start(), first.y));
    for _ in 0..spans.len() {
        let (mut best, mut fwd, mut best_d) = (0, true, f64::INFINITY);
        for (i, s) in spans.iter().enumerate().filter(|(i, _)| !used[*i]) {
            let dl = position.distance(v2(*s.range.start(), s.y));
            let du = position.distance(v2(*s.range.end(), s.y));
            if dl < best_d - TIE {
                (best, fwd, best_d) = (i, true, dl);
            }
            if du < best_d - TIE {
                (best, fwd, best_d) = (i, false, du);
            }
        }
        used[best] = true;
        seq.push(best);
        forward[best] = fwd;
        let s = &spans[best];
        position = v2(
            if fwd {
                *s.range.end()
            } else {
                *s.range.start()
            },
            s.y,
        );
    }
    (seq, forward)
}

/// The rows of a region in several sections (#469) in an order where a section joining others
/// is sewn after them: rows that continue one-to-one (each overlaps only the other in the next
/// row) form a section; sections touch where a row overlaps another section's.
/// - Sections in a ring (they touch round a hole) keep the order of `walk` (main's): going
///   round sews them with the least travel. The branches hanging off the ring are sewn just
///   before the ring section they hang from, each branch's own branches first, each from the
///   row farthest from where it hangs towards it.
/// - Without a ring: along a spanning tree rooted at the section touching most, every
///   section's branches come first, in the same way.
///
/// `None` for one section, and for a ring with nothing hanging off it (main's order).
fn connectors_last(spans: &[RowSpan], walk: &[usize]) -> Option<Vec<usize>> {
    let n = spans.len();
    let overlaps = |a: &RowSpan, b: &RowSpan| {
        a.row.abs_diff(b.row) == 1
            && a.range.start() <= b.range.end()
            && b.range.start() <= a.range.end()
    };
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        for j in 0..n {
            if i != j && overlaps(&spans[i], &spans[j]) {
                next[i].push(j);
            }
        }
    }
    // Sections: union of one-to-one continuations.
    let mut section: Vec<usize> = (0..n).collect();
    fn root(s: &mut [usize], mut i: usize) -> usize {
        while s[i] != i {
            s[i] = s[s[i]];
            i = s[i];
        }
        i
    }
    for i in 0..n {
        for &j in &next[i] {
            if spans[j].row == spans[i].row + 1 {
                let up = next[i]
                    .iter()
                    .filter(|&&k| spans[k].row == spans[i].row + 1)
                    .count();
                let down = next[j]
                    .iter()
                    .filter(|&&k| spans[k].row == spans[j].row - 1)
                    .count();
                if up == 1 && down == 1 {
                    let (a, b) = (root(&mut section, i), root(&mut section, j));
                    section[a] = b;
                }
            }
        }
    }
    let ids: Vec<usize> = (0..n).map(|i| root(&mut section, i)).collect();
    let mut names: Vec<usize> = ids.clone();
    names.sort_unstable();
    names.dedup();
    let m = names.len();
    if m < 2 {
        return None;
    }
    let sec: Vec<usize> = ids
        .iter()
        .map(|id| names.binary_search(id).expect("named"))
        .collect();
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); m];
    for (i, &s) in sec.iter().enumerate() {
        members[s].push(i);
    }
    // Sections touching, and where (the rows that touch).
    let mut touch = vec![vec![false; m]; m];
    for i in 0..n {
        for &j in &next[i] {
            if sec[i] != sec[j] {
                touch[sec[i]][sec[j]] = true;
            }
        }
    }
    let degree = |s: usize| touch[s].iter().filter(|&&t| t).count();
    // The ring: what is left after taking off, again and again, sections touching one other.
    let mut in_ring = vec![true; m];
    let mut deg: Vec<usize> = (0..m).map(degree).collect();
    let mut stack: Vec<usize> = (0..m).filter(|&s| deg[s] <= 1).collect();
    while let Some(u) = stack.pop() {
        if !in_ring[u] {
            continue;
        }
        in_ring[u] = false;
        for v in 0..m {
            if touch[u][v] && in_ring[v] {
                deg[v] -= 1;
                if deg[v] <= 1 {
                    stack.push(v);
                }
            }
        }
    }
    // A section's rows from the one farthest from `towards` (a section it touches) to it.
    let rows_towards = |s: usize, towards: Option<usize>| {
        let mut rows = members[s].clone();
        rows.sort_by_key(|&i| spans[i].row);
        if let Some(p) = towards {
            let touches = |i: usize| next[i].iter().any(|&j| sec[j] == p);
            if rows.first().is_some_and(|&i| touches(i)) {
                rows.reverse();
            }
        }
        rows
    };
    if in_ring.iter().any(|&r| r) {
        if in_ring.iter().all(|&r| r) {
            return None;
        }
        // Each section off the ring hangs from its neighbour nearer the ring.
        let mut parent: Vec<Option<usize>> = vec![None; m];
        let mut queue: std::collections::VecDeque<usize> = (0..m).filter(|&s| in_ring[s]).collect();
        let mut seen = in_ring.clone();
        while let Some(u) = queue.pop_front() {
            for v in 0..m {
                if touch[u][v] && !seen[v] {
                    seen[v] = true;
                    parent[v] = Some(u);
                    queue.push_back(v);
                }
            }
        }
        // Ring sections in main's order, each after the branches hanging from it.
        let mut ring_order: Vec<usize> = Vec::new();
        for &i in walk {
            let s = sec[i];
            if in_ring[s] && !ring_order.contains(&s) {
                ring_order.push(s);
            }
        }
        let mut order = Vec::with_capacity(n);
        let mut placed = vec![false; n];
        for &c in &ring_order {
            // The branches off `c`, children first.
            let mut stack: Vec<(usize, bool)> = (0..m)
                .rev()
                .filter(|&v| parent[v] == Some(c))
                .map(|v| (v, false))
                .collect();
            while let Some((u, expanded)) = stack.pop() {
                if expanded {
                    for i in rows_towards(u, parent[u]) {
                        placed[i] = true;
                        order.push(i);
                    }
                    continue;
                }
                stack.push((u, true));
                for v in (0..m).rev() {
                    if parent[v] == Some(u) {
                        stack.push((v, false));
                    }
                }
            }
            // The ring section itself, its rows in main's order.
            for &i in walk {
                if sec[i] == c && !placed[i] {
                    placed[i] = true;
                    order.push(i);
                }
            }
        }
        // Anything not reached (a part apart from the ring) in main's order, at the end.
        order.extend(walk.iter().copied().filter(|&i| !placed[i]));
        return Some(order);
    }
    let mut parent: Vec<Option<usize>> = vec![None; m];
    let mut seen = vec![false; m];
    let mut by_degree: Vec<usize> = (0..m).collect();
    by_degree.sort_by_key(|&s| (std::cmp::Reverse(degree(s)), s));
    let mut roots = Vec::new();
    for &r in &by_degree {
        if seen[r] {
            continue;
        }
        seen[r] = true;
        roots.push(r);
        let mut queue = std::collections::VecDeque::from([r]);
        while let Some(u) = queue.pop_front() {
            for v in 0..m {
                if touch[u][v] && !seen[v] {
                    seen[v] = true;
                    parent[v] = Some(u);
                    queue.push_back(v);
                }
            }
        }
    }
    // Post-order: children (in section order) before their parent.
    let mut sections: Vec<usize> = Vec::with_capacity(m);
    for &r in &roots {
        let mut stack = vec![(r, false)];
        while let Some((u, expanded)) = stack.pop() {
            if expanded {
                sections.push(u);
                continue;
            }
            stack.push((u, true));
            for v in (0..m).rev() {
                if parent[v] == Some(u) {
                    stack.push((v, false));
                }
            }
        }
    }
    // Each section's rows from the one farthest from its parent towards it; a root by row.
    let mut order = Vec::with_capacity(n);
    for s in sections {
        let mut rows = members[s].clone();
        rows.sort_by_key(|&i| spans[i].row);
        if let Some(p) = parent[s] {
            let parent_row = |i: usize| next[i].iter().any(|&j| sec[j] == p);
            // The parent touches the first row: sew from the last one back.
            if rows.first().is_some_and(|&i| parent_row(i)) {
                rows.reverse();
            }
        }
        order.extend(rows);
    }
    Some(order)
}

/// Longest travel stitch of PE-DESIGN's fill (its entry, exit and between-half runs), 0.1 mm.
const FILL_TRAVEL: f64 = 20.0;
/// PE-DESIGN leaves out a needle grid point closer than this to its row's end, 0.1 mm (it
/// keeps 0.4 mm, drops 0.2 mm).
const PE_END_GAP: f64 = 3.0;

/// Where PE-DESIGN's needle grid starts (rotated space): the start of the first row, on the
/// outline's extreme point across the rows (a square's corner, a pentagon's top vertex).
fn grid_anchor(contours: &[Contour], p: &RegionParams, holes: &[Contour]) -> Option<f64> {
    let b = crate::region::rotated_bounds(contours, p.angle)?;
    let y = b.min_y + 1e-3_f64.min(b.height() / 4.0);
    Some(
        crate::region::spans_at(contours, holes, p.angle, y)
            .first()
            .map_or(b.min_x, |s| *s.start()),
    )
}

/// PE-DESIGN's needle grid inside `span` (its ends left out): every `l` from `phase`, without
/// points closer than [`PE_END_GAP`] to an end.
fn pe_grid(span: &Span, phase: f64, l: f64) -> Vec<f64> {
    let (lo, hi) = (*span.start(), *span.end());
    let mut j = ((lo - phase) / l).floor();
    let mut xs = Vec::new();
    loop {
        let x = phase + j * l;
        if x >= hi - PE_END_GAP {
            break;
        }
        if x > lo + PE_END_GAP {
            xs.push(x);
        }
        j += 1.0;
    }
    xs
}

/// PE-DESIGN 11's fill order for a region whose every row is one span (a convex region),
/// starting and ending at `s` (rotated space). The rows on each side of `s` are a half, sewn
/// from its far edge in towards `s`; the row through `s` belongs to both, and a half holding
/// nothing but that row is left out. The half on the rotated minimum side goes first. Each
/// half's first row is sewn along the row direction (+x) and its last row back (−x), so the
/// first row is sewn out and back when the half has an odd number of rows. Rows sewn +x keep
/// both edge points; rows sewn −x have none (the stitches go straight from row to row), except
/// a half's last row, and except when rows are more than [`FILL_TRAVEL`] apart: then every row
/// keeps its ends and the needle walks from row to row in stitches up to that length. Straight
/// runs of stitches up to [`FILL_TRAVEL`] go from `s` to each half's first row and from the last
/// row back to `s`. Row k's needle grid lies `(k + 1)·shift` of a stitch from `min_x` (see
/// [`grid_anchor`]), k counted from the minimum side (PE's Frequency, or the seeded random
/// stagger); with [`Stagger::Third`] the old ⅓ stagger stays.
/// `None` when a row has more than one span or there are no rows.
fn pe_halves(
    spans: &[RowSpan],
    p: &RegionParams,
    stagger: Stagger,
    s: Vec2,
    min_x: f64,
    sloped: bool,
    outline: Option<&[Contour]>,
) -> Option<Run> {
    let (first, last) = (spans.first()?, spans.last()?);
    let rows = last.row.checked_sub(first.row)? + 1;
    if spans.len() != rows || spans.windows(2).any(|w| w[1].row != w[0].row + 1) {
        return None;
    }
    let l = p.stitch_length;
    let gap = if spans.len() > 1 {
        (last.y - first.y) / (spans.len() - 1) as f64
    } else {
        p.row_spacing
    };
    // The row through s: the nearest row within half a spacing.
    let through = spans
        .iter()
        .position(|r| (r.y - s.y).abs() <= gap / 2.0 + 1e-9);
    let low: Vec<&RowSpan> = spans
        .iter()
        .enumerate()
        .filter(|(i, r)| through.map_or(r.y < s.y, |t| *i <= t))
        .map(|(_, r)| r)
        .collect();
    let mut high: Vec<&RowSpan> = spans
        .iter()
        .enumerate()
        .filter(|(i, r)| through.map_or(r.y > s.y, |t| *i >= t))
        .map(|(_, r)| r)
        .collect();
    high.reverse();
    let mut halves = vec![low, high];
    let shared_only = |h: &Vec<&RowSpan>| h.len() == 1 && through.is_some();
    if halves.iter().any(|h| h.len() > 1) {
        halves.retain(|h| !h.is_empty() && !shared_only(h));
    }
    let walk = gap > FILL_TRAVEL;
    let pull = p.pull_compensation.max(0.0);
    let grid = |r: &RowSpan| -> Vec<f64> {
        let k = (r.row - first.row) as f64;
        let phase = match stagger.anchored(k as usize) {
            Some(f) => min_x + f * l,
            None => stagger.phase(r.row) * l,
        };
        pe_grid(&r.range, phase, l)
    };
    let home = from_rotated(s, p.angle);
    let mut run: Run = vec![home];
    // Straight travel in equal stitches up to FILL_TRAVEL; a needle point within 0.1 mm of
    // the target (a row end sampled just inside the outline) is moved onto it.
    let go = |run: &mut Run, to: Vec2| {
        let from = *run.last().expect("starts at s");
        let d = from.distance(to);
        if d < 1.0 {
            *run.last_mut().expect("starts at s") = to;
            return;
        }
        // Along the outline (#628): the travel stays at the region's edge instead of
        // crossing it, where open rows would show it and a calibration swatch would measure it.
        if let Some(outline) = outline {
            run.extend(
                crate::route::travel(outline, from, to, FILL_TRAVEL)
                    .into_iter()
                    .skip(1),
            );
            return;
        }
        let n = ((d / FILL_TRAVEL) - 1e-3).ceil().max(1.0) as usize;
        run.extend((1..=n).map(|k| from.lerp(to, k as f64 / n as f64)));
    };
    for half in halves.iter().filter(|h| !h.is_empty()) {
        if sloped && half.len() > 1 {
            sloped_half(half, &mut run, &go, pull, p, &|b: &RowSpan| {
                // Grid step k · f · L for the pass ending on row k (anchored(k - 1) is k · f).
                let k = b.row - first.row;
                let f = match k.checked_sub(1) {
                    Some(k) => stagger.anchored(k),
                    None => stagger.anchored(0).map(|_| 0.0),
                };
                match f {
                    Some(f) => min_x + f * l,
                    None => stagger.phase(b.row.saturating_sub(1)) * l,
                }
            });
            go(&mut run, home);
            continue;
        }
        let odd = half.len() % 2 == 1;
        let mut passes = vec![(half[0], true)];
        if odd {
            passes.push((half[0], false));
        }
        for (i, r) in half.iter().enumerate().skip(1) {
            passes.push((r, (i % 2 == 1) == odd));
        }
        let n = passes.len();
        for (i, (r, forward)) in passes.into_iter().enumerate() {
            let (lo, hi) = (*r.range.start(), *r.range.end());
            let mut xs = grid(r);
            let ends = forward || walk || i + 1 == n;
            if ends {
                xs.insert(0, lo);
                xs.push(hi);
            }
            if !forward {
                xs.reverse();
            }
            let pts: Vec<Vec2> = xs
                .iter()
                .map(|&x| from_rotated(v2(x, r.y), p.angle))
                .collect();
            let Some(&head) = pts.first() else { continue };
            if i == 0 {
                // Travel reaches the outline; the row end lies the pull compensation beyond.
                go(&mut run, from_rotated(v2(lo + pull, r.y), p.angle));
                run.push(head);
            } else if walk {
                go(&mut run, head);
            } else {
                run.push(head);
            }
            run.extend(pts.into_iter().skip(1));
            if i + 1 == n {
                go(&mut run, from_rotated(v2(lo + pull, r.y), p.angle));
            }
        }
        go(&mut run, home);
    }
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
    Some(run)
}

/// One half of PE-DESIGN's Stitch Type 2 fill (its second Stitch Type button): sloped rows.
/// Pass i runs from row i's edge to row i + 1's far edge, rising one row spacing over its
/// length, so consecutive passes meet at the edges in one continuous zigzag with no step at the
/// row ends (PE-DESIGN 11.50, round-5 captures tatami-40-st2-dir0/30/90 and round 4's
/// tatami-40-stitch-type-2: every pass rises 0.20-0.22 mm, Type 1's are flat). The needle grid
/// along each pass ([`pe_grid`], the Frequency rule) is the one of the row before its end row in
/// row order: the start row in the half sewn upward, two rows past the end in the half sewn
/// downward (tatami-40-st2-dir90's second half), each point at the height interpolated along
/// the pass. The half's first pass is sewn along the row direction
/// (+x) from its edge point, after the travel from the start point; the half ends at its last
/// row's start side, as Type 1 does.
fn sloped_half(
    half: &[&RowSpan],
    run: &mut Run,
    go: &dyn Fn(&mut Run, Vec2),
    pull: f64,
    p: &RegionParams,
    phase: &dyn Fn(&RowSpan) -> f64,
) {
    let l = p.stitch_length;
    let mut forward = true;
    for (i, w) in half.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let (from, to) = if forward {
            (*a.range.start(), *b.range.end())
        } else {
            (*a.range.end(), *b.range.start())
        };
        let (lo, hi) = (from.min(to), from.max(to));
        let mut xs = pe_grid(&(lo..=hi), phase(b), l);
        if !forward {
            xs.reverse();
        }
        let along = (to - from).abs().max(1e-9);
        let at = |x: f64| {
            let t = ((x - from).abs() / along).clamp(0.0, 1.0);
            from_rotated(v2(x, a.y + (b.y - a.y) * t), p.angle)
        };
        if i == 0 {
            // Travel reaches the outline; the row end lies the pull compensation beyond.
            let edge = if forward { from + pull } else { from - pull };
            go(run, from_rotated(v2(edge, a.y), p.angle));
            run.push(at(from));
        }
        run.extend(xs.into_iter().map(at));
        run.push(at(to));
        forward = !forward;
    }
    if let Some(last) = half.last() {
        go(
            run,
            from_rotated(v2(*last.range.start() + pull, last.y), p.angle),
        );
    }
}

/// Rows from the rotated bounds' minimum to their maximum, `row_spacing` apart, lengthened by
/// the pull compensation. With `stretch` the spacing grows so that a whole number of gaps
/// fills the height and the last row lies on the far edge (PE-DESIGN: 149 rows over 33 mm at
/// 4.5 rows/mm, 33/148 mm apart). Spans shorter than
/// [`MIN_EDGE_ROW_SPAN`] are left out: an end row that only touches a curved outline at its
/// tip would otherwise be a stitch a fraction of a millimetre long.
fn edge_row_spans(
    contours: &[Contour],
    p: &RegionParams,
    holes: &[Contour],
    stretch: bool,
) -> Vec<RowSpan> {
    let Some(b) = crate::region::rotated_bounds(contours, p.angle) else {
        return Vec::new();
    };
    let (h, s) = (b.height(), p.row_spacing);
    if !(h > 0.0 && s > 0.0) {
        return Vec::new();
    }
    let n = ((h / s) + 1e-6).floor().clamp(1.0, 100_000.0) as usize;
    let s = if stretch { h / n as f64 } else { s };
    let inside = 1e-3_f64.min(h / 4.0);
    (0..=n)
        .flat_map(|row| {
            // Sampled just past the row's line, so a row on a horizontal edge inside the region
            // (a notch's end) does not run along that edge across the gap.
            let y = (b.min_y + row as f64 * s + inside).clamp(b.min_y + inside, b.max_y - inside);
            crate::region::spans_at(contours, holes, p.angle, y)
                .into_iter()
                .filter_map(move |sp| {
                    let lo = sp.start() - p.pull_compensation;
                    let hi = sp.end() + p.pull_compensation;
                    (hi - lo >= MIN_EDGE_ROW_SPAN).then_some(RowSpan {
                        row,
                        y,
                        range: lo..=hi,
                    })
                })
        })
        .collect()
}

/// Tatami needle positions along `span`: its ends, and the `l` grid shifted by `phase` in
/// between, leaving out grid points within a quarter stitch of an end.
fn tatami_xs(span: &Span, phase: f64, l: f64) -> Vec<f64> {
    let mut xs = vec![*span.start()];
    let mut k = ((span.start() - phase) / l).floor() + 1.0;
    while phase + k * l < *span.end() {
        let x = phase + k * l;
        if x - span.start() > l * 0.25 && span.end() - x > l * 0.25 {
            xs.push(x);
        }
        k += 1.0;
    }
    xs.push(*span.end());
    xs
}

/// Satin region: one needle point per row on alternate edges, so every stitch crosses the
/// region (see [`chain_satin_spans`]).
pub fn satin(contours: &[Contour], p: &RegionParams, holes: &[Contour]) -> Vec<Run> {
    chain_satin_spans(&row_spans(contours, p, holes), p, |span, _, forward| {
        if forward {
            vec![*span.start(), *span.end()]
        } else {
            vec![*span.end(), *span.start()]
        }
    })
}

/// One inside interval of a scan row, in rotated space.
pub struct RowSpan {
    pub row: usize,
    pub y: f64,
    pub range: Span,
}

/// Scanline rows → runs. Spans chain greedily (nearest unsewn span end), entering from the
/// nearer end; a new run starts when the next span is beyond three row spacings plus a stitch.
fn sew_rows(
    contours: &[Contour],
    p: &RegionParams,
    holes: &[Contour],
    points: impl Fn(&Span, usize, bool) -> Vec<f64>,
) -> Vec<Run> {
    chain_spans(&row_spans(contours, p, holes), p, points)
}

/// Inside intervals of every scan row, lengthened by the pull compensation at both ends. A
/// negative pull compensation (an inset) leaves out spans shorter than twice the inset: they
/// would collapse to a needle point outside the region.
fn row_spans(contours: &[Contour], p: &RegionParams, holes: &[Contour]) -> Vec<RowSpan> {
    scan_rows(contours, p.angle, p.row_spacing, holes)
        .into_iter()
        .flat_map(|r| {
            let (row, y) = (r.index, r.y);
            r.spans.into_iter().filter_map(move |s| {
                let lo = s.start() - p.pull_compensation;
                let hi = s.end() + p.pull_compensation;
                (hi >= lo).then_some(RowSpan {
                    row,
                    y,
                    range: lo..=hi,
                })
            })
        })
        .collect()
}

/// Sews prepared row spans (see [`sew_rows`]): greedy nearest-end chaining, needle positions
/// along each span from `points(span, row, forward)`, mapped back from rotated space.
pub fn chain_spans(
    spans: &[RowSpan],
    p: &RegionParams,
    points: impl Fn(&Span, usize, bool) -> Vec<f64>,
) -> Vec<Run> {
    chain(spans, p, points, Chain::Fill, None)
}

/// [`chain_spans`] for satin: when a row follows the previous one along the same edge (the
/// next row, entered within three row distances of the last needle point), its first needle
/// point is left out. The stitch then goes from one edge straight to the far edge of the next
/// row, so needle points alternate between the edges and no short stitch runs along an edge.
/// The first row of each run is sewn across in full.
pub fn chain_satin_spans(
    spans: &[RowSpan],
    p: &RegionParams,
    points: impl Fn(&Span, usize, bool) -> Vec<f64>,
) -> Vec<Run> {
    chain(spans, p, points, Chain::Satin, None)
}

/// [`chain_spans`] for PE-DESIGN's satin sawtooth: when a row follows the previous one along
/// the same edge, it is entered from its far end, so every row is sewn whole in the same
/// direction and the stitch between rows slants back across the column. Rows are then two
/// crossings apart (twice the satin's line spacing).
pub fn chain_sawtooth_spans(
    spans: &[RowSpan],
    p: &RegionParams,
    points: impl Fn(&Span, usize, bool) -> Vec<f64>,
) -> Vec<Run> {
    chain(spans, p, points, Chain::Sawtooth, None)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Chain {
    Fill,
    /// [`Chain::Fill`] with PE-DESIGN's brick row ends (the top fill).
    Brick,
    Satin,
    Sawtooth,
}

/// Distances closer than this (0.1 mm) are equal when [`chain`] picks the nearest span.
const TIE: f64 = 1e-9;

fn chain(
    spans: &[RowSpan],
    p: &RegionParams,
    points: impl Fn(&Span, usize, bool) -> Vec<f64>,
    mode: Chain,
    start: Option<Vec2>,
) -> Vec<Run> {
    chain_in(spans, p, points, mode, start, None, None)
}

/// [`chain`], taking the spans in `order` when given (each entered from its nearer end)
/// instead of always the nearest unsewn one.
fn chain_in(
    spans: &[RowSpan],
    p: &RegionParams,
    points: impl Fn(&Span, usize, bool) -> Vec<f64>,
    mode: Chain,
    start: Option<Vec2>,
    order: Option<&[usize]>,
    directions: Option<&[bool]>,
) -> Vec<Run> {
    let Some(first) = spans.first() else {
        return Vec::new();
    };
    let mut used = vec![false; spans.len()];
    let mut runs: Vec<Run> = Vec::new();
    let mut current: Run = Vec::new();
    let mut position = start.unwrap_or(v2(*first.range.start(), first.y));
    let join_limit = p.row_spacing * 3.0 + p.stitch_length;
    let mut last_row: Option<(usize, f64)> = None;
    let mut trim_end = false;
    for step in 0..spans.len() {
        let (mut best, mut forward, mut best_d) = (0, true, f64::INFINITY);
        let fixed = order.and_then(|o| o.get(step).copied());
        for (i, s) in spans
            .iter()
            .enumerate()
            .filter(|(i, _)| !used[*i] && fixed.is_none_or(|f| f == *i))
        {
            let dl = position.distance(v2(*s.range.start(), s.y));
            let du = position.distance(v2(*s.range.end(), s.y));
            // Ties within float noise go to the first span (#567): a span whose ends are
            // equally near, as rows at 30 degrees are, was picked by the last bits of where
            // the shape stood.
            if dl < best_d - TIE {
                (best, forward, best_d) = (i, true, dl);
            }
            if du < best_d - TIE {
                (best, forward, best_d) = (i, false, du);
            }
        }
        // A fixed way per row (main's, #469): the row keeps its direction and texture.
        if let Some(&way) = directions.and_then(|d| d.get(best)) {
            let s = &spans[best];
            forward = way;
            best_d = position.distance(v2(
                if way {
                    *s.range.start()
                } else {
                    *s.range.end()
                },
                s.y,
            ));
        }
        used[best] = true;
        let s = &spans[best];
        if best_d > join_limit && !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
        let along_edge = !current.is_empty()
            && last_row.is_some_and(|(row, y)| {
                row.abs_diff(s.row) == 1 && best_d <= 3.0 * (s.y - y).abs() + 1e-9
            });
        let forward = if mode == Chain::Sawtooth && along_edge {
            !forward
        } else {
            forward
        };
        let xs = points(&s.range, s.row, forward);
        // Brick: PE-DESIGN's row ends. A row sewn backwards straight after the row next
        // to it has no edge points: the stitches run diagonally from the previous row's end to
        // its first grid point, and from its last grid point to the next row's start.
        // Rows shorter than a stitch keep their ends (the narrow rows at a pentagon's vertices,
        // hole-sewing-pentagon-star). PE-DESIGN drops them on a letter's 3.5 mm strokes
        // (text-size-h); the rule between the two is not settled.
        let brick = mode == Chain::Brick
            && !forward
            && along_edge
            && xs.len() > 2
            && s.range.end() - s.range.start() >= p.stitch_length;
        if mode == Chain::Brick && along_edge && trim_end && current.len() > 1 {
            current.pop();
        }
        trim_end = brick;
        let skip = usize::from((mode == Chain::Satin && xs.len() >= 2 && along_edge) || brick);
        current.extend(
            xs.iter()
                .skip(skip)
                .map(|&x| from_rotated(v2(x, s.y), p.angle)),
        );
        if let Some(&x) = xs.last() {
            position = v2(x, s.y);
        }
        last_row = Some((s.row, s.y));
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs
}
