//! Auto Punch approximation: image → 3×3 modal colour census (at most 100 tracked colours)
//! → N PEC threads → regions (4-connected; those of at most the small-region floor removed)
//! → traced outlines → shape objects, satin where the region is a stroke no wider than the
//! satin width, fill elsewhere.
//!
//! Specified from PE-DESIGN 11's public guide (Auto Punch converts an image's colour areas
//! into embroidery regions with chosen colours omitted) and the Phase 6 gap analysis (#127:
//! the modal census, the `max(4, round(param · 100 / mmPerPx²))` small-region floor at
//! 4-connectivity and a centerline-thinness satin test); the algorithm is this project's own.

use crate::palette::{self, Census, Palette, Rgb};
use crate::perceptual;
use crate::raster::{Frame, Raster};
use crate::regions::{self, NONE, TraceOptions};
use crate::{DigitizeError, Fidelity, Mode, outline};
use openstitch_core::Thread;
use openstitch_design::fmath::Det;
use openstitch_design::{DesignObject, RegionSewing, RegionType};

#[derive(Clone, Debug, PartialEq)]
pub struct AutoPunch {
    /// Thread colours to reduce to (1–64), or 0 for the automatic count PE-DESIGN 11.50 fills in
    /// from the image ([`Census::auto_colors`]; the default).
    pub colors: usize,
    /// Longest side of the working raster, pixels.
    pub resolution: u32,
    /// Small-region parameter, mm²: a 4-connected region of at most
    /// `max(4, round(min_area_mm2 / mmPerPx²))` working pixels is recoloured into a neighbour
    /// (#127's `max(4, round(param · 100 / mmPerPx²))` with `param` in cm²; see
    /// [`small_region_floor`]).
    pub min_area_mm2: f64,
    /// Extra 3×3 majority-filter passes on the thread labels after the modal colour census
    /// (0: the census's own 3×3 modal filter is the only smoothing).
    pub smooth: u32,
    pub trace: TraceOptions,
    /// Regions whose stroke width (twice the deepest inscribed distance) is at most this are
    /// satin across the stroke, mm; wider regions are fill. PE-DESIGN 11.50 keeps 6.1 mm bars
    /// satin and fills 7.1 mm ones (`autopunch-thin-strokes-wide.pes`): default 6.5.
    pub satin_max_mm: f64,
    /// Strokes narrower than this are fill, not satin, mm. PE-DESIGN 11.50 fills a 0.5 mm bar
    /// and satins a 0.75 mm one (`autopunch-thin-strokes.pes`): default 0.7.
    pub satin_min_mm: f64,
    /// Leave out the colour covering most of the image border (the background).
    pub skip_background: bool,
    /// OpenStitch mode, with `skip_background`: the fabric colour. The background is then left
    /// out only when it is the fabric's colour (within [`FABRIC_DE`]); any other background is
    /// part of the picture, and is sewn. `None` leaves the background out whatever its colour.
    pub fabric: Option<Rgb>,
    /// OpenStitch mode: strokes of the darkest colour at most this wide, mm, are sewn as satin
    /// outlines along their skeleton, on top of the regions beside them
    /// ([`crate::outline`]; 0: never).
    pub outline_max_mm: f64,
    /// PE-DESIGN's colour reduction, or OpenStitch's perceptual one
    /// ([`Palette::from_census_perceptual`]).
    pub mode: Mode,
    /// OpenStitch mode: the threads to choose from (B16.1 T5: other brands' charts, the user's
    /// own threads); empty is the Brother PEC chart. PE-DESIGN mode ignores it.
    pub threads: Vec<Thread>,
    /// OpenStitch mode's fidelity: High lets the automatic colour count reach
    /// [`crate::HIGH_COLORS`]. PE-DESIGN mode ignores it.
    pub fidelity: Fidelity,
}

impl Default for AutoPunch {
    fn default() -> Self {
        AutoPunch {
            colors: 0,
            resolution: 400,
            min_area_mm2: 1.0,
            smooth: 0,
            trace: TraceOptions::default(),
            satin_max_mm: 6.5,
            satin_min_mm: 0.7,
            skip_background: true,
            fabric: None,
            outline_max_mm: 0.0,
            mode: Mode::PeDesign,
            threads: Vec::new(),
            fidelity: Fidelity::Standard,
        }
    }
}

/// The fill directions Auto Punch chooses from, degrees counter-clockwise on screen, in order
/// of preference.
pub const FILL_ANGLES: [f64; 4] = [90.0, 45.0, 135.0, 0.0];
/// A later direction in [`FILL_ANGLES`] wins only when its rows spread at least this much
/// further, mm: one fill row at PE-DESIGN's 4.5 lines/mm.
pub const FILL_ROW_MM: f64 = 1.0 / 4.5;

/// The fill direction PE-DESIGN 11.50's Auto Punch gives a region: of [`FILL_ANGLES`], the one
/// whose rows spread furthest across the region (its extent perpendicular to the stitches, so
/// the most rows), where a later direction must add at least one row ([`FILL_ROW_MM`]) to win.
/// On the round-4 captures this gives every fill's direction:
/// - 30 mm bars 7–12 mm wide at 90° and 14 mm wide at 45°;
/// - squares and rectangles at 45°;
/// - the isosceles triangle at 90° and the right triangle at 45°;
/// - the discs at 90°, except one whose diagonal spread is 0.5 mm wider, at 45°. A disc 0.2 mm
///   wider stays at 90°.
pub fn fill_angle(m: &[bool], mw: usize, mh: usize, mm_per_px: f64) -> f64 {
    let margin = if mm_per_px > 0.0 {
        FILL_ROW_MM / mm_per_px
    } else {
        0.0
    };
    let mut best: Option<(f64, f64)> = None;
    for &deg in &FILL_ANGLES {
        let a = deg.to_radians();
        // Across the stitches, in pixel coordinates (y down): a counter-clockwise stitch
        // direction (cos a, −sin a) has the perpendicular (sin a, cos a).
        let (px, py) = (a.dsin(), a.dcos());
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for y in 0..mh {
            for x in 0..mw {
                if m[y * mw + x] {
                    let v = x as f64 * px + y as f64 * py;
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            }
        }
        // Whole pixels: a pixel spans |px| + |py| across.
        let extent = hi - lo + px.abs() + py.abs();
        if best.is_none_or(|b| extent >= b.0 + margin) {
            best = Some((extent, deg));
        }
    }
    best.map_or(FILL_ANGLES[0], |b| b.1)
}

/// Most regions one conversion may produce.
pub const MAX_REGIONS: usize = 5000;

#[derive(Clone, Debug, PartialEq)]
pub struct Converted {
    /// Shape objects in sewing order (ids unassigned).
    pub objects: Vec<DesignObject>,
    /// Threads in sewing order.
    pub threads: Vec<Thread>,
    /// The background thread left out, if any.
    pub background: Option<Thread>,
}

/// Labels per working pixel from the census's modal colours: thread index into
/// `palette.threads`, or [`NONE`] where the pixel is transparent.
pub fn label(census: &Census, palette: &Palette) -> Vec<u16> {
    census
        .modal
        .iter()
        .map(|m| m.map_or(NONE, |c| palette.classify(c) as u16))
        .collect()
}

/// The small-region floor in pixels (#127): `max(4, round(param · 100 / mmPerPx²))` with the
/// parameter given in mm² (`param` in cm², so the `· 100` is the unit change) and `mm_per_px`
/// the working raster's pixel size, `max(width_mm / width_px, height_mm / height_px)`.
/// Regions of at most this many 4-connected pixels are removed.
pub fn small_region_floor(min_area_mm2: f64, mm_per_px: f64) -> usize {
    if mm_per_px <= 0.0 || !min_area_mm2.is_finite() {
        return 4;
    }
    let px = (min_area_mm2.max(0.0) / (mm_per_px * mm_per_px)).round();
    (px.min(1e9) as usize).max(4)
}

/// The label covering more than half of the border pixels, if any.
pub fn border_majority(labels: &[u16], w: usize, h: usize) -> Option<u16> {
    let mut counts: Vec<(u16, usize)> = Vec::new();
    let mut total = 0;
    for y in 0..h {
        for x in 0..w {
            if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                total += 1;
                let l = labels[y * w + x];
                match counts.iter_mut().find(|c| c.0 == l) {
                    Some(c) => c.1 += 1,
                    None => counts.push((l, 1)),
                }
            }
        }
    }
    counts
        .into_iter()
        .filter(|c| c.0 != NONE && c.1 * 2 > total)
        .map(|c| c.0)
        .next()
}

/// OpenStitch mode: a region under the minimum area is kept as a piece of a line when its mean
/// width (area over its longest side) is at most this, mm, and it is at least twice as long.
pub const LINE_WIDTH_MM: f64 = 0.5;

/// OpenStitch mode's rows per mm in fills and satins: the sparsest that still covers the
/// fabric with OpenStitch's fill (fewer rows leave gaps a thread wide).
pub const OPEN_STITCH_DENSITY: f64 = 4.5;

/// OpenStitch mode's longest fill stitch, mm: long enough to keep a fill near 0.7 needle
/// points per mm² inside, short of the 7 mm sewability limit.
pub const OPEN_STITCH_FILL_STITCH_MM: f64 = 6.5;

/// OpenStitch mode sews a region as satin only when it is a stroke, at least this many times
/// as long as it is wide (area over width²); a compact one is filled.
pub const SATIN_ASPECT: f64 = 3.0;

/// OpenStitch mode treats an image as line art when the border's colour (the background, left
/// out or sewn) covers at least this share of the opaque pixels (the mandala about 0.9; photos and paintings far less).
pub const LINE_ART_SHARE: f64 = 0.75;

/// OpenStitch mode: a region keeps only the parts at least this wide, mm (a morphological
/// opening, [`open_slivers`]); thinner spikes and necks of a larger region stacked needle drops
/// on the machine (#290: 29 in one 0.5 mm spot on the storybook picture).
pub const SLIVER_MM: f64 = 1.25;

/// The opening's radius in working pixels for a pixel `mm_per_px` wide: a square of
/// 2 r + 1 pixels at least [`SLIVER_MM`] across, at least 1.
fn sliver_radius(mm_per_px: f64) -> usize {
    ((SLIVER_MM / mm_per_px - 1.0) / 2.0).ceil().max(1.0) as usize
}

/// Removes every spike and neck narrower than a (2 r + 1)-pixel square from the regions of
/// `labels` (a morphological opening per label: a pixel stays when some such square around
/// it is all its own label). A region with nothing that wide is a stroke and is kept whole,
/// unless `hairlines` goes and nothing in it is even 3 pixels wide (a satin that narrow sews
/// as a run of stitches in place); the removed pixels take the label most of their remaining
/// neighbours have.
pub fn open_slivers(labels: &mut [u16], w: usize, h: usize, r: usize, hairlines: bool) {
    let at = |x: isize, y: isize, l: &[u16]| {
        (x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h)
            .then(|| l[y as usize * w + x as usize])
    };
    // The image's edge does not cut a region: the square is clamped to the image.
    let own = |x: isize, y: isize, l: &[u16]| {
        l[(y.clamp(0, h as isize - 1) as usize) * w + x.clamp(0, w as isize - 1) as usize]
    };
    let r = r as isize;
    // Erosion: the pixels whose whole (2 r + 1)-square is their own label.
    let eroded = |r: isize| -> Vec<bool> {
        (0..w * h)
            .map(|i| {
                let (x, y, l) = ((i % w) as isize, (i / w) as isize, labels[i]);
                l != NONE && (-r..=r).all(|dy| (-r..=r).all(|dx| own(x + dx, y + dy, labels) == l))
            })
            .collect()
    };
    let core = eroded(r);
    let thick = if hairlines { eroded(1) } else { Vec::new() };
    // Dilation: a pixel stays when a core pixel of its label lies within the square.
    let keep: Vec<bool> = (0..w * h)
        .map(|i| {
            let (x, y, l) = ((i % w) as isize, (i / w) as isize, labels[i]);
            l == NONE
                || (-r..=r).any(|dy| {
                    (-r..=r).any(|dx| {
                        let (nx, ny) = (
                            (x + dx).clamp(0, w as isize - 1),
                            (y + dy).clamp(0, h as isize - 1),
                        );
                        own(nx, ny, labels) == l && core[ny as usize * w + nx as usize]
                    })
                })
        })
        .collect();
    // Strokes (a component with no core pixel at all) stay whole.
    let (id, comps) = regions::components(labels, w, h);
    let mut has_core = vec![false; comps.len()];
    let mut has_thick = vec![!hairlines; comps.len()];
    for i in 0..w * h {
        if id[i] != u32::MAX {
            has_core[id[i] as usize] |= core[i];
            has_thick[id[i] as usize] |= hairlines && thick[i];
        }
    }
    let mut todo: Vec<usize> = (0..w * h)
        .filter(|&i| {
            id[i] != u32::MAX
                && ((!keep[i] && has_core[id[i] as usize]) || !has_thick[id[i] as usize])
        })
        .collect();
    let mut gone = vec![false; w * h];
    for &i in &todo {
        gone[i] = true;
    }
    // Hand the removed pixels to their neighbours, from the outside in.
    for _ in 0..(4 * r as usize + 8) {
        if todo.is_empty() {
            break;
        }
        let mut next = Vec::new();
        let mut set = Vec::new();
        for &i in &todo {
            let (x, y) = ((i % w) as isize, (i / w) as isize);
            let mut votes: Vec<(u16, u32)> = Vec::new();
            for (dx, dy) in [
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ] {
                let (nx, ny) = (x + dx, y + dy);
                if let Some(l) = at(nx, ny, labels)
                    && l != NONE
                    && l != labels[i]
                    && !gone[ny as usize * w + nx as usize]
                {
                    match votes.iter_mut().find(|v| v.0 == l) {
                        Some(v) => v.1 += 1,
                        None => votes.push((l, 1)),
                    }
                }
            }
            match votes.iter().max_by_key(|v| (v.1, std::cmp::Reverse(v.0))) {
                Some(&(l, _)) => set.push((i, l)),
                None => next.push(i),
            }
        }
        for (i, l) in set {
            labels[i] = l;
            gone[i] = false;
        }
        todo = next;
    }
}

/// OpenStitch mode with a fabric colour ([`AutoPunch::fabric`]) leaves the background (the
/// border's majority colour) out only when it is within this CIEDE2000 of the fabric: that is
/// the fabric showing. A sky, a cream card or any other coloured background is sewn.
pub const FABRIC_DE: f64 = 5.0;

pub fn convert(image: &Raster, frame: &Frame, p: &AutoPunch) -> Result<Converted, DigitizeError> {
    let work = image.fit_mode(p.resolution.clamp(16, 2048));
    let frame = frame.rescaled(image.width(), work.width());
    let (w, h) = (work.width() as usize, work.height() as usize);
    let census = palette::census(&work);
    if census.colors.is_empty() {
        return Err(DigitizeError::Empty(
            "the image is fully transparent".into(),
        ));
    }
    let colors = match p.colors {
        0 => match p.mode {
            Mode::PeDesign => census.auto_colors(),
            Mode::OpenStitch => census.auto_colors_up_to(p.fidelity.auto_max()),
        },
        n => n.min(64),
    };
    let palette = match p.mode {
        Mode::PeDesign => Palette::from_census(&census, colors),
        Mode::OpenStitch => Palette::from_census_perceptual_in(&census, colors, &p.threads),
    };
    let mut labels = label(&census, &palette);
    // OpenStitch mode: pixels of the border's majority colour, sewn or not.
    let mut border_pixels = 0;
    let background = match (p.skip_background, p.mode) {
        (false, _) => None,
        (true, Mode::PeDesign) => {
            let bg = border_majority(&labels, w, h);
            if let Some(bg) = bg {
                for l in &mut labels {
                    if *l == bg {
                        *l = NONE;
                    }
                }
            }
            bg
        }
        (true, Mode::OpenStitch) => {
            // Only the background colour's own pixels are left out, not every colour that
            // shares its thread: a thin line's light-grey edge shades sew in the white thread
            // rather than vanish with the white background (round-5 mandala, g11).
            let means: Vec<u16> = census
                .modal
                .iter()
                .map(|m| m.map_or(NONE, |c| palette.nearest_mean(c) as u16))
                .collect();
            let bg = border_majority(&means, w, h);
            if let Some(bg) = bg {
                border_pixels = means.iter().filter(|&&m| m == bg).count();
            }
            // With a fabric colour, a background of another colour is part of the picture, and
            // is sewn.
            let bg = bg.filter(|&m| {
                p.fabric.is_none_or(|f| {
                    let bg = perceptual::lab(palette.means[m as usize]);
                    perceptual::de2000(bg, perceptual::lab(f)) <= FABRIC_DE
                })
            });
            if let Some(bg) = bg {
                for (l, m) in labels.iter_mut().zip(&means) {
                    if *m == bg {
                        *l = NONE;
                    }
                }
            }
            bg.map(|m| palette.thread_of[m as usize] as u16)
        }
    };
    let unsmoothed = labels.clone();
    regions::majority(&mut labels, w, h, p.smooth.min(5));
    // The frame scales x and y alike, so the working pixel's mm/px is both ratios.
    let floor = small_region_floor(p.min_area_mm2, frame.mm_per_px());
    match p.mode {
        Mode::PeDesign => regions::merge_small(&mut labels, w, h, floor + 1),
        Mode::OpenStitch => {
            // On line art (the background colour covers at least [`LINE_ART_SHARE`] of the
            // image) only compact specks merge: a small region whose mean width (area over its
            // longest side) is at most [`LINE_WIDTH_MM`] is a piece of a line, and stays. On
            // photos and paintings such slivers are edge shades, and merge.
            let opaque = census.modal.iter().filter(|m| m.is_some()).count().max(1);
            let line_art = border_pixels as f64 >= LINE_ART_SHARE * opaque as f64;
            let mm = frame.mm_per_px();
            regions::merge_small_except(&mut labels, w, h, floor + 1, |k| {
                let long = (k.x1 - k.x0).max(k.y1 - k.y0) as f64 * mm;
                line_art
                    && long >= 2.0 * LINE_WIDTH_MM
                    && k.area as f64 * mm * mm / long <= LINE_WIDTH_MM
            });
        }
    }
    // OpenStitch mode: the dark contour lines become satin lines sewn on top (T12).
    let outlines = match p.mode {
        Mode::OpenStitch => outline::extract(
            &mut labels,
            w,
            h,
            &palette,
            &frame,
            p.outline_max_mm,
            &p.trace,
        ),
        Mode::PeDesign => None,
    };
    // OpenStitch mode, after the outlines are taken out: spikes and necks thinner than about 1.25 mm on a larger region go to
    // their neighbours (#290): rows or satin crossing them land their ends on one spot.
    if p.mode == Mode::OpenStitch {
        let opaque = census.modal.iter().filter(|m| m.is_some()).count().max(1);
        let line_art = border_pixels as f64 >= LINE_ART_SHARE * opaque as f64;
        open_slivers(
            &mut labels,
            w,
            h,
            sliver_radius(frame.mm_per_px()),
            !line_art,
        );
    }
    let (mut id, mut comps) = regions::components(&labels, w, h);
    if comps.is_empty() && p.mode == Mode::OpenStitch && outlines.is_none() {
        // OpenStitch mode's cleanup can merge away a drawing made only of thin lines (the
        // round-5 mandala); keep PE-DESIGN's cleanup for it rather than sew nothing.
        labels = unsmoothed;
        let floor = small_region_floor(AutoPunch::default().min_area_mm2, frame.mm_per_px());
        regions::merge_small(&mut labels, w, h, floor + 1);
        (id, comps) = regions::components(&labels, w, h);
    }
    if comps.is_empty() {
        return Err(DigitizeError::Empty(
            "no regions left after removing the background".into(),
        ));
    }
    if comps.len() > MAX_REGIONS {
        return Err(DigitizeError::TooLarge(format!(
            "{} regions (limit {MAX_REGIONS}); raise the minimum area or lower the resolution",
            comps.len()
        )));
    }
    // Colours in PE-DESIGN's order (brightest channel sum first); within a colour, regions
    // in scan order of their first pixel, reversed for the first, third, … colour sewn, as
    // PE-DESIGN sews them (round-4 captures: the bars bottom first; on the specks image blue,
    // sewn first, from the bottom specks up to its disc, then red from its square down to its
    // specks).
    let mut area = vec![0usize; palette.threads.len()];
    for c in &comps {
        area[c.label as usize] += c.area;
    }
    let order = palette::brightness_order(
        &palette,
        (0..palette.threads.len()).filter(|&t| area[t] > 0),
    );
    let mut objects = Vec::new();
    for (colour, &t) in order.iter().enumerate() {
        let thread = palette.threads[t].clone();
        let mut mine: Vec<(usize, &regions::Component)> = comps
            .iter()
            .enumerate()
            .filter(|(_, k)| k.label as usize == t)
            .collect();
        if colour.is_multiple_of(2) {
            mine.reverse();
        }
        for (c, k) in mine {
            let (m, mw, mh) = regions::mask(&id, w, c as u32, k);
            let contours = regions::trace_mask(&m, mw, mh, k.x0, k.y0, &p.trace, &frame);
            if contours.is_empty() {
                continue;
            }
            let axis = regions::principal_axis(&m, mw, mh);
            let across = normalize(axis + 90.0);
            // Centerline test: a region is a stroke (satin across it) when its width at its
            // deepest point is within the satin width; one that keeps area beyond it is fill.
            let width_mm = regions::thickness(&m, mw, mh) * frame.mm_per_px();
            // OpenStitch mode keeps satin for strokes: a compact shape (a dot, a pupil) sewn
            // in one satin direction shows long, uneven stitches, so it is filled.
            let stroke = match p.mode {
                Mode::PeDesign => true,
                Mode::OpenStitch => {
                    let w_px = width_mm / frame.mm_per_px();
                    k.area as f64 >= SATIN_ASPECT * w_px * w_px
                }
            };
            let satin = stroke && (p.satin_min_mm..=p.satin_max_mm).contains(&width_mm);
            let mut region = RegionSewing::new(
                if satin {
                    RegionType::Satin
                } else {
                    RegionType::Fill
                },
                thread.clone(),
            );
            // As PE-DESIGN 11.50 sets them (round-4 captures): satin with Auto direction and
            // no under sewing, across the stroke; fill with under sewing, in the direction
            // whose rows spread furthest across the region ([`fill_angle`]). The axis is
            // measured with the design's y pointing down; region angles are stored
            // counter-clockwise on screen, as PE-DESIGN gives them.
            if satin {
                region.auto_direction = true;
                region.underlay = false;
            }
            if p.mode == Mode::OpenStitch {
                // OpenStitch mode sews one covering layer and no more needle points than it
                // needs (owner, 2026-10-31: a normal single-layer fill is within the density
                // limits): rows at 4.5 per mm, fill stitches up to 6.5 mm, no under sewing.
                region.density = OPEN_STITCH_DENSITY;
                region.stitch_length = OPEN_STITCH_FILL_STITCH_MM;
                region.underlay = false;
            }
            region.angle = if satin {
                normalize(-across) + 0.0
            } else {
                fill_angle(&m, mw, mh, frame.mm_per_px())
            };
            let mut o = DesignObject::shape(contours, Some(region), None);
            o.name = Some(format!(
                "Auto Punch {} {}",
                objects.len() + 1,
                thread.name.as_deref().unwrap_or("thread")
            ));
            objects.push(o);
        }
    }
    // The outlines are sewn last, on the regions they border; their ink is the darkest
    // thread, so normally the colour sewn last already.
    let mut threads: Vec<Thread> = order.iter().map(|&t| palette.threads[t].clone()).collect();
    if let Some(o) = outlines {
        let ink = palette.threads[o.thread].clone();
        if order.last() != Some(&o.thread) {
            threads.push(ink.clone());
        }
        for mut line in o.lines {
            line.name = Some(format!(
                "Auto Punch {} {} outline",
                objects.len() + 1,
                ink.name.as_deref().unwrap_or("thread")
            ));
            objects.push(line);
        }
    }
    if objects.is_empty() {
        return Err(DigitizeError::Empty(
            "every region was too small to trace".into(),
        ));
    }
    Ok(Converted {
        objects,
        threads,
        background: background.map(|b| palette.threads[b as usize].clone()),
    })
}

/// Angle in (-90, 90].
fn normalize(deg: f64) -> f64 {
    let mut d = deg % 180.0;
    if d <= -90.0 {
        d += 180.0;
    }
    if d > 90.0 {
        d -= 180.0;
    }
    d
}
