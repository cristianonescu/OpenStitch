//! Variable-direction satin along a skeleton: a satin column whose stitches stay perpendicular
//! to a user-given centre line, with a width given at each centre-line point and interpolated
//! along it. Around a bend the direction turns smoothly over the column's half width. Own
//! algorithm. Points are 0.1 mm, widths mm.

use crate::fmath::Det;
use crate::generate::Run;
use crate::geometry::{Contour, Transform, Vec2, v2};
use serde::{Deserialize, Serialize};

/// Widest column, mm.
pub const MAX_WIDTH: f64 = 30.0;
/// Most centre-line points.
pub const MAX_POINTS: usize = 10_000;
/// Width used where a point has none, mm.
pub const DEFAULT_WIDTH: f64 = 3.0;
/// Most stitches across one column.
const MAX_RUNGS: usize = 200_000;

/// Centre line and the column width at each of its points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skeleton {
    /// Centre line, 0.1 mm.
    pub points: Vec<Vec2>,
    /// Full column width at each point, mm.
    pub widths: Vec<f64>,
}

/// Why a skeleton cannot make a column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkeletonError {
    TooFew,
    TooMany,
    /// The number of widths differs from the number of points.
    WidthCount,
    /// Width `i` is not between 0 and [`MAX_WIDTH`] mm.
    Width(usize),
    /// Every width is (nearly) zero.
    Thin,
    /// The centre line is shorter than 0.1 mm or not finite.
    Short,
}

impl Skeleton {
    pub fn validate(&self) -> Result<(), SkeletonError> {
        if self.points.len() < 2 {
            return Err(SkeletonError::TooFew);
        }
        if self.points.len() > MAX_POINTS {
            return Err(SkeletonError::TooMany);
        }
        if self.widths.len() != self.points.len() {
            return Err(SkeletonError::WidthCount);
        }
        if let Some(i) = self
            .widths
            .iter()
            .position(|w| !(w.is_finite() && (0.0..=MAX_WIDTH).contains(w)))
        {
            return Err(SkeletonError::Width(i));
        }
        if self.widths.iter().all(|&w| w < 0.1) {
            return Err(SkeletonError::Thin);
        }
        if !self
            .points
            .iter()
            .all(|p| p.x.is_finite() && p.y.is_finite())
            || self.length() < 1.0
        {
            return Err(SkeletonError::Short);
        }
        Ok(())
    }

    pub fn length(&self) -> f64 {
        self.points.windows(2).map(|w| w[0].distance(w[1])).sum()
    }

    /// Moved by `t`; widths scale with the transform's average scale.
    pub fn transformed(&self, t: &Transform) -> Skeleton {
        let k = (t.a * t.d - t.b * t.c).abs().sqrt();
        Skeleton {
            points: self.points.iter().map(|&p| t.apply(p)).collect(),
            widths: self.widths.iter().map(|w| (w * k).min(MAX_WIDTH)).collect(),
        }
    }
}

/// The skeleton cleaned for sewing: finite points, coincident neighbours merged, a width for
/// every point (0.1 mm, half width).
struct Centre {
    points: Vec<Vec2>,
    half: Vec<f64>,
    /// Arc length at each point.
    at: Vec<f64>,
}

impl Centre {
    fn new(s: &Skeleton) -> Option<Centre> {
        let mut points: Vec<Vec2> = Vec::new();
        let mut half: Vec<f64> = Vec::new();
        for (i, &p) in s.points.iter().enumerate() {
            if !(p.x.is_finite() && p.y.is_finite()) {
                continue;
            }
            let w = s
                .widths
                .get(i)
                .or(s.widths.last())
                .copied()
                .unwrap_or(DEFAULT_WIDTH);
            let w = if w.is_finite() {
                w.clamp(0.0, MAX_WIDTH)
            } else {
                0.0
            };
            if points.last().is_some_and(|q| q.distance(p) < 1e-6) {
                continue;
            }
            points.push(p);
            half.push(w * 5.0);
        }
        if points.len() < 2 {
            return None;
        }
        let mut at = vec![0.0];
        for w in points.windows(2) {
            at.push(at[at.len() - 1] + w[0].distance(w[1]));
        }
        (at[at.len() - 1] >= 1e-6 && at[at.len() - 1].is_finite()).then_some(Centre {
            points,
            half,
            at,
        })
    }

    fn length(&self) -> f64 {
        self.at[self.at.len() - 1]
    }

    fn direction(&self, j: usize) -> Vec2 {
        let d = self.points[j + 1] - self.points[j];
        d * (1.0 / d.length())
    }

    /// Centre point, unit tangent and half width at arc length `s`.
    fn station(&self, s: f64) -> (Vec2, Vec2, f64) {
        let last = self.points.len() - 2;
        let j = self.at[1..].partition_point(|&a| a < s).min(last);
        let len = self.at[j + 1] - self.at[j];
        let f = ((s - self.at[j]) / len).clamp(0.0, 1.0);
        let centre = self.points[j].lerp(self.points[j + 1], f);
        let half = self.half[j] + (self.half[j + 1] - self.half[j]) * f;
        let u = self.direction(j);
        // Turn smoothly through the vertex at either end of this segment.
        let blend = |v: usize, d: f64| -> Option<Vec2> {
            let (before, after) = (self.direction(v - 1), self.direction(v));
            let reach = self.half[v]
                .min((self.at[v] - self.at[v - 1]) / 2.0)
                .min((self.at[v + 1] - self.at[v]) / 2.0);
            if reach <= 1e-9 || d.abs() >= reach {
                return None;
            }
            let k = 0.5 + d / (2.0 * reach);
            let t = before.lerp(after, k);
            (t.length() > 1e-6).then(|| t * (1.0 / t.length()))
        };
        let tangent = if j > 0 && s - self.at[j] < len / 2.0 {
            blend(j, s - self.at[j])
        } else if j < last {
            blend(j + 1, s - self.at[j + 1])
        } else {
            None
        };
        (centre, tangent.unwrap_or(u), half)
    }

    /// One rung per centre-line point [left, right]: square to the line at its ends and to
    /// the bisector at a vertex, as long as the column's offset edges make it there (half
    /// width over the cosine of half the turn, at most five half widths).
    fn vertex_rungs(&self) -> Vec<[Vec2; 2]> {
        let n = self.points.len();
        (0..n)
            .map(|i| {
                let (t, stretch) = if i == 0 {
                    (self.direction(0), 1.0)
                } else if i == n - 1 {
                    (self.direction(n - 2), 1.0)
                } else {
                    let (a, b) = (self.direction(i - 1), self.direction(i));
                    let sum = a + b;
                    if sum.length() < 1e-6 {
                        (a, 1.0)
                    } else {
                        let t = sum * (1.0 / sum.length());
                        (t, 1.0 / (t.x * b.x + t.y * b.y).max(0.2))
                    }
                };
                let h = self.half[i] * stretch;
                let left = v2(t.y, -t.x);
                let c = self.points[i];
                [c + left * h, c - left * h]
            })
            .collect()
    }

    /// Stitch ends across the column every `spacing` (or a little less) along the centre
    /// line, first and last ends included: [left, right], each lengthened by `pull`.
    fn rungs(&self, spacing: f64, pull: f64) -> Vec<[Vec2; 2]> {
        let length = self.length();
        let spacing = if spacing.is_finite() {
            spacing.max(0.1)
        } else {
            1.0
        };
        let pull = if pull.is_finite() { pull } else { 0.0 };
        let n = ((length / spacing).ceil() as usize).clamp(1, MAX_RUNGS);
        (0..=n)
            .map(|k| {
                let (c, t, half) = self.station(length * k as f64 / n as f64);
                let h = (half + pull).max(0.0);
                let left = v2(t.y, -t.x);
                [c + left * h, c - left * h]
            })
            .collect()
    }
}

/// Satin along the skeleton: the first rung sewn across, then one needle point per rung on
/// alternate sides ([`crate::column::zigzag`]), the rungs `spacing` apart along the centre
/// line (0.1 mm), each lengthened by `pull` at both ends. Empty for a degenerate skeleton.
pub fn satin(s: &Skeleton, spacing: f64, pull: f64) -> Run {
    let Some(c) = Centre::new(s) else {
        return Vec::new();
    };
    let rungs = c.rungs(spacing, pull);
    // The needle points of `column::zigzag`, with Half Stitch where a bend crowds one side
    // (#466): at a sharp apex every rung's inner end met in one spot, a pile of needle points.
    let sides: Vec<usize> = (0..rungs.len()).map(|k| 1 - k % 2).collect();
    let mut run = vec![rungs[0][0]];
    run.extend(crate::region::half_stitch(&rungs, &sides));
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
    run
}

/// The satin of a column from [`branches`] (#526): its rungs within `contours` (the part of
/// the shape the column covers, [`rungs_within`]), and past each end flagged in `ends` (an end
/// on the shape's outline) more rungs, square to the column's end, `spacing` apart and cut by
/// `contours`, until the outline closes them off. A square end rung sewed past a slanted end
/// edge on one side and left its acute corner on the other (K's legs, X's feet); these reach
/// both of the edge's corners. Empty for a degenerate skeleton.
pub fn satin_to_outline(
    s: &Skeleton,
    contours: &[Contour],
    spacing: f64,
    pull: f64,
    ends: [bool; 2],
) -> Run {
    let mut rungs = rungs_reaching(s, contours, spacing, pull, None, BRANCH_WITHIN_REACH);
    let Some(c) = Centre::new(s) else {
        return Vec::new();
    };
    if rungs.len() < 2 {
        return Vec::new();
    }
    let edges: Vec<(Vec2, Vec2)> = contours
        .iter()
        .filter(|c| c.closed)
        .flat_map(Contour::edges)
        .collect();
    let spacing = if spacing.is_finite() {
        spacing.max(0.1)
    } else {
        1.0
    };
    let length = c.length();
    for (e, &on) in ends.iter().enumerate() {
        if !on {
            continue;
        }
        let (p, t, half) = c.station(if e == 0 { 0.0 } else { length });
        let out = if e == 0 { t * -1.0 } else { t };
        let left = v2(t.y, -t.x);
        let reach = 4.0 * half.max(10.0);
        let mut extra = Vec::new();
        for k in 1..=MAX_END_RUNGS {
            let d = spacing * k as f64;
            if d > reach {
                break;
            }
            // The first rung too short to go on with is kept: it lies in the corner.
            match cut_across(p + out * d, left, &edges, 2.0 * half.max(10.0)) {
                Some(r) => {
                    extra.push(r);
                    if r[0].distance(r[1]) <= END_RUNG_MIN {
                        break;
                    }
                }
                None => break,
            }
        }
        if e == 0 {
            extra.reverse();
            rungs.splice(0..0, extra);
        } else {
            rungs.extend(extra);
        }
    }
    let sides: Vec<usize> = (0..rungs.len()).map(|k| 1 - k % 2).collect();
    let mut run = vec![rungs[0][0]];
    run.extend(crate::region::half_stitch(&rungs, &sides));
    // The satin is sewn across its first rung; past a flagged last end it is sewn across the
    // last too, so both of that end edge's corners get a needle point.
    if ends[1]
        && let (Some(&last), Some(r)) = (run.last(), rungs.last())
    {
        let other = if last.distance(r[0]) <= last.distance(r[1]) {
            r[1]
        } else {
            r[0]
        };
        run.push(other);
    }
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
    run
}

/// Most rungs [`satin_to_outline`] adds past one end.
const MAX_END_RUNGS: usize = 400;
/// A rung past an end this short (0.1 mm) or shorter is the last of them: it lies in the corner.
const END_RUNG_MIN: f64 = 3.0;

/// The stretch of the line through `q` along `left` that lies inside `edges` (even-odd) and
/// contains `q`, or else the nearest one within `near` (0.1 mm) of it: [its end towards
/// `left`, its other end]. `None` when there is none.
fn cut_across(q: Vec2, left: Vec2, edges: &[(Vec2, Vec2)], near: f64) -> Option<[Vec2; 2]> {
    let mut us: Vec<f64> = edges
        .iter()
        .filter_map(|&(a, b)| {
            let e = b - a;
            let den = left.x * e.y - left.y * e.x;
            if den.abs() < 1e-12 {
                return None;
            }
            let w = a - q;
            let u = (w.x * e.y - w.y * e.x) / den;
            let v = (w.x * left.y - w.y * left.x) / den;
            (0.0..1.0).contains(&v).then_some(u)
        })
        .collect();
    us.sort_by(f64::total_cmp);
    us.as_chunks::<2>()
        .0
        .iter()
        .map(|w| (w[0], w[1]))
        .filter(|&(lo, hi)| hi - lo > 1e-9)
        .map(|(lo, hi)| {
            let gap = if lo <= 0.0 && hi >= 0.0 {
                0.0
            } else {
                lo.abs().min(hi.abs())
            };
            (gap, lo, hi)
        })
        .filter(|&(gap, _, _)| gap <= near)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, lo, hi)| [q + left * hi, q + left * lo])
}

/// [`satin`] with OpenStitch's satin shaping (#675): corner constructions, short stitches,
/// compensation per side and per cent of width, the satin styles. With corner constructions
/// the column is one rung per centre-line point (square to the bisector, reaching the mitre
/// of the column's edges), so a corner of the centre line is a corner of the rung chain;
/// otherwise rungs every 1 mm along the smoothly turning column. Without shaping it is
/// [`satin`].
pub fn satin_with(
    s: &Skeleton,
    spacing: f64,
    pull: f64,
    shaping: &crate::column::SatinShaping,
) -> Run {
    if shaping.is_plain() {
        return satin(s, spacing, pull);
    }
    let Some(c) = Centre::new(s) else {
        return Vec::new();
    };
    let rungs = if shaping.corners.style == crate::column::CornerStyle::Turn {
        c.rungs(10.0, 0.0)
    } else {
        c.vertex_rungs()
    };
    let pull = if pull.is_finite() { pull } else { 0.0 };
    crate::column::zigzag_shaped(&rungs, spacing, pull, shaping)
}

/// One rung per centre-line point of `s` [left, right]: what [`satin_with`] builds corners
/// on (#675). Empty for a degenerate skeleton.
pub fn vertex_rungs(s: &Skeleton) -> Vec<[Vec2; 2]> {
    Centre::new(s).map_or_else(Vec::new, |c| c.vertex_rungs())
}

/// [`vertex_rungs`] reaching the outline of the closed region `contours` along each rung
/// (at most 0.5 mm past the skeleton's width): an Auto satin region's corner constructions
/// (#675) on its derived skeleton, whose widths are only approximate.
pub fn vertex_rungs_within(s: &Skeleton, contours: &[Contour]) -> Vec<[Vec2; 2]> {
    let edges: Vec<(Vec2, Vec2)> = contours
        .iter()
        .filter(|c| c.closed)
        .flat_map(Contour::edges)
        .collect();
    vertex_rungs(s)
        .into_iter()
        .map(|[l, r]| {
            let c = l.lerp(r, 0.5);
            let half = c.distance(l);
            if half < 1e-6 || !crate::geometry::contains(c, contours) {
                return [l, r];
            }
            let reach = |end: Vec2| {
                let dir = (end - c) * (1.0 / half);
                c + dir * ray(c, dir, &edges).map_or(half, |d| d.min(half + 5.0))
            };
            [reach(l), reach(r)]
        })
        .collect()
}

/// OpenStitch's under sewing of a skeleton satin where its region asks for it (#675, see
/// [`crate::column::rung_underlay`]): the width-following rows of `underlay: auto`, the
/// centre walk, the second layer. From the column's start, where the satin begins.
pub fn underlay(s: &Skeleton, r: &crate::model::RegionSewing) -> [Option<Run>; 2] {
    let Some(c) = Centre::new(s) else {
        return [None, None];
    };
    let rungs = c.rungs(10.0, 0.0);
    let Some(first) = rungs.first() else {
        return [None, None];
    };
    crate::column::rung_underlay(&rungs, r, r.entry.unwrap_or(first[0]))
}

/// The column of a skeleton as rungs [left, right] about `step` (0.1 mm) apart along its
/// centre line, at its own widths: what the motif band (#544) is stretched across.
pub fn rungs(s: &Skeleton, step: f64) -> Vec<[Vec2; 2]> {
    Centre::new(s).map_or_else(Vec::new, |c| c.rungs(step, 0.0))
}

/// The rungs of [`satin`] filling the closed region `contours`, [left, right] in sewing
/// order: each rung reaches from the centre line to the region's outline along the rung (at
/// most 0.5 mm beyond the skeleton's half width), so the column covers the region where a
/// derived width is only approximate; then it is lengthened by `pull` at both ends. The rungs
/// are `spacing` apart along the column's longer side, as PE-DESIGN 11 spaces Auto satin: on
/// a bend the outer edge keeps the density and the rungs crowd on the inner edge. Empty for
/// a degenerate skeleton.
pub fn rungs_within(s: &Skeleton, contours: &[Contour], spacing: f64, pull: f64) -> Vec<[Vec2; 2]> {
    rungs_within_from(s, contours, spacing, pull, None)
}

/// [`rungs_within`] with the spacing kept exact and the rungs on a lattice anchored `beyond`
/// (0.1 mm) past the skeleton's end along the longer side: PE-DESIGN's Auto satin counts its
/// rungs from the far end it sews first (satin-auto-bend-90-nohalf: the outer edge's needle
/// points lie at whole spacings from the far edge), and the remainder is left at the start.
/// `None`: rungs evenly stretched from end to end.
pub fn rungs_within_from(
    s: &Skeleton,
    contours: &[Contour],
    spacing: f64,
    pull: f64,
    beyond: Option<f64>,
) -> Vec<[Vec2; 2]> {
    rungs_reaching(s, contours, spacing, pull, beyond, WITHIN_REACH)
}

/// How far past the skeleton's half width (0.1 mm) [`rungs_within`] reaches for the outline.
const WITHIN_REACH: f64 = 5.0;
/// ... and a branch column's ([`satin_to_outline`], #526): a derived centre line can lie off
/// the stroke's middle (A's bar, A's left leg past the bar at 25 mm), and its rungs fell short
/// of one edge.
const BRANCH_WITHIN_REACH: f64 = 25.0;

/// [`rungs_within_from`] with the rungs reaching up to `cap` (0.1 mm) past the half width.
fn rungs_reaching(
    s: &Skeleton,
    contours: &[Contour],
    spacing: f64,
    pull: f64,
    beyond: Option<f64>,
    cap: f64,
) -> Vec<[Vec2; 2]> {
    let Some(c) = Centre::new(s) else {
        return Vec::new();
    };
    let edges: Vec<(Vec2, Vec2)> = contours
        .iter()
        .filter(|c| c.closed)
        .flat_map(Contour::edges)
        .collect();
    let pull = if pull.is_finite() { pull } else { 0.0 };
    let spacing = if spacing.is_finite() {
        spacing.max(0.1)
    } else {
        1.0
    };
    let rung_at = |s: f64| -> [Vec2; 2] {
        let (centre, t, half) = c.station(s);
        let left = v2(t.y, -t.x);
        let (l, r) = (centre + left * half, centre - left * half);
        if half < 1e-6 || !crate::geometry::contains(centre, contours) {
            return [l, r];
        }
        let reach = |end: Vec2| {
            let dir = (end - centre) * (1.0 / half);
            let d = ray(centre, dir, &edges).map_or(half, |d| d.min(half + cap));
            centre + dir * (d + pull).max(0.0)
        };
        [reach(l), reach(r)]
    };
    // Sample the column finely, measure along the longer side, then space the rungs evenly
    // on that measure.
    let length = c.length();
    let fine = ((4.0 * length / spacing).ceil() as usize).clamp(1, MAX_RUNGS);
    let at: Vec<f64> = (0..=fine)
        .map(|k| length * k as f64 / fine as f64)
        .collect();
    let samples: Vec<[Vec2; 2]> = at.iter().map(|&s| rung_at(s)).collect();
    let mut side = vec![0.0];
    for w in samples.windows(2) {
        let step = w[0][0].distance(w[1][0]).max(w[0][1].distance(w[1][1]));
        side.push(side[side.len() - 1] + step);
    }
    let total = side[fine];
    let targets: Vec<f64> = match beyond {
        None => {
            let n = ((total / spacing).ceil() as usize).clamp(1, MAX_RUNGS);
            (0..=n).map(|i| total * i as f64 / n as f64).collect()
        }
        Some(beyond) => {
            let anchor = total + beyond.max(0.0);
            let first = ((anchor - total) / spacing).ceil().max(0.0);
            let last = (anchor / spacing).floor().max(first);
            let n = ((last - first) as usize).min(MAX_RUNGS);
            (0..=n)
                .rev()
                .map(|i| (anchor - spacing * (first + i as f64)).max(0.0))
                .collect()
        }
    };
    targets
        .into_iter()
        .map(|target| {
            let j = side.partition_point(|&m| m < target).clamp(1, fine);
            let span = side[j] - side[j - 1];
            let f = if span > 1e-9 {
                ((target - side[j - 1]) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            rung_at(at[j - 1] + (at[j] - at[j - 1]) * f)
        })
        .collect()
}

/// A width key of a satin line (#524): at `at` (0 = the line's start, 1 = its end) the column
/// reaches `left` mm to the left of the line and `right` mm to the right (left: the side to
/// the left of the drawing direction). Between keys the widths change smoothly.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidthKey {
    pub at: f64,
    pub left: f64,
    pub right: f64,
}

/// How a satin line's column follows its line (#524).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SatinLine {
    /// Full width where there are no keys, mm.
    pub width: f64,
    /// Width keys, any order.
    pub keys: Vec<WidthKey>,
    /// Taper lengths at the start and the end, mm (0 = none).
    pub taper: [f64; 2],
    /// Degrees the crossings turn from square to the line (−60…60).
    pub slant: f64,
}

impl SatinLine {
    /// Whether the column is the line's width all along, square to it: plain skeleton satin.
    pub fn is_uniform(&self) -> bool {
        self.taper == [0.0, 0.0]
            && self.slant == 0.0
            && self
                .keys
                .iter()
                .all(|k| k.left == k.right && k.left == self.keys[0].left)
    }

    /// The full width of a uniform line, mm.
    pub fn uniform_width(&self) -> f64 {
        self.keys.first().map_or(self.width, |k| k.left + k.right)
    }

    /// The left and right reach (mm) at `f` (0…1) along a line `length` (0.1 mm) long.
    pub fn sides(&self, f: f64, length: f64) -> [f64; 2] {
        let smooth = |t: f64| {
            let t = t.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let mut keys = self.keys.clone();
        keys.sort_by(|a, b| a.at.total_cmp(&b.at));
        let [mut l, mut r] = match keys.as_slice() {
            [] => [self.width / 2.0; 2],
            [k] => [k.left, k.right],
            _ => {
                let j = keys.partition_point(|k| k.at <= f);
                if j == 0 {
                    [keys[0].left, keys[0].right]
                } else if j == keys.len() {
                    let k = keys[j - 1];
                    [k.left, k.right]
                } else {
                    let (a, b) = (keys[j - 1], keys[j]);
                    let t = smooth((f - a.at) / (b.at - a.at).max(1e-9));
                    [
                        a.left + (b.left - a.left) * t,
                        a.right + (b.right - a.right) * t,
                    ]
                }
            }
        };
        let s = f * length / 10.0;
        let e = (1.0 - f) * length / 10.0;
        let mut k = 1.0;
        if self.taper[0] > 0.0 {
            k *= smooth(s / self.taper[0]);
        }
        if self.taper[1] > 0.0 {
            k *= smooth(e / self.taper[1]);
        }
        l *= k;
        r *= k;
        [l.clamp(0.0, MAX_WIDTH), r.clamp(0.0, MAX_WIDTH)]
    }
}

/// The rungs [left, right] of a satin line along `path` (0.1 mm), about `step` apart: each
/// crosses the line at its point, turned `slant` from square, reaching the line's left and
/// right widths measured square to it. Empty for a degenerate line.
pub fn satin_line_rungs(path: &[Vec2], line: &SatinLine, step: f64) -> Vec<[Vec2; 2]> {
    let widest = line
        .keys
        .iter()
        .map(|k| k.left + k.right)
        .fold(line.width, f64::max)
        .clamp(0.1, MAX_WIDTH);
    let Some(c) = Centre::new(&Skeleton {
        points: path.to_vec(),
        widths: vec![widest; path.len()],
    }) else {
        return Vec::new();
    };
    let length = c.length();
    let n = ((length / step.max(1.0)).ceil() as usize).clamp(1, MAX_RUNGS);
    let a = line.slant.clamp(-60.0, 60.0).to_radians();
    let (sin, cos) = (crate::fmath::Det::dsin(a), crate::fmath::Det::dcos(a));
    (0..=n)
        .map(|k| {
            let f = k as f64 / n as f64;
            let (p, t, _) = c.station(length * f);
            let [l, r] = line.sides(f, length);
            let left = v2(t.y, -t.x);
            let dir = left * cos + t * sin;
            [p + dir * (l * 10.0 / cos), p - dir * (r * 10.0 / cos)]
        })
        .collect()
}

/// The column's outline: the left side along the centre line, then the right side back.
pub fn outline(s: &Skeleton) -> Option<Contour> {
    let c = Centre::new(s)?;
    let rungs = c.rungs(5.0, 0.0);
    let mut pts: Vec<Vec2> = rungs.iter().map(|r| r[0]).collect();
    pts.extend(rungs.iter().rev().map(|r| r[1]));
    pts.dedup_by(|a, b| a.distance(*b) < 1e-6);
    (pts.len() >= 3).then(|| Contour::new(pts, true))
}

/// Grid cells along the longer side of the region when a skeleton is derived.
const DERIVE_CELLS: f64 = 160.0;
/// Most region edges a derivation looks at.
const DERIVE_MAX_EDGES: usize = 5_000;
/// Clearance growth along the line (per unit length) above which an end runs into a corner
/// (a corner of about 25° or more).
const CORNER_RATE: f64 = 0.2;

/// A grid over the region: which cells are inside and how far each inside cell's centre is
/// from the outline (0.1 mm).
struct Grid {
    origin: Vec2,
    step: f64,
    nx: usize,
    ny: usize,
    /// Distance to the outline; negative outside.
    clearance: Vec<f64>,
}

impl Grid {
    fn new(edges: &[(Vec2, Vec2)], b: &crate::geometry::Rect) -> Option<Grid> {
        let step = (b.width().max(b.height()) / DERIVE_CELLS).max(0.25);
        let nx = (b.width() / step).ceil() as usize + 1;
        let ny = (b.height() / step).ceil() as usize + 1;
        if nx < 2 || ny < 2 || nx * ny > 200_000 {
            return None;
        }
        let origin = v2(b.min_x, b.min_y);
        let mut clearance = vec![-1.0; nx * ny];
        let mut crossings = Vec::new();
        for j in 0..ny {
            let y = origin.y + (j as f64 + 0.5) * step;
            // Even-odd: the cells between alternate crossings of this row are inside.
            crossings.clear();
            for &(a, c) in edges {
                if (a.y > y) != (c.y > y) {
                    crossings.push(a.x + (y - a.y) * (c.x - a.x) / (c.y - a.y));
                }
            }
            crossings.sort_by(f64::total_cmp);
            for pair in crossings.as_chunks::<2>().0 {
                for i in 0..nx {
                    let x = origin.x + (i as f64 + 0.5) * step;
                    if x > pair[0] && x < pair[1] {
                        let p = v2(x, y);
                        clearance[j * nx + i] = edges
                            .iter()
                            .map(|&(a, c)| segment_distance(p, a, c))
                            .fold(f64::INFINITY, f64::min);
                    }
                }
            }
        }
        Some(Grid {
            origin,
            step,
            nx,
            ny,
            clearance,
        })
    }

    /// The clearance of the cell `p` lies in; negative outside the grid or the region.
    fn clearance_at(&self, p: Vec2) -> f64 {
        let i = ((p.x - self.origin.x) / self.step).floor();
        let j = ((p.y - self.origin.y) / self.step).floor();
        if i < 0.0 || j < 0.0 || i >= self.nx as f64 || j >= self.ny as f64 {
            return -1.0;
        }
        self.clearance[j as usize * self.nx + i as usize]
    }

    /// The cell `p` lies in; `None` outside the grid.
    fn cell_at(&self, p: Vec2) -> Option<usize> {
        let i = ((p.x - self.origin.x) / self.step).floor();
        let j = ((p.y - self.origin.y) / self.step).floor();
        (i >= 0.0 && j >= 0.0 && i < self.nx as f64 && j < self.ny as f64)
            .then(|| j as usize * self.nx + i as usize)
    }

    fn centre(&self, k: usize) -> Vec2 {
        v2(
            self.origin.x + ((k % self.nx) as f64 + 0.5) * self.step,
            self.origin.y + ((k / self.nx) as f64 + 0.5) * self.step,
        )
    }

    /// Inside neighbours of cell `k` and the step to each (in cells).
    fn neighbours(&self, k: usize) -> impl Iterator<Item = (usize, f64)> + '_ {
        let (i, j) = ((k % self.nx) as i64, (k / self.nx) as i64);
        [
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
            (1, 1),
        ]
        .into_iter()
        .filter_map(move |(di, dj)| {
            let (a, b) = (i + di, j + dj);
            if a < 0 || b < 0 || a >= self.nx as i64 || b >= self.ny as i64 {
                return None;
            }
            let n = b as usize * self.nx + a as usize;
            (self.clearance[n] >= 0.0).then_some((
                n,
                if di != 0 && dj != 0 {
                    std::f64::consts::SQRT_2
                } else {
                    1.0
                },
            ))
        })
    }

    /// Shortest paths from `from` with the step cost `cost(cell entered, step)`: the cost to
    /// each cell and the cell before it.
    fn paths(&self, from: usize, cost: impl Fn(usize, f64) -> f64) -> (Vec<f64>, Vec<usize>) {
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;
        #[derive(PartialEq)]
        struct Cost(f64);
        impl Eq for Cost {}
        impl PartialOrd for Cost {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Cost {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                self.0.total_cmp(&o.0)
            }
        }
        let mut best = vec![f64::INFINITY; self.clearance.len()];
        let mut prev = vec![usize::MAX; self.clearance.len()];
        let mut heap = BinaryHeap::new();
        best[from] = 0.0;
        heap.push(Reverse((Cost(0.0), from)));
        while let Some(Reverse((Cost(c), k))) = heap.pop() {
            if c > best[k] {
                continue;
            }
            for (n, step) in self.neighbours(k) {
                let nc = c + cost(n, step);
                if nc < best[n] {
                    best[n] = nc;
                    prev[n] = k;
                    heap.push(Reverse((Cost(nc), n)));
                }
            }
        }
        (best, prev)
    }
}

pub(crate) fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let d = b - a;
    let len2 = d.x * d.x + d.y * d.y;
    let t = if len2 > 0.0 {
        (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + d * t)
}

/// Distance from `p` along `dir` (unit) to the nearest edge, if one is hit.
fn ray(p: Vec2, dir: Vec2, edges: &[(Vec2, Vec2)]) -> Option<f64> {
    edges
        .iter()
        .filter_map(|&(a, b)| {
            let e = b - a;
            let den = dir.x * e.y - dir.y * e.x;
            if den.abs() < 1e-12 {
                return None;
            }
            let w = a - p;
            let t = (w.x * e.y - w.y * e.x) / den;
            let u = (w.x * dir.y - w.y * dir.x) / den;
            (t > 1e-9 && (-1e-9..=1.0 + 1e-9).contains(&u)).then_some(t)
        })
        .min_by(f64::total_cmp)
}

/// Douglas–Peucker: the indexes of `pts` to keep so the line stays within `tolerance`.
fn simplify(pts: &[Vec2], tolerance: f64) -> Vec<usize> {
    fn walk(pts: &[Vec2], lo: usize, hi: usize, tolerance: f64, keep: &mut Vec<usize>) {
        let (far, d) = (lo + 1..hi)
            .map(|i| (i, segment_distance(pts[i], pts[lo], pts[hi])))
            .fold((lo, 0.0), |a, b| if b.1 > a.1 { b } else { a });
        if d > tolerance {
            walk(pts, lo, far, tolerance, keep);
            walk(pts, far, hi, tolerance, keep);
        } else {
            keep.push(hi);
        }
    }
    let mut keep = vec![0];
    if pts.len() > 1 {
        walk(pts, 0, pts.len() - 1, tolerance, &mut keep);
    }
    keep
}

/// A skeleton for the closed region `contours` (0.1 mm, even-odd, holes allowed): an
/// approximation of its medial axis, not an exact one. The region is sampled on a grid of
/// about 160 cells along its longer side; the centre line runs between the two cells farthest
/// apart through the region, along the path that keeps furthest from the outline. Where an
/// end runs into a corner (the clearance grows quickly along it) it is trimmed back, at most a
/// quarter of the line, then reaches out again most of its clearance towards the end of the
/// stroke. The line is smoothed and simplified, and the width at each point is the chord
/// across the region perpendicular to the line there (twice the clearance where that chord
/// runs far along the region at a bend). Widths are capped at
/// [`MAX_WIDTH`]. Branches are not followed: a Y gives its longest stroke. `None` when the
/// region is empty, too small, or no line can be found.
pub fn derive(contours: &[Contour]) -> Option<Skeleton> {
    let edges: Vec<(Vec2, Vec2)> = contours
        .iter()
        .filter(|c| c.closed)
        .flat_map(Contour::edges)
        .filter(|(a, b)| a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite())
        .collect();
    if edges.len() < 3 || edges.len() > DERIVE_MAX_EDGES {
        return None;
    }
    let b = crate::geometry::Rect::enclosing(edges.iter().map(|e| e.0))?;
    if b.width().max(b.height()) < 2.0 {
        return None;
    }
    let grid = Grid::new(&edges, &b)?;
    // Start in the widest part, so a region in pieces gives its main piece.
    let seed = (0..grid.clearance.len())
        .max_by(|&a, &c| grid.clearance[a].total_cmp(&grid.clearance[c]))
        .filter(|&k| grid.clearance[k] > 0.0)?;
    let farthest = |best: &[f64]| {
        (0..best.len())
            .filter(|&k| best[k].is_finite())
            .max_by(|&a, &c| best[a].total_cmp(&best[c]))
    };
    let length = |_: usize, step: f64| step;
    let a = farthest(&grid.paths(seed, length).0)?;
    let end = farthest(&grid.paths(a, length).0)?;
    if a == end {
        return None;
    }
    // Along the ridge: steps close to the outline cost much more.
    let widest = grid.clearance[seed];
    let (_, prev) = grid.paths(a, |n, step| {
        let k = widest / (grid.clearance[n] + grid.step * 0.5);
        step * k * k
    });
    let mut cells = vec![end];
    while let Some(&k) = cells.last() {
        if k == a {
            break;
        }
        let p = prev[k];
        if p == usize::MAX || cells.len() > grid.clearance.len() {
            return None;
        }
        cells.push(p);
    }
    cells.reverse();
    let pts: Vec<Vec2> = cells.iter().map(|&k| grid.centre(k)).collect();
    let clear: Vec<f64> = cells.iter().map(|&k| grid.clearance[k]).collect();
    // Trim the ends that run into corners: there the clearance grows quickly along the
    // line, while along the middle of a stroke it stays about the same.
    let at: Vec<f64> = std::iter::once(0.0)
        .chain(pts.windows(2).scan(0.0, |s, w| {
            *s += w[0].distance(w[1]);
            Some(*s)
        }))
        .collect();
    let total = at[at.len() - 1];
    let window = 3.0 * grid.step;
    let mut lo = 0;
    while lo + 2 < pts.len() && at[lo] <= total / 4.0 {
        let Some(j) = (lo + 1..pts.len()).find(|&j| at[j] - at[lo] >= window) else {
            break;
        };
        if clear[j] - clear[lo] <= CORNER_RATE * (at[j] - at[lo]) {
            break;
        }
        lo += 1;
    }
    let mut hi = pts.len() - 1;
    while hi > lo + 2 && total - at[hi] <= total / 4.0 {
        let Some(j) = (0..hi).rev().find(|&j| at[hi] - at[j] >= window) else {
            break;
        };
        if clear[j] - clear[hi] <= CORNER_RATE * (at[hi] - at[j]) {
            break;
        }
        hi -= 1;
    }
    // Then reach back towards the end of the stroke, most of the clearance further.
    let reach = |end: usize, inner: usize| -> Option<Vec2> {
        let d = pts[end] - pts[inner];
        (d.length() > 1e-9 && clear[end] > grid.step)
            .then(|| pts[end] + d * (0.8 * clear[end] / d.length()))
    };
    let inner_of = |end: usize, toward: i64| {
        let mut k = end as i64;
        while (k + toward) >= lo as i64
            && (k + toward) <= hi as i64
            && (at[k as usize] - at[end]).abs() < (3.0 * grid.step).max(2.0 * clear[end])
        {
            k += toward;
        }
        k as usize
    };
    let head = reach(lo, inner_of(lo, 1));
    let tail = reach(hi, inner_of(hi, -1));
    let pts: Vec<Vec2> = head
        .into_iter()
        .chain(pts[lo..=hi].iter().copied())
        .chain(tail)
        .collect();
    line_to_skeleton(
        pts,
        head.is_some(),
        tail.is_some(),
        &edges,
        grid.step,
        WIDE_CHORD,
    )
}

/// Most columns one branching region is cut into.
const MAX_BRANCHES: usize = 16;
/// How far past a part's tip (in its widest clearances) a bridge is looked for: the part runs
/// into another column within this, before leaving the region (#526).
const BRIDGE_REACH: f64 = 6.0;

/// How far cell `p` lies inside the satin columns of `arms` (0.1 mm): its distance in from the
/// nearest column's edge, negative outside every column.
fn depth_in(p: Vec2, arms: &[Skeleton]) -> f64 {
    let mut best = f64::NEG_INFINITY;
    for sk in arms {
        for (w, ws) in sk.points.windows(2).zip(sk.widths.windows(2)) {
            let (a, b) = (w[0], w[1]);
            let d = b - a;
            let len2 = d.x * d.x + d.y * d.y;
            let t = if len2 > 0.0 {
                (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let half = (ws[0] + (ws[1] - ws[0]) * t) * 5.0;
            best = best.max(half - p.distance(a + d * t));
        }
    }
    best
}

/// One satin column per arm of a branching region (#526, beyond PE-DESIGN): the main stroke
/// as [`derive`] finds it, then, while part of the region is left uncovered that is longer than
/// it is wide (an arm of a Y, a star's point, a K's leg), a column from that arm's tip along
/// its ridge into the column it leaves, ending `join_overlap` mm inside it so the two do not
/// part. Broad leftovers and the nubs at a column's ends are not arms. A region without
/// branches gives the one skeleton [`derive`] gives; empty when it gives none.
pub fn branches(contours: &[Contour], join_overlap: f64) -> Vec<Skeleton> {
    let Some(main) = derive(contours) else {
        return Vec::new();
    };
    let edges: Vec<(Vec2, Vec2)> = contours
        .iter()
        .filter(|c| c.closed)
        .flat_map(Contour::edges)
        .filter(|(a, b)| a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite())
        .collect();
    let Some(grid) = crate::geometry::Rect::enclosing(edges.iter().map(|e| e.0))
        .and_then(|b| Grid::new(&edges, &b))
    else {
        return vec![main];
    };
    let overlap = join_overlap.clamp(0.0, 10.0) * 10.0;
    let plain = split(main.clone(), &grid, &edges, overlap, Trim::Plain);
    if plain.len() == 1 {
        // No arm leaves the one stroke found, yet strokes may meet at sharp inward corners
        // (N, M, W): cut there first and give each stroke its own column (#526).
        return notch_columns(contours, &edges, overlap).unwrap_or(plain);
    }
    // Arms whose width spikes across a junction (A's left leg past its crossbar) swallow the
    // part beyond it, which then never becomes a column of its own; the split with those
    // spikes capped ([`capped`]) is kept when it sews more of the region (A), the plain one
    // otherwise (y, whose tail's spike is its own stroke); so is the capped split whose arms
    // also run straight past a junction ([`unkinked`]: A's left leg at 25 mm) (#526).
    let mut pick = plain;
    let mut best = covered(&grid, &pick);
    for trim in [Trim::Capped, Trim::Straightened] {
        let other = split(main.clone(), &grid, &edges, overlap, trim);
        let share = covered(&grid, &other);
        if other.len() > 1 && share > best + 1e-9 {
            (pick, best) = (other, share);
        }
    }
    let reached = reach_free_ends(pick, &edges);
    reach_into_joins(reached, &edges, overlap)
}

/// An inward corner of the outline turning at least this much (degrees) is a notch where two
/// strokes meet (N's, M's and W's; an L's 90° inside corner is not one) (#526).
const NOTCH_TURN: f64 = 100.0;
/// Most pieces a region is cut into at its notches.
const MAX_NOTCH_PIECES: usize = 8;

/// The indices of the notches of the closed polygon `pts`: its inward corners turning at least
/// [`NOTCH_TURN`].
fn notches(pts: &[Vec2]) -> Vec<usize> {
    let n = pts.len();
    if n < 4 {
        return Vec::new();
    }
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    (0..n)
        .filter(|&i| {
            let (d1, d2) = (pts[i] - pts[(i + n - 1) % n], pts[(i + 1) % n] - pts[i]);
            if d1.length() < 1e-9 || d2.length() < 1e-9 {
                return false;
            }
            let cross = d1.x * d2.y - d1.y * d2.x;
            let turn = cross.datan2(d1.x * d2.x + d1.y * d2.y).to_degrees();
            // Inward: turning against the polygon's own winding.
            turn * area < 0.0 && turn.abs() >= NOTCH_TURN
        })
        .collect()
}

/// The closed polygon `pts` cut at its notch `i`: one of the notch's two edges carried straight
/// on across the region to the far outline (the more upright one, else the one falling to the
/// right, `\`, as PE-DESIGN's text strokes are cut), and the two pieces with the chord.
#[allow(clippy::type_complexity)]
fn cut_at(pts: &[Vec2], i: usize) -> Option<(Vec<Vec2>, Vec<Vec2>, [Vec2; 2])> {
    let n = pts.len();
    let v = pts[i];
    let unit = |d: Vec2| {
        let l = d.length();
        (l > 1e-9).then(|| d * (1.0 / l))
    };
    let on_in = unit(v - pts[(i + n - 1) % n])?;
    let on_out = unit(v - pts[(i + 1) % n])?;
    let upright = |d: Vec2| d.y.abs();
    let falls_right = |d: Vec2| d.x * d.y > 0.0;
    let mut dirs = [on_in, on_out];
    dirs.sort_by(|a, b| {
        upright(*b)
            .total_cmp(&upright(*a))
            .then(falls_right(*b).cmp(&falls_right(*a)))
    });
    if (upright(dirs[0]) - upright(dirs[1])).abs() < 0.1 && falls_right(dirs[1]) {
        dirs.swap(0, 1);
    }
    for d in dirs {
        // The nearest outline edge the ray meets, not the notch's own two edges.
        let mut best: Option<(f64, usize)> = None;
        for j in 0..n {
            if j == i || (j + 1) % n == i {
                continue;
            }
            let (a, b) = (pts[j], pts[(j + 1) % n]);
            let e = b - a;
            let den = d.x * e.y - d.y * e.x;
            if den.abs() < 1e-12 {
                continue;
            }
            let w = a - v;
            let t = (w.x * e.y - w.y * e.x) / den;
            let u = (w.x * d.y - w.y * d.x) / den;
            if t > 1e-6 && (0.0..=1.0).contains(&u) && best.is_none_or(|(bt, _)| t < bt) {
                best = Some((t, j));
            }
        }
        let Some((t, j)) = best else { continue };
        let q = v + d * t;
        // v, pts[i + 1 ..= j], q  and  q, pts[j + 1 ..= i].
        let mut one = vec![v];
        let mut k = (i + 1) % n;
        loop {
            one.push(pts[k]);
            if k == j {
                break;
            }
            k = (k + 1) % n;
        }
        one.push(q);
        let mut two = vec![q];
        let mut k = (j + 1) % n;
        loop {
            two.push(pts[k]);
            if k == i {
                break;
            }
            k = (k + 1) % n;
        }
        if one.len() >= 3 && two.len() >= 3 {
            return Some((one, two, [v, q]));
        }
    }
    None
}

/// The closed polygon `pts` cut at its notches, piece by piece, into at most
/// [`MAX_NOTCH_PIECES`] pieces, with the chords cut.
fn notch_pieces(pts: Vec<Vec2>) -> (Vec<Vec<Vec2>>, Vec<[Vec2; 2]>) {
    let (mut done, mut chords) = (Vec::new(), Vec::new());
    let mut todo = vec![pts];
    while let Some(piece) = todo.pop() {
        if done.len() + todo.len() + 1 >= MAX_NOTCH_PIECES {
            done.push(piece);
            continue;
        }
        match notches(&piece).into_iter().find_map(|i| cut_at(&piece, i)) {
            Some((one, two, chord)) => {
                chords.push(chord);
                todo.push(one);
                todo.push(two);
            }
            None => done.push(piece),
        }
    }
    (done, chords)
}

/// [`branches`] for a region whose one stroke meets others at notches (N, M, W): its outline
/// cut there first ([`notch_pieces`]), a column [`derive`]d for each piece, each end near a
/// cut carried across it `overlap` (0.1 mm) and half its width into the next piece, then the
/// free ends run on to the outline. The column derived through the whole letter fans across
/// its acute corners (W at 25 mm: rendered IoU 0.70, gaps up to 9 mm) (#526). `None` when the
/// region has holes, has no notch, or a piece gives no column.
fn notch_columns(
    contours: &[Contour],
    edges: &[(Vec2, Vec2)],
    overlap: f64,
) -> Option<Vec<Skeleton>> {
    let closed: Vec<&Contour> = contours.iter().filter(|c| c.closed).collect();
    let [outline] = closed.as_slice() else {
        return None;
    };
    let mut pts = outline.outline_points();
    pts.dedup_by(|a, b| a.distance(*b) < 1e-6);
    if pts.len() > 2 && pts[0].distance(pts[pts.len() - 1]) < 1e-6 {
        pts.pop();
    }
    let (pieces, chords) = notch_pieces(pts);
    if pieces.len() < 2 {
        return None;
    }
    let mut columns = Vec::with_capacity(pieces.len());
    for piece in pieces {
        columns.push(derive(&[Contour::new(piece, true)])?);
    }
    for s in &mut columns {
        for last in [false, true] {
            let Some((end, w, dir)) = end_of(s, last) else {
                continue;
            };
            let half = (w * 5.0).max(1.0);
            // The cut this end faces, within three half widths along its own direction.
            let across = chords
                .iter()
                .filter_map(|&[a, b]| {
                    let e = b - a;
                    let den = dir.x * e.y - dir.y * e.x;
                    if den.abs() < 1e-12 {
                        return None;
                    }
                    let q = a - end;
                    let t = (q.x * e.y - q.y * e.x) / den;
                    let u = (q.x * dir.y - q.y * dir.x) / den;
                    (t > 0.0 && t <= 3.0 * half && (-0.1..=1.1).contains(&u)).then_some(t)
                })
                .min_by(f64::total_cmp);
            let Some(t) = across else { continue };
            let tip = end + dir * (t + overlap + half);
            if last {
                s.points.push(tip);
                s.widths.push(w);
            } else {
                s.points.insert(0, tip);
                s.widths.insert(0, w);
            }
        }
    }
    let reached = reach_free_ends(columns, edges);
    Some(reach_into_joins(reached, edges, overlap))
}

/// How [`split`] trims its arms (#526).
#[derive(Clone, Copy, PartialEq)]
enum Trim {
    /// As derived.
    Plain,
    /// Width spikes across a junction capped ([`capped`]).
    Capped,
    /// Capped, and without points where the centre line turns back ([`unkinked`]).
    Straightened,
}

/// The columns of [`branches`] before their free ends reach the outline, the arms trimmed as
/// `trim` says; `main` alone when no arm leaves it.
fn split(
    main: Skeleton,
    grid: &Grid,
    edges: &[(Vec2, Vec2)],
    overlap: f64,
    trim: Trim,
) -> Vec<Skeleton> {
    let first = main.clone();
    let arms = arms_from(main, grid, edges, overlap, false, trim);
    if arms.len() == 1 {
        return vec![first];
    }
    // A main stroke that bends sharply through a junction (a K's leg into its stem, an R's leg
    // into its bowl) fans there and leaves a gap in the stroke it turns off (#526). Strokes
    // between the columns' free ends that run straight are tried as the main stroke instead,
    // and the split that covers the region best is kept.
    // A main column that narrows to nothing at a junction (the V through a Y's arms) leaves
    // the crotch unsewn: strokes along the ridges between the free ends (a Y's stem and one
    // arm) are tried too.
    let bends = sharpest_bend(&arms[0]) > STRAIGHT_TURN;
    let narrows = collapses(&arms[0]);
    if !bends && !narrows {
        return arms;
    }
    let mut candidates = Vec::new();
    if bends {
        candidates.extend(straight_mains(&arms, grid, edges));
    }
    if narrows {
        candidates.extend(ridge_mains(&arms, grid, edges));
    }
    let mut best = (covered(grid, &arms), arms);
    for main in candidates {
        let arms = arms_from(main, grid, edges, overlap, true, trim);
        let share = covered(grid, &arms);
        if share > best.0 + 1e-9 {
            best = (share, arms);
        }
    }
    best.1
}

/// How far inside the outline a column's free end stops (0.1 mm): its end rung's centre stays
/// inside the region, so the rung is clipped to the outline ([`rungs_within`]).
const END_INSET: f64 = 1.0;
/// A free end runs on to the outline only when it is this many half widths away or nearer.
const END_REACH: f64 = 3.0;

/// How far inside column `j`'s satin `p` lies (0.1 mm; negative outside it).
fn inside(arms: &[Skeleton], j: usize, p: Vec2) -> f64 {
    let s = &arms[j];
    let mut best = f64::NEG_INFINITY;
    for (k, w) in s.points.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let d = b - a;
        let len2 = d.x * d.x + d.y * d.y;
        let t = if len2 > 0.0 {
            (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let wa = s.widths.get(k).copied().unwrap_or(0.0);
        let wb = s.widths.get(k + 1).copied().unwrap_or(wa);
        best = best.max((wa + (wb - wa) * t) * 5.0 - p.distance(a + d * t));
    }
    best
}

/// `s`'s width (mm) at its centre line's nearest point to `p`.
fn width_at(s: &Skeleton, p: Vec2) -> f64 {
    let mut best = (f64::INFINITY, 0.0);
    for (k, w) in s.points.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let d = b - a;
        let len2 = d.x * d.x + d.y * d.y;
        let t = if len2 > 0.0 {
            (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let wa = s.widths.get(k).copied().unwrap_or(0.0);
        let wb = s.widths.get(k + 1).copied().unwrap_or(wa);
        let dist = p.distance(a + d * t);
        if dist < best.0 {
            best = (dist, wa + (wb - wa) * t);
        }
    }
    best.1
}

/// An end of `s` (`last` or first): the end point, its width (mm) and its outward direction
/// over its last half width or more; `None` for a degenerate column.
fn end_of(s: &Skeleton, last: bool) -> Option<(Vec2, f64, Vec2)> {
    let n = s.points.len();
    if n < 2 {
        return None;
    }
    let (end, w) = if last {
        (s.points[n - 1], s.widths[n - 1])
    } else {
        (s.points[0], s.widths[0])
    };
    let half = (w * 5.0).max(1.0);
    let order: Vec<usize> = if last {
        (0..n - 1).rev().collect()
    } else {
        (1..n).collect()
    };
    let &from = order
        .iter()
        .find(|&&k| s.points[k].distance(end) >= half)
        .or(order.last())?;
    let d = end - s.points[from];
    let len = d.length();
    (len > 1e-9).then(|| (end, w, d * (1.0 / len)))
}

/// A column end that joins another column runs on until both its edges, not only its centre,
/// lie `overlap` (0.1 mm) inside that column: an end square to its own column meeting another
/// on a curve or at a slant (a ring's quarter joining its thinner main) was only centre-deep
/// and left the outline beside the join unsewn (#526).
/// It runs on up to one width and half the joined column's, staying [`END_INSET`] inside the
/// outline; where both edges cannot get in (the other column is narrower), to where its centre
/// lies deepest in it.
fn reach_into_joins(
    mut arms: Vec<Skeleton>,
    edges: &[(Vec2, Vec2)],
    overlap: f64,
) -> Vec<Skeleton> {
    for i in 0..arms.len() {
        for last in [false, true] {
            let Some((end, w, dir)) = end_of(&arms[i], last) else {
                continue;
            };
            // The column this end joins: the one it lies deepest in.
            let Some((j, depth)) = (0..arms.len())
                .filter(|&j| j != i)
                .map(|j| (j, inside(&arms, j, end)))
                .max_by(|a, b| a.1.total_cmp(&b.1))
            else {
                continue;
            };
            if depth < 0.0 {
                continue;
            }
            let normal = v2(-dir.y, dir.x) * (w * 5.0);
            let rails_in =
                |p: Vec2| inside(&arms, j, p + normal).min(inside(&arms, j, p - normal)) >= overlap;
            if rails_in(end) {
                continue;
            }
            let room = ray(end, dir, edges).map_or(0.0, |t| t - END_INSET);
            // At most half the joined column's width there, so two satins are not stacked
            // deeper than the join needs (#957).
            let most = (w * 10.0).min(room).min(width_at(&arms[j], end) * 5.0);
            // The first run that brings both edges in; failing that (a column narrower than
            // this end, a ring's thin quarter), the run that takes the centre deepest in.
            let step = 0.5;
            let mut run = step;
            let mut found = None;
            let mut deepest = (depth, 0.0);
            while run <= most {
                let p = end + dir * run;
                if rails_in(p) {
                    found = Some(run);
                    break;
                }
                let d = inside(&arms, j, p);
                if d > deepest.0 {
                    deepest = (d, run);
                }
                run += step;
            }
            let Some(run) =
                found.or((deepest.1 > 0.0 && deepest.0 >= overlap).then_some(deepest.1))
            else {
                continue;
            };
            let s = &mut arms[i];
            if last {
                s.points.push(end + dir * run);
                s.widths.push(w);
            } else {
                s.points.insert(0, end + dir * run);
                s.widths.insert(0, w);
            }
        }
    }
    arms
}

/// The columns of [`branches`] with each free end (one not inside another column's satin)
/// run on along its own direction to [`END_INSET`] inside the outline (#526). The columns'
/// ends are placed short of the outline (most of their clearance, or a sixth of the width),
/// which left a flat end unsewn (K's stem top) and a slanted end's corners (K's and X's feet);
/// at the outline the end rungs are clipped by the end edge and reach into its corners. Join
/// ends keep their overlap.
fn reach_free_ends(mut arms: Vec<Skeleton>, edges: &[(Vec2, Vec2)]) -> Vec<Skeleton> {
    let ends: Vec<(usize, bool)> = (0..arms.len())
        .flat_map(|i| [(i, false), (i, true)])
        .filter(|&(i, last)| {
            let s = &arms[i];
            let Some(&p) = (if last {
                s.points.last()
            } else {
                s.points.first()
            }) else {
                return false;
            };
            s.points.len() >= 2 && (0..arms.len()).all(|j| j == i || inside(&arms, j, p) < 0.0)
        })
        .collect();
    for (i, last) in ends {
        let Some((end, w, dir)) = end_of(&arms[i], last) else {
            continue;
        };
        let half = (w * 5.0).max(1.0);
        let s = &mut arms[i];
        let Some(t) = ray(end, dir, edges) else {
            continue;
        };
        if t <= END_INSET + 0.5 || t > END_REACH * half {
            continue;
        }
        let tip = end + dir * (t - END_INSET);
        if last {
            s.points.push(tip);
            s.widths.push(w);
        } else {
            s.points.insert(0, tip);
            s.widths.insert(0, w);
        }
    }
    arms
}

/// A main stroke turning more than this (degrees) within [`BEND_WINDOW`] widths bends
/// through a junction (#526).
const STRAIGHT_TURN: f64 = 40.0;
/// The stretch of a stroke, in column widths, over which its turn is summed.
const BEND_WINDOW: f64 = 1.5;
/// Most straight strokes tried as a main stroke.
const MAX_MAINS: usize = 4;
/// A main column narrowing inside to under this share of its median width is measured across
/// a junction's mouth (#526).
const COLLAPSE: f64 = 0.4;
/// A chord across a stroke up to this many times the distance to its nearest edge is taken as
/// the stroke's width (a stroke widening or tapering); longer ones run out through a gap.
const WIDE_CHORD: f64 = 6.0;
/// The same for a main stroke tried along a ridge, which passes junctions (#526).
const NARROW_CHORD: f64 = 2.5;

/// The largest turn (degrees) of `s`'s centre line within [`BEND_WINDOW`] column widths: a
/// sharp bend there, a gentle curve spread along it.
fn sharpest_bend(s: &Skeleton) -> f64 {
    let p = &s.points;
    if p.len() < 3 {
        return 0.0;
    }
    let width = s.widths.iter().copied().fold(0.0, f64::max) * 10.0;
    let window = (BEND_WINDOW * width).max(10.0);
    let at: Vec<f64> = std::iter::once(0.0)
        .chain(p.windows(2).scan(0.0, |d, w| {
            *d += w[0].distance(w[1]);
            Some(*d)
        }))
        .collect();
    // The ends reach out into the stroke's corners and may hook there: turns within a column
    // width of either end do not count.
    let total = at[at.len() - 1];
    let turn: Vec<f64> = (1..p.len() - 1)
        .map(|i| {
            if at[i] < width || at[i] > total - width {
                return 0.0;
            }
            let (a, b) = (p[i] - p[i - 1], p[i + 1] - p[i]);
            (a.x * b.y - a.y * b.x).datan2(a.x * b.x + a.y * b.y)
        })
        .collect();
    let mut most: f64 = 0.0;
    for i in 0..turn.len() {
        let mut sum: f64 = 0.0;
        for j in i..turn.len() {
            if at[j + 1] - at[i + 1] > window {
                break;
            }
            sum += turn[j];
            most = most.max(sum.abs());
        }
    }
    most.to_degrees()
}

/// The share of the region's inner cells (a cell or more from the outline) that the satin of
/// `arms` sews over: inside a quadrilateral between two of a column's crossings 1 mm apart, as
/// the satin lays them (a column fanning round a sharp bend leaves gaps between its fans).
fn covered(grid: &Grid, arms: &[Skeleton]) -> f64 {
    let mut hit = vec![false; grid.clearance.len()];
    for sk in arms {
        let Some(c) = Centre::new(sk) else {
            continue;
        };
        for w in c.rungs(10.0, 0.0).windows(2) {
            let quad = [w[0][0], w[1][0], w[1][1], w[0][1]];
            let Some(b) = crate::geometry::Rect::enclosing(quad.iter().copied()) else {
                continue;
            };
            let poly = [Contour::new(quad.to_vec(), true)];
            let (i0, j0) = (
                ((b.min_x - grid.origin.x) / grid.step).floor().max(0.0) as usize,
                ((b.min_y - grid.origin.y) / grid.step).floor().max(0.0) as usize,
            );
            let (i1, j1) = (
                (((b.max_x - grid.origin.x) / grid.step).ceil().max(0.0) as usize).min(grid.nx),
                (((b.max_y - grid.origin.y) / grid.step).ceil().max(0.0) as usize).min(grid.ny),
            );
            for j in j0..j1 {
                for i in i0..i1 {
                    let k = j * grid.nx + i;
                    if !hit[k] && crate::geometry::contains(grid.centre(k), &poly) {
                        hit[k] = true;
                    }
                }
            }
        }
    }
    let inside = (0..grid.clearance.len()).filter(|&k| grid.clearance[k] >= grid.step);
    let (n, h) = inside.fold((0usize, 0usize), |(n, h), k| {
        (n + 1, h + usize::from(hit[k]))
    });
    if n == 0 { 0.0 } else { h as f64 / n as f64 }
}

/// A straight main stroke's column along `line` (0.1 mm) as it lies (#526): at each point as
/// wide as twice the nearer side of the chord across the region, so where the chord runs on
/// into a junction (a K's legs leave its stem) the column keeps the stroke's own width and
/// line instead of bending and widening into the junction. The ends, reached out towards the
/// outline, are as wide as next door.
fn along_line(line: &[Vec2], edges: &[(Vec2, Vec2)]) -> Option<Skeleton> {
    let n = line.len();
    if n < 2 {
        return None;
    }
    let mut widths: Vec<f64> = (0..n)
        .map(|i| {
            let t = line[(i + 1).min(n - 1)] - line[i.saturating_sub(1)];
            let p = line[i];
            let near = edges
                .iter()
                .map(|&(a, c)| segment_distance(p, a, c))
                .fold(f64::INFINITY, f64::min);
            if t.length() <= 1e-9 {
                return 2.0 * near;
            }
            let t = t * (1.0 / t.length());
            let left = v2(t.y, -t.x);
            match (ray(p, left, edges), ray(p, left * -1.0, edges)) {
                (Some(l), Some(r)) => 2.0 * l.min(r),
                _ => 2.0 * near,
            }
        })
        .collect();
    if n > 2 {
        widths[0] = widths[1];
        widths[n - 1] = widths[n - 2];
    }
    let s = Skeleton {
        points: line.to_vec(),
        widths: widths
            .into_iter()
            .map(|w| (w / 10.0).clamp(0.0, MAX_WIDTH))
            .collect(),
    };
    s.validate().is_ok().then_some(s)
}

/// The inner cell nearest `p` (at least a cell from the outline).
fn cell_near(grid: &Grid, p: Vec2) -> Option<usize> {
    (0..grid.clearance.len())
        .filter(|&k| grid.clearance[k] >= grid.step)
        .min_by(|&a, &b| {
            grid.centre(a)
                .distance(p)
                .total_cmp(&grid.centre(b).distance(p))
        })
}

/// Straight strokes between the free ends of `arms` (the main stroke's two ends and each
/// arm's tip) that run inside the region, longest first, at most [`MAX_MAINS`].
fn straight_mains(arms: &[Skeleton], grid: &Grid, edges: &[(Vec2, Vec2)]) -> Vec<Skeleton> {
    let mut found: Vec<(f64, Skeleton)> = tip_ridges(arms, grid)
        .iter()
        .filter_map(|ridge| straight_stroke(ridge, grid, edges))
        .map(|sk| (sk.points[0].distance(sk.points[sk.points.len() - 1]), sk))
        .collect();
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.into_iter().take(MAX_MAINS).map(|f| f.1).collect()
}

/// Curved strokes along the ridges between the free ends of `arms`, longest first, at most
/// [`MAX_MAINS`] (#526): a main stroke whose column collapses at a junction (the V through a
/// Y's arms) is tried again along the stem and one arm.
fn ridge_mains(arms: &[Skeleton], grid: &Grid, edges: &[(Vec2, Vec2)]) -> Vec<Skeleton> {
    let mut found: Vec<(f64, Skeleton)> = tip_ridges(arms, grid)
        .into_iter()
        .filter_map(|ridge| {
            let mut pts: Vec<Vec2> = ridge.iter().map(|&k| grid.centre(k)).collect();
            // Both ends reach out most of their clearance, like derive's.
            let last = pts.len() - 1;
            let reach = |end: usize, inner: usize, k: usize| {
                let d = pts[end] - pts[inner];
                (d.length() > 1e-9)
                    .then(|| pts[end] + d * (0.8 * grid.clearance[k].max(0.0) / d.length()))
            };
            let head = reach(0, 3.min(last), ridge[0]);
            let tail = reach(last, last.saturating_sub(3), ridge[last]);
            if let Some(p) = head {
                pts.insert(0, p);
            }
            if let Some(p) = tail {
                pts.push(p);
            }
            let len: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
            // Through a junction its chord would run into the other arm's mouth and pull the
            // column over it (a y's stroke past its short arm): only a chord about as wide as
            // the stroke there counts (#526).
            line_to_skeleton(pts, true, true, edges, grid.step, NARROW_CHORD).map(|sk| (len, sk))
        })
        .collect();
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.into_iter().take(MAX_MAINS).map(|f| f.1).collect()
}

/// Whether the column of `s` narrows inside to under [`COLLAPSE`] of its median width (#526):
/// a main stroke turning through a junction is measured across the junction's mouth there.
fn collapses(s: &Skeleton) -> bool {
    let n = s.widths.len();
    if n < 3 {
        return false;
    }
    let mut w = s.widths.clone();
    w.sort_by(f64::total_cmp);
    let median = w[n / 2];
    s.widths[1..n - 1].iter().any(|&x| x < COLLAPSE * median)
}

/// The grid ridges (cells, from one end to the other) between each pair of the free ends of
/// `arms`: the main stroke's two ends and each arm's tip.
fn tip_ridges(arms: &[Skeleton], grid: &Grid) -> Vec<Vec<usize>> {
    let mut tips: Vec<Vec2> = Vec::new();
    if let (Some(&a), Some(&b)) = (arms[0].points.first(), arms[0].points.last()) {
        tips.extend([a, b]);
    }
    tips.extend(arms[1..].iter().filter_map(|s| s.points.last().copied()));
    let cells: Vec<usize> = tips.iter().filter_map(|&p| cell_near(grid, p)).collect();
    let widest = grid.clearance.iter().copied().fold(grid.step, f64::max);
    let mut found: Vec<Vec<usize>> = Vec::new();
    for (i, &from) in cells.iter().enumerate() {
        let (_, prev) = grid.paths(from, |n, step| {
            let k = widest / (grid.clearance[n].max(0.0) + grid.step * 0.5);
            step * k * k
        });
        for &to in &cells[i + 1..] {
            // The ridge between the two ends, as derive follows a stroke.
            let mut ridge = vec![to];
            while let Some(&k) = ridge.last() {
                if k == from || ridge.len() > grid.clearance.len() {
                    break;
                }
                match prev[k] {
                    usize::MAX => break,
                    p => ridge.push(p),
                }
            }
            if ridge.last() != Some(&from) || ridge.len() < 8 {
                continue;
            }
            found.push(ridge);
        }
    }
    found
}

/// A straight line (a point on it and its unit direction) through `p` if at least 60 % of the
/// points lie within `near` of it (fitted twice: then to those points only); `None` otherwise.
fn straight_fit(p: &[Vec2], near: f64) -> Option<(Vec2, Vec2)> {
    let fit = |p: &[Vec2]| -> Option<(Vec2, Vec2)> {
        if p.len() < 3 {
            return None;
        }
        let c = p.iter().fold(v2(0.0, 0.0), |a, &q| a + q) * (1.0 / p.len() as f64);
        let (mut xx, mut xy, mut yy) = (0.0, 0.0, 0.0);
        for q in p {
            let d = *q - c;
            xx += d.x * d.x;
            xy += d.x * d.y;
            yy += d.y * d.y;
        }
        let angle = 0.5 * (2.0 * xy).datan2(xx - yy);
        Some((c, v2(angle.dcos(), angle.dsin())))
    };
    let off = |q: Vec2, (c, d): (Vec2, Vec2)| {
        let e = q - c;
        (e.x * d.y - e.y * d.x).abs()
    };
    let first = fit(p)?;
    let close: Vec<Vec2> = p
        .iter()
        .copied()
        .filter(|&q| off(q, first) <= near)
        .collect();
    if close.len() * 10 < p.len() * 6 {
        return None;
    }
    let line = fit(&close)?;
    (p.iter().filter(|&&q| off(q, line) <= near).count() * 10 >= p.len() * 6).then_some(line)
}

/// The straight stroke along the grid `ridge` (cells), if the ridge runs straight: a line
/// fitted to its middle 70 % (the ends hook into the stroke's corners), on which at least 60 %
/// of those cells lie within a third of the stroke's half width (a junction pulls the ridge
/// aside for a while), from end to end of the ridge and on to the outline, a sixth of the
/// stroke's width short of it. `None` for a bent ridge or a line that leaves the region.
fn straight_stroke(ridge: &[usize], grid: &Grid, edges: &[(Vec2, Vec2)]) -> Option<Skeleton> {
    let pts: Vec<Vec2> = ridge.iter().map(|&k| grid.centre(k)).collect();
    let m = pts.len();
    let inner = &pts[m * 15 / 100..m - m * 15 / 100];
    let mut half: Vec<f64> = ridge.iter().map(|&k| grid.clearance[k]).collect();
    half.sort_by(f64::total_cmp);
    let median = half[half.len() / 2];
    let (c, d) = straight_fit(inner, (median / 3.0).max(grid.step))?;
    let along = |q: Vec2| (q - c).x * d.x + (q - c).y * d.y;
    let (lo, hi) = pts
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &q| {
            (lo.min(along(q)), hi.max(along(q)))
        });
    let (a, b) = (c + d * lo, c + d * hi);
    let n = ((a.distance(b) / grid.step).ceil() as usize).max(2);
    if (0..=n).any(|k| grid.clearance_at(a.lerp(b, k as f64 / n as f64)) < grid.step) {
        return None;
    }
    let inset = median / 3.0;
    let head = a - d * ray(a, d * -1.0, edges).map_or(0.0, |e| (e - inset).max(0.0));
    let tail = b + d * ray(b, d, edges).map_or(0.0, |e| (e - inset).max(0.0));
    let n = ((head.distance(tail) / (4.0 * grid.step)).ceil() as usize).max(2);
    let line: Vec<Vec2> = (0..=n)
        .map(|k| head.lerp(tail, k as f64 / n as f64))
        .collect();
    along_line(&line, edges)
}

/// The columns of a branching region from its `main` stroke (see [`branches`]): the main
/// column measured again along its whole length, then one column per arm left uncovered,
/// each joining a column `overlap` (0.1 mm) deep. A `straight` main stroke keeps its line
/// ([`along_line`]).
fn arms_from(
    main: Skeleton,
    grid: &Grid,
    edges: &[(Vec2, Vec2)],
    overlap: f64,
    straight: bool,
    trim: Trim,
) -> Vec<Skeleton> {
    let cells = grid.clearance.len();
    let first = main.clone();
    let mut arms = vec![main];
    // The main column's widths are measured again along its whole length: derive simplifies a
    // straight stroke to its two ends, whose width is then the nearest edge's (the end of the
    // bar across a plus). The arms join this column; without arms derive's own is kept.
    if straight {
        let mut dense = vec![first.points[0]];
        for w in first.points.windows(2) {
            let n = ((w[0].distance(w[1]) / (4.0 * grid.step)).ceil() as usize).max(1);
            dense.extend((1..=n).map(|i| w[0].lerp(w[1], i as f64 / n as f64)));
        }
        if let Some(main) = along_line(&dense, edges) {
            arms[0] = main;
        }
    } else {
        let mut dense = vec![first.points[0]];
        for w in first.points.windows(2) {
            let n = ((w[0].distance(w[1]) / (4.0 * grid.step)).ceil() as usize).max(1);
            dense.extend((1..=n).map(|i| w[0].lerp(w[1], i as f64 / n as f64)));
        }
        if let Some(mut main) = measure(&dense, true, true, edges, grid.step, WIDE_CHORD) {
            // Where the chord runs on into another arm (the middle of a star), the column is
            // as wide as the clearance there allows, not the chord.
            for (q, w) in main.points.iter().zip(main.widths.iter_mut()) {
                let near = edges
                    .iter()
                    .map(|&(a, c)| segment_distance(*q, a, c))
                    .fold(f64::INFINITY, f64::min);
                *w = w.min(2.6 * near / 10.0);
            }
            if main.validate().is_ok() {
                arms[0] = main;
            }
        }
    }
    // Cells already tried as an arm that gave no column.
    let mut tried = vec![false; cells];
    while arms.len() < MAX_BRANCHES {
        let depth: Vec<f64> = (0..cells)
            .map(|k| {
                if grid.clearance[k] < 0.0 {
                    f64::NEG_INFINITY
                } else {
                    depth_in(grid.centre(k), &arms)
                }
            })
            .collect();
        let open =
            |k: usize| grid.clearance[k] >= grid.step && depth[k] < -0.75 * grid.step && !tried[k];
        // The uncovered parts, each with its farthest cell from the columns.
        let mut seen = vec![false; cells];
        let mut best: Option<(f64, usize, Vec<usize>)> = None;
        for start in 0..cells {
            if seen[start] || !open(start) {
                continue;
            }
            let mut part = vec![start];
            seen[start] = true;
            let mut i = 0;
            while i < part.len() {
                let k = part[i];
                for (n, _) in grid.neighbours(k) {
                    if !seen[n] && open(n) {
                        seen[n] = true;
                        part.push(n);
                    }
                }
                i += 1;
            }
            let (corner, reach) = part
                .iter()
                .map(|&k| (k, -depth[k]))
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .expect("not empty");
            let wide = part.iter().map(|&k| grid.clearance[k]).fold(0.0, f64::max);
            // A flat end square to the arm (a y's tail, a Y's stem foot) has a row of cells at
            // its furthest reach, and the furthest of them is a corner: an arm from there runs
            // slanted across the stroke. There the tip is the one of those cells most in the
            // middle. A pointed or slanted end (an R's leg) keeps its furthest cell (#526).
            let front: Vec<usize> = part
                .iter()
                .copied()
                .filter(|&k| -depth[k] >= reach - 1.5 * grid.step)
                .collect();
            let flat = front.len() as f64 * grid.step >= wide;
            let tip = if flat {
                front
                    .iter()
                    .copied()
                    .max_by(|&a, &b| {
                        grid.clearance[a]
                            .total_cmp(&grid.clearance[b])
                            .then((-depth[a]).total_cmp(&-depth[b]))
                    })
                    .unwrap_or(corner)
            } else {
                corner
            };
            // An arm reaches out further than twice its half width.
            if reach >= (2.0 * wide).max(3.0 * grid.step)
                && best.as_ref().is_none_or(|b| reach > b.0)
            {
                best = Some((reach, tip, part));
            }
        }
        let Some((_, tip, part)) = best else {
            break;
        };
        // Along the arm's ridge from its tip into the columns, as derive follows its stroke.
        let widest = part
            .iter()
            .map(|&k| grid.clearance[k])
            .fold(grid.step, f64::max);
        let (cost, prev) = grid.paths(tip, |n, step| {
            let k = widest / (grid.clearance[n].max(0.0) + grid.step * 0.5);
            step * k * k
        });
        let target = (0..cells)
            .filter(|&k| depth[k] >= overlap && cost[k].is_finite())
            .min_by(|&a, &b| cost[a].total_cmp(&cost[b]));
        let walk = |t: usize| {
            let mut line: Vec<usize> = Vec::new();
            let mut k = t;
            while k != usize::MAX && line.len() <= cells {
                line.push(k);
                if k == tip {
                    break;
                }
                k = prev[k];
            }
            line
        };
        let mut line: Vec<usize> = target.map(walk).unwrap_or_default();
        // A bridge (A's and H's crossbar): past its tip, away from the column it joins, the
        // part runs into another column before it leaves the region, so it has no free end.
        // It becomes one column from the one to the other, joining both, instead of an arm
        // from its middle that left the other half unsewn (#526).
        let bridge = target.and_then(|t1| {
            let away = grid.centre(tip) - grid.centre(t1);
            let len = away.length();
            if len < 1e-9 {
                return None;
            }
            let d = away * (1.0 / len);
            let steps = (BRIDGE_REACH * widest / grid.step).ceil() as usize;
            let hits = (1..=steps).find_map(|i| {
                let k = grid.cell_at(grid.centre(tip) + d * (grid.step * i as f64))?;
                if grid.clearance[k] < 0.0 {
                    Some(false)
                } else if depth[k] >= 0.0 {
                    Some(true)
                } else {
                    None
                }
            });
            if hits != Some(true) {
                return None;
            }
            (0..cells)
                .filter(|&k| {
                    depth[k] >= overlap && cost[k].is_finite() && {
                        let v = grid.centre(k) - grid.centre(tip);
                        v.x * d.x + v.y * d.y > 0.0
                    }
                })
                .min_by(|&a, &b| cost[a].total_cmp(&cost[b]))
        });
        if let Some(t2) = bridge {
            let other = walk(t2);
            if other.last() == Some(&tip) && line.last() == Some(&tip) {
                // From the first join through the tip to the second.
                line.pop();
                line.extend(other.into_iter().rev());
                let pts: Vec<Vec2> = line.iter().map(|&k| grid.centre(k)).collect();
                match line_to_skeleton(pts, false, false, edges, grid.step, WIDE_CHORD).map(|sk| {
                    if trim == Trim::Plain {
                        sk
                    } else {
                        capped(sk, edges)
                    }
                }) {
                    Some(sk) => {
                        arms.push(sk);
                        line.iter().for_each(|&k| tried[k] = true);
                    }
                    None => part.iter().for_each(|&k| tried[k] = true),
                }
                continue;
            }
        }
        // From the join to the tip, the tip reaching out most of its clearance like derive's.
        let mut pts: Vec<Vec2> = line.iter().map(|&k| grid.centre(k)).collect();
        let last = pts.len().saturating_sub(1);
        let inner = last.saturating_sub(3);
        let reached = last > inner && grid.clearance[tip] > grid.step && {
            let d = pts[last] - pts[inner];
            let ok = d.length() > 1e-9;
            if ok {
                let extra = pts[last] + d * (0.8 * grid.clearance[tip] / d.length());
                pts.push(extra);
            }
            ok
        };
        let arm = (line.last() == Some(&tip))
            .then(|| line_to_skeleton(pts, false, reached, edges, grid.step, WIDE_CHORD))
            .flatten()
            .map(|sk| match trim {
                Trim::Plain => sk,
                Trim::Capped => capped(sk, edges),
                Trim::Straightened => capped(unkinked(sk), edges),
            });
        match arm {
            // The arm's own cells are never an arm again; a part next to it (two arms meeting
            // in the uncovered middle of a star) still is.
            Some(sk) => {
                arms.push(sk);
                line.iter().for_each(|&k| tried[k] = true);
            }
            None => part.iter().for_each(|&k| tried[k] = true),
        }
    }
    arms
}

/// An arm's column without width spikes: where its line crosses a junction (A's left leg past
/// the crossbar) the chord there spanned the junction (8.7 mm on a 4 mm leg), and the column
/// swallowed half the bar, which then never became a column of its own (#526). A point wider
/// than [`SPIKE`] times the arm's median width is kept within the clearance there, as the main
/// column is (2.6 times the distance to the nearest edge); the rest keep their chords (R's
/// bowl).
fn capped(mut sk: Skeleton, edges: &[(Vec2, Vec2)]) -> Skeleton {
    let mut ws = sk.widths.clone();
    ws.sort_by(f64::total_cmp);
    let Some(&median) = ws.get(ws.len() / 2) else {
        return sk;
    };
    for (q, w) in sk.points.iter().zip(sk.widths.iter_mut()) {
        if *w <= SPIKE * median {
            continue;
        }
        let near = edges
            .iter()
            .map(|&(a, c)| segment_distance(*q, a, c))
            .fold(f64::INFINITY, f64::min);
        *w = w.min(2.6 * near / 10.0);
    }
    sk
}

/// An arm's centre line without points where it turns back on itself (more than [`KINK`]
/// degrees): the ridge through a junction pulls one point into it (A's left leg at the bar,
/// 2.5 mm off the leg's middle), and the column there missed the leg's outer edge (#526). Not
/// for a bridge, whose line turns at its tip by design.
fn unkinked(mut sk: Skeleton) -> Skeleton {
    let mut k = 1;
    while k + 1 < sk.points.len() {
        let (u, v) = (
            sk.points[k] - sk.points[k - 1],
            sk.points[k + 1] - sk.points[k],
        );
        let turn = (u.x * v.y - u.y * v.x)
            .datan2(u.x * v.x + u.y * v.y)
            .abs()
            .to_degrees();
        if turn > KINK {
            sk.points.remove(k);
            sk.widths.remove(k);
            k = k.saturating_sub(1).max(1);
        } else {
            k += 1;
        }
    }
    sk
}

/// An arm's width this many times its median is a spike across a junction ([`capped`]).
const SPIKE: f64 = 2.0;
/// A centre line turning more than this (degrees) at one point turns back on itself there.
const KINK: f64 = 100.0;

/// A skeleton from a centre line on the grid (`pts`, 0.1 mm): smoothed, simplified, each point
/// moved to the middle of the chord across the region there and given its width (see
/// [`derive`]); `head` and `tail` say whether the ends were reached out towards the outline.
fn line_to_skeleton(
    mut pts: Vec<Vec2>,
    head: bool,
    tail: bool,
    edges: &[(Vec2, Vec2)],
    step: f64,
    chord_limit: f64,
) -> Option<Skeleton> {
    // Smooth the grid staircase (ends stay).
    for _ in 0..3 {
        let old = pts.clone();
        for i in 1..pts.len().saturating_sub(1) {
            pts[i] = (old[i - 1] + old[i] * 2.0 + old[i + 1]) * 0.25;
        }
    }
    let keep = simplify(&pts, step * 0.75);
    let line: Vec<Vec2> = keep.iter().map(|&i| pts[i]).collect();
    measure(&line, head, tail, edges, step, chord_limit)
}

/// The skeleton along `line` (0.1 mm): each point moved to the middle of the chord across the
/// region there and given its width; `head` and `tail` say whether the ends were reached out
/// towards the outline (their width is then no wider than next door). A chord longer than
/// `chord_limit` times the distance to the nearest edge is not across the stroke (it runs out
/// through a gap); the point keeps its place and twice that distance as its width.
fn measure(
    line: &[Vec2],
    head: bool,
    tail: bool,
    edges: &[(Vec2, Vec2)],
    step: f64,
    chord_limit: f64,
) -> Option<Skeleton> {
    if line.len() < 2 {
        return None;
    }
    let mut points = Vec::with_capacity(line.len());
    let mut widths = Vec::with_capacity(line.len());
    let last = line.len() - 1;
    let mut chords = Vec::with_capacity(line.len());
    for i in 0..line.len() {
        let t = line[(i + 1).min(last)] - line[i.saturating_sub(1)];
        let p = line[i];
        let near = edges
            .iter()
            .map(|&(a, c)| segment_distance(p, a, c))
            .fold(f64::INFINITY, f64::min);
        let chord = (t.length() > 1e-9).then(|| {
            let t = t * (1.0 / t.length());
            let left = v2(t.y, -t.x);
            (left, ray(p, left, edges), ray(p, left * -1.0, edges))
        });
        let (centre, width) = match chord {
            Some((left, Some(l), Some(r))) if l + r <= chord_limit * near + 2.0 * step => {
                (p + left * ((l - r) / 2.0), l + r)
            }
            _ => (p, 2.0 * near),
        };
        chords.push(chord);
        points.push(centre);
        widths.push(width);
    }
    // The reached-out ends are close to the outline: their chord, no wider than next door.
    for (end, next, reached) in [(0, 1, head), (last, last - 1, tail)] {
        if !reached || end == next {
            continue;
        }
        let w = match chords[end] {
            Some((left, Some(l), Some(r))) if l + r <= widths[next] * 1.1 => {
                points[end] = line[end] + left * ((l - r) / 2.0);
                (l + r).min(widths[next])
            }
            _ => widths[next],
        };
        widths[end] = w;
    }
    let widths = widths
        .into_iter()
        .map(|w| (w / 10.0).clamp(0.0, MAX_WIDTH))
        .collect();
    let s = Skeleton { points, widths };
    s.validate().is_ok().then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmath::Det;

    fn skeleton(points: &[(f64, f64)], widths: &[f64]) -> Skeleton {
        Skeleton {
            points: points.iter().map(|&(x, y)| v2(x, y)).collect(),
            widths: widths.to_vec(),
        }
    }

    #[test]
    fn stitches_are_perpendicular_to_the_centre_line() {
        // An L: right 200, then down 200; 4 mm wide.
        let s = skeleton(&[(0.0, 0.0), (200.0, 0.0), (200.0, 200.0)], &[4.0; 3]);
        assert_eq!(s.validate(), Ok(()));
        let run = satin(&s, 3.0, 0.0);
        let c = Centre::new(&s).expect("centre");
        for [l, r] in c.rungs(3.0, 0.0) {
            let mid = l.lerp(r, 0.5);
            let across = r - l;
            assert!((across.length() - 40.0).abs() < 1e-6, "full width");
            if mid.x < 170.0 {
                // First leg: centre on y = 0, stitch vertical.
                assert!(mid.y.abs() < 1e-9 && across.x.abs() < 1e-9, "{l:?} {r:?}");
            } else if mid.y > 30.0 {
                assert!((mid.x - 200.0).abs() < 1e-9 && across.y.abs() < 1e-9);
            }
        }
        // Through the bend the direction turns gradually.
        let turning = c.rungs(3.0, 0.0).iter().any(|[l, r]| {
            let d = *r - *l;
            d.x.abs() > 5.0 && d.y.abs() > 5.0
        });
        assert!(turning);
        assert!(run.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
    }

    #[test]
    fn density_and_width_are_respected() {
        let s = skeleton(&[(0.0, 0.0), (300.0, 0.0)], &[2.0, 6.0]);
        let spacing = 10.0 / 4.0; // 4 lines per mm
        let run = satin(&s, spacing, 0.0);
        // One needle point per spacing step, both ends included, plus the first rung's other
        // end.
        assert_eq!(run.len(), 122);
        // Every stitch crosses the centre line (y = 0): needle points alternate sides.
        for w in run.windows(2) {
            assert!(w[0].y * w[1].y < 0.0, "{w:?} runs along an edge");
        }
        let centres: Vec<Vec2> = c_rungs(&s, spacing)
            .iter()
            .map(|[l, r]| l.lerp(*r, 0.5))
            .collect();
        for w in centres.windows(2) {
            assert!(w[0].distance(w[1]) <= spacing + 1e-9);
        }
        // Width tapers from 2 mm to 6 mm.
        assert!((run[0].distance(run[1]) - 20.0).abs() < 1e-6);
        let last = run[run.len() - 1];
        assert!((last.x - 300.0).abs() < 1e-6 && (last.y.abs() - 30.0).abs() < 1e-6);
        // Pull compensation lengthens each stitch at both ends.
        let pulled = satin(&s, spacing, 3.0);
        assert!((pulled[0].distance(pulled[1]) - 26.0).abs() < 1e-6);
    }

    fn c_rungs(s: &Skeleton, spacing: f64) -> Vec<[Vec2; 2]> {
        Centre::new(s).expect("centre").rungs(spacing, 0.0)
    }

    #[test]
    fn degenerate_input_gives_no_nan() {
        let cases = [
            skeleton(&[], &[]),
            skeleton(&[(5.0, 5.0)], &[3.0]),
            skeleton(&[(5.0, 5.0), (5.0, 5.0), (5.0, 5.0)], &[3.0; 3]),
            skeleton(
                &[(0.0, 0.0), (f64::NAN, 1.0), (10.0, 0.0)],
                &[f64::NAN, 3.0],
            ),
            skeleton(&[(0.0, 0.0), (100.0, 0.0), (0.0, 0.0)], &[]),
            skeleton(&[(0.0, 0.0), (100.0, 0.0)], &[f64::INFINITY, -2.0]),
        ];
        for s in &cases {
            for (sp, pull) in [(3.0, 0.0), (0.0, 0.0), (f64::NAN, f64::NAN), (1e-12, -50.0)] {
                let run = satin(s, sp, pull);
                assert!(
                    run.iter().all(|p| p.x.is_finite() && p.y.is_finite()),
                    "{s:?}"
                );
            }
            if let Some(o) = outline(s) {
                assert!(o.points.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
            }
        }
        assert!(satin(&cases[0], 3.0, 0.0).is_empty());
        assert!(satin(&cases[2], 3.0, 0.0).is_empty());
        // A centre line that turns straight back still sews.
        assert!(!satin(&cases[4], 3.0, 0.0).is_empty());
    }

    #[test]
    fn validation() {
        let ok = skeleton(&[(0.0, 0.0), (100.0, 0.0)], &[3.0, 0.0]);
        assert_eq!(ok.validate(), Ok(()));
        assert_eq!(
            skeleton(&[(0.0, 0.0)], &[3.0]).validate(),
            Err(SkeletonError::TooFew)
        );
        assert_eq!(
            skeleton(&[(0.0, 0.0), (10.0, 0.0)], &[3.0]).validate(),
            Err(SkeletonError::WidthCount)
        );
        assert_eq!(
            skeleton(&[(0.0, 0.0), (10.0, 0.0)], &[3.0, 31.0]).validate(),
            Err(SkeletonError::Width(1))
        );
        assert_eq!(
            skeleton(&[(0.0, 0.0), (10.0, 0.0)], &[0.0, 0.0]).validate(),
            Err(SkeletonError::Thin)
        );
        assert_eq!(
            skeleton(&[(0.0, 0.0), (0.5, 0.0)], &[3.0, 3.0]).validate(),
            Err(SkeletonError::Short)
        );
        let o = outline(&ok).expect("outline");
        assert!(crate::geometry::contains(v2(50.0, 0.0), &[o]));
        let t = Transform::scaling(2.0, 2.0, v2(0.0, 0.0));
        assert_eq!(ok.transformed(&t).widths, vec![6.0, 0.0]);
    }

    #[test]
    fn derived_skeleton_of_a_bar_runs_down_its_middle() {
        // 100 mm × 8 mm.
        let bar = Contour::rectangle(0.0, 0.0, 1000.0, 80.0);
        let s = derive(std::slice::from_ref(&bar)).expect("skeleton");
        assert_eq!(s.validate(), Ok(()));
        assert!(s.length() > 800.0, "length {}", s.length());
        for (p, w) in s.points.iter().zip(&s.widths) {
            assert!((p.y - 40.0).abs() < 4.0, "{p:?} off the middle");
            if p.x > 100.0 && p.x < 900.0 {
                assert!((w - 8.0).abs() < 0.5, "width {w} at {p:?}");
            }
        }
        // A straight bar simplifies to a few points.
        assert!(s.points.len() <= 12, "{} points", s.points.len());
        let run = satin(&s, 3.0, 0.0);
        assert!(run.iter().all(
            |p| crate::geometry::contains(*p, std::slice::from_ref(&bar))
                || p.y.abs() < 1.0
                || (p.y - 80.0).abs() < 1.0
        ));
    }

    #[test]
    fn derived_skeleton_follows_a_bend() {
        // Half a ring: radii 200 and 260 (6 mm band) around (0, 0), the upper half.
        let n = 48;
        let mut pts: Vec<Vec2> = (0..=n)
            .map(|k| {
                let a = std::f64::consts::PI * k as f64 / n as f64;
                v2(260.0 * a.cos(), -260.0 * a.sin())
            })
            .collect();
        pts.extend((0..=n).rev().map(|k| {
            let a = std::f64::consts::PI * k as f64 / n as f64;
            v2(200.0 * a.cos(), -200.0 * a.sin())
        }));
        let band = Contour::new(pts, true);
        let s = derive(std::slice::from_ref(&band)).expect("skeleton");
        assert!(s.length() > 400.0, "length {}", s.length());
        let last = s.points.len() - 1;
        for (i, (p, w)) in s.points.iter().zip(&s.widths).enumerate() {
            // Within 0.8 mm of the band's middle (the reached-out ends 1.5 mm).
            let r = p.length();
            let off = if i == 0 || i == last { 15.0 } else { 8.0 };
            assert!((r - 230.0).abs() < off, "{p:?} radius {r}");
            if p.y < -60.0 {
                assert!((w - 6.0).abs() < 0.8, "width {w} at {p:?}");
            }
        }
        // The column stays perpendicular to the band: across, not along.
        let o = outline(&s).expect("outline");
        assert!(o.points.iter().all(|p| (p.length() - 230.0).abs() < 40.0));
    }

    #[test]
    fn deriving_from_nothing_gives_none() {
        assert_eq!(derive(&[]), None);
        let open = Contour::new(vec![v2(0.0, 0.0), v2(100.0, 0.0), v2(100.0, 50.0)], false);
        assert_eq!(derive(&[open]), None);
        assert_eq!(derive(&[Contour::rectangle(0.0, 0.0, 1.0, 1.0)]), None);
        let nan = Contour::new(
            vec![
                v2(0.0, 0.0),
                v2(f64::NAN, 0.0),
                v2(100.0, 50.0),
                v2(0.0, 50.0),
            ],
            true,
        );
        if let Some(s) = derive(&[nan]) {
            assert_eq!(s.validate(), Ok(()));
        }
        // A wide disc still gives a sewable line, widths capped.
        let disc = Contour::ellipse(v2(0.0, 0.0), 500.0, 500.0, 72);
        let s = derive(&[disc]).expect("disc");
        assert!(s.widths.iter().all(|w| *w <= MAX_WIDTH));
        assert_eq!(s.validate(), Ok(()));
    }

    fn line(keys: &[(f64, f64, f64)]) -> SatinLine {
        SatinLine {
            width: 3.0,
            keys: keys
                .iter()
                .map(|&(at, left, right)| WidthKey { at, left, right })
                .collect(),
            ..SatinLine::default()
        }
    }

    #[test]
    fn satin_line_widths_follow_keys_sides_tapers_and_slant() {
        let path = vec![v2(0.0, 0.0), v2(400.0, 0.0)];
        // Keys interpolate smoothly: 2 mm at the start, 6 mm at the end, 4 mm halfway.
        let l = line(&[(0.0, 1.0, 1.0), (1.0, 3.0, 3.0)]);
        assert_eq!(l.sides(0.0, 400.0), [1.0, 1.0]);
        assert_eq!(l.sides(1.0, 400.0), [3.0, 3.0]);
        assert!((l.sides(0.5, 400.0)[0] - 2.0).abs() < 1e-9);
        assert!(l.sides(0.25, 400.0)[0] < 1.5, "smooth, not linear");
        // One side only: the column lies wholly to the right of the line.
        let right = line(&[(0.0, 0.0, 3.0)]);
        for r in satin_line_rungs(&path, &right, 10.0) {
            assert!(r[0].y.abs() < 1e-9, "{r:?}");
            assert!((r[1].y - 30.0).abs() < 1e-9, "{r:?}");
        }
        // Tapers reach a point at the ends.
        let taper = SatinLine {
            taper: [10.0, 5.0],
            ..line(&[])
        };
        let rungs = satin_line_rungs(&path, &taper, 10.0);
        let w = |r: &[Vec2; 2]| r[0].distance(r[1]);
        assert!(w(&rungs[0]) < 1e-9 && w(rungs.last().expect("end")) < 1e-9);
        assert!(
            (w(&rungs[20]) - 30.0).abs() < 1e-9,
            "full width in the middle"
        );
        assert!(w(&rungs[5]) > 0.0 && w(&rungs[5]) < 30.0, "tapering");
        // Slant: the crossings turn 30 degrees; the edges stay 1.5 mm from the line.
        let slant = SatinLine {
            slant: 30.0,
            ..line(&[])
        };
        for r in satin_line_rungs(&path, &slant, 10.0) {
            let d = r[1] - r[0];
            let angle = (d.x / d.length()).dasin().to_degrees();
            assert!((angle.abs() - 30.0).abs() < 1e-6, "{angle}");
            assert!((r[0].y.abs() - 15.0).abs() < 1e-9 && (r[1].y.abs() - 15.0).abs() < 1e-9);
        }
        // One symmetric key and no slant: a uniform line.
        assert!(line(&[(0.3, 1.5, 1.5)]).is_uniform());
        assert_eq!(line(&[(0.3, 1.5, 1.5)]).uniform_width(), 3.0);
        assert!(!line(&[(0.0, 0.0, 3.0)]).is_uniform());
    }

    fn polygon(pts: &[(f64, f64)]) -> Vec<Contour> {
        vec![Contour::new(
            pts.iter().map(|&(x, y)| v2(x, y)).collect(),
            true,
        )]
    }

    /// Share of the region's inside grid cells covered by the columns of `arms`.
    fn coverage(c: &[Contour], arms: &[Skeleton]) -> f64 {
        let edges: Vec<(Vec2, Vec2)> = c.iter().flat_map(Contour::edges).collect();
        let b = crate::geometry::Rect::enclosing(edges.iter().map(|e| e.0)).expect("b");
        let g = Grid::new(&edges, &b).expect("grid");
        let inside: Vec<usize> = (0..g.clearance.len())
            .filter(|&k| g.clearance[k] >= g.step)
            .collect();
        let covered = inside
            .iter()
            .filter(|&&k| depth_in(g.centre(k), arms) >= -g.step)
            .count();
        covered as f64 / inside.len() as f64
    }

    #[test]
    fn branching_shapes_split_into_one_column_per_arm() {
        let bar = polygon(&[(0.0, 0.0), (300.0, 0.0), (300.0, 40.0), (0.0, 40.0)]);
        let one = branches(&bar, 0.3);
        assert_eq!(one.len(), 1, "no branches: derive's one column");
        assert_eq!(one[0], derive(&bar).expect("bar"));
        // A Y 4 mm wide: the main stroke through two arms, a column for the third.
        let y = polygon(&[
            (180.0, 0.0),
            (220.0, 0.0),
            (220.0, 200.0),
            (340.0, 320.0),
            (312.0, 348.0),
            (200.0, 236.0),
            (88.0, 348.0),
            (60.0, 320.0),
            (180.0, 200.0),
        ]);
        let arms = branches(&y, 0.3);
        assert_eq!(arms.len(), 2, "{arms:?}");
        assert!(coverage(&y, &arms) > 0.9, "{}", coverage(&y, &arms));
        // The arm ends inside the main column, by the overlap at least.
        let join = arms[1].points[0];
        assert!(
            depth_in(join, &arms[..1]) >= 3.0 - 1e-6,
            "{}",
            depth_in(join, &arms[..1])
        );
        // A plus: one bar, then its two halves of the other.
        let plus = polygon(&[
            (130.0, 0.0),
            (170.0, 0.0),
            (170.0, 130.0),
            (300.0, 130.0),
            (300.0, 170.0),
            (170.0, 170.0),
            (170.0, 300.0),
            (130.0, 300.0),
            (130.0, 170.0),
            (0.0, 170.0),
            (0.0, 130.0),
            (130.0, 130.0),
        ]);
        let arms = branches(&plus, 0.3);
        assert_eq!(arms.len(), 3, "{arms:?}");
        assert!(
            coverage(&plus, &arms) > 0.9,
            "{} {arms:?}",
            coverage(&plus, &arms)
        );
        // A five-pointed star: the main stroke through two points, a column for each other.
        let star: Vec<(f64, f64)> = (0..10)
            .map(|i| {
                let a = std::f64::consts::PI * i as f64 / 5.0 - std::f64::consts::FRAC_PI_2;
                let r = if i % 2 == 0 { 250.0 } else { 70.0 };
                (250.0 + r * a.dcos(), 250.0 + r * a.dsin())
            })
            .collect();
        let star = polygon(&star);
        let arms = branches(&star, 0.3);
        assert_eq!(arms.len(), 4, "{arms:?}");
        assert!(coverage(&star, &arms) > 0.9, "{}", coverage(&star, &arms));
        for sk in &arms {
            assert_eq!(sk.validate(), Ok(()));
        }
    }
}

#[cfg(test)]
mod letter_tests {
    use super::*;

    fn letter(ch: &str, size: f64) -> Vec<Contour> {
        let spec = crate::text::TextSpec::new(ch, "lato-bold", size);
        crate::text::layout(&spec, None).expect("layout").contours()
    }

    /// The share of the letter's inner cells the satin of `arms` sews over, and the grid.
    fn sewn(c: &[Contour], arms: &[Skeleton]) -> f64 {
        let edges: Vec<(Vec2, Vec2)> = c.iter().flat_map(Contour::edges).collect();
        let b = crate::geometry::Rect::enclosing(edges.iter().map(|e| e.0)).expect("b");
        covered(&Grid::new(&edges, &b).expect("grid"), arms)
    }

    #[test]
    fn k_and_r_keep_their_stem_straight_through_the_junction() {
        // #526: Lato Bold K and R at 10 and 25 mm. Their longest stroke ran down a leg and
        // turned up the stem at the junction, fanning there and leaving a gap in the stem.
        for (ch, size, least) in [
            ("K", 10.0, 0.97),
            ("K", 25.0, 0.97),
            ("R", 10.0, 0.93),
            ("R", 25.0, 0.93),
        ] {
            let c = letter(ch, size);
            let arms = branches(&c, 0.3);
            assert!(arms.len() >= 3, "{ch} {size}: {}", arms.len());
            // The first column is the stem: straight, upright, the letter's height.
            let stem = &arms[0];
            let (a, b) = (stem.points[0], stem.points[stem.points.len() - 1]);
            assert!(sharpest_bend(stem) < 1.0, "{ch} {size}");
            assert!(
                (a.x - b.x).abs() < 0.05 * (a.y - b.y).abs(),
                "{ch} {size}: {a:?} {b:?}"
            );
            assert!(
                (a.y - b.y).abs() > 0.9 * size * 7.0,
                "{ch} {size}: {a:?} {b:?}"
            );
            let share = sewn(&c, &arms);
            assert!(share >= least, "{ch} {size}: {share:.3}");
            for sk in &arms {
                assert_eq!(sk.validate(), Ok(()));
            }
        }
    }

    /// The share of each letter the columns sew (Lato Bold at 10 and 25 mm, #526): K, Y and y
    /// over the 0.95 bar (Y's main column is its stem and one arm, not the V through both arms,
    /// which narrows to nothing at the crotch; y's is its tail and long stroke, measured on its
    /// own side of the junction); the others no lower than measured on 2026-11-12. The letters
    /// without a junction (N, M, W) are still under the bar (#526).
    #[test]
    fn letters_split_into_columns_that_sew_them() {
        // Raised 2026-11-12, when free column ends began to reach the outline (#526); A's when
        // its crossbar became a column of its own (a bridge).
        let floors = [
            ("K", 0.98, 0.99),
            ("Y", 0.96, 0.97),
            ("X", 0.98, 0.98),
            ("R", 0.94, 0.96),
            ("A", 0.97, 0.98),
            ("y", 0.97, 0.98),
            // N, M and W cut at their notches, one column per stroke (#526): 0.98-0.99 from
            // one fanning column's 0.85-0.96.
            ("N", 0.97, 0.97),
            ("M", 0.97, 0.97),
            ("W", 0.97, 0.97),
        ];
        for (ch, small, large) in floors {
            for (size, floor) in [(10.0, small), (25.0, large)] {
                let c = letter(ch, size);
                let columns = branches(&c, 0.3);
                let strokes = match ch {
                    "N" => Some(3),
                    "M" | "W" => Some(4),
                    _ => None,
                };
                if let Some(n) = strokes {
                    assert_eq!(columns.len(), n, "{ch} at {size} mm");
                }
                let share = sewn(&c, &columns);
                assert!(
                    share >= floor,
                    "{ch} at {size} mm sews {share:.3}, under {floor}"
                );
            }
        }
    }

    /// The #526 measurement table: columns and the share sewn for Lato Bold letters at 10 and
    /// 25 mm. Run with `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn letter_table() {
        for ch in ["K", "X", "R", "N", "Y", "y", "A", "M", "W"] {
            for size in [10.0, 25.0] {
                let c = letter(ch, size);
                let arms = branches(&c, 0.3);
                let widest = arms
                    .iter()
                    .flat_map(|a| a.widths.iter().copied())
                    .fold(0.0, f64::max);
                eprintln!(
                    "TABLE {ch} {size}: {} columns, sewn {:.3}, widest {widest:.1} mm, main bend \
                     {:.0}, collapses {}",
                    arms.len(),
                    sewn(&c, &arms),
                    sharpest_bend(&arms[0]),
                    collapses(&arms[0])
                );
                for (k, a) in arms.iter().enumerate() {
                    let n = a.points.len();
                    eprintln!(
                        "  col {k}: {:.0?} -> {:.0?}, {n} pts, widths {:.1}..{:.1}",
                        a.points[0],
                        a.points[n - 1],
                        a.widths.iter().copied().fold(f64::INFINITY, f64::min),
                        a.widths.iter().copied().fold(0.0, f64::max)
                    );
                }
            }
        }
    }

    #[test]
    fn letters_without_a_bent_junction_split_as_before() {
        // X keeps its two crossing strokes; N, cut at its two notches since #526, its three.
        assert_eq!(branches(&letter("X", 25.0), 0.3).len(), 3);
        assert_eq!(branches(&letter("N", 25.0), 0.3).len(), 3);
    }

    #[test]
    fn a_column_on_a_slanted_end_reaches_both_corners_without_sewing_past_it() {
        // #526: a 4 mm column up a stroke whose top is cut at a slant, from (0, 2) mm down to
        // (4, 0) mm, its end on that edge. A square end rung at the end sewed past the edge on
        // the left and left the right corner 1 mm off; the column on the outline reaches both
        // corners within 0.2 mm and every needle point stays inside.
        let stroke = Contour::new(
            vec![
                v2(0.0, 200.0),
                v2(40.0, 200.0),
                v2(40.0, 0.0),
                v2(0.0, 20.0),
            ],
            true,
        );
        let sk = Skeleton {
            points: vec![v2(20.0, 195.0), v2(20.0, 11.0)],
            widths: vec![4.0, 4.0],
        };
        let shape = std::slice::from_ref(&stroke);
        let run = satin_to_outline(&sk, shape, 2.0, 0.0, [false, true]);
        assert!(run.len() > 100, "{}", run.len());
        for corner in [v2(0.0, 20.0), v2(40.0, 0.0)] {
            let near = run
                .iter()
                .map(|p| p.distance(corner))
                .fold(f64::INFINITY, f64::min);
            assert!(near <= 2.0, "corner {corner:?}: {near:.2}");
        }
        let edges = stroke.edges();
        for p in &run {
            let inside = crate::geometry::contains(*p, shape);
            let on = edges
                .iter()
                .any(|&(a, b)| segment_distance(*p, a, b) <= 0.5);
            assert!(inside || on, "{p:?} is outside the stroke");
        }
    }
}
