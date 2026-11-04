//! Coverage for two Photo Stitch 2 OpenStitch-mode claims `open_stitch_line_screen_keeps_the_limits`
//! leaves out (#334): no 10 × 10 mm window denser than 97 % of PE-DESIGN-compatible mode's
//! densest (that test checks 100 %), and needle points on whole 0.1 mm units.

use openstitch_core::{Command, Stitch};
use openstitch_digitize::photo::{self, PhotoMode, PhotoStitch, PhotoStyle};
use openstitch_digitize::{Frame, Mode, Raster};

/// 60 × 60 px over 30 × 30 mm: the image `open_stitch_line_screen_keeps_the_limits` uses.
fn image() -> Raster {
    Raster::from_fn(60, 60, |x, y| {
        let v = ((x + y) * 2) as u8;
        [v, 255 - v, (x * 4) as u8, 255]
    })
    .expect("size")
}

fn sewn(compat: Mode) -> Vec<Stitch> {
    let frame = Frame::new(0.0, 0.0, 30.0, 60);
    let p = PhotoStitch {
        compat,
        ..PhotoStitch::new(PhotoStyle::Lines, PhotoMode::Color(4))
    };
    let out = photo::convert(&image(), &frame, &p).expect("converts");
    out.objects
        .iter()
        .flat_map(|o| o.stitch_block.as_ref().expect("block").stitches.clone())
        .collect()
}

/// Needle points in the densest 10 × 10 mm window, on a 1 mm grid over the 30 mm image.
fn densest(st: &[Stitch]) -> u32 {
    let mut grid = vec![0u32; 30 * 30];
    for s in st.iter().filter(|s| s.command == Command::Stitch) {
        let (x, y) = ((s.x / 10.0).floor() as usize, (s.y / 10.0).floor() as usize);
        grid[y.min(29) * 30 + x.min(29)] += 1;
    }
    let mut best = 0;
    for y0 in 0..=20 {
        for x0 in 0..=20 {
            let n: u32 = (y0..y0 + 10)
                .map(|y| grid[y * 30 + x0..y * 30 + x0 + 10].iter().sum::<u32>())
                .sum();
            best = best.max(n);
        }
    }
    best
}

#[test]
fn open_stitch_line_screen_stays_within_97_percent_of_pe_modes_densest_window() {
    let (pe, os) = (
        densest(&sewn(Mode::PeDesign)),
        densest(&sewn(Mode::OpenStitch)),
    );
    assert!(pe > 0);
    assert!(
        f64::from(os) <= 0.97 * f64::from(pe),
        "OpenStitch mode {os} against PE-DESIGN-compatible mode {pe} ({:.1} %)",
        100.0 * f64::from(os) / f64::from(pe)
    );
}

#[test]
fn open_stitch_line_screen_puts_needle_points_on_whole_tenths_of_a_millimetre() {
    let os = sewn(Mode::OpenStitch);
    let points: Vec<&Stitch> = os.iter().filter(|s| s.command == Command::Stitch).collect();
    assert!(!points.is_empty());
    for s in points {
        assert!(
            s.x.fract() == 0.0 && s.y.fract() == 0.0,
            "needle point ({}, {}) is between 0.1 mm units",
            s.x,
            s.y
        );
    }
}
