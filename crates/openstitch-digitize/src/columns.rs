//! Straight columns of a satin region (#443): a region made of straight bars meeting at
//! junctions (the letters T, H, E, F, L in a bold sans) is split into one piece per bar, each
//! sewn as satin straight across it. PE-DESIGN 11.50's Auto Punch sews such letters that way
//! (round-5 `g08-lettering__autopunch.pes`: the T's bar across at 90°, its stem at 0°; the H's
//! stems at 0°, its crossbar at 90°), where one Auto satin column through the whole letter fans
//! its stitches around the junctions.
//!
//! Each pixel takes the direction, of four (horizontal, vertical and the two diagonals), along
//! which the region runs furthest through it: the direction of the bar it belongs to. Pixels of
//! one direction that touch make a piece; pieces smaller than a quarter of the stroke width
//! squared go to the neighbour they share most border with. The split is used only when every
//! piece is close to a rectangle along its direction ([`RECTANGULAR`]). Curved strokes (S, C,
//! O) give pieces that are not: they are sewn as one column through rungs square to their
//! centre line ([`rungs`]). A region of one direction (a single bar) keeps Auto satin. Own
//! algorithm.

use openstitch_design::fmath::Det;
use openstitch_design::{Contour, Vec2, paths, skeleton, v2};

/// A piece is a bar when it covers at least this share of its bounding box along its
/// direction.
pub const RECTANGULAR: f64 = 0.8;

/// Pieces under this share of the stroke width squared go to a neighbour.
const SMALL: f64 = 0.25;

/// A run counts at most this many stroke widths on each side of a pixel.
const REACH: f64 = 1.0;

/// A diagonal run must be this many times longer than a horizontal or vertical one to win.
const DIAGONAL: f64 = 1.1;

/// The directions, in tie order: (dx, dy) on the working raster (y down) and the stroke
/// direction in degrees with y down.
const DIRS: [(isize, isize, f64); 4] = [(1, 0, 0.0), (0, 1, 90.0), (1, 1, 45.0), (1, -1, -45.0)];

/// One straight bar of a region: its pixels on the region's `mw`×`mh` crop (grown one pixel
/// into the pieces beside it, so neighbouring satins meet without a gap) and the direction the
/// bar runs in, degrees with y down (0 horizontal, 90 vertical).
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub mask: Vec<bool>,
    pub direction: f64,
}

/// What a satin region is made of.
#[derive(Clone, Debug, PartialEq)]
pub enum Strokes {
    /// One bar (or nothing to split).
    Single,
    /// Straight bars, in sewing order (each next to the one before where it can be).
    Bars(Vec<Column>),
    /// Strokes that change direction, not straight bars: a curved column (S, C) or a shape
    /// of no clear direction.
    Curved,
}

/// What the region `m` (`mw`×`mh`), whose stroke is `width` pixels wide, is made of.
pub fn split(m: &[bool], mw: usize, mh: usize, width: f64) -> Strokes {
    let n = mw * mh;
    if n == 0 || m.len() != n || !(width.is_finite() && width > 0.0) {
        return Strokes::Single;
    }
    bars(m, mw, mh, width).unwrap_or(Strokes::Curved)
}

/// [`split`]'s work; `None` for [`Strokes::Curved`].
fn bars(m: &[bool], mw: usize, mh: usize, width: f64) -> Option<Strokes> {
    let n = mw * mh;
    // The run through each pixel along each direction, pixels (a diagonal step counts √2).
    // (capped run, whole run, direction) of the best direction so far.
    let mut best = vec![(0.0f64, 0.0f64, usize::MAX); n];
    for (k, &(dx, dy, _)) in DIRS.iter().enumerate() {
        let step = ((dx * dx + dy * dy) as f64).sqrt();
        let mut fwd = vec![0u32; n];
        let mut back = vec![0u32; n];
        let at = |x: isize, y: isize| -> Option<usize> {
            (x >= 0 && y >= 0 && (x as usize) < mw && (y as usize) < mh)
                .then(|| y as usize * mw + x as usize)
        };
        // Every predecessor (x − dx, y − dy) has x − 1 or the same x with y − 1: column-major
        // order visits it first.
        for x in 0..mw as isize {
            for y in 0..mh as isize {
                let i = y as usize * mw + x as usize;
                if m[i] {
                    fwd[i] = 1 + at(x - dx, y - dy).map_or(0, |j| fwd[j]);
                }
            }
        }
        for x in (0..mw as isize).rev() {
            for y in (0..mh as isize).rev() {
                let i = y as usize * mw + x as usize;
                if m[i] {
                    back[i] = 1 + at(x + dx, y + dy).map_or(0, |j| back[j]);
                }
            }
        }
        // Each side of the run counts up to [`REACH`] stroke widths, so the measure stays local:
        // a long chord from one bar's corner through a junction into another bar does not
        // outweigh the bar the pixel lies in. A diagonal wins only by [`DIAGONAL`] more. Where
        // two directions both reach that far, the longer whole run wins (a stem's pixel beside
        // a crossbar belongs to the stem when the stem is the longer).
        let reach = REACH * width;
        let weight = if dx != 0 && dy != 0 {
            1.0 / DIAGONAL
        } else {
            1.0
        };
        for i in 0..n {
            if m[i] {
                let side = |c: u32| (f64::from(c - 1) * step).min(reach);
                let run = (step + side(fwd[i]) + side(back[i])) * weight;
                let whole = f64::from(fwd[i] + back[i] - 1) * step * weight;
                let b = best[i];
                if b.2 == usize::MAX
                    || run > b.0 + 1e-9
                    || ((run - b.0).abs() <= 1e-9 && whole > b.1 + 1e-9)
                {
                    best[i] = (run, whole, k);
                }
            }
        }
    }
    let mut label: Vec<usize> = best.iter().map(|b| b.2).collect();
    // Two 3×3 majority passes within the region smooth the boundaries between directions.
    for _ in 0..2 {
        let prev = label.clone();
        for y in 0..mh {
            for x in 0..mw {
                let i = y * mw + x;
                if !m[i] {
                    continue;
                }
                let mut votes = [0u8; 4];
                for ny in y.saturating_sub(1)..(y + 2).min(mh) {
                    for nx in x.saturating_sub(1)..(x + 2).min(mw) {
                        let j = ny * mw + nx;
                        if m[j] {
                            votes[prev[j]] += 1;
                        }
                    }
                }
                let own = prev[i];
                if let Some(k) = (0..4)
                    .filter(|&k| votes[k] > votes[own])
                    .max_by_key(|&k| (votes[k], std::cmp::Reverse(k)))
                {
                    label[i] = k;
                }
            }
        }
    }
    let small = SMALL * width * width;
    let mut piece = pieces(m, mw, mh, &label);
    // Small pieces go to the neighbour they share most border with, smallest first.
    for _ in 0..64 {
        let count = piece
            .iter()
            .filter(|&&p| p != usize::MAX)
            .fold(0, |a, &p| a.max(p + 1));
        let mut area = vec![0usize; count];
        for &p in piece.iter().filter(|&&p| p != usize::MAX) {
            area[p] += 1;
        }
        let Some(tiny) = (0..count)
            .filter(|&p| (area[p] as f64) < small)
            .min_by_key(|&p| (area[p], p))
        else {
            break;
        };
        let mut border = vec![0usize; count];
        for y in 0..mh {
            for x in 0..mw {
                let i = y * mw + x;
                if piece[i] != tiny {
                    continue;
                }
                for (nx, ny) in [
                    (x + 1, y),
                    (x.wrapping_sub(1), y),
                    (x, y + 1),
                    (x, y.wrapping_sub(1)),
                ] {
                    if nx < mw && ny < mh {
                        let q = piece[ny * mw + nx];
                        if q != usize::MAX && q != tiny {
                            border[q] += 1;
                        }
                    }
                }
            }
        }
        let Some(into) = (0..count)
            .filter(|&q| border[q] > 0)
            .max_by_key(|&q| (border[q], std::cmp::Reverse(q)))
        else {
            // An isolated small piece: the region is all small, nothing to split.
            return Some(Strokes::Single);
        };
        let to = label[piece.iter().position(|&p| p == into)?];
        for i in 0..n {
            if piece[i] == tiny {
                label[i] = to;
            }
        }
        piece = pieces(m, mw, mh, &label);
    }
    let count = piece
        .iter()
        .filter(|&&p| p != usize::MAX)
        .fold(0, |a, &p| a.max(p + 1));
    if count < 2 {
        return Some(Strokes::Single);
    }
    let mut columns = Vec::with_capacity(count);
    for p in 0..count {
        let first = piece.iter().position(|&q| q == p)?;
        let direction = DIRS[label[first]].2;
        let own: Vec<bool> = piece.iter().map(|&q| q == p).collect();
        if !rectangular(&own, mw, mh, direction) {
            return None;
        }
        columns.push((own, direction));
    }
    // Sewing order: from the piece met first in scan order, on to a neighbour each time while
    // there is one (else the next piece in scan order).
    let touches = |a: &[bool], b: &[bool]| {
        (0..n).any(|i| {
            let (x, y) = (i % mw, i / mw);
            a[i] && ((x + 1 < mw && b[i + 1])
                || (y + 1 < mh && b[i + mw])
                || (x > 0 && b[i - 1])
                || (y > 0 && b[i - mw]))
        })
    };
    let mut order = vec![0usize];
    let mut left: Vec<usize> = (1..count).collect();
    while !left.is_empty() {
        let last = *order.last()?;
        let next = left
            .iter()
            .position(|&q| touches(&columns[last].0, &columns[q].0))
            .unwrap_or(0);
        order.push(left.remove(next));
    }
    Some(Strokes::Bars(
        order
            .into_iter()
            .map(|p| Column {
                mask: grown(&columns[p].0, m, mw, mh),
                direction: columns[p].1,
            })
            .collect(),
    ))
}

/// Pieces: the 4-connected pixels of one label within the region, numbered in scan order of
/// their first pixel; `usize::MAX` outside the region.
fn pieces(m: &[bool], mw: usize, mh: usize, label: &[usize]) -> Vec<usize> {
    let mut id = vec![usize::MAX; mw * mh];
    let mut next = 0;
    let mut stack = Vec::new();
    for start in 0..mw * mh {
        if !m[start] || id[start] != usize::MAX {
            continue;
        }
        id[start] = next;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % mw, i / mw);
            for (nx, ny) in [
                (x + 1, y),
                (x.wrapping_sub(1), y),
                (x, y + 1),
                (x, y.wrapping_sub(1)),
            ] {
                if nx < mw && ny < mh {
                    let j = ny * mw + nx;
                    if m[j] && id[j] == usize::MAX && label[j] == label[start] {
                        id[j] = next;
                        stack.push(j);
                    }
                }
            }
        }
        next += 1;
    }
    id
}

/// Whether the piece covers at least [`RECTANGULAR`] of its bounding box along `direction`.
fn rectangular(p: &[bool], mw: usize, mh: usize, direction: f64) -> bool {
    let a = direction.to_radians();
    let (s, c) = (a.dsin(), a.dcos());
    let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    let mut count = 0usize;
    for i in (0..p.len()).filter(|&i| p[i]) {
        let (x, y) = ((i % mw) as f64, (i / mw) as f64);
        let (u, v) = (x * c + y * s, -x * s + y * c);
        u0 = u0.min(u);
        u1 = u1.max(u);
        v0 = v0.min(v);
        v1 = v1.max(v);
        count += 1;
    }
    if count == 0 || p.len() != mw * mh {
        return false;
    }
    // Pixel centres one unit apart cover the box between the outermost ones plus half a pixel
    // on each side.
    count as f64 >= RECTANGULAR * (u1 - u0 + 1.0) * (v1 - v0 + 1.0)
}

/// The piece grown by one pixel (4-neighbours) into the rest of the region `m`.
fn grown(p: &[bool], m: &[bool], mw: usize, mh: usize) -> Vec<bool> {
    (0..mw * mh)
        .map(|i| {
            let (x, y) = (i % mw, i / mw);
            p[i] || (m[i]
                && ((x + 1 < mw && p[i + 1])
                    || (y + 1 < mh && p[i + mw])
                    || (x > 0 && p[i - 1])
                    || (y > 0 && p[i - mw])))
        })
        .collect()
}

/// Rungs of a curved column are this far apart along its centre line, 0.1 mm; the satin
/// between two rungs follows the straight lines between their ends.
const RUNG_STEP: f64 = 7.5;

/// The rungs ([left, right], 0.1 mm, in sewing order) of a satin column filling the closed
/// region `contours` (one outline, no holes) along its centre line (#443): each rung crosses
/// the region square to the smoothed centre line, from outline to outline. A rung that would
/// cross the one before it, or reach much further than the column is wide (through a gap in the
/// outline), is left out; past the centre line's ends the rungs keep the end's direction and
/// step on until they leave the region, so square-cut ends are covered by straight stitches.
/// `None` when no centre line is found or the rungs cover less than [`COVERED`] of the region.
///
/// Auto satin derives its own rungs; on letters cut with corners (S, C) its corner fans crossed
/// one another. These rungs never cross, and the satin between them fans evenly round a curve,
/// as PE-DESIGN's Auto Punch sews the S and C of `g08-lettering__autopunch.pes`.
pub fn rungs(contours: &[Contour]) -> Option<Vec<[Vec2; 2]>> {
    if contours.len() != 1 || !contours[0].closed {
        return None;
    }
    let s = skeleton::derive(contours)?;
    let edges = contours[0].edges();
    // Resample the centre line every RUNG_STEP, then smooth it (a moving average over 5).
    let pts = resample(&s.points, RUNG_STEP);
    if pts.len() < 3 {
        return None;
    }
    let smooth: Vec<Vec2> = (0..pts.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(2), (i + 2).min(pts.len() - 1));
            let n = (b - a + 1) as f64;
            let sum = pts[a..=b].iter().fold(v2(0.0, 0.0), |s, &p| s + p);
            sum * (1.0 / n)
        })
        .collect();
    let tangent = |i: usize| {
        let (a, b) = (i.saturating_sub(2), (i + 2).min(smooth.len() - 1));
        paths::unit(smooth[b] - smooth[a])
    };
    let width = s.widths.iter().copied().fold(0.0, f64::max) * 10.0;
    let longest = 2.5 * width.max(1.0);
    let outline = contours[0].outline_points();
    // A rung starts inside the region: past a stroke's end the step may land in the gap beside
    // it (a C's mouth), and a rung from there would span the gap.
    let rung_at = |p: Vec2, t: Vec2| -> Option<[Vec2; 2]> {
        if !inside(p, &outline) {
            return None;
        }
        let n = v2(-t.y, t.x);
        let l = cast(p, n * -1.0, &edges)?;
        let r = cast(p, n, &edges)?;
        (l + r <= longest).then(|| [p + n * -l, p + n * r])
    };
    let mut out: Vec<[Vec2; 2]> = Vec::new();
    // Before the start: the first direction, stepping back until the rungs leave the region.
    let t0 = tangent(0);
    let mut head = Vec::new();
    for k in 1..200 {
        match rung_at(smooth[0] - t0 * (RUNG_STEP * k as f64), t0) {
            Some(g) => head.push(g),
            None => break,
        }
    }
    head.reverse();
    out.extend(head);
    for (i, &p) in smooth.iter().enumerate() {
        if let Some(g) = rung_at(p, tangent(i))
            && out.last().is_none_or(|&prev| !crosses(prev, g))
        {
            out.push(g);
        }
    }
    let last = smooth.len() - 1;
    let t1 = tangent(last);
    for k in 1..200 {
        match rung_at(smooth[last] + t1 * (RUNG_STEP * k as f64), t1) {
            Some(g) if out.last().is_none_or(|&prev| !crosses(prev, g)) => out.push(g),
            _ => break,
        }
    }
    if out.len() < 3 {
        return None;
    }
    let mut column: Vec<Vec2> = out.iter().map(|g| g[0]).collect();
    column.extend(out.iter().rev().map(|g| g[1]));
    let region = area(&outline);
    (region > 0.0 && area(&column) >= COVERED * region).then_some(out)
}

/// Whether `p` lies inside the closed polygon `poly` (even-odd).
fn inside(p: Vec2, poly: &[Vec2]) -> bool {
    let n = poly.len();
    let mut on = false;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            on = !on;
        }
    }
    on
}

/// The rungs must cover at least this share of the region's area.
pub const COVERED: f64 = 0.9;

fn area(p: &[Vec2]) -> f64 {
    let n = p.len();
    (0..n)
        .map(|i| paths::cross(p[i], p[(i + 1) % n]))
        .sum::<f64>()
        .abs()
        / 2.0
}

/// Points along the polyline `p` every `step`, its ends included.
fn resample(p: &[Vec2], step: f64) -> Vec<Vec2> {
    let total: f64 = p.windows(2).map(|w| w[0].distance(w[1])).sum();
    if p.len() < 2 || total < step {
        return p.to_vec();
    }
    let n = (total / step).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(n + 1);
    let (mut seg, mut along) = (0usize, 0.0);
    for k in 0..=n {
        let want = total * k as f64 / n as f64;
        while seg + 1 < p.len() - 1 && along + p[seg].distance(p[seg + 1]) < want {
            along += p[seg].distance(p[seg + 1]);
            seg += 1;
        }
        let len = p[seg].distance(p[seg + 1]).max(1e-12);
        out.push(p[seg].lerp(p[seg + 1], ((want - along) / len).clamp(0.0, 1.0)));
    }
    out
}

/// Distance from `p` along the unit direction `d` to the nearest outline edge, if any.
fn cast(p: Vec2, d: Vec2, edges: &[(Vec2, Vec2)]) -> Option<f64> {
    edges
        .iter()
        .filter_map(|&(a, b)| {
            let e = b - a;
            let den = paths::cross(d, e);
            if den.abs() < 1e-12 {
                return None;
            }
            let w = a - p;
            let t = paths::cross(w, e) / den;
            let u = paths::cross(w, d) / den;
            (t > 1e-9 && (0.0..=1.0).contains(&u)).then_some(t)
        })
        .min_by(f64::total_cmp)
}

/// Whether the segments `a` and `b` cross (touching ends do not count).
fn crosses(a: [Vec2; 2], b: [Vec2; 2]) -> bool {
    let side = |p: Vec2, q: [Vec2; 2]| paths::cross(q[1] - q[0], p - q[0]);
    let (d1, d2) = (side(b[0], a), side(b[1], a));
    let (d3, d4) = (side(a[0], b), side(a[1], b));
    d1 * d2 < -1e-9 && d3 * d4 < -1e-9
}
