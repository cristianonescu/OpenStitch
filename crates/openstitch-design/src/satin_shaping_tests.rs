//! OpenStitch's satin shaping on skeleton satin and Auto satin regions (#675): corner
//! constructions, short stitches, compensation per side and per cent, and the under sewing
//! along the column (width-following rows, centre walk, second layer).

use crate::column::{CornerStyle, SatinShaping, SecondUnderlay};
use crate::generate::Run;
use crate::geometry::{Contour, Vec2, v2};
use crate::model::*;
use crate::skeleton::{self, Skeleton};
use openstitch_core::Thread;

const RED: Thread = Thread::rgb(0xED171F);

/// A 4 mm skeleton satin with a 60 degree corner at (300, 0).
fn vee() -> Skeleton {
    Skeleton {
        points: vec![v2(0.0, 0.0), v2(300.0, 0.0), v2(150.0, 259.8)],
        widths: vec![4.0, 4.0, 4.0],
    }
}

fn shaping(f: impl FnOnce(&mut SatinShaping)) -> SatinShaping {
    let mut s = SatinShaping::default();
    f(&mut s);
    s
}

/// Distance from `p` to the polyline `line`.
fn off_line(p: Vec2, line: &[Vec2]) -> f64 {
    line.windows(2)
        .map(|w| skeleton::segment_distance(p, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn skeleton_satin_without_shaping_sews_as_before() {
    let s = vee();
    assert_eq!(
        skeleton::satin_with(&s, 2.2, 1.5, &SatinShaping::default()),
        skeleton::satin(&s, 2.2, 1.5)
    );
}

#[test]
fn skeleton_satin_mitres_and_caps_its_corner() {
    let s = vee();
    let plain = skeleton::satin(&s, 2.2, 0.0);
    for style in [CornerStyle::Mitre, CornerStyle::Cap] {
        let sh = shaping(|x| x.corners.style = style);
        assert!(
            crate::column::has_corners(&skeleton::vertex_rungs(&s), 2.2, &sh.corners),
            "{style:?}: the corner is found on the centre line's points"
        );
        let run = skeleton::satin_with(&s, 2.2, 0.0, &sh);
        assert_ne!(run, plain, "{style:?}");
        assert!(run.len() > 100, "{style:?}: {}", run.len());
        // Every needle point stays on the column (its edges meet in a mitre at the corner),
        // up to the legs' 0.5 mm overlap past the seam.
        let rungs = skeleton::vertex_rungs(&s);
        let mut edge: Vec<Vec2> = rungs.iter().map(|r| r[0]).collect();
        edge.extend(rungs.iter().rev().map(|r| r[1]));
        let column = Contour::new(edge, true);
        let mut ring = column.outline_points();
        ring.push(ring[0]);
        for p in &run {
            assert!(
                crate::geometry::contains(*p, std::slice::from_ref(&column))
                    || off_line(*p, &ring) <= 6.0,
                "{style:?}: {p:?}"
            );
        }
        // The plain satin fans round the corner: at the inner corner its needle points pile
        // up. A mitre or cap sews fewer there.
        let inner = v2(300.0, 0.0) + v2(-0.866, 0.5) * (20.0 / 30f64.to_radians().sin());
        let near = |r: &Run| r.iter().filter(|p| p.distance(inner) < 15.0).count();
        assert!(
            near(&run) < near(&plain),
            "{style:?}: {} vs {}",
            near(&run),
            near(&plain)
        );
    }
}

#[test]
fn skeleton_satin_short_stitches_on_the_inside_of_a_curve() {
    // A 5 mm satin round a quarter circle of radius 6 mm: the inner edge is crowded.
    let points: Vec<Vec2> = (0..=16)
        .map(|k| {
            let a = std::f64::consts::FRAC_PI_2 * k as f64 / 16.0;
            v2(60.0 * a.cos(), 60.0 * a.sin())
        })
        .collect();
    let s = Skeleton {
        widths: vec![5.0; points.len()],
        points,
    };
    // Shaping that changes nothing (a compensation limit on no compensation): the shaped
    // satin without short stitches.
    let plain = skeleton::satin_with(&s, 2.2, 0.0, &shaping(|x| x.compensation.max = Some(3.0)));
    let short = skeleton::satin_with(&s, 2.2, 0.0, &shaping(|x| x.short_stitches.distance = 0.25));
    assert_eq!(plain.len(), short.len());
    // Points pulled in from the inner edge (radius 3.5 mm) towards the middle.
    let inner = |r: &Run| r.iter().filter(|p| (p.length() - 35.0).abs() < 1.0).count();
    assert!(
        inner(&short) + 5 < inner(&plain),
        "{} vs {}",
        inner(&short),
        inner(&plain)
    );
    // The outer edge keeps every point.
    let outer = |r: &Run| r.iter().filter(|p| (p.length() - 85.0).abs() < 1.0).count();
    assert_eq!(outer(&short), outer(&plain));
}

#[test]
fn skeleton_satin_compensates_each_side_and_by_width() {
    let s = Skeleton {
        points: vec![v2(0.0, 0.0), v2(200.0, 0.0)],
        widths: vec![4.0, 4.0],
    };
    // Left of the drawing direction is -y (rungs run [left, right] = [-y, +y]).
    let reach = |r: &Run| {
        let ys = r.iter().filter(|p| p.x > 20.0 && p.x < 180.0).map(|p| p.y);
        ys.fold((0.0f64, 0.0f64), |(lo, hi), y| (lo.min(y), hi.max(y)))
    };
    let (lo, hi) = reach(&skeleton::satin_with(
        &s,
        2.2,
        2.0,
        &shaping(|x| x.compensation.left = Some(0.5)),
    ));
    assert!(
        (lo + 25.0).abs() < 0.01 && (hi - 22.0).abs() < 0.01,
        "{lo} {hi}"
    );
    let (lo, hi) = reach(&skeleton::satin_with(
        &s,
        2.2,
        0.0,
        &shaping(|x| x.compensation.percent = 10.0),
    ));
    assert!(
        (lo + 24.0).abs() < 0.01 && (hi - 24.0).abs() < 0.01,
        "{lo} {hi}"
    );
}

/// A skeleton satin tapering from 2 to 8 mm over 60 mm.
fn taper() -> Skeleton {
    Skeleton {
        points: vec![v2(0.0, 0.0), v2(600.0, 0.0)],
        widths: vec![2.0, 8.0],
    }
}

fn underlay_region(style: UnderlayStyle) -> RegionSewing {
    let mut r = RegionSewing::new(RegionType::Satin, RED);
    r.underlay = true;
    r.underlay_style = style;
    r.underlay_spacing = Some(2.0);
    r
}

#[test]
fn width_following_rows_follow_a_taper_and_stay_inside() {
    let s = taper();
    let [first, second] = skeleton::underlay(&s, &underlay_region(UnderlayStyle::Auto));
    assert!(second.is_none());
    let run = first.expect("rows");
    // The column's half width at x, 0.5 mm in: no needle point past it.
    let half = |x: f64| (10.0 + 30.0 * x / 600.0) - 5.0;
    for p in &run[1..run.len() - 1] {
        assert!(
            p.y.abs() <= half(p.x.clamp(0.0, 600.0)) + 0.01,
            "{p:?} past the inset edge"
        );
    }
    // Rows across at x: the distinct needle points' offsets from the middle there.
    let rows_at = |x: f64| {
        let mut ys: Vec<f64> = run
            .iter()
            .filter(|p| (p.x - x).abs() < 13.0)
            .map(|p| (p.y * 2.0).round() / 2.0)
            .collect();
        ys.sort_by(f64::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 1.0);
        ys.len()
    };
    // Under 3 mm wide (the first 10 mm): one row.
    assert_eq!(rows_at(50.0), 1, "{run:?}");
    // 6 mm wide at x = 40 mm: three rows (one per 2 mm).
    assert_eq!(rows_at(400.0), 3);
    // 7.5 mm at x = 55 mm: still three (one per 2 mm rounded down to an odd count).
    assert_eq!(rows_at(550.0), 3);
    // It starts and ends at the start of the column, where the satin begins.
    assert_eq!(run.first(), run.last());
}

#[test]
fn centre_walk_and_second_layer_on_a_skeleton_satin() {
    let s = taper();
    let [first, second] = skeleton::underlay(&s, &underlay_region(UnderlayStyle::CentreWalk));
    let walk = first.expect("centre walk");
    // From the start (the first crossing's left end) along the middle and back.
    assert!(
        walk[1..walk.len() - 1].iter().all(|p| p.y.abs() < 0.01),
        "along the middle: {walk:?}"
    );
    assert!(second.is_none());
    // A PE-DESIGN style keeps the region's own first layer; the second layer is added.
    let mut r = underlay_region(UnderlayStyle::EdgeZigzag);
    r.underlay2 = Some(SecondUnderlay::default());
    let [first, second] = skeleton::underlay(&s, &r);
    assert!(first.is_none());
    assert!(second.expect("second layer").len() > 10);
    // Through the region: the skeleton satin's layers carry them.
    let outline = skeleton::outline(&s).expect("outline");
    let mut r = underlay_region(UnderlayStyle::Auto);
    r.skeleton = Some(s.clone());
    r.underlay2 = Some(SecondUnderlay::default());
    let [rows, second] = skeleton::underlay(&s, &r).map(|u| u.expect("layer").len());
    let layers = crate::region::layers(std::slice::from_ref(&outline), &r, &[]);
    // The rows and the second layer (joined where one ends at the other's start), then the
    // satin.
    let under: usize = layers[..layers.len() - 1]
        .iter()
        .flatten()
        .map(Vec::len)
        .sum();
    assert!(under + 2 >= rows + second, "{under} < {rows} + {second}");
    assert_eq!(
        layers.last().and_then(|l| l.last()),
        Some(&skeleton::satin(
            &s,
            10.0 / r.density,
            r.pull_compensation * 10.0
        ))
    );
}

/// The 90 degree bent band of compat satin-auto-bend-90 (two 5 mm legs).
fn band() -> Contour {
    Contour::new(
        vec![
            v2(0.0, 0.0),
            v2(300.0, 0.0),
            v2(300.0, 300.0),
            v2(250.0, 300.0),
            v2(250.0, 50.0),
            v2(0.0, 50.0),
        ],
        true,
    )
}

fn auto_satin(f: impl FnOnce(&mut RegionSewing)) -> Vec<Vec<Run>> {
    let mut r = RegionSewing::new(RegionType::Satin, RED);
    r.auto_direction = true;
    r.underlay = false;
    f(&mut r);
    crate::region::layers(std::slice::from_ref(&band()), &r, &[])
}

#[test]
fn auto_satin_takes_the_shaping_options() {
    let plain = auto_satin(|_| {});
    assert_eq!(
        plain,
        auto_satin(|r| r.satin_shaping = SatinShaping::default()),
        "unchanged at the defaults"
    );
    let top = |l: &Vec<Vec<Run>>| l.last().and_then(|t| t.last()).cloned().expect("top");
    let mitred = top(&auto_satin(|r| {
        r.satin_shaping.corners.style = CornerStyle::Mitre
    }));
    assert_ne!(mitred, top(&plain));
    let outline = band();
    for p in &mitred {
        assert!(
            crate::geometry::contains(*p, std::slice::from_ref(&outline))
                || off_line(*p, &outline.outline_points()) <= 6.0,
            "{p:?} off the band"
        );
    }
    // Compensation per side widens the satin past the band's edge on one side only.
    let wide = top(&auto_satin(|r| {
        r.satin_shaping.compensation.left = Some(0.5)
    }));
    let past = |r: &Run| r.iter().filter(|p| p.y < -2.0 || p.x > 302.0).count();
    let (l, total) = (past(&wide), wide.len());
    assert!(l > 0 && l < total, "{l} of {total}");
    // Short stitches change the inner corner, the run stays the same length (against shaping
    // that changes nothing: a limit on no compensation).
    let neutral = top(&auto_satin(|r| {
        r.satin_shaping.compensation.max = Some(3.0)
    }));
    let short = top(&auto_satin(|r| {
        r.satin_shaping.short_stitches.distance = 0.3
    }));
    assert_eq!(short.len(), neutral.len());
    assert_ne!(short, neutral);
}

#[test]
fn auto_satin_sews_the_under_sewing_along_its_column() {
    let layers = auto_satin(|r| {
        r.underlay = true;
        r.underlay_style = UnderlayStyle::Auto;
        r.underlay_spacing = Some(2.0);
    });
    assert_eq!(layers.len(), 2, "the rows, then the satin");
    let rows = &layers[0][0];
    // 5 mm wide: three rows along each leg, all inside the band.
    let outline = band();
    for p in rows {
        assert!(
            crate::geometry::contains(*p, std::slice::from_ref(&outline)),
            "{p:?}"
        );
    }
    let across: Vec<f64> = rows
        .iter()
        .filter(|p| (p.x - 100.0).abs() < 13.0)
        .map(|p| p.y)
        .collect();
    let mut ys = across.clone();
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 2.0);
    assert!(ys.len() >= 3, "{ys:?}");
}
