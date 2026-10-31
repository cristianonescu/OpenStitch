//! OpenStitch mode's outlines for Auto Punch (B16.1 T12; docs/research/beyond/
//! image-similarity-plan.md): the thin, long strokes of the darkest colour (the ink of a clip
//! art's or a cartoon's contour lines) are sewn as satin lines along their skeleton, on top,
//! the way an embroidery digitizer sews outlines; the regions on either side are extended
//! under them, so no fabric shows along an outline. Thick parts of that colour (a pupil, a
//! solid shape) stay regions.
//!
//! The skeleton is [`center::thin`] (Zhang–Suen) traced by [`center::skeleton_paths`], the
//! stroke width the chamfer distance ([`regions::distance_map`]) along it; all integer or
//! plain `f64` arithmetic in a fixed order, so the result is the same on every platform.

use crate::center;
use crate::palette::Palette;
use crate::raster::Frame;
use crate::regions::{self, NONE, TraceOptions};
use crate::trace;
use openstitch_design::{DesignObject, LineSewing, LineType, Vec2, v2};
use std::collections::VecDeque;

/// Only a colour at most this light (CIELAB L*) is taken for outline ink.
pub const INK_MAX_L: f64 = 25.0;

/// A stroke is an outline when its skeleton is at least this many times as long as the stroke
/// is wide.
pub const ASPECT: f64 = 3.0;

/// …and at least this long, mm; shorter pieces stay regions.
pub const MIN_LENGTH_MM: f64 = 3.5;

/// …and its skeleton spans a box of at least this, mm² (each side at least the stroke width),
/// so no outline is a speck: a squiggle with a long skeleton in a small box, or a short
/// straight dash, stays a region.
pub const MIN_BOX_MM2: f64 = 2.0;

/// A thin piece whose border touches the ink's thick parts for more than 1/this of its length
/// is a rim of a solid shape, not a line, and stays a region.
pub const RIM_SHARE_DIV: usize = 4;

/// Zigzag density of the outlines, stitches per mm (PE-DESIGN's zigzag default).
pub const DENSITY: f64 = 4.5;

/// Thinnest outline zigzag, mm; a narrower stroke (a hairline) is a running stitch.
pub const MIN_WIDTH_MM: f64 = 0.6;

/// Stitch length of a hairline's running stitch, mm.
pub const RUN_PITCH_MM: f64 = 1.5;

/// The outlines taken out of a label map.
#[derive(Clone, Debug, PartialEq)]
pub struct Outlines {
    /// Thread index (into `palette.threads`) of the ink.
    pub thread: usize,
    /// One line object per stroke, in scan order of the stroke's first pixel.
    pub lines: Vec<DesignObject>,
}

/// Takes the outlines out of `labels` (thread index per pixel, [`NONE`] where unsewn): the
/// darkest thread's strokes no wider than `max_mm` that are long enough ([`ASPECT`],
/// [`MIN_LENGTH_MM`]) become lines, and their pixels take the labels of the pixels beside
/// them (nearest first, ties in scan order), so the neighbouring regions run under the line.
/// `None` when no thread is dark enough ([`INK_MAX_L`]) or no stroke qualifies.
pub fn extract(
    labels: &mut [u16],
    w: usize,
    h: usize,
    palette: &Palette,
    frame: &Frame,
    max_mm: f64,
    trace_options: &TraceOptions,
) -> Option<Outlines> {
    let mm = frame.mm_per_px();
    if mm <= 0.0 || max_mm <= 0.0 || w == 0 || h == 0 {
        return None;
    }
    // The ink: the darkest thread present.
    let mut present = vec![false; palette.threads.len()];
    for &l in labels.iter() {
        if l != NONE && (l as usize) < present.len() {
            present[l as usize] = true;
        }
    }
    let ink = (0..palette.threads.len())
        .filter(|&t| present[t])
        .map(|t| {
            let c = &palette.threads[t];
            (crate::perceptual::lab([c.red, c.green, c.blue])[0], t)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .filter(|&(l, _)| l <= INK_MAX_L)?
        .1 as u16;
    let dark: Vec<bool> = labels.iter().map(|&l| l == ink).collect();
    let depth_of = regions::distance_map(&dark, w, h);
    // Thick parts: a morphological opening, every ink pixel within half of `max_mm` of a
    // pixel deeper than that.
    let r_px = max_mm / 2.0 / mm;
    let depth = (3.0 * (r_px + 0.5)).round() as u32;
    let core: Vec<bool> = depth_of.iter().map(|&d| d > depth).collect();
    let mut thick = vec![false; w * h];
    if core.iter().any(|&c| c) {
        let pad = r_px.ceil() as usize + 2;
        let (pw, ph) = (w + 2 * pad, h + 2 * pad);
        let mut away = vec![true; pw * ph];
        for y in 0..h {
            for x in 0..w {
                away[(y + pad) * pw + x + pad] = !core[y * w + x];
            }
        }
        let to_core = regions::distance_map(&away, pw, ph);
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                thick[i] = dark[i] && to_core[(y + pad) * pw + x + pad] <= depth + 3;
            }
        }
    }
    let thin: Vec<u16> = (0..w * h)
        .map(|i| if dark[i] && !thick[i] { 0 } else { NONE })
        .collect();
    let (id, comps) = regions::components(&thin, w, h);
    let thread = &palette.threads[ink as usize];
    let mut lines = Vec::new();
    let mut taken = vec![false; w * h];
    for (c, k) in comps.iter().enumerate() {
        let (m, mw, mh) = regions::mask(&id, w, c as u32, k);
        // A rim of a thick part (the black between a pupil's highlight and its edge) is not
        // a line: it stays with the region when much of its border is that thick part.
        let (mut rim, mut border) = (0usize, 0usize);
        for y in 0..mh {
            for x in 0..mw {
                if !m[y * mw + x] {
                    continue;
                }
                for j in neighbours((y + k.y0) * w + x + k.x0, w, h) {
                    if id[j] != c as u32 {
                        border += 1;
                        if thick[j] {
                            rim += 1;
                        }
                    }
                }
            }
        }
        if rim * RIM_SHARE_DIV > border {
            continue;
        }
        let skeleton = center::thin(&m, mw, mh);
        let paths = center::skeleton_paths(&skeleton, mw, mh);
        // Widths along the skeleton, measured on the whole ink mask so a stroke that meets a
        // thick part keeps its own width.
        let width_px = |pts: &[Vec2]| -> f64 {
            let mut d: Vec<u32> = pts
                .iter()
                .map(|q| {
                    let x = (q.x.max(0.0) as usize).min(mw - 1) + k.x0;
                    let y = (q.y.max(0.0) as usize).min(mh - 1) + k.y0;
                    depth_of[y * w + x]
                })
                .collect();
            d.sort_unstable();
            d.get(d.len() / 2).map_or(1.0, |&v| regions::width_for(v))
        };
        let all: Vec<Vec2> = paths.iter().flat_map(|(p, _)| p.iter().copied()).collect();
        if all.is_empty() {
            continue;
        }
        let width = width_px(&all);
        let kept_paths: Vec<&(Vec<Vec2>, bool)> = paths
            .iter()
            .filter(|(p, closed)| *closed || paths.len() == 1 || length(p) >= width)
            .collect();
        let total: f64 = kept_paths.iter().map(|(p, _)| length(p)).sum();
        // The box the kept skeleton spans, each side at least the stroke's width.
        let (mut lo, mut hi) = (v2(f64::MAX, f64::MAX), v2(f64::MIN, f64::MIN));
        for (pts, _) in &kept_paths {
            for q in pts {
                lo = v2(lo.x.min(q.x), lo.y.min(q.y));
                hi = v2(hi.x.max(q.x), hi.y.max(q.y));
            }
        }
        let (bw, bh) = ((hi.x - lo.x).max(width) * mm, (hi.y - lo.y).max(width) * mm);
        if total < ASPECT * width || total * mm < MIN_LENGTH_MM || bw * bh < MIN_BOX_MM2 {
            continue;
        }
        let shifted = Frame {
            origin: frame.point(k.x0 as f64, k.y0 as f64),
            scale: frame.scale,
        };
        let mut contours = Vec::new();
        for (pts, closed) in kept_paths {
            let kept = if *closed {
                trace::simplify_closed(pts, trace_options.simplify)
            } else {
                trace::simplify_open(pts, trace_options.simplify)
            };
            if kept.len() < 2 {
                continue;
            }
            contours.push(trace::contour(
                pts,
                &kept,
                *closed,
                trace_options.curves,
                trace_options.simplify,
                trace_options.corner_degrees,
                &shifted,
            ));
        }
        if contours.is_empty() {
            continue;
        }
        let line = if width * mm < MIN_WIDTH_MM {
            // A hairline: one thread wide.
            let mut l = LineSewing::new(LineType::Running, thread.clone());
            l.pitch = RUN_PITCH_MM;
            l
        } else {
            let mut l = LineSewing::new(LineType::Zigzag, thread.clone());
            l.width = width * mm;
            l.density = DENSITY;
            // One covering layer, as OpenStitch mode's regions (the density rule).
            l.underlay = false;
            l
        };
        lines.push(DesignObject::shape(contours, None, Some(line)));
        for y in 0..mh {
            for x in 0..mw {
                if m[y * mw + x] {
                    taken[(y + k.y0) * w + x + k.x0] = true;
                }
            }
        }
    }
    if lines.is_empty() {
        return None;
    }
    // The strokes' pixels take the labels beside them, spreading in from every side.
    let mut queue = VecDeque::new();
    for i in 0..w * h {
        if !taken[i] && neighbours(i, w, h).any(|j| taken[j]) {
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        for j in neighbours(i, w, h) {
            if taken[j] {
                taken[j] = false;
                labels[j] = labels[i];
                queue.push_back(j);
            }
        }
    }
    Some(Outlines {
        thread: ink as usize,
        lines,
    })
}

fn neighbours(i: usize, w: usize, h: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (i % w, i / w);
    [
        (x > 0).then(|| i - 1),
        (x + 1 < w).then(|| i + 1),
        (y > 0).then(|| i - w),
        (y + 1 < h).then(|| i + w),
    ]
    .into_iter()
    .flatten()
}

fn length(pts: &[Vec2]) -> f64 {
    pts.windows(2).map(|w| w[0].distance(w[1])).sum()
}
