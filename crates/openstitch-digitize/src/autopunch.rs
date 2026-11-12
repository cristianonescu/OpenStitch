//! Auto Punch approximation: image → 3×3 modal colour census (at most 100 tracked colours)
//! → N PEC threads → regions (4-connected; those of at most the small-region floor removed)
//! → traced outlines → shape objects, satin where the region is a stroke no wider than the
//! satin width, fill elsewhere.
//!
//! Specified from PE-DESIGN 11's public guide (Auto Punch converts an image's colour areas
//! into embroidery regions with chosen colours omitted) and the Phase 6 gap analysis (#127:
//! the modal census, the `max(4, round(param · 100 / mmPerPx²))` small-region floor at
//! 4-connectivity and a centerline-thinness satin test); the algorithm is this project's own.

use crate::grabcut;
use crate::palette::{self, Census, Palette, Rgb};
use crate::perceptual;
use crate::raster::{Frame, Raster};
use crate::regions::{self, NONE, TraceOptions};
use crate::{DigitizeError, Fidelity, Mode, columns, outline};
use openstitch_core::Thread;
use openstitch_design::fmath::Det;
use openstitch_design::{Contour, DesignObject, LineSewing, LineType, RegionSewing, RegionType};

#[derive(Clone, Debug, PartialEq)]
pub struct AutoPunch {
    /// Thread colours to reduce to (1–64), or 0 for the automatic count PE-DESIGN 11.50 fills in
    /// from the image ([`Census::auto_colors`]; the default).
    pub colors: usize,
    /// Longest side of the working raster, pixels. PE-DESIGN mode traces flat graphics (at most
    /// [`FLAT_COLORS`] automatic colours) at twice this.
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
    /// OpenStitch mode: isolate the subject by an iterated graph cut ([`crate::grabcut`],
    /// #127) and leave out everything around it, whatever its colour, in place of the
    /// border-colour background rule. `None` (the default) is off. PE-DESIGN mode ignores it.
    pub isolate: Option<grabcut::Seed>,
    /// With `isolate`: the user's subject and background strokes (#350).
    pub marks: grabcut::Marks,
    /// With `isolate`: a click on the subject (`[x, y]`, fractions of the image), which keeps
    /// only the part of the subject holding it ([`grabcut::isolate_at`], #490); the border is
    /// then the background the cut learns from.
    pub subject_point: Option<[f64; 2]>,
    /// Per-region choices that replace Auto Punch's own (#489), by region number in sewing
    /// order ([`Region::id`], as a run with the same options numbers them).
    pub overrides: Vec<Override>,
    /// How fills choose their direction (#489).
    pub angle_rule: AngleRule,
    /// OpenStitch mode: each fill reaches this far, mm, under the touching regions sewn after
    /// it, so no fabric shows along the seam where two colours meet (#469; 0 = off, the
    /// default; at most [`MAX_UNDERLAP_MM`]). PE-DESIGN mode ignores it.
    pub underlap_mm: f64,
    /// The colours to sew first, in this order, by thread colour (`0xRRGGBB`, #489); the
    /// others follow in PE-DESIGN's order. Empty (the default) is PE-DESIGN's order. A colour
    /// the conversion does not sew is passed over and listed in [`Converted::order_ignored`]
    /// (the threads change with the mode and colour count); one listed twice is an error.
    pub order: Vec<u32>,
    /// OpenStitch mode: each fill's rows follow the direction of the picture's own strokes
    /// ([`crate::flow`], #538) instead of one angle; a fill whose angle is set keeps it. Off
    /// (the default) and PE-DESIGN mode sew straight rows.
    pub flow_fill: bool,
    /// OpenStitch mode: a satin stroke that branches (a Y, a K, a star, a forked line) is sewn
    /// as one satin column per arm, overlapping [`BRANCH_JOIN_MM`] where they meet
    /// (`openstitch_design::skeleton::branches`, #702), instead of one satin across the whole
    /// shape. Off (the default) and PE-DESIGN mode sew it as before.
    pub split_branches: bool,
}

/// How far a branch column reaches into the column it leaves, mm (#702).
pub const BRANCH_JOIN_MM: f64 = 0.3;

/// A column's typical width, mm: the median of its widths (#702).
fn median_width(s: &openstitch_design::skeleton::Skeleton) -> f64 {
    let mut w = s.widths.clone();
    if w.is_empty() {
        return 0.0;
    }
    w.sort_by(f64::total_cmp);
    w[w.len() / 2]
}

/// Longest [`AutoPunch::underlap_mm`], mm.
pub const MAX_UNDERLAP_MM: f64 = 1.5;

/// How a traced region is sewn (#489).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stitch {
    Fill,
    Satin,
    /// A running line along the region's skeleton (or a satin outline, for the dark
    /// contour lines OpenStitch mode takes out).
    Line,
    /// Left out.
    Skip,
}

impl Stitch {
    pub const ALL: [Stitch; 4] = [Stitch::Fill, Stitch::Satin, Stitch::Line, Stitch::Skip];

    pub fn name(self) -> &'static str {
        match self {
            Stitch::Fill => "fill",
            Stitch::Satin => "satin",
            Stitch::Line => "line",
            Stitch::Skip => "skip",
        }
    }

    pub fn parse(s: &str) -> Option<Stitch> {
        Stitch::ALL
            .into_iter()
            .find(|k| k.name().eq_ignore_ascii_case(s))
    }
}

/// Why a region got its stitch (#489), as a code with an English sentence ([`Region::why`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// A stroke within the satin widths.
    SatinWidth,
    /// Wider than the satin maximum.
    Wide,
    /// Narrower than the satin minimum.
    Narrow,
    /// OpenStitch mode: compact, not a stroke.
    Compact,
    /// A satin whose stitches would be too long.
    LongSatin,
    /// OpenStitch mode: a dark contour line, sewn as a satin outline on top.
    Outline,
    /// OpenStitch mode: under [`outline::THIN_MM`] wide, a running line.
    ThinLine,
    /// Set by an override.
    Override,
    /// OpenStitch mode, not on line art: every stitch would fall within [`SPECK_MM`], a knot
    /// rather than a shape; left out (#690).
    Speck,
    /// OpenStitch mode, line art: a drawn dot (every stitch within [`SPECK_MM`]) sewn as a
    /// fill without ties of its own, so it is not a knot (#713).
    Dot,
    /// OpenStitch mode with `split_branches`: a branching stroke, one satin column per arm
    /// (#702).
    Branches,
}

impl Reason {
    pub fn code(self) -> &'static str {
        match self {
            Reason::SatinWidth => "satin_width",
            Reason::Wide => "wide",
            Reason::Narrow => "narrow",
            Reason::Compact => "compact",
            Reason::LongSatin => "long_satin",
            Reason::Outline => "outline",
            Reason::ThinLine => "thin_line",
            Reason::Override => "override",
            Reason::Speck => "speck",
            Reason::Dot => "dot",
            Reason::Branches => "branches",
        }
    }
}

/// One traced region of a conversion and how it is sewn (#489).
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    /// Number in sewing order, what an [`Override`] names.
    pub id: usize,
    pub thread: Thread,
    pub area_mm2: f64,
    /// Stroke width, mm (twice the deepest inscribed distance; 0 for lines).
    pub width_mm: f64,
    pub stitch: Stitch,
    pub reason: Reason,
    /// Fill or fixed satin direction, degrees counter-clockwise on screen.
    pub angle: Option<f64>,
}

impl Region {
    /// The reason as a sentence.
    pub fn why(&self, p: &AutoPunch) -> String {
        let w = self.width_mm;
        match self.reason {
            Reason::SatinWidth => format!(
                "{w:.1} mm wide, within the satin widths {:.1}-{:.1} mm: satin",
                p.satin_min_mm, p.satin_max_mm
            ),
            Reason::Wide => format!(
                "{w:.1} mm wide, over the satin maximum {:.1} mm: fill",
                p.satin_max_mm
            ),
            Reason::Narrow => format!(
                "{w:.1} mm wide, under the satin minimum {:.1} mm: fill",
                p.satin_min_mm
            ),
            Reason::Compact => {
                format!("a compact shape, not a stroke {SATIN_ASPECT} times as long as wide: fill")
            }
            Reason::LongSatin => "satin stitches would be over 10.5 mm long: fill".to_string(),
            Reason::Outline => "a dark contour line: satin outline sewn on top".to_string(),
            Reason::ThinLine => format!("under {} mm wide: running line", outline::THIN_MM),
            Reason::Override => format!("set to {} by an override", self.stitch.name()),
            Reason::Speck => {
                format!("every stitch within {SPECK_MM} mm, a knot rather than a shape: left out")
            }
            Reason::Branches => format!(
                "a branching stroke whose arms are each within the satin maximum {:.1} mm: one \
                 satin column per arm",
                p.satin_max_mm
            ),
            Reason::Dot => format!(
                "a drawn dot, every stitch within {SPECK_MM} mm: fill, tied only where a trim cuts it"
            ),
        }
    }
}

/// A user's choice for one region (#489).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Override {
    pub region: usize,
    /// The stitch, or `None` to keep Auto Punch's.
    pub stitch: Option<Stitch>,
    /// Fill or satin direction, degrees counter-clockwise on screen.
    pub angle: Option<f64>,
}

/// How fills choose their direction (#489).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AngleRule {
    /// PE-DESIGN's rule, [`fill_angle`] (the default).
    #[default]
    Pe,
    /// [`fill_angle`], turned (by up to 90°, [`vary_angle`]) away from the touching fills sewn
    /// before it when it lies within [`NEIGHBOUR_DEG`] of one, so fitted panels read apart;
    /// fills under [`VARY_MIN_MM2`] keep [`fill_angle`].
    VaryNeighbours,
    /// Of [`TRAVEL_ANGLES`], the direction whose sewn path is shortest (#489): the rows cover
    /// the same area whichever way they run, so the difference is the travel between a
    /// concave region's sections. [`fill_angle`] keeps the region unless another direction
    /// saves at least [`TRAVEL_SAVING`].
    LeastTravel,
}

/// [`AngleRule::LeastTravel`] leaves fills smaller than this, mm², at [`fill_angle`]: they
/// have little travel to save, and comparing every direction on the many small regions of a
/// photo took six times as long as the conversion (p03).
pub const TRAVEL_MIN_MM2: f64 = 10.0;

/// The directions [`AngleRule::LeastTravel`] compares, degrees: every 15°.
pub const TRAVEL_ANGLES: [f64; 12] = [
    0.0, 15.0, 30.0, 45.0, 60.0, 75.0, 90.0, 105.0, 120.0, 135.0, 150.0, 165.0,
];

/// [`AngleRule::LeastTravel`] turns a fill away from [`fill_angle`] only to save at least this
/// much thread, 0.1 mm (1 mm).
pub const TRAVEL_SAVING: f64 = 10.0;

/// With [`AngleRule::VaryNeighbours`]: least angle between touching fills, degrees.
pub const NEIGHBOUR_DEG: f64 = 20.0;

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
            isolate: None,
            marks: grabcut::Marks::default(),
            subject_point: None,
            overrides: Vec::new(),
            angle_rule: AngleRule::Pe,
            underlap_mm: 0.0,
            order: Vec::new(),
            flow_fill: false,
            split_branches: false,
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

/// An image of at most this many automatic colours at the working resolution is flat graphics
/// (lettering, logos, clip art): PE-DESIGN-compatible mode traces it at twice the working
/// resolution and sews its satins as straight bars or curved columns, and both modes label it
/// keeping shades ([`convert`], #443).
pub const FLAT_COLORS: usize = 5;

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
    /// Every traced region and how it is sewn, in sewing order (#489).
    pub regions: Vec<Region>,
    /// The region ([`Region::id`]) of every working pixel, `u32::MAX` where none is (the
    /// background, thin lines and outlines), row by row, [`Converted::map_size`] pixels: what
    /// the image dialog's click-to-fix looks up (#489).
    pub region_map: Vec<u32>,
    /// Width and height of [`Converted::region_map`], working pixels.
    pub map_size: (usize, usize),
    /// The colours of [`AutoPunch::order`] this conversion does not sew.
    pub order_ignored: Vec<u32>,
}

/// Labels per working pixel from the census's modal colours with their shades kept
/// ([`Census::labels`]): thread index into `palette.threads`, or [`NONE`] where the pixel is
/// transparent.
pub fn label(census: &Census, palette: &Palette) -> Vec<u16> {
    census
        .labels
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

/// OpenStitch mode keeps thin features as lines (#469) on pictures of at most this many
/// colours (graphics: g02's balloon with its basket's brown, 7). On photographs (p03 and d01,
/// 10) such slivers are shading, and as lines they added 3-4 colour changes for no likeness.
pub const THIN_LINE_COLORS: usize = 7;

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

/// A background at least this light (CIELAB L*) …
pub const PAPER_L: f64 = 88.0;
/// … and at most this colourful (CIELAB chroma) is paper (white, cream, off-white scans): on a
/// light fabric it is the fabric showing, not a picture to sew (#445: g08's cream ground,
/// ΔE00 6-7 from white, was sewn as a 100 × 67 mm fill and fused the letters).
pub const PAPER_CHROMA: f64 = 15.0;

/// Whether a background of colour `bg` is fabric `fabric` showing: within [`FABRIC_DE`] of it,
/// or paper ([`PAPER_L`], [`PAPER_CHROMA`]) on a fabric that is itself paper-light.
pub fn is_fabric(bg: Rgb, fabric: Rgb) -> bool {
    let (b, f) = (perceptual::lab(bg), perceptual::lab(fabric));
    let paper = |l: perceptual::Lab| l[0] >= PAPER_L && l[1].dhypot(l[2]) <= PAPER_CHROMA;
    perceptual::de2000(b, f) <= FABRIC_DE || (paper(b) && paper(f))
}

pub fn convert(image: &Raster, frame: &Frame, p: &AutoPunch) -> Result<Converted, DigitizeError> {
    let mut work = image.fit_mode(p.resolution.clamp(16, 2048));
    let mut census = palette::census(&work);
    // PE-DESIGN-compatible mode traces flat graphics (at most FLAT_COLORS automatic colours)
    // at twice the working size: lettering's 0.1-0.3 mm gaps and straight edges survive, and
    // the edge shades a box-mode copy makes no longer count as a colour (g08 at 400 px: a grey
    // fourth colour and the S and the T fused; at its own 768 px: 3 colours and every letter
    // apart, as PE-DESIGN's own Auto Punch, #443). The threads are still chosen on the working
    // size's census, on which the reduction was calibrated (chosen at 800 px, the teapot's
    // black outline became a dark green). Photographs and paintings stay at the working size
    // (at 800 px the storybook picture's orange thread gave way to a grey, #447).
    let flat = census.auto_colors() <= FLAT_COLORS;
    let mut base = None;
    if p.mode == Mode::PeDesign && flat {
        let fine = image.fit_mode((p.resolution.clamp(16, 2048) * 2).min(2048));
        if fine.width() > work.width() {
            base = Some(std::mem::replace(&mut census, palette::census(&fine)));
            work = fine;
        }
    }
    // Flat graphics only (#443): pixels keep the shades of their colour against a flat
    // neighbour ([`Census::labels`]), and in PE-DESIGN mode satin regions are split into
    // straight bars or sewn along their curve ([`columns`]). Photographs and paintings keep the plain modal and Auto
    // satin, which serve them better (the storybook picture, the dog and the fruit lost
    // likeness with either).
    if !flat {
        census.labels = census.modal.clone();
    }
    let frame = frame.rescaled(image.width(), work.width());
    let (w, h) = (work.width() as usize, work.height() as usize);
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
        Mode::PeDesign => Palette::from_census(base.as_ref().unwrap_or(&census), colors),
        Mode::OpenStitch => Palette::from_census_perceptual_in(&census, colors, &p.threads),
    };
    let mut labels = label(&census, &palette);
    // OpenStitch mode: pixels of the border's majority colour, sewn or not.
    let mut border_pixels = 0;
    // OpenStitch mode with `isolate`: the graph cut decides the subject; everything outside it
    // is left out, and the background thread is the one most common there.
    let subject = match (p.isolate, p.mode) {
        (Some(seed), Mode::OpenStitch) => match p.subject_point {
            Some(at) => grabcut::isolate_at(&census.modal, w, h, at, &p.marks),
            None => grabcut::isolate(&census.modal, w, h, seed, &p.marks),
        },
        _ => None,
    };
    if subject.as_ref().is_some_and(|s| !s.contains(&true)) {
        // The user asked for isolation; sewing the whole picture in its place would hide that
        // it failed (#355).
        return Err(DigitizeError::Empty(
            "the graph cut found no subject; mark the subject with a subject stroke, or remove \
             background strokes that cover it"
                .into(),
        ));
    }
    let background = match (p.skip_background, p.mode) {
        _ if subject.is_some() => {
            let subject = subject.as_deref().unwrap_or_default();
            let mut counts = vec![0usize; palette.threads.len()];
            for (l, &keep) in labels.iter_mut().zip(subject) {
                if !keep {
                    if *l != NONE {
                        counts[*l as usize] += 1;
                        border_pixels += 1;
                    }
                    *l = NONE;
                }
            }
            (0..counts.len())
                .filter(|&t| counts[t] > 0)
                .max_by_key(|&t| (counts[t], std::cmp::Reverse(t)))
                .map(|t| t as u16)
        }
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
                .labels
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
                p.fabric
                    .is_none_or(|f| is_fabric(palette.means[m as usize], f))
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
    // Line art: the background colour covers at least [`LINE_ART_SHARE`] of the picture.
    let line_art = {
        let opaque = census.modal.iter().filter(|m| m.is_some()).count().max(1);
        border_pixels as f64 >= LINE_ART_SHARE * opaque as f64
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
    // Of what it takes, parts under 1 mm wide on one ground (a balloon's ropes cut from its
    // basket) become running lines in their thread rather than vanish (#469); line art
    // keeps its thin strokes whole, so has none.
    let mut thin = Vec::new();
    if p.mode == Mode::OpenStitch {
        let before = labels.clone();
        open_slivers(
            &mut labels,
            w,
            h,
            sliver_radius(frame.mm_per_px()),
            !line_art,
        );
        if !line_art && colors <= THIN_LINE_COLORS {
            thin = outline::thin_lines(&before, &labels, w, h, &frame, &p.trace);
        }
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
    let mut order = palette::brightness_order(
        &palette,
        (0..palette.threads.len()).filter(|&t| area[t] > 0),
    );
    // The user's colours first (#489).
    let mut order_ignored = Vec::new();
    let mut k = 0;
    for (n, &rgb) in p.order.iter().enumerate() {
        if p.order[..n].contains(&rgb) {
            return Err(DigitizeError::Invalid(format!(
                "order: #{rgb:06x} is listed twice"
            )));
        }
        let Some(at) = order
            .iter()
            .position(|&t| palette.threads[t].rgb_value() == rgb)
        else {
            order_ignored.push(rgb);
            continue;
        };
        let t = order.remove(at);
        order.insert(k, t);
        k += 1;
    }
    let mm = frame.mm_per_px();
    // The picture's stroke directions, for the flow fill (#538, OpenStitch mode only).
    let tensor =
        (p.mode == Mode::OpenStitch && p.flow_fill).then(|| crate::flow::Tensor::of(&work, mm));
    // Touching regions, for the neighbour angle rule.
    let adjacent = match p.angle_rule {
        AngleRule::VaryNeighbours => adjacency(&id, w, h, comps.len()),
        AngleRule::Pe | AngleRule::LeastTravel => Vec::new(),
    };
    // Each region's place in the sewing order, for the underlap (#469).
    let underlap_px = match p.mode {
        Mode::OpenStitch if p.underlap_mm > 0.0 => {
            (p.underlap_mm.min(MAX_UNDERLAP_MM) / mm).round() as usize
        }
        _ => 0,
    };
    let mut rank = vec![usize::MAX; comps.len()];
    if underlap_px > 0 {
        let mut n = 0;
        for (colour, &t) in order.iter().enumerate() {
            let mut mine: Vec<usize> = (0..comps.len())
                .filter(|&c| comps[c].label as usize == t)
                .collect();
            if colour.is_multiple_of(2) {
                mine.reverse();
            }
            for c in mine {
                rank[c] = n;
                n += 1;
            }
        }
    }
    let mut fill_of: Vec<Option<f64>> = vec![None; comps.len()];
    let mut objects = Vec::new();
    let mut plan: Vec<Region> = Vec::new();
    // The plan region of each component, for the region map.
    let mut region_of: Vec<u32> = vec![u32::MAX; comps.len()];
    let mut threads: Vec<Thread> = Vec::new();
    let mut sew = |o: DesignObject, thread: &Thread, objects: &mut Vec<DesignObject>| {
        if threads.last() != Some(thread) {
            threads.push(thread.clone());
        }
        objects.push(o);
    };
    let override_of = |id: usize| p.overrides.iter().find(|o| o.region == id);
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
            let width_mm = regions::thickness(&m, mw, mh) * mm;
            // OpenStitch mode keeps satin for strokes: a compact shape (a dot, a pupil) sewn
            // in one satin direction shows long, uneven stitches, so it is filled.
            let stroke = match p.mode {
                Mode::PeDesign => true,
                Mode::OpenStitch => {
                    let w_px = width_mm / mm;
                    k.area as f64 >= SATIN_ASPECT * w_px * w_px
                }
            };
            let (auto, reason) = if width_mm > p.satin_max_mm {
                (Stitch::Fill, Reason::Wide)
            } else if width_mm < p.satin_min_mm {
                (Stitch::Fill, Reason::Narrow)
            } else if !stroke {
                (Stitch::Fill, Reason::Compact)
            } else {
                (Stitch::Satin, Reason::SatinWidth)
            };
            // OpenStitch mode, when asked: a branching stroke is widest at its junction, so it
            // may be planned as fill for that alone. When it splits into two or more columns,
            // each (by its median width; the junction itself is wider) within the satin maximum, it is satin, one column per arm (#702).
            let arms: Option<Vec<openstitch_design::skeleton::Skeleton>> = (p.split_branches
                && p.mode == Mode::OpenStitch
                && stroke
                && matches!(reason, Reason::SatinWidth | Reason::Wide))
            .then(|| openstitch_design::skeleton::branches(&contours, BRANCH_JOIN_MM))
            .filter(|arms| {
                arms.len() >= 2 && arms.iter().all(|a| median_width(a) <= p.satin_max_mm)
            });
            let (auto, reason) = if arms.is_some() {
                (Stitch::Satin, Reason::Branches)
            } else {
                (auto, reason)
            };
            region_of[c] = plan.len() as u32;
            let ov = override_of(plan.len());
            let set = ov.and_then(|o| o.stitch);
            let mut entry = Region {
                id: plan.len(),
                thread: thread.clone(),
                area_mm2: k.area as f64 * mm * mm,
                width_mm,
                stitch: set.unwrap_or(auto),
                reason: if set.is_some() {
                    Reason::Override
                } else {
                    reason
                },
                angle: None,
            };
            match entry.stitch {
                Stitch::Skip => {
                    plan.push(entry);
                    continue;
                }
                Stitch::Line => {
                    let mut line = LineSewing::new(LineType::Running, thread.clone());
                    line.pitch = outline::THIN_PITCH_MM;
                    if let Some(mut o) =
                        outline::skeleton_line(&m, mw, mh, (k.x0, k.y0), &frame, &p.trace, line)
                    {
                        o.name = Some(format!("Auto Punch {} line", objects.len() + 1));
                        sew(o, &thread, &mut objects);
                    }
                    plan.push(entry);
                    continue;
                }
                Stitch::Fill | Stitch::Satin => {}
            }
            let satin = entry.stitch == Stitch::Satin;
            let set_angle = ov.and_then(|o| o.angle);
            // The fill direction: the override's, PE-DESIGN's rule, or turned away from the
            // touching fills sewn before (#489).
            let fill_deg = set_angle.unwrap_or_else(|| {
                let base = fill_angle(&m, mw, mh, mm);
                match p.angle_rule {
                    AngleRule::Pe => base,
                    AngleRule::VaryNeighbours => {
                        let big = |n: usize| comps[n].area as f64 * mm * mm >= VARY_MIN_MM2;
                        let near: Vec<f64> = if big(c) {
                            adjacent[c]
                                .iter()
                                .filter(|&&n| big(n))
                                .filter_map(|&n| fill_of[n])
                                .collect()
                        } else {
                            Vec::new()
                        };
                        vary_angle(base, &near)
                    }
                    // Chosen on the region's own outline below, once it is built.
                    AngleRule::LeastTravel => base,
                }
            });
            // Flat graphics, PE-DESIGN mode (OpenStitch mode sews wide strokes as fill and its
            // thin ones as satin outlines; the split did not help it): a satin region made of
            // straight bars (T, H) is sewn bar by bar, each straight across; a curved stroke
            // (S, C) as one column through rungs square to its centre line; a single bar keeps
            // Auto satin ([`columns`], #443). A set angle keeps one satin.
            let strokes = if satin && flat && p.mode == Mode::PeDesign && set_angle.is_none() {
                columns::split(&m, mw, mh, width_mm / mm)
            } else {
                columns::Strokes::Single
            };
            // OpenStitch mode, when asked: a branching stroke as one satin column per arm, each
            // along its own centre line (#702). A stroke with one column is sewn as before.
            let branch_parts: Option<Vec<(Vec<Contour>, Sewn)>> =
                arms.filter(|_| satin && set_angle.is_none()).map(|arms| {
                    arms.into_iter()
                        .filter_map(|sk| {
                            openstitch_design::skeleton::outline(&sk)
                                .map(|c| (vec![c], Sewn::Skeleton(Box::new(sk))))
                        })
                        .collect()
                });
            let parts: Vec<(Vec<Contour>, Sewn)> = if let Some(parts) = branch_parts {
                parts
            } else {
                match strokes {
                    columns::Strokes::Bars(cols) => cols
                        .iter()
                        .map(|c| {
                            let contours =
                                regions::trace_mask(&c.mask, mw, mh, k.x0, k.y0, &p.trace, &frame);
                            (bar_outline(contours), Sewn::Bar(c.direction))
                        })
                        .filter(|(c, _)| !c.is_empty())
                        .collect(),
                    columns::Strokes::Curved => match columns::rungs(&contours) {
                        Some(rungs) => vec![(contours, Sewn::Column(rungs))],
                        None => vec![(contours, Sewn::Auto)],
                    },
                    columns::Strokes::Single => {
                        // A fill reaches under its later neighbours (#469).
                        let contours = if !satin && underlap_px > 0 {
                            let (u, uw, uh, ux, uy) = underlap(&id, w, h, c, k, underlap_px, &rank);
                            let grown = regions::trace_mask(&u, uw, uh, ux, uy, &p.trace, &frame);
                            if grown.is_empty() { contours } else { grown }
                        } else {
                            contours
                        };
                        vec![(contours, Sewn::Auto)]
                    }
                }
            };
            for (contours, sewn) in parts {
                let build = |satin: bool| -> RegionSewing {
                    let mut region = RegionSewing::new(
                        if satin {
                            RegionType::Satin
                        } else {
                            RegionType::Fill
                        },
                        thread.clone(),
                    );
                    // As PE-DESIGN 11.50 sets them (round-4 captures): satin with Auto direction
                    // and no under sewing, across the stroke; fill with under sewing, in the
                    // direction whose rows spread furthest across the region ([`fill_angle`]). The
                    // axis is measured with the design's y pointing down; region angles are stored
                    // counter-clockwise on screen, as PE-DESIGN gives them. A bar of a split region
                    // has its own fixed direction, straight across it; a curved column its rungs.
                    if satin {
                        region.auto_direction = matches!(sewn, Sewn::Auto) && set_angle.is_none();
                        region.underlay = false;
                        // A bar travels to its far end along its outline, under the rows' ends, as
                        // PE-DESIGN 11.50's Auto Punch columns of g08 do (0.5 mm inside the edge);
                        // the shortest path inside crossed the column diagonally and showed
                        // between its rows. (Auto satin travels that way by itself.)
                        if matches!(sewn, Sewn::Bar(_)) {
                            region.running_stitch_path =
                                Some(openstitch_design::RunningStitchPath::AlongOutline);
                        }
                    }
                    if p.mode == Mode::OpenStitch {
                        // OpenStitch mode sews one covering layer and no more needle points than
                        // it needs (owner, 2026-10-31: a normal single-layer fill is within the
                        // density limits): rows at 4.5 per mm, fill stitches up to 6.5 mm, no
                        // under sewing.
                        region.density = OPEN_STITCH_DENSITY;
                        region.stitch_length = OPEN_STITCH_FILL_STITCH_MM;
                        region.underlay = false;
                        // A fill in several sections sews a section joining others after them,
                        // so the travel between them is hidden under thread sewn later (#469).
                        if !satin {
                            region.running_stitch_path =
                                Some(openstitch_design::RunningStitchPath::InsideHidden);
                        }
                    }
                    region.angle = match (satin, &sewn, set_angle) {
                        (true, Sewn::Bar(d), _) => normalize(-(d + 90.0)) + 0.0,
                        (true, _, Some(a)) => normalize(a) + 0.0,
                        (true, _, None) => normalize(-across) + 0.0,
                        _ => fill_deg,
                    };
                    if satin && let Sewn::Column(rungs) = &sewn {
                        region.direction_lines = rungs.clone();
                    }
                    if satin && let Sewn::Skeleton(sk) = &sewn {
                        region.skeleton = Some((**sk).clone());
                    }
                    region
                };
                // A satin whose crossings come out longer than PE-DESIGN's Auto Punch ever sews
                // them (10.4 mm over 12 round-5 pictures; its 30 mm satin reference shows it does
                // not split them) is sewn as fill: a region that keeps area beyond the stroke
                // width falls back to one fixed direction across its principal axis, and the
                // storybook's PE-compatible picture had 30 stitches of 12-16 mm (#460).
                let mut region = build(satin);
                if satin && longest_crossing(&contours, &region) > AUTO_PUNCH_LONGEST_SATIN {
                    region = build(false);
                    entry.stitch = Stitch::Fill;
                    entry.reason = Reason::LongSatin;
                }
                if region.kind == RegionType::Fill
                    && p.angle_rule == AngleRule::LeastTravel
                    && set_angle.is_none()
                    && entry.area_mm2 >= TRAVEL_MIN_MM2
                {
                    region.angle = least_travel(&contours, &region);
                }
                // Flow fill (#538): the rows follow the picture's strokes in this region, on a
                // 1 mm grid; where it has no clear direction they keep the fill's own angle.
                if let Some(t) = &tensor
                    && region.kind == RegionType::Fill
                    && set_angle.is_none()
                {
                    region.flow_field =
                        t.region_field(&m, mw, mh, (k.x0, k.y0), &frame, region.angle);
                }
                // OpenStitch mode leaves out a region that would sew as a knot: its stitches
                // and the ties round them stack in one spot (p07: 8 stitches in 0.46 mm). Not
                // on line art, whose dots are drawn on purpose: leaving out the mandala's (g11)
                // lowered its MS-SSIM 0.653 → 0.611.
                let dot = p.mode == Mode::OpenStitch && set.is_none() && speck(&contours, &region);
                if dot && !line_art {
                    entry.stitch = Stitch::Skip;
                    entry.reason = Reason::Speck;
                    continue;
                }
                if dot {
                    entry.reason = Reason::Dot;
                }
                // Line art's hairlines: rows across one are 0.2-0.3 mm long and, with the tie-in,
                // stand in one spot (g11 with the photo preset: 7 in a row, #713). The rows turn
                // to run along it.
                if p.mode == Mode::OpenStitch
                    && line_art
                    && region.kind == RegionType::Fill
                    && set_angle.is_none()
                    && stands_in_place(&contours, &region)
                {
                    let mut turned = region.clone();
                    turned.angle = normalize(region.angle + 90.0) + 0.0;
                    if !stands_in_place(&contours, &turned) {
                        region = turned;
                    }
                }
                if region.kind == RegionType::Fill {
                    fill_of[c] = Some(region.angle);
                }
                entry.angle.get_or_insert(region.angle);
                let mut o = DesignObject::shape(contours, Some(region), None);
                if dot {
                    // Line art's dots are drawn on purpose: kept, but the fill's own tie-in and
                    // lock are left out, so a dot reached by a running connector is no knot.
                    // The writer still ties it in after a trim and locks it before one (#713).
                    o.finish = Some(openstitch_design::finish::Finish {
                        tie_count: Some(1),
                        ..Default::default()
                    });
                }
                o.name = Some(format!(
                    "Auto Punch {} {}",
                    objects.len() + 1,
                    thread.name.as_deref().unwrap_or("thread")
                ));
                sew(o, &thread, &mut objects);
            }
            plan.push(entry);
        }
        // Thin features of this colour lying on a ground sewn already (or left out) are sewn
        // with it, at no extra colour change (#469).
        let early: Vec<outline::ThinLine> = thin
            .extract_if(.., |l| {
                l.thread == t
                    && (l.ground == NONE || order[..=colour].contains(&(l.ground as usize)))
            })
            .collect();
        for l in early {
            let n = objects.len();
            if let Some(o) = place_line(
                &mut plan,
                &p.overrides,
                &thread,
                l.line,
                Reason::ThinLine,
                n,
            )? {
                sew(o, &thread, &mut objects);
            }
        }
    }
    // The other thin features after every region, colour by colour; then the outlines, sewn
    // last on the regions they border (their ink is the darkest thread, so normally the
    // colour sewn last already). Either can be left out by an override; they are no longer
    // regions, so no other override applies.
    let mut lines: Vec<(Thread, DesignObject, Reason)> = Vec::new();
    let mut thin_threads: Vec<usize> = thin.iter().map(|l| l.thread).collect();
    thin_threads.sort_unstable();
    thin_threads.dedup();
    for t in palette::brightness_order(&palette, thin_threads) {
        for l in thin.iter().filter(|l| l.thread == t) {
            lines.push((palette.threads[t].clone(), l.line.clone(), Reason::ThinLine));
        }
    }
    if let Some(o) = outlines {
        let ink = palette.threads[o.thread].clone();
        for line in o.lines {
            lines.push((ink.clone(), line, Reason::Outline));
        }
    }
    for (thread, line, reason) in lines {
        if let Some(o) = place_line(
            &mut plan,
            &p.overrides,
            &thread,
            line,
            reason,
            objects.len(),
        )? {
            sew(o, &thread, &mut objects);
        }
    }
    if let Some(o) = p.overrides.iter().find(|o| o.region >= plan.len()) {
        return Err(DigitizeError::Invalid(format!(
            "override of region {}: the image has regions 0-{}",
            o.region,
            plan.len().saturating_sub(1)
        )));
    }
    if objects.is_empty() {
        return Err(DigitizeError::Empty(
            "every region was too small to trace or left out".into(),
        ));
    }
    if p.mode == Mode::OpenStitch && line_art {
        join_small_pieces(&mut objects);
    }
    Ok(Converted {
        objects,
        threads,
        background: background.map(|b| palette.threads[b as usize].clone()),
        regions: plan,
        region_map: id
            .iter()
            .map(|&c| region_of.get(c as usize).copied().unwrap_or(u32::MAX))
            .collect(),
        map_size: (w, h),
        order_ignored,
    })
}

/// Adds a line taken out of the picture (a thin feature or an outline, `reason`) to the plan
/// as the next region, and returns it in `thread`, named as object `n + 1`, unless an override
/// leaves it out. Such a line is no longer a region: an override can only skip it.
fn place_line(
    plan: &mut Vec<Region>,
    overrides: &[Override],
    thread: &Thread,
    mut line: DesignObject,
    reason: Reason,
    n: usize,
) -> Result<Option<DesignObject>, DigitizeError> {
    let id = plan.len();
    let stitch = match overrides
        .iter()
        .find(|o| o.region == id)
        .and_then(|o| o.stitch)
    {
        None | Some(Stitch::Line) => Stitch::Line,
        Some(Stitch::Skip) => Stitch::Skip,
        Some(other) => {
            return Err(DigitizeError::Invalid(format!(
                "region {id} is a line ({}); an override can only skip it, not make it {}",
                reason.code(),
                other.name()
            )));
        }
    };
    plan.push(Region {
        id,
        thread: thread.clone(),
        area_mm2: 0.0,
        width_mm: 0.0,
        stitch,
        reason: if stitch == Stitch::Skip {
            Reason::Override
        } else {
            reason
        },
        angle: None,
    });
    if stitch == Stitch::Skip {
        return Ok(None);
    }
    if let Some(l) = line.line.as_mut() {
        l.thread = thread.clone();
    }
    line.name = Some(format!(
        "Auto Punch {} {} {}",
        n + 1,
        thread.name.as_deref().unwrap_or("thread"),
        if reason == Reason::Outline {
            "outline"
        } else {
            "line"
        }
    ));
    Ok(Some(line))
}

/// Component `c`'s mask grown by up to `r` pixels (a disc) into the components sewn after it
/// (`rank`), never into the background or regions sewn before; cropped to its box, with the
/// box's top-left pixel.
fn underlap(
    id: &[u32],
    w: usize,
    h: usize,
    c: usize,
    k: &regions::Component,
    r: usize,
    rank: &[usize],
) -> (Vec<bool>, usize, usize, usize, usize) {
    let (x0, y0) = (k.x0.saturating_sub(r), k.y0.saturating_sub(r));
    let (x1, y1) = ((k.x1 + r).min(w), (k.y1 + r).min(h));
    let (mw, mh) = (x1 - x0, y1 - y0);
    let mut m = vec![false; mw * mh];
    let later = |i: usize| {
        let o = id[i];
        o != u32::MAX && o as usize != c && rank[o as usize] > rank[c]
    };
    let rr = (r * r) as i64;
    for y in k.y0..k.y1 {
        for x in k.x0..k.x1 {
            if id[y * w + x] as usize != c {
                continue;
            }
            m[(y - y0) * mw + x - x0] = true;
            // Only edge pixels grow.
            let edge = [(0i64, 1i64), (1, 0), (0, -1), (-1, 0)]
                .iter()
                .any(|&(dx, dy)| {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    nx >= 0
                        && ny >= 0
                        && (nx as usize) < w
                        && (ny as usize) < h
                        && id[ny as usize * w + nx as usize] as usize != c
                });
            if !edge {
                continue;
            }
            for dy in -(r as i64)..=r as i64 {
                for dx in -(r as i64)..=r as i64 {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if dx * dx + dy * dy > rr
                        || nx < x0 as i64
                        || ny < y0 as i64
                        || nx >= x1 as i64
                        || ny >= y1 as i64
                    {
                        continue;
                    }
                    let i = ny as usize * w + nx as usize;
                    if later(i) {
                        m[(ny as usize - y0) * mw + nx as usize - x0] = true;
                    }
                }
            }
        }
    }
    (m, mw, mh, x0, y0)
}

/// For each of `n` components, the others it touches (4-neighbours).
fn adjacency(id: &[u32], w: usize, h: usize, n: usize) -> Vec<Vec<usize>> {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut link = |a: u32, b: u32| {
        if a != b && a != u32::MAX && b != u32::MAX {
            let (a, b) = (a as usize, b as usize);
            if !adj[a].contains(&b) {
                adj[a].push(b);
                adj[b].push(a);
            }
        }
    };
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if x + 1 < w {
                link(id[i], id[i + 1]);
            }
            if y + 1 < h {
                link(id[i], id[i + w]);
            }
        }
    }
    adj
}

/// [`AngleRule::VaryNeighbours`]: `base` when it is at least [`NEIGHBOUR_DEG`] from every
/// angle of `near` (directions, so modulo 180°); otherwise, of `base` turned by 30°, -30°, 60°,
/// -60° and 90°, the one furthest from the nearest of them (ties to the earlier). Taking the
/// first turn that cleared 20° instead lowered the likeness on g01, g05 and g08 (#690).
pub fn vary_angle(base: f64, near: &[f64]) -> f64 {
    let apart = |a: f64, b: f64| {
        let d = (a - b).rem_euclid(180.0);
        d.min(180.0 - d)
    };
    let nearest = |a: f64| near.iter().map(|&n| apart(a, n)).fold(f64::MAX, f64::min);
    if nearest(base) >= NEIGHBOUR_DEG {
        return base;
    }
    let mut best = base;
    for t in [30.0, -30.0, 60.0, -60.0, 90.0] {
        let a = normalize(base + t) + 0.0;
        if nearest(a) > nearest(best) + 1e-9 {
            best = a;
        }
    }
    best
}

/// [`AngleRule::VaryNeighbours`] turns only fills of at least this area, mm², and only away from
/// touching fills as large: smaller pieces read as detail, not as panels, and turning them
/// lowered the likeness (g05, g15; #690).
pub const VARY_MIN_MM2: f64 = 20.0;

/// [`AngleRule::LeastTravel`]: the direction of [`TRAVEL_ANGLES`] whose layers sew the least
/// thread for the fill `region` over `contours`, or the region's own direction unless another
/// saves at least [`TRAVEL_SAVING`] (ties to the earlier angle).
pub fn least_travel(contours: &[Contour], region: &RegionSewing) -> f64 {
    // On the outlines flattened once: the travel router tests visibility against them for
    // every row end, and flattened their curves again on each test.
    let closed: Vec<Contour> = contours
        .iter()
        .filter(|c| c.closed)
        .map(|c| Contour::new(c.path(), true))
        .collect();
    let thread = |deg: f64| -> f64 {
        let mut r = region.clone();
        r.angle = deg;
        openstitch_design::region::layers(&closed, &r, &[])
            .iter()
            .flatten()
            .flat_map(|run| run.windows(2).map(|w| w[0].distance(w[1])))
            .sum()
    };
    let own = thread(region.angle);
    let mut best = (own - TRAVEL_SAVING, region.angle);
    for deg in TRAVEL_ANGLES {
        let a = normalize(deg) + 0.0;
        let t = thread(a);
        if t < best.0 {
            best = (t, a);
        }
    }
    best.1
}

/// OpenStitch mode: a region whose stitches all fall within a box this size, mm, is a speck
/// ([`Reason::Speck`], #690).
pub const SPECK_MM: f64 = 0.8;

/// Whether the region's layers sew within a [`SPECK_MM`] box.
fn speck(contours: &[Contour], region: &RegionSewing) -> bool {
    let closed: Vec<Contour> = contours.iter().filter(|c| c.closed).cloned().collect();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for q in openstitch_design::region::layers(&closed, region, &[])
        .iter()
        .flatten()
        .flatten()
    {
        (x0, y0, x1, y1) = (x0.min(q.x), y0.min(q.y), x1.max(q.x), y1.max(q.y));
    }
    x0 <= x1 && x1 - x0 < SPECK_MM * 10.0 && y1 - y0 < SPECK_MM * 10.0
}

/// Most stitches of 0.3 mm or less in a row a region may sew (`tools/sew_check.py` allows 6).
const MAX_IN_PLACE: usize = 6;

/// The tie-in the compiler sews before a run: 0.3 mm out and back.
const TIE_IN_STEPS: usize = 3;

/// Whether the region's layers, with their tie-in, would sew more than [`MAX_IN_PLACE`]
/// stitches of 0.3 mm or less in a row (#713).
fn stands_in_place(contours: &[Contour], region: &RegionSewing) -> bool {
    let closed: Vec<Contour> = contours.iter().filter(|c| c.closed).cloned().collect();
    openstitch_design::region::layers(&closed, region, &[])
        .iter()
        .flatten()
        .any(|run| {
            let mut n = 0;
            run.windows(2).any(|w| {
                n = if w[0].distance(w[1]) <= 3.0 { n + 1 } else { 0 };
                n + TIE_IN_STEPS > MAX_IN_PLACE
            })
        })
}

/// Line art's small pieces of one thread joined (#713): each run of objects sewn in the same
/// thread is put in nearest-next order (one thread, so the order changes nothing that shows),
/// and two pieces this close (mm, between their bounds) are joined by a short running
/// connector instead of a trim.
const JOIN_MM: f64 = 2.0;

fn join_small_pieces(objects: &mut [DesignObject]) {
    let thread_of = |o: &DesignObject| -> Option<Thread> {
        o.region
            .as_ref()
            .map(|r| r.thread.clone())
            .or_else(|| o.line.as_ref().map(|l| l.thread.clone()))
            .or_else(|| {
                o.stitch_block
                    .as_ref()
                    .and_then(|b| b.threads.first().cloned())
            })
    };
    let gap = |a: &DesignObject, b: &DesignObject| match (a.bounds(), b.bounds()) {
        (Some(p), Some(q)) => {
            let dx = (p.min_x.max(q.min_x) - p.max_x.min(q.max_x)).max(0.0);
            let dy = (p.min_y.max(q.min_y) - p.max_y.min(q.max_y)).max(0.0);
            (dx * dx + dy * dy).sqrt()
        }
        _ => f64::INFINITY,
    };
    let mut i = 0;
    while i < objects.len() {
        let t = thread_of(&objects[i]);
        let mut j = i + 1;
        while j < objects.len() && thread_of(&objects[j]) == t && objects[j].line.is_none() {
            j += 1;
        }
        if objects[i].line.is_none() {
            // Nearest next from the run's first piece.
            for k in i + 1..j {
                let last = &objects[k - 1];
                let best = (k..j)
                    .min_by(|&a, &b| gap(last, &objects[a]).total_cmp(&gap(last, &objects[b])))
                    .expect("non-empty");
                objects.swap(k, best);
            }
            for k in i..j.saturating_sub(1) {
                let joined = gap(&objects[k], &objects[k + 1]) <= JOIN_MM * 10.0;
                objects[k].connector_run = joined;
                if joined {
                    // Pieces joined by a connector carry the thread in and out: their fills'
                    // own tie-in and lock are left out (a small piece's tie, rows and lock
                    // stood 7 in one spot), and the writer ties only where a trim cuts (#713).
                    for o in [k, k + 1] {
                        if objects[o].region.is_some() && objects[o].finish.is_none() {
                            objects[o].finish = Some(openstitch_design::finish::Finish {
                                tie_count: Some(1),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
        i = j;
    }
}

/// Longest stitch (0.1 mm) Auto Punch lets a satin region sew: PE-DESIGN 11.50's Auto Punch
/// output reaches 10.4 mm (round-5 g02-balloon), 9.1 mm on the storybook picture (#460).
const AUTO_PUNCH_LONGEST_SATIN: f64 = 105.0;

/// The longest stitch of a region's top layer, as the design compiler sews it (0.1 mm).
fn longest_crossing(contours: &[Contour], region: &RegionSewing) -> f64 {
    let closed: Vec<Contour> = contours.iter().filter(|c| c.closed).cloned().collect();
    openstitch_design::region::layers(&closed, region, &[])
        .last()
        .into_iter()
        .flatten()
        .flat_map(|run| run.windows(2).map(|w| w[0].distance(w[1])))
        .fold(0.0, f64::max)
}

/// A straight bar's outline with clean ends: the convex hull of its traced outline. Cut from
/// its letter on pixels, a bar's end carries notches of 0.3-0.5 mm where the junction was
/// split (the T's stem under its bar), and the fixed-direction satin's rows broke into slivers
/// there, sewn after the column with travel across it (g08, #450 follow-up). A bar is convex
/// ([`columns::RECTANGULAR`]), so its hull adds only the notches, which lie inside the letter
/// or within the outline's tracing tolerance. A bar traced with a hole keeps its outline.
fn bar_outline(contours: Vec<Contour>) -> Vec<Contour> {
    let [c] = &contours[..] else {
        return contours;
    };
    let hull = convex_hull(&c.path());
    if hull.len() < 3 {
        return contours;
    }
    vec![Contour::new(hull, true)]
}

/// Convex hull of `points`, counter-clockwise from the lowest-then-leftmost point (Andrew's
/// monotone chain).
fn convex_hull(points: &[openstitch_design::Vec2]) -> Vec<openstitch_design::Vec2> {
    let mut p: Vec<openstitch_design::Vec2> = points.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if p.len() < 3 {
        return p;
    }
    let cross =
        |o: openstitch_design::Vec2, a: openstitch_design::Vec2, b: openstitch_design::Vec2| {
            (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
        };
    let mut lower: Vec<openstitch_design::Vec2> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<openstitch_design::Vec2> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// How a satin region (or a piece of one) is sewn.
enum Sewn {
    /// Auto direction, as PE-DESIGN sets it.
    Auto,
    /// A straight bar of a split region, running this way (degrees, y down).
    Bar(f64),
    /// A curved column through these rungs (0.1 mm).
    Column(Vec<[openstitch_design::Vec2; 2]>),
    /// One arm of a branching stroke, sewn along its centre line (#702).
    Skeleton(Box<openstitch_design::skeleton::Skeleton>),
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

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_design::v2;

    #[test]
    fn a_bar_cut_from_its_letter_gets_clean_ends() {
        // g08's T stem as traced (0.1 mm): notches of 0.3-0.5 mm where it was cut from the bar
        // above it broke the satin's top rows into slivers, sewn after the column with travel
        // across it (#450 follow-up). Its outline becomes the hull: four straight sides.
        let stem = [
            (145.2, 37.8),
            (149.1, 41.7),
            (151.7, 41.7),
            (154.3, 39.1),
            (175.1, 39.1),
            (179.0, 44.3),
            (181.6, 44.3),
            (184.2, 41.7),
            (187.5, 42.3),
            (186.2, 44.9),
            (186.2, 145.2),
            (184.9, 146.5),
            (186.2, 149.1),
            (182.9, 152.4),
            (180.3, 151.1),
            (147.8, 151.1),
            (143.2, 149.1),
            (143.2, 44.9),
            (141.9, 43.6),
            (143.2, 41.0),
            (141.9, 39.7),
        ];
        let c = Contour::new(stem.iter().map(|&(x, y)| v2(x, y)).collect(), true);
        let out = bar_outline(vec![c]);
        assert_eq!(out.len(), 1);
        let hull = &out[0].points;
        assert!(hull.len() <= 10, "{} vertices", hull.len());
        // Every turn is the same way round: a convex outline, no notch left.
        let n = hull.len();
        let turns: Vec<f64> = (0..n)
            .map(|i| {
                let (a, b, c) = (hull[i], hull[(i + 1) % n], hull[(i + 2) % n]);
                (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x)
            })
            .collect();
        assert!(turns.iter().all(|&t| t >= 0.0) || turns.iter().all(|&t| t <= 0.0));
        // Two contours (a bar traced with a hole) are left as they are.
        let two = vec![out[0].clone(), out[0].clone()];
        assert_eq!(bar_outline(two.clone()), two);
    }
}
