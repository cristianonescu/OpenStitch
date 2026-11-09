//! Regions sewn along several direction lines (PE-DESIGN's Manual Punch shapes): consecutive
//! lines are the rungs of a column; stitches follow directions interpolated between them.
//! Satin zigzags across the column; fill sews rows parallel to the interpolated rungs. Own
//! algorithm. Lengths are 0.1 mm.

use crate::fmath::Det;
use crate::generate::{Run, subdivide};
use crate::geometry::{Bezier, Contour, Vec2, v2};
use crate::model::{RegionSewing, RegionType, UnderlayParts, UnderlayStyle};
use serde::{Deserialize, Serialize};

/// The closed outline of a column: the left ends in order, then the right ends back.
pub fn outline(rungs: &[[Vec2; 2]]) -> Contour {
    let mut pts: Vec<Vec2> = rungs.iter().map(|r| r[0]).collect();
    pts.extend(rungs.iter().rev().map(|r| r[1]));
    Contour::new(pts, true)
}

/// Curved Block: each rail (the left ends, the right ends) as a smooth curve through its
/// points — Catmull-Rom segments written as cubic Béziers, the end tangents along the chord.
fn rail_curves(rail: &[Vec2]) -> Vec<Bezier> {
    let n = rail.len();
    (0..n.saturating_sub(1))
        .map(|i| {
            let prev = rail[i.saturating_sub(1)];
            let next = rail[(i + 2).min(n - 1)];
            Bezier {
                c1: rail[i] + (rail[i + 1] - prev) * (1.0 / 6.0),
                c2: rail[i + 1] - (next - rail[i]) * (1.0 / 6.0),
            }
        })
        .collect()
}

fn bezier_at(a: Vec2, c: Bezier, b: Vec2, t: f64) -> Vec2 {
    let s = 1.0 - t;
    a * (s * s * s) + c.c1 * (3.0 * s * s * t) + c.c2 * (3.0 * s * t * t) + b * (t * t * t)
}

/// The closed outline of a Curved Block column: both rails curved through the line ends, the
/// first and last lines straight.
pub fn curved_outline(rungs: &[[Vec2; 2]]) -> Contour {
    let mut c = outline(rungs);
    let n = rungs.len();
    if n < 3 {
        return c;
    }
    let left: Vec<Vec2> = rungs.iter().map(|r| r[0]).collect();
    let right: Vec<Vec2> = rungs.iter().map(|r| r[1]).collect();
    let mut curves: Vec<Option<Bezier>> = rail_curves(&left).into_iter().map(Some).collect();
    curves.push(None);
    // The right rail runs backwards in the outline.
    curves.extend(
        rail_curves(&right)
            .into_iter()
            .rev()
            .map(|z| Some(Bezier { c1: z.c2, c2: z.c1 })),
    );
    curves.push(None);
    c.curves = curves;
    c
}

/// Curved Block: the direction lines between the clicked ones, following the curved rails
/// (about one every 2 mm of rail), clicked lines included.
pub fn curved_rungs(rungs: &[[Vec2; 2]]) -> Vec<[Vec2; 2]> {
    if rungs.len() < 3 {
        return rungs.to_vec();
    }
    let left: Vec<Vec2> = rungs.iter().map(|r| r[0]).collect();
    let right: Vec<Vec2> = rungs.iter().map(|r| r[1]).collect();
    let (lc, rc) = (rail_curves(&left), rail_curves(&right));
    let mut out = vec![rungs[0]];
    for i in 0..rungs.len() - 1 {
        let len = left[i]
            .distance(left[i + 1])
            .max(right[i].distance(right[i + 1]));
        let steps = ((len / 20.0).ceil() as usize).clamp(1, 64);
        for k in 1..=steps {
            let t = k as f64 / steps as f64;
            out.push([
                bezier_at(left[i], lc[i], left[i + 1], t),
                bezier_at(right[i], rc[i], right[i + 1], t),
            ]);
        }
    }
    out
}

/// Whether segments a–b and c–d cross (touching ends count).
fn cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let o = |p: Vec2, q: Vec2, r: Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
    (d1 * d2 <= 0.0) && (d3 * d4 <= 0.0) && !(d1 == 0.0 && d2 == 0.0)
}

/// Why rungs cannot make a column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RungError {
    TooFew,
    /// A rung shorter than 0.1 mm.
    Degenerate(usize),
    /// Rungs `i` and `i + 1`, or the rails between them, cross.
    Crossing(usize),
}

pub fn validate(rungs: &[[Vec2; 2]]) -> Result<(), RungError> {
    if rungs.len() < 2 {
        return Err(RungError::TooFew);
    }
    if let Some(i) = rungs.iter().position(|r| r[0].distance(r[1]) < 1.0) {
        return Err(RungError::Degenerate(i));
    }
    for (i, w) in rungs.windows(2).enumerate() {
        let ([a, b], [c, d]) = (w[0], w[1]);
        // Neighbouring rungs must not cross, nor may the left and right rails.
        if cross(a, b, c, d) || cross(a, c, b, d) {
            return Err(RungError::Crossing(i));
        }
    }
    Ok(())
}

/// Untangles twisted rungs (#525): a rung clicked right-then-left, so that the rails between
/// it and the rung before cross, is turned round. Returns the rungs and how many were turned.
/// Rungs that cross each other (a real overlap) stay, and [`validate`] still reports them.
pub fn untangle(rungs: &[[Vec2; 2]]) -> (Vec<[Vec2; 2]>, usize) {
    let mut out = rungs.to_vec();
    let mut turned = 0;
    for i in 1..out.len() {
        let ([a, b], [c, d]) = (out[i - 1], out[i]);
        if cross(a, c, b, d) && !cross(a, d, b, c) {
            out[i] = [d, c];
            turned += 1;
        }
    }
    (out, turned)
}

/// The column with its two sides swapped (#525): every crossing starts from the other rail.
/// The needle points stay the same; their order is mirrored across the column.
pub fn swap_sides(rungs: &[[Vec2; 2]]) -> Vec<[Vec2; 2]> {
    rungs.iter().map(|&[l, r]| [r, l]).collect()
}

/// Points along the column, `spacing` apart on the longer rail of each section: one
/// interpolated rung per step, first and last rungs included.
fn rungs_at(rungs: &[[Vec2; 2]], spacing: f64) -> Vec<[Vec2; 2]> {
    let mut out = vec![rungs[0]];
    for w in rungs.windows(2) {
        let ([a, b], [c, d]) = (w[0], w[1]);
        let len = a.distance(c).max(b.distance(d));
        let n = ((len / spacing.max(0.1)).ceil() as usize).clamp(1, 100_000);
        for k in 1..=n {
            let t = k as f64 / n as f64;
            out.push([a.lerp(c, t), b.lerp(d, t)]);
        }
    }
    out
}

/// Rungs along the column `pitch` apart measured across each rung, as [`crate::lines::
/// zigzag_rails`] spaces a zigzag line's stitches: a rung leaning along the column steps
/// further, so the thread density stays the same. First and last rungs included.
fn rungs_leaning(rungs: &[[Vec2; 2]], pitch: f64) -> Vec<[Vec2; 2]> {
    let mut out = vec![rungs[0]];
    for w in rungs.windows(2) {
        let ([a, b], [c, d]) = (w[0], w[1]);
        let len = mid(w[0]).distance(mid(w[1]));
        let Some(u) = unit(mid(w[1]) - mid(w[0])) else {
            continue;
        };
        let mut f = 0.0;
        loop {
            let r = b.lerp(d, f) - a.lerp(c, f);
            // A rung leaning along the column steps further; a point (a tapered end) or a
            // rung along the column steps as a square one, so every step moves on.
            let across = cross2(r, u).abs();
            let lean = if across > 1e-6 {
                (r.length() / across).clamp(1.0, 10.0)
            } else {
                1.0
            };
            f += pitch.max(0.1) * lean / len.max(1e-9);
            if f >= 1.0 - 1e-9 {
                break;
            }
            out.push([a.lerp(c, f), b.lerp(d, f)]);
        }
        out.push(w[1]);
    }
    out
}

/// Rungs evenly spaced along the column, at least `pitch` apart on the longer rail of each
/// section (the number of steps rounded down, as PE-DESIGN 11 places Manual Punch satin),
/// first and last rungs included.
fn rungs_within_pitch(rungs: &[[Vec2; 2]], pitch: f64) -> Vec<[Vec2; 2]> {
    rungs_within_pitch_at(rungs, pitch, 1.0)
}

/// [`rungs_within_pitch`] with the spacing measured `at` across the column (#528): 1 on the
/// longer side (PE-DESIGN's), 0 on the shorter side, 0.5 halfway.
fn rungs_within_pitch_at(rungs: &[[Vec2; 2]], pitch: f64, at: f64) -> Vec<[Vec2; 2]> {
    let mut out = vec![rungs[0]];
    for w in rungs.windows(2) {
        let ([a, b], [c, d]) = (w[0], w[1]);
        let (l, r) = (a.distance(c), b.distance(d));
        let len = if at >= 1.0 {
            l.max(r)
        } else {
            l.min(r) + at.max(0.0) * (l.max(r) - l.min(r))
        };
        let n = ((len / pitch.max(0.1) + 1e-9).floor() as usize).clamp(1, 100_000);
        for k in 1..=n {
            let t = k as f64 / n as f64;
            out.push([a.lerp(c, t), b.lerp(d, t)]);
        }
    }
    out
}

/// Rungs evenly `pitch` apart along a finely sampled column (skeleton satin, Auto satin),
/// measured `at` across it (#685, see [`SatinShaping::spacing_at`]): the column's length on
/// that line divided into a whole number of steps, each rung interpolated between the two
/// samples around it. First and last rungs kept.
fn rungs_even_at(rungs: &[[Vec2; 2]], pitch: f64, at: f64) -> Vec<[Vec2; 2]> {
    if rungs.len() < 2 {
        return rungs.to_vec();
    }
    let at = at.clamp(0.0, 1.0);
    let mut measure = vec![0.0];
    for w in rungs.windows(2) {
        let (l, r) = (w[0][0].distance(w[1][0]), w[0][1].distance(w[1][1]));
        let len = l.min(r) + at * (l.max(r) - l.min(r));
        measure.push(measure[measure.len() - 1] + len);
    }
    let total = measure[measure.len() - 1];
    let n = ((total / pitch.max(0.1)).round() as usize).clamp(1, 100_000);
    (0..=n)
        .map(|k| {
            let target = total * k as f64 / n as f64;
            let j = measure
                .partition_point(|&m| m < target)
                .clamp(1, rungs.len() - 1);
            let span = measure[j] - measure[j - 1];
            let t = if span > 1e-9 {
                ((target - measure[j - 1]) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let ([a, b], [c, d]) = (rungs[j - 1], rungs[j]);
            [a.lerp(c, t), b.lerp(d, t)]
        })
        .collect()
}

/// Satin across the column as PE-DESIGN 11 sews Manual Punch satin: a sawtooth (see
/// [`sawtooth`]) of rungs two crossings apart (`spacing` is the distance between crossings),
/// each rung lengthened by `pull` at both ends.
pub fn satin(rungs: &[[Vec2; 2]], spacing: f64, pull: f64) -> Run {
    sawtooth(
        rungs_within_pitch(rungs, 2.0 * spacing)
            .into_iter()
            .map(|[l, r]| {
                let len = l.distance(r).max(1e-9);
                let d = (r - l) * (pull / len);
                [l - d, r + d]
            }),
    )
}

/// Satin needle points from rungs [left, right] in sewing order, every rung sewn from left to
/// right: the stitch along each rung crosses the column in the sewing direction, the one to
/// the next rung slants back. Every stitch crosses the column.
pub fn sawtooth(rungs: impl IntoIterator<Item = [Vec2; 2]>) -> Run {
    rungs.into_iter().flatten().collect()
}

/// Satin needle points from rungs [left, right] in sewing order: the first rung is sewn
/// across (left, right), then each later rung gives one needle point, left on odd rungs and
/// right on even ones. Consecutive points are on opposite sides, so every stitch crosses the
/// column and none runs along its edge.
pub fn zigzag(rungs: impl IntoIterator<Item = [Vec2; 2]>) -> Run {
    let mut run = Vec::new();
    for (k, [l, r]) in rungs.into_iter().enumerate() {
        match k {
            0 => run.extend([l, r]),
            _ if k % 2 == 1 => run.push(l),
            _ => run.push(r),
        }
    }
    run
}

/// Fill: rows along the interpolated rungs, `spacing` apart, needle points at most
/// `stitch_length` apart with a third-of-a-stitch stagger per row, sewn back and forth.
pub fn fill(rungs: &[[Vec2; 2]], spacing: f64, stitch_length: f64, pull: f64) -> Run {
    let mut run = Vec::new();
    for (k, [l, r]) in rungs_at(rungs, spacing).into_iter().enumerate() {
        let len = l.distance(r).max(1e-9);
        let d = (r - l) * (pull / len);
        let (l, r) = (l - d, r + d);
        let (from, to) = if k % 2 == 0 { (l, r) } else { (r, l) };
        let total = from.distance(to);
        let phase = (k % 3) as f64 / 3.0 * stitch_length;
        let mut row = vec![from];
        let mut s = phase.max(stitch_length * 0.25);
        while s < total - stitch_length * 0.25 {
            row.push(from.lerp(to, s / total));
            s += stitch_length;
        }
        row.push(to);
        run.extend(subdivide(&row, stitch_length));
    }
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
    run
}

/// Inset of a column's under sewing from its outline (0.1 mm): PE-DESIGN 11.50 sews a Manual
/// Punch column's under sewing 0.5 mm inside the rails and the end rungs (round-4 captures
/// satin-col-us-*, a 39.5 × 6 mm straight column: edge run and zigzag on x 11.0–49.5,
/// y 10.5–15.5).
const UNDERLAY_INSET: f64 = 5.0;
/// Longest stitch (0.1 mm) of the column under sewing's edge run, piping and the travel
/// between its parts: equal stitches up to 2.5 mm (PE-DESIGN: 2.41 mm along a 38.5 mm rail,
/// 2.5 across the 5 mm end).
const UNDERLAY_PITCH: f64 = 25.0;

/// The rungs of a column moved `inset` (PE-DESIGN: [`UNDERLAY_INSET`]) inside its outline: each rung shortened at
/// both ends, the first and last rung moved inward along the column.
fn inset_rungs(rungs: &[[Vec2; 2]], inset: f64) -> Vec<[Vec2; 2]> {
    let n = rungs.len();
    let mut out: Vec<[Vec2; 2]> = rungs
        .iter()
        .map(|&[l, r]| {
            let len = l.distance(r);
            if len <= 2.0 * inset + 1.0 {
                let m = l.lerp(r, 0.5);
                return [m, m];
            }
            let d = (r - l) * (inset / len);
            [l + d, r - d]
        })
        .collect();
    if n >= 2 {
        for (i, j) in [(0, 1), (n - 1, n - 2)] {
            let along = [rungs[j][0] - rungs[i][0], rungs[j][1] - rungs[i][1]];
            for side in 0..2 {
                let len = along[side].length();
                if len > inset {
                    out[i][side] = out[i][side] + along[side] * (inset / len);
                }
            }
        }
    }
    out
}

/// The under sewing of a Manual Punch column as PE-DESIGN 11.50 sews it (round-4 captures of a
/// 39.5 × 6 mm straight satin column, 4.5 lines/mm, Density Medium), from `start` (the
/// column's start point) and back to it, as one run: the edge run [`UNDERLAY_INSET`] inside
/// the outline in equal stitches up to [`UNDERLAY_PITCH`], entered at the inset point nearest
/// the start (PE: 2 × 16 stitches of 2.41 mm along the rails and 2.5 mm across the ends; 39
/// stitches in all with the travel); the piping, a running stitch down the middle of the
/// column from the start end (16 × 2.41 mm); and the zigzag between the inset rails from the
/// far end back, its needle points `spacing` apart along the column (a whole number of equal
/// gaps, 19 of 2.03 mm over 38.5 mm at Medium) with every diagonal split into equal stitches
/// no longer than `stitch` (PE: 42 stitches for the 19 diagonals of 5.4 mm). Which parts are
/// sewn follows `parts` (PE's "Edge only", "Region only (zigzag)", "Region only (piping)",
/// "Edge and region (zigzag)" and "Region only (zigzag and piping)"; the captures show edge +
/// piping + zigzag at 286 stitches against 186 without under sewing, edge alone 225, piping +
/// zigzag 249). Empty with fewer than two rungs or nothing to sew.
pub fn underlay(
    rungs: &[[Vec2; 2]],
    parts: crate::model::UnderlayParts,
    spacing: f64,
    stitch: f64,
    start: Vec2,
) -> Run {
    underlay_at(
        rungs,
        parts,
        spacing,
        stitch,
        start,
        UNDERLAY_INSET,
        UNDERLAY_PITCH,
    )
}

/// [`underlay`] `inset` (0.1 mm) inside the outline with its edge run, piping and travel in
/// stitches up to `pitch` (0.1 mm): the second under sewing layer (#523).
pub fn underlay_at(
    rungs: &[[Vec2; 2]],
    parts: crate::model::UnderlayParts,
    spacing: f64,
    stitch: f64,
    start: Vec2,
    inset: f64,
    pitch: f64,
) -> Run {
    if rungs.len() < 2 || !parts.any() {
        return Vec::new();
    }
    let pitch = pitch.max(1.0);
    let inner = inset_rungs(rungs, inset.max(0.0));
    let n = inner.len();
    let near_first = start.distance(inner[0][0]).min(start.distance(inner[0][1]))
        <= start
            .distance(inner[n - 1][0])
            .min(start.distance(inner[n - 1][1]));
    // The column in sewing order: its start end first.
    let inner: Vec<[Vec2; 2]> = if near_first {
        inner
    } else {
        inner.into_iter().rev().collect()
    };
    let mut run = vec![start];
    let go = |run: &mut Run, to: Vec2| {
        let from = *run.last().expect("starts at the start point");
        run.extend(subdivide(&[from, to], pitch).into_iter().skip(1));
    };
    if parts.edge {
        let outline = outline(&inner);
        let at = crate::route::snap(std::slice::from_ref(&outline), start);
        let edge = crate::generate::running(&outline.starting_at(at), pitch, 1);
        if let Some(&first) = edge.first() {
            go(&mut run, first);
            run.extend(edge.into_iter().skip(1));
        }
    }
    if parts.center {
        let mids: Vec<Vec2> = inner.iter().map(|[l, r]| l.lerp(*r, 0.5)).collect();
        let line = subdivide(&mids, pitch);
        if let Some(&first) = line.first() {
            go(&mut run, first);
            run.extend(line.into_iter().skip(1));
        }
    }
    if parts.zigzag {
        // Stations along the column, a whole number of equal gaps per section nearest to
        // `spacing`, from the far end back to the start end.
        let mut stations: Vec<[Vec2; 2]> = vec![inner[0]];
        for w in inner.windows(2) {
            let ([a, b], [c, d]) = (w[0], w[1]);
            let len = a.distance(c).max(b.distance(d));
            let k = ((len / spacing.max(0.1)).round() as usize).clamp(1, 100_000);
            stations.extend((1..=k).map(|i| {
                let t = i as f64 / k as f64;
                [a.lerp(c, t), b.lerp(d, t)]
            }));
        }
        stations.reverse();
        // The diagonals alternate rails from the far end and end on the rail nearer the start
        // point (PE-DESIGN: 20 stations, the first on the far end's bottom rail, the last on the
        // top rail next to the start point at the column's top-left corner).
        let start_side = usize::from(start.distance(inner[0][1]) < start.distance(inner[0][0]));
        let mut side = if stations.len().is_multiple_of(2) {
            1 - start_side
        } else {
            start_side
        };
        let mut path = Vec::new();
        for s in &stations {
            path.push(s[side]);
            side = 1 - side;
        }
        let zig = subdivide(&path, stitch.max(1.0));
        if let Some(&first) = zig.first() {
            go(&mut run, first);
            run.extend(zig.into_iter().skip(1));
        }
    }
    go(&mut run, start);
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
    run
}

// OpenStitch's satin shaping beyond PE-DESIGN (#520 corners, #521 short stitches, #522
// compensation per side and per cent of width, #523 automatic and second under sewing). Every
// option is off by default, and a column or zigzag line without them sews exactly as before.

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// How a satin sews a sharp corner (#520).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CornerStyle {
    /// Every corner turned: the stitches fan round it (PE-DESIGN's output, the default).
    #[default]
    Turn,
    /// By angle: a cap below `cap_below`, a mitre below `mitre_below`, turned otherwise.
    Auto,
    /// Every corner sharper than `mitre_below` mitred.
    Mitre,
    /// Every corner sharper than `mitre_below` capped (mitred where the cap would be longer
    /// than `cap_max_length`).
    Cap,
}

impl CornerStyle {
    pub const ALL: [CornerStyle; 4] = [
        CornerStyle::Turn,
        CornerStyle::Auto,
        CornerStyle::Mitre,
        CornerStyle::Cap,
    ];

    pub fn id(self) -> &'static str {
        match self {
            CornerStyle::Turn => "turn",
            CornerStyle::Auto => "auto",
            CornerStyle::Mitre => "mitre",
            CornerStyle::Cap => "cap",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }
}

/// Corner construction of a satin (#520). A corner is a vertex of the centre line (the chain
/// of rung midpoints) whose interior angle is below a threshold. A **mitre** splits the
/// column at the corner's rung (the seam from the inner corner to the outer tip): the stitches
/// of each leg stay square to that leg and are cut at the seam, each leg running `overlap`
/// past it. A **cap** ends both legs on a square line through the inner corner and sews the
/// outer tip as its own short column, reached by a running stitch hidden under it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Corners {
    pub style: CornerStyle,
    /// Interior angle (degrees) below which a corner is mitred or capped (default 75).
    pub mitre_below: f64,
    /// Interior angle (degrees) below which `auto` caps a corner (default 45).
    pub cap_below: f64,
    /// How far each leg of a mitre, and a cap into its leg, runs past the cut, mm (0.5).
    pub overlap: f64,
    /// Longest cap stitch, mm (8): a longer cap is mitred instead.
    pub cap_max_length: f64,
}

impl Default for Corners {
    fn default() -> Self {
        Corners {
            style: CornerStyle::Turn,
            mitre_below: 75.0,
            cap_below: 45.0,
            overlap: 0.5,
            cap_max_length: 8.0,
        }
    }
}

/// Short stitches on the inside of curves (#521): where a needle point lands closer than
/// `distance` to the last full-length one on the same edge, it is pulled in by `inset` % of
/// the stitch's width, so every other crowded point stops short.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShortStitches {
    /// mm; 0 turns short stitches off (the default). 0.25 suits most satins.
    pub distance: f64,
    /// % of the stitch's width (default 15).
    pub inset: f64,
}

impl Default for ShortStitches {
    fn default() -> Self {
        ShortStitches {
            distance: 0.0,
            inset: 15.0,
        }
    }
}

/// Pull compensation per side and as a percentage of each stitch's width (#522), on top of
/// PE-DESIGN's single value (`pull_compensation` of a region): each end of a stitch `w` long
/// is extended by its side's amount (the single value where unset) plus `percent` % of `w`,
/// at most `max`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Compensation {
    /// Left side (the start rail of the first stitch), mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<f64>,
    /// Right side, mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<f64>,
    /// % of each stitch's own width added to each side.
    #[serde(skip_serializing_if = "is_default")]
    pub percent: f64,
    /// Largest extension of one side, mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

impl Compensation {
    /// The extension (0.1 mm) of the left and right end of a stitch `width` long (0.1 mm),
    /// from the single value `pull` (0.1 mm).
    pub fn sides(&self, pull: f64, width: f64) -> [f64; 2] {
        let pct = self.percent / 100.0 * width;
        let cap = |v: f64| self.max.map_or(v, |m| v.min(m * 10.0));
        [
            cap(self.left.map_or(pull, |l| l * 10.0) + pct),
            cap(self.right.map_or(pull, |r| r * 10.0) + pct),
        ]
    }
}

/// The satin shaping options of a column or zigzag line (see the module notes above).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SatinShaping {
    #[serde(skip_serializing_if = "is_default")]
    pub corners: Corners,
    #[serde(skip_serializing_if = "is_default")]
    pub short_stitches: ShortStitches,
    #[serde(skip_serializing_if = "is_default")]
    pub compensation: Compensation,
    /// The satin's look (#528): normal, S-stitch, open or double.
    #[serde(skip_serializing_if = "is_default")]
    pub style: SatinStyle,
    /// A jagged edge (#528).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jagged: Option<Jagged>,
    /// Where across a Manual Punch column the density is measured (#528), 0 (shorter side)
    /// to 1 (longer side, PE-DESIGN's; unset).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spacing_at: Option<f64>,
    /// Padding lanes under the satin for a raised look (#531).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raised: Option<Raised>,
}

impl SatinShaping {
    /// Whether nothing is set: the satin sews exactly as PE-DESIGN's.
    pub fn is_plain(&self) -> bool {
        self.corners.style == CornerStyle::Turn
            && self.short_stitches.distance <= 0.0
            && self.compensation == Compensation::default()
            && self.style == SatinStyle::Normal
            && self.jagged.is_none()
            && self.spacing_at.is_none_or(|a| a >= 1.0)
            && self.raised.is_none()
    }

    /// The rung pitch for `pitch`: three times wider for the open style.
    fn pitch(&self, pitch: f64) -> f64 {
        if self.style == SatinStyle::Open {
            3.0 * pitch
        } else {
            pitch
        }
    }
}

/// The look of a satin (#528, beyond PE-DESIGN).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SatinStyle {
    #[default]
    Normal,
    /// Each crossing sewn as three short slanted stitches in an S: a corded edge.
    SStitch,
    /// A light satin: the rows three times as far apart.
    Open,
    /// The satin sewn twice, the second pass back from the end with its needle points midway
    /// between the first's, so the crossings form an X lattice.
    Double,
}

impl SatinStyle {
    pub const ALL: [SatinStyle; 4] = [
        SatinStyle::Normal,
        SatinStyle::SStitch,
        SatinStyle::Open,
        SatinStyle::Double,
    ];

    pub fn id(self) -> &'static str {
        match self {
            SatinStyle::Normal => "normal",
            SatinStyle::SStitch => "sStitch",
            SatinStyle::Open => "open",
            SatinStyle::Double => "double",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }
}

/// Which way a jagged edge's needle points move.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JaggedMode {
    /// Into the column (the outline stays the limit).
    #[default]
    In,
    /// Out past the edge.
    Out,
    /// Both ways.
    Both,
}

/// Which edges are jagged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JaggedSide {
    #[default]
    Both,
    /// The left edge (the first stitch's start).
    Left,
    Right,
}

/// A jagged edge for fur, grass and feathers (#528): each needle point on the chosen edges
/// moves by a pseudo-random share of `amount` across the column, the same for the same
/// `seed`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Jagged {
    pub mode: JaggedMode,
    pub side: JaggedSide,
    /// Largest move, mm (default 1).
    pub amount: f64,
    pub seed: u64,
}

impl Default for Jagged {
    fn default() -> Self {
        Jagged {
            mode: JaggedMode::In,
            side: JaggedSide::Both,
            amount: 1.0,
            seed: 1,
        }
    }
}

/// Raised satin (#531): running-stitch lanes along the column under the satin, about one
/// per `lane_spacing` of width (an even number, at least two, so they end where the satin
/// starts), `inset` in from the edges.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Raised {
    /// mm (default 1.5).
    pub lane_spacing: f64,
    /// mm (default 0.5).
    pub inset: f64,
    /// mm (default 2.5).
    pub stitch_length: f64,
}

impl Default for Raised {
    fn default() -> Self {
        Raised {
            lane_spacing: 1.5,
            inset: 0.5,
            stitch_length: 2.5,
        }
    }
}

/// Narrowest column (mm) raised satin is meant for: a narrower one gets a hint.
pub const RAISED_MIN_WIDTH: f64 = 3.0;

/// The padding lanes of raised satin (#531) on a column of `rungs`, starting at its start end
/// and ending there, nearest the first rung's left end where the satin begins.
pub fn padding(rungs: &[[Vec2; 2]], raised: &Raised) -> Run {
    if rungs.len() < 2 {
        return Vec::new();
    }
    let inner = inset_rungs(rungs, raised.inset.max(0.0) * 10.0);
    let width = mean_width(&inner);
    let mut lanes = ((width / raised.lane_spacing.max(0.1)).floor() as usize + 1).max(2);
    lanes += lanes % 2;
    let pitch = (raised.stitch_length * 10.0).max(1.0);
    let mut run: Run = Vec::new();
    // From the right lane forwards to the left lane backwards: lanes alternate direction and,
    // being even in number, the last ends at the start end's left.
    for j in 0..lanes {
        let f = 1.0 - j as f64 / (lanes - 1) as f64;
        let mut lane: Vec<Vec2> = inner.iter().map(|r| r[0].lerp(r[1], f)).collect();
        if j % 2 == 1 {
            lane.reverse();
        }
        run.extend(subdivide(&lane, pitch));
    }
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
    run
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// The jagged edge on a satin run whose needle points alternate edges, left first (#528):
/// `n` counts the needle points before this run, so each point keeps its own move.
fn jag(run: &mut Run, j: &Jagged, n: usize) {
    let amount = j.amount.max(0.0) * 10.0;
    if amount <= 0.0 || run.len() < 3 {
        return;
    }
    let orig = run.clone();
    for i in 0..orig.len() {
        let side = i % 2;
        match (j.side, side) {
            (JaggedSide::Left, 1) | (JaggedSide::Right, 0) => continue,
            _ => {}
        }
        let across = match (i.checked_sub(1), orig.get(i + 1)) {
            (Some(a), Some(&b)) => orig[a].lerp(b, 0.5),
            (Some(a), None) => orig[a],
            (None, Some(&b)) => b,
            (None, None) => continue,
        };
        let Some(out) = unit(orig[i] - across) else {
            continue;
        };
        let r = (splitmix(j.seed ^ splitmix((n + i) as u64)) >> 11) as f64 / (1u64 << 53) as f64;
        let d = match j.mode {
            JaggedMode::In => -r,
            JaggedMode::Out => r,
            JaggedMode::Both => 2.0 * r - 1.0,
        };
        // Never across the column: an inward move stops short of the other edge.
        let room = orig[i].distance(across) * 0.8;
        run[i] = orig[i] + out * (d * amount).max(-room);
    }
}

/// S-stitch (#528): every stitch of `run` sewn as three short stitches bent into an S.
fn s_stitch(run: &Run) -> Run {
    let mut out: Run = Vec::with_capacity(run.len() * 3);
    for (i, w) in run.windows(2).enumerate() {
        let (p, q) = (w[0], w[1]);
        if i == 0 {
            out.push(p);
        }
        let len = p.distance(q);
        if let Some(d) = unit(q - p).filter(|_| len > 6.0) {
            let n = v2(-d.y, d.x) * (len / 6.0).min(3.0);
            out.push(p.lerp(q, 1.0 / 3.0) + n);
            out.push(p.lerp(q, 2.0 / 3.0) - n);
        }
        out.push(q);
    }
    out
}

/// Double satin's second pass (#528) over a run whose needle points alternate edges: back
/// from its end, each needle point midway between two of the first pass's on its edge.
fn second_pass(run: &Run) -> Run {
    let mut back: Run = run.windows(3).map(|w| w[0].lerp(w[2], 0.5)).collect();
    back.reverse();
    back
}

/// Automatic under sewing (#523, `UnderlayStyle::Auto`): chosen from the column's average
/// width each time it sews: a centre walk below `centre_below`, the inset edge run below
/// `edge_below`, the edge run and the zigzag above. Stretchy and piled fabrics lower both.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoUnderlay {
    /// mm (default 2).
    pub centre_below: f64,
    /// mm (default 4).
    pub edge_below: f64,
}

impl Default for AutoUnderlay {
    fn default() -> Self {
        AutoUnderlay {
            centre_below: 2.0,
            edge_below: 4.0,
        }
    }
}

impl AutoUnderlay {
    /// The style for a satin `width_mm` wide.
    pub fn style(&self, width_mm: f64) -> UnderlayStyle {
        if width_mm < self.centre_below {
            UnderlayStyle::CentreWalk
        } else if width_mm < self.edge_below {
            UnderlayStyle::Edge
        } else {
            UnderlayStyle::EdgeZigzag
        }
    }

    /// The column components for a column `width_mm` wide.
    pub fn parts(&self, width_mm: f64) -> UnderlayParts {
        let style = self.style(width_mm);
        UnderlayParts {
            edge: style != UnderlayStyle::CentreWalk,
            zigzag: style == UnderlayStyle::EdgeZigzag,
            lattice: false,
            center: style == UnderlayStyle::CentreWalk,
        }
    }
}

/// A second under sewing layer of a column (#523), sewn after the first with its own style,
/// inset, spacing and stitch length.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SecondUnderlay {
    /// Its components as on a column (`edge`, `zigzag`, `edgeZigzag`, `centreWalk`, `auto`).
    pub style: UnderlayStyle,
    /// Distance inside the outline, mm (default 0.5).
    pub inset: f64,
    /// Distance between the zigzag's needle points along the column, mm (default 2).
    pub spacing: f64,
    /// Longest stitch, mm (default 2.5).
    pub stitch_length: f64,
    /// On a fill or satin region sewn by rows (#677): the direction of its rows, degrees from
    /// the fill (or satin) angle. The default 0 is square to PE-DESIGN's first fill under
    /// sewing layer, whose rows run at 90° to the fill rows (crossed tatami). Columns ignore
    /// it. Saved only when not 0.
    #[serde(skip_serializing_if = "is_zero")]
    pub angle: f64,
}

fn is_zero(x: &f64) -> bool {
    *x == 0.0
}

impl Default for SecondUnderlay {
    fn default() -> Self {
        SecondUnderlay {
            style: UnderlayStyle::Zigzag,
            inset: 0.5,
            spacing: 2.0,
            stitch_length: 2.5,
            angle: 0.0,
        }
    }
}

/// The average width of a column, mm, weighted by the length of its sections.
pub fn mean_width(rungs: &[[Vec2; 2]]) -> f64 {
    let (mut sum, mut len) = (0.0, 0.0);
    for w in rungs.windows(2) {
        let l = w[0][0]
            .lerp(w[0][1], 0.5)
            .distance(w[1][0].lerp(w[1][1], 0.5));
        sum += l * (w[0][0].distance(w[0][1]) + w[1][0].distance(w[1][1])) / 2.0;
        len += l;
    }
    match rungs {
        [] => 0.0,
        _ if len <= 1e-9 => rungs[0][0].distance(rungs[0][1]) / 10.0,
        _ => sum / len / 10.0,
    }
}

/// The layers of a Manual Punch column `lines` (rungs, Curved Block already applied) sewn
/// with `r`: the under sewing (PE-DESIGN's, [`underlay`]; with `UnderlayStyle::Auto` the parts
/// [`AutoUnderlay`] picks; then the second layer when set), then the satin or fill.
pub fn layers(lines: &[[Vec2; 2]], r: &RegionSewing) -> Vec<Vec<Run>> {
    let spacing = 10.0 / r.density.max(0.1);
    let pull = r.pull_compensation * 10.0;
    let mut out = Vec::new();
    if r.underlay && lines.len() >= 2 {
        let start = r.entry.unwrap_or(lines[0][0]);
        let width = mean_width(lines);
        let style = match r.underlay_style {
            UnderlayStyle::Auto => r.underlay_auto.style(width),
            s => s,
        };
        let run = match (r.underlay_parts, style, r.underlay_style) {
            (None, UnderlayStyle::CentreWalk, _) => {
                centre_walk(lines, start, UNDERLAY_INSET, UNDERLAY_PITCH)
            }
            (None, _, UnderlayStyle::Auto) => underlay(
                lines,
                r.underlay_auto.parts(width),
                r.underlay_spacing_mm() * 10.0,
                r.stitch_length * 10.0,
                start,
            ),
            _ => underlay(
                lines,
                r.underlay_parts(),
                r.underlay_spacing_mm() * 10.0,
                r.stitch_length * 10.0,
                start,
            ),
        };
        if run.len() > 1 {
            out.push(vec![run]);
        }
        if let Some(u) = r.underlay2 {
            let style = match u.style {
                UnderlayStyle::Auto => r.underlay_auto.style(width),
                s => s,
            };
            let pitch = (u.stitch_length * 10.0).max(1.0);
            let run = match style {
                UnderlayStyle::CentreWalk => centre_walk(lines, start, u.inset * 10.0, pitch),
                s => underlay_at(
                    lines,
                    s.column_parts(),
                    u.spacing * 10.0,
                    pitch,
                    start,
                    u.inset * 10.0,
                    pitch,
                ),
            };
            if run.len() > 1 {
                out.push(vec![run]);
            }
        }
    }
    if r.kind == RegionType::Satin
        && let Some(raised) = &r.satin_shaping.raised
    {
        let pad = padding(lines, raised);
        if pad.len() > 1 {
            out.push(vec![pad]);
        }
    }
    out.push(vec![match r.kind {
        RegionType::Satin => satin_with(lines, spacing, pull, &r.satin_shaping),
        _ => fill(lines, spacing, r.stitch_length * 10.0, pull),
    }]);
    out
}

/// A centre walk under a column (#523): from `start` to the middle of its start end, along the
/// middle (the rungs `inset` (0.1 mm) in from both ends) in stitches up to `pitch`, back the
/// same way with the needle points shifted half a stitch, and back to `start`.
pub fn centre_walk(rungs: &[[Vec2; 2]], start: Vec2, inset: f64, pitch: f64) -> Run {
    if rungs.len() < 2 {
        return Vec::new();
    }
    let inner = inset_rungs(rungs, inset.max(0.0));
    let n = inner.len();
    let mut mids: Vec<Vec2> = inner.iter().map(|&r| mid(r)).collect();
    let near_first = start.distance(mids[0]) <= start.distance(mids[n - 1]);
    if !near_first {
        mids.reverse();
    }
    let pitch = pitch.max(1.0);
    let out = subdivide(&mids, pitch);
    let mut run = vec![start];
    run.extend(out.iter().copied());
    // Back between the outward needle points, so the two passes do not share holes.
    let mut back: Vec<Vec2> = out.windows(2).rev().map(|w| w[0].lerp(w[1], 0.5)).collect();
    back.push(out[0]);
    run.extend(back);
    run.push(start);
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
    run
}

/// [`satin`] with the shaping options: corner constructions, compensation per side and
/// per cent of width, short stitches on the inside of curves.
pub fn satin_with(rungs: &[[Vec2; 2]], spacing: f64, pull: f64, shaping: &SatinShaping) -> Run {
    if shaping.is_plain() || rungs.len() < 2 {
        return satin(rungs, spacing, pull);
    }
    let at = shaping.spacing_at.unwrap_or(1.0);
    stitch_pieces(
        pieces_at(
            rungs,
            shaping.pitch(2.0 * spacing),
            true,
            &shaping.corners,
            at,
        ),
        pull,
        shaping,
        sawtooth,
    )
}

/// Zigzag across rungs with the shaping options (zigzag lines, see
/// [`crate::lines::zigzag_shaped`]): rungs `spacing` apart, one needle point each.
pub fn zigzag_with(rungs: &[[Vec2; 2]], spacing: f64, shaping: &SatinShaping) -> Run {
    zigzag_shaped(rungs, spacing, 0.0, shaping)
}

/// [`zigzag_with`] with each crossing lengthened by `pull` (0.1 mm) at both ends, or by the
/// shaping's compensation per side and per cent of width: skeleton satin and Auto satin with
/// satin shaping (#675). `rungs` are the column's edges without compensation.
pub fn zigzag_shaped(rungs: &[[Vec2; 2]], spacing: f64, pull: f64, shaping: &SatinShaping) -> Run {
    // `spacingAt` below 1 (#685): the spacing measured that far across the column, from the
    // shorter side, as on a Manual Punch column; `rungs` must then be finer than the spacing.
    let pitch = shaping.pitch(spacing);
    let pieces = match shaping.spacing_at.filter(|&a| a < 1.0) {
        Some(at) if !has_corners(rungs, pitch, &shaping.corners) => {
            vec![Piece::Rungs(rungs_even_at(rungs, pitch, at))]
        }
        Some(at) => pieces_at(rungs, pitch, true, &shaping.corners, at),
        None => pieces(rungs, pitch, false, &shaping.corners),
    };
    stitch_pieces(pieces, pull, shaping, zigzag)
}

/// Below this satin width (mm) the width-following under sewing sews one row (#523, #675).
pub const ONE_ROW_BELOW: f64 = 3.0;

/// The number of width-following rows under a satin `width` wide (mm), rows `spacing` (mm)
/// apart: one below [`ONE_ROW_BELOW`], else one per spacing, rounded down to an odd number so
/// the rows end at the far end of their stretch.
fn rows_for(width: f64, spacing: f64) -> usize {
    if width < ONE_ROW_BELOW {
        return 1;
    }
    let n = ((width / spacing.max(0.1)).round() as usize).clamp(1, 99);
    if n.is_multiple_of(2) { n - 1 } else { n }
}

/// Width-following under sewing of a satin column (#523, #675, `underlay: auto` on a skeleton
/// satin or an Auto satin region): running rows along the column, `inset` (0.1 mm) inside its
/// edges, as many across as [`rows_for`] gives at each place (one where it is narrower than
/// 3 mm, about one per `spacing` (0.1 mm) where it is wider), so a tapering column gets more
/// rows where it is wide and never a row past its edge. Each stretch of one row count is sewn
/// back and forth across the column; then a centre run back to `start`, its needle points
/// between the rows'. Stitches up to `pitch` (0.1 mm).
pub fn width_rows(rungs: &[[Vec2; 2]], start: Vec2, inset: f64, spacing: f64, pitch: f64) -> Run {
    if rungs.len() < 2 {
        return Vec::new();
    }
    let n = rungs.len();
    let near_first = start.distance(mid(rungs[0])) <= start.distance(mid(rungs[n - 1]));
    let ordered: Vec<[Vec2; 2]> = if near_first {
        rungs.to_vec()
    } else {
        rungs.iter().rev().copied().collect()
    };
    let pitch = pitch.max(1.0);
    let stations = rungs_at(&ordered, pitch);
    let inset = inset.max(0.0);
    let inner: Vec<[Vec2; 2]> = stations
        .iter()
        .map(|&[l, r]| {
            let len = l.distance(r);
            if len <= 2.0 * inset + 1.0 {
                let m = l.lerp(r, 0.5);
                return [m, m];
            }
            let d = (r - l) * (inset / len);
            [l + d, r - d]
        })
        .collect();
    let mut counts: Vec<usize> = stations
        .iter()
        .map(|&[l, r]| rows_for(l.distance(r) / 10.0, spacing / 10.0))
        .collect();
    // Stretches shorter than two spacings join the stretch before them, so the count does
    // not flicker where the width hovers at a bound.
    let min_len = 2.0 * spacing;
    let mut i = 0;
    while i < counts.len() {
        let mut j = i;
        while j < counts.len() && counts[j] == counts[i] {
            j += 1;
        }
        let len = mid(stations[i]).distance(mid(stations[j - 1]));
        if i > 0 && len < min_len {
            let c = counts[i - 1];
            counts[i..j].iter_mut().for_each(|k| *k = c);
        }
        i = j;
    }
    let mut run = vec![start];
    // The side of the column the last row ended on (0 left, 1 right): the next stretch's rows
    // start there.
    let mut from_right = false;
    let mut i = 0;
    while i < counts.len() {
        let c = counts[i];
        let mut j = i;
        while j < counts.len() && counts[j] == c {
            j += 1;
        }
        // The stretch shares its first station with the one before it.
        let lo = i.saturating_sub(1);
        for row in 0..c {
            let lane = if from_right { c - 1 - row } else { row };
            let t = (lane as f64 + 0.5) / c as f64;
            let points = inner[lo..j].iter().map(|&[l, r]| l.lerp(r, t));
            if row % 2 == 0 {
                run.extend(points);
            } else {
                run.extend(points.rev());
            }
        }
        if c > 1 {
            from_right = !from_right;
        }
        i = j;
    }
    // Back along the middle, between the outward needle points.
    let mids: Vec<Vec2> = inner.iter().map(|&r| mid(r)).collect();
    let back: Vec<Vec2> = mids
        .windows(2)
        .rev()
        .map(|w| w[0].lerp(w[1], 0.5))
        .collect();
    run.extend(back);
    run.push(mids[0]);
    run.push(start);
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
    run
}

/// OpenStitch's under sewing on a satin column given by `rungs` from its start end, for a
/// skeleton satin or an Auto satin region (#675), where `r` asks for it: the first layer for
/// `underlayStyle` `auto` ([`width_rows`]) and `centreWalk`, and the second layer
/// (`underlay2`). `None` where PE-DESIGN's region under sewing stays (the first layer of the
/// other styles, no second layer). Both start and end at `start`.
pub fn rung_underlay(rungs: &[[Vec2; 2]], r: &RegionSewing, start: Vec2) -> [Option<Run>; 2] {
    if !r.underlay || rungs.len() < 2 {
        return [None, None];
    }
    let keep = |run: Run| (run.len() > 1).then_some(run);
    let first = match r.underlay_style {
        UnderlayStyle::Auto => keep(width_rows(
            rungs,
            start,
            UNDERLAY_INSET,
            r.underlay_spacing_mm() * 10.0,
            UNDERLAY_PITCH,
        )),
        UnderlayStyle::CentreWalk => {
            keep(centre_walk(rungs, start, UNDERLAY_INSET, UNDERLAY_PITCH))
        }
        _ => None,
    };
    let second = r.underlay2.and_then(|u| {
        let pitch = (u.stitch_length * 10.0).max(1.0);
        keep(match u.style {
            UnderlayStyle::Auto => {
                width_rows(rungs, start, u.inset * 10.0, u.spacing * 10.0, pitch)
            }
            UnderlayStyle::CentreWalk => centre_walk(rungs, start, u.inset * 10.0, pitch),
            s => underlay_at(
                rungs,
                s.column_parts(),
                u.spacing * 10.0,
                pitch,
                start,
                u.inset * 10.0,
                pitch,
            ),
        })
    });
    [first, second]
}

/// A part of a shaped column in sewing order.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    /// Rungs [left, right] to sew across.
    Rungs(Vec<[Vec2; 2]>),
    /// A running stitch hidden under the piece that follows it (the way into a cap).
    Run(Vec<Vec2>),
}

fn stitch_pieces(
    pieces: Vec<Piece>,
    pull: f64,
    shaping: &SatinShaping,
    stitch: impl Fn(Vec<[Vec2; 2]>) -> Run,
) -> Run {
    let mut passes: Vec<(Run, bool)> = Vec::new();
    for p in pieces {
        match p {
            Piece::Rungs(r) => {
                let r: Vec<[Vec2; 2]> = r
                    .into_iter()
                    .map(|[l, rt]| {
                        let len = l.distance(rt);
                        if len < 1e-9 {
                            return [l, rt];
                        }
                        let d = (rt - l) * (1.0 / len);
                        let [el, er] = shaping.compensation.sides(pull, len);
                        [l - d * el, rt + d * er]
                    })
                    .collect();
                passes.push((stitch(r), true));
            }
            Piece::Run(r) => passes.push((r, false)),
        }
    }
    finish(passes, shaping)
}

/// The satin runs of a shaped column in sewing order (`true`: needle points alternating
/// edges, left first; `false`: a hidden run) with the run-based options applied: the jagged
/// edge, short stitches, double satin's second pass and S-stitch; then joined.
pub(crate) fn finish(mut passes: Vec<(Run, bool)>, shaping: &SatinShaping) -> Run {
    let mut points = 0;
    for (s, _) in passes.iter_mut().filter(|p| p.1) {
        if let Some(j) = &shaping.jagged {
            jag(s, j, points);
        }
        points += s.len();
        short_stitches(
            s,
            shaping.short_stitches.distance * 10.0,
            shaping.short_stitches.inset / 100.0,
        );
    }
    if shaping.style == SatinStyle::Double {
        let back: Vec<(Run, bool)> = passes
            .iter()
            .rev()
            .filter(|p| p.1)
            .map(|p| (second_pass(&p.0), true))
            .collect();
        passes.extend(back);
    }
    let mut run: Run = Vec::new();
    for (s, satin) in passes {
        if satin && shaping.style == SatinStyle::SStitch {
            run.extend(s_stitch(&s));
        } else {
            run.extend(s);
        }
    }
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
    run
}

/// Short stitches (#521) on a satin run whose needle points alternate between the two edges
/// (a [`sawtooth`] or [`zigzag`]): a point closer than `distance` (0.1 mm) to the last
/// full-length point on its edge is pulled towards the other edge by `inset` (a fraction of
/// the stitch's width), so on the inside of a curve every other point stops short. The first
/// and last points stay. Nothing changes where `distance` is 0.
pub fn short_stitches(run: &mut Run, distance: f64, inset: f64) {
    if distance <= 0.0 || inset <= 0.0 || run.len() < 4 {
        return;
    }
    let orig = run.clone();
    let mut last_full: [Option<Vec2>; 2] = [None, None];
    for i in 0..orig.len() {
        let side = i % 2;
        let p = orig[i];
        if let Some(prev) = last_full[side]
            && i + 1 < orig.len()
            && prev.distance(p) < distance
        {
            // The other edge across from here: between this point's two neighbours.
            let across = orig[i - 1].lerp(orig[i + 1], 0.5);
            run[i] = p.lerp(across, inset.min(1.0));
            continue;
        }
        last_full[side] = Some(p);
    }
}

fn dot(a: Vec2, b: Vec2) -> f64 {
    a.x * b.x + a.y * b.y
}

fn cross2(a: Vec2, b: Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}

fn unit(v: Vec2) -> Option<Vec2> {
    let l = v.length();
    (l > 1e-9).then(|| v * (1.0 / l))
}

/// The part of polygon `poly` where `dot(p - o, n) >= 0` (Sutherland-Hodgman).
fn clip(poly: &[Vec2], o: Vec2, n: Vec2) -> Vec<Vec2> {
    let f = |p: Vec2| dot(p - o, n);
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (fa, fb) = (f(a), f(b));
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            out.push(a.lerp(b, fa / (fa - fb)));
        }
    }
    out
}

/// Where the line `p + d·t` crosses polygon `poly`: the smallest and largest `t`.
fn span(poly: &[Vec2], p: Vec2, d: Vec2) -> Option<(f64, f64)> {
    let mut lo: Option<(f64, f64)> = None;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let e = b - a;
        let den = cross2(d, e);
        if den.abs() < 1e-12 {
            continue;
        }
        let s = cross2(a - p, d) / den;
        if !(-1e-9..=1.0 + 1e-9).contains(&s) {
            continue;
        }
        let t = cross2(a - p, e) / den;
        lo = Some(lo.map_or((t, t), |(x, y)| (x.min(t), y.max(t))));
    }
    lo
}

/// Stitch lines square to `u` across `polys`, `pitch` apart along `u` from `origin`, each
/// [left, right] with `left` the end towards `left_dir`. Lines shorter than 0.05 mm are
/// left out.
fn sweep(polys: &[&[Vec2]], origin: Vec2, u: Vec2, pitch: f64, left_dir: Vec2) -> Vec<[Vec2; 2]> {
    let all = polys.iter().flat_map(|p| p.iter());
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &v in all {
        let s = dot(v - origin, u);
        lo = lo.min(s);
        hi = hi.max(s);
    }
    if hi - lo < 1e-6 {
        return Vec::new();
    }
    let n = ((hi - lo) / pitch.max(0.1)).ceil().clamp(1.0, 100_000.0) as usize;
    let d = v2(-u.y, u.x);
    let mut out = Vec::new();
    for i in 0..=n {
        let s = (lo + (hi - lo) * i as f64 / n as f64).clamp(lo + 1e-6, hi - 1e-6);
        let p = origin + u * s;
        let Some((t0, t1)) = polys
            .iter()
            .filter_map(|poly| span(poly, p, d))
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
        else {
            continue;
        };
        if t1 - t0 < 0.5 {
            continue;
        }
        let (a, b) = (p + d * t0, p + d * t1);
        out.push(if dot(a - b, left_dir) >= 0.0 {
            [a, b]
        } else {
            [b, a]
        });
    }
    out
}

/// A constructed corner at a rung.
#[derive(Clone, Debug, PartialEq)]
enum Join {
    Mitre,
    /// The inner corner and the cap's rungs from its base to the tip.
    Cap(Vec2, Vec<[Vec2; 2]>),
}

/// The section between rungs `j` and `j + 1` as a polygon.
fn quad(rungs: &[[Vec2; 2]], j: usize) -> Vec<Vec2> {
    vec![rungs[j][0], rungs[j + 1][0], rungs[j + 1][1], rungs[j][1]]
}

fn mid(r: [Vec2; 2]) -> Vec2 {
    r[0].lerp(r[1], 0.5)
}

/// The corner constructions of `rungs` under `c`, one per rung (none at the ends).
fn joins(rungs: &[[Vec2; 2]], pitch: f64, c: &Corners) -> Vec<Option<Join>> {
    let n = rungs.len();
    let mut out = vec![None; n];
    if c.style == CornerStyle::Turn {
        return out;
    }
    let ov = c.overlap.max(0.0) * 10.0;
    for k in 1..n.saturating_sub(1) {
        let (ma, mk, mb) = (mid(rungs[k - 1]), mid(rungs[k]), mid(rungs[k + 1]));
        let (Some(ua), Some(ub)) = (unit(mk - ma), unit(mb - mk)) else {
            continue;
        };
        let interior = dot(ua * -1.0, ub).clamp(-1.0, 1.0).dacos().to_degrees();
        let want_cap = match c.style {
            CornerStyle::Turn => continue,
            _ if interior >= c.mitre_below => continue,
            CornerStyle::Auto => interior < c.cap_below,
            CornerStyle::Mitre => false,
            CornerStyle::Cap => true,
        };
        // A leg too short to hold the construction leaves the corner turned.
        let room = ov.max(pitch);
        if ma.distance(mk) < room || mk.distance(mb) < room {
            continue;
        }
        if want_cap && let Some(cap) = cap(rungs, k, ua, ub, ov, pitch, c.cap_max_length * 10.0) {
            out[k] = Some(cap);
            continue;
        }
        out[k] = Some(Join::Mitre);
    }
    // Two constructions on one short section would cut it twice: the later corner turns.
    for k in 2..n.saturating_sub(1) {
        if out[k - 1].is_some() && out[k].is_some() {
            let len = mid(rungs[k - 1]).distance(mid(rungs[k]));
            if len < 2.0 * ov.max(pitch) {
                out[k] = None;
            }
        }
    }
    out
}

/// The cap of the corner at rung `k`, if both legs reach past its square ends and its longest
/// stitch is at most `max_len`.
fn cap(
    rungs: &[[Vec2; 2]],
    k: usize,
    ua: Vec2,
    ub: Vec2,
    ov: f64,
    pitch: f64,
    max_len: f64,
) -> Option<Join> {
    let inward = ub - ua;
    let [e0, e1] = rungs[k];
    let inner = if dot(e0, inward) >= dot(e1, inward) {
        e0
    } else {
        e1
    };
    // Each leg's neighbouring rung must lie behind the cut.
    let behind_a = rungs[k - 1]
        .iter()
        .all(|&e| dot(e - inner, ua) < -ov - 1e-6);
    let ahead_b = rungs[k + 1].iter().all(|&e| dot(e - inner, ub) > 1e-6);
    if !behind_a || !ahead_b {
        return None;
    }
    let a = clip(&quad(rungs, k - 1), inner - ua * ov, ua);
    let b = clip(&quad(rungs, k), inner, ub * -1.0);
    let left = rungs[k - 1][0] - rungs[k - 1][1];
    let cap_rungs = sweep(&[&a, &b], inner, ua, pitch, left);
    let longest = cap_rungs
        .iter()
        .map(|r| r[0].distance(r[1]))
        .fold(0.0, f64::max);
    (!cap_rungs.is_empty() && longest <= max_len).then_some(Join::Cap(inner, cap_rungs))
}

/// Whether `c` builds a mitre or cap at any corner of `rungs` (rungs `pitch` apart).
pub fn has_corners(rungs: &[[Vec2; 2]], pitch: f64, c: &Corners) -> bool {
    joins(rungs, pitch, c).iter().any(Option::is_some)
}

/// The rungs of a column `pitch` apart in sewing order with the corner constructions of `c`
/// (see [`Corners`]); `floor` places the plain sections as [`satin`] does (a whole number of
/// steps at least `pitch` apart), otherwise at most `pitch` apart.
pub fn pieces(rungs: &[[Vec2; 2]], pitch: f64, floor: bool, c: &Corners) -> Vec<Piece> {
    pieces_at(rungs, pitch, floor, c, 1.0)
}

/// [`pieces`] with a `floor` column's spacing measured `at` across it (see
/// [`SatinShaping::spacing_at`]).
fn pieces_at(rungs: &[[Vec2; 2]], pitch: f64, floor: bool, c: &Corners, at: f64) -> Vec<Piece> {
    let plain = |r: &[[Vec2; 2]]| {
        if floor {
            rungs_within_pitch_at(r, pitch, at)
        } else {
            rungs_leaning(r, pitch)
        }
    };
    if rungs.len() < 2 {
        return vec![Piece::Rungs(rungs.to_vec())];
    }
    let joins = joins(rungs, pitch, c);
    if joins.iter().all(Option::is_none) {
        return vec![Piece::Rungs(plain(rungs))];
    }
    let ov = c.overlap.max(0.0) * 10.0;
    let mut out = Vec::new();
    let mut cur: Vec<[Vec2; 2]> = vec![rungs[0]];
    for j in 0..rungs.len() - 1 {
        let (start, end) = (&joins[j], &joins[j + 1]);
        if start.is_none() && end.is_none() {
            cur.extend(plain(&rungs[j..j + 2]).into_iter().skip(1));
        } else if let Some(u) = unit(mid(rungs[j + 1]) - mid(rungs[j])) {
            let mut r = [rungs[j], rungs[j + 1]];
            if *start == Some(Join::Mitre) {
                r[0] = [r[0][0] - u * ov, r[0][1] - u * ov];
            }
            if *end == Some(Join::Mitre) {
                r[1] = [r[1][0] + u * ov, r[1][1] + u * ov];
            }
            let mut poly = quad(&r, 0);
            if let Some(Join::Cap(inner, _)) = start {
                poly = clip(&poly, *inner, u);
            }
            if let Some(Join::Cap(inner, _)) = end {
                poly = clip(&poly, *inner, u * -1.0);
            }
            let left = rungs[j][0] - rungs[j][1];
            cur.extend(sweep(&[&poly], mid(rungs[j]), u, pitch, left));
            if end.is_none() {
                cur.push(rungs[j + 1]);
            }
        }
        match end {
            Some(Join::Mitre) => out.push(Piece::Rungs(std::mem::take(&mut cur))),
            Some(Join::Cap(_, cap)) => {
                out.push(Piece::Rungs(std::mem::take(&mut cur)));
                // Into the cap under its stitches: along its middle to the tip, then the cap
                // from the tip back to its base.
                if let (Some(&base), Some(&tip)) = (cap.first(), cap.last()) {
                    out.push(Piece::Run(subdivide(
                        &[mid(base), mid(tip)],
                        UNDERLAY_PITCH,
                    )));
                }
                out.push(Piece::Rungs(cap.iter().rev().copied().collect()));
            }
            None => {}
        }
    }
    out.push(Piece::Rungs(cur));
    out.retain(|p| !matches!(p, Piece::Rungs(r) if r.is_empty()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{contains, v2};

    fn bend() -> Vec<[Vec2; 2]> {
        vec![
            [v2(0.0, 0.0), v2(0.0, 40.0)],
            [v2(100.0, 0.0), v2(80.0, 40.0)],
            [v2(140.0, 80.0), v2(100.0, 80.0)],
        ]
    }

    #[test]
    fn curved_block_rails_pass_through_the_line_ends() {
        let o = curved_outline(&bend());
        assert_eq!(o.points.len(), 6);
        assert_eq!(o.curves.len(), 6);
        // Rails curved; the first and last lines straight.
        assert!(o.curve(0).is_some() && o.curve(1).is_some() && o.curve(2).is_none());
        assert!(o.curve(3).is_some() && o.curve(4).is_some() && o.curve(5).is_none());
        let dense = curved_rungs(&bend());
        assert!(dense.len() > 6);
        assert_eq!(dense[0], bend()[0]);
        assert_eq!(*dense.last().expect("last"), bend()[2]);
        assert!(dense.contains(&bend()[1]), "clicked lines are kept");
        assert_eq!(validate(&dense), Ok(()));
        // Two lines: nothing to curve.
        assert_eq!(curved_rungs(&bend()[..2]), bend()[..2].to_vec());
        assert!(!curved_outline(&bend()[..2]).has_curves());
    }

    #[test]
    fn outline_and_validation() {
        let o = outline(&bend());
        assert_eq!(o.points.len(), 6);
        assert!(contains(v2(50.0, 20.0), std::slice::from_ref(&o)));
        assert_eq!(validate(&bend()), Ok(()));
        assert_eq!(validate(&bend()[..1]), Err(RungError::TooFew));
        let mut flat = bend();
        flat[1] = [v2(50.0, 5.0), v2(50.0, 5.05)];
        assert_eq!(validate(&flat), Err(RungError::Degenerate(1)));
        // Second rung crossing the first.
        let crossing = vec![
            [v2(0.0, 0.0), v2(0.0, 40.0)],
            [v2(-20.0, 20.0), v2(20.0, 20.0)],
        ];
        assert_eq!(validate(&crossing), Err(RungError::Crossing(0)));
        // Twisted: left and right swap sides.
        let twisted = vec![
            [v2(0.0, 0.0), v2(0.0, 40.0)],
            [v2(50.0, 40.0), v2(50.0, 0.0)],
        ];
        assert_eq!(validate(&twisted), Err(RungError::Crossing(0)));
    }

    #[test]
    fn satin_alternates_rails_and_follows_the_turn() {
        let run = satin(&bend(), 4.0, 0.0);
        assert!(run.len() > 20);
        // The first rung is sewn across, then one needle point per rung on alternate rails:
        // every stitch crosses the column, none runs along a rail.
        let on = |side: usize, p: Vec2| {
            let rail = Contour::new(bend().iter().map(|r| r[side]).collect(), false);
            rail.nearest(p).expect("n").point.distance(p) < 1e-6
        };
        let rail = |p: Vec2| {
            assert!(on(0, p) != on(1, p), "{p:?} is on exactly one rail");
            usize::from(on(1, p))
        };
        assert_eq!(run[0], bend()[0][0]);
        assert_eq!(run[1], bend()[0][1]);
        for w in run.windows(2).skip(1) {
            assert_ne!(rail(w[0]), rail(w[1]), "{:?} runs along a rail", w);
        }
        // The last needle point is on the last rung.
        let last = run[run.len() - 1];
        assert!(last == v2(140.0, 80.0) || last == v2(100.0, 80.0));
        // Pull compensation lengthens each rung at both ends.
        let pulled = satin(&bend(), 4.0, 3.0);
        assert_eq!(pulled.len(), run.len());
        assert!(pulled[0].distance(pulled[1]) > run[0].distance(run[1]) + 5.0);
    }

    #[test]
    fn fill_rows_stay_inside_and_are_short() {
        let run = fill(&bend(), 4.0, 30.0, 0.0);
        let o = [outline(&bend())];
        for w in run.windows(2) {
            assert!(w[0].distance(w[1]) <= 30.0 + 1e-6);
            let mid = w[0].lerp(w[1], 0.5);
            assert!(contains(mid, &o) || o[0].nearest(mid).expect("n").point.distance(mid) < 1.0);
        }
    }

    /// A column `w` (0.1 mm) wide along two legs `leg` long meeting at an interior angle of
    /// `deg` degrees, with a mitred rung at the corner: [start, corner, end].
    fn corner(deg: f64, w: f64, leg: f64) -> Vec<[Vec2; 2]> {
        let half = (180.0 - deg).to_radians() / 2.0;
        // Leg A along +x into the origin, leg B turning left by 180 - deg.
        let ua = v2(1.0, 0.0);
        let ub = v2((2.0 * half).dcos(), (2.0 * half).dsin());
        let n = |u: Vec2| v2(-u.y, u.x);
        let h = w / 2.0;
        let bis = (n(ua) + n(ub)) * (1.0 / (1.0 + dot(n(ua), n(ub))));
        let a0 = ua * -leg;
        let b1 = ub * leg;
        vec![
            [a0 + n(ua) * h, a0 - n(ua) * h],
            [bis * h, bis * -h],
            [b1 + n(ub) * h, b1 - n(ub) * h],
        ]
    }

    fn with(style: CornerStyle) -> Corners {
        Corners {
            style,
            ..Corners::default()
        }
    }

    #[test]
    fn corners_are_classified_by_their_interior_angle() {
        let auto = with(CornerStyle::Auto);
        let class = |deg: f64, w: f64, c: &Corners| match &joins(&corner(deg, w, 300.0), 4.0, c)[1]
        {
            None => "turn",
            Some(Join::Mitre) => "mitre",
            Some(Join::Cap(..)) => "cap",
        };
        assert_eq!(class(80.0, 40.0, &auto), "turn");
        assert_eq!(class(60.0, 40.0, &auto), "mitre");
        assert_eq!(class(30.0, 30.0, &auto), "cap");
        // Off by default: every corner turns.
        assert_eq!(class(30.0, 30.0, &Corners::default()), "turn");
        assert_eq!(class(60.0, 40.0, &with(CornerStyle::Cap)), "cap");
        assert_eq!(class(30.0, 30.0, &with(CornerStyle::Mitre)), "mitre");
        // A cap longer than its limit falls back to a mitre.
        let short_caps = Corners {
            cap_max_length: 2.0,
            ..auto
        };
        assert_eq!(class(30.0, 30.0, &short_caps), "mitre");
        // A side too short to hold the construction stays turned.
        assert!(joins(&corner(60.0, 40.0, 3.0), 4.0, &auto)[1].is_none());
        // Thresholds are editable.
        let low = Corners {
            mitre_below: 50.0,
            ..auto
        };
        assert_eq!(class(60.0, 40.0, &low), "turn");
    }

    #[test]
    fn a_mitre_keeps_each_legs_stitches_square_and_overlaps_the_seam() {
        let rungs = corner(60.0, 40.0, 300.0);
        let c = with(CornerStyle::Mitre);
        let p = pieces(&rungs, 4.0, true, &c);
        let legs: Vec<&Vec<[Vec2; 2]>> = p
            .iter()
            .filter_map(|x| match x {
                Piece::Rungs(r) => Some(r),
                Piece::Run(_) => None,
            })
            .collect();
        assert_eq!(legs.len(), 2, "{p:?}");
        let ua = v2(1.0, 0.0);
        // Leg A's stitches after its first rung are square to it.
        for r in &legs[0][1..] {
            assert!(dot(r[1] - r[0], ua).abs() < 1e-6, "{r:?}");
        }
        // Leg A runs `overlap` past the seam (rung 1, through the origin along the bisector):
        // its last stitch reaches beyond the seam on the outer side.
        let seam_n = unit(rungs[1][0] - rungs[1][1])
            .map(|d| v2(d.y, -d.x))
            .expect("seam");
        let past = legs[0]
            .iter()
            .flat_map(|r| r.iter())
            .map(|&q| dot(q - rungs[1][1], seam_n).abs().min(f64::INFINITY))
            .fold(0.0, f64::max);
        assert!(past > 0.0);
        let beyond = legs[0].iter().flatten().any(|&q| dot(q, ua) > 0.0);
        assert!(beyond, "leg A crosses the seam by the overlap");
        // Every needle point stays inside the column grown by the overlap.
        let o = outline(&rungs);
        for q in legs.iter().flat_map(|l| l.iter()).flatten() {
            let d = o.nearest(*q).expect("n").point.distance(*q);
            assert!(
                contains(*q, std::slice::from_ref(&o)) || d <= 5.0 + 1e-6,
                "{q:?} {d}"
            );
        }
        // One satin run, no gap between the legs longer than the overlap and a stitch.
        let run = satin_with(
            &rungs,
            2.0,
            0.0,
            &SatinShaping {
                corners: c,
                ..SatinShaping::default()
            },
        );
        for w in run.windows(2) {
            assert!(w[0].distance(w[1]) < 60.0, "{w:?}");
        }
    }

    #[test]
    fn a_cap_is_its_own_piece_reached_by_a_hidden_run() {
        let rungs = corner(30.0, 30.0, 300.0);
        let p = pieces(&rungs, 4.0, true, &with(CornerStyle::Cap));
        let kinds: Vec<&str> = p
            .iter()
            .map(|x| match x {
                Piece::Rungs(_) => "rungs",
                Piece::Run(_) => "run",
            })
            .collect();
        assert_eq!(kinds, ["rungs", "run", "rungs", "rungs"]);
        let (Piece::Run(hidden), Piece::Rungs(cap)) = (&p[1], &p[2]) else {
            unreachable!()
        };
        // The cap's stitches are no longer than its limit and parallel to leg A's end.
        for r in cap {
            assert!(r[0].distance(r[1]) <= 80.0 + 1e-6);
            assert!(r[0].x.abs() > 0.0 || r[1].x.abs() > 0.0);
            assert!((r[1] - r[0]).x.abs() < 1e-6, "square to leg A: {r:?}");
        }
        // The hidden run lies under the cap: every point between the cap's first and last rung.
        let (lo, hi) = (cap.last().expect("base")[0].x, cap[0][0].x);
        for q in hidden {
            assert!(
                q.x >= lo.min(hi) - 1e-6 && q.x <= lo.max(hi) + 1e-6,
                "{q:?}"
            );
        }
        // The legs stop on the square line through the inner corner (x = inner corner's x).
        let Piece::Rungs(a) = &p[0] else {
            unreachable!()
        };
        let inner_x = rungs[1].iter().map(|q| q.x).fold(f64::INFINITY, f64::min);
        for q in a.iter().flatten() {
            assert!(q.x <= inner_x + 1e-6, "{q:?}");
        }
    }

    #[test]
    fn short_stitches_thin_the_inside_of_a_curve() {
        // A column round a tight arc: centre radius 3 mm, 4 mm wide.
        let rungs: Vec<[Vec2; 2]> = (0..=24)
            .map(|i| {
                let a = std::f64::consts::PI * i as f64 / 24.0;
                let d = v2(a.dcos(), a.dsin());
                [d * 10.0, d * 50.0]
            })
            .collect();
        let crowd = |run: &Run| {
            // Most needle points in one 0.5 mm cell within 1 mm of the inner edge.
            let mut cells = std::collections::HashMap::new();
            for q in run.iter().filter(|q| q.length() < 20.0) {
                *cells
                    .entry(((q.x / 5.0).floor() as i64, (q.y / 5.0).floor() as i64))
                    .or_insert(0) += 1;
            }
            cells.values().copied().max().unwrap_or(0)
        };
        let plain = satin(&rungs, 2.0, 0.0);
        let shaping = SatinShaping {
            short_stitches: ShortStitches {
                distance: 0.25,
                inset: 15.0,
            },
            ..SatinShaping::default()
        };
        let short = satin_with(&rungs, 2.0, 0.0, &shaping);
        assert_eq!(plain.len(), short.len());
        assert!(
            crowd(&short) as f64 <= 0.7 * crowd(&plain) as f64,
            "{} vs {}",
            crowd(&short),
            crowd(&plain)
        );
        // The outer edge is untouched; shortened points move 15 % of the width inwards.
        let mut moved = 0;
        for (a, b) in plain.iter().zip(&short) {
            if a.length() > 40.0 {
                assert_eq!(a, b);
            } else if a != b {
                moved += 1;
                assert!(
                    (b.length() - a.length() - 6.0).abs() < 1.0,
                    "{a:?} -> {b:?}"
                );
            }
        }
        assert!(moved > 5);
        // Off (distance 0): nothing moves.
        let mut same = plain.clone();
        short_stitches(&mut same, 0.0, 0.15);
        assert_eq!(same, plain);
    }

    #[test]
    fn compensation_per_side_and_per_cent_of_width() {
        let straight = |w: f64| vec![[v2(0.0, 0.0), v2(0.0, w)], [v2(100.0, 0.0), v2(100.0, w)]];
        let pct = SatinShaping {
            compensation: Compensation {
                percent: 3.0,
                ..Compensation::default()
            },
            ..SatinShaping::default()
        };
        // 3 % of a 2 mm and a 6 mm stitch: 0.06 and 0.18 mm more on each side.
        for (w, extra) in [(20.0, 0.6), (60.0, 1.8)] {
            let run = satin_with(&straight(w), 4.0, 0.0, &pct);
            assert!((run[0].y + extra).abs() < 1e-9, "{:?}", run[0]);
            assert!((run[1].y - w - extra).abs() < 1e-9, "{:?}", run[1]);
        }
        // One side only moves that side; the max caps it.
        let left = SatinShaping {
            compensation: Compensation {
                left: Some(0.4),
                max: Some(0.3),
                ..Compensation::default()
            },
            ..SatinShaping::default()
        };
        let run = satin_with(&straight(40.0), 4.0, 1.0, &left);
        assert!(
            (run[0].y + 3.0).abs() < 1e-9,
            "left capped at 0.3 mm: {:?}",
            run[0]
        );
        assert!(
            (run[1].y - 41.0).abs() < 1e-9,
            "right keeps pull: {:?}",
            run[1]
        );
        // Nothing set: PE-DESIGN's satin, unchanged.
        let plain = satin(&bend(), 4.0, 3.0);
        assert_eq!(
            satin_with(&bend(), 4.0, 3.0, &SatinShaping::default()),
            plain
        );
    }

    #[test]
    fn automatic_underlay_follows_the_width_and_a_second_layer_follows_the_first() {
        let a = AutoUnderlay::default();
        assert_eq!(a.style(1.5), UnderlayStyle::CentreWalk);
        assert_eq!(a.style(3.0), UnderlayStyle::Edge);
        assert_eq!(a.style(5.0), UnderlayStyle::EdgeZigzag);
        let knit = AutoUnderlay {
            centre_below: 1.5,
            edge_below: 3.0,
        };
        assert_eq!(knit.style(1.8), UnderlayStyle::Edge);
        assert_eq!(knit.style(3.5), UnderlayStyle::EdgeZigzag);
        let col = |w: f64| vec![[v2(0.0, 0.0), v2(0.0, w)], [v2(300.0, 0.0), v2(300.0, w)]];
        assert!((mean_width(&col(15.0)) - 1.5).abs() < 1e-9);
        let mut r = RegionSewing::new(RegionType::Satin, openstitch_core::Thread::rgb(0xED171F));
        r.underlay_style = UnderlayStyle::Auto;
        // A 1.5 mm column: a centre walk, every point on the middle line.
        let thin = layers(&col(15.0), &r);
        assert_eq!(thin.len(), 2);
        assert!(
            thin[0][0]
                .iter()
                .all(|q| (q.y - 7.5).abs() < 1e-6 || q.x == 0.0)
        );
        // A 5 mm column: the edge run and the zigzag (more stitches than the edge alone).
        let wide = layers(&col(50.0), &r);
        r.underlay_style = UnderlayStyle::Edge;
        let edge = layers(&col(50.0), &r);
        assert!(wide[0][0].len() > edge[0][0].len());
        // A second layer sews after the first, before the satin.
        r.underlay2 = Some(SecondUnderlay::default());
        let two = layers(&col(50.0), &r);
        assert_eq!(two.len(), 3);
        assert_eq!(two[0], edge[0]);
        assert_eq!(two[2], edge[1]);
        assert!(two[1][0].len() > 2);
    }

    fn straight(w: f64) -> Vec<[Vec2; 2]> {
        vec![[v2(0.0, 0.0), v2(0.0, w)], [v2(300.0, 0.0), v2(300.0, w)]]
    }

    fn styled(style: SatinStyle) -> SatinShaping {
        SatinShaping {
            style,
            ..SatinShaping::default()
        }
    }

    #[test]
    fn double_satin_sews_back_between_the_first_pass() {
        let one = satin(&straight(40.0), 2.0, 0.0);
        let two = satin_with(&straight(40.0), 2.0, 0.0, &styled(SatinStyle::Double));
        assert_eq!(&two[..one.len()], &one[..], "pass 1 is the plain satin");
        let back = &two[one.len()..];
        // Pass 2 starts where pass 1 ends (no jump) and ends near the start.
        assert!(one.last().expect("end").distance(back[0]) < 10.0);
        assert!(back.last().expect("start").x < 10.0);
        // Its needle points sit midway between pass-1 points on the same edge.
        for (k, q) in back.iter().enumerate() {
            let i = one.len() - 3 - k;
            assert_eq!(*q, one[i].lerp(one[i + 2], 0.5));
        }
    }

    #[test]
    fn open_and_s_stitch_satin() {
        let plain = satin(&straight(40.0), 2.0, 0.0);
        let open = satin_with(&straight(40.0), 2.0, 0.0, &styled(SatinStyle::Open));
        let ratio = plain.len() as f64 / open.len() as f64;
        assert!((ratio - 3.0).abs() < 0.2, "{ratio}");
        let s = satin_with(&straight(40.0), 2.0, 0.0, &styled(SatinStyle::SStitch));
        assert_eq!(s.len(), 3 * plain.len() - 2, "two bends per stitch");
        for w in s.windows(2) {
            let l = w[0].distance(w[1]);
            assert!((5.0..=70.0).contains(&l), "{l}");
        }
        // Inside the column.
        assert!(s.iter().all(|q| (-1e-6..=40.0 + 1e-6).contains(&q.y)));
    }

    #[test]
    fn jagged_edge_is_seeded_and_one_sided() {
        let one_side = SatinShaping {
            jagged: Some(Jagged {
                mode: JaggedMode::In,
                side: JaggedSide::Right,
                amount: 1.0,
                seed: 7,
            }),
            ..SatinShaping::default()
        };
        let plain = satin(&straight(40.0), 2.0, 0.0);
        let a = satin_with(&straight(40.0), 2.0, 0.0, &one_side);
        assert_eq!(
            a,
            satin_with(&straight(40.0), 2.0, 0.0, &one_side),
            "same seed"
        );
        let mut other = one_side;
        other.jagged.as_mut().expect("j").seed = 8;
        assert_ne!(a, satin_with(&straight(40.0), 2.0, 0.0, &other));
        // The left edge stays smooth; the right moves in by up to 1 mm, never out.
        for (p, q) in plain.iter().zip(&a) {
            if p.y == 0.0 {
                assert_eq!(p, q);
            } else {
                assert!(q.y <= 40.0 + 1e-9 && q.y >= 30.0 - 1e-9, "{q:?}");
            }
        }
        assert!(a.iter().any(|q| q.y > 0.0 && q.y < 39.0));
        let out = SatinShaping {
            jagged: Some(Jagged {
                mode: JaggedMode::Out,
                ..Jagged::default()
            }),
            ..SatinShaping::default()
        };
        let o = satin_with(&straight(40.0), 2.0, 0.0, &out);
        assert!(o.iter().any(|q| q.y > 40.0) && o.iter().any(|q| q.y < 0.0));
    }

    #[test]
    fn spacing_measured_across_a_curved_column() {
        // A section whose left rail is 10 mm and right rail 20 mm long.
        let fan = vec![
            [v2(0.0, 0.0), v2(0.0, 100.0)],
            [v2(100.0, 0.0), v2(173.2, 100.0)],
        ];
        let at = |a: Option<f64>| {
            satin_with(
                &fan,
                2.0,
                0.0,
                &SatinShaping {
                    spacing_at: a,
                    ..SatinShaping::default()
                },
            )
            .len()
        };
        assert_eq!(at(None), satin(&fan, 2.0, 0.0).len());
        assert_eq!(at(Some(1.0)), at(None), "1 is PE-DESIGN's");
        assert!(at(Some(0.0)) < at(Some(0.5)) && at(Some(0.5)) < at(None));
    }

    #[test]
    fn raised_satin_pads_under_the_column() {
        let raised = Raised::default();
        // 3, 5 and 8 mm columns: lanes inset 0.5 mm, about one per 1.5 mm, an even number.
        for (w, lanes) in [(30.0, 2), (50.0, 4), (80.0, 6)] {
            let pad = padding(&straight(w), &raised);
            let mut ys: Vec<i64> = pad.iter().map(|q| (q.y * 100.0).round() as i64).collect();
            ys.sort_unstable();
            ys.dedup();
            assert_eq!(ys.len(), lanes, "{w}: {ys:?}");
            assert!(
                pad.iter()
                    .all(|q| q.y >= 5.0 - 1e-6 && q.y <= w - 5.0 + 1e-6)
            );
            // Ends at the start end next to the satin's first needle point.
            let end = *pad.last().expect("end");
            assert!(end.distance(v2(5.0, 5.0)) < 1e-6, "{end:?}");
            // Stitch count follows the length.
            let long = vec![[v2(0.0, 0.0), v2(0.0, w)], [v2(600.0, 0.0), v2(600.0, w)]];
            assert!(padding(&long, &raised).len() > pad.len() * 3 / 2);
        }
        let mut r = RegionSewing::new(RegionType::Satin, openstitch_core::Thread::rgb(0xED171F));
        r.underlay = false;
        r.direction_lines = straight(50.0);
        r.satin_shaping.raised = Some(raised);
        let l = layers(&straight(50.0), &r);
        assert_eq!(l.len(), 2, "padding, then the satin");
        assert_eq!(l[1][0], satin(&straight(50.0), 10.0 / 4.5, 0.0));
    }

    #[test]
    fn twisted_rungs_untangle_and_sides_swap() {
        let mut twisted = bend();
        twisted[1] = [twisted[1][1], twisted[1][0]];
        assert_eq!(validate(&twisted), Err(RungError::Crossing(0)));
        let (fixed, turned) = untangle(&twisted);
        assert_eq!(turned, 1);
        assert_eq!(fixed, bend());
        assert_eq!(untangle(&bend()), (bend(), 0));
        // A real overlap stays an error.
        let crossing = vec![
            [v2(0.0, 0.0), v2(0.0, 40.0)],
            [v2(-20.0, 20.0), v2(20.0, 20.0)],
        ];
        assert!(validate(&untangle(&crossing).0).is_err());
        // Swapping sides keeps the needle points and starts each crossing on the other rail.
        let a = satin(&bend(), 4.0, 0.0);
        let b = satin(&swap_sides(&bend()), 4.0, 0.0);
        let key = |v: &Run| {
            let mut k: Vec<(i64, i64)> = v
                .iter()
                .map(|q| ((q.x * 1e6) as i64, (q.y * 1e6) as i64))
                .collect();
            k.sort_unstable();
            k
        };
        assert_eq!(key(&a), key(&b));
        assert_eq!(b[0], bend()[0][1]);
    }
}
