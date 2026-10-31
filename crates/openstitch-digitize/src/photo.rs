//! Photo Stitch approximations: tonal images become density-modulated stitching.
//!
//! - [`PhotoStyle::Fill`] (after Photo Stitch 1): the image is sampled on a grid of fill
//!   stitches (row spacing × stitch length) and error-diffused (Floyd–Steinberg) onto the
//!   thread colours plus the fabric colour. Each thread sews its cells as staggered fill rows,
//!   the runs visited nearest first so a colour sews in few long paths (see `fill_layer`);
//!   fabric cells stay unsewn, so tone becomes stitch coverage. In OpenStitch mode, in colour,
//!   the rows are sampled every [`FINE_STEP_MM`] instead and each row's threads chosen by
//!   [`crate::rows`] under a needle-density budget, so a stitch ends where the thread changes
//!   and the Run Pitch is only the longest stitch (B16.1 T2).
//! - [`PhotoStyle::Lines`] (Photo Stitch 2, as PE-DESIGN 11.50 sews it): a line screen. Each
//!   thread sews parallel lines a Line interval apart over the whole image at its own fixed
//!   angle (0°, 45°, 90°, 135° in sewing order, turned by the Direction), straight where its
//!   share of the local colour is near zero and a zigzag across the line where it is higher
//!   ([`crate::screen`], #129).
//!
//! Mono mode works on integer Rec. 601 luminance (`(30 R + 59 G + 11 B) / 100`) and first
//! stretches the image's contrast over the full 0–255 range from its darkest to its lightest
//! grey (plain min/max auto-levels; left alone when the range is 1 level or less, or already
//! full), as PE-DESIGN's mono Photo Stitch does (#128).
//!
//! Specified from PE-DESIGN 11's public guide (Photo Stitch 1/2 create stitching from photos
//! in colour, sepia/grey or mono); the algorithms are this project's own.

use crate::palette::{self, Palette, Rgb, rgb_f};
use crate::perceptual::{self, Lab};
use crate::raster::{Frame, Raster};
use crate::{DigitizeError, Mode};
use openstitch_core::{Command, Stitch, Thread};
use openstitch_design::fmath::Det;
use openstitch_design::{DesignObject, StitchBlock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhotoStyle {
    Fill,
    Lines,
}

/// Photo Stitch 2's Color Set (PE-DESIGN 11.50's list: Auto Select, CMYK, RGBK, CRYK, BMYK).
/// A named set is four fixed chart threads, sewn in the listed order, which also gives each its
/// screen angle (round-4 captures `photostitch2-cmyk/-rgbk/-crvk/-bmyk.pes`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorSet {
    /// The threads come from the image's colours ([`PhotoMode::Color`]'s count).
    #[default]
    Auto,
    /// Sky Blue, Dark Fuchsia, Yellow, Black.
    Cmyk,
    /// Red, Blue, Lime Green, Black.
    Rgbk,
    /// Sky Blue, Red, Yellow, Black.
    Cryk,
    /// Blue, Dark Fuchsia, Yellow, Black.
    Bmyk,
}

impl ColorSet {
    /// The set's PEC chart numbers in sewing order, `None` for Auto Select.
    pub fn pec(self) -> Option<[usize; 4]> {
        match self {
            ColorSet::Auto => None,
            ColorSet::Cmyk => Some([32, 51, 13, 20]),
            ColorSet::Rgbk => Some([5, 2, 14, 20]),
            ColorSet::Cryk => Some([32, 5, 13, 20]),
            ColorSet::Bmyk => Some([2, 51, 13, 20]),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PhotoMode {
    /// Reduce to this many PEC threads.
    Color(usize),
    /// One thread.
    Mono(Thread),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhotoStitch {
    pub style: PhotoStyle,
    pub mode: PhotoMode,
    /// Fabric (page) colour: what unsewn areas show.
    pub fabric: Rgb,
    /// Fill style: sew the page colour too (PE-DESIGN 11.50's "Sew Page Color", ticked by
    /// default in Color mode), as a thread of its own that sews first, so every opaque cell is
    /// sewn and the image's light areas get the page thread instead of bare fabric. Mono and
    /// Lines ignore it (PE-DESIGN's Mono has no such option).
    pub sew_page: bool,
    /// Distance between rows (Fill) or the Line interval (Lines), mm.
    pub row_spacing_mm: f64,
    /// Fill: stitch length (Run Pitch). Lines: the straight running stitch where a thread's
    /// colour is not needed, mm.
    pub stitch_mm: f64,
    /// Lines: Max. Density, zigzag lines per mm at full colour (PE-DESIGN's default 4.5).
    pub max_density: f64,
    /// Lines: Direction, degrees added to every thread's screen angle (default 0).
    pub direction_deg: f64,
    /// Lines, colour mode: the Color Set (default Auto Select).
    pub color_set: ColorSet,
    /// Colour mode: PE-DESIGN's method, or OpenStitch's: the perceptual palette
    /// ([`Palette::build_perceptual`]; Photo Stitch 2's Auto Select too), dithering in CIELAB
    /// (Photo Stitch 1, [`palette::dither_perceptual`]) and the line screen's mix in linear
    /// light (Photo Stitch 2, [`crate::screen::Shares::colour_in`]).
    pub compat: Mode,
}

impl PhotoStitch {
    pub fn new(style: PhotoStyle, mode: PhotoMode) -> Self {
        let (row_spacing_mm, stitch_mm) = match style {
            // Run Pitch 3.0 mm is PE-DESIGN 11.50's default; 0.25 mm rows give its density
            // (160-210 needle points per cm² in the tone bands of the round-4 capture
            // `photostitch1-default-plus-cross.pes`; 12 860 needle points against its 12 255).
            PhotoStyle::Fill => (FILL_ROW_SPACING_MM, RUN_PITCH_MM),
            // PE-DESIGN 11.50's Photo Stitch 2: Line interval 2.0 mm; its straight runs are
            // 2.4–2.5 mm (round-4 `photostitch2-color-owner-shapes.pes`).
            PhotoStyle::Lines => (LINE_INTERVAL_MM, LINE_RUN_MM),
        };
        let sew_page = style == PhotoStyle::Fill && matches!(mode, PhotoMode::Color(_));
        PhotoStitch {
            style,
            mode,
            fabric: [255, 255, 255],
            // PE-DESIGN's Mono mode has no Sew Page Color and leaves light areas unsewn
            // (round-4 `photostitch1-mono-grey-tonal.pes`).
            sew_page,
            row_spacing_mm,
            stitch_mm,
            max_density: MAX_DENSITY,
            direction_deg: 0.0,
            color_set: ColorSet::Auto,
            compat: Mode::PeDesign,
        }
    }
}

/// Photo Stitch 2's default Line interval, mm (PE-DESIGN 11.50).
pub const LINE_INTERVAL_MM: f64 = 2.0;
/// Photo Stitch 2's straight running stitch where a thread's colour is not needed, mm.
pub const LINE_RUN_MM: f64 = 2.5;
/// Photo Stitch 2's default Max. Density, lines per mm (PE-DESIGN 11.50).
pub const MAX_DENSITY: f64 = 4.5;
/// Photo Stitch 2's thread count with Color Set Auto Select (four on the round-4 capture).
pub const LINE_COLORS: usize = 4;

/// Photo Stitch 1's default Run Pitch, mm (PE-DESIGN 11.50).
pub const RUN_PITCH_MM: f64 = 3.0;
/// Default fill row spacing, mm: with [`RUN_PITCH_MM`] it gives PE-DESIGN's needle density.
pub const FILL_ROW_SPACING_MM: f64 = 0.25;
/// Photo Stitch 1's default Max. Number of Colors (PE-DESIGN 11.50).
pub const COLORS: usize = 10;

/// OpenStitch mode's Photo Stitch 1: sample spacing along a row, mm.
pub const FINE_STEP_MM: f64 = 0.25;
/// Thread width, mm: wider than the fill rows' spacing, so neighbouring rows overlap.
pub const THREAD_MM: f64 = 0.4;
/// OpenStitch mode's Photo Stitch 1: the shortest stitch, mm.
pub const MIN_STITCH_MM: f64 = 1.0;
/// OpenStitch mode's Photo Stitch 1: the longest travel stitch between two runs of a thread,
/// mm; a travel is sewn only where later layers cover it (`covered`).
pub const TRAVEL_MM: f64 = 6.0;
/// OpenStitch mode's Photo Stitch 1: the needle points per mm² the fine rows aim at over the
/// design (the owner's cap is 2.1 in any 10 × 10 mm window).
pub const FINE_DENSITY: f64 = 2.0;

/// Most grid samples one conversion may use.
pub const MAX_SAMPLES: usize = 400_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Converted {
    /// One stitch object per thread, in sewing order (ids unassigned).
    pub objects: Vec<DesignObject>,
    pub threads: Vec<Thread>,
    /// Needle points per object.
    pub stitches: Vec<usize>,
}

/// Integer Rec. 601 luminance, 0–255: `(30 R + 59 G + 11 B) / 100`, truncated.
pub fn luma(c: [f64; 3]) -> f64 {
    f64::from(crate::raster::luma601(c[0] as u8, c[1] as u8, c[2] as u8))
}

pub fn convert(image: &Raster, frame: &Frame, p: &PhotoStitch) -> Result<Converted, DigitizeError> {
    let spacing = p.row_spacing_mm.clamp(0.2, 5.0) * 10.0;
    let pitch = p.stitch_mm.clamp(0.3, 7.0) * 10.0;
    let width = f64::from(image.width()) * frame.scale;
    let height = f64::from(image.height()) * frame.scale;
    let mono = matches!(p.mode, PhotoMode::Mono(_));
    // OpenStitch mode's Photo Stitch 1 in colour samples every FINE_STEP_MM along a row and
    // lets a stitch end wherever the thread changes (fine rows, below); the Run Pitch becomes
    // the longest stitch instead of the cell.
    let fine = p.compat == Mode::OpenStitch && p.style == PhotoStyle::Fill && !mono;
    let rows = (height / spacing).round().max(1.0);
    let cols = if fine {
        // As fine as the sample limit allows, never coarser than the Run Pitch.
        (width / (FINE_STEP_MM * 10.0))
            .round()
            .min((MAX_SAMPLES as f64 / rows).floor())
            .max((width / pitch).round())
            .max(1.0)
    } else {
        (width / pitch).round().max(1.0)
    };
    if cols * rows > MAX_SAMPLES as f64 {
        return Err(DigitizeError::TooLarge(format!(
            "{cols}×{rows} samples (limit {MAX_SAMPLES}); use a smaller width or coarser spacing"
        )));
    }
    let (cols, rows) = (cols as usize, rows as usize);
    let (cw, ch) = (width / cols as f64, height / rows as f64);
    let grid = image.resample(cols as u32, rows as u32);
    let mut cells: Vec<Option<[f64; 3]>> = (0..rows as u32)
        .flat_map(|y| (0..cols as u32).map(move |x| (x, y)))
        .map(|(x, y)| {
            let [r, g, b, a] = grid.pixel(x, y);
            (a >= 128).then(|| [f64::from(r), f64::from(g), f64::from(b)])
        })
        .collect();
    if cells.iter().all(Option::is_none) {
        return Err(DigitizeError::Empty(
            "the image is fully transparent".into(),
        ));
    }
    if mono {
        stretch(&mut cells);
    }
    let fabric_pec = palette::nearest_pec(p.fabric).0;
    // OpenStitch mode reaches both styles in colour: Photo Stitch 1 and Photo Stitch 2's Auto
    // Select take the perceptual palette, Photo Stitch 1 dithers in CIELAB and Photo Stitch 2
    // mixes in linear light. Mono stays as PE-DESIGN's.
    let perceptual = p.compat == Mode::OpenStitch && !mono;
    let linear = perceptual && p.style == PhotoStyle::Lines;
    // Threads, and for the line screen the image colour each one stands for (its reduced
    // colour), against which the screen measures each thread's share.
    // A named Color Set: four fixed chart threads, sewn in the set's order.
    let fixed = match (&p.mode, p.style, p.color_set.pec()) {
        (PhotoMode::Color(_), PhotoStyle::Lines, Some(pec)) => Some(pec),
        _ => None,
    };
    let (threads, stands_for): (Vec<Thread>, Vec<[f64; 3]>) = match (&p.mode, fixed) {
        (_, Some(pec)) => pec
            .iter()
            .filter_map(|&i| openstitch_core::PEC_CHART.get(i).cloned().flatten())
            .map(|t| {
                let c = rgb_f(&t);
                (t, c)
            })
            .unzip(),
        (PhotoMode::Mono(t), _) => (vec![t.clone()], vec![[0.0; 3]]),
        (PhotoMode::Color(n), None) => {
            let colors: Vec<Rgb> = cells.iter().flatten().map(|c| c.map(|v| v as u8)).collect();
            let n = (*n).clamp(1, 64);
            // Photo Stitch 2's Color Set holds N threads besides the page, so the line screen
            // reduces to one colour more and leaves the page's out.
            let boxes = if p.style == PhotoStyle::Lines {
                n + 1
            } else {
                n
            };
            let pal = if perceptual {
                Palette::build_perceptual(&colors, boxes.min(64))
            } else {
                Palette::build(&colors, boxes.min(64))
            };
            // A thread matching the fabric is left out: the fabric shows instead.
            pal.chart
                .iter()
                .zip(&pal.threads)
                .enumerate()
                .filter(|(_, (c, _))| **c != fabric_pec)
                .take(if p.style == PhotoStyle::Lines {
                    n
                } else {
                    usize::MAX
                })
                .map(|(k, (_, t))| {
                    let mean = pal
                        .means
                        .iter()
                        .zip(&pal.thread_of)
                        .find(|(_, of)| **of == k)
                        .map_or_else(|| rgb_f(t), |(m, _)| m.map(f64::from));
                    (t.clone(), mean)
                })
                .unzip()
        }
    };
    if threads.is_empty() {
        return Err(DigitizeError::Empty(
            "every colour matches the fabric".into(),
        ));
    }
    // Mono works on the stretched grey image: the thread is black, the fabric white.
    let fabric = if mono {
        [255.0; 3]
    } else {
        p.fabric.map(f64::from)
    };
    let mut targets: Vec<[f64; 3]> = if mono {
        vec![[0.0; 3]]
    } else {
        threads.iter().map(rgb_f).collect()
    };
    targets.push(fabric);
    // Sew Page Color: the fabric target becomes a thread of its own (the nearest chart thread),
    // so the cells dithered to it are sewn too.
    let mut threads = threads;
    if p.sew_page && p.style == PhotoStyle::Fill && !mono {
        threads.push(palette::nearest_pec(p.fabric).1);
    }
    // Fill: light threads first, so darker detail sews on top.
    let light_first = |a: &usize, b: &usize| {
        luma(targets[*b])
            .total_cmp(&luma(targets[*a]))
            .then(a.cmp(b))
    };
    let dithered = if fine {
        let sewn = threads.len();
        let mut order: Vec<usize> = (0..targets.len()).collect();
        order.sort_by(light_first);
        // Sewing rank per target; an unsewn fabric is below every thread.
        let mut rank = vec![0; targets.len()];
        for (i, &t) in order.iter().filter(|&&t| t < sewn).enumerate() {
            rank[t] = i + 1;
        }
        fine_rows(&cells, cols, rows, &targets, &rank, sewn, cw, ch, pitch)
    } else if perceptual {
        palette::dither_perceptual(&cells, cols, rows, &targets)
    } else {
        palette::dither(&cells, cols, rows, &targets)
    };
    let mut objects = Vec::new();
    let mut counts = Vec::new();
    let mut sewn_threads = Vec::new();
    let mut total = 0;
    // Lines: each thread's share of the local colour, on a fine grid over the image.
    let screen = (p.style == PhotoStyle::Lines).then(|| {
        let shares = if mono {
            crate::screen::Shares::mono(image, frame)
        } else {
            crate::screen::Shares::colour_in(image, frame, &stands_for, fabric, linear)
        };
        let settings = crate::screen::Screen {
            interval: p.row_spacing_mm.clamp(0.5, 10.0) * 10.0,
            run: p.stitch_mm.clamp(0.5, 7.0) * 10.0,
            max_density: p.max_density.clamp(0.5, 10.0),
        };
        (shares, settings)
    });
    let mut order: Vec<usize> = (0..threads.len()).collect();
    match &screen {
        None => order.sort_by(light_first),
        // Lines: the thread the screen sews most first, as PE-DESIGN orders Photo Stitch 2
        // (every round-4 capture, Auto Select and the named Color Sets alike: on the owner's
        // shapes CMYK sews cyan, magenta, yellow, black; on the ramp cyan, yellow, magenta,
        // black); the screen angles follow this order.
        Some((shares, _)) => {
            order.sort_by(|&a, &b| shares.usage(b).total_cmp(&shares.usage(a)).then(a.cmp(&b)))
        }
    }
    let mut rank = vec![0; order.len()];
    for (layer, &t) in order.iter().enumerate() {
        rank[t] = layer;
    }
    for (layer, &t) in order.iter().enumerate() {
        let stitches = match &screen {
            None if fine => {
                let runs = fine_runs(&dithered, cols, rows, t, frame, cw, ch, pitch);
                let hidden = |x0: f64, y0: f64, x1: f64, y1: f64| {
                    covered(
                        &dithered,
                        cols,
                        rows,
                        &rank,
                        t,
                        frame,
                        cw,
                        ch,
                        [x0, y0, x1, y1],
                    )
                };
                let travel = TRAVEL_MM * 10.0;
                let sewing = Sewing {
                    reach_x: travel,
                    reach: travel,
                    shortest: MIN_STITCH_MM * 10.0 + QUANTUM,
                    row_reach: (travel / ch) as usize,
                    turn: 0.0,
                    away: Some(CONNECT),
                };
                no_short(sew_runs(&runs, rows, ch, &sewing, hidden))
            }
            None => fill_layer(&dithered, cols, rows, t, frame, cw, ch),
            Some((shares, settings)) => {
                let angle = crate::screen::angle(layer, p.direction_deg);
                crate::screen::layer(shares, t, angle, frame, settings)
            }
        };
        if stitches.iter().all(|s| s.command != Command::Stitch) {
            continue;
        }
        total += stitches.len();
        if total > crate::MAX_STITCHES {
            return Err(DigitizeError::TooLarge(format!(
                "more than {} stitches; use a smaller width or coarser spacing",
                crate::MAX_STITCHES
            )));
        }
        counts.push(
            stitches
                .iter()
                .filter(|s| s.command == Command::Stitch)
                .count(),
        );
        let thread = threads[t].clone();
        let mut o = DesignObject::stitches(StitchBlock {
            stitches,
            threads: vec![thread.clone()],
        });
        o.name = Some(format!(
            "Photo Stitch {}",
            thread.name.as_deref().unwrap_or("thread")
        ));
        objects.push(o);
        sewn_threads.push(thread);
    }
    if objects.is_empty() {
        return Err(DigitizeError::Empty(
            "the image is the fabric colour throughout".into(),
        ));
    }
    Ok(Converted {
        objects,
        threads: sewn_threads,
        stitches: counts,
    })
}

/// Mono contrast stretch: every opaque cell becomes its grey `g = (30 R + 59 G + 11 B) / 100`;
/// when the greys span more than one level and less than the whole range, each is mapped
/// linearly so the darkest becomes 0 and the lightest 255: `g' = (g − min) · 255 / (max − min)`,
/// in integers. A flat image (range 0 or 1) and one already spanning 0–255 are left as they are.
fn stretch(cells: &mut [Option<[f64; 3]>]) {
    let mut greys: Vec<u8> = cells.iter().flatten().map(|c| luma(*c) as u8).collect();
    auto_levels(&mut greys);
    for (c, g) in cells.iter_mut().flatten().zip(greys) {
        *c = [f64::from(g); 3];
    }
}

/// Full-range auto-levels on 8-bit greys: when `max − min` is in `(1, 255)`, every grey becomes
/// `(g − min) · 255 / (max − min)` (integer division); otherwise the greys are left alone.
pub fn auto_levels(greys: &mut [u8]) {
    let (lo, hi) = greys
        .iter()
        .fold((255u8, 0u8), |(lo, hi), &g| (lo.min(g), hi.max(g)));
    let range = u32::from(hi.saturating_sub(lo));
    if !(1 < range && range < 255) {
        return;
    }
    for g in greys {
        *g = (u32::from(*g - lo) * 255 / range) as u8;
    }
}

/// Longest move between two runs of one thread that is sewn as a travel stitch instead of a
/// jump (and trim), in cell widths.
pub const JOIN_CELLS: f64 = 2.0;
/// A turn into the next run leaves out the previous run's last needle point when the stitch
/// from its second-to-last point stays within this many cell widths.
pub const TURN_CELLS: f64 = 1.25;

/// One run of neighbouring cells of a thread in a grid row, with its needle points (x in
/// design units, left to right) and the row's y.
struct Run {
    xs: Vec<f64>,
    y: f64,
    row: usize,
}

/// Fill rows for thread `t`. Each run of neighbouring cells in a row is a straight row of
/// needle points every cell width, staggered by thirds from row to row. The runs are sewn
/// nearest first, as PE-DESIGN's Photo Stitch 1 sews a colour in few long paths
/// (round-4 captures: 1–5 jumps a colour): from where the needle is, the closest unsewn run
/// end within [`JOIN_CELLS`] of a cell width in x and two rows in y is sewn next, entered from
/// that end, and reached with a travel stitch; when none is that close, the needle jumps to
/// the first unsewn run in row order.
fn fill_layer(
    d: &[Option<usize>],
    cols: usize,
    rows: usize,
    t: usize,
    frame: &Frame,
    cw: f64,
    ch: f64,
) -> Vec<Stitch> {
    let runs = cell_runs(d, cols, rows, t, frame, cw, ch);
    let sewing = Sewing {
        reach_x: JOIN_CELLS * cw + 1e-6,
        reach: f64::INFINITY,
        shortest: 0.0,
        row_reach: 2,
        turn: TURN_CELLS * cw + 1e-6,
        away: None,
    };
    sew_runs(&runs, rows, ch, &sewing, |_, _, _, _| true)
}

/// Thread `t`'s runs in grid order: each run of neighbouring cells in a row has a needle point
/// every cell width, staggered by thirds from row to row.
fn cell_runs(
    d: &[Option<usize>],
    cols: usize,
    rows: usize,
    t: usize,
    frame: &Frame,
    cw: f64,
    ch: f64,
) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for y in 0..rows {
        let row_y = frame.origin.y + (y as f64 + 0.5) * ch;
        let phase = (y % 3) as f64 / 3.0;
        let mut x = 0;
        while x < cols {
            if d[y * cols + x] != Some(t) {
                x += 1;
                continue;
            }
            let a = x;
            while x < cols && d[y * cols + x] == Some(t) {
                x += 1;
            }
            let (x0, x1) = (a as f64 * cw, x as f64 * cw);
            let mut xs = vec![frame.origin.x + x0];
            let mut k = (x0 / cw - phase).floor() + 1.0;
            while (k + phase) * cw < x1 {
                let px = (k + phase) * cw;
                if px - x0 > cw * 0.25 && x1 - px > cw * 0.25 {
                    xs.push(frame.origin.x + px);
                }
                k += 1.0;
            }
            xs.push(frame.origin.x + x1);
            runs.push(Run {
                xs,
                y: row_y,
                row: y,
            });
        }
    }
    runs
}

/// How [`sew_runs`] moves between runs (distances in design units).
#[derive(Clone, Copy)]
struct Sewing {
    /// Farthest run end in x a travel may reach.
    reach_x: f64,
    /// Farthest run end a travel may reach.
    reach: f64,
    /// Shortest travel: a nearer run end is not sewn to (it would make a too-short stitch).
    shortest: f64,
    /// Rows above and below the needle's searched for the next run.
    row_reach: usize,
    /// A travel within this of the previous run's second-to-last needle point leaves that run's
    /// last point out (a fill row's turn); 0 for never.
    turn: f64,
    /// When no run is near: jump to the first unsewn run farther than this, if any (a shorter
    /// move would be sewn across by the compiler); `None` for the first unsewn run.
    away: Option<f64>,
}

/// Sews `runs` (in grid order) nearest first: from where the needle is, the closest unsewn run
/// end within [`Sewing`]'s reach for which `hidden(needle x, y, end x, y)` holds is sewn next,
/// entered from that end and reached with a travel stitch; when none is, the needle jumps to the
/// first unsewn run in row order (see [`Sewing::away`]).
fn sew_runs(
    runs: &[Run],
    rows: usize,
    ch: f64,
    s: &Sewing,
    hidden: impl Fn(f64, f64, f64, f64) -> bool,
) -> Vec<Stitch> {
    let Sewing {
        reach_x,
        reach,
        shortest,
        row_reach,
        turn,
        away,
    } = *s;
    let mut by_row: Vec<Vec<usize>> = vec![Vec::new(); rows];
    for (i, r) in runs.iter().enumerate() {
        by_row[r.row].push(i);
    }
    let mut done = vec![false; runs.len()];
    let mut next_scan = 0usize;
    let mut out: Vec<Stitch> = Vec::new();
    let mut pen: Option<(f64, usize)> = None;
    let mut prow_run = 0usize;
    let mut last_len = 0usize;
    for _ in 0..runs.len() {
        // The closest unsewn run end near the needle: (distance, run, enter from the right).
        let mut best: Option<(f64, usize, bool)> = None;
        if let Some((px, prow)) = pen {
            let first_row = prow.saturating_sub(row_reach);
            let near = by_row
                .iter()
                .enumerate()
                .take(prow + row_reach + 1)
                .skip(first_row);
            for (row, in_row) in near {
                let dy = (row as f64 - prow as f64).abs() * ch;
                for &r in in_row {
                    if done[r] {
                        continue;
                    }
                    let (l, rgt) = (runs[r].xs[0], runs[r].xs[runs[r].xs.len() - 1]);
                    for (end, from_right) in [(l, false), (rgt, true)] {
                        let dx = (end - px).abs();
                        if dx > reach_x {
                            continue;
                        }
                        let dist = dx.dhypot(dy);
                        if dist <= reach
                            && dist >= shortest
                            && best.is_none_or(|b| dist < b.0 - 1e-9)
                            && hidden(px, runs[prow_run].y, end, runs[r].y)
                        {
                            best = Some((dist, r, from_right));
                        }
                    }
                }
            }
        }
        let (r, from_right, travel) = match best {
            Some((_, r, from_right)) => (r, from_right, true),
            None => {
                while done[next_scan] {
                    next_scan += 1;
                }
                // Serpentine as before: odd rows enter from the right.
                let entry = |r: usize| runs[r].row % 2 == 1;
                // A jump shorter than `away` would be sewn across: the first unsewn run beyond
                // it instead, when there is one.
                let far = |r: usize| {
                    let xs = &runs[r].xs;
                    let x = if entry(r) { xs[xs.len() - 1] } else { xs[0] };
                    let y = runs[r].y;
                    match (away, pen) {
                        (Some(a), Some((px, _))) => (x - px).dhypot(y - runs[prow_run].y) > a,
                        _ => true,
                    }
                };
                match (next_scan..runs.len()).find(|&r| !done[r] && far(r)) {
                    Some(r) => (r, entry(r), false),
                    None => {
                        // Every unsewn run is within reach: enter the first by whichever end
                        // keeps the move at least `shortest` long.
                        let r = next_scan;
                        let (px, py) = pen.map_or((0.0, 0.0), |(px, _)| (px, runs[prow_run].y));
                        let xs = &runs[r].xs;
                        let from_right = if entry(r) {
                            (xs[xs.len() - 1] - px).dhypot(runs[r].y - py) >= shortest
                        } else {
                            (xs[0] - px).dhypot(runs[r].y - py) < shortest
                        };
                        (r, from_right, false)
                    }
                }
            }
        };
        done[r] = true;
        let run = &runs[r];
        let mut xs = run.xs.clone();
        if from_right {
            xs.reverse();
        }
        if !travel {
            out.push(Stitch::new(xs[0], run.y, Command::Jump));
        } else if last_len >= 2
            && let Some(prev) = out.len().checked_sub(2).map(|i| out[i])
            && (prev.x - xs[0]).dhypot(prev.y - run.y) <= turn
        {
            // A short hop to the next row: the previous run's last needle point is left out,
            // so the stitch turns from its second-to-last point straight into this run, as a
            // fill row turns, instead of a sub-millimetre travel stitch.
            out.pop();
        }
        for &x in &xs {
            out.push(Stitch::new(x, run.y, Command::Stitch));
        }
        last_len = xs.len();
        pen = Some((xs[xs.len() - 1], run.row));
        prow_run = r;
    }
    out
}

/// OpenStitch mode's Photo Stitch 1 rows ([`crate::rows`]): every cell's target, the runs at
/// least [`MIN_STITCH_MM`] long, with the change-of-thread penalty bisected so the needle
/// points (counted as [`fine_runs`] places them) meet [`FINE_DENSITY`] over the opaque area.
/// `sewn` is how many targets are sewn (the fabric, last, is not unless Sew Page Color made it
/// a thread).
#[allow(clippy::too_many_arguments)]
fn fine_rows(
    cells: &[Option<[f64; 3]>],
    cols: usize,
    rows: usize,
    targets: &[[f64; 3]],
    rank: &[usize],
    sewn: usize,
    cw: f64,
    ch: f64,
    pitch: f64,
) -> Vec<Option<usize>> {
    let to_lab = |c: [f64; 3]| perceptual::lab(c.map(|v| v.round().clamp(0.0, 255.0) as u8));
    let lab: Vec<Option<Lab>> = cells.iter().map(|c| c.map(to_lab)).collect();
    let target_lab: Vec<Lab> = targets.iter().map(|&t| to_lab(t)).collect();
    let min_run = ((MIN_STITCH_MM * 10.0 / cw) - 1e-9).ceil().max(1.0) as usize;
    let opaque = cells.iter().filter(|c| c.is_some()).count() as f64;
    let budget = FINE_DENSITY * opaque * cw * ch / 100.0;
    let points = |d: &[Option<usize>]| {
        let mut n = 0usize;
        for row in d.chunks(cols) {
            let mut x = 0;
            while x < cols {
                let Some(t) = row[x] else {
                    x += 1;
                    continue;
                };
                let a = x;
                while x < cols && row[x] == Some(t) {
                    x += 1;
                }
                if t < sewn {
                    n += run_points((x - a) as f64 * cw, pitch);
                }
            }
        }
        n as f64
    };
    crate::rows::fit_density(budget, 1.0e6, 24, |lambda| {
        let d = crate::rows::assign(
            &lab,
            cols,
            rows,
            &target_lab,
            rank,
            crate::rows::Rows {
                min_run,
                lambda,
                // Half the geometric overlap: threads flatten into each other rather than
                // lying fully on top (to be calibrated on the sew-out, plan T1).
                strip: 0.5 * crate::rows::strip(THREAD_MM * 10.0, ch),
            },
        );
        let n = points(&d);
        (d, n)
    })
    .0
}

/// Needle points of one fine run `len` long: its start and the end of every stitch, the
/// stitches as few as keep each within `pitch`.
fn run_points(len: f64, pitch: f64) -> usize {
    (len / pitch - 1e-9).ceil().max(1.0) as usize + 1
}

/// Thread `t`'s fine runs in grid order: each run of neighbouring cells in a row is split into
/// equal stitches no longer than `pitch`; inside a run the needle points shift by up to a third
/// of a stitch from row to row (so neighbouring rows do not line their holes up), never making
/// a stitch shorter than [`MIN_STITCH_MM`] or longer than `pitch`. Runs shorter than
/// [`MIN_STITCH_MM`] (slivers between transparent cells) are left unsewn.
#[allow(clippy::too_many_arguments)]
fn fine_runs(
    d: &[Option<usize>],
    cols: usize,
    rows: usize,
    t: usize,
    frame: &Frame,
    cw: f64,
    ch: f64,
    pitch: f64,
) -> Vec<Run> {
    let min = MIN_STITCH_MM * 10.0;
    let mut runs = Vec::new();
    for y in 0..rows {
        let row_y = frame.origin.y + (y as f64 + 0.5) * ch;
        let mut x = 0;
        while x < cols {
            if d[y * cols + x] != Some(t) {
                x += 1;
                continue;
            }
            let a = x;
            while x < cols && d[y * cols + x] == Some(t) {
                x += 1;
            }
            let (x0, x1) = (a as f64 * cw, x as f64 * cw);
            let len = x1 - x0;
            if len < min - 1e-6 {
                continue;
            }
            let n = run_points(len, pitch) - 1;
            let seg = len / n as f64;
            let room = (seg / 3.0)
                .min(seg - min - QUANTUM)
                .min(pitch - seg)
                .max(0.0);
            let shift = room * [0.0, 1.0, -1.0][y % 3];
            // On whole design units (0.1 mm), as machine files hold them: a run of at least
            // MIN_STITCH_MM then stays that long after rounding.
            let mut xs = Vec::with_capacity(n + 1);
            xs.push(frame.origin.x + x0);
            for i in 1..n {
                xs.push(frame.origin.x + x0 + i as f64 * seg + shift);
            }
            xs.push(frame.origin.x + x1);
            runs.push(Run {
                xs: xs.into_iter().map(f64::round).collect(),
                y: row_y,
                row: y,
            });
        }
    }
    runs
}

/// Whether a travel stitch of thread `t` along `seg` (design units: x0, y0, x1, y1) is hidden:
/// every cell it crosses, sampled every half cell, is `t`'s own or sewn by a later layer (whose
/// stitches then cover it), so the travel does not show.
#[allow(clippy::too_many_arguments)]
fn covered(
    d: &[Option<usize>],
    cols: usize,
    rows: usize,
    rank: &[usize],
    t: usize,
    frame: &Frame,
    cw: f64,
    ch: f64,
    seg: [f64; 4],
) -> bool {
    let [x0, y0, x1, y1] = seg;
    let len = (x1 - x0).dhypot(y1 - y0);
    let n = (len / (0.5 * cw.min(ch))).ceil().max(1.0) as usize;
    (0..=n).all(|i| {
        let f = i as f64 / n as f64;
        let x = ((x0 + (x1 - x0) * f - frame.origin.x) / cw).floor();
        let y = ((y0 + (y1 - y0) * f - frame.origin.y) / ch).floor();
        let (x, y) = (
            x.clamp(0.0, cols as f64 - 1.0) as usize,
            y.clamp(0.0, rows as f64 - 1.0) as usize,
        );
        match d[y * cols + x] {
            Some(u) => u == t || rank.get(u).is_some_and(|&r| r > rank[t]),
            None => false,
        }
    })
}

/// The longest move inside one stitch object that the design's compiler sews across as a
/// stitch rather than a trimmed jump (`openstitch_design::compile::Compiler`'s
/// `connect_distance`), design units.
const CONNECT: f64 = 40.0;

/// Machine files hold needle points to 0.1 mm (1 design unit): stitches the generator may
/// place freely keep this margin above [`MIN_STITCH_MM`] so rounding cannot take them under.
const QUANTUM: f64 = 1.0;

/// Drops every needle point that would end a stitch shorter than [`MIN_STITCH_MM`] (the
/// stitch then runs on to the following point). A jump no longer than [`CONNECT`] counts as the
/// stitch the compiler will sew across; a longer jump's landing point stays.
fn no_short(mut stitches: Vec<Stitch>) -> Vec<Stitch> {
    for i in 1..stitches.len() {
        let (a, b) = (stitches[i - 1], stitches[i]);
        if b.command == Command::Jump && (b.x - a.x).dhypot(b.y - a.y) <= CONNECT {
            stitches[i].command = Command::Stitch;
        }
    }
    let min = MIN_STITCH_MM * 10.0 - 1e-6;
    let mut out: Vec<Stitch> = Vec::with_capacity(stitches.len());
    for (i, s) in stitches.iter().enumerate() {
        if s.command == Command::Stitch
            && let Some(prev) = out.last()
            && prev.command == Command::Stitch
            && (s.x - prev.x).dhypot(s.y - prev.y) < min
        {
            // The last point of a path stays (it ends the thread there); its predecessor goes
            // instead when that leaves a long enough stitch.
            let last = stitches
                .get(i + 1)
                .is_none_or(|n| n.command != Command::Stitch);
            if !last {
                continue;
            }
            if out.len() >= 2 && out[out.len() - 2].command == Command::Stitch {
                let p2 = out[out.len() - 2];
                if (s.x - p2.x).dhypot(s.y - p2.y) >= min {
                    out.pop();
                    out.push(*s);
                }
            }
            continue;
        }
        out.push(*s);
    }
    out
}
