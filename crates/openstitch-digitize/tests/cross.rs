mod common;

use common::*;
use openstitch_core::Command;
use openstitch_design::{Compiler, Design, DesignPage};
use openstitch_digitize::cross::{self, CrossStitch, CrossType};
use openstitch_digitize::{DigitizeError, Frame, Mode, Raster};

#[test]
fn grid_cells_colours_and_crosses() {
    // 40×20 px at 0.5 mm/px = 20×10 mm → 8×4 cells of 2.5 mm (5 px). Red covers cells
    // 1–3 of rows 1–2, blue cell 6 of row 1; white background.
    let img = rects(40, 20, WHITE, &[(5, 5, 20, 15, RED), (30, 5, 35, 10, BLUE)]);
    let frame = Frame::new(5.0, 5.0, 20.0, 40);
    let out = cross::convert(
        &img,
        &frame,
        &CrossStitch {
            colors: 3,
            ..Default::default()
        },
    )
    .expect("converts");
    assert_eq!((out.columns, out.rows), (8, 4));
    assert_eq!(out.cells, vec![6, 1]);
    assert_eq!(out.objects.len(), 2);
    assert!(out.background.is_some());
    let red = out.objects[0].stitch_block.as_ref().expect("block");
    // 2 runs of 3 cells: jump + anchor + 3×3 forward + 3×3 back = 20 each (six stitches a
    // cell, as PE-DESIGN 11.50 sews them), each run tied in and locked as PE-DESIGN's are.
    assert_eq!(unlocked(&red.stitches).len(), 2 * 20);
    assert!(red.stitches.len() > 2 * 20);
    assert_eq!(
        red.stitches
            .iter()
            .filter(|s| s.command == Command::Jump)
            .count(),
        2
    );
    // Every needle point is a cell corner or a cell centre inside the frame, and every
    // stitch lies on a diagonal (the tie-ins and locks step along one).
    let red_cells = unlocked(&red.stitches);
    for s in &red_cells {
        let (u, v) = ((s.x - 50.0) / 12.5, (s.y - 50.0) / 12.5);
        assert!(
            u.fract() == 0.0 && v.fract() == 0.0 && (u as i64 % 2 == v as i64 % 2),
            "{s:?}"
        );
        assert!(s.x >= 50.0 && s.x <= 50.0 + 100.0 && s.y >= 50.0 && s.y <= 150.0);
    }
    for w in red_cells.windows(2) {
        if w[1].command == Command::Stitch && (w[0].x, w[0].y) != (w[1].x, w[1].y) {
            assert_eq!((w[1].x - w[0].x).abs(), (w[1].y - w[0].y).abs(), "{w:?}");
        }
    }
    let blue = out.objects[1].stitch_block.as_ref().expect("block");
    let pts: Vec<(f64, f64)> = unlocked(&blue.stitches)
        .iter()
        .map(|s| (s.x, s.y))
        .collect();
    // Cell (6, 1) with the frame at (5, 5) mm: TL (200,75) → centre → BL (200,100) → "/" to
    // TR (225,75) → centre → BR (225,100) → "\" to TL.
    assert_eq!(
        pts,
        vec![
            (200.0, 75.0),
            (200.0, 75.0),
            (212.5, 87.5),
            (200.0, 100.0),
            (225.0, 75.0),
            (212.5, 87.5),
            (225.0, 100.0),
            (200.0, 75.0)
        ]
    );
}

#[test]
fn half_crosses_and_determinism() {
    let img = rects(10, 10, WHITE, &[(0, 0, 10, 5, BLACK)]);
    let frame = Frame::new(0.0, 0.0, 10.0, 10);
    let p = CrossStitch {
        cell_mm: 5.0,
        colors: 2,
        kind: CrossType::Half,
        ..Default::default()
    };
    let a = cross::convert(&img, &frame, &p).expect("converts");
    assert_eq!(a, cross::convert(&img, &frame, &p).expect("converts"));
    // Colours sew brightest first (PE-DESIGN's order): the black row is the dark thread's.
    let black = a
        .threads
        .iter()
        .position(|t| t.red < 64)
        .expect("black thread");
    let b = a.objects[black].stitch_block.as_ref().expect("block");
    let pts: Vec<(f64, f64)> = unlocked(&b.stitches).iter().map(|s| (s.x, s.y)).collect();
    assert_eq!(
        pts,
        vec![
            (0.0, 50.0),
            (0.0, 50.0),
            (50.0, 0.0),
            (50.0, 50.0),
            (100.0, 0.0)
        ]
    );
    let mut d = Design::new("t", DesignPage::default());
    for o in a.objects {
        d.add(o);
    }
    assert!(Compiler::default().compile(&d).count(Command::Stitch) >= 4);
}

#[test]
fn hostile_grids_fail_cleanly() {
    let img = rects(4, 4, RED, &[]);
    // 4 px at 1000 mm wide with 1 mm cells → 1 000 000 cells.
    let huge = Frame::new(0.0, 0.0, 1000.0, 4);
    assert!(matches!(
        cross::convert(
            &img,
            &huge,
            &CrossStitch {
                cell_mm: 1.0,
                ..Default::default()
            }
        ),
        Err(DigitizeError::TooLarge(_))
    ));
    let clear = rects(4, 4, CLEAR, &[]);
    let frame = Frame::new(0.0, 0.0, 10.0, 4);
    assert!(matches!(
        cross::convert(&clear, &frame, &CrossStitch::default()),
        Err(DigitizeError::Empty(_))
    ));
    assert!(matches!(
        cross::convert(&rects(4, 4, WHITE, &[]), &frame, &CrossStitch::default()),
        Err(DigitizeError::Empty(_))
    ));
    // Tiny frame: one cell.
    let tiny = Frame::new(0.0, 0.0, 0.1, 4);
    let out = cross::convert(
        &img,
        &tiny,
        &CrossStitch {
            skip_background: false,
            ..Default::default()
        },
    )
    .expect("one cell");
    assert_eq!(out.cells, vec![1]);
}

/// #132: the documented defaults, and the needle points per cell they imply.
#[test]
fn defaults_are_documented_and_configurable() {
    let d = CrossStitch::default();
    assert_eq!(d.cell_mm, 2.5);
    assert_eq!(d.cell_mm, CrossStitch::CELL_MM);
    assert_eq!(
        d.colors, 0,
        "automatic, as PE-DESIGN fills in Max. Number of Colors"
    );
    assert_eq!(d.colors, CrossStitch::COLORS);
    assert_eq!(d.kind, CrossType::Full);
    assert!(!d.triple);
    assert!(d.skip_background);
    assert_eq!(d.stitches_per_cell(), 6, "PE-DESIGN's six stitches a cell");
    let triple = CrossStitch {
        triple: true,
        ..Default::default()
    };
    assert_eq!(triple.stitches_per_cell(), 10);
    let half = CrossStitch {
        kind: CrossType::Half,
        ..Default::default()
    };
    assert_eq!(half.stitches_per_cell(), 2);
}

/// #132: the grid of a sample image is stable: 20 × 10 mm at 2.5 mm cells is 8 × 4 cells, and
/// a 3 mm cell gives round(20 / 3) × round(10 / 3) = 7 × 3.
#[test]
fn grid_follows_the_cell_size() {
    let img = rects(40, 20, WHITE, &[(5, 5, 20, 15, RED)]);
    let frame = Frame::new(0.0, 0.0, 20.0, 40);
    let a = cross::convert(&img, &frame, &CrossStitch::default()).expect("converts");
    assert_eq!((a.columns, a.rows), (8, 4));
    let b = cross::convert(
        &img,
        &frame,
        &CrossStitch {
            cell_mm: 3.0,
            ..Default::default()
        },
    )
    .expect("converts");
    assert_eq!((b.columns, b.rows), (7, 3));
    assert_eq!(
        a,
        cross::convert(&img, &frame, &CrossStitch::default()).expect("converts")
    );
}

/// #132: colour reduction is Auto Punch's census prepass: a one-pixel speck inside a cell takes
/// its 3×3 modal colour and never reaches the vote, so the cell is its surround's colour and
/// the speck's colour is not a thread at all.
#[test]
fn colours_come_from_the_shared_census() {
    // 10 px per cell (1 mm/px, 10 mm cells): one blue pixel in each red cell.
    let img = rects(
        40,
        40,
        WHITE,
        &[
            (0, 0, 20, 20, RED),
            (5, 5, 6, 6, BLUE),
            (15, 15, 16, 16, BLUE),
        ],
    );
    let frame = Frame::new(0.0, 0.0, 40.0, 40);
    let out = cross::convert(
        &img,
        &frame,
        &CrossStitch {
            cell_mm: 10.0,
            colors: 8,
            ..Default::default()
        },
    )
    .expect("converts");
    assert_eq!((out.columns, out.rows), (4, 4));
    assert_eq!(out.threads.len(), 1, "{:?}", out.threads);
    assert!(out.threads[0].red > 150 && out.threads[0].blue < 100);
    assert_eq!(out.cells, vec![4]);
}

/// #132: triple sews every full diagonal there, back and there again; the needle still ends
/// where the single cross does, so the run continues the same way.
#[test]
fn triple_sews_each_diagonal_three_times() {
    let img = rects(10, 10, WHITE, &[(0, 0, 5, 5, BLACK)]);
    let frame = Frame::new(0.0, 0.0, 10.0, 10);
    let single = cross::convert(
        &img,
        &frame,
        &CrossStitch {
            cell_mm: 5.0,
            colors: 2,
            ..Default::default()
        },
    )
    .expect("converts");
    let triple = cross::convert(
        &img,
        &frame,
        &CrossStitch {
            cell_mm: 5.0,
            colors: 2,
            triple: true,
            ..Default::default()
        },
    )
    .expect("converts");
    let pts = |c: &cross::Converted| -> Vec<(f64, f64)> {
        unlocked(&c.objects[0].stitch_block.as_ref().expect("block").stitches)
            .iter()
            .map(|s| (s.x, s.y))
            .collect()
    };
    // One cell: TL anchor, halves through the centre to BL, "/" to TR, halves through the
    // centre to BR, "\" to TL.
    assert_eq!(
        pts(&single),
        vec![
            (0.0, 0.0),
            (0.0, 0.0),
            (25.0, 25.0),
            (0.0, 50.0),
            (50.0, 0.0),
            (25.0, 25.0),
            (50.0, 50.0),
            (0.0, 0.0)
        ]
    );
    assert_eq!(
        pts(&triple),
        vec![
            (0.0, 0.0),
            (0.0, 0.0),
            (25.0, 25.0),
            (0.0, 50.0),
            (50.0, 0.0),
            (0.0, 50.0),
            (50.0, 0.0),
            (25.0, 25.0),
            (50.0, 50.0),
            (0.0, 0.0),
            (50.0, 50.0),
            (0.0, 0.0)
        ]
    );
}

/// PE-DESIGN 11.50's Cross Stitch on the owner's four-shape image (round-4 capture
/// `crossstitch-default.pes`): six stitches on the diagonals per cell, half of them halves
/// through the centre, none along a cell side.
#[test]
fn cells_are_six_diagonal_stitches_as_pe_design_sews_them() {
    let img = rects(30, 30, WHITE, &[(5, 5, 25, 25, RED)]);
    let out = cross::convert(
        &img,
        &Frame::new(0.0, 0.0, 30.0, 30),
        &CrossStitch::default(),
    )
    .expect("converts");
    let cells: usize = out.cells.iter().sum();
    let b = out.objects[0].stitch_block.as_ref().expect("block");
    let mut halves = 0;
    let mut fulls = 0;
    for w in unlocked(&b.stitches).windows(2) {
        if w[1].command != Command::Stitch || (w[0].x, w[0].y) == (w[1].x, w[1].y) {
            continue;
        }
        let (dx, dy) = ((w[1].x - w[0].x).abs(), (w[1].y - w[0].y).abs());
        assert_eq!(dx, dy, "only diagonals: {w:?}");
        if dx == 12.5 {
            halves += 1;
        } else {
            assert_eq!(dx, 25.0);
            fulls += 1;
        }
    }
    assert_eq!((fulls, halves), (2 * cells, 4 * cells));
}

fn open_stitch(cell_mm: f64, colors: usize) -> CrossStitch {
    CrossStitch {
        cell_mm,
        colors,
        mode: Mode::OpenStitch,
        ..Default::default()
    }
}

/// B16.1 T14: OpenStitch mode colours a cell by its mean colour in CIELAB. A cell of fine black
/// and white checks is grey to the eye: PE-DESIGN mode's vote makes it black or white, the
/// mean makes it the image's grey.
#[test]
fn open_stitch_cells_take_their_mean_colour() {
    const GREY: [u8; 4] = [119, 119, 119, 255];
    // 40×20 px at 0.5 mm/px: left half 1-pixel checks, right half grey; 5 mm cells.
    let img = Raster::from_fn(40, 20, |x, y| {
        if x >= 20 {
            GREY
        } else if (x + y) % 2 == 0 {
            BLACK
        } else {
            WHITE
        }
    })
    .expect("valid size");
    let frame = Frame::new(0.0, 0.0, 20.0, 40);
    let p = CrossStitch {
        skip_background: false,
        ..open_stitch(5.0, 3)
    };
    let os = cross::convert(&img, &frame, &p).expect("converts");
    assert_eq!(os.threads.len(), 1, "{:?}", os.threads);
    let g = &os.threads[0];
    assert!((90..150).contains(&g.red) && g.red == g.green, "{g:?}");
    let pe = cross::convert(
        &img,
        &frame,
        &CrossStitch {
            mode: Mode::PeDesign,
            ..p
        },
    )
    .expect("converts");
    assert!(pe.threads.len() > 1, "{:?}", pe.threads);
}

/// B16.1 T14: filled crosses cover each cell with parallel diagonals in both directions, stay
/// inside their run, sew no stitch under 1 mm or over the longest allowed, and keep within
/// the needle-point budget.
#[test]
fn open_stitch_fills_the_cells() {
    // 40×20 px at 0.5 mm/px = 20×10 mm → 8×4 cells of 2.5 mm; red covers cells 1–5 of row 1.
    let img = rects(40, 20, WHITE, &[(5, 5, 30, 10, RED)]);
    let frame = Frame::new(0.0, 0.0, 20.0, 40);
    let out = cross::convert(&img, &frame, &open_stitch(2.5, 2)).expect("converts");
    assert_eq!(out.cells, vec![5]);
    let red = out.objects[0].stitch_block.as_ref().expect("block");
    let s = &red.stitches;
    assert_eq!(s[0].command, Command::Jump);
    assert_eq!(
        s.iter().filter(|s| s.command == Command::Jump).count(),
        1,
        "one run, one jump"
    );
    for p in s {
        assert!(
            (25.0 - 1e-9..=150.0 + 1e-9).contains(&p.x)
                && (25.0 - 1e-9..=50.0 + 1e-9).contains(&p.y),
            "{p:?}"
        );
    }
    let mut slopes = (0usize, 0usize);
    for w in s.windows(2).skip(1) {
        let (dx, dy) = (w[1].x - w[0].x, w[1].y - w[0].y);
        let len = (dx * dx + dy * dy).sqrt();
        assert!(
            (cross::MIN_STITCH_MM * 10.0 - 1e-9..=10.0 * cross::FILL_MAX_STITCH_MM).contains(&len),
            "{w:?}"
        );
        // Whole diagonals of the zigzags: "\" and "/" about equally often.
        if (dx.abs() - dy.abs()).abs() < 1e-9 && (dx.abs() - 25.0).abs() < 1e-9 {
            if dx * dy > 0.0 {
                slopes.0 += 1;
            } else {
                slopes.1 += 1;
            }
        }
    }
    assert!(
        slopes.0.min(slopes.1) >= 15 && slopes.0.abs_diff(slopes.1) <= 2,
        "{slopes:?}"
    );
    // Six lines per cell and direction at 2.5 mm, if the budget allows: at most 2 needle
    // points per mm² of the run's 31.25 mm².
    assert_eq!(cross::fill_lines(2.5), (6, 1));
    assert!(
        s.len() as f64 <= cross::FILL_MAX_DENSITY * 31.25,
        "{}",
        s.len()
    );
    // Deterministic.
    assert_eq!(
        out,
        cross::convert(&img, &frame, &open_stitch(2.5, 2)).expect("converts")
    );
}

/// B16.1 T14: lines get fewer where the density budget runs out, never below one each way;
/// long diagonals are split; half crosses sew one direction; `fill: false` sews PE-DESIGN's
/// crosses on the mean-colour cells.
#[test]
fn open_stitch_fill_limits_and_options() {
    assert_eq!(cross::fill_lines(1.0), (3, 1));
    // A 10 mm diagonal is 14.1 mm: three stitches of 4.7 mm.
    assert_eq!(cross::fill_lines(10.0).1, 3);
    let img = rects(10, 10, WHITE, &[(0, 0, 5, 5, BLACK)]);
    let frame = Frame::new(0.0, 0.0, 10.0, 10);
    // One 1 mm cell: the budget (2 points) leaves one line each way.
    let one = rects(20, 20, WHITE, &[(0, 0, 2, 2, BLACK)]);
    let tiny = cross::convert(&one, &Frame::new(0.0, 0.0, 10.0, 20), &open_stitch(1.0, 2))
        .expect("converts");
    let block = tiny.objects[0].stitch_block.as_ref().expect("block");
    assert!(unlocked(&block.stitches).len() <= 6, "{:?}", block.stitches);
    for (kind, lo) in [(CrossType::Full, 2), (CrossType::Half, 1)] {
        let out = cross::convert(
            &img,
            &frame,
            &CrossStitch {
                kind,
                ..open_stitch(5.0, 2)
            },
        )
        .expect("converts");
        let s = &out.objects[0]
            .stitch_block
            .as_ref()
            .expect("block")
            .stitches;
        let (mut down, mut up) = (0, 0);
        for w in s.windows(2).skip(1) {
            let (dx, dy) = (w[1].x - w[0].x, w[1].y - w[0].y);
            if dx * dy > 0.0 {
                down += 1;
            } else if dx * dy < 0.0 {
                up += 1;
            }
            assert!(dx.hypot(dy) <= 10.0 * cross::FILL_MAX_STITCH_MM + 1e-9);
        }
        let dirs = usize::from(down > 3) + usize::from(up > 3);
        assert_eq!(dirs, lo, "{kind:?}: {down} \\, {up} /");
    }
    let plain = cross::convert(
        &img,
        &frame,
        &CrossStitch {
            fill: false,
            ..open_stitch(5.0, 2)
        },
    )
    .expect("converts");
    let pe = cross::convert(
        &img,
        &frame,
        &CrossStitch {
            cell_mm: 5.0,
            colors: 2,
            ..Default::default()
        },
    )
    .expect("converts");
    // The same stitches; only the locks' steps differ (1 mm in OpenStitch mode, PE-DESIGN's
    // 0.3 mm otherwise).
    let blocks = |c: &cross::Converted| -> Vec<Vec<openstitch_core::Stitch>> {
        c.objects
            .iter()
            .map(|o| unlocked(&o.stitch_block.as_ref().expect("block").stitches))
            .collect()
    };
    assert_eq!(blocks(&plain), blocks(&pe));
}

/// B16.1 T14: with `part_crosses`, a cell whose halves along a diagonal differ becomes two
/// part-crosses: a red triangle on white sews red only above the diagonal (filled, or as a
/// three-quarter cross without fill), and whole cells stay whole.
#[test]
fn open_stitch_part_crosses_split_cells_along_a_diagonal() {
    // 50×30 px at 0.5 mm/px, 5 mm cells (10 px), 5×3 cells on white: cell (1, 1) red, cell
    // (2, 1) red above its "/" diagonal.
    let img = Raster::from_fn(50, 30, |x, y| {
        let red = (10..20).contains(&y)
            && ((10..20).contains(&x) || ((20..30).contains(&x) && (x - 20) + (y - 10) < 10));
        if red { RED } else { WHITE }
    })
    .expect("valid size");
    let frame = Frame::new(0.0, 0.0, 25.0, 50);
    let p = CrossStitch {
        part_crosses: true,
        ..open_stitch(5.0, 2)
    };
    // Cell (2, 1) spans 100..150 × 50..100 in design units; above "/": (x − 100) + (y − 50) ≤ 50.
    let above = |x: f64, y: f64| (x - 100.0) + (y - 50.0) <= 50.0 + 1e-6;
    for fill in [true, false] {
        let out =
            cross::convert(&img, &frame, &CrossStitch { fill, ..p.clone() }).expect("converts");
        assert_eq!(out.cells, vec![2], "fill {fill}");
        let red = &out.objects[0]
            .stitch_block
            .as_ref()
            .expect("block")
            .stitches;
        let in_cell: Vec<_> = red.iter().filter(|s| s.x > 100.0 + 1e-9).collect();
        assert!(!in_cell.is_empty(), "fill {fill}");
        for s in in_cell {
            assert!(above(s.x, s.y), "fill {fill}: {s:?}");
        }
    }
    // Off by default: the half-red cell is a whole cross of one thread or none.
    let whole = cross::convert(&img, &frame, &open_stitch(5.0, 2)).expect("converts");
    let red = &whole.objects[0]
        .stitch_block
        .as_ref()
        .expect("block")
        .stitches;
    let below = red.iter().any(|s| s.x > 100.0 + 1e-9 && !above(s.x, s.y));
    assert_eq!(whole.cells, vec![if below { 2 } else { 1 }]);
}

/// Pixel art for #994: a 10 × 8 art-pixel picture, each art pixel 12 image pixels, starting
/// 5 pixels in (the art's grid is not the image's), with a soft one-pixel seam between art
/// pixels as an upscaler leaves. The art is white with a red block (art pixels 2–5 × 2–4) and
/// one blue art pixel (7, 2), plus a 2 × 2 patch of red, red, yellow, white at (2–3, 6–7).
fn pixel_art() -> Raster {
    const S: u32 = 12;
    const O: u32 = 5;
    let art = |i: i64, j: i64| -> [u8; 4] {
        match (i, j) {
            (2..=5, 2..=4) => RED,
            (7, 2) => BLUE,
            (2, 6) | (3, 6) | (2, 7) => RED,
            (3, 7) => YELLOW,
            _ => WHITE,
        }
    };
    Raster::from_fn(10 * S + 2 * O, 8 * S + 2 * O, |x, y| {
        let (i, j) = (
            (i64::from(x) - i64::from(O)).div_euclid(i64::from(S)),
            (i64::from(y) - i64::from(O)).div_euclid(i64::from(S)),
        );
        let c = art(i, j);
        let seam = (i64::from(x) - i64::from(O)).rem_euclid(i64::from(S)) == 0
            || (i64::from(y) - i64::from(O)).rem_euclid(i64::from(S)) == 0;
        if seam {
            // Blend with the art pixel up and left: neither colour.
            let n = art(i - 1, j - 1);
            [0, 1, 2, 3].map(|k| ((u16::from(c[k]) + u16::from(n[k])) / 2) as u8)
        } else {
            c
        }
    })
    .expect("valid size")
}

/// #994: in OpenStitch mode Cross Stitch finds pixel art's own grid and, with `pixel_grid`,
/// lays one cross per art pixel on it (cell side = the art pixel, whatever `cell_mm` says),
/// with every cross centred on an art pixel's centre.
#[test]
fn open_stitch_follows_pixel_arts_own_grid() {
    let img = pixel_art();
    // 0.25 mm per image pixel: art pixels of 3 mm.
    let frame = Frame::new(0.0, 0.0, img.width() as f64 * 0.25, img.width());
    let plain = cross::convert(&img, &frame, &open_stitch(2.5, 4)).expect("converts");
    let art = plain.pixel_art.clone().expect("pixel art found");
    assert_eq!((art.columns, art.rows), (10, 8), "{art:?}");
    assert!((art.pixel_mm[0] - 3.0).abs() < 0.05 && (art.pixel_mm[1] - 3.0).abs() < 0.05);
    assert!(!art.followed);
    let p = CrossStitch {
        pixel_grid: true,
        fill: false,
        ..open_stitch(2.5, 4)
    };
    let out = cross::convert(&img, &frame, &p).expect("converts");
    assert!(out.pixel_art.as_ref().expect("found").followed);
    assert_eq!((out.columns, out.rows), (10, 8));
    // Red: 12 block + 3 patch; blue 1; yellow 1. White is the background.
    let mut cells = out.cells.clone();
    cells.sort_unstable();
    assert_eq!(cells, vec![1, 1, 15], "{:?}", out.threads);
    // Every cross centre is an art pixel's centre: 5 + 12 k + 6 image pixels = (11 + 12 k) ×
    // 2.5 design units.
    for o in &out.objects {
        let st = &o.stitch_block.as_ref().expect("block").stitches;
        for s in unlocked(st) {
            let u = (s.x / 2.5 - 5.0) / 6.0;
            let v = (s.y / 2.5 - 5.0) / 6.0;
            assert!(
                (u - u.round()).abs() < 0.05 && (v - v.round()).abs() < 0.05,
                "{s:?} is not on the art's half-pixel lattice"
            );
        }
    }
    // Deterministic.
    assert_eq!(cross::convert(&img, &frame, &p).expect("again"), out);
}

/// #994: a cross over several art pixels takes the colour of one of them (the nearest their
/// mean), never a blend none of them has: the red, red, red, yellow patch is red, not orange.
#[test]
fn open_stitch_art_grid_cells_take_one_of_their_own_colours() {
    let img = pixel_art();
    let frame = Frame::new(0.0, 0.0, img.width() as f64 * 0.25, img.width());
    let p = CrossStitch {
        pixel_grid: true,
        pixels_per_cross: 2,
        fill: false,
        ..open_stitch(2.5, 6)
    };
    let out = cross::convert(&img, &frame, &p).expect("converts");
    assert_eq!((out.columns, out.rows), (5, 4));
    for t in &out.threads {
        let [r, g, b] = [t.red, t.green, t.blue];
        let orange = r > 200 && (90..=170).contains(&g) && b < 90;
        assert!(!orange, "a blend: {t:?}");
    }
    // The 2 × 2 crosses are 6 mm: needle points on a 3 mm lattice from the art's corner.
    for o in &out.objects {
        for s in unlocked(&o.stitch_block.as_ref().expect("block").stitches) {
            let u = (s.x / 2.5 - 5.0) / 12.0;
            assert!((u - u.round()).abs() < 0.05, "{s:?}");
        }
    }
}

/// #994: PE-compatible mode never looks for the art's grid (PE-DESIGN keeps its plain grid on
/// pixel art), so its output is the same with `pixel_grid` on; and a picture that is not pixel
/// art keeps the plain grid in OpenStitch mode too.
#[test]
fn pixel_grid_leaves_pe_mode_and_other_pictures_alone() {
    let img = pixel_art();
    let frame = Frame::new(0.0, 0.0, img.width() as f64 * 0.25, img.width());
    let pe = CrossStitch {
        colors: 4,
        ..Default::default()
    };
    let a = cross::convert(&img, &frame, &pe).expect("converts");
    let b = cross::convert(
        &img,
        &frame,
        &CrossStitch {
            pixel_grid: true,
            ..pe.clone()
        },
    )
    .expect("converts");
    assert_eq!(a, b);
    assert_eq!(a.pixel_art, None);
    let disc = Raster::from_fn(200, 200, |x, y| {
        let (dx, dy) = (x as f64 - 100.0, y as f64 - 100.0);
        if dx * dx + dy * dy < 70.0 * 70.0 {
            RED
        } else {
            WHITE
        }
    })
    .expect("valid size");
    let frame = Frame::new(0.0, 0.0, 50.0, 200);
    let p = CrossStitch {
        pixel_grid: true,
        ..open_stitch(2.5, 2)
    };
    let out = cross::convert(&disc, &frame, &p).expect("converts");
    assert_eq!(out.pixel_art, None);
    assert_eq!(
        out,
        cross::convert(&disc, &frame, &open_stitch(2.5, 2)).expect("plain")
    );
}

/// #994: the repository's pixel strawberry (`examples/images/g07-pixel-strawberry.png`, an
/// upscaled picture whose art pixels are about 34.7 × 33.2 image pixels) is found as pixel
/// art; no photograph or drawing among the example images is.
#[test]
fn the_example_pixel_art_is_found_and_nothing_else() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/images");
    let mut found = Vec::new();
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .expect("images")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("png" | "jpg")))
        .collect();
    names.sort();
    for path in names {
        let img = Raster::decode(&std::fs::read(&path).expect("read")).expect("decode");
        if let Some(g) = openstitch_digitize::pixelgrid::detect(&img.fit(1024)) {
            found.push((
                path.file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned(),
                g,
            ));
        }
    }
    assert_eq!(found.len(), 1, "{found:?}");
    let (name, g) = &found[0];
    assert_eq!(name, "g07-pixel-strawberry.png");
    assert!(
        (g.x.pitch - 34.7).abs() < 1.0 && (g.y.pitch - 33.2).abs() < 1.0,
        "{g:?}"
    );
}
