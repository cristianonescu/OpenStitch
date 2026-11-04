#![allow(dead_code)]

use openstitch_design::{Contour, DesignObject};
use openstitch_digitize::Raster;

pub const WHITE: [u8; 4] = [255, 255, 255, 255];
pub const RED: [u8; 4] = [220, 20, 30, 255];
pub const BLUE: [u8; 4] = [20, 40, 200, 255];
pub const YELLOW: [u8; 4] = [250, 200, 20, 255];
pub const BLACK: [u8; 4] = [0, 0, 0, 255];
pub const CLEAR: [u8; 4] = [0, 0, 0, 0];

/// `w`×`h` image of `bg` with filled rectangles `(x0, y0, x1, y1, colour)` (later on top).
pub fn rects(w: u32, h: u32, bg: [u8; 4], rs: &[(u32, u32, u32, u32, [u8; 4])]) -> Raster {
    Raster::from_fn(w, h, |x, y| {
        rs.iter()
            .rev()
            .find(|r| x >= r.0 && x < r.2 && y >= r.1 && y < r.3)
            .map_or(bg, |r| r.4)
    })
    .expect("valid size")
}

pub fn png(r: &Raster) -> Vec<u8> {
    let mut data = Vec::new();
    for y in 0..r.height() {
        for x in 0..r.width() {
            data.extend_from_slice(&r.pixel(x, y));
        }
    }
    let img = image::RgbaImage::from_raw(r.width(), r.height(), data).expect("size");
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .expect("encode");
    out.into_inner()
}

/// Every flattened contour point of every object lies within the box (0.1 mm), with `slack`.
pub fn all_inside(
    objects: &[DesignObject],
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    slack: f64,
) -> bool {
    objects
        .iter()
        .flat_map(|o| o.contours.iter())
        .all(|c: &Contour| {
            c.path().iter().all(|p| {
                p.x >= x0 - slack && p.x <= x1 + slack && p.y >= y0 - slack && p.y <= y1 + slack
            })
        })
}

/// The owner's four shapes (red disc, green triangle, yellow rectangle, blue square on white)
/// in two layouts with every shape moved and recoloured.
pub fn shapes(moved: bool) -> Raster {
    let (r, g, b, y) = (
        [220, 30, 30, 255],
        [30, 160, 60, 255],
        [30, 60, 200, 255],
        [240, 200, 40, 255],
    );
    Raster::from_fn(240, 240, |px, py| {
        let (fx, fy) = (f64::from(px), f64::from(py));
        let disc = |cx: f64, cy: f64, rad: f64| (fx - cx).powi(2) + (fy - cy).powi(2) < rad * rad;
        let tri = |x0: f64, x1: f64, base: f64, apex: f64| {
            let t = (fy - apex) / (base - apex);
            (0.0..=1.0).contains(&t) && (fx - (x0 + x1) / 2.0).abs() <= t * (x1 - x0) / 2.0
        };
        let rect = |x0, y0, x1, y1| px >= x0 && px < x1 && py >= y0 && py < y1;
        let hit = if moved {
            [
                (rect(20, 20, 100, 100), g),
                (rect(130, 40, 210, 90), b),
                (tri(20.0, 120.0, 220.0, 150.0), r),
                (disc(170.0, 170.0, 49.0), y),
            ]
        } else {
            [
                (disc(60.0, 60.0, 49.0), r),
                (tri(130.0, 230.0, 100.0, 30.0), g),
                (rect(20, 150, 100, 200), y),
                (rect(130, 130, 210, 210), b),
            ]
        };
        hit.iter().find(|h| h.0).map_or(WHITE, |h| h.1)
    })
    .expect("size")
}

/// `stitches` without the tie-in and end lock of each run (`stitch_edit::lock_runs`): a run's
/// first needle point is sewn again within the next five stitches of at most 1 mm, and so is
/// its last within the last four. Lets a test check the stitching itself.
pub fn unlocked(stitches: &[openstitch_core::Stitch]) -> Vec<openstitch_core::Stitch> {
    use openstitch_core::Command;
    let same = |a: &openstitch_core::Stitch, b: &openstitch_core::Stitch| {
        (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
    };
    let short = |a: &openstitch_core::Stitch, b: &openstitch_core::Stitch| {
        (a.x - b.x).hypot(a.y - b.y) <= 10.5
    };
    // Runs: from a jump or trim to the next.
    let mut runs: Vec<Vec<openstitch_core::Stitch>> = vec![Vec::new()];
    for s in stitches {
        if matches!(
            s.command,
            Command::Jump | Command::Trim | Command::ColorChange
        ) && runs.last().is_some_and(|r| !r.is_empty())
        {
            runs.push(Vec::new());
        }
        runs.last_mut().expect("one").push(*s);
    }
    let mut out = Vec::new();
    for mut r in runs {
        // Index of the first needle point (the jump's landing or the first stitch) and the
        // first point that moves away from it.
        if let Some(a) = r
            .iter()
            .position(|s| matches!(s.command, Command::Jump | Command::Stitch))
        {
            let mut b = a;
            while b + 1 < r.len() && same(&r[b + 1], &r[a]) {
                b += 1;
            }
            if let Some(k) = (b + 2..(b + 7).min(r.len()))
                .find(|&k| same(&r[k], &r[a]) && (b + 1..=k).all(|i| short(&r[i - 1], &r[i])))
            {
                r.drain(b + 1..=k);
            }
        }
        let n = r.len();
        if n >= 3
            && let Some(k) = (2..5.min(n)).find(|&k| {
                same(&r[n - 1 - k], &r[n - 1]) && (n - k..n).all(|i| short(&r[i - 1], &r[i]))
            })
        {
            r.truncate(n - k);
        }
        out.extend(r);
    }
    out
}
