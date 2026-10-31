mod common;

use common::*;
use openstitch_core::Thread;
use openstitch_design::{Compiler, Design, DesignPage, LineType, RegionType};
use openstitch_digitize::center::{self, FigureOptions, LineImageOptions, RegionSew, SewSettings};
use openstitch_digitize::{DigitizeError, Frame, Mode, Raster};

/// 60×60 px white with a 3-px black square outline (outer 10..50); `inside` fills the middle.
fn square(inside: [u8; 4]) -> Raster {
    rects(
        60,
        60,
        WHITE,
        &[(10, 10, 50, 50, BLACK), (13, 13, 47, 47, inside)],
    )
}

fn run(
    img: &Raster,
    settings: &SewSettings,
) -> (center::Figure, Vec<openstitch_design::DesignObject>) {
    let frame = Frame::new(0.0, 0.0, 60.0, 60); // 1 mm per pixel
    let li = center::line_image(img, &LineImageOptions::default());
    let fig = center::figure(img, &li, &frame, &FigureOptions::default()).expect("figure");
    let objects = center::sew(&fig, settings);
    (fig, objects)
}

#[test]
fn line_image_thresholds_dark_pixels() {
    let li = center::line_image(&square(WHITE), &LineImageOptions::default());
    assert_eq!((li.width, li.height), (60, 60));
    assert_eq!(li.line_pixels(), 40 * 40 - 34 * 34);
    let none = center::line_image(
        &square(WHITE),
        &LineImageOptions {
            threshold: 0.0,
            ..Default::default()
        },
    );
    assert_eq!(none.line_pixels(), 0);
}

#[test]
fn thinning_leaves_a_one_pixel_line() {
    // A 5-pixel-wide horizontal bar, 30 long.
    let mut mask = vec![false; 40 * 9];
    for y in 2..7 {
        for x in 5..35 {
            mask[y * 40 + x] = true;
        }
    }
    let s = center::thin(&mask, 40, 9);
    for x in 8..32 {
        assert_eq!((0..9).filter(|&y| s[y * 40 + x]).count(), 1, "column {x}");
    }
    let paths = center::skeleton_paths(&s, 40, 9);
    assert_eq!(paths.len(), 1);
    assert!(!paths[0].1);
}

#[test]
fn outline_becomes_one_closed_line_and_one_region() {
    let (fig, objects) = run(&square(WHITE), &SewSettings::default());
    assert_eq!(fig.lines.len(), 1);
    assert!(fig.lines[0].closed);
    assert_eq!(fig.regions.len(), 1);
    assert!(
        (fig.line_width_mm - 3.0).abs() <= 1.0,
        "{}",
        fig.line_width_mm
    );
    // The centre line of the 3-px ring lies 11.5 px from the image edge.
    for p in fig.lines[0].path() {
        assert!(
            p.x >= 100.0 && p.x <= 500.0 && p.y >= 100.0 && p.y <= 500.0,
            "{p:?}"
        );
    }
    // Auto leaves the white (paper) region unsewn: only the line object.
    assert_eq!(objects.len(), 1);
    assert_eq!(
        objects[0].line.as_ref().map(|l| l.kind),
        Some(LineType::Running)
    );
    assert!(objects[0].region.is_none());
}

#[test]
fn coloured_region_is_filled_in_its_colour() {
    // Light enough (luma > 128) not to count as line pixels.
    let (fig, objects) = run(
        &square(YELLOW),
        &SewSettings {
            line: Some(LineType::Zigzag),
            ..Default::default()
        },
    );
    assert_eq!(fig.regions.len(), 1);
    assert_eq!(objects.len(), 2, "region, then lines");
    let region = objects[0].region.as_ref().expect("region");
    assert_eq!(region.kind, RegionType::Fill);
    assert!(
        region.thread.red > 150 && region.thread.green > 120 && region.thread.blue < 100,
        "{:?}",
        region.thread
    );
    let line = objects[1].line.as_ref().expect("line");
    assert_eq!(line.kind, LineType::Zigzag);
    assert!(
        (line.width - 3.0).abs() <= 1.0,
        "zigzag width follows the stroke: {}",
        line.width
    );
    // Region outline sits inside the ring.
    assert!(all_inside(&objects[..1], 125.0, 125.0, 475.0, 475.0, 1.0));
    let mut d = Design::new(
        "t",
        DesignPage {
            width: 600.0,
            height: 600.0,
        },
    );
    for o in objects {
        d.add(o);
    }
    assert_eq!(Compiler::default().compile(&d).threads.len(), 2);
}

#[test]
fn sew_settings_choose_types_and_threads() {
    let settings = SewSettings {
        line: None,
        region: RegionSew::Satin,
        region_thread: Some(Thread::rgb(0x00ff00)),
        ..Default::default()
    };
    let (_, objects) = run(&square(WHITE), &settings);
    assert_eq!(objects.len(), 1);
    let region = objects[0].region.as_ref().expect("region");
    assert_eq!(region.kind, RegionType::Satin);
    assert_eq!(region.thread.hex(), "#00ff00");
    let (_, none) = run(
        &square(WHITE),
        &SewSettings {
            line: None,
            region: RegionSew::None,
            ..Default::default()
        },
    );
    assert!(none.is_empty());
}

#[test]
fn junctions_split_open_lines() {
    // A "T": horizontal bar and a vertical bar hanging from its middle; nothing enclosed.
    let img = rects(
        60,
        60,
        WHITE,
        &[(5, 10, 55, 13, BLACK), (29, 10, 32, 55, BLACK)],
    );
    let (fig, objects) = run(&img, &SewSettings::default());
    assert!(fig.regions.is_empty());
    assert!(fig.lines.len() >= 2, "{}", fig.lines.len());
    assert!(fig.lines.iter().all(|c| !c.closed));
    assert_eq!(objects.len(), 1);
    assert!(all_inside(&objects, 50.0, 100.0, 550.0, 550.0, 5.0));
}

#[test]
fn deterministic_and_hostile() {
    let (a, _) = run(&square(YELLOW), &SewSettings::default());
    let (b, _) = run(&square(YELLOW), &SewSettings::default());
    assert_eq!(a, b);
    let frame = Frame::new(0.0, 0.0, 60.0, 60);
    let blank = rects(60, 60, WHITE, &[]);
    let li = center::line_image(&blank, &LineImageOptions::default());
    assert!(matches!(
        center::figure(&blank, &li, &frame, &FigureOptions::default()),
        Err(DigitizeError::Empty(_))
    ));
    let dot = rects(1, 1, BLACK, &[]);
    let li = center::line_image(&dot, &LineImageOptions::default());
    assert!(
        center::figure(
            &dot,
            &li,
            &Frame::new(0.0, 0.0, 1.0, 1),
            &FigureOptions::default()
        )
        .is_err()
    );
    // Noise: bounded result or a clean refusal.
    let noise = Raster::from_fn(500, 500, |x, y| {
        let v = ((x * 7919 + y * 104_729).wrapping_mul(2_654_435_761) >> 24) as u8;
        [v, v, v, 255]
    })
    .expect("size");
    let li = center::line_image(
        &noise,
        &LineImageOptions {
            resolution: 500,
            ..Default::default()
        },
    );
    match center::figure(
        &noise,
        &li,
        &Frame::new(0.0, 0.0, 100.0, 500),
        &FigureOptions {
            min_line_mm: 0.0,
            min_region_mm2: 0.0,
            ..Default::default()
        },
    ) {
        Ok(f) => assert!(f.lines.len() + f.regions.len() <= center::MAX_FIGURE_PARTS + 1),
        Err(e) => assert!(matches!(e, DigitizeError::TooLarge(_)), "{e}"),
    }
}

/// B16.1: OpenStitch mode's threshold is Otsu's: it falls between an image's two tones, so a
/// mid-grey drawing on a light-grey ground is found where the fixed 128 finds nothing.
#[test]
fn otsu_threshold_splits_the_two_tones() {
    const GROUND: [u8; 4] = [220, 220, 220, 255];
    const INK: [u8; 4] = [150, 150, 150, 255];
    let img = rects(
        60,
        60,
        GROUND,
        &[(10, 10, 50, 13, INK), (10, 30, 50, 33, INK)],
    );
    let t = center::otsu_threshold(&img, 600);
    assert!((151.0..=220.0).contains(&t), "{t}");
    let li = center::line_image(
        &img,
        &LineImageOptions {
            threshold: t,
            ..Default::default()
        },
    );
    assert_eq!(li.line_pixels(), 2 * 40 * 3);
    assert_eq!(
        center::line_image(&img, &LineImageOptions::default()).line_pixels(),
        0
    );
    // One tone: the default.
    assert_eq!(center::otsu_threshold(&rects(8, 8, WHITE, &[]), 600), 128.0);
}

/// B16.1: OpenStitch mode sews bold strokes as parallel running lines across a little more than
/// the measured median width and thin ones as triple running stitch; region threads are
/// matched by CIEDE2000; the PE-DESIGN path (`sew`) is `sew_with` in PE-DESIGN mode.
#[test]
fn open_stitch_lines_follow_the_stroke_width() {
    let (fig, objects) = run(&square(YELLOW), &SewSettings::default());
    // A 3 px ring at 1 mm/px: 3 mm strokes.
    let center::OpenLine::Parallel { width_mm, passes } = center::open_stitch_line(&fig) else {
        panic!("bold strokes are parallel lines");
    };
    assert!(
        (width_mm - fig.line_width_mm * center::BOLD_WIDTH_FACTOR).abs() < 1e-9,
        "{width_mm}"
    );
    let gap = (width_mm - 0.4) / (passes - 1) as f64;
    assert!(
        passes % 2 == 0 && gap <= center::PARALLEL_SPACING_MM,
        "{passes} {gap}"
    );
    let thin = center::Figure {
        line_width_mm: 0.3,
        ..fig.clone()
    };
    assert_eq!(center::open_stitch_line(&thin), center::OpenLine::Triple);
    assert_eq!(
        objects,
        center::sew_with(&fig, &SewSettings::default(), Mode::PeDesign)
    );
    let os = center::sew_with(&fig, &SewSettings::default(), Mode::OpenStitch);
    assert_eq!(os.len(), objects.len());
    let t = &os[0].region.as_ref().expect("region").thread;
    assert!(t.red > 150 && t.green > 120 && t.blue < 100, "{t:?}");
}

/// B16.1: the parallel lines of a closed ring are one open path per line, every point within
/// half the band of the original line, consecutive points at least 1 mm apart but for the
/// steps between passes, which are left out.
#[test]
fn parallel_lines_cover_the_band_in_one_path() {
    let (fig, _) = run(&square(WHITE), &SewSettings::default());
    assert_eq!(fig.lines.len(), 1);
    let lines = center::parallel_lines(&fig, 3.0, 4);
    assert_eq!(lines.len(), 1);
    let path = &lines[0].points;
    assert!(!lines[0].closed);
    // The ring's centre line is the square 11.5..48.5 mm (115..485 units); the band is
    // (3 − 0.4) / 2 = 1.3 mm either side of it.
    let original = fig.lines[0].path();
    for p in path {
        let d = original
            .windows(2)
            .map(|w| {
                let (a, b) = (w[0], w[1]);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let t =
                    (((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
            })
            .fold(f64::INFINITY, f64::min);
        assert!(d <= 13.0 * 2.0 + 1e-6, "{p:?} is {d} from the line");
    }
    let far = path
        .iter()
        .filter(|p| p.x < 105.0 || p.x > 495.0 || p.y < 105.0 || p.y > 495.0)
        .count();
    assert!(far > 0, "the outer pass lies outside the centre line");
    for w in path.windows(2) {
        assert!(w[0].distance(w[1]) >= 10.0 - 1e-6, "{w:?}");
    }
    // One pass with no band is the line itself, thinned.
    let one = center::parallel_lines(&fig, 0.0, 1);
    assert!(one[0].points.len() <= original.len());
}
