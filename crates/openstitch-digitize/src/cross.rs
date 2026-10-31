//! Cross Stitch from an image: the image's colours are reduced to N PEC threads through the
//! colour-reduction prepass Auto Punch uses (3×3 modal colours, census capped at 100 tracked
//! colours, median cut; #127 #132), each cell of a square grid takes the thread most of its
//! modal pixels have, and each thread becomes one stitch object of crosses.
//!
//! Defaults, documented and configurable (#132), as PE-DESIGN 11.50's Cross Stitch wizard
//! shows them (round-4 capture `crossstitch-default.pes`): Cross size 2.5 mm (0.4 cells per
//! mm), Max. Number of Colors automatic from the image (0; PE showed 5 for four shapes on
//! white), full crosses, Cross Stitch Times Single (`triple` sews each
//! full diagonal three times, as the Cross stitch region type's Triple does), the
//! border-majority colour left out (PE omits the white ground the same way). The grid is
//! `round(width / cell) × round(height / cell)` cells over the placed image. Colour → cell:
//! the cell's pixels (about 8 per cell side on the working raster) vote with their modal
//! colour's thread; most votes wins, ties go to the lower thread, a cell most of whose pixels
//! are transparent stays empty. On the owner's four-shape test image this grid gives PE's cells
//! to within one per colour on three of the four colours.
//!
//! Specified from PE-DESIGN 11's public guide (Cross Stitch converts an image into cross
//! stitches on a grid with a chosen cell size and colour count) and PE-DESIGN 11.50's output:
//! each cell is six stitches on its diagonals (see `sew_run`); the order of the runs is this
//! project's own (each run of a row, top to bottom, with a jump between runs, where PE-DESIGN
//! travels along the diagonals instead).
//!
//! OpenStitch mode (`mode: Mode::OpenStitch`, B16.1 T14,
//! docs/research/beyond/image-similarity-plan.md) changes two things and keeps the grid:
//! - **Colour:** each cell takes the mean CIELAB colour of its opaque pixels (on an
//!   area-averaged working raster), not a vote of modal colours, and takes the thread whose
//!   centre is nearest that mean in the census's perceptual palette (k-means in CIELAB,
//!   snapped to PEC threads by CIEDE2000; [`Palette::from_census_perceptual`]).
//! - **Coverage** (`fill`, on by default): a single cross leaves most of its cell as bare
//!   fabric (about 45 % covered at 2.5 mm), so a red cell reads pink. Each diagonal becomes a
//!   band of parallel lines about [`FILL_SPACING_MM`] apart that covers the cell in two thread
//!   layers, one per direction (see `filled_run`), capped at [`FILL_MAX_DENSITY`] needle
//!   points per mm² per run. On the pixel strawberry with exact colours this lifts the
//!   similarity score from 59 to 86 at 2.5 mm; the colour blocks' ceiling is 87.
//! - **Part-crosses** (`part_crosses`, off by default): a cell whose two halves along a
//!   diagonal are clearly different colours is sewn as two halves, a three-quarter and a
//!   quarter cross.

use crate::autopunch::border_majority;
use crate::palette::{self, Palette};
use crate::perceptual::{self, Lab};
use crate::raster::{Frame, Raster};
use crate::regions::NONE;
use crate::{DigitizeError, Mode};
use openstitch_core::{Command, Stitch, Thread};
use openstitch_design::{DesignObject, StitchBlock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossType {
    /// Full cross: both diagonals.
    Full,
    /// Half cross: "/" only.
    Half,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CrossStitch {
    /// Grid cell side, mm (1–10); the grid has `1 / cell_mm` cells per mm.
    pub cell_mm: f64,
    /// Thread colours to reduce to (1–64), or 0 for the automatic count.
    pub colors: usize,
    pub kind: CrossType,
    /// Each diagonal sewn three times instead of once (3 or 1 stitches per diagonal).
    pub triple: bool,
    /// Leave out the colour covering most of the grid's border.
    pub skip_background: bool,
    /// PE-DESIGN 11.50's method or OpenStitch's (mean-colour cells, filled crosses).
    pub mode: Mode,
    /// OpenStitch mode: cover each cell with parallel diagonals instead of one cross (`triple`
    /// then has no effect). Ignored in PE-DESIGN mode.
    pub fill: bool,
    /// OpenStitch mode: a cell whose two halves along a diagonal are clearly different colours
    /// becomes two part-crosses (see `mean_cells`). Off by default (owner, 2026-10-31): it
    /// changes the look more than the likeness (+0.9 points on seven test images). Ignored in
    /// PE-DESIGN mode.
    pub part_crosses: bool,
}

/// OpenStitch mode's filled crosses: distance between neighbouring parallel lines, mm. Lines
/// of 0.4 mm thread this far apart overlap a little, so no fabric shows between them.
pub const FILL_SPACING_MM: f64 = 0.3;

/// OpenStitch mode's filled crosses: most needle points per mm² of a row run (the sewability
/// limit is 2.1 over any 10 × 10 mm). Short runs and small cells get fewer lines, so they are
/// not fully covered.
pub const FILL_MAX_DENSITY: f64 = 2.0;

/// OpenStitch mode: longest stitch, mm; a longer diagonal is split into equal stitches.
pub const FILL_MAX_STITCH_MM: f64 = 6.0;

/// OpenStitch mode: shortest stitch, mm; a needle point nearer than this to the previous one
/// is left out.
pub const MIN_STITCH_MM: f64 = 1.0;

impl CrossStitch {
    /// Default grid cell, mm.
    pub const CELL_MM: f64 = 2.5;
    /// Default thread colours: 0, the automatic count PE-DESIGN fills in from the image
    /// (5 for the owner's four shapes on white).
    pub const COLORS: usize = 0;

    /// Stitches per cell for these settings: a full cross is two full diagonals (1 or 3
    /// stitches each) and four half diagonals; a half cross one "/" and a step along the bottom.
    pub fn stitches_per_cell(&self) -> usize {
        let full = if self.triple { 3 } else { 1 };
        match self.kind {
            CrossType::Full => 2 * full + 4,
            CrossType::Half => full + 1,
        }
    }
}

impl Default for CrossStitch {
    fn default() -> Self {
        CrossStitch {
            cell_mm: Self::CELL_MM,
            colors: Self::COLORS,
            kind: CrossType::Full,
            triple: false,
            skip_background: true,
            mode: Mode::PeDesign,
            fill: true,
            part_crosses: false,
        }
    }
}

/// Most grid cells one conversion may sample.
pub const MAX_CELLS: usize = 160_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Converted {
    /// One stitch object per thread, in sewing order (ids unassigned).
    pub objects: Vec<DesignObject>,
    pub threads: Vec<Thread>,
    /// Crosses per object (a part-cross counts as one).
    pub cells: Vec<usize>,
    pub columns: usize,
    pub rows: usize,
    pub background: Option<Thread>,
}

/// `frame` places the source image; its width in mm fixes the grid.
pub fn convert(image: &Raster, frame: &Frame, p: &CrossStitch) -> Result<Converted, DigitizeError> {
    let cell = p.cell_mm.clamp(1.0, 10.0) * 10.0;
    let width = f64::from(image.width()) * frame.scale;
    let height = f64::from(image.height()) * frame.scale;
    let cols = (width / cell).round().max(1.0);
    let rows = (height / cell).round().max(1.0);
    if cols * rows > MAX_CELLS as f64 {
        return Err(DigitizeError::TooLarge(format!(
            "{cols}×{rows} cells (limit {MAX_CELLS}); use larger cells or a smaller width"
        )));
    }
    let (cols, rows) = (cols as usize, rows as usize);
    // Classify a working copy (about 8 pixels per cell side) against the reduced palette.
    let side = (cols.max(rows) * 8).clamp(64, 1024) as u32;
    let work = image.fit_mode(side);
    let census = palette::census(&work);
    if census.colors.is_empty() {
        return Err(DigitizeError::Empty(
            "the image is fully transparent".into(),
        ));
    }
    let colors = match p.colors {
        0 => census.auto_colors(),
        n => n.min(64),
    };
    let (palette, labels, splits) = match p.mode {
        Mode::PeDesign => {
            let (palette, labels) = modal_cells(&work, &census, colors, cols, rows);
            (palette, labels, Vec::new())
        }
        Mode::OpenStitch => mean_cells(
            &image.fit(side),
            &census,
            colors,
            cols,
            rows,
            p.part_crosses,
        ),
    };
    let split_at = |i: usize| splits.get(i).copied().flatten();
    let background = if p.skip_background {
        border_majority(&labels, cols, rows)
    } else {
        None
    };
    let sewn = |l: u16| l != NONE && Some(l) != background;
    let mut count = vec![0usize; palette.threads.len()];
    for (i, &l) in labels.iter().enumerate() {
        let parts = match split_at(i) {
            Some(sp) => vec![sp.above, sp.below],
            None => vec![l],
        };
        for l in parts.into_iter().filter(|&l| sewn(l)) {
            count[l as usize] += 1;
        }
    }
    // PE-DESIGN's colour order: brightest channel sum first.
    let order = palette::brightness_order(
        &palette,
        (0..palette.threads.len()).filter(|&t| count[t] > 0),
    );
    if order.is_empty() {
        return Err(DigitizeError::Empty("every cell is background".into()));
    }
    let mut objects = Vec::new();
    for &t in &order {
        let mut stitches = Vec::new();
        for y in 0..rows {
            let mut x = 0;
            while x < cols {
                if let Some(sp) = split_at(y * cols + x) {
                    for (mine, other, above) in
                        [(sp.above, sp.below, true), (sp.below, sp.above, false)]
                    {
                        if mine as usize == t && sewn(mine) {
                            // The lower thread, or the only one sewn, takes the diagonal.
                            let diagonal = !sewn(other) || mine < other;
                            let part = Part {
                                slash: sp.slash,
                                above,
                                diagonal,
                            };
                            sew_part(&mut stitches, frame, cell, y, x, part, p.fill);
                        }
                    }
                    x += 1;
                    continue;
                }
                if labels[y * cols + x] as usize != t {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < cols
                    && labels[y * cols + x] as usize == t
                    && split_at(y * cols + x).is_none()
                {
                    x += 1;
                }
                if p.mode == Mode::OpenStitch && p.fill {
                    sew_filled_run(&mut stitches, frame, cell, y, start, x, p.kind);
                } else {
                    sew_run(&mut stitches, frame, cell, y, start, x, p.kind, p.triple);
                }
            }
        }
        let thread = palette.threads[t].clone();
        let mut o = DesignObject::stitches(StitchBlock {
            stitches,
            threads: vec![thread.clone()],
        });
        o.name = Some(format!(
            "Cross Stitch {}",
            thread.name.as_deref().unwrap_or("thread")
        ));
        objects.push(o);
    }
    Ok(Converted {
        objects,
        threads: order.iter().map(|&t| palette.threads[t].clone()).collect(),
        cells: order.iter().map(|&t| count[t]).collect(),
        columns: cols,
        rows,
        background: background.map(|b| palette.threads[b as usize].clone()),
    })
}

/// The `x0..x1 × y0..y1` working pixels of cell (`x`, `y`) on a `cols × rows` grid over a
/// `w × h` raster (at least one pixel each way).
fn cell_span(x: usize, y: usize, cols: usize, rows: usize, w: usize, h: usize) -> [usize; 4] {
    [
        x * w / cols,
        ((x + 1) * w / cols).max(x * w / cols + 1).min(w),
        y * h / rows,
        ((y + 1) * h / rows).max(y * h / rows + 1).min(h),
    ]
}

/// PE-DESIGN mode: the census's median-cut palette; each cell takes the thread most of its
/// pixels' modal colours have, and is empty when most of its pixels are transparent.
fn modal_cells(
    work: &Raster,
    census: &palette::Census,
    colors: usize,
    cols: usize,
    rows: usize,
) -> (Palette, Vec<u16>) {
    let palette = Palette::from_census(census, colors);
    let (ww, wh) = (work.width() as usize, work.height() as usize);
    // Thread of every working pixel from its modal colour; the transparent ones vote "empty".
    let pixel_thread: Vec<usize> = census
        .modal
        .iter()
        .map(|m| m.map_or(palette.threads.len(), |c| palette.classify(c)))
        .collect();
    let mut labels: Vec<u16> = Vec::with_capacity(cols * rows);
    let mut votes = vec![0usize; palette.threads.len() + 1];
    for y in 0..rows {
        for x in 0..cols {
            let [x0, x1, y0, y1] = cell_span(x, y, cols, rows, ww, wh);
            votes.iter_mut().for_each(|v| *v = 0);
            for py in y0..y1 {
                for px in x0..x1 {
                    votes[pixel_thread[py * ww + px]] += 1;
                }
            }
            // Most votes; ties to the lower thread, transparency last.
            let best = (0..votes.len())
                .max_by_key(|&k| (votes[k], std::cmp::Reverse(k)))
                .unwrap_or(0);
            labels.push(if best == palette.threads.len() {
                NONE
            } else {
                best as u16
            });
        }
    }
    (palette, labels)
}

/// A cell split along a diagonal into two part-crosses (OpenStitch mode's `part_crosses`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Split {
    /// Along "/" (bottom-left to top-right) or "\".
    slash: bool,
    /// Thread (or [`NONE`]) of the half above the diagonal ("/": top-left, "\": top-right)
    /// and of the half below it.
    above: u16,
    below: u16,
}

/// Part-crosses: a split must leave at most this share of the cell's colour variance (sum of
/// squared CIELAB distances to the mean) …
const SPLIT_SHARE: f64 = 0.5;

/// … of a cell whose colours vary by at least this much (root mean square, ΔE), so flat or
/// noisy cells stay whole.
const SPLIT_MIN_RMS: f64 = 10.0;

/// Sums of a group of pixels: CIELAB sum, sum of squares, opaque count, all count.
#[derive(Clone, Copy, Default)]
struct Sums {
    lab: [f64; 3],
    sq: f64,
    opaque: usize,
    all: usize,
}

impl Sums {
    fn add(&mut self, l: Option<Lab>) {
        self.all += 1;
        if let Some(l) = l {
            self.lab.iter_mut().zip(l).for_each(|(s, v)| *s += v);
            self.sq += l.iter().map(|v| v * v).sum::<f64>();
            self.opaque += 1;
        }
    }

    fn minus(self, o: Sums) -> Sums {
        Sums {
            lab: [0, 1, 2].map(|k| self.lab[k] - o.lab[k]),
            sq: self.sq - o.sq,
            opaque: self.opaque - o.opaque,
            all: self.all - o.all,
        }
    }

    /// The mean, or `None` when most of the group is transparent.
    fn mean(&self) -> Option<Lab> {
        (2 * self.opaque > self.all).then(|| self.lab.map(|v| v / self.opaque as f64))
    }

    /// Sum of squared distances to the mean.
    fn variance(&self) -> f64 {
        if self.opaque == 0 {
            return 0.0;
        }
        (self.sq - self.lab.iter().map(|v| v * v).sum::<f64>() / self.opaque as f64).max(0.0)
    }
}

/// OpenStitch mode: each cell's mean CIELAB colour over its opaque pixels of the
/// area-averaged working raster `work` (empty when most of its pixels are transparent) takes
/// the thread of the nearest centre (CIEDE2000) of the census's perceptual palette (k-means in
/// CIELAB snapped to PEC threads, as Auto Punch's OpenStitch mode). The palette comes from the
/// image's own colours, not the cell means, so a dark outline blended with the white ground in
/// its cells still gets its own dark thread. With `part`, a cell is split along the diagonal
/// that leaves the least colour variance in its halves, when that is at most [`SPLIT_SHARE`]
/// of the cell's, the cell varies by at least [`SPLIT_MIN_RMS`] and the halves take different
/// threads. Sums run in a fixed scan order.
fn mean_cells(
    work: &Raster,
    census: &palette::Census,
    colors: usize,
    cols: usize,
    rows: usize,
    part: bool,
) -> (Palette, Vec<u16>, Vec<Option<Split>>) {
    let palette = Palette::from_census_perceptual(census, colors);
    let thread = |m: Option<Lab>| m.map_or(NONE, |m| palette.classify(perceptual::rgb(m)) as u16);
    let (ww, wh) = (work.width() as usize, work.height() as usize);
    let mut labels = Vec::with_capacity(cols * rows);
    let mut splits = Vec::with_capacity(if part { cols * rows } else { 0 });
    for y in 0..rows {
        for x in 0..cols {
            let [x0, x1, y0, y1] = cell_span(x, y, cols, rows, ww, wh);
            // The whole cell, the half above "/" and the half above "\".
            let (mut whole, mut slash, mut back) =
                (Sums::default(), Sums::default(), Sums::default());
            for py in y0..y1 {
                for px in x0..x1 {
                    let [r, g, b, a] = work.pixel(px as u32, py as u32);
                    let l = (a >= 128).then(|| perceptual::lab([r, g, b]));
                    whole.add(l);
                    // Pixel centre in cell units, (0, 0) top-left.
                    let u = ((px - x0) as f64 + 0.5) / (x1 - x0) as f64;
                    let v = ((py - y0) as f64 + 0.5) / (y1 - y0) as f64;
                    if u + v < 1.0 {
                        slash.add(l);
                    }
                    if u > v {
                        back.add(l);
                    }
                }
            }
            labels.push(thread(whole.mean()));
            if !part {
                continue;
            }
            let total = whole.variance();
            let rms = (total / whole.opaque.max(1) as f64).sqrt();
            let best = [(true, slash), (false, back)]
                .into_iter()
                .filter(|(_, above)| above.all > 0 && above.all < whole.all)
                .map(|(is_slash, above)| {
                    let below = whole.minus(above);
                    (above.variance() + below.variance(), is_slash, above, below)
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            splits.push(best.and_then(|(left, is_slash, above, below)| {
                let (ta, tb) = (thread(above.mean()), thread(below.mean()));
                (rms >= SPLIT_MIN_RMS && left <= SPLIT_SHARE * total && ta != tb).then_some(Split {
                    slash: is_slash,
                    above: ta,
                    below: tb,
                })
            }));
        }
    }
    (palette, labels, splits)
}

/// Cells `a..b` of grid row `row`. Full crosses are sewn as PE-DESIGN 11.50 sews them
/// (reference `crossstitch-default.pes`): every needle point is a cell corner or a cell
/// centre and every stitch lies on a diagonal, six per cell. Going right from the run's
/// top-left corner, each cell is sewn top-left → centre → bottom-left (two half diagonals),
/// then "/" in full to its top-right corner, the next cell's top-left; coming back from the
/// last cell's top-right corner, each cell is sewn top-right → centre → bottom-right, then
/// "\" in full to its top-left corner. Each diagonal is so covered twice, once whole and once
/// in halves, and nothing runs along a cell side. With `triple` every full diagonal is sewn
/// there, back and there again. Half crosses are one "/" per cell, stepping along the bottom.
#[allow(clippy::too_many_arguments)]
fn sew_run(
    out: &mut Vec<Stitch>,
    frame: &Frame,
    cell: f64,
    row: usize,
    a: usize,
    b: usize,
    kind: CrossType,
    triple: bool,
) {
    let at = |col: f64, r: f64| (frame.origin.x + col * cell, frame.origin.y + r * cell);
    let put = |out: &mut Vec<Stitch>, (x, y): (f64, f64), command| {
        out.push(Stitch::new(x, y, command));
    };
    // A full diagonal from the needle's current point `from` to `to`, once or three times.
    let diagonal = |out: &mut Vec<Stitch>, from: (f64, f64), to: (f64, f64)| {
        put(out, to, Command::Stitch);
        if triple {
            put(out, from, Command::Stitch);
            put(out, to, Command::Stitch);
        }
    };
    let (top, bottom) = (row as f64, row as f64 + 1.0);
    if kind == CrossType::Half {
        put(out, at(a as f64, bottom), Command::Jump);
        put(out, at(a as f64, bottom), Command::Stitch);
        for i in a..b {
            let i = i as f64;
            diagonal(out, at(i, bottom), at(i + 1.0, top));
            if i + 1.0 < b as f64 {
                put(out, at(i + 1.0, bottom), Command::Stitch);
            }
        }
        return;
    }
    put(out, at(a as f64, top), Command::Jump);
    put(out, at(a as f64, top), Command::Stitch);
    for i in a..b {
        let i = i as f64;
        put(out, at(i + 0.5, top + 0.5), Command::Stitch);
        put(out, at(i, bottom), Command::Stitch);
        diagonal(out, at(i, bottom), at(i + 1.0, top));
    }
    for i in (a..b).rev() {
        let i = i as f64;
        put(out, at(i + 0.5, top + 0.5), Command::Stitch);
        put(out, at(i + 1.0, bottom), Command::Stitch);
        diagonal(out, at(i + 1.0, bottom), at(i, top));
    }
}

/// OpenStitch mode's filled crosses for a cell of `cell_mm`: the most lines per cell and
/// direction (lines [`FILL_SPACING_MM`] apart, at least one), and the stitches each full
/// diagonal is split into ([`FILL_MAX_STITCH_MM`]).
pub fn fill_lines(cell_mm: f64) -> (usize, usize) {
    let diagonal = cell_mm * std::f64::consts::SQRT_2;
    let split = (diagonal / FILL_MAX_STITCH_MM).ceil().max(1.0) as usize;
    let wanted = (cell_mm / (std::f64::consts::SQRT_2 * FILL_SPACING_MM)).ceil();
    (wanted.max(1.0) as usize, split)
}

/// OpenStitch mode: cells `a..b` of grid row `row` as filled crosses, with as many lines per
/// cell and direction as [`fill_lines`] wants and the run's needle points stay within
/// [`FILL_MAX_DENSITY`] per mm² of its area (one line each way at the least; a short run pays
/// for the zigzags' clipped ends, so it gets fewer lines than a long one).
#[allow(clippy::too_many_arguments)]
fn sew_filled_run(
    out: &mut Vec<Stitch>,
    frame: &Frame,
    cell: f64,
    row: usize,
    a: usize,
    b: usize,
    kind: CrossType,
) {
    let cell_mm = cell / 10.0;
    let (wanted, split) = fill_lines(cell_mm);
    let budget = FILL_MAX_DENSITY * (b - a) as f64 * cell_mm * cell_mm;
    let mut k = wanted;
    loop {
        let run = filled_run(frame, cell, row, a, b, kind, k, split);
        if k == 1 || run.len() as f64 <= budget {
            out.extend(run);
            return;
        }
        k -= 1;
    }
}

/// Cells `a..b` of grid row `row` as filled crosses of `k` lines per cell and direction: the
/// "\" lines of the run are `x − y = i / k` and the "/" lines `x + y = i / k` (in cells from
/// the grid's top-left corner, y down), so the bands continue from cell to cell and from row to
/// row. They are sewn as `2k` zigzags between the row's top and bottom edges, each a phase
/// `j / k` further right; a zigzag alternates "\" and "/" diagonals and is clipped to the run,
/// so every stitch is a whole diagonal except at the run's ends. The zigzags go right and back
/// in turn. Half crosses sew the "/" lines only, up one and down the next. Diagonals are split
/// into `split` stitches; a needle point nearer than [`MIN_STITCH_MM`] to the previous one is
/// left out, so the short steps between zigzags along the run's ends merge into the next
/// diagonal.
#[allow(clippy::too_many_arguments)]
fn filled_run(
    frame: &Frame,
    cell: f64,
    row: usize,
    a: usize,
    b: usize,
    kind: CrossType,
    k: usize,
    split: usize,
) -> Vec<Stitch> {
    let (a, b, top) = (a as f64, b as f64, row as f64);
    let kf = k as f64;
    // Polylines in cell units, (column, row).
    let mut lines: Vec<Vec<(f64, f64)>> = Vec::new();
    match kind {
        CrossType::Full => {
            for j in 0..2 * k {
                let phase = j as f64 / kf;
                // Vertices at a + phase + m, on the top edge for even m and the bottom for odd.
                let vertex = |m: i64| {
                    let y = if m.rem_euclid(2) == 0 { top } else { top + 1.0 };
                    (a + phase + m as f64, y)
                };
                let mut line = Vec::new();
                let last = (b - a).ceil() as i64 + 2;
                for m in -2..last {
                    let (p, q) = (vertex(m), vertex(m + 1));
                    if q.0 <= a || p.0 >= b {
                        continue;
                    }
                    let at = |x: f64| (x, p.1 + (q.1 - p.1) * (x - p.0) / (q.0 - p.0));
                    if line.is_empty() {
                        line.push(if p.0 < a { at(a) } else { p });
                    }
                    line.push(if q.0 > b { at(b) } else { q });
                }
                if j % 2 == 1 {
                    line.reverse();
                }
                lines.push(line);
            }
        }
        CrossType::Half => {
            // "/" lines x + y = t through the run, t in (a, b + 1).
            let count = ((b - a + 1.0) * kf) as usize;
            for i in 1..count {
                let t = a + top + i as f64 / kf;
                let (y0, y1) = ((t - b).max(top), (t - a).min(top + 1.0));
                let mut line = vec![(t - y1, y1), (t - y0, y0)];
                if i % 2 == 0 {
                    line.reverse();
                }
                lines.push(line);
            }
        }
    }
    emit(&lines, frame, cell, split)
}

/// Stitches along `lines` (polylines in cells from the grid's top-left corner) in order, from a
/// jump to the first point: a line whose horizontal extent is a whole cell is split into `split`
/// equal stitches, a shorter one into as many as its share needs, and a needle point nearer
/// than [`MIN_STITCH_MM`] to the previous one is left out.
fn emit(lines: &[Vec<(f64, f64)>], frame: &Frame, cell: f64, split: usize) -> Vec<Stitch> {
    let mut stitches = Vec::new();
    let out = &mut stitches;
    let at = |(col, r): (f64, f64)| (frame.origin.x + col * cell, frame.origin.y + r * cell);
    let min = MIN_STITCH_MM * 10.0;
    let mut last: Option<(f64, f64)> = None;
    let mut put = |out: &mut Vec<Stitch>, p: (f64, f64)| match last {
        None => {
            out.push(Stitch::new(p.0, p.1, Command::Jump));
            out.push(Stitch::new(p.0, p.1, Command::Stitch));
            last = Some(p);
        }
        Some(l) if (p.0 - l.0).powi(2) + (p.1 - l.1).powi(2) < min * min => {}
        Some(_) => {
            out.push(Stitch::new(p.0, p.1, Command::Stitch));
            last = Some(p);
        }
    };
    for line in lines {
        let mut prev: Option<(f64, f64)> = None;
        for p in line.iter().copied().map(at) {
            if let Some(q) = prev {
                // A whole diagonal is split into `split` equal stitches; a clipped one into as
                // many as its share of the diagonal needs.
                let pieces = ((p.0 - q.0).abs() / cell * split as f64).ceil().max(1.0) as usize;
                for s in 1..pieces {
                    let f = s as f64 / pieces as f64;
                    put(out, (q.0 + (p.0 - q.0) * f, q.1 + (p.1 - q.1) * f));
                }
            }
            put(out, p);
            prev = Some(p);
        }
    }
    stitches
}

/// One half of a split cell (OpenStitch mode's part-crosses).
#[derive(Clone, Copy, Debug)]
struct Part {
    /// The cell is split along "/" (else "\").
    slash: bool,
    /// The half above the diagonal.
    above: bool,
    /// This half also sews the diagonal itself (the cell's other half does not).
    diagonal: bool,
}

/// One half of cell (`col`, `row`). Filled: lines parallel to the dividing diagonal, as far
/// apart as a filled cross's (see [`fill_lines`]), within [`FILL_MAX_DENSITY`] points per mm²
/// of the half, from the diagonal outwards. Not filled, the stitcher's three-quarter and
/// quarter crosses: a quarter stitch from the half's right-angled corner to the cell centre,
/// and the half that takes the diagonal sews it whole from there.
fn sew_part(
    out: &mut Vec<Stitch>,
    frame: &Frame,
    cell: f64,
    row: usize,
    col: usize,
    part: Part,
    fill: bool,
) {
    let (c, r) = (col as f64, row as f64);
    // The diagonal's ends and the half's right-angled corner, in cell units.
    let (d0, d1, corner) = match (part.slash, part.above) {
        (true, true) => ((0.0, 1.0), (1.0, 0.0), (0.0, 0.0)),
        (true, false) => ((0.0, 1.0), (1.0, 0.0), (1.0, 1.0)),
        (false, true) => ((0.0, 0.0), (1.0, 1.0), (1.0, 0.0)),
        (false, false) => ((0.0, 0.0), (1.0, 1.0), (0.0, 1.0)),
    };
    let at = |(x, y): (f64, f64)| (c + x, r + y);
    if !fill {
        let mut line = vec![at(corner), at((0.5, 0.5))];
        if part.diagonal {
            line.extend([at(d0), at(d1)]);
        }
        out.extend(emit(&[line], frame, cell, 1));
        return;
    }
    let cell_mm = cell / 10.0;
    let (wanted, split) = fill_lines(cell_mm);
    let budget = FILL_MAX_DENSITY * cell_mm * cell_mm / 2.0;
    let mut k = wanted;
    loop {
        // Line j of k: the diagonal moved j / k of the way to the corner.
        let first = usize::from(!part.diagonal);
        let lines: Vec<Vec<(f64, f64)>> = (first..k)
            .map(|j| {
                let f = j as f64 / k as f64;
                let towards =
                    |(x, y): (f64, f64)| at((x + (corner.0 - x) * f, y + (corner.1 - y) * f));
                let line = vec![towards(d0), towards(d1)];
                if j % 2 == 1 {
                    line.into_iter().rev().collect()
                } else {
                    line
                }
            })
            .collect();
        let run = emit(&lines, frame, cell, split);
        if k == 1 || run.len() as f64 <= budget + 2.0 {
            out.extend(run);
            return;
        }
        k -= 1;
    }
}
