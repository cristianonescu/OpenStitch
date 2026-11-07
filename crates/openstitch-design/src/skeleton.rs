//! Variable-direction satin along a skeleton: a satin column whose stitches stay perpendicular
//! to a user-given centre line, with a width given at each centre-line point and interpolated
//! along it. Around a bend the direction turns smoothly over the column's half width. Own
//! algorithm. Points are 0.1 mm, widths mm.

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
            let d = ray(centre, dir, &edges).map_or(half, |d| d.min(half + 5.0));
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
    line_to_skeleton(pts, head.is_some(), tail.is_some(), &edges, grid.step)
}

/// Most columns one branching region is cut into.
const MAX_BRANCHES: usize = 16;

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
    let cells = grid.clearance.len();
    let first = main.clone();
    let mut arms = vec![main];
    // The main column's widths are measured again along its whole length: derive simplifies a
    // straight stroke to its two ends, whose width is then the nearest edge's (the end of the
    // bar across a plus). The arms join this column; without arms derive's own is kept.
    {
        let mut dense = vec![first.points[0]];
        for w in first.points.windows(2) {
            let n = ((w[0].distance(w[1]) / (4.0 * grid.step)).ceil() as usize).max(1);
            dense.extend((1..=n).map(|i| w[0].lerp(w[1], i as f64 / n as f64)));
        }
        if let Some(mut main) = measure(&dense, true, true, &edges, grid.step) {
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
            let (tip, reach) = part
                .iter()
                .map(|&k| (k, -depth[k]))
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .expect("not empty");
            let wide = part.iter().map(|&k| grid.clearance[k]).fold(0.0, f64::max);
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
        let mut line: Vec<usize> = Vec::new();
        if let Some(t) = target {
            let mut k = t;
            while k != usize::MAX && line.len() <= cells {
                line.push(k);
                if k == tip {
                    break;
                }
                k = prev[k];
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
        match (line.last() == Some(&tip))
            .then(|| line_to_skeleton(pts, false, reached, &edges, grid.step))
            .flatten()
        {
            // The arm's own cells are never an arm again; a part next to it (two arms meeting
            // in the uncovered middle of a star) still is.
            Some(sk) => {
                arms.push(sk);
                line.iter().for_each(|&k| tried[k] = true);
            }
            None => part.iter().for_each(|&k| tried[k] = true),
        }
    }
    if arms.len() == 1 {
        return vec![first];
    }
    arms
}

/// A skeleton from a centre line on the grid (`pts`, 0.1 mm): smoothed, simplified, each point
/// moved to the middle of the chord across the region there and given its width (see
/// [`derive`]); `head` and `tail` say whether the ends were reached out towards the outline.
fn line_to_skeleton(
    mut pts: Vec<Vec2>,
    head: bool,
    tail: bool,
    edges: &[(Vec2, Vec2)],
    step: f64,
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
    measure(&line, head, tail, edges, step)
}

/// The skeleton along `line` (0.1 mm): each point moved to the middle of the chord across the
/// region there and given its width; `head` and `tail` say whether the ends were reached out
/// towards the outline (their width is then no wider than next door).
fn measure(
    line: &[Vec2],
    head: bool,
    tail: bool,
    edges: &[(Vec2, Vec2)],
    step: f64,
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
            Some((left, Some(l), Some(r))) if l + r <= 6.0 * near + 2.0 * step => {
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
