//! Tests for the stitch types added after running/triple/zigzag/fill/satin: the pattern
//! library and its format, line types, satin/fill options and region types.

use crate::Compiler;
use crate::generate::Run;
use crate::geometry::{Contour, Vec2, v2};
use crate::lines::{self, MotifLine, StrokeLine};
use crate::model::*;
use crate::patterns::{self, PatternKind};
use openstitch_core::{Command, Thread};

const RED: Thread = Thread::rgb(0xED171F);

fn longest(run: &[Vec2]) -> f64 {
    run.windows(2)
        .map(|w| w[0].distance(w[1]))
        .fold(0.0, f64::max)
}

/// Horizontal open line from (0, 0) to (len, 0).
fn hline(len: f64) -> Vec<Vec2> {
    vec![v2(0.0, 0.0), v2(len, 0.0)]
}

fn circle() -> Contour {
    Contour::ellipse(v2(200.0, 200.0), 150.0, 150.0, 64)
}

// ---------------------------------------------------------------- pattern library

#[test]
fn library_parses_with_every_kind_and_unique_ids() {
    let lib = patterns::library();
    for kind in [
        PatternKind::Motif,
        PatternKind::Fill,
        PatternKind::Decorative,
    ] {
        let ids: Vec<&str> = lib
            .iter()
            .filter(|p| p.kind == kind)
            .map(|p| p.id.as_str())
            .collect();
        assert!(ids.len() >= 6, "{kind:?}: {ids:?}");
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
    }
    assert_eq!(
        patterns::find(PatternKind::Motif, "heart").map(|p| p.paths.len()),
        Some(1)
    );
    assert!(patterns::find(PatternKind::Fill, "heart").is_none());
}

#[test]
fn pattern_format_rejects_bad_documents() {
    let ok = r#"{"format":"openstitch.pattern","version":1,"kind":"motif","id":"m","name":"M","paths":[[[0,0],[0.5,1],[1,0]]]}"#;
    assert!(patterns::parse(ok).is_ok());
    for bad in [
        "{}",
        "not json",
        r#"{"format":"other","version":1,"kind":"motif","id":"m","name":"M","paths":[[[0,0],[1,0]]]}"#,
        r#"{"format":"openstitch.pattern","version":2,"kind":"motif","id":"m","name":"M","paths":[[[0,0],[1,0]]]}"#,
        r#"{"format":"openstitch.pattern","version":1,"kind":"motif","id":"m","name":"M","paths":[[[0,0],[0.9,0]]]}"#,
        r#"{"format":"openstitch.pattern","version":1,"kind":"motif","id":"m","name":"M","paths":[[[0,0],[0.5,3],[1,0]]]}"#,
        r#"{"format":"openstitch.pattern","version":1,"kind":"motif","id":"Bad Id","name":"M","paths":[[[0,0],[1,0]]]}"#,
        r#"{"format":"openstitch.pattern","version":1,"kind":"fill","id":"f","name":"F","paths":[]}"#,
        r#"{"format":"openstitch.pattern","version":1,"kind":"fill","id":"f","name":"F","paths":[[[0,0]]]}"#,
        r#"{"format":"openstitch.pattern","version":1,"kind":"decorative","id":"d","name":"D","paths":[[[0,0.2],[1,0.8]]]}"#,
        r#"{"format":"openstitch.pattern","version":1,"kind":"fill","id":"f","name":"F","rowShift":2,"paths":[[[0,0],[1,1]]]}"#,
        r#"{"format":"openstitch.pattern","version":1,"kind":"fill","id":"f","name":"F","extra":1,"paths":[[[0,0],[1,1]]]}"#,
    ] {
        assert!(patterns::parse(bad).is_err(), "{bad}");
    }
    let many = format!(
        r#"{{"format":"openstitch.pattern","version":1,"kind":"fill","id":"f","name":"F","paths":[{}]}}"#,
        vec!["[[0,0],[1,1]]"; 65].join(",")
    );
    assert!(patterns::parse(&many).is_err());
}

// ---------------------------------------------------------------- line types

#[test]
fn motif_groups_free_ends_and_motifs_in_turn() {
    // #544: groups of 3 peaks, 1 mm apart, 8 mm after each group, 6 mm free at both ends;
    // then peak and heart in turn.
    let peak = patterns::find(PatternKind::Motif, "peak").expect("peak");
    let heart = patterns::find(PatternKind::Motif, "heart").expect("heart");
    let m = MotifLine {
        pattern: peak,
        width: 40.0,
        height: 30.0,
        gap: 10.0,
        pitch: 20.0,
        arrange: Arrange::Normal,
    };
    let groups = [(3, 80.0)];
    let plan = lines::MotifPlan {
        groups: &groups,
        free_start: 60.0,
        free_end: 60.0,
        patterns: vec![peak],
    };
    let run = lines::motif_planned(&hline(800.0), &m, &plan);
    assert!(longest(&run) <= 20.0 + 1e-9);
    assert_eq!(run.first().copied(), Some(v2(0.0, 0.0)));
    assert_eq!(run.last().copied(), Some(v2(800.0, 0.0)));
    let tips: Vec<f64> = run
        .iter()
        .filter(|p| (p.y.abs() - 27.0).abs() < 1e-6)
        .map(|p| p.x)
        .collect();
    // Free ends: no motif within 6 mm of either end.
    assert!(
        tips.iter().all(|&x| (60.0..=740.0).contains(&x)),
        "{tips:?}"
    );
    // Groups of three: tips 5 mm apart inside a group, 12 mm (4 + 8) between groups.
    let steps: Vec<i64> = tips
        .windows(2)
        .map(|w| (w[1] - w[0]).round() as i64)
        .collect();
    assert!(steps.len() >= 5, "{steps:?}");
    for (i, d) in steps.iter().enumerate() {
        let want = if i % 3 == 2 { 120 } else { 50 };
        assert_eq!(*d, want, "{steps:?}");
    }
    // Motifs in turn: peak, heart, peak, heart.
    let turn = lines::MotifPlan {
        groups: &[],
        free_start: 0.0,
        free_end: 0.0,
        patterns: vec![peak, heart],
    };
    let both = lines::motif_planned(&hline(500.0), &m, &turn);
    let peaks = both
        .iter()
        .filter(|p| (p.y.abs() - 27.0).abs() < 1e-6)
        .count();
    assert!((4..=6).contains(&peaks), "{peaks}");
    assert!(both.iter().any(|p| p.y < -29.0), "a heart's top");
    // Nothing beyond PE-DESIGN asked for: PE-DESIGN's motif line, unchanged.
    let plain = lines::MotifPlan {
        groups: &[],
        free_start: 0.0,
        free_end: 0.0,
        patterns: vec![peak],
    };
    assert_eq!(
        lines::motif_planned(&hline(500.0), &m, &plain),
        lines::motif(&hline(500.0), false, &m)
    );
}

/// #544: motifs in turn come in order (peak, heart, peak, heart…, each copy where the plan puts
/// it), and with mirror-alternate every other copy is on the other side of the line.
#[test]
fn motifs_in_turn_alternate_in_order_and_mirror_alternate_flips_every_other() {
    let peak = patterns::find(PatternKind::Motif, "peak").expect("peak");
    let heart = patterns::find(PatternKind::Motif, "heart").expect("heart");
    let m = MotifLine {
        pattern: peak,
        width: 40.0,
        height: 30.0,
        gap: 10.0,
        pitch: 20.0,
        arrange: Arrange::Normal,
    };
    let plan = lines::MotifPlan {
        groups: &[],
        free_start: 0.0,
        free_end: 0.0,
        patterns: vec![peak, heart],
    };
    // Each copy k sits on x in [50k, 50k + 40]: which motif it is (a heart reaches past
    // 2.9 mm, a peak tops at 2.7 mm) and on which side its farthest point lies.
    let copies = |run: &[Vec2]| -> Vec<(&'static str, f64)> {
        (0..10)
            .map(|k| {
                let (a, b) = (50.0 * k as f64 + 1.0, 50.0 * k as f64 + 39.0);
                let far = run
                    .iter()
                    .filter(|p| p.x > a && p.x < b)
                    .max_by(|p, q| p.y.abs().total_cmp(&q.y.abs()))
                    .expect("a copy");
                let kind = if far.y.abs() > 29.0 { "heart" } else { "peak" };
                (kind, far.y.signum())
            })
            .collect()
    };
    let turn = copies(&lines::motif_planned(&hline(500.0), &m, &plan));
    let kinds: Vec<&str> = turn.iter().map(|c| c.0).collect();
    assert_eq!(
        kinds,
        [
            "peak", "heart", "peak", "heart", "peak", "heart", "peak", "heart", "peak", "heart"
        ]
    );
    assert!(
        turn.iter().all(|c| c.1 == turn[0].1),
        "all on one side: {turn:?}"
    );
    // Mirror-alternate: the same order, every other copy on the other side.
    let alternate = MotifLine {
        arrange: Arrange::Alternate,
        ..m
    };
    let flipped = copies(&lines::motif_planned(&hline(500.0), &alternate, &plan));
    assert_eq!(flipped.iter().map(|c| c.0).collect::<Vec<_>>(), kinds);
    for (k, c) in flipped.iter().enumerate() {
        let want = if k % 2 == 0 {
            flipped[0].1
        } else {
            -flipped[0].1
        };
        assert_eq!(c.1, want, "copy {k}: {flipped:?}");
    }
}

#[test]
fn motif_line_repeats_inside_its_band_with_short_stitches() {
    let pattern = patterns::find(PatternKind::Motif, "peak").expect("peak");
    let m = MotifLine {
        pattern,
        width: 40.0,
        height: 30.0,
        gap: 10.0,
        pitch: 20.0,
        arrange: Arrange::Normal,
    };
    let run = lines::motif(&hline(500.0), false, &m);
    assert!(longest(&run) <= 20.0 + 1e-9);
    assert!(run.iter().all(|p| p.y.abs() <= 30.0 + 1e-9));
    assert_eq!(run.first().copied(), Some(v2(0.0, 0.0)));
    assert_eq!(run.last().copied(), Some(v2(500.0, 0.0)));
    // (500 + 10) / 50 = 10 peaks, all on one side.
    let peaks = run
        .iter()
        .filter(|p| (p.y.abs() - 27.0).abs() < 1e-6)
        .count();
    assert_eq!(peaks, 10);
    assert!(run.iter().all(|p| p.y <= 1e-9));
    let mirrored = lines::motif(
        &hline(500.0),
        false,
        &MotifLine {
            arrange: Arrange::Mirror,
            ..m
        },
    );
    assert!(mirrored.iter().all(|p| p.y >= -1e-9));
    let alternate = lines::motif(
        &hline(500.0),
        false,
        &MotifLine {
            arrange: Arrange::Alternate,
            ..m
        },
    );
    assert!(alternate.iter().any(|p| p.y > 20.0) && alternate.iter().any(|p| p.y < -20.0));
}

#[test]
fn motif_line_closes_on_a_closed_contour() {
    let c = circle();
    for id in ["wave", "scallop", "heart", "leaf", "loop", "diamond"] {
        let pattern = patterns::find(PatternKind::Motif, id).expect("motif");
        let run = lines::motif(
            &c.path(),
            true,
            &MotifLine {
                pattern,
                width: 40.0,
                height: 30.0,
                gap: 0.0,
                pitch: 15.0,
                arrange: Arrange::Normal,
            },
        );
        assert!(longest(&run) <= 15.0 + 1e-9, "{id}");
        let (a, b) = (run[0], *run.last().expect("points"));
        assert!(a.distance(b) < 1e-6, "{id}: {a:?} {b:?}");
        // Every point within the motif height of the circle.
        assert!(
            run.iter()
                .all(|p| (p.distance(v2(200.0, 200.0)) - 150.0).abs() <= 31.0),
            "{id}"
        );
    }
}

#[test]
fn stem_stitches_slant_across_the_line() {
    // PE-DESIGN's stem: 21 stations (300 / 15 = 20 intervals), each sewn top, bottom, top,
    // bottom, between the line's start and end points.
    let run = lines::stem(&hline(300.0), &[], 20.0, 15.0, 45.0, false);
    assert_eq!(run.len(), 21 * 4 + 2);
    assert_eq!(run.first().copied(), Some(v2(0.0, 0.0)));
    assert_eq!(run.last().copied(), Some(v2(300.0, 0.0)));
    assert!(run.iter().all(|p| p.y.abs() <= 11.0 + 1e-9));
    let slant = 20.0 / 45f64.to_radians().sin();
    assert!((run[1].distance(run[2]) - slant).abs() < 1e-9);
    // 45°: from the top right to the bottom left (y down).
    assert!(run[1].x > run[2].x && run[1].y < run[2].y);
    let single = lines::stem(&hline(300.0), &[], 20.0, 15.0, 90.0, true);
    assert_eq!(single.len(), 21 * 2 + 2);
    assert!(
        (single[1].x - single[2].x).abs() < 1e-9,
        "90° is perpendicular"
    );
}

#[test]
fn candlewick_knots_sit_on_the_line() {
    // Knots at 0, 50, 100, 150, 200 (200 / (20 + 30) = 4 intervals), each a {24/11} star
    // visiting all 24 points of its circle.
    let run = lines::candlewick(&hline(200.0), &[], 20.0, 24, 30.0, 25.0);
    assert!(longest(&run) <= 25.0 + 1e-9);
    assert!(run.iter().all(|p| p.y.abs() <= 10.0 + 1e-9));
    for x in [0.0, 50.0, 100.0, 150.0, 200.0] {
        let mut on: Vec<Vec2> = run
            .iter()
            .copied()
            .filter(|p| (v2(x, 0.0).distance(*p) - 10.0).abs() < 1e-9)
            .collect();
        on.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        on.dedup_by(|a, b| a.distance(*b) < 1e-6);
        assert_eq!(on.len(), 24, "knot at {x}");
    }
}

#[test]
fn e_stitch_prongs_slant_by_their_angle() {
    // #543: angle 0 sews today's square prongs; 45 degrees turns each prong 45 ± 1 degrees
    // forward, at the same length.
    let e = StrokeLine {
        interval: 50.0,
        stroke: 30.0,
        pitch: 25.0,
        run_times: 1,
        stroke_times: 1,
        arrange: Arrange::Normal,
        angle: 0.0,
    };
    let square = lines::e_stitch(&hline(300.0), &e);
    let slanted = lines::e_stitch(&hline(300.0), &StrokeLine { angle: 45.0, ..e });
    let prongs = |run: &[Vec2]| -> Vec<(Vec2, Vec2)> {
        run.windows(2)
            .filter(|w| w[0].y.abs() < 1e-9 && w[1].y.abs() > 1.0)
            .map(|w| (w[0], w[1]))
            .collect()
    };
    let sq = prongs(&square);
    assert!(!sq.is_empty());
    assert!(sq.iter().all(|(a, b)| (b.x - a.x).abs() < 1e-9));
    let sl = prongs(&slanted);
    assert_eq!(sl.len(), sq.len());
    for (a, b) in sl {
        let deg = (b.x - a.x).atan2(b.y.abs()).to_degrees();
        assert!((deg - 45.0).abs() <= 1.0, "{deg}");
        assert!((a.distance(b) - 30.0).abs() < 1e-6);
    }
}

#[test]
fn e_and_v_strokes_reach_the_stroke_width() {
    let e = StrokeLine {
        interval: 30.0,
        stroke: 25.0,
        pitch: 25.0,
        run_times: 1,
        stroke_times: 2,
        arrange: Arrange::Normal,
        angle: 0.0,
    };
    let run = lines::e_stitch(&hline(300.0), &e);
    assert!(longest(&run) <= 25.0 + 1e-9);
    let tips = run
        .iter()
        .filter(|p| (p.y.abs() - 25.0).abs() < 1e-9)
        .count();
    assert_eq!(tips, 11 * 2, "11 strokes, sewn twice");
    assert_eq!(run.last().copied(), Some(v2(300.0, 0.0)));
    let triple = lines::e_stitch(&hline(300.0), &StrokeLine { run_times: 2, ..e });
    assert_eq!(
        triple.last().copied(),
        Some(v2(300.0, 0.0)),
        "even passes end forward"
    );
    // PE-DESIGN's V: one apex at every multiple of the interval, start and end included, its
    // arms meeting the line a fifth of the interval to either side.
    let v = lines::v_stitch(&hline(300.0), &e);
    let apexes: Vec<f64> = v
        .iter()
        .filter(|p| (p.y.abs() - 25.0).abs() < 1e-9)
        .map(|p| p.x)
        .collect();
    assert_eq!(
        apexes,
        (0..=10).map(|k| k as f64 * 30.0).collect::<Vec<_>>()
    );
    assert!(longest(&v) <= 25.0f64.hypot(6.0) + 1e-9);
    assert_eq!(v.first().copied(), Some(v2(0.0, 0.0)));
    assert_eq!(v.last().copied(), Some(v2(300.0, 0.0)));
}

#[test]
fn chain_links_stay_within_their_size() {
    // Triangles: floor(200 / 15) = 13 links, each with its base 20 ahead and 20/3 to either
    // side, sewn twice and then to its left corner: 5 corners per link.
    let run = lines::chain(
        &hline(200.0),
        &[],
        20.0,
        ChainShape::Triangle,
        2,
        Arrange::Normal,
    );
    let h = 20.0 / 3.0;
    assert!(run.iter().all(|p| p.y.abs() <= h + 1e-9));
    assert!(longest(&run) <= 20.0f64.hypot(h) + 1e-9);
    let corners = run.iter().filter(|p| (p.y.abs() - h).abs() < 1e-9).count();
    assert_eq!(corners, 13 * 5);
    let run = lines::chain(
        &hline(200.0),
        &[],
        20.0,
        ChainShape::Diamond,
        2,
        Arrange::Normal,
    );
    assert!(run.iter().all(|p| p.y.abs() <= 10.0 + 1e-9));
    assert!(longest(&run) <= 10.0f64.hypot(10.0) + 1e-9);
    let peaks = run
        .iter()
        .filter(|p| (p.y.abs() - 10.0).abs() < 1e-9)
        .count();
    assert_eq!(peaks, 20 * 2);
}

/// The rectangle of compat line-*-startend-*: (10,10)-(50,10)-(50,30)-(10,30) mm, closed; its
/// bottom edge is sewn towards -x.
fn startend_rect() -> Vec<Vec2> {
    Contour::rectangle(100.0, 100.0, 400.0, 200.0).path()
}

#[test]
fn moved_points_keep_chain_links_in_place_and_start_where_pe_does() {
    let path = startend_rect();
    let at = |x: f64| lines::arc_of(&path, v2(x, 300.0));
    let link = |j: f64| v2(500.0 - j * 400.0 / 13.0, 300.0);
    let line = lines::LoopLine::Chain {
        size: 40.0,
        times: 5,
        arrange: Arrange::Normal,
    };
    let in_place = lines::chain(&path, &[], 40.0, ChainShape::Triangle, 5, Arrange::Normal);
    let same_points = |run: &Run| {
        run.iter()
            .all(|p| in_place.iter().any(|q| q.distance(*p) < 1e-6))
    };
    // Feed: the pattern starts with the link of the exit point (L2 at 25.5 mm: link 7) and
    // goes round to the one before.
    let (feed, lead) = lines::start_loop(&path, &[], line, at(255.0), None).expect("feed");
    assert!(!lead && same_points(&feed));
    assert!(feed[0].distance(link(7.0)) < 1e-6);
    assert_eq!(feed.len(), in_place.len());
    // Run: from the entry point's link (L1 at 29.8 mm: link 6) one stitch on to link 7.
    let (run, lead) = lines::start_loop(&path, &[], line, at(255.0), Some(at(298.0))).expect("run");
    assert!(lead && same_points(&run));
    assert!(run[0].distance(link(6.0)) < 1e-6);
    assert_eq!(&run[1..], &feed[..]);
    // The shorter way may be backwards: L2 at the corner (50, 30), L1 at 23.8 mm (link 8)
    // leads back over links 7 to 0 (line-chain-startend-l2side-run.pes).
    let (run, _) = lines::start_loop(&path, &[], line, at(500.0), Some(at(238.0))).expect("back");
    for (k, j) in (0..=8).rev().enumerate() {
        assert!(run[k].distance(link(f64::from(j))) < 1e-6);
    }
}

#[test]
fn moved_points_start_stem_and_candlewick_at_the_exit_element_and_lead_in_with_run() {
    let path = startend_rect();
    let (exit, entry) = (
        lines::arc_of(&path, v2(255.0, 300.0)),
        Some(lines::arc_of(&path, v2(298.0, 300.0))),
    );
    let stem = lines::LoopLine::Stem {
        width: 20.0,
        spacing: 15.0,
        angle: 45.0,
        single: false,
    };
    // Stations 400/27 apart on the bottom edge: L2's is the 16th (263.0), L1's the 13th (307.4).
    let station = |j: f64| v2(500.0 - j * 400.0 / 27.0, 300.0);
    let (feed, _) = lines::start_loop(&path, &[], stem, exit, None).expect("feed");
    assert!(feed[0].distance(station(16.0)) < 1e-6);
    let (run, lead) = lines::start_loop(&path, &[], stem, exit, entry).expect("run");
    assert!(lead);
    for (k, j) in [13.0, 14.0, 15.0, 16.0].into_iter().enumerate() {
        assert!(run[k].distance(station(j)) < 1e-6);
    }
    assert_eq!(&run[4..], &feed[1..]);
    // Candlewicking starts with the running stitch into L2's knot (centred 4 × 400/7 along
    // the bottom edge, from where that knot begins) and sews that knot last.
    let knots = lines::LoopLine::Candlewick {
        size: 50.0,
        points: 24,
        spacing: 10.0,
        pitch: 25.0,
    };
    let (feed, lead) = lines::start_loop(&path, &[], knots, exit, None).expect("candlewick");
    assert!(lead);
    assert!(feed[0].distance(v2(500.0 - 4.0 * 400.0 / 7.0 + 25.0, 300.0)) < 1e-6);
    let front = v2(500.0 - 4.0 * 400.0 / 7.0 - 25.0, 300.0);
    assert_eq!(
        feed[1..].iter().position(|p| p.distance(front) < 1e-6),
        Some(1)
    );
    assert!(feed.last().expect("stitches").distance(front) < 1e-6);
}

#[test]
fn cutting_lines_go_to_the_needle_matching_their_direction() {
    let square = Contour::rectangle(0.0, 0.0, 100.0, 100.0);
    let needles = lines::cutting(&square.path(), true, false);
    let total = |n: &Vec<Run>| -> f64 {
        n.iter()
            .map(|r| r.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>())
            .sum()
    };
    assert!((total(&needles[0]) - 200.0).abs() < 1e-9);
    assert!((total(&needles[2]) - 200.0).abs() < 1e-9);
    assert!(needles[1].is_empty() && needles[3].is_empty());
    assert!(
        needles
            .iter()
            .flatten()
            .all(|r| longest(r) <= lines::CUT_PITCH + 1e-9)
    );
    let diagonal = lines::cutting(&[v2(0.0, 0.0), v2(100.0, 100.0)], false, false);
    assert!(!diagonal[1].is_empty() && diagonal[0].is_empty());
    let bridged = lines::cutting(&square.path(), true, true);
    let cut: f64 = bridged.iter().map(total).sum();
    assert!(cut < 400.0 - 3.0 * lines::CUT_BRIDGE && cut > 400.0 - 5.0 * lines::CUT_BRIDGE);
}

#[test]
fn auto_satin_ends_on_its_start_point_and_travels_forward_along_the_outline() {
    // The 5 mm band bent through 60° of compat satin-auto-bend-60 (0.1 mm): PE-DESIGN 11.50
    // starts at the first vertex, runs 0.5 mm inside the outer (top) edge to the far end, sews
    // the satin back with one needle point per rung and ends on the start point.
    let band = Contour::new(
        vec![
            v2(0.0, 0.0),
            v2(300.0, 0.0),
            v2(386.6, 150.0),
            v2(343.3, 175.0),
            v2(271.1, 50.0),
            v2(0.0, 50.0),
        ],
        true,
    );
    let mut r = region(RegionType::Satin);
    r.auto_direction = true;
    r.underlay = false;
    r.density = 3.0;
    let run = crate::region::layers(std::slice::from_ref(&band), &r, &[])
        .pop()
        .expect("top")
        .pop()
        .expect("run");
    let last = run[run.len() - 1];
    assert!(last.distance(v2(0.0, 0.0)) < 2.0, "ends at {last:?}");
    // The lead travel (before the first rung, which is at the far end) stays by the top edge.
    let first_rung = run
        .iter()
        .position(|p| p.x > 330.0 && p.y > 100.0)
        .expect("far end reached");
    let lead = &run[..first_rung];
    assert!(lead.len() > 10, "{lead:?}");
    for p in lead.iter().filter(|p| p.x > 50.0 && p.x < 250.0) {
        assert!(
            p.y > 0.0 && p.y < 10.0,
            "lead by the top edge, not {p:?}: {lead:?}"
        );
    }
    // Along the straight leg the needle points alternate between the edges at exactly a
    // spacing (3.33 mm / 10 → 0.333 mm): the rungs are square to the leg and on one lattice.
    let leg: Vec<Vec2> = run[first_rung..]
        .iter()
        .copied()
        .filter(|p| p.x > 10.0 && p.x < 200.0 && (p.y.abs() < 0.5 || (p.y - 50.0).abs() < 0.5))
        .collect();
    assert!(leg.len() > 50, "{}", leg.len());
    for w in leg.windows(2) {
        assert!((w[0].y - w[1].y).abs() > 40.0, "alternate edges: {w:?}");
        assert!(
            ((w[0].x - w[1].x).abs() - 10.0 / 3.0).abs() < 0.05,
            "a spacing apart: {w:?}"
        );
    }
}

#[test]
fn half_stitch_thins_the_inner_side_of_bends() {
    let straight = Contour::new(hline(200.0), false);
    assert_eq!(
        lines::zigzag_half(&straight.path(), 20.0, 4.0),
        crate::generate::zigzag(&straight, 20.0, 4.0),
        "nothing crowds on a straight line"
    );
    let tight = Contour::ellipse(v2(0.0, 0.0), 15.0, 15.0, 48);
    let plain = crate::generate::zigzag(&tight, 20.0, 3.0);
    let half = lines::zigzag_half(&tight.path(), 20.0, 3.0);
    assert_eq!(plain.len(), half.len());
    let near_centre = |r: &Run| r.iter().filter(|p| (p.length() - 15.0).abs() < 0.1).count();
    assert!(near_centre(&half) >= near_centre(&plain) + 5);
}

fn shape_with_line(c: Contour, line: LineSewing) -> Design {
    let mut d = Design::new("t", DesignPage::default());
    d.add(DesignObject::shape(vec![c], None, Some(line)));
    d
}

#[test]
fn every_line_type_compiles_deterministically() {
    for kind in LineType::ALL {
        let d = shape_with_line(circle(), LineSewing::new(kind, RED));
        let a = Compiler::default().compile(&d);
        let b = Compiler::default().compile(&d);
        assert_eq!(a, b, "{kind:?}");
        assert!(a.count(Command::Stitch) > 20, "{kind:?}");
    }
}

#[test]
fn cutting_needles_are_separate_colour_blocks() {
    let d = shape_with_line(
        Contour::new(
            vec![
                v2(0.0, 0.0),
                v2(100.0, 0.0),
                v2(200.0, 100.0),
                v2(200.0, 200.0),
            ],
            false,
        ),
        LineSewing::new(LineType::Cutting, RED),
    );
    let p = Compiler::default().compile(&d);
    assert_eq!(p.threads.len(), 3, "needles 1, 2 and 3");
    assert_eq!(p.count(Command::ColorChange), 2);
    assert!(
        p.threads[0]
            .name
            .as_deref()
            .is_some_and(|n| n.starts_with("Cutwork needle 1"))
    );
}

// ---------------------------------------------------------------- satin and fill options

/// Inside the region (even-odd) or within `tol` of its outline.
fn inside_or_on(p: Vec2, contours: &[Contour], tol: f64) -> bool {
    if crate::geometry::contains(p, contours) {
        return true;
    }
    contours.iter().flat_map(Contour::edges).any(|(a, b)| {
        let d = b - a;
        let l2 = d.x * d.x + d.y * d.y;
        let t = if l2 > 0.0 {
            (((p - a).x * d.x + (p - a).y * d.y) / l2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        p.distance(a + d * t) <= tol
    })
}

/// Every needle point and every stitch midpoint inside the region.
fn assert_inside(runs: &[Run], contours: &[Contour], what: &str) {
    for r in runs {
        for w in r.windows(2) {
            for p in [w[0], w[1], w[0].lerp(w[1], 0.5)] {
                assert!(inside_or_on(p, contours, 0.6), "{what}: {p:?} outside");
            }
        }
    }
}

fn region(kind: RegionType) -> RegionSewing {
    RegionSewing::new(kind, RED)
}

#[test]
fn auto_satin_on_a_sharp_bend_spaces_its_needle_points_along_the_outer_edge() {
    // The 120 degree band of compat satin-auto-bend-120 (PE-DESIGN 11.50: 161 stitches, the
    // longest crossing 9.6 mm, needle points 1/3 mm apart along the 47.3 mm outer edge): its
    // rungs are rebuilt on the outer edge, square on the legs and fanned round the bend.
    let band = Contour::new(
        vec![
            v2(0.0, 0.0),
            v2(300.0, 0.0),
            v2(213.4, 150.0),
            v2(170.1, 125.0),
            v2(213.4, 50.0),
            v2(0.0, 50.0),
        ],
        true,
    );
    let mut r = region(RegionType::Satin);
    r.auto_direction = true;
    r.density = 3.0;
    r.underlay = false;
    let layers = crate::region::layers(std::slice::from_ref(&band), &r, &[]);
    let run = layers.concat().concat();
    let crossings: Vec<f64> = run
        .windows(2)
        .map(|w| w[0].distance(w[1]))
        .filter(|&d| d >= 40.0)
        .collect();
    // About 3 per millimetre of outer edge (141 for PE-DESIGN), none longer than two widths.
    assert!(
        (130..=150).contains(&crossings.len()),
        "{}",
        crossings.len()
    );
    assert!(crossings.iter().all(|&d| d <= 100.0 + 1.0), "{crossings:?}");
    // Every needle point inside the band.
    for p in &run {
        assert!(
            crate::geometry::contains(*p, std::slice::from_ref(&band))
                || band
                    .edges()
                    .iter()
                    .any(|&(a, b)| crate::skeleton::segment_distance(*p, a, b) < 0.6),
            "{p:?} outside"
        );
    }
    // The 90 degree band keeps the skeleton's lattice (the outer-edge rebuild starts past 100
    // degrees).
    let right = Contour::new(
        vec![
            v2(0.0, 0.0),
            v2(300.0, 0.0),
            v2(300.0, 300.0),
            v2(250.0, 300.0),
            v2(250.0, 50.0),
            v2(0.0, 50.0),
        ],
        true,
    );
    let turn = |c: &Contour| crate::region::sharpest_reflex_turn(c).to_degrees();
    assert!((turn(&band) - 120.0).abs() < 0.1, "{}", turn(&band));
    assert!((turn(&right) - 90.0).abs() < 0.1, "{}", turn(&right));
    assert_eq!(turn(&Contour::rectangle(0.0, 0.0, 100.0, 50.0)), 0.0);
}

#[test]
fn auto_satin_half_stitch_pulls_back_every_other_crowded_inner_point() {
    // PE-DESIGN 11.50's Half Stitch (#372; satin-auto-bend-120 against -nohalf): the same
    // stitches, every other needle point crowded on the bend's inner side pulled back along
    // its crossing (8 of PE-DESIGN's, 0.8 to 2.4 mm); a 45 degree bend is left alone.
    let bend =
        |pts: &[(f64, f64)]| Contour::new(pts.iter().map(|&(x, y)| v2(x, y)).collect(), true);
    let sew = |c: &Contour, half: bool| {
        let mut r = region(RegionType::Satin);
        r.auto_direction = true;
        r.density = 3.0;
        r.underlay = false;
        r.half_stitch = half;
        crate::region::layers(std::slice::from_ref(c), &r, &[])
            .concat()
            .concat()
    };
    let sharp = bend(&[
        (0.0, 0.0),
        (300.0, 0.0),
        (213.4, 150.0),
        (170.1, 125.0),
        (213.4, 50.0),
        (0.0, 50.0),
    ]);
    let (on, off) = (sew(&sharp, true), sew(&sharp, false));
    assert_eq!(on.len(), off.len(), "Half Stitch moves needle points only");
    let moved: Vec<(usize, f64)> = on
        .iter()
        .zip(&off)
        .enumerate()
        .map(|(i, (a, b))| (i, a.distance(*b)))
        .filter(|&(_, d)| d > 0.5)
        .collect();
    assert!((6..=14).contains(&moved.len()), "{moved:?}");
    // Every other needle point on one side: four needle points apart in the run.
    for w in moved.windows(2) {
        assert_eq!(w[1].0 - w[0].0, 4, "{moved:?}");
    }
    // Pulled back up to a quarter of a crossing: under 2.6 mm here.
    assert!(moved.iter().all(|&(_, d)| d <= 26.0), "{moved:?}");
    let thread = |run: &Run| run.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>();
    assert!(thread(&on) < thread(&off));
    let gentle = bend(&[
        (0.0, 0.0),
        (300.0, 0.0),
        (422.5, 122.5),
        (387.1, 157.8),
        (279.3, 50.0),
        (0.0, 50.0),
    ]);
    assert_eq!(sew(&gentle, true), sew(&gentle, false));
}

/// The storybook regions of #304 (Auto Punch, PE-DESIGN-compatible mode), sewn as Auto satin
/// without under sewing: every needle point in order.
fn storybook_satin(name: &str) -> Vec<Vec2> {
    let doc: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/storybook-satins.json"))
            .expect("fixture");
    let contours: Vec<Contour> = doc[name]
        .as_array()
        .expect("region")
        .iter()
        .map(|c| {
            let pts = c["points"].as_array().expect("points");
            Contour::new(
                pts.iter()
                    .map(|p| v2(p[0].as_f64().expect("x"), p[1].as_f64().expect("y")))
                    .collect(),
                true,
            )
        })
        .collect();
    let mut r = region(RegionType::Satin);
    r.auto_direction = true;
    r.underlay = false;
    crate::region::layers(&contours, &r, &[]).concat().concat()
}

/// The most needle points in one 0.5 mm spot (`tools/sew_check.py`'s cell).
fn most_in_a_spot(run: &[Vec2]) -> usize {
    let mut cells = std::collections::HashMap::new();
    for p in run {
        *cells
            .entry(((p.x / 5.0).floor() as i64, (p.y / 5.0).floor() as i64))
            .or_insert(0usize) += 1;
    }
    cells.values().copied().max().unwrap_or(0)
}

#[test]
fn satin_travels_round_holes_spread_their_needle_points() {
    // #304: the storybook's khaki satin (four holes, sewn in sections) travelled 20 times
    // through one corner of a hole and dropped the needle on the same point each time.
    let run = storybook_satin("khaki-holes");
    assert!(run.len() > 1000, "{}", run.len());
    // Before: 20 in one spot, all on one point; sew_check allows 20.
    let most = most_in_a_spot(&run);
    assert!(most <= 17, "{most} needle points in one 0.5 mm spot");
    let mut same = std::collections::HashMap::new();
    for p in &run {
        *same
            .entry(((p.x * 2.0).round() as i64, (p.y * 2.0).round() as i64))
            .or_insert(0usize) += 1;
    }
    let repeats = same.values().copied().max().unwrap_or(0);
    assert!(repeats <= 8, "{repeats} needle points on one point");
}

#[test]
fn auto_satin_on_a_doubled_back_skeleton_keeps_one_direction() {
    // #304: a 6 mm jagged Auto Punch blob whose derived skeleton doubles back; its crossings
    // crossed each other, 18 needle points in one spot and 171 stitches. It now sews the
    // fixed-direction satin across its principal axis.
    let run = storybook_satin("jagged-blob");
    let most = most_in_a_spot(&run);
    assert!(most <= 6, "{most} needle points in one 0.5 mm spot");
    assert!(run.len() < 120, "{} needle points", run.len());
}

#[test]
fn auto_satin_start_travel_skips_needle_points_under_half_a_millimetre() {
    // #304: the travel round a 1.7 mm region's inset outline dropped the needle on every
    // vertex: seven stitches under 0.3 mm in a row.
    let run = storybook_satin("small-pentagon");
    let mut longest = 0;
    let mut tiny = 0;
    for w in run.windows(2) {
        tiny = if w[0].distance(w[1]) < 3.0 {
            tiny + 1
        } else {
            0
        };
        longest = longest.max(tiny);
    }
    assert!(longest <= 3, "{longest} stitches under 0.3 mm in a row");
}

#[test]
fn half_stitch_is_on_by_default_and_saved_only_when_off() {
    let mut r = region(RegionType::Satin);
    assert!(r.half_stitch, "PE-DESIGN's drawing tools tick Half Stitch");
    let on = serde_json::to_value(&r).expect("json");
    assert!(on.get("halfStitch").is_none(), "{on}");
    r.half_stitch = false;
    let off = serde_json::to_value(&r).expect("json");
    assert_eq!(off["halfStitch"], serde_json::json!(false));
    let back: RegionSewing = serde_json::from_value(off).expect("read");
    assert!(!back.half_stitch);
    let old: RegionSewing = serde_json::from_value(on).expect("read");
    assert!(old.half_stitch, "files without the field keep the default");
}

#[test]
fn auto_direction_sews_across_the_narrow_side() {
    let wide = Contour::rectangle(0.0, 0.0, 200.0, 50.0);
    assert_eq!(crate::region::auto_angle(std::slice::from_ref(&wide)), 90.0);
    let tall = Contour::rectangle(0.0, 0.0, 50.0, 200.0);
    assert_eq!(crate::region::auto_angle(std::slice::from_ref(&tall)), 0.0);
    let tilted = wide.transformed(&crate::geometry::Transform::rotation(30.0, v2(0.0, 0.0)));
    assert!((crate::region::auto_angle(std::slice::from_ref(&tilted)) - -60.0).abs() < 0.01);
    let mut r = region(RegionType::Satin);
    r.auto_direction = true;
    let layers = crate::region::layers(std::slice::from_ref(&wide), &r, &[]);
    let runs = layers.last().expect("top");
    // A zigzag stitch crosses the narrow side, slanting one row (1/3 mm) along it.
    let slant = 50.0f64.hypot(10.0 / 3.0);
    for run in runs {
        for w in run.windows(2) {
            let d = w[0].distance(w[1]);
            assert!(d <= slant + 1e-6, "stitch {d} longer than the narrow side");
        }
    }
    assert_inside(runs, std::slice::from_ref(&wide), "auto satin");
    // Holes keep the single direction across the narrow side: a sawtooth whose slanted
    // stitches move one row (two crossings) along it. Travel between the parts (running
    // stitches of at most 2.5 mm) is left out.
    let hole = Contour::rectangle(80.0, 20.0, 40.0, 10.0);
    let holed = crate::region::layers(std::slice::from_ref(&wide), &r, &[hole]);
    let row = 2.0 * 10.0 / r.density;
    for run in holed.last().expect("top") {
        for w in run.windows(2).filter(|w| w[0].distance(w[1]) > 25.0 + 1e-6) {
            let d = w[0] - w[1];
            assert!(d.x.abs() <= row + 1e-6 || d.y.abs() < 1e-6, "{w:?}");
        }
    }
}

#[test]
fn cross_under_sewing_sews_two_layers_of_lines_in_halves_from_the_far_corners() {
    // The satin of compat sweep-satin-us-type-2: 40 x 6 mm at 90 degrees, start point at the
    // middle of the bottom edge, PE-DESIGN's second under sewing type.
    let c = vec![Contour::rectangle(100.0, 100.0, 400.0, 60.0)];
    let mut r = region(RegionType::Satin);
    r.angle = 90.0;
    r.density = 4.5;
    r.underlay = true;
    r.underlay_style = UnderlayStyle::Cross;
    r.entry = Some(v2(300.0, 160.0));
    let layers = crate::region::layers(&c, &r, &[]);
    assert_eq!(layers.len(), 2, "under sewing, then the satin");
    let under: Vec<Vec2> = layers[0].iter().flatten().copied().collect();
    assert_inside(&layers[0], &c, "cross under sewing");
    let inset = |p: &Vec2| p.x >= 109.9 && p.x <= 490.1 && p.y >= 109.9 && p.y <= 150.1;
    let outside: Vec<&Vec2> = under.iter().filter(|p| !inset(p)).collect();
    assert!(
        outside.len() <= 1 && outside.iter().all(|p| **p == v2(300.0, 160.0)),
        "1 mm inside the outline but for the start point: {outside:?}"
    );
    // Stitches by direction: the edge run (along the edges), the lines (at 45 degrees, up-right
    // first, then up-left), the travel to each half's corner.
    let kind = |a: Vec2, b: Vec2| -> &'static str {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        if dx.abs() < 0.01 || dy.abs() < 0.01 {
            "edge"
        } else if (dx.abs() - dy.abs()).abs() < 0.01 {
            if dx * dy < 0.0 { "up-right" } else { "up-left" }
        } else {
            "travel"
        }
    };
    let kinds: Vec<&str> = under.windows(2).map(|w| kind(w[0], w[1])).collect();
    let first_up_right = kinds.iter().position(|&k| k == "up-right").expect("lines");
    let first_up_left = kinds.iter().position(|&k| k == "up-left").expect("lines");
    assert!(
        first_up_right < first_up_left,
        "direction - 45 degrees first"
    );
    assert!(kinds[first_up_left..].iter().all(|&k| k != "up-right"));
    // Each layer's lines are 2 mm apart (2.83 mm along the edges): 14 of them, as PE-DESIGN's.
    let count = |k: &str| kinds.iter().filter(|&&x| x == k).count();
    assert!(
        (26..=30).contains(&count("up-right")),
        "{}",
        count("up-right")
    );
    assert!(
        (26..=30).contains(&count("up-left")),
        "{}",
        count("up-left")
    );
    // The first half goes to the corner first along the outline from the start point (the
    // bottom-left for the up-left lines; the top-left, reached by a straight travel, for
    // the up-right lines) and every half ends at the start point.
    let at = |i: usize| under[i];
    assert!(
        at(first_up_right).x < 140.0 && at(first_up_right).y > 130.0,
        "{:?}",
        at(first_up_right)
    );
    assert!(
        at(first_up_left).x < 140.0 && at(first_up_left).y > 140.0,
        "{:?}",
        at(first_up_left)
    );
    // Every half ends at the start point on the inset; the layer then steps out to the
    // region's start point, where the top satin begins.
    assert_eq!(under[under.len() - 2], v2(300.0, 150.0));
    assert_eq!(under.last().copied(), Some(v2(300.0, 160.0)));
    assert!(count("travel") >= 8, "{}", count("travel"));
    assert_eq!(layers[1][0][0], v2(300.0, 160.0));
}

#[test]
fn underlay_styles_layer_inside_the_region() {
    let c = vec![circle()];
    let mut r = region(RegionType::Fill);
    r.underlay = true;
    // Every style is a combination of the under sewing's parts; each sews inside the region,
    // 1 mm inside the outline, before the top stitching.
    for (style, parts) in [
        (UnderlayStyle::Edge, "edge"),
        (UnderlayStyle::Zigzag, "zigzag"),
        (UnderlayStyle::EdgeZigzag, "edge, zigzag"),
        (UnderlayStyle::EdgeFill, "edge, zigzag"),
        (UnderlayStyle::Double, "lattice"),
        (UnderlayStyle::Cross, "edge, lattice"),
    ] {
        r.underlay_style = style;
        assert_eq!(r.underlay_parts().names().join(", "), parts, "{style:?}");
        let out = crate::region::layers(&c, &r, &[]);
        assert!(out.len() >= 2, "{style:?}");
        for l in &out {
            assert_inside(l, &c, &format!("{style:?}"));
        }
        // PE-DESIGN's types: entered from the start point on the outline, the rest 1 mm
        // inside (OpenStitch's older styles keep their own insets, see `legacy_underlay`).
        if r.legacy_underlay() {
            continue;
        }
        for l in &out[..out.len() - 1] {
            assert!(
                l.iter()
                    .flat_map(|run| run.iter().skip(1))
                    .all(|p| p.distance(v2(200.0, 200.0)) <= 150.0 - 9.0),
                "{style:?}: under sewing on the outline"
            );
        }
    }
}

/// Distinct positions (0.01 mm) with at least two stitches of `runs` longer than `min` along
/// `axis` (0 = x, 1 = y) that run straight along the other axis (a lone travel stitch is not
/// a line).
fn straight_lines(runs: &[Run], axis: usize, min: f64) -> Vec<i64> {
    let mut at: Vec<i64> = runs
        .iter()
        .flat_map(|r| r.windows(2))
        .filter(|w| {
            let d = w[1] - w[0];
            let (along, across) = if axis == 0 { (d.y, d.x) } else { (d.x, d.y) };
            across.abs() < 0.05 && along.abs() >= min
        })
        .map(|w| {
            let p = w[0];
            ((if axis == 0 { p.x } else { p.y }) * 10.0).round() as i64
        })
        .collect();
    at.sort_unstable();
    let mut lines: Vec<i64> = at
        .windows(2)
        .filter(|w| w[0] == w[1])
        .map(|w| w[0])
        .collect();
    lines.dedup();
    lines
}

#[test]
fn under_sewing_density_sets_the_row_count_with_rows_on_the_inset_edges() {
    // The 40 x 40 mm square of compat sweep-fill-us-density-*: PE-DESIGN's rows lie 1 mm
    // inside the outline, on both inset edges, 19 gaps of 2.0 mm at Medium, 27 of 1.407 at
    // Dense and 11 of 3.455 at Light.
    let c = vec![Contour::rectangle(100.0, 100.0, 400.0, 400.0)];
    let mut r = region(RegionType::Fill);
    r.angle = 0.0;
    r.underlay = true;
    for (density, rows) in [
        (crate::UnderlayDensity::Medium, 20),
        (crate::UnderlayDensity::Dense, 28),
        (crate::UnderlayDensity::Light, 12),
    ] {
        r.underlay_density = density;
        let out = crate::region::layers(&c, &r, &[]);
        let xs = straight_lines(&out[0], 0, 20.0);
        assert_eq!(xs.len(), rows, "{density:?}: {xs:?}");
        assert_eq!((xs[0], xs[xs.len() - 1]), (1100, 4900), "{density:?}");
        let gap = (xs[xs.len() - 1] - xs[0]) as f64 / (rows - 1) as f64;
        assert!(
            xs.windows(2)
                .all(|w| ((w[1] - w[0]) as f64 - gap).abs() <= 1.0),
            "{density:?}: {xs:?}"
        );
    }
    // An explicit spacing overrides the Density.
    r.underlay_spacing = Some(4.0);
    let out = crate::region::layers(&c, &r, &[]);
    assert_eq!(straight_lines(&out[0], 0, 20.0).len(), 11);
    // Satin rows across the column take the same rule inside the 1 mm inset: the 40 x 6 mm
    // satin of sweep-satin-us-* has rows at 11, 13 and 15 mm at Medium.
    let c = vec![Contour::rectangle(100.0, 100.0, 400.0, 60.0)];
    let mut r = region(RegionType::Satin);
    r.angle = 90.0;
    r.underlay = true;
    r.entry = Some(v2(300.0, 160.0));
    for (density, ys) in [
        (crate::UnderlayDensity::Medium, vec![1100, 1300, 1500]),
        (crate::UnderlayDensity::Dense, vec![1100, 1233, 1367, 1500]),
        (crate::UnderlayDensity::Light, vec![1100, 1500]),
    ] {
        r.underlay_density = density;
        let out = crate::region::layers(&c, &r, &[]);
        let rows = &out[out.len() - 2];
        assert_eq!(straight_lines(rows, 1, 20.0), ys, "{density:?}");
    }
}

#[test]
fn under_sewing_parts_combine_and_name_themselves() {
    use crate::UnderlayParts;
    assert_eq!(
        UnderlayParts::from_names(["lattice", "edge"]),
        Some(UnderlayParts {
            edge: true,
            lattice: true,
            ..UnderlayParts::default()
        })
    );
    assert_eq!(UnderlayParts::from_names(["edge", "net"]), None);
    assert_eq!(
        UnderlayParts::from_names(["center", "zigzag", "lattice", "edge"])
            .expect("parts")
            .names(),
        ["edge", "zigzag", "lattice", "center"]
    );
    assert!(!UnderlayParts::default().any());
    let c = vec![Contour::rectangle(100.0, 100.0, 400.0, 400.0)];
    let mut r = region(RegionType::Fill);
    r.angle = 0.0;
    r.underlay = true;
    let with = |names: &[&str]| {
        let mut r = r.clone();
        r.underlay_parts = UnderlayParts::from_names(names.iter().copied());
        crate::region::layers(&c, &r, &[])
    };
    // Nothing on: the top stitching alone.
    assert_eq!(with(&[]).len(), 1);
    // Edge, rows and piping are one sequence; the lattice follows as its own.
    let all = with(&["edge", "zigzag", "lattice", "center"]);
    assert_eq!(all.len(), 3);
    let first = &all[0];
    // 20 rows across the sewing direction and the piping down the middle, along them.
    let xs = straight_lines(first, 0, 20.0);
    assert_eq!(xs.len(), 21, "rows and piping: {xs:?}");
    assert!(xs.contains(&3000), "piping along the middle");
    assert!(!straight_lines(&with(&["edge", "zigzag"])[0], 0, 20.0).contains(&3000));
    let edge = with(&["edge"]);
    assert_eq!(edge.len(), 2);
    assert_eq!(straight_lines(&edge[0], 0, 20.0), [1100, 4900], "no rows");
    let lattice = with(&["lattice"]);
    assert_eq!(lattice.len(), 2);
    let diagonal = lattice[0]
        .iter()
        .flat_map(|run| run.windows(2))
        .filter(|w| {
            let d = w[1] - w[0];
            (d.x.abs() - d.y.abs()).abs() < 0.5 && d.x.abs() > 20.0
        })
        .count();
    assert!(diagonal > 50, "{diagonal} lattice stitches");
    // Without the edge run the lattice only walks the inset outline between its lines and
    // back to the start point; the edge run adds a whole side of points along x = 11 mm.
    let on_left = |layer: &[Run]| {
        layer
            .iter()
            .flatten()
            .filter(|p| (p.x - 110.0).abs() < 0.01)
            .count()
    };
    let with_edge = with(&["edge", "lattice"]);
    assert!(
        on_left(&with_edge[0]) >= on_left(&lattice[0]) + 10,
        "{} with the edge run, {} without",
        on_left(&with_edge[0]),
        on_left(&lattice[0])
    );
    // OpenStitch's own pre-PE styles keep their geometry unless parts are set: `edge` runs on
    // the outline itself, `double` sews two plain crossing layers (no edge, no halves).
    let mut old = r.clone();
    old.underlay_style = UnderlayStyle::Edge;
    assert!(old.legacy_underlay());
    let layers = crate::region::layers(&c, &old, &[]);
    assert_eq!(layers[0], vec![crate::generate::running(&c[0], 25.0, 1)]);
    old.underlay_style = UnderlayStyle::Double;
    assert_eq!(crate::region::layers(&c, &old, &[]).len(), 3);
    old.underlay_style = UnderlayStyle::EdgeFill;
    assert!(!old.legacy_underlay());
    old.underlay_style = UnderlayStyle::Edge;
    old.underlay_parts = Some(UnderlayParts {
        edge: true,
        ..UnderlayParts::default()
    });
    assert!(
        !old.legacy_underlay(),
        "explicit parts take the new generators"
    );
    // Explicit parts win over the style.
    let mut both = r.clone();
    both.underlay_style = UnderlayStyle::Cross;
    both.underlay_parts = Some(UnderlayParts {
        edge: true,
        ..UnderlayParts::default()
    });
    assert_eq!(both.underlay_parts().names(), ["edge"]);
}

#[test]
fn column_under_sewing_sews_the_edge_the_piping_and_the_zigzag_as_pe_design() {
    // PE-DESIGN 11.50's Manual Punch column of the round-4 captures (satin-col-us-*): a
    // 39.5 x 6 mm straight satin column from (10.5, 10) mm, 4.5 lines/mm, Density Medium:
    // 186 stitches with under sewing off, 286 with the edge run, the piping and the zigzag
    // (edge 39 stitches, piping 17, zigzag 42 plus the travel), 225 with the edge alone, 249
    // with the piping and zigzag.
    let rungs = vec![
        [v2(105.0, 100.0), v2(105.0, 160.0)],
        [v2(500.0, 100.0), v2(500.0, 160.0)],
    ];
    let mut r = region(RegionType::Satin);
    r.direction_lines = rungs.clone();
    r.underlay = true;
    let outline = crate::column::outline(&rungs);
    let count = |names: &[&str]| {
        let mut r = r.clone();
        r.underlay_parts = crate::UnderlayParts::from_names(names.iter().copied());
        let layers = crate::region::layers(std::slice::from_ref(&outline), &r, &[]);
        assert_eq!(layers.len(), 2, "{names:?}");
        let under = &layers[0];
        assert_eq!(under.len(), 1, "{names:?}: one run");
        let run = &under[0];
        assert_eq!(
            (run[0], run[run.len() - 1]),
            (v2(105.0, 100.0), v2(105.0, 100.0))
        );
        // Everything lies within the 0.5 mm inset but the travel from the start point.
        let outside = run[1..run.len() - 1]
            .iter()
            .filter(|p| !((109.0..=496.0).contains(&p.x) && (104.0..=156.0).contains(&p.y)))
            .count();
        assert!(
            outside <= 2,
            "{names:?}: {outside} points outside the 0.5 mm inset"
        );
        for w in run.windows(2) {
            assert!(w[0].distance(w[1]) <= 40.0 + 1e-6, "{names:?}: {w:?}");
        }
        run.len() - 1
    };
    // PE-DESIGN's exclusive buttons map onto the parts: the default style sews all three.
    assert_eq!(r.underlay_parts().names(), ["edge", "zigzag", "center"]);
    let mut by_style = r.clone();
    by_style.underlay_style = UnderlayStyle::Edge;
    assert_eq!(by_style.underlay_parts().names(), ["edge"]);
    by_style.underlay_style = UnderlayStyle::Zigzag;
    assert_eq!(by_style.underlay_parts().names(), ["zigzag", "center"]);
    let within = |n: usize, pe: usize| (n as f64 - pe as f64).abs() <= pe as f64 * 0.1;
    let all = count(&["edge", "center", "zigzag"]);
    assert!(
        within(all, 100),
        "edge + piping + zigzag: {all} stitches to PE-DESIGN's 100"
    );
    let edge = count(&["edge"]);
    assert!(within(edge, 39), "edge: {edge} to PE-DESIGN's 39");
    let inner = count(&["center", "zigzag"]);
    assert!(
        within(inner, 63),
        "piping + zigzag: {inner} to PE-DESIGN's 63"
    );
    // The zigzag's needle points lie on the inset rails, a whole number of gaps apart (19 of
    // 2.03 mm over 38.5 mm at Medium).
    let mut r = r.clone();
    r.underlay_parts = crate::UnderlayParts::from_names(["zigzag"]);
    let layers = crate::region::layers(std::slice::from_ref(&outline), &r, &[]);
    let rails: Vec<Vec2> = layers[0][0]
        .iter()
        .copied()
        .filter(|p| (p.y - 105.0).abs() < 0.01 || (p.y - 155.0).abs() < 0.01)
        .collect();
    assert_eq!(rails.len(), 20, "{rails:?}");
    assert!(
        rails
            .windows(2)
            .all(|w| ((w[0].x - w[1].x).abs() - 20.263).abs() < 0.05)
    );
    assert!(
        rails.windows(2).all(|w| (w[0].y - w[1].y).abs() > 49.0),
        "alternating rails"
    );
    // No under sewing, no layer.
    r.underlay = false;
    assert_eq!(
        crate::region::layers(std::slice::from_ref(&outline), &r, &[]).len(),
        1
    );
}

#[test]
fn regular_stagger_is_the_frequency_brick_and_the_third_without_one() {
    use crate::generate::Stagger;
    // The old third-of-a-stitch stagger cycles 0, 1/3, 2/3 from the page origin; the regular
    // stagger with a 1/3 offset walks the same three phases, anchored at the first row's
    // start as PE-DESIGN's Frequency grid is ((k + 1) · f).
    let third: Vec<f64> = (0..6).map(|k| Stagger::Third.phase(k)).collect();
    assert_eq!(
        third,
        [0.0, 1.0 / 3.0, 2.0 / 3.0, 0.0, 1.0 / 3.0, 2.0 / 3.0]
    );
    let regular = Stagger::Regular(1.0 / 3.0);
    for k in 0..6 {
        let a = regular.anchored(k).expect("anchored");
        assert!((a - ((k + 1) % 3) as f64 / 3.0).abs() < 1e-9, "{k}: {a}");
    }
    assert_eq!(Stagger::Third.anchored(0), None);
    assert!((Stagger::Regular(0.3).anchored(2).expect("anchored") - 0.9).abs() < 1e-9);
    // A region's stagger: the Frequency unless the random one is chosen; no Frequency, the third.
    let mut r = region(RegionType::Fill);
    assert_eq!(r.fill_stagger(), Stagger::Regular(0.3));
    r.frequency = None;
    assert_eq!(r.fill_stagger(), Stagger::Third);
    r.stagger = crate::StaggerKind::Random;
    assert_eq!(
        r.fill_stagger(),
        Stagger::Random(RegionSewing::DEFAULT_STAGGER_SEED)
    );
    r.stagger_seed = Some(42);
    assert_eq!(r.fill_stagger(), Stagger::Random(42));
}

#[test]
fn random_stagger_is_seeded_reproducible_and_does_not_repeat() {
    use crate::generate::Stagger;
    let a: Vec<f64> = (0..40)
        .map(|k| Stagger::Random(7).anchored(k).expect("phase"))
        .collect();
    let b: Vec<f64> = (0..40)
        .map(|k| Stagger::Random(7).anchored(k).expect("phase"))
        .collect();
    assert_eq!(a, b, "the same seed gives the same rows");
    let c: Vec<f64> = (0..40)
        .map(|k| Stagger::Random(8).anchored(k).expect("phase"))
        .collect();
    assert_ne!(a, c, "another seed gives other rows");
    assert!(a.iter().all(|&f| (0.0..1.0).contains(&f)));
    let mut sorted: Vec<i64> = a.iter().map(|f| (f * 1e9) as i64).collect();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 40, "no two rows share an offset: {a:?}");
    // A fill sewn twice with the same seed is identical; with the regular stagger its interior
    // needle points fall on three columns per 1.2 mm shift cycle, with the random one they do not.
    let c = vec![Contour::rectangle(100.0, 100.0, 400.0, 400.0)];
    let mut r = region(RegionType::Fill);
    r.angle = 0.0;
    r.underlay = false;
    r.stagger = crate::StaggerKind::Random;
    r.stagger_seed = Some(3);
    let once = crate::region::layers(&c, &r, &[]);
    let twice = crate::region::layers(&c, &r, &[]);
    assert_eq!(once, twice);
    let offsets = |layers: &[Vec<Run>]| -> usize {
        let mut xs: Vec<i64> = layers
            .last()
            .expect("top")
            .iter()
            .flatten()
            .filter(|p| p.x > 112.0 && p.x < 488.0)
            .map(|p| ((p.x - 100.0).rem_euclid(40.0) * 10.0).round() as i64)
            .collect();
        xs.sort_unstable();
        xs.dedup();
        xs.len()
    };
    assert!(
        offsets(&once) > 50,
        "{} distinct grid offsets",
        offsets(&once)
    );
    r.stagger = crate::StaggerKind::Regular;
    let regular = crate::region::layers(&c, &r, &[]);
    assert!(
        offsets(&regular) <= 12,
        "{} distinct grid offsets",
        offsets(&regular)
    );
    for l in &once {
        assert_inside(l, &c, "random stagger");
    }
}

#[test]
fn new_fills_start_at_45_degrees_and_saved_designs_keep_their_angle() {
    // PE-DESIGN 11.50's shape tools start Fill Stitch at 45 degrees (pes-objects r01) and the
    // guide gives programmable fill 45; the other types start at 0.
    assert_eq!(region(RegionType::Fill).angle, 45.0);
    assert_eq!(region(RegionType::ProgrammableFill).angle, 45.0);
    assert_eq!(region(RegionType::Satin).angle, 0.0);
    assert_eq!(RegionSewing::default_angle(RegionType::NetFill), 0.0);
    // Every saved region stores its angle, so a design saved with a 0-degree fill (the old
    // default) loads at 0 degrees, and a region without one is not a valid file.
    let mut r = region(RegionType::Fill);
    r.angle = 0.0;
    let json = serde_json::to_value(&r).expect("json");
    assert_eq!(json["angle"], 0.0);
    let back: RegionSewing = serde_json::from_value(json.clone()).expect("load");
    assert_eq!(back.angle, 0.0);
    let mut missing = json;
    missing.as_object_mut().expect("object").remove("angle");
    assert!(serde_json::from_value::<RegionSewing>(missing).is_err());
}

#[test]
fn gradation_spreads_rows_towards_the_far_side() {
    let c = vec![Contour::rectangle(0.0, 0.0, 100.0, 200.0)];
    let even = crate::region::row_positions(&c, 0.0, 4.0, None);
    let graded =
        crate::region::row_positions(&c, 0.0, 4.0, Some(&crate::Gradation::continuous(25.0)));
    assert_eq!(even.len(), 50);
    assert!(graded.len() < 40 && graded.len() > 25, "{}", graded.len());
    let gaps: Vec<f64> = graded.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(gaps.windows(2).all(|g| g[1] >= g[0]));
    assert!(gaps[gaps.len() - 1] > 3.0 * gaps[0]);
    let mut r = region(RegionType::Satin);
    r.gradation = Some(crate::Gradation::continuous(25.0));
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    assert_inside(&runs, &c, "gradation");
}

#[test]
fn gradation_pattern_sews_rows_at_equal_steps_of_the_cumulative_line_count() {
    // The U of compat region-travel-u-gradation: 30 mm tall at 4.5 lines/mm with PE-DESIGN's
    // default pattern (53 % at the top falling linearly to 5 % at the bottom). PE-DESIGN 11.50
    // sews 40 rows; a 30 × 30 mm square has the same rows.
    let c = vec![Contour::rectangle(100.0, 100.0, 300.0, 300.0)];
    let spacing = 10.0 / 4.5;
    let g = crate::Gradation::PE_DEFAULT;
    let ys = crate::region::row_positions(&c, 0.0, spacing, Some(&g));
    assert_eq!(ys.len(), 40, "{ys:?}");
    // First row on the start edge, last on the far edge (sampled just inside).
    assert!(ys[0] < 100.01 && ys[39] > 400.0 - 0.01, "{ys:?}");
    // PE-DESIGN's rows (page mm, from its reference file), every one within 0.12 mm.
    let pe = [
        10.0, 10.4, 10.8, 11.3, 11.7, 12.2, 12.6, 13.1, 13.5, 14.0, 14.5, 15.0, 15.5, 16.0, 16.5,
        17.0, 17.6, 18.1, 18.7, 19.3, 19.9, 20.5, 21.1, 21.8, 22.4, 23.1, 23.8, 24.6, 25.3, 26.1,
        27.0, 27.9, 28.8, 29.8, 30.9, 32.1, 33.5, 35.1, 37.1, 40.0,
    ];
    for (y, p) in ys.iter().zip(pe) {
        assert!((y / 10.0 - p).abs() <= 0.12, "{y} vs {p}: {ys:?}");
    }
    // Equal steps of the cumulative line count: the density integrated over every gap is the
    // same, 39 gaps for the 39.15 lines the profile integrates to.
    let lines = |y0: f64, y1: f64| {
        let f = |y: f64| g.fraction((y - 100.0) / 300.0);
        (y1 - y0) * (f(y0) + f(y1)) / 2.0 / spacing
    };
    let per_gap = lines(ys[0], ys[1]);
    assert!((per_gap - 39.15 / 39.0).abs() < 0.01, "{per_gap}");
    for w in ys.windows(2) {
        assert!((lines(w[0], w[1]) - per_gap).abs() < 1e-3, "{w:?}");
    }
    assert!((g.fraction(0.5) - 0.29).abs() < 1e-12);
    let mut r = region(RegionType::Fill);
    r.gradation = Some(crate::Gradation::PE_DEFAULT);
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    assert_inside(&runs, &c, "gradation pattern");
}

#[test]
fn satin_at_the_minimum_density_crosses_on_every_line() {
    // The 40 x 6 mm column of compat sweep-satin-density-min at 0.1 lines/mm, start point at
    // the middle of the bottom edge: PE-DESIGN 11.50 sews crossings 10 mm apart (every line)
    // with 11.7 mm diagonals, not 20 mm apart with 20.9 mm ones.
    let c = vec![Contour::rectangle(100.0, 100.0, 400.0, 60.0)];
    let mut r = region(RegionType::Satin);
    r.angle = 90.0;
    r.density = 0.1;
    r.underlay = false;
    r.entry = Some(v2(300.0, 160.0));
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    let run = &runs[0];
    let mut crossings: Vec<f64> = run
        .windows(2)
        .filter(|w| (w[0].x - w[1].x).abs() < 0.01 && (w[0].y - w[1].y).abs() > 59.0)
        .map(|w| w[0].x)
        .collect();
    crossings.sort_by(f64::total_cmp);
    let rounded: Vec<f64> = crossings.iter().map(|x| x.round()).collect();
    assert_eq!(
        rounded,
        [100.0, 200.0, 300.0, 300.0, 400.0, 500.0],
        "{run:?}"
    );
    let longest = run
        .windows(2)
        .map(|w| w[0].distance(w[1]))
        .fold(0.0_f64, f64::max);
    assert!((longest - 116.6).abs() < 0.5, "{longest}");
    // At 3 lines/mm the start point lies on line 60: the left half crosses the far end's line
    // (0) and every other line counted back from the hop (59, 57, ..., 1), then the hop at the
    // start point; the single line next to the far end (0.33 mm) is crossed, as plain satin
    // crosses it at every size (pe-refs-4's satin-length-sweep: 0.22, 0.33, 1 and 5 mm, #122).
    // Only a feathered satin spreads those crossings evenly (satin-feather).
    r.density = 3.0;
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    let mut xs: Vec<f64> = runs[0]
        .windows(2)
        .filter(|w| (w[0].x - w[1].x).abs() < 0.01 && (w[0].y - w[1].y).abs() > 59.0)
        .map(|w| w[0].x)
        .filter(|&x| x < 300.5)
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    assert_eq!(xs.len(), 32, "{xs:?}");
    let line = 200.0 / 60.0;
    assert!((xs[0] - 100.0).abs() < 0.05, "{xs:?}");
    for (k, x) in xs[1..31].iter().enumerate() {
        let want = 100.0 + line * (1.0 + 2.0 * k as f64);
        assert!((x - want).abs() < 0.05, "{k}: {x} vs {want}");
    }
    assert!((xs[31] - 300.0).abs() < 1e-6, "{xs:?}");
}

/// A 3 mm wide satin column `length` mm long at direction 0 (rows along x, so the crossings
/// run across the 3 mm), started at its top-left corner as an imported SVG rectangle is, as
/// pe-refs-4's satin-length-sweep columns: its top stitching.
fn sweep_column(length: f64, density: f64) -> Run {
    let c = vec![Contour::new(
        vec![
            v2(0.0, 0.0),
            v2(30.0, 0.0),
            v2(30.0, length * 10.0),
            v2(0.0, length * 10.0),
        ],
        true,
    )];
    let mut r = region(RegionType::Satin);
    r.angle = 0.0;
    r.density = density;
    r.underlay = false;
    let mut layers = crate::region::layers(&c, &r, &[]);
    crate::region::tie_and_lock(&mut layers);
    layers.concat().concat()
}

/// The rows (0.1 mm, from the top) crossed by full-width stitches of `run`, in sewing order.
fn crossings(run: &Run) -> Vec<i64> {
    run.windows(2)
        .filter(|w| (w[0].x - w[1].x).abs() > 29.0)
        .map(|w| w[1].y.round() as i64)
        .collect()
}

#[test]
fn satin_lattice_stretches_to_whole_gaps_and_crosses_every_far_end_gap() {
    // PE-DESIGN 11.50 (satin-length-sweep-d1p0 / -d0p2 / -d4p5, #122): from the start point
    // (top-left) along the edge to the far end, then crossings on the far end's line and on
    // every other line counted back from the start, on a lattice of floor(length x density)
    // equal gaps; the single-line gap next to the far end is always crossed.
    let rows = |len: f64, d: f64| {
        let mut ys = crossings(&sweep_column(len, d));
        ys.dedup();
        ys
    };
    assert_eq!(rows(3.0, 1.0), [0, 30, 20, 0]);
    assert_eq!(rows(5.0, 1.0), [0, 50, 40, 20, 0]);
    assert_eq!(rows(10.0, 1.0), [0, 100, 80, 60, 40, 20, 0]);
    assert_eq!(rows(15.0, 0.2), [0, 150, 100, 0]);
    assert_eq!(rows(10.0, 0.2), [0, 100, 0]);
    // 15 mm at 4.5 lines/mm: 67 gaps of 0.2239 mm, not 0.2222 (PE: 15.0, 14.8, 14.3 ...).
    let ys = rows(15.0, 4.5);
    assert_eq!(&ys[..4], [0, 150, 148, 143]);
    let gap = 150.0 / 67.0;
    for (k, &y) in ys[2..ys.len() - 1].iter().enumerate() {
        let line = 1.0 + 2.0 * k as f64;
        assert!((y as f64 - (150.0 - gap * line)).abs() <= 0.5, "{k}: {y}");
    }
    // 6.75 lines round half down to 7 gaps of 0.214 mm (PE: 1.5, 1.3, 0.9, 0.4, 0;
    // satin-length-sweep-noopt-d4p5), 49.5 to 49 (PE's 11 mm column: 11.0, 10.8, 10.3 ...).
    assert_eq!(rows(1.5, 4.5), [0, 15, 13, 9, 4, 0]);
    assert_eq!(&rows(11.0, 4.5)[..4], [0, 110, 108, 103]);
    // The column ends on its start point, its last crossing towards it.
    let run = sweep_column(3.0, 1.0);
    let n = run.len();
    assert!(
        run[n - 3..]
            .iter()
            .all(|p| p.y.abs() < 0.05 && p.x <= 3.0 + 1e-6),
        "{:?}",
        &run[n - 6..]
    );
}

#[test]
fn satin_ties_in_three_steps_after_a_three_millimetre_first_stitch() {
    // PE-DESIGN: one 0.3 mm step after a first stitch of up to 2.3 mm, three (0.3, 0.6,
    // 0.9 mm) after 3.0 mm (the 3 mm wide sweep column) and 3.5 mm.
    // (Rows on an edge lie 0.001 mm inside it.)
    let near = |a: &[Vec2], b: &[Vec2]| {
        a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.distance(*q) < 0.05)
    };
    let three = sweep_column(3.0, 1.0);
    assert!(
        near(
            &three[..4],
            &[v2(0.0, 0.0), v2(3.0, 0.0), v2(6.0, 0.0), v2(9.0, 0.0)]
        ),
        "{:?}",
        &three[..4]
    );
    let mut r = region(RegionType::Satin);
    r.angle = 0.0;
    r.underlay = false;
    let narrow = vec![Contour::new(
        vec![v2(0.0, 0.0), v2(20.0, 0.0), v2(20.0, 50.0), v2(0.0, 50.0)],
        true,
    )];
    let mut layers = crate::region::layers(&narrow, &r, &[]);
    crate::region::tie_and_lock(&mut layers);
    let run = layers.concat().concat();
    assert!(
        near(&run[..3], &[v2(0.0, 0.0), v2(3.0, 0.0), v2(0.0, 0.0)]),
        "{:?}",
        &run[..3]
    );
}

#[test]
fn satin_gradation_splits_on_every_other_lattice_line() {
    // The 20 x 5 mm rectangle of compat satin-gradation: satin at 3 lines/mm with PE-DESIGN's
    // default pattern, under sewing off, start point at the middle of the bottom edge.
    // PE-DESIGN 11.50 sews the left half from the left edge with vertical crossings on every
    // other line of the cumulative-line-count lattice (lines 0, 2, ... 10), hops one line (11)
    // and crosses, runs home; then the right half from the right edge (lines 17, 15, 13) and
    // the hop to the start point's line (12).
    let c = vec![Contour::rectangle(100.0, 100.0, 200.0, 50.0)];
    let g = crate::Gradation::PE_DEFAULT;
    assert_eq!(
        crate::region::row_positions(&c, 90.0, 10.0 / 3.0, Some(&g)).len(),
        18
    );
    let mut r = region(RegionType::Satin);
    r.angle = 90.0;
    r.density = 3.0;
    r.underlay = false;
    r.gradation = Some(g);
    r.entry = Some(v2(200.0, 150.0));
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    assert_eq!(runs.len(), 1, "one run, split at the start point");
    let run = &runs[0];
    let crossings: Vec<f64> = run
        .windows(2)
        .filter(|w| (w[0].x - w[1].x).abs() < 0.01 && (w[0].y - w[1].y).abs() > 49.0)
        .map(|w| w[0].x / 10.0)
        .collect();
    let pe = [
        10.0, 11.3, 12.7, 14.3, 15.9, 17.8, 18.8, 30.0, 24.3, 21.2, 20.0,
    ];
    assert_eq!(crossings.len(), pe.len(), "{crossings:?}");
    for (x, p) in crossings.iter().zip(pe) {
        assert!((x - p).abs() <= 0.15, "{x} vs {p}: {crossings:?}");
    }
    assert_eq!(run.len(), 32, "{run:?}"); // 37 stitches with the tie-in and lock
    assert_inside(&runs, &c, "satin gradation");
}

#[test]
fn fill_under_sewing_rows_stay_inside_narrow_points() {
    // The star of compat hole-sewing-pentagon-star: rows across its points are shorter than
    // the 1 mm inset at both ends; they must not collapse to a needle point outside the star.
    let pts = [
        (50.0, 42.0),
        (52.3511, 48.7639),
        (59.5106, 48.9098),
        (53.8042, 53.2361),
        (55.8779, 60.0902),
        (50.0, 56.0),
        (44.1221, 60.0902),
        (46.1958, 53.2361),
        (40.4894, 48.9098),
        (47.6489, 48.7639),
    ];
    let star = vec![Contour::new(
        pts.iter().map(|&(x, y)| v2(x * 10.0, y * 10.0)).collect(),
        true,
    )];
    let r = region(RegionType::Fill);
    assert!(r.underlay);
    let layers = crate::region::layers(&star, &r, &[]);
    for l in &layers {
        assert_inside(l, &star, "star");
    }
    // The under sewing is one run: no jump or trim inside the star.
    assert_eq!(layers[0].len(), 1);
}

#[test]
fn gradation_reads_the_old_single_percentage() {
    let old: crate::Gradation = serde_json::from_str("40").expect("number");
    assert_eq!(old, crate::Gradation::continuous(40.0));
    let pattern = serde_json::to_string(&crate::Gradation::PE_DEFAULT).expect("json");
    assert_eq!(pattern, r#"{"max":53.0,"min":5.0,"steps":9}"#);
    let back: crate::Gradation = serde_json::from_str(&pattern).expect("pattern");
    assert_eq!(back, crate::Gradation::PE_DEFAULT);
    // Continuous: the density falls linearly, as before patterns.
    let g = crate::Gradation::continuous(25.0);
    assert!((g.fraction(0.0) - 1.0).abs() < 1e-12 && (g.fraction(1.0) - 0.25).abs() < 1e-12);
    assert!((g.fraction(0.5) - 0.625).abs() < 1e-12);
}

#[test]
fn feathered_edges_shorten_row_ends_deterministically() {
    let c = vec![Contour::rectangle(0.0, 0.0, 200.0, 100.0)];
    let mut r = region(RegionType::Satin);
    r.feather = Some(Feather {
        side: FeatherSide::Start,
        width: 3.0,
        ..Default::default()
    });
    let a = crate::region::layers(&c, &r, &[]);
    assert_eq!(a, crate::region::layers(&c, &r, &[]));
    let runs = a.last().expect("top");
    let xs: Vec<f64> = runs.iter().flatten().map(|p| p.x).collect();
    assert!(xs.iter().all(|&x| (0.0..=200.0).contains(&x)));
    // Start ends vary across 0…3 mm; far ends stay on the outline.
    let starts: Vec<f64> = xs.iter().copied().filter(|&x| x < 100.0).collect();
    assert!(starts.iter().all(|&x| x <= 30.0), "{starts:?}");
    assert!(starts.iter().copied().fold(0.0, f64::max) > 20.0);
    assert!(xs.iter().filter(|&&x| x > 100.0).all(|&x| x == 200.0));
    r.feather = Some(Feather {
        side: FeatherSide::Both,
        width: 3.0,
        ..Default::default()
    });
    let both = crate::region::layers(&c, &r, &[]).pop().expect("top");
    assert!(both.iter().flatten().any(|p| p.x < 199.0 && p.x > 170.0));
}

#[test]
fn feathered_edges_go_out_both_ways_and_take_a_seed() {
    // #685, beyond PE-DESIGN: out lengthens the start ends past the edge, both moves them either
    // way, and a seed gives a pattern of its own that the same seed repeats.
    let c = vec![Contour::rectangle(0.0, 0.0, 200.0, 100.0)];
    let mut r = region(RegionType::Satin);
    let starts = |r: &RegionSewing| -> Vec<f64> {
        let top = crate::region::layers(&c, r, &[]).pop().expect("top");
        top.iter()
            .flatten()
            .map(|p| p.x)
            .filter(|&x| x < 100.0)
            .collect()
    };
    let feather = |mode, seed| {
        Some(Feather {
            side: FeatherSide::Start,
            width: 3.0,
            mode,
            seed,
        })
    };
    r.feather = feather(FeatherMode::Out, None);
    let out = starts(&r);
    assert!(out.iter().all(|&x| (-30.0..=0.0).contains(&x)), "{out:?}");
    assert!(out.iter().copied().fold(0.0, f64::min) < -20.0);
    r.feather = feather(FeatherMode::Both, None);
    let both = starts(&r);
    assert!(
        both.iter().all(|&x| (-15.0..=15.0).contains(&x)),
        "{both:?}"
    );
    assert!(both.iter().any(|&x| x < -1.0) && both.iter().any(|&x| x > 1.0));
    r.feather = feather(FeatherMode::In, Some(7));
    let seeded = starts(&r);
    assert_eq!(seeded, starts(&r), "same seed");
    assert!(seeded.iter().all(|&x| (0.0..=30.0).contains(&x)));
    r.feather = feather(FeatherMode::In, None);
    let pe = starts(&r);
    assert_ne!(seeded, pe, "a seed is not PE-DESIGN's pattern");
    r.feather = feather(FeatherMode::In, Some(8));
    assert_ne!(starts(&r), seeded, "another seed");
}

#[test]
fn long_satin_stitches_split_and_stagger() {
    let c = vec![Contour::rectangle(0.0, 0.0, 200.0, 40.0)];
    // Top satin only, rows 1/3 mm apart.
    let plain_satin = || {
        let mut r = region(RegionType::Satin);
        r.density = 3.0;
        r.underlay = false;
        r
    };
    let mut r = plain_satin();
    r.split_length = Some(5.0);
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    // The first stitch of a row comes from the previous row's edge, one row (1/3 mm) away.
    let slant = 50.0f64.hypot(10.0 / 3.0);
    for run in &runs {
        assert!(longest(run) <= slant + 1e-6);
    }
    assert_inside(&runs, &c, "split");
    let plain = crate::region::layers(&c, &plain_satin(), &[]);
    assert!(
        longest(&plain[0][0]) > 190.0,
        "without split a row is one stitch"
    );
    // Split points of neighbouring rows do not line up.
    let pts: Vec<Vec2> = runs.iter().flatten().copied().collect();
    let interior = |y_lo: f64, y_hi: f64| -> Vec<f64> {
        let mut xs: Vec<f64> = pts
            .iter()
            .filter(|p| p.y > y_lo && p.y < y_hi && p.x > 1.0 && p.x < 199.0)
            .map(|p| (p.x * 1000.0).round() / 1000.0)
            .collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        xs
    };
    let (row0, row1) = (interior(0.0, 3.0), interior(3.0, 6.0));
    assert!(!row0.is_empty() && !row1.is_empty());
    assert!(row0.iter().all(|x| !row1.contains(x)));
}

#[test]
fn travel_stays_inside_instead_of_jumping() {
    // A ring: the fill splits into parts on either side of the hole.
    let ring = vec![circle(), Contour::ellipse(v2(200.0, 200.0), 80.0, 80.0, 48)];
    let mut r = region(RegionType::Fill);
    r.running_stitch_path = None;
    let plain = crate::region::layers(&ring, &r, &[]).pop().expect("top");
    assert!(plain.len() > 1, "fill without travel jumps between parts");
    r.running_stitch_path = Some(crate::model::RunningStitchPath::Inside);
    let travelled = crate::region::layers(&ring, &r, &[]).pop().expect("top");
    assert_eq!(travelled.len(), 1);
    // The travel stitches (points not in the plain fill) stay inside the ring. (Fill rows
    // themselves may step across a narrow gap, as plain fill does.)
    let fill_points: Vec<Vec2> = plain.iter().flatten().copied().collect();
    let travel: Vec<Vec2> = travelled[0]
        .iter()
        .filter(|p| !fill_points.contains(p))
        .copied()
        .collect();
    assert!(travel.len() > 2);
    assert!(travel.iter().all(|&p| inside_or_on(p, &ring, 0.6)));
    let plain_longest = plain.iter().map(|r| longest(r)).fold(25.0, f64::max);
    assert!(longest(&travelled[0]) <= plain_longest + 1e-6);
    let mut d = Design::new("ring", DesignPage::default());
    d.add(DesignObject::shape(ring.clone(), Some(r.clone()), None));
    let p = Compiler::default().compile(&d);
    assert_eq!(p.count(Command::Jump), 1, "only the initial move");
    r.running_stitch_path = None;
    d.objects[0].region = Some(r);
    assert!(Compiler::default().compile(&d).count(Command::Jump) > 1);
}

#[test]
fn travel_graph_routes_around_a_notch() {
    let u = [Contour::new(
        vec![
            v2(0.0, 0.0),
            v2(60.0, 0.0),
            v2(60.0, 200.0),
            v2(140.0, 200.0),
            v2(140.0, 0.0),
            v2(200.0, 0.0),
            v2(200.0, 260.0),
            v2(0.0, 260.0),
        ],
        true,
    )];
    let path =
        crate::paths::inside_path(v2(30.0, 10.0), v2(170.0, 10.0), &u, &[]).expect("path inside");
    assert!(path.len() >= 4, "{path:?}");
    let len: f64 = path.windows(2).map(|w| w[0].distance(w[1])).sum();
    assert!(len > 2.0 * 190.0 && len < 2.0 * 190.0 + 200.0, "{len}");
    for w in path.windows(2) {
        for k in 0..=10 {
            assert!(inside_or_on(w[0].lerp(w[1], k as f64 / 10.0), &u, 0.6));
        }
    }
}

#[test]
fn outline_travel_walks_on_or_just_inside_the_outline() {
    use crate::paths::{OutlineTravel, TravelGraph};
    // The U of travel_graph_routes_around_a_notch; both ends on its bottom edges.
    let u = [Contour::new(
        vec![
            v2(0.0, 0.0),
            v2(60.0, 0.0),
            v2(60.0, 200.0),
            v2(140.0, 200.0),
            v2(140.0, 0.0),
            v2(200.0, 0.0),
            v2(200.0, 260.0),
            v2(0.0, 260.0),
        ],
        true,
    )];
    let (a, b) = (v2(30.0, 0.0), v2(170.0, 0.0));
    let mut g = TravelGraph::new(&u, &[]);
    g.outline = Some(OutlineTravel::On);
    let on = g.path(a, b).expect("on outline");
    let edges = u[0].edges();
    let to_outline = |p: Vec2| {
        edges
            .iter()
            .map(|&(e0, e1)| {
                let d = e1 - e0;
                let t = (((p - e0).x * d.x + (p - e0).y * d.y) / (d.x * d.x + d.y * d.y))
                    .clamp(0.0, 1.0);
                p.distance(e0 + d * t)
            })
            .fold(f64::INFINITY, f64::min)
    };
    // The short way round the notch: up, across, down, every point on the outline.
    assert!(on.iter().all(|&p| to_outline(p) < 1e-6), "{on:?}");
    assert!(on.iter().any(|p| (p.y - 200.0).abs() < 1e-6));
    g.outline = Some(OutlineTravel::Inset(5.0));
    let along = g.path(a, b).expect("along outline");
    let interior = &along[1..along.len() - 1];
    assert!(!interior.is_empty());
    // 0.5 mm inside along the edges; miter points at the reflex corners are up to √2 further.
    assert!(
        interior
            .iter()
            .all(|&p| (4.9..=5.0 * std::f64::consts::SQRT_2 + 0.1).contains(&to_outline(p))),
        "{along:?}"
    );
    for w in along.windows(2) {
        for k in 0..=10 {
            assert!(inside_or_on(w[0].lerp(w[1], k as f64 / 10.0), &u, 0.6));
        }
    }
}

// ---------------------------------------------------------------- region types

/// A flower-ish region with a hole: tests clipping, holes and travel together.
fn blob_with_hole() -> Vec<Contour> {
    let outer: Vec<Vec2> = (0..72)
        .map(|k| {
            let t = k as f64 / 72.0 * std::f64::consts::TAU;
            let r = 200.0 + 40.0 * (5.0 * t).sin();
            v2(300.0 + r * t.cos(), 300.0 + r * t.sin())
        })
        .collect();
    vec![
        Contour::new(outer, true),
        Contour::ellipse(v2(300.0, 300.0), 50.0, 50.0, 32),
    ]
}

const NEW_REGION_TYPES: [RegionType; 13] = [
    RegionType::ProgrammableFill,
    RegionType::MotifFill,
    RegionType::Piping,
    RegionType::CrossStitch,
    RegionType::ConcentricCircle,
    RegionType::Radial,
    RegionType::Spiral,
    RegionType::FlexibleSpiral,
    RegionType::Stippling,
    RegionType::NetFill,
    RegionType::ZigzagNetFill,
    RegionType::DecorativeFill,
    RegionType::Contour,
];

/// The longest stitch a region type may make with default attributes (0.1 mm).
fn stitch_limit(r: &RegionSewing) -> f64 {
    let pitch = r.run_pitch_mm() * 10.0;
    match r.kind {
        RegionType::ProgrammableFill => crate::regions::PROGRAMMABLE_MAX_STITCH,
        RegionType::CrossStitch => (r.spacing_mm() * 10.0 * std::f64::consts::SQRT_2).max(pitch),
        RegionType::ZigzagNetFill => {
            // A rung at a right-angle corner reaches √2 × the band's width (the mitre).
            let w = r.pattern_size_mm().0 * 10.0 * std::f64::consts::SQRT_2;
            w.hypot(10.0 / r.density).max(pitch)
        }
        RegionType::DecorativeFill => pitch * 1.5,
        // A blended spiral turn may run 0.05 mm over the pitch, as PE-DESIGN 11.50's does
        // (2.04 mm at a 2 mm pitch, spiral-circle40).
        RegionType::Spiral => pitch + 0.5,
        _ => pitch,
    }
}

#[test]
fn every_region_type_sews_inside_with_bounded_stitches() {
    let region_contours = blob_with_hole();
    for kind in NEW_REGION_TYPES {
        let mut r = region(kind);
        if kind == RegionType::DecorativeFill {
            r.random_shift = Some(30.0);
        }
        let layers = crate::region::layers(&region_contours, &r, &[]);
        assert_eq!(
            layers,
            crate::region::layers(&region_contours, &r, &[]),
            "{kind:?}"
        );
        let runs = layers.last().expect("top layer");
        let points: usize = runs.iter().map(Vec::len).sum();
        assert!(points > 100, "{kind:?}: {points} points");
        // PE-DESIGN 11.50 sews Concentric Circle's whole circles, trims, then the cut ones
        // (references concentric-*); every other type travels from part to part.
        let expected_runs = if kind == RegionType::ConcentricCircle {
            2
        } else {
            1
        };
        assert_eq!(
            runs.len(),
            expected_runs,
            "{kind:?}: travel joins everything into {expected_runs} run(s)"
        );
        let limit = stitch_limit(&r);
        for run in runs {
            assert!(
                longest(run) <= limit + 1e-6,
                "{kind:?}: {} > {limit}",
                longest(run)
            );
        }
        let tol = if kind == RegionType::ZigzagNetFill {
            r.pattern_size_mm().0 * 5.0 + 0.6
        } else if kind == RegionType::Spiral {
            // Stitches along a hole's own edge are chords: PE-DESIGN 11.50's reach 0.16 mm into
            // the hole (spiral-ring40-hole15).
            1.5
        } else {
            0.6
        };
        for run in runs {
            for w in run.windows(2) {
                for p in [w[0], w[1], w[0].lerp(w[1], 0.5)] {
                    assert!(
                        inside_or_on(p, &region_contours, tol),
                        "{kind:?}: {p:?} outside"
                    );
                }
            }
        }
        // The compiler sews it as one colour block without jumps after the first.
        let mut d = Design::new("t", DesignPage::default());
        d.add(DesignObject::shape(region_contours.clone(), Some(r), None));
        let p = Compiler::default().compile(&d);
        assert_eq!(p.count(Command::Jump), 1, "{kind:?}");
        if kind == RegionType::ConcentricCircle {
            assert!(
                p.count(Command::Trim) >= 1,
                "the trim before the cut circles"
            );
        }
    }
}

#[test]
fn region_types_without_travel_jump_between_parts() {
    let region_contours = blob_with_hole();
    for kind in [
        RegionType::NetFill,
        RegionType::Radial,
        RegionType::MotifFill,
    ] {
        let mut r = region(kind);
        r.running_stitch_path = None;
        let runs = crate::region::layers(&region_contours, &r, &[])
            .pop()
            .expect("top");
        assert!(runs.len() > 1, "{kind:?}");
    }
}

#[test]
fn every_pattern_of_every_patterned_type_sews() {
    let c = vec![circle()];
    let cases: Vec<(RegionType, Vec<String>)> = vec![
        (
            RegionType::ProgrammableFill,
            crate::patterns::library()
                .iter()
                .filter(|p| p.kind == PatternKind::Fill)
                .map(|p| p.id.clone())
                .collect(),
        ),
        (
            RegionType::MotifFill,
            crate::patterns::library()
                .iter()
                .filter(|p| p.kind == PatternKind::Motif)
                .map(|p| p.id.clone())
                .collect(),
        ),
        (
            RegionType::DecorativeFill,
            crate::patterns::library()
                .iter()
                .filter(|p| p.kind == PatternKind::Decorative)
                .map(|p| p.id.clone())
                .collect(),
        ),
        (
            RegionType::NetFill,
            crate::patterns::NET_PATTERNS
                .iter()
                .map(|n| n.id.to_string())
                .collect(),
        ),
        (
            RegionType::ZigzagNetFill,
            crate::patterns::NET_PATTERNS[..crate::patterns::ZIGZAG_NET_PATTERNS]
                .iter()
                .map(|n| n.id.to_string())
                .collect(),
        ),
    ];
    for (kind, ids) in cases {
        let mut outputs = Vec::new();
        for id in ids {
            let mut r = region(kind);
            r.pattern = Some(id.clone());
            let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
            assert!(
                runs.iter().map(Vec::len).sum::<usize>() > 50,
                "{kind:?} {id}"
            );
            assert!(
                runs.iter()
                    .all(|run| longest(run) <= stitch_limit(&r) + 1e-6),
                "{kind:?} {id}"
            );
            outputs.push(runs);
        }
        for i in 1..outputs.len() {
            assert_ne!(outputs[i], outputs[0], "{kind:?}: patterns differ");
        }
    }
}

/// #923: a programmable fill along a direction curve sews its rows as the curve's offsets and
/// tiles its pattern in the curve's own frame, so its needle points lie on rows a row spacing
/// apart all along the curve: at least 70 % of the needle points inside the square lie within a
/// tenth of a row spacing of one. With the chord warp the rows were shifted copies across the
/// chord and the points hardly clustered (R 0.07 on the S curve of
/// `examples/sewing-direction.json`'s orange square; 0.79 now). Its crossings stay no more than
/// the straight pattern's on the same square (2,094 against 2,210).
#[test]
fn programmable_fill_along_a_curve_keeps_to_the_curves_offsets() {
    use crate::geometry::Bezier;
    let square = Contour::rectangle(550.0, 550.0, 400.0, 380.0);
    let mut curve = Contour::new(vec![v2(520.0, 600.0), v2(980.0, 900.0)], false);
    curve.set_curve(
        0,
        Some(Bezier {
            c1: v2(700.0, 500.0),
            c2: v2(800.0, 1000.0),
        }),
    );
    let mut r = region(RegionType::ProgrammableFill);
    r.underlay = false;
    r.base_sewing = false;
    r.direction_curve = Some(curve.clone());
    let layers = crate::region::layers(std::slice::from_ref(&square), &r, &[]);
    let spacing = 10.0 / r.density;
    let (_, guide) =
        crate::direction::guided_lines(std::slice::from_ref(&square), &[], &curve, spacing)
            .expect("guide");
    // Needle points well inside the square (not row ends or travel along the outline).
    let points: Vec<Vec2> = layers
        .last()
        .expect("top")
        .iter()
        .flatten()
        .copied()
        .filter(|p| p.x > 600.0 && p.x < 900.0 && p.y > 600.0 && p.y < 880.0)
        .collect();
    assert!(points.len() > 500, "{} needle points", points.len());
    let on_row = points
        .iter()
        .filter(|&&p| {
            let d = guide.frame(p).1 / spacing;
            (d - d.round()).abs() <= 0.1
        })
        .count();
    let share = on_row as f64 / points.len() as f64;
    assert!(
        share >= 0.7,
        "{:.0} % of needle points on an offset row",
        share * 100.0
    );
}

#[test]
fn programmable_fill_puts_needle_points_on_the_pattern() {
    let c = vec![Contour::rectangle(0.0, 0.0, 400.0, 400.0)];
    let mut r = region(RegionType::ProgrammableFill);
    r.pattern = Some("diamonds".into());
    r.angle = 0.0;
    r.running_stitch_path = None;
    // The pattern and base sewing layers only, without under sewing.
    r.underlay = false;
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    // Diamond lines in an 8 mm tile: |x mod 80 - 40| + |y mod 80 - 40|·… = 40, i.e. points
    // where x' + y' = 40 within the tile. Count needle points within 0.3 mm of a line.
    let on_line = |p: &Vec2| {
        let (u, v) = ((p.x / 80.0).fract(), (p.y / 80.0).fract());
        let d = ((u - 0.5).abs() + (v - 0.5).abs() - 0.5).abs() * 80.0 / std::f64::consts::SQRT_2;
        d < 3.0
    };
    let pts: Vec<&Vec2> = runs.iter().flatten().collect();
    let share = pts.iter().filter(|p| on_line(p)).count() as f64 / pts.len() as f64;
    let mut plain = region(RegionType::Fill);
    plain.running_stitch_path = None;
    let fill = crate::region::layers(&c, &plain, &[]).pop().expect("fill");
    let fpts: Vec<&Vec2> = fill.iter().flatten().collect();
    let fill_share = fpts.iter().filter(|p| on_line(p)).count() as f64 / fpts.len() as f64;
    assert!(share > fill_share * 1.5, "{share} vs {fill_share}");
    r.base_sewing = true;
    assert_eq!(
        crate::region::layers(&c, &r, &[]).len(),
        2,
        "base sewing layer"
    );
}

#[test]
fn piping_rows_shift_by_the_frequency() {
    let c = vec![Contour::rectangle(0.0, 0.0, 300.0, 100.0)];
    let mut r = region(RegionType::Piping);
    r.frequency = Some(50.0);
    r.running_stitch_path = None;
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    let row = |y_lo: f64, y_hi: f64| -> Vec<f64> {
        let mut xs: Vec<f64> = runs
            .iter()
            .flatten()
            .filter(|p| p.y > y_lo && p.y < y_hi && p.x > 1.0 && p.x < 299.0)
            .map(|p| p.x)
            .collect();
        xs.sort_by(f64::total_cmp);
        xs
    };
    // Rows 1 mm apart, stitches 2 mm, shifted by half a stitch per row.
    let (a, b) = (row(0.0, 10.0), row(10.0, 20.0));
    assert!((a[0] - 20.0).abs() < 1e-6, "{a:?}");
    assert!((b[0] - 10.0).abs() < 1e-6, "{b:?}");
}

#[test]
fn cross_stitch_sews_an_x_in_every_cell() {
    let c = vec![Contour::rectangle(0.0, 0.0, 100.0, 100.0)];
    let mut r = region(RegionType::CrossStitch);
    r.running_stitch_path = None;
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    assert_eq!(runs.len(), 1, "rows joined along the crosses");
    let diagonals = |runs: &[Run]| {
        runs.iter()
            .flat_map(|r| r.windows(2))
            .filter(|w| (w[0].distance(w[1]) - 25.0 * std::f64::consts::SQRT_2).abs() < 1e-6)
            .count()
    };
    assert_eq!(diagonals(&runs), 16 * 2);
    // The old triple switch sews what Times Double sews: each diagonal whole three times.
    r.triple = true;
    let triple = crate::region::layers(&c, &r, &[]).pop().expect("top");
    assert_eq!(diagonals(&triple), 16 * 2 * 3);
    r.triple = false;
    for (times, whole) in [
        (crate::CrossTimes::Single, 1),
        (crate::CrossTimes::Double, 3),
        (crate::CrossTimes::Triple, 5),
    ] {
        r.times = times;
        let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
        assert_eq!(diagonals(&runs), 16 * 2 * whole, "{times:?}");
    }
    // A cell crossing the outline is skipped.
    let small = vec![Contour::rectangle(0.0, 0.0, 90.0, 90.0)];
    let partial = crate::region::layers(&small, &region(RegionType::CrossStitch), &[]);
    assert_eq!(diagonals(partial.last().expect("top")), 9 * 2);
}

/// The cross kinds beyond PE-DESIGN (#614) sew only their own strokes in every cell: a half
/// cross the "/" diagonal, an upright cross a "+" through the middle, a Smyrna cross both an X
/// and a "+"; each stroke covered 2·times times.
#[test]
fn cross_kinds_sew_their_own_strokes_in_every_cell() {
    use crate::{CrossKind, CrossTimes};
    let diagonal = 25.0 * std::f64::consts::SQRT_2;
    // Stitches of a square of side `side` as ("/" diagonals, "\" diagonals, cell-length strokes
    // through cell middles).
    let strokes = |side: f64, kind: CrossKind, times: CrossTimes| {
        let c = vec![Contour::rectangle(0.0, 0.0, side, side)];
        let mut r = region(RegionType::CrossStitch);
        r.running_stitch_path = None;
        r.cross_kind = kind;
        r.times = times;
        let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
        let (mut slash, mut back, mut plus) = (0, 0, 0);
        for w in runs.iter().flat_map(|r| r.windows(2)) {
            let d = w[1] - w[0];
            let mid = (w[0] + w[1]) * 0.5;
            let middle = |v: f64| ((v / 25.0).fract() - 0.5).abs() < 1e-6;
            if (d.length() - diagonal).abs() < 1e-6 {
                if d.x * d.y < 0.0 {
                    slash += 1;
                } else {
                    back += 1;
                }
            } else if (d.length() - 25.0).abs() < 1e-6
                && ((d.y == 0.0 && middle(mid.y)) || (d.x == 0.0 && middle(mid.x)))
            {
                plus += 1;
            }
        }
        (slash, back, plus)
    };
    use CrossKind::{Half, Smyrna, Upright};
    use CrossTimes::{Double, Single, Triple};
    assert_eq!(strokes(100.0, Half, Single), (32, 0, 0));
    assert_eq!(strokes(100.0, Half, Triple), (96, 0, 0));
    assert_eq!(strokes(100.0, Upright, Single), (0, 0, 64));
    assert_eq!(strokes(100.0, Upright, Double), (0, 0, 128));
    // Smyrna: the full cross (each diagonal whole once, plus its halves) and the "+".
    assert_eq!(strokes(100.0, Smyrna, Single), (16, 16, 64));
    // On a 90 mm square only the 3 × 3 whole cells are sewn.
    assert_eq!(strokes(90.0, Half, Single), (18, 0, 0));
    assert_eq!(strokes(90.0, Upright, Single), (0, 0, 36));
}

/// Cross stitch cells of `runs` on a `size` grid from the origin: per cell, how many of its
/// four half diagonals are sewn; also the stitches that are not on a half diagonal of the
/// grid (along a cell side, or off the grid).
fn cross_cells(runs: &[Run], size: f64) -> (std::collections::BTreeMap<(i64, i64), usize>, usize) {
    let h = size / 2.0;
    let on_lattice = |p: Vec2| (p.x - p.x.round()).abs() < 1e-6 && (p.y - p.y.round()).abs() < 1e-6;
    let mut halves = std::collections::BTreeSet::new();
    let mut off = 0;
    for w in runs.iter().flat_map(|r| r.windows(2)) {
        let (a, b) = (w[0] * (1.0 / h), w[1] * (1.0 / h));
        let d = b - a;
        if !on_lattice(a) || !on_lattice(b) || (d.x.abs() - d.y.abs()).abs() > 1e-6 || d.x == 0.0 {
            off += 1;
            continue;
        }
        let steps = d.x.abs().round() as i64;
        for k in 0..steps {
            let p = a + d * ((k as f64 + 0.5) / steps as f64);
            let (cx, cy) = ((p.x / 2.0).floor() as i64, (p.y / 2.0).floor() as i64);
            // Which half diagonal of the cell: the quadrant round its centre.
            let quadrant = (p.x > (2 * cx + 1) as f64, p.y > (2 * cy + 1) as f64);
            halves.insert((cx, cy, quadrant));
        }
    }
    let mut cells = std::collections::BTreeMap::new();
    for (cx, cy, _) in halves {
        *cells.entry((cx, cy)).or_insert(0) += 1;
    }
    (cells, off)
}

#[test]
fn cross_stitch_sews_whole_xs_with_travel_only_under_them_at_any_direction() {
    // A 20 × 20 mm square at (10, 10) mm: 8 × 8 cells of 2.5 mm (#463).
    let c = vec![Contour::rectangle(100.0, 100.0, 200.0, 200.0)];
    for angle in [0.0, 30.0, 45.0, 90.0] {
        let mut r = region(RegionType::CrossStitch);
        r.angle = angle;
        r.running_stitch_path = None;
        let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
        assert_eq!(runs.len(), 1, "{angle}°: one run, no trim");
        let (cells, off) = cross_cells(&runs, 25.0);
        // Nothing along a cell side or off the diagonals: travel lies under the crosses.
        assert_eq!(off, 0, "{angle}°");
        assert_eq!(cells.len(), 64, "{angle}°");
        assert!(cells.values().all(|&n| n == 4), "{angle}°: every X whole");
        assert!(
            cells
                .keys()
                .all(|&(x, y)| (4..12).contains(&x) && (4..12).contains(&y)),
            "{angle}°: inside the square"
        );
        assert!(longest(&runs[0]) <= 25.0 * std::f64::consts::SQRT_2 + 1e-6);
    }
    // A concave region, an L of six cells: still every X whole and nothing on a side.
    let l = vec![Contour::new(
        vec![
            v2(0.0, 0.0),
            v2(75.0, 0.0),
            v2(75.0, 25.0),
            v2(25.0, 25.0),
            v2(25.0, 100.0),
            v2(0.0, 100.0),
        ],
        true,
    )];
    let mut r = region(RegionType::CrossStitch);
    r.running_stitch_path = None;
    let runs = crate::region::layers(&l, &r, &[]).pop().expect("top");
    let (cells, off) = cross_cells(&runs, 25.0);
    assert_eq!((cells.len(), off, runs.len()), (6, 0, 1));
    assert!(cells.values().all(|&n| n == 4));
}

#[test]
fn zigzag_net_stays_inside_and_its_rungs_do_not_cross() {
    // #463: the honeycomb's bands fanned round the hexagon corners (rungs crossing their
    // neighbours) and their rungs reached about 1 mm past the outline.
    // Two stitches crossing more than 0.3 mm from all their ends (where the rungs of two bands
    // meet in a corner their ends touch, which is not a crossing).
    let proper_cross = |a: Vec2, b: Vec2, c: Vec2, d: Vec2| {
        let side =
            |p: Vec2, q: Vec2, r: Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
        let (d1, d2) = (side(a, b, c), side(a, b, d));
        let (d3, d4) = (side(c, d, a), side(c, d, b));
        if d1 * d2 >= -1e-6 || d3 * d4 >= -1e-6 {
            return false;
        }
        let x = c.lerp(d, d1 / (d1 - d2));
        [a, b, c, d].iter().all(|p| p.distance(x) > 3.0)
    };
    for (c, name) in [
        (
            vec![Contour::rectangle(100.0, 100.0, 200.0, 200.0)],
            "square",
        ),
        (vec![circle()], "circle"),
    ] {
        for id in ["honeycomb", "square", "diamond", "triangle"] {
            for angle in [0.0, 30.0] {
                let mut r = region(RegionType::ZigzagNetFill);
                r.pattern = Some(id.into());
                r.angle = angle;
                let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
                assert_eq!(runs.len(), 1, "{name} {id} {angle}°");
                // #616: the Direction does not turn the net (PE-DESIGN 11.50 sews it on the page
                // axes at Direction 45°).
                let mut at_zero = r.clone();
                at_zero.angle = 0.0;
                assert_eq!(
                    runs,
                    crate::region::layers(&c, &at_zero, &[]).pop().expect("top"),
                    "{name} {id} {angle}°: the same net as at 0°"
                );
                let run = &runs[0];
                for p in run {
                    assert!(
                        inside_or_on(*p, &c, 0.5),
                        "{name} {id} {angle}°: {p:?} outside"
                    );
                }
                // Stitches along a net line (travel, and the run under a band, which every rung
                // crosses) lie in a line direction; rungs and their diagonals do not.
                let lines: &[f64] = match id {
                    "square" => &[0.0, 90.0],
                    "diamond" => &[45.0, -45.0],
                    _ => &[0.0, 60.0, 120.0],
                };
                let along_line = |j: usize| {
                    let d = run[j + 1] - run[j];
                    let a = d.y.atan2(d.x).to_degrees();
                    // The net lies on the page axes whatever the Direction (#616), and turns the
                    // other way round on the page (y down).
                    lines.iter().any(|l| {
                        let off = (a + l).rem_euclid(180.0);
                        off.min(180.0 - off) < 2.0
                    })
                };
                let crosses = |i: usize| {
                    !along_line(i)
                        && !along_line(i + 2)
                        && proper_cross(run[i], run[i + 1], run[i + 2], run[i + 3])
                };
                let crossing = (0..run.len().saturating_sub(3))
                    .filter(|&i| crosses(i))
                    .count();
                let first = (0..run.len().saturating_sub(3))
                    .find(|&i| crosses(i))
                    .map(|i| (i, &run[i.saturating_sub(4)..(i + 6).min(run.len())]));
                assert_eq!(
                    crossing, 0,
                    "{name} {id} {angle}°: a rung crosses the next {first:?}"
                );
            }
        }
    }
}

#[test]
fn neighbouring_cross_stitch_regions_share_one_grid() {
    // Two squares side by side at an offset that is no multiple of the cell: every needle
    // point of both lies on the same 2.5 mm grid, at a cell corner or a cell centre.
    let half = 12.5;
    let left = vec![Contour::rectangle(33.0, 17.0, 120.0, 120.0)];
    let right = vec![Contour::rectangle(153.0, 17.0, 120.0, 120.0)];
    for angle in [0.0, 45.0] {
        let mut r = region(RegionType::CrossStitch);
        r.angle = angle;
        r.running_stitch_path = None;
        for c in [&left, &right] {
            let runs = crate::region::layers(c, &r, &[]).pop().expect("top");
            assert!(runs.iter().map(Vec::len).sum::<usize>() > 50);
            for p in runs.iter().flatten() {
                let (u, v) = (p.x / half, p.y / half);
                assert!(
                    (u - u.round()).abs() < 1e-6 && (v - v.round()).abs() < 1e-6,
                    "{p:?} off the shared grid"
                );
                assert_eq!(
                    (u.round() as i64 % 2 == 0),
                    (v.round() as i64 % 2 == 0),
                    "{p:?}: a corner or a centre"
                );
            }
        }
    }
}

#[test]
fn circles_and_spirals_start_where_asked() {
    let c = vec![circle()];
    let centre = v2(200.0, 200.0);
    for kind in [
        RegionType::ConcentricCircle,
        RegionType::Spiral,
        RegionType::FlexibleSpiral,
    ] {
        let mut r = region(kind);
        let outside = crate::region::layers(&c, &r, &[]).pop().expect("top");
        r.start_inside = true;
        let inside = crate::region::layers(&c, &r, &[]).pop().expect("top");
        let first = |run: &Run| run[0].distance(centre);
        assert!(
            first(&outside[0]) > 120.0,
            "{kind:?}: {}",
            first(&outside[0])
        );
        // Concentric Circle's second Start Point button sews the same stitches backwards: the
        // cut circles first, then the whole circles from the middle out (PE-DESIGN 11.50,
        // concentric-circle40-start2).
        let middle_out = if kind == RegionType::ConcentricCircle {
            inside.last().expect("whole circles")
        } else {
            &inside[0]
        };
        assert!(first(middle_out) < 30.0, "{kind:?}: {}", first(middle_out));
    }
    // Concentric circles sit at multiples of the spacing (1 / 2.0 mm, PE-DESIGN's default),
    // needle points on the 0.1 mm grid.
    let rings = crate::region::layers(&c, &region(RegionType::ConcentricCircle), &[]);
    let spacing = 10.0 / 2.0;
    let on_ring = rings[0][0]
        .iter()
        .filter(|p| {
            let k = p.distance(centre) / spacing;
            (k - k.round()).abs() * spacing < 1.0
        })
        .count();
    assert!(on_ring as f64 > rings[0][0].len() as f64 * 0.9);
    // Spirals turn either way: with the outline, or against it.
    let mut with = region(RegionType::Spiral);
    with.start_inside = true;
    let mut against = with.clone();
    against.counterclockwise = true;
    let turn = |r: &RegionSewing| {
        let runs = crate::region::layers(&c, r, &[]).pop().expect("top");
        let p = &runs[0];
        let (a, b) = (p[3] - centre, p[4] - centre);
        a.x * b.y - a.y * b.x
    };
    assert!(turn(&with) * turn(&against) < 0.0);
}

#[test]
fn radial_lines_thin_out_towards_the_centre() {
    let c = vec![circle()];
    let runs = crate::region::layers(&c, &region(RegionType::Radial), &[]);
    let pts: Vec<&Vec2> = runs[0].iter().flatten().collect();
    let centre = v2(200.0, 200.0);
    let density = |r0: f64, r1: f64| {
        let n = pts
            .iter()
            .filter(|p| (r0..r1).contains(&p.distance(centre)))
            .count() as f64;
        n / (std::f64::consts::PI * (r1 * r1 - r0 * r0))
    };
    let (inner, outer) = (density(10.0, 40.0), density(110.0, 140.0));
    assert!(inner < outer * 4.0, "inner {inner} outer {outer}");
}

/// The sewn stretches of a compiled design: needle points between jumps and trims.
fn sewn_stretches(d: &Design) -> Vec<Vec<Vec2>> {
    let p = Compiler::default().compile(d);
    let mut out: Vec<Vec<Vec2>> = vec![Vec::new()];
    for s in &p.stitches {
        match s.command {
            Command::Stitch => out.last_mut().expect("one").push(v2(s.x, s.y)),
            _ => out.push(Vec::new()),
        }
    }
    out.retain(|s| s.len() >= 2);
    out
}

/// The longest stretch of three or more consecutive stitches of 1 mm or more (travel, not the
/// 0.5 mm steps along a seam) whose needle points all lie within `tol` of the straight line
/// between its ends.
fn longest_straight(points: &[Vec2], tol: f64) -> f64 {
    let mut best: f64 = 0.0;
    for i in 0..points.len() {
        let mut j = i + 1;
        while j + 1 < points.len() {
            let (a, b) = (points[i], points[j + 1]);
            let len = a.distance(b);
            let straight = len > 1e-9
                && points[j].distance(points[j + 1]) >= 10.0
                && points[i].distance(points[i + 1]) >= 10.0
                && points[i + 1..=j].iter().all(|p| {
                    ((b.x - a.x) * (a.y - p.y) - (a.x - p.x) * (b.y - a.y)).abs() / len <= tol
                });
            if !straight {
                break;
            }
            j += 1;
        }
        if j - i >= 3 {
            best = best.max(points[i].distance(points[j]));
        }
    }
    best
}

#[test]
fn circles_spirals_and_rings_never_run_straight_back_across_their_rows() {
    // A 30 mm circle with its start and end points at the bottom, as the shape tools set them.
    // The flexible spiral ended in the middle and travelled straight out to the end point
    // across its turns; concentric circles about a centre well off the middle stepped from the
    // outermost arc straight in to the whole circles, 16 mm across the arcs (#461).
    let c = circle();
    let bottom = v2(200.0, 350.0);
    let mut cases = Vec::new();
    for kind in [
        RegionType::FlexibleSpiral,
        RegionType::ConcentricCircle,
        RegionType::Spiral,
        RegionType::Contour,
    ] {
        cases.push((kind, ContourStrategy::Rings, None));
    }
    cases.push((RegionType::Contour, ContourStrategy::DoubleSpiral, None));
    cases.push((
        RegionType::ConcentricCircle,
        ContourStrategy::Rings,
        Some(v2(245.0, 155.0)),
    ));
    for (kind, strategy, centre) in cases {
        let mut r = region(kind);
        r.contour_strategy = strategy;
        r.center = centre;
        r.entry = Some(bottom);
        r.exit = Some(bottom);
        let mut d = Design::new("t", DesignPage::default());
        d.add(DesignObject::shape(vec![c.clone()], Some(r), None));
        let straight = sewn_stretches(&d)
            .iter()
            .map(|s| longest_straight(s, 2.0))
            .fold(0.0, f64::max);
        assert!(
            straight < 60.0,
            "{kind:?} {strategy:?} {centre:?}: a straight run of {:.1} mm",
            straight / 10.0
        );
    }
}

#[test]
fn radial_lines_keep_their_needle_points_apart_at_the_centre() {
    // The lines converged on the centre, needle points of neighbouring lines 0.1-0.25 mm apart
    // there (#461): within 3 mm of the centre no two needle points are closer than 0.4 mm,
    // unless they are the same point sewn over again. On centre and off it.
    let c = vec![circle()];
    for centre in [None, Some(v2(245.0, 155.0))] {
        let mut r = region(RegionType::Radial);
        r.center = centre;
        r.entry = Some(v2(200.0, 350.0));
        let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
        let mid = centre.unwrap_or(v2(200.0, 200.0));
        let mut pts: Vec<Vec2> = runs
            .iter()
            .flatten()
            .copied()
            .filter(|p| p.distance(mid) < 30.0)
            .collect();
        pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        pts.dedup_by(|a, b| a.distance(*b) < 1e-6);
        let mut close = 0;
        for i in 0..pts.len() {
            for j in i + 1..pts.len() {
                let d = pts[i].distance(pts[j]);
                if d < 3.9 {
                    close += 1;
                }
            }
        }
        assert_eq!(
            close,
            0,
            "{centre:?}: {close} pairs under 0.4 mm of {}",
            pts.len()
        );
        // It starts and ends on the outline, near the start point.
        let (first, last) = (
            runs[0][0],
            *runs.last().and_then(|r| r.last()).expect("points"),
        );
        for p in [first, last] {
            assert!(
                (p.distance(v2(200.0, 200.0)) - 150.0).abs() < 3.0,
                "{centre:?}: {p:?}"
            );
        }
        assert!(
            first.distance(v2(200.0, 350.0)) < 15.0,
            "{centre:?}: {first:?}"
        );
        // No stitch shorter than 0.3 mm at the outline.
        let short: Vec<(Vec2, Vec2)> = runs
            .iter()
            .flat_map(|run| run.windows(2).map(|w| (w[0], w[1])))
            .filter(|(a, b)| a.distance(*b) < 3.0)
            .collect();
        assert!(
            short.len() <= 2,
            "{centre:?}: stitches under 0.3 mm {short:?}"
        );
    }
}

#[test]
fn stippling_is_a_closed_path_that_never_crosses_itself() {
    let c = vec![Contour::ellipse(v2(150.0, 150.0), 120.0, 90.0, 48)];
    let mut r = region(RegionType::Stippling);
    r.running_stitch_path = None;
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    assert_eq!(runs.len(), 1, "one connected part, one loop");
    let run = &runs[0];
    assert!(
        run[0].distance(*run.last().expect("points")) < 1e-6,
        "closed"
    );
    let segs: Vec<(Vec2, Vec2)> = run.windows(2).map(|w| (w[0], w[1])).collect();
    let cross = |a: Vec2, b: Vec2, c: Vec2, d: Vec2| {
        let o = |p: Vec2, q: Vec2, r: Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
        o(a, b, c) * o(a, b, d) < -1e-9 && o(c, d, a) * o(c, d, b) < -1e-9
    };
    for i in 0..segs.len() {
        for j in i + 2..segs.len() {
            if i == 0 && j == segs.len() - 1 {
                continue;
            }
            assert!(
                !cross(segs[i].0, segs[i].1, segs[j].0, segs[j].1),
                "segments {i} and {j} cross"
            );
        }
    }
}

#[test]
fn flexible_spiral_never_stitches_in_place_at_its_centre() {
    // Its turns keep the same number of points while they shrink, so towards the centre the
    // points crowded under 0.3 mm apart (52 stitches in a row in the ready-to-sew kit).
    let c = vec![Contour::ellipse(v2(130.0, 130.0), 130.0, 130.0, 48)];
    let mut r = region(RegionType::FlexibleSpiral);
    r.running_stitch_path = None;
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    let short = runs
        .iter()
        .flat_map(|run| run.windows(2).map(|w| w[0].distance(w[1])))
        .filter(|&d| d < 3.0)
        .count();
    assert!(short <= 1, "{short} stitches under 0.3 mm");
}

#[test]
fn stippling_sews_at_the_run_pitch_not_at_its_curve_vertices() {
    // The meander's smoothing leaves vertices about 0.2 mm apart; sewing each one stitched in
    // place (800 stitches under 0.3 mm in the ready-to-sew kit). Stitches follow the Run Pitch,
    // at most half the spacing on the turns.
    let c = vec![Contour::ellipse(v2(150.0, 150.0), 120.0, 90.0, 48)];
    let mut r = region(RegionType::Stippling);
    r.running_stitch_path = None;
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    let lens: Vec<f64> = runs
        .iter()
        .flat_map(|run| run.windows(2).map(|w| w[0].distance(w[1])))
        .collect();
    let short = lens.iter().filter(|&&d| d < 3.0).count();
    assert!(short <= 2, "{short} stitches under 0.3 mm");
    let max = lens.iter().copied().fold(0.0, f64::max);
    assert!(
        max <= 10.5,
        "longest stitch {max} (0.1 mm), half the 2 mm spacing"
    );
}

/// #638: a region one grid cell wide (or high) gave a lattice of one column, where a step down
/// (`cell + cols`) was taken for a step right (`cell + 1`). The loop left the lattice, the loop
/// merge walked links that never closed and grew until a 32 GiB allocation aborted the process
/// (random design seed 993: stippling on a 24 mm "!"). Thin bars stipple into closed loops
/// that stay inside, quickly.
#[test]
fn stippling_one_cell_wide_or_high_closes_its_loops() {
    let started = std::time::Instant::now();
    // At 0° the lattice lies on the page axes; at the default spacing, bars 3.6-6.6 mm wide are
    // one column (or, lying down, one row) of cells. Before the fix they sewed two broken
    // loops of about 25 points over a 40 mm bar.
    for width in [36.0, 42.0, 48.0, 54.0, 60.0, 66.0] {
        for vertical in [true, false] {
            let (w, h) = if vertical {
                (width, 400.0)
            } else {
                (400.0, width)
            };
            let bar = vec![Contour::rectangle(100.0, 100.0, w, h)];
            let mut r = region(RegionType::Stippling);
            r.running_stitch_path = None;
            r.angle = 0.0;
            let runs = crate::region::layers(&bar, &r, &[]).pop().expect("top");
            assert_eq!(runs.len(), 1, "{w} x {h}: one loop");
            let pts = &runs[0];
            assert!(pts.len() < 5_000, "{w} x {h}: {} points", pts.len());
            for p in pts {
                assert!(
                    p.x > 99.0 && p.x < 101.0 + w && p.y > 99.0 && p.y < 101.0 + h,
                    "{w} x {h}: {p:?} outside the bar"
                );
            }
            let along = |p: &Vec2| if vertical { p.y } else { p.x };
            let lo = pts.iter().map(along).fold(f64::MAX, f64::min);
            let hi = pts.iter().map(along).fold(f64::MIN, f64::max);
            assert!(
                hi - lo > 0.9 * 400.0,
                "{w} x {h}: covers {lo:.0}-{hi:.0} of 100-500"
            );
        }
    }
    assert!(started.elapsed().as_secs() < 20, "{:?}", started.elapsed());
}

/// #615: a ring whose hole is a contour of the region itself (as `shape.combine` difference
/// leaves it) or a separate hole. Stippling in several pieces and the net, motif and decorative
/// fills join their pieces inside the band: no stitch of any layer crosses the hole. #708: at
/// 3 mm the 7.5 mm band is too narrow for the meander's cells, and it still gets one.
#[test]
fn fills_on_a_ring_never_cross_its_hole_and_a_narrow_band_still_stipples() {
    let c = v2(250.0, 250.0);
    let outer = Contour::ellipse(c, 125.0, 125.0, 96);
    let inner = Contour::ellipse(c, 50.0, 50.0, 48);
    let shapes = [
        ("inner contour", vec![outer.clone(), inner.clone()], vec![]),
        ("separate hole", vec![outer.clone()], vec![inner.clone()]),
    ];
    let mut cases: Vec<(RegionType, f64)> = [1.5, 2.0, 2.5, 3.0]
        .into_iter()
        .map(|s| (RegionType::Stippling, s))
        .collect();
    cases.extend(
        [
            RegionType::NetFill,
            RegionType::MotifFill,
            RegionType::DecorativeFill,
        ]
        .into_iter()
        .map(|k| (k, 3.0)),
    );
    for (name, closed, holes) in &shapes {
        for &(kind, spacing) in &cases {
            let mut r = region(kind);
            r.spacing = Some(spacing);
            let layers = crate::region::layers(closed, &r, holes);
            let stitches: Vec<&[Vec2]> = layers
                .iter()
                .flatten()
                .flat_map(|run| run.windows(2))
                .collect();
            assert!(
                !stitches.is_empty(),
                "{name} {kind:?} {spacing}: no stitches"
            );
            let across = stitches
                .iter()
                .filter(|w| seg_distance(c, w[0], w[1]) < 45.0)
                .count();
            assert_eq!(
                across, 0,
                "{name} {kind:?} {spacing}: stitches across the hole"
            );
        }
    }
}

/// Stippling of `closed` minus `holes` at `spacing` mm (no travel): the sewn runs.
fn stippled(closed: &[Contour], holes: &[Contour], spacing: Option<f64>) -> Vec<Vec<Vec2>> {
    let mut r = region(RegionType::Stippling);
    r.running_stitch_path = None;
    r.spacing = spacing;
    crate::region::layers(closed, &r, holes).pop().expect("top")
}

fn seg_distance(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let d = b - a;
    let len2 = d.x * d.x + d.y * d.y;
    let t = if len2 > 0.0 {
        (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + d * t)
}

/// Mean distance (0.1 mm) between neighbouring passes: from the middle of each stitch, square
/// to it on both sides, to the first other stitch hit within 2.5 spacings that lies more than
/// 1.2 spacings away along the path (the other leg of a turn counts).
fn mean_pass_spacing(runs: &[Vec<Vec2>], spacing: f64) -> f64 {
    let pts: Vec<Vec2> = runs.iter().flatten().copied().collect();
    let segs: Vec<(Vec2, Vec2)> = pts.windows(2).map(|w| (w[0], w[1])).collect();
    let mut along = vec![0.0];
    for (a, b) in &segs {
        along.push(along.last().expect("start") + a.distance(*b));
    }
    let mut found = Vec::new();
    for (i, &(a, b)) in segs.iter().enumerate() {
        let len = a.distance(b);
        if len < 1e-6 {
            continue;
        }
        let mid = a.lerp(b, 0.5);
        let n = v2(-(b.y - a.y) / len, (b.x - a.x) / len);
        for side in [1.0, -1.0] {
            let d = n * side;
            let hit = segs
                .iter()
                .enumerate()
                .filter(|&(j, _)| (along[j] - along[i]).abs() > 1.2 * spacing)
                .filter_map(|(_, &(c, e))| {
                    let ex = e - c;
                    let den = d.x * ex.y - d.y * ex.x;
                    if den.abs() < 1e-12 {
                        return None;
                    }
                    let w = c - mid;
                    let t = (w.x * ex.y - w.y * ex.x) / den;
                    let u = (w.x * d.y - w.y * d.x) / den;
                    (t > 0.0 && t <= 2.5 * spacing && (0.0..=1.0).contains(&u)).then_some(t)
                })
                .fold(None, |m: Option<f64>, t| Some(m.map_or(t, |m| m.min(t))));
            if let Some(t) = hit {
                found.push(t);
            }
        }
    }
    found.iter().sum::<f64>() / found.len().max(1) as f64
}

/// Distances (0.1 mm) from points every 0.5 mm along the outlines to the nearest stitch:
/// (largest, mean).
fn outline_gaps(runs: &[Vec<Vec2>], outlines: &[Contour]) -> (f64, f64) {
    let segs: Vec<(Vec2, Vec2)> = runs
        .iter()
        .flat_map(|r| r.windows(2).map(|w| (w[0], w[1])))
        .collect();
    let gaps: Vec<f64> = outlines
        .iter()
        .flat_map(|c| {
            c.edges().into_iter().flat_map(|(a, b)| {
                let n = (a.distance(b) / 5.0).ceil().max(1.0) as usize;
                (0..n).map(move |k| a.lerp(b, k as f64 / n as f64))
            })
        })
        .map(|p| {
            segs.iter()
                .map(|&(a, b)| seg_distance(p, a, b))
                .fold(f64::MAX, f64::min)
        })
        .collect();
    let max = gaps.iter().copied().fold(0.0, f64::max);
    (max, gaps.iter().sum::<f64>() / gaps.len().max(1) as f64)
}

fn stipple_crossings(runs: &[Vec<Vec2>]) -> usize {
    let segs: Vec<(Vec2, Vec2)> = runs
        .iter()
        .flat_map(|r| r.windows(2).map(|w| (w[0], w[1])))
        .collect();
    let o = |p: Vec2, q: Vec2, r: Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let mut n = 0;
    for i in 0..segs.len() {
        for j in i + 2..segs.len() {
            let ((a, b), (c, d)) = (segs[i], segs[j]);
            if o(a, b, c) * o(a, b, d) < -1e-9 && o(c, d, a) * o(c, d, b) < -1e-9 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn stippling_honours_its_spacing_and_fills_a_square_to_the_outline() {
    // #462: the meander kept 3-6 mm off the outline; spacing must hold on 2 and 4 mm alike.
    let c = vec![Contour::rectangle(0.0, 0.0, 500.0, 500.0)];
    for spacing in [2.0, 4.0] {
        let runs = stippled(&c, &[], Some(spacing));
        let s = spacing * 10.0;
        let mean = mean_pass_spacing(&runs, s);
        assert!(
            (mean - s).abs() <= 0.15 * s,
            "spacing {spacing}: mean pass spacing {mean}"
        );
        // Half a spacing to the outline on average; the most, between two turns that meet
        // the outline side by side, is under 0.9 spacing (it was 2.7: a 5.5 mm band).
        let (max, avg) = outline_gaps(&runs, &c);
        assert!(
            avg <= 0.55 * s && max <= 0.9 * s,
            "spacing {spacing}: gaps {max} {avg}"
        );
        assert_eq!(stipple_crossings(&runs), 0, "spacing {spacing}");
    }
}

#[test]
fn stippling_fills_a_ring_to_its_outline_and_its_hole() {
    let outer = Contour::ellipse(v2(250.0, 250.0), 250.0, 250.0, 96);
    let hole = Contour::ellipse(v2(250.0, 250.0), 100.0, 100.0, 48);
    let runs = stippled(
        std::slice::from_ref(&outer),
        std::slice::from_ref(&hole),
        None,
    );
    let mean = mean_pass_spacing(&runs, 20.0);
    assert!((mean - 20.0).abs() <= 3.0, "mean pass spacing {mean}");
    let (max, avg) = outline_gaps(&runs, &[outer, hole]);
    assert!(avg <= 11.0 && max <= 18.0, "gaps {max} {avg}");
    assert_eq!(stipple_crossings(&runs), 0);
}

#[test]
fn stippling_starts_and_ends_inside_and_never_runs_along_the_outline() {
    // The exposed L of #462 ran along the bottom and left outline.
    let c = vec![Contour::rectangle(0.0, 0.0, 260.0, 260.0)];
    let runs = stippled(&c, &[], None);
    let edges = c[0].edges();
    let near = |p: Vec2| edges.iter().any(|&(a, b)| seg_distance(p, a, b) < 4.0);
    let along: f64 = runs
        .iter()
        .flat_map(|r| r.windows(2))
        .filter(|w| near(w[0]) && near(w[1]))
        .map(|w| w[0].distance(w[1]))
        .sum();
    assert!(along < 1.0, "{along} (0.1 mm) along the outline");
    let first = runs[0][0];
    let last = *runs.last().and_then(|r| r.last()).expect("stitches");
    assert!(!near(first) && !near(last), "{first:?} {last:?}");
    // Repeatable.
    assert_eq!(runs, stippled(&c, &[], None));
}

#[test]
fn square_net_lines_are_straight_and_axis_aligned() {
    let c = vec![Contour::rectangle(0.0, 0.0, 200.0, 200.0)];
    let mut r = region(RegionType::NetFill);
    r.running_stitch_path = None;
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    for run in &runs {
        for w in run.windows(2) {
            let d = w[1] - w[0];
            assert!(d.x.abs() < 1e-6 || d.y.abs() < 1e-6, "{d:?}");
        }
    }
    // 4 mm spacing over 20 mm: lines at 0, 4, … in both directions.
    let xs: std::collections::BTreeSet<i64> = runs
        .iter()
        .flatten()
        .filter(|p| p.y > 5.0 && p.y < 195.0)
        .filter(|p| p.x > 1.0 && p.x < 199.0)
        .map(|p| p.x.round() as i64)
        .filter(|x| x % 40 == 0)
        .collect();
    assert_eq!(xs.into_iter().collect::<Vec<_>>(), vec![40, 80, 120, 160]);
}

/// #617: motif lines and motif fill sewed every vertex of the motif, many closer than 0.3 mm
/// (170 such stitches in the kit's wave row, 56 in line-types.json's heart row, a run of 10 in
/// region-types.json's heart fill). No motif stitch is shorter now, and the lines still start
/// and end at the path's ends.
#[test]
fn motifs_never_stitch_under_0_3_mm() {
    let short = |run: &[Vec2]| {
        run.windows(2)
            .filter(|w| w[0].distance(w[1]) < crate::generate::MOTIF_MIN_STITCH - 1e-9)
            .count()
    };
    let curve: Vec<Vec2> = (0..=60)
        .map(|k| {
            let t = k as f64 / 60.0;
            v2(t * 600.0, (t * std::f64::consts::TAU).sin() * 120.0)
        })
        .collect();
    for (id, width, height, gap) in [
        ("wave", 60.0, 50.0, 10.0),
        ("heart", 60.0, 50.0, 10.0),
        ("heart", 30.0, 25.0, 5.0),
    ] {
        let pattern = patterns::find(PatternKind::Motif, id).expect(id);
        for path in [hline(500.0), curve.clone()] {
            let m = MotifLine {
                pattern,
                width,
                height,
                gap,
                pitch: 25.0,
                arrange: Arrange::Normal,
            };
            let run = lines::motif(&path, false, &m);
            assert_eq!(short(&run), 0, "{id} {width}: motif line");
            let at = |a: Option<&Vec2>, b: Option<&Vec2>| {
                a.zip(b).is_some_and(|(a, b)| a.distance(*b) < 1e-6)
            };
            assert!(
                at(run.first(), path.first()),
                "{id}: starts at the path's start"
            );
            assert!(at(run.last(), path.last()), "{id}: ends at the path's end");
        }
    }
    let c = vec![Contour::rectangle(0.0, 0.0, 300.0, 200.0)];
    let mut r = region(RegionType::MotifFill);
    r.pattern = Some("heart".into());
    r.pattern_width = Some(3.0);
    r.pattern_height = Some(2.5);
    r.spacing = Some(0.5);
    r.running_stitch_path = None;
    for run in crate::region::layers(&c, &r, &[]).pop().expect("top") {
        assert_eq!(short(&run), 0, "heart motif fill");
    }
}

#[test]
fn motif_fill_repeats_the_motif_in_rows() {
    let c = vec![Contour::rectangle(0.0, 0.0, 300.0, 200.0)];
    let mut r = region(RegionType::MotifFill);
    r.pattern = Some("peak".into());
    r.running_stitch_path = None;
    let runs = crate::region::layers(&c, &r, &[]).pop().expect("top");
    let pts: Vec<&Vec2> = runs.iter().flatten().collect();
    // Rows every 4 mm (a 3 mm motif and 1 mm spacing), each base line half the motif below
    // its band's top (the bands starting at the region's top edge); peaks are 2.7 mm (0.9 of
    // the height) above the base line.
    let peaks = pts
        .iter()
        .filter(|p| ((p.y + 12.0) / 40.0).fract().abs() < 1e-6)
        .count();
    assert!(peaks >= 25, "{peaks}");
}

// ---------------------------------------------------------------- fill Stitch Type

/// A 40 mm fill square at 0°, 4.5 lines/mm, without under sewing, started at the middle of its
/// left side (the halves need a start point) and sewn with `stitch_type` (0.1 mm units).
fn square_run(stitch_type: FillStitchType) -> Run {
    let c = vec![Contour::rectangle(0.0, 0.0, 400.0, 400.0)];
    let mut r = region(RegionType::Fill);
    r.angle = 0.0;
    r.density = 4.5;
    r.underlay = false;
    r.stitch_type = stitch_type;
    r.entry = Some(v2(0.0, 200.0));
    crate::region::layers(&c, &r, &[])
        .pop()
        .expect("top")
        .concat()
}

/// Stitch Type 2's passes on [`square_run`]: for every pass from one side to the other, its
/// rise from start to end, its end's height and its first inner needle point's position
/// modulo the 4 mm stitch length.
fn square_passes() -> Vec<(f64, f64, f64)> {
    let run = square_run(FillStitchType::Two);
    let edge = |p: &Vec2| p.x < 0.5 || p.x > 399.5;
    let mut passes = vec![];
    let mut last: Option<usize> = None;
    for (i, p) in run.iter().enumerate().filter(|(_, p)| edge(p)) {
        if let Some(j) = last
            && i > j + 1
            && (run[j].x - p.x).abs() > 399.0
        {
            passes.push((p.y - run[j].y, p.y, run[j + 1].x.rem_euclid(40.0)));
        }
        last = Some(i);
    }
    passes
}

#[test]
fn stitch_type_2_passes_rise_one_row_from_edge_to_edge() {
    // PE-DESIGN 11.50 (round 5, tatami-40-st2-dir0/30/90): Stitch Type 2 sews each pass from
    // one row's edge to the next row's far edge, so it rises one row spacing; Stitch Type 1's
    // rows are flat (tatami-40-st1-dir*).
    let spacing = 10.0 / 4.5;
    let passes = square_passes();
    assert!(passes.len() > 170, "{}", passes.len());
    for &(rise, y, _) in &passes {
        assert!((rise.abs() - spacing).abs() < 0.01, "{rise} at {y}");
    }
    // Both halves: upward from the start point, then downward from the far side.
    assert!(passes.iter().any(|p| p.0 > 0.0) && passes.iter().any(|p| p.0 < 0.0));
    // Every whole 4 mm stitch along the rows climbs a tenth of a spacing in Type 2 (within
    // 0.0002 mm: the first and last rows lie 0.0001 mm inside the outline), none in Type 1.
    let climbs = |t: FillStitchType| -> Vec<f64> {
        square_run(t)
            .windows(2)
            .filter(|w| (w[1].x - w[0].x).abs() > 39.9 && (w[1].x - w[0].x).abs() < 40.1)
            .map(|w| (w[1].y - w[0].y).abs())
            .collect()
    };
    let sloped = climbs(FillStitchType::Two);
    assert!(sloped.len() > 1500, "{}", sloped.len());
    let off: Vec<_> = sloped
        .iter()
        .filter(|dy| (*dy - spacing / 10.0).abs() >= 2e-3)
        .collect();
    assert!(off.is_empty(), "{off:?} of {}", sloped.len());
    let flat = climbs(FillStitchType::One);
    assert!(flat.len() > 1500, "{}", flat.len());
    assert!(flat.iter().all(|&dy| dy < 1e-9), "{flat:?}");
}

#[test]
fn stitch_type_2_needle_grid_follows_the_row_a_pass_ends_on() {
    // The Frequency grid of a pass ending on row k is k · 30 % · 4 mm along the rows, in the
    // half sewn upward and in the half sewn downward alike (tatami-40-st2-dir90: the downward
    // half's first pass, from the top row, has the grid of the row two below it).
    let spacing = 10.0 / 4.5;
    let passes = square_passes();
    let y0 = passes.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let offsets: Vec<f64> = passes
        .iter()
        .map(|&(_, y, x)| {
            let k = ((y - y0) / spacing).round();
            (x - 12.0 * k).rem_euclid(40.0)
        })
        .collect();
    for o in &offsets {
        let d = (o - offsets[0]).rem_euclid(40.0);
        assert!(!(1e-6..=40.0 - 1e-6).contains(&d), "{offsets:?}");
    }
}

#[test]
fn stitch_type_is_saved_only_when_it_is_type_2() {
    let mut r = region(RegionType::Fill);
    let json = serde_json::to_string(&r).expect("json");
    assert!(!json.contains("stitchType"), "{json}");
    r.stitch_type = FillStitchType::Two;
    let json = serde_json::to_string(&r).expect("json");
    assert!(json.contains(r#""stitchType":"two""#), "{json}");
    let back: RegionSewing = serde_json::from_str(&json).expect("parse");
    assert_eq!(back.stitch_type, FillStitchType::Two);
}

#[test]
fn auto_satin_on_a_straight_bar_travels_along_its_outline() {
    // PE-DESIGN 11.50's Auto Punch I of g08 (a 4.1 × 14.6 mm bar): from its start at the top it
    // runs 0.5 mm inside the left edge to the far end and along that end to the first row, under
    // the rows' ends. OpenStitch travelled the shortest way inside, diagonally through the
    // column, and the travel showed between the rows (#450 follow-up).
    let bar = Contour::new(
        vec![v2(0.0, 0.0), v2(41.0, 0.0), v2(41.0, 146.0), v2(0.0, 146.0)],
        true,
    );
    let mut r = region(RegionType::Satin);
    r.auto_direction = true;
    r.underlay = false;
    let run = crate::region::layers(std::slice::from_ref(&bar), &r, &[])
        .concat()
        .concat();
    assert!(run.len() > 60, "{}", run.len());
    let from_edge = |p: Vec2| p.x.min(41.0 - p.x).min(p.y).min(146.0 - p.y);
    let deepest = run.iter().map(|&p| from_edge(p)).fold(0.0, f64::max);
    assert!(
        deepest <= 8.0,
        "a needle point {deepest} (0.1 mm) inside the bar"
    );
}

#[test]
fn narrow_auto_satin_travels_to_its_far_end_without_a_long_stitch() {
    // #460: the kit's 1 mm × 40 mm Auto satin column (calibration/satin-and-underlay) started
    // with one 39.5 mm stitch from its start point (the middle of the bottom edge) to the far
    // end: the travel along the inset outline found no path in a column narrower than twice
    // the inset, and the fallback was a single straight stitch.
    for width in [10.0, 15.0, 20.0, 60.0] {
        let col = Contour::rectangle(90.0, 90.0, width, 400.0);
        let mut r = region(RegionType::Satin);
        r.auto_direction = true;
        r.underlay = false;
        r.entry = Some(v2(90.0 + width / 2.0, 490.0));
        r.exit = r.entry;
        r.running_stitch_path = Some(RunningStitchPath::Inside);
        // Travel stitches up to 2 mm; crossings as wide as the column (6 mm: 6.04 mm).
        let limit = f64::max(25.0, width + 2.0);
        let layers = crate::region::layers(std::slice::from_ref(&col), &r, &[]);
        for run in layers.iter().flatten() {
            let l = longest(run);
            assert!(l <= limit, "{width}: a {l} stitch in {run:?}");
        }
        // As the design sews it (travel from the start point, tie-in and lock).
        let mut d = Design::new("column", DesignPage::default());
        d.add(DesignObject::shape(vec![col], Some(r), None));
        let p = Compiler::default().compile(&d);
        let pts: Vec<Vec2> = p
            .stitches
            .iter()
            .filter(|s| s.command == Command::Stitch)
            .map(|s| v2(s.x, s.y))
            .collect();
        let l = longest(&pts);
        assert!(
            l <= limit,
            "{width}: a {l} stitch in {:?}",
            &pts[..12.min(pts.len())]
        );
    }
}

/// An S-curve about 60 mm long, one Bézier segment from (0, 0) to (50, 0) mm: flattened, its
/// chords are short at the bends and long between them (#464).
fn curve60() -> (Vec<Vec2>, Vec<usize>) {
    let mut c = Contour::new(vec![v2(0.0, 0.0), v2(500.0, 0.0)], false);
    c.set_curve(
        0,
        Some(crate::geometry::Bezier {
            c1: v2(150.0, -220.0),
            c2: v2(350.0, 220.0),
        }),
    );
    (c.path(), lines::nodes_of(&c))
}

/// Mean, standard deviation and max/min ratio of the gaps between consecutive arcs.
fn spacing_stats(arcs: &[f64]) -> (f64, f64, f64) {
    let d: Vec<f64> = arcs.windows(2).map(|w| w[1] - w[0]).collect();
    let mean = d.iter().sum::<f64>() / d.len() as f64;
    let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / d.len() as f64).sqrt();
    let (lo, hi) = d
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    (mean, sd, hi / lo)
}

#[test]
fn stem_on_a_curve_is_an_even_overlapping_rope() {
    let (path, nodes) = curve60();
    let len = crate::paths::length(&path);
    assert!((550.0..650.0).contains(&len), "{len}");
    let (width, spacing) = (25.0, 15.0);
    let line = lines::LoopLine::Stem {
        width,
        spacing,
        angle: 45.0,
        single: false,
    };
    let stations = lines::elements(&path, &nodes, line);
    // One leg: no corner points, a station every len / round(len / spacing).
    assert_eq!(stations.len(), (len / spacing).round() as usize + 1);
    let centres: Vec<f64> = stations
        .iter()
        .map(|(lead, _)| lines::arc_of(&path, lead[0]))
        .collect();
    let (mean, sd, ratio) = spacing_stats(&centres);
    assert!((mean - spacing).abs() <= spacing * 0.1, "mean {mean}");
    assert!(sd < 0.5 && ratio < 1.05, "sd {sd}, max/min {ratio}");
    // Each slanted stitch, seen along the line, overlaps the one before by at least a third of
    // its own extent: no gap anywhere along the rope.
    let spans: Vec<(f64, f64)> = stations
        .iter()
        .map(|(_, pts)| {
            let (a, b) = (lines::arc_of(&path, pts[1]), lines::arc_of(&path, pts[0]));
            (a.min(b), a.max(b))
        })
        .collect();
    for (k, w) in spans.windows(2).enumerate().skip(1) {
        let (prev, cur) = (w[0], w[1]);
        let overlap = prev.1 - cur.0;
        assert!(
            overlap >= (cur.1 - cur.0) / 3.0,
            "station {k}: overlap {overlap} of {:?}",
            cur
        );
    }
}

#[test]
fn candlewick_knots_and_chain_links_are_spaced_by_arc_length_on_a_curve() {
    let (path, nodes) = curve60();
    let len = crate::paths::length(&path);
    // Knots of 5 mm, 1 mm apart (PE-DESIGN's defaults): centres on the line, evenly spaced.
    let knots = lines::elements(
        &path,
        &nodes,
        lines::LoopLine::Candlewick {
            size: 50.0,
            points: 24,
            spacing: 10.0,
            pitch: 25.0,
        },
    );
    assert_eq!(knots.len(), (len / 60.0).round() as usize + 1);
    let centres: Vec<f64> = knots
        .iter()
        .map(|(_, pts)| {
            let c = pts[..24].iter().fold(v2(0.0, 0.0), |s, &p| s + p) * (1.0 / 24.0);
            assert!(path_distance(&path, c) < 0.5, "knot centre off the line");
            lines::arc_of(&path, c)
        })
        .collect();
    let (mean, sd, ratio) = spacing_stats(&centres);
    assert!((mean - 60.0).abs() <= 6.0, "mean {mean}");
    assert!(sd < 0.5 && ratio < 1.05, "sd {sd}, max/min {ratio}");
    // The running stitch between knots keeps no flattening vertex: no stitch under 0.3 mm.
    let run = lines::candlewick(&path, &nodes, 50.0, 24, 10.0, 25.0);
    assert!(
        run.windows(2)
            .all(|w| w[0].distance(w[1]) >= 3.0 - 1e-9 || w[0] == w[1])
    );
    // Triangle links of 4 mm: evenly spaced, each the same size with its base on the line.
    let links = lines::elements(
        &path,
        &nodes,
        lines::LoopLine::Chain {
            size: 40.0,
            times: 5,
            arrange: Arrange::Normal,
        },
    );
    assert_eq!(links.len(), (len / 30.0).floor() as usize);
    let points: Vec<f64> = links
        .iter()
        .map(|(lead, _)| lines::arc_of(&path, lead[0]))
        .collect();
    let (mean, sd, ratio) = spacing_stats(&points);
    assert!((mean - 30.0).abs() <= 3.0 * 1.34, "mean {mean}");
    assert!(sd < 0.5 && ratio < 1.05, "sd {sd}, max/min {ratio}");
    for (lead, pts) in &links[..links.len() - 1] {
        let base = (pts[0] + pts[1]) * 0.5;
        assert!((lead[0].distance(base) - 40.0).abs() < 1e-6);
        assert!(path_distance(&path, base) < 2.0, "base off the line");
    }
}

/// Distance from `p` to the polyline `path`.
fn path_distance(path: &[Vec2], p: Vec2) -> f64 {
    let s = lines::arc_of(path, p);
    crate::paths::Walker::new(path).at(s).distance(p)
}

#[test]
fn running_stitch_keeps_ornaments_smaller_than_a_stitch() {
    // A 2 mm scallop (a half circle of radius 1 mm as one Bézier) under a 2 mm pitch: sewn as
    // its 2 mm chord it was a straight edge; now it follows the arc.
    let k = 0.552_284_75 * 10.0;
    let mut c = Contour::new(vec![v2(0.0, 0.0), v2(20.0, 0.0)], false);
    c.set_curve(
        0,
        Some(crate::geometry::Bezier {
            c1: v2(0.0, -k * 1.33),
            c2: v2(20.0, -k * 1.33),
        }),
    );
    let run = crate::generate::running(&c, 20.0, 1);
    assert!(run.len() >= 3, "{run:?}");
    let top = run.iter().map(|p| -p.y).fold(0.0, f64::max);
    assert!(top > 5.0, "the scallop keeps its height: {top}");
    assert!(run.windows(2).all(|w| w[0].distance(w[1]) >= 3.0));
}

// ---------------------------------------------------------------- #466 cusps and apexes

fn distance_to_path(p: Vec2, path: &[Vec2]) -> f64 {
    path.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let len2 = ab.x * ab.x + ab.y * ab.y;
            let t = if len2 > 0.0 {
                (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            a.lerp(b, t).distance(p)
        })
        .fold(f64::INFINITY, f64::min)
}

fn heart_40mm() -> Contour {
    crate::shapes::fit(&crate::shapes::heart(), v2(500.0, 500.0), 400.0, 400.0, 0.0)
}

#[test]
fn a_zigzag_round_a_hearts_cusp_does_not_fold_over_the_line() {
    // #466: the appliqué cover (zigzag 3 mm, 5 lines/mm) round a heart knotted at its top
    // cusp: needle points offset to the inner side landed on or past the line's other side.
    let heart = heart_40mm();
    let path = heart.path();
    let folded = |run: &[Vec2]| {
        run.iter()
            .filter(|&&p| {
                let d = distance_to_path(p, &path);
                d > 1e-6 && d < 0.5 * 15.0
            })
            .count()
    };
    let plain = crate::generate::zigzag(&heart, 30.0, 2.0);
    let mut fixed = plain.clone();
    lines::unfold_zigzag(&path, &mut fixed, 30.0);
    assert!(folded(&plain) > 0, "the plain zigzag folds at the cusp");
    assert_eq!(folded(&fixed), 0);
    // Only the folded points move: elsewhere the zigzag is unchanged.
    let moved = plain.iter().zip(&fixed).filter(|(a, b)| a != b).count();
    assert!(moved <= folded(&plain), "{moved}");
    // Compiled as a line: the same.
    let mut line = LineSewing::new(LineType::Zigzag, RED);
    line.width = 3.0;
    line.density = 5.0;
    // The top stitching alone (the under sewing runs along the line, within the column).
    line.underlay = false;
    let mut d = Design::new("heart", DesignPage::default());
    d.add(DesignObject::shape(vec![heart.clone()], None, Some(line)));
    let p = Compiler::default().compile(&d);
    let pts: Vec<Vec2> = p
        .stitches
        .iter()
        .filter(|s| s.command == Command::Stitch)
        .map(|s| v2(s.x, s.y))
        .collect();
    // Needle points rounded to the 0.1 mm grid stay within 0.2 mm of the line or at least
    // 0.5 mm from it; the tie-in and lock (0.3 mm steps along a crossing) are left out.
    let bad: Vec<_> = pts[4..pts.len() - 4]
        .iter()
        .map(|&q| distance_to_path(q, &path))
        .filter(|&d| d > 2.0 && d < 5.0)
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn zigzag_under_sewing_bevels_a_cusp_instead_of_spiking() {
    // #466: the under sewing's mitre at a cusp had no limit (1 / (1 + cos) of the offset).
    let heart = heart_40mm();
    let path = heart.path();
    let offset = 30.0 / 4.0;
    let run = lines::zigzag_underlay(&path, offset, 25.0);
    let far = run
        .iter()
        .map(|&p| distance_to_path(p, &path))
        .fold(0.0, f64::max);
    assert!(far <= 2.0 * offset + 0.5, "{far}");
}

#[test]
fn a_skeleton_satin_apex_does_not_pile_its_inner_needle_points() {
    // #466: shape.skeletonSatin [[8,82],[20,70],[32,82]], widths [1.5, 4, 1.5]: every rung's
    // inner end met at the apex, needle points piled on one spot (kit shapes/shape-tools).
    let s = crate::skeleton::Skeleton {
        points: vec![v2(80.0, 820.0), v2(200.0, 700.0), v2(320.0, 820.0)],
        widths: vec![1.5, 4.0, 1.5],
    };
    let run = crate::skeleton::satin(&s, 10.0 / 4.5, 0.0);
    // Needle points within 0.2 mm of another one sewn two before (the same side).
    let piled = |run: &[Vec2]| run.windows(3).filter(|w| w[0].distance(w[2]) < 2.0).count();
    assert!(piled(&run) <= 2, "{} piled: {run:?}", piled(&run));
}

#[test]
fn tatami_textures_shift_the_needle_grid_by_steps_and_curves() {
    use crate::generate::{Stagger, StaggerCurve};
    // Steps: the shift sequence repeats with period n × repeat, each offset held repeat rows.
    let steps = Stagger::Steps { n: 3, repeat: 2 };
    let seq: Vec<f64> = (0..12).map(|k| steps.phase(k)).collect();
    let (a, b, c) = (1.0 / 6.0, 0.5, 5.0 / 6.0);
    let want = [a, a, b, b, c, c];
    for (k, s) in seq.iter().enumerate() {
        assert!((s - want[k % 6]).abs() < 1e-9, "row {k}: {seq:?}");
        assert_eq!(steps.anchored(k), Some(*s));
    }
    // Curves: the shift follows the curve within 0.05 mm on a 4 mm stitch.
    for (kind, mid) in [
        (StaggerCurve::Wave, 1.0),
        (StaggerCurve::Triangle, 1.0),
        (StaggerCurve::Arc, 1.0),
    ] {
        let c = Stagger::Curve {
            kind,
            amplitude: 0.5,
            period: 8,
        };
        assert!(c.phase(0).abs() * 4.0 < 0.05, "{kind:?} starts at 0");
        assert!(
            ((c.phase(4) - 0.5 * mid) * 4.0).abs() < 0.05,
            "{kind:?} peaks"
        );
        for k in 0..32 {
            let want = 0.5 * kind.at(k as f64 / 8.0);
            assert!(((c.phase(k) - want) * 4.0).abs() < 0.05, "{kind:?} row {k}");
        }
    }
    assert!((StaggerCurve::Triangle.at(0.25) - 0.5).abs() < 1e-9);
    assert!((StaggerCurve::Wave.at(0.25) - 0.5).abs() < 1e-9);
    assert!((StaggerCurve::Arc.at(0.999)).abs() < 0.01);
}

#[test]
fn named_tatami_textures_keep_the_stitch_count_and_stay_off_by_default() {
    let c = vec![Contour::rectangle(100.0, 100.0, 600.0, 600.0)];
    let mut r = region(RegionType::Fill);
    r.angle = 0.0;
    r.underlay = false;
    assert_eq!(
        r.texture().map(|t| t.id),
        Some("smooth"),
        "PE's brick by default"
    );
    let plain = crate::region::layers(&c, &r, &[]);
    let count = |l: &[Vec<Run>]| l.iter().flatten().map(Vec::len).sum::<usize>() as f64;
    let base = count(&plain);
    assert!(crate::FILL_TEXTURES.len() >= 12);
    let mut looks = std::collections::HashSet::new();
    for t in &crate::FILL_TEXTURES {
        let mut r = r.clone();
        r.set_texture(t);
        assert_eq!(r.texture().map(|x| x.id), Some(t.id), "{} round trip", t.id);
        let layers = crate::region::layers(&c, &r, &[]);
        let n = count(&layers);
        assert!(
            (n - base).abs() / base <= 0.03,
            "{}: {n} stitches against plain {base}",
            t.id
        );
        for l in &layers {
            assert_inside(l, &c, t.id);
        }
        looks.insert(format!("{layers:?}"));
        // Saved and loaded, the texture stays.
        let json = serde_json::to_string(&r).expect("save");
        let back: RegionSewing = serde_json::from_str(&json).expect("load");
        assert_eq!(back.texture().map(|x| x.id), Some(t.id));
    }
    assert_eq!(
        looks.len(),
        crate::FILL_TEXTURES.len(),
        "every texture differs"
    );
    // Smooth is the untouched Frequency brick: nothing new is saved.
    let mut smooth = r.clone();
    smooth.set_texture(&crate::FILL_TEXTURES[0]);
    assert_eq!(
        serde_json::to_value(&smooth).expect("save"),
        serde_json::to_value(&r).expect("save")
    );
}

/// Wave fill (#538): rows on `layer` in the row frame of stored angle `angle`, as (x, the row's
/// offset from the sine `a·sin(kx)`), away from the shape's left and right ends.
fn wave_rows(layer: &[Run], angle: f64, a: f64, k: f64, x: (f64, f64)) -> Vec<(f64, f64)> {
    let rad = angle.to_radians();
    layer
        .iter()
        .flatten()
        .map(|p| p.rotated(rad))
        .filter(|q| q.x > x.0 && q.x < x.1)
        .map(|q| (q.x, q.y - a * (k * q.x).sin()))
        .collect()
}

#[test]
fn wave_fill_rows_are_one_sine_a_row_spacing_apart() {
    let c = vec![Contour::rectangle(100.0, 100.0, 700.0, 500.0)];
    for angle in [0.0, 30.0] {
        let mut r = region(RegionType::Fill);
        r.angle = angle;
        r.underlay = false;
        r.wave_height = Some(4.0);
        r.wave_length = Some(20.0);
        let (a, l) = r.wave().expect("a wave");
        assert_eq!((a, l), (2.0, 20.0));
        let (a, k) = (a * 10.0, std::f64::consts::TAU / (l * 10.0));
        let layers = crate::region::layers(&c, &r, &[]);
        let top = layers.last().expect("top stitching");
        // Inside the shape, well away from where the rows end and turn.
        let pts: Vec<(f64, f64)> = if angle == 0.0 {
            wave_rows(top, angle, a, k, (140.0, 660.0))
                .into_iter()
                .collect()
        } else {
            // The rotated square's middle: the frame's middle third.
            let rad = angle.to_radians();
            let mid = v2(400.0, 300.0).rotated(rad);
            wave_rows(top, angle, a, k, (mid.x - 100.0, mid.x + 100.0))
                .into_iter()
                .filter(|&(_, y)| (y - mid.y).abs() < 80.0)
                .collect()
        };
        assert!(pts.len() > 100, "{angle}°: {} points", pts.len());
        // The needle points lie on copies of the set sine: their offsets from it gather in
        // tight rows (under 0.06 mm across) a step apart (within 3 %: the fill spreads its rows
        // evenly over the shape); the travel between rows and across the shape may not. A wrong
        // height or length would smear the offsets over millimetres.
        let slope = a * k;
        let step = 10.0 / r.density * 2.0 / (1.0 + 1.0 / (1.0 + slope * slope).sqrt());
        let mut ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
        ys.sort_by(f64::total_cmp);
        let mut rows: Vec<Vec<f64>> = vec![vec![ys[0]]];
        for &y in &ys[1..] {
            let row = rows.last_mut().expect("row");
            if y - row[row.len() - 1] < 0.3 {
                row.push(y);
            } else {
                rows.push(vec![y]);
            }
        }
        let tight: Vec<&Vec<f64>> = rows
            .iter()
            .filter(|r| r.len() >= 8 && r[r.len() - 1] - r[0] < 0.6)
            .collect();
        let on: usize = tight.iter().map(|r| r.len()).sum();
        assert!(
            on * 10 >= pts.len() * 9,
            "{angle}°: {on} of {} points on tight rows",
            pts.len()
        );
        let mean = |r: &Vec<f64>| r.iter().sum::<f64>() / r.len() as f64;
        let gaps = (mean(tight[tight.len() - 1]) - mean(tight[0])) / (tight.len() - 1) as f64;
        assert!(
            (gaps / step - 1.0).abs() < 0.03,
            "{angle}°: rows {gaps:.3} apart, a step is {step:.3}"
        );
        let y0 = mean(tight[tight.len() / 2]);
        // The wave's height measured on the stitches: crest to trough within 0.2 mm.
        let row: Vec<f64> = top
            .iter()
            .flatten()
            .map(|p| p.rotated(angle.to_radians()))
            .filter(|q| ((q.y - a * (k * q.x).sin()) - y0).abs() < 1.0)
            .map(|q| q.y)
            .collect();
        let height = row.iter().copied().fold(f64::MIN, f64::max)
            - row.iter().copied().fold(f64::MAX, f64::min);
        assert!((height - 2.0 * a).abs() <= 2.0, "{angle}°: height {height}");
    }
}

#[test]
fn wave_fill_keeps_the_spacing_across_steep_rows_and_height_zero_is_a_plain_fill() {
    let c = vec![Contour::rectangle(100.0, 100.0, 500.0, 400.0)];
    let mut r = region(RegionType::Fill);
    r.angle = 0.0;
    r.underlay = false;
    // Far too steep: sewn at the 40° limit.
    r.wave_height = Some(20.0);
    r.wave_length = Some(10.0);
    let (a, l) = r.wave().expect("a wave");
    assert!((a * std::f64::consts::TAU / l - RegionSewing::MAX_WAVE_SLOPE).abs() < 1e-9);
    let (a, k) = (a * 10.0, std::f64::consts::TAU / (l * 10.0));
    let spacing = 10.0 / r.density;
    let step = spacing * 2.0 / (1.0 + 1.0 / (1.0 + (a * k).powi(2)).sqrt());
    let layers = crate::region::layers(&c, &r, &[]);
    let pts = wave_rows(layers.last().expect("top"), 0.0, a, k, (150.0, 450.0));
    assert!(pts.len() > 100);
    // Distance from each needle point to the next row's sine, across the rows.
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for &(x, y) in pts.iter().filter(|p| p.1 < 350.0) {
        let next = y + step;
        let d = (-400..=400)
            .map(|i| {
                let u = x + i as f64 * 0.05;
                (u - x).hypot(next + a * (k * u).sin() - (y + a * (k * x).sin()))
            })
            .fold(f64::MAX, f64::min);
        lo = lo.min(d / spacing);
        hi = hi.max(d / spacing);
    }
    assert!(
        lo >= 0.85 && hi <= 1.15,
        "spacing across the rows {lo:.3}–{hi:.3}"
    );
    // No stitch under 0.3 mm, travel along the edges included (sew_check's tiny runs).
    for run in layers.iter().flatten() {
        for w in run.windows(2) {
            assert!(
                w[0].distance(w[1]) >= 3.0 - 1e-9,
                "{:?} to {:?}",
                w[0],
                w[1]
            );
        }
    }
    // No height: the plain fill, stitch for stitch; and only a fill waves.
    let mut flat = region(RegionType::Fill);
    flat.angle = 0.0;
    let plain = crate::region::layers(&c, &flat, &[]);
    flat.wave_height = Some(0.0);
    flat.wave_length = Some(30.0);
    assert_eq!(crate::region::layers(&c, &flat, &[]), plain);
    let mut satin = region(RegionType::Satin);
    satin.wave_height = Some(3.0);
    assert!(satin.wave().is_none());
    // Saved only when set.
    let json = serde_json::to_string(&region(RegionType::Fill)).expect("json");
    assert!(!json.contains("wave"));
    let mut wavy = region(RegionType::Fill);
    wavy.wave_height = Some(3.0);
    wavy.wave_length = Some(12.0);
    let back: RegionSewing =
        serde_json::from_str(&serde_json::to_string(&wavy).expect("json")).expect("load");
    assert_eq!(back, wavy);
}

/// Maze fill (#533): the top stitching of a maze on `c` with `holes`.
fn maze(c: &[Contour], holes: &[Contour], straightness: f64, seed: u64) -> Vec<Run> {
    let mut r = region(RegionType::Maze);
    r.angle = 0.0;
    r.straightness = Some(straightness);
    r.maze_seed = Some(seed);
    crate::region::layers(c, &r, holes)
        .pop()
        .expect("top stitching")
}

/// Distance between segments ab and cd (0 where they cross).
fn segment_gap(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> f64 {
    let cross = |o: Vec2, p: Vec2, q: Vec2| (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
    let (d1, d2) = (cross(a, b, c), cross(a, b, d));
    let (d3, d4) = (cross(c, d, a), cross(c, d, b));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    let to = |p: Vec2, u: Vec2, v: Vec2| {
        let e = v - u;
        let l2 = e.x * e.x + e.y * e.y;
        let t = if l2 > 0.0 {
            (((p.x - u.x) * e.x + (p.y - u.y) * e.y) / l2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        p.distance(u + e * t)
    };
    to(a, c, d)
        .min(to(b, c, d))
        .min(to(c, a, b))
        .min(to(d, a, b))
}

/// Mean length of the straight stretches of a path, between its turns.
fn mean_corridor(path: &[Vec2]) -> f64 {
    let mut stretches = Vec::new();
    let mut len = 0.0;
    for w in path.windows(3) {
        len += w[0].distance(w[1]);
        let (u, v) = (w[1] - w[0], w[2] - w[1]);
        if (u.x * v.y - u.y * v.x).abs() > 1e-6 * u.length() * v.length() {
            stretches.push(len);
            len = 0.0;
        }
    }
    stretches.iter().sum::<f64>() / stretches.len().max(1) as f64
}

#[test]
fn maze_fill_is_one_path_that_never_crosses_or_crowds_itself_and_reaches_the_edge() {
    // A 40 mm square at the default 2 mm spacing.
    let c = vec![Contour::rectangle(100.0, 100.0, 500.0, 500.0)];
    let spacing = 20.0;
    let runs = maze(&c, &[], 70.0, 533);
    let sewn: Vec<&Run> = runs.iter().filter(|r| r.len() >= 2).collect();
    assert_eq!(sewn.len(), 1, "one continuous run");
    let path = sewn[0];
    assert!(path.iter().all(|&p| crate::geometry::contains(p, &c)));
    // Passes at least 0.85 spacing apart wherever they are not neighbours along the path.
    let mut at = vec![0.0];
    for w in path.windows(2) {
        at.push(at[at.len() - 1] + w[0].distance(w[1]));
    }
    let total = at[at.len() - 1];
    let mut nearest = f64::MAX;
    for i in 0..path.len() - 1 {
        for j in i + 2..path.len() - 1 {
            let along = (at[j] - at[i + 1]).min(total - at[j + 1] + at[i]);
            if along < 2.0 * spacing {
                continue;
            }
            nearest = nearest.min(segment_gap(path[i], path[i + 1], path[j], path[j + 1]));
        }
    }
    assert!(nearest >= 0.85 * spacing, "passes {nearest:.1} apart");
    // The whole square is within a spacing of the path, edges included (≤ half a cell).
    let mut far: f64 = 0.0;
    for x in (100..=500).step_by(10) {
        for y in (100..=500).step_by(10) {
            let p = v2(x as f64, y as f64);
            let d = path
                .windows(2)
                .map(|w| segment_gap(p, p, w[0], w[1]))
                .fold(f64::MAX, f64::min);
            far = far.max(d);
        }
    }
    assert!(far <= spacing + 1.0, "a point {far:.1} from the path");
    // Corridors: the bias makes the straight stretches longer than the uniform tree's.
    let twisty = maze(&c, &[], 0.0, 533);
    let (long, short) = (mean_corridor(path), mean_corridor(&twisty[0]));
    assert!(long > 1.4 * short, "corridors {long:.1} vs {short:.1}");
    assert!(long >= 3.0 * spacing, "corridors {long:.1}");
}

#[test]
fn maze_fill_is_seeded_keeps_out_of_holes_and_leaves_stippling_as_it_was() {
    let c = vec![Contour::rectangle(100.0, 100.0, 500.0, 500.0)];
    let hole = vec![Contour::rectangle(250.0, 250.0, 350.0, 350.0)];
    let a = maze(&c, &hole, 70.0, 7);
    assert_eq!(
        a,
        maze(&c, &hole, 70.0, 7),
        "the same seed sews the same maze"
    );
    let b = maze(&c, &hole, 70.0, 8);
    assert_ne!(a, b, "another seed, another maze");
    for run in [&a, &b] {
        let n: usize = run.iter().map(Vec::len).sum();
        let m: usize = b.iter().map(Vec::len).sum();
        assert!((n as f64 / m as f64 - 1.0).abs() < 0.1, "{n} vs {m} points");
        for p in run.iter().flatten() {
            assert!(!crate::geometry::contains(*p, &hole), "{p:?} in the hole");
        }
    }
    // Defaults, and saved only when set.
    let r = region(RegionType::Maze);
    assert_eq!((r.spacing_mm(), r.maze_straightness()), (2.0, 0.7));
    let json = serde_json::to_string(&r).expect("json");
    assert!(!json.contains("straightness") && !json.contains("mazeSeed"));
    let mut s = r.clone();
    s.straightness = Some(40.0);
    s.maze_seed = Some(12);
    let back: RegionSewing =
        serde_json::from_str(&serde_json::to_string(&s).expect("json")).expect("load");
    assert_eq!(back, s);
    // Stippling ignores both: its walk is the uniform tree it always was.
    let mut st = region(RegionType::Stippling);
    let plain = crate::region::layers(&c, &st, &[]);
    st.straightness = Some(90.0);
    st.maze_seed = Some(3);
    assert_eq!(crate::region::layers(&c, &st, &[]), plain);
}

/// Proper crossings between non-adjacent stitches of `runs` (touching ends do not count).
fn proper_crossings(runs: &[Run]) -> usize {
    let segs: Vec<(Vec2, Vec2, usize, usize)> = runs
        .iter()
        .enumerate()
        .flat_map(|(r, run)| {
            run.windows(2)
                .enumerate()
                .map(move |(i, w)| (w[0], w[1], r, i))
        })
        .filter(|s| s.0.distance(s.1) > 1e-9)
        .collect();
    let cross = |o: Vec2, p: Vec2, q: Vec2| (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
    let mut n = 0;
    for (k, &(a, b, ra, ia)) in segs.iter().enumerate() {
        for &(c, d, rb, ib) in &segs[k + 1..] {
            if ra == rb && ib <= ia + 1 {
                continue;
            }
            let (d1, d2) = (cross(a, b, c), cross(a, b, d));
            let (d3, d4) = (cross(c, d, a), cross(c, d, b));
            if d1 * d2 < -1e-6 && d3 * d4 < -1e-6 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn meander_tiles_are_own_drawings_that_chain_without_crossing() {
    use crate::patterns::{self, PatternKind};
    let tiles: Vec<_> = patterns::library()
        .iter()
        .filter(|p| p.kind == PatternKind::Meander)
        .collect();
    assert!(tiles.len() >= 12, "{} meander tiles", tiles.len());
    let mut ids: Vec<&str> = tiles.iter().map(|p| p.id.as_str()).collect();
    ids.dedup();
    assert_eq!(ids.len(), tiles.len());
    assert_eq!(patterns::default_for(PatternKind::Meander).id, "vermicelli");
    // A tile that crosses itself, touches the row above, or leaves the tile is refused.
    let doc = |paths: &str, shift: f64| {
        format!(
            r#"{{"format":"openstitch.pattern","version":1,"kind":"meander","id":"t","name":"T","rowShift":{shift},"paths":{paths}}}"#
        )
    };
    assert!(
        patterns::parse(&doc(
            "[[[0,0.5],[0.5,0.2],[0.5,0.8],[0.2,0.4],[1,0.5]]]",
            0.0
        ))
        .is_err()
    );
    assert!(patterns::parse(&doc("[[[0,0.5],[0.5,0.0],[0.5,1.0],[1,0.5]]]", 0.0)).is_err());
    assert!(patterns::parse(&doc("[[[0,0.5],[1,0.4]]]", 0.0)).is_err());
    assert!(patterns::parse(&doc("[[[0,0.5],[0.5,0.8],[1,0.5]]]", 0.5)).is_ok());
}

#[test]
fn meander_fill_sews_every_tile_inside_the_shape_without_crossing() {
    use crate::patterns::{self, PatternKind};
    // A 30 mm square with a 6 mm square hole (rectangle: x, y, width, height).
    let c = vec![Contour::rectangle(100.0, 100.0, 300.0, 300.0)];
    let hole = vec![Contour::rectangle(220.0, 220.0, 60.0, 60.0)];
    for tile in patterns::library()
        .iter()
        .filter(|p| p.kind == PatternKind::Meander)
    {
        let mut r = region(RegionType::Meander);
        r.pattern = Some(tile.id.clone());
        r.angle = 0.0;
        let layers = crate::region::layers(&c, &r, &hole);
        let top = layers.last().expect("top");
        let n: usize = top.iter().map(Vec::len).sum();
        assert!(n > 50, "{}: {n} points", tile.id);
        for p in top.iter().flatten() {
            assert!(
                crate::geometry::contains(*p, &[Contour::rectangle(99.0, 99.0, 302.0, 302.0)]),
                "{}: {p:?} outside",
                tile.id
            );
            assert!(
                !crate::geometry::contains(*p, &[Contour::rectangle(222.0, 222.0, 56.0, 56.0)]),
                "{}: {p:?} in the hole",
                tile.id
            );
        }
        assert_eq!(proper_crossings(top), 0, "{}: stitches cross", tile.id);
        // No runs of tiny stitches (sew_check's "in place"): never two under 0.3 mm in a row.
        for run in top {
            for w in run.windows(3) {
                assert!(
                    w[0].distance(w[1]) >= 3.0 - 1e-9 || w[1].distance(w[2]) >= 3.0 - 1e-9,
                    "{}: tiny stitches at {:?}",
                    tile.id,
                    w[1]
                );
            }
        }
        // The same settings sew the same stitches.
        assert_eq!(
            &crate::region::layers(&c, &r, &hole),
            &layers,
            "{}",
            tile.id
        );
    }
    // Unset fields are not saved; the pattern size defaults to 8 mm.
    let r = region(RegionType::Meander);
    assert_eq!(r.pattern_size_mm(), (8.0, 8.0));
    assert_eq!(RegionType::from_id("meander"), Some(RegionType::Meander));
}

/// Gradient fill (#532): the objects of a gradient on `c`, one per colour of `stops`.
fn gradient_layers(
    c: &[Contour],
    stops: &[f64],
    from: Vec2,
    to: Vec2,
    kind: crate::GradientKind,
    dither: crate::GradientDither,
) -> (RegionSewing, Vec<Vec<Run>>) {
    let mut r = region(RegionType::Fill);
    r.angle = 0.0;
    r.underlay = false;
    let layers = (0..stops.len())
        .map(|k| {
            let mut l = r.clone();
            l.gradient = Some(crate::Gradient {
                kind,
                from,
                to,
                stops: stops.to_vec(),
                layer: k,
                dither,
                curve: crate::GradientCurve::Linear,
                seed: 1,
            });
            crate::region::layers(c, &l, &[]).pop().expect("top")
        })
        .collect();
    (r, layers)
}

/// The stitches of `runs` as rounded segments (either direction).
fn segments(runs: &[Run]) -> Vec<((i64, i64), (i64, i64))> {
    let key = |p: Vec2| ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64);
    let mut out: Vec<_> = runs
        .iter()
        .flat_map(|r| r.windows(2).map(|w| (key(w[0]), key(w[1]))))
        .filter(|(a, b)| a != b)
        .map(|(a, b)| if a < b { (a, b) } else { (b, a) })
        .collect();
    out.sort_unstable();
    out
}

/// #891: a colour of a blend sews only its share of the rows, so travel across them shows. In
/// the kit's blends, long straight travel crossed the other colours (a purple line across the
/// radial blend's gold centre), both between a colour's pieces and to and from its start and
/// end points. Of any straight stretch running across the rows (more than 10° off them), no
/// more than 2 mm may show: lie over 1 mm cells where the same colour's thread covers under
/// 70 % of the fabric (#901: elsewhere travel lies on its own colour) and away from the outline
/// (within 0.6 mm, under the row ends).
#[test]
fn blend_travel_never_runs_far_across_the_rows() {
    use crate::{GradientDither as D, GradientKind as K};
    let rect = Contour::rectangle(100.0, 100.0, 400.0, 300.0);
    for (c, kind, dither, stops, from, to) in [
        // As the kit's radial swatch: a 20 mm circle, three colours, the last round the edge.
        (
            Contour::ellipse(v2(270.0, 600.0), 200.0, 200.0, 64),
            K::Radial,
            D::Noise,
            vec![0.0, 0.5, 1.0],
            v2(270.0, 600.0),
            v2(470.0, 600.0),
        ),
        (
            circle(),
            K::Radial,
            D::Noise,
            vec![0.0, 0.4, 1.0],
            v2(200.0, 200.0),
            v2(350.0, 200.0),
        ),
        (
            rect.clone(),
            K::Linear,
            D::Ordered,
            vec![0.0, 0.5, 1.0],
            v2(100.0, 250.0),
            v2(500.0, 250.0),
        ),
        (
            rect,
            K::Linear,
            D::Rows,
            vec![0.0, 1.0],
            v2(300.0, 100.0),
            v2(300.0, 400.0),
        ),
    ] {
        let mut r = region(RegionType::Fill);
        r.angle = 0.0;
        r.underlay = false;
        let mut d = Design::new("t", DesignPage::default());
        for k in 0..stops.len() {
            let mut l = r.clone();
            // A thread of its own, so the colours are told apart.
            l.thread = Thread::rgb(0x20_40_60 * (k as u32 + 1));
            l.gradient = Some(crate::Gradient {
                kind,
                from,
                to,
                stops: stops.clone(),
                layer: k,
                dither,
                curve: crate::GradientCurve::Linear,
                seed: 7,
            });
            d.add(DesignObject::shape(vec![c.clone()], Some(l), None));
        }
        let colours = colour_stretches(&d);
        assert_eq!(colours.len(), stops.len(), "one colour block per colour");
        let across = colours
            .iter()
            .map(|s| longest_exposed_across_rows(s, 2.0, &c, 10.0 / r.density))
            .fold(0.0, f64::max);
        assert!(
            across <= 20.0,
            "{kind:?} {dither:?}: {:.1} mm of travel showing across the rows",
            across / 10.0
        );
    }
}

/// The sewn stretches of a compiled design (needle points between jumps and trims), grouped by
/// colour (split at colour changes).
fn colour_stretches(d: &Design) -> Vec<Vec<Vec<Vec2>>> {
    let p = Compiler::default().compile(d);
    let mut out: Vec<Vec<Vec<Vec2>>> = vec![vec![Vec::new()]];
    for s in &p.stitches {
        let colour = out.last_mut().expect("one");
        match s.command {
            Command::Stitch => colour.last_mut().expect("one").push(v2(s.x, s.y)),
            Command::ColorChange => out.push(vec![Vec::new()]),
            _ => colour.push(Vec::new()),
        }
    }
    for colour in &mut out {
        colour.retain(|s| s.len() >= 2);
    }
    out
}

/// Of one colour's `stretches`, the most (0.1 mm, in all) that shows of a straight stretch
/// (needle points within `tol` of the line between its ends) more than 10° off horizontal rows:
/// over 1 mm cells where the colour's thread covers under 70 % of what rows `spacing` apart
/// would, and farther than 6 from `outline`.
fn longest_exposed_across_rows(
    stretches: &[Vec<Vec2>],
    tol: f64,
    outline: &Contour,
    spacing: f64,
) -> f64 {
    let ring = outline.outline_points();
    let on_outline = |p: Vec2| {
        (0..ring.len()).any(|k| {
            crate::simplify::segment_distance(p, ring[k], ring[(k + 1) % ring.len()]) <= 6.0
        })
    };
    // The colour's thread length per 1 mm cell, against rows `spacing` apart filling it.
    let key = |p: Vec2| ((p.x / 10.0).floor() as i64, (p.y / 10.0).floor() as i64);
    let mut length = std::collections::HashMap::<(i64, i64), f64>::new();
    for w in stretches.iter().flat_map(|s| s.windows(2)) {
        let n = (w[0].distance(w[1]) / 2.0).ceil().max(1.0) as usize;
        for s in 0..n {
            let p = w[0].lerp(w[1], (s as f64 + 0.5) / n as f64);
            *length.entry(key(p)).or_insert(0.0) += w[0].distance(w[1]) / n as f64;
        }
    }
    let full = 100.0 / spacing;
    let mut best: f64 = 0.0;
    for points in stretches {
        for i in 0..points.len().saturating_sub(1) {
            let mut j = i + 1;
            while j + 1 < points.len() {
                let (a, b) = (points[i], points[j + 1]);
                let len = a.distance(b);
                let straight = len > 1e-9
                    && points[i + 1..=j].iter().all(|p| {
                        ((b.x - a.x) * (a.y - p.y) - (a.x - p.x) * (b.y - a.y)).abs() / len <= tol
                    });
                if !straight {
                    break;
                }
                j += 1;
            }
            let d = points[j] - points[i];
            // sin 10°.
            let off_rows = d.y.abs() > d.length() * 0.173_648;
            if j - i < 2 || !off_rows || d.length() <= best {
                continue;
            }
            // Walk the stretch in 0.2 mm steps, timing the runs that show.
            let hidden =
                |p: Vec2| on_outline(p) || length.get(&key(p)).is_some_and(|&l| l >= 0.7 * full);
            let mut shown = 0.0;
            for w in points[i..=j].windows(2) {
                let n = (w[0].distance(w[1]) / 2.0).ceil().max(1.0) as usize;
                let step = w[0].distance(w[1]) / n as f64;
                for s in 0..n {
                    if !hidden(w[0].lerp(w[1], (s as f64 + 0.5) / n as f64)) {
                        shown += step;
                    }
                }
            }
            best = best.max(shown);
        }
    }
    best
}

#[test]
fn gradient_colours_share_one_set_of_rows_each_stitch_sewn_once() {
    use crate::{GradientDither as D, GradientKind as K};
    // A 40 x 30 mm rectangle (x, y, width, height).
    let c = vec![Contour::rectangle(100.0, 100.0, 400.0, 300.0)];
    for (kind, dither, stops, from, to) in [
        (
            K::Linear,
            D::Rows,
            vec![0.0, 1.0],
            v2(300.0, 100.0),
            v2(300.0, 400.0),
        ),
        (
            K::Linear,
            D::Ordered,
            vec![0.0, 0.5, 1.0],
            v2(100.0, 250.0),
            v2(500.0, 250.0),
        ),
        (
            K::Radial,
            D::Noise,
            vec![0.0, 0.4, 1.0],
            v2(300.0, 250.0),
            v2(500.0, 400.0),
        ),
        (
            K::Linear,
            D::Bands,
            vec![0.0, 1.0],
            v2(100.0, 100.0),
            v2(500.0, 400.0),
        ),
    ] {
        let (r, layers) = gradient_layers(&c, &stops, from, to, kind, dither);
        let p = crate::generate::RegionParams {
            angle: 0.0,
            row_spacing: 10.0 / r.density,
            stitch_length: r.stitch_length * 10.0,
            pull_compensation: 0.0,
        };
        let rows = crate::generate::fill_shifted(&c, &p, &[], r.fill_stagger());
        let shared = segments(&rows);
        // Each stitch of the shared rows has exactly one owner, and its colour sews it (the
        // travel between a colour's pieces may run along another colour's stitches).
        let owned: Vec<Vec<_>> = (0..stops.len())
            .map(|k| {
                let g = crate::Gradient {
                    kind,
                    from,
                    to,
                    stops: stops.clone(),
                    layer: k,
                    dither,
                    curve: crate::GradientCurve::Linear,
                    seed: 1,
                };
                segments(&crate::gradient::layer_pieces(
                    &rows,
                    &g,
                    0.0,
                    p.row_spacing,
                    p.stitch_length,
                ))
            })
            .collect();
        for s in &shared {
            let owners = owned.iter().filter(|l| l.binary_search(s).is_ok()).count();
            assert_eq!(
                owners, 1,
                "{kind:?} {dither:?}: stitch {s:?} owned {owners} times"
            );
        }
        for (k, l) in owned.iter().enumerate() {
            let sewn = segments(&layers[k]);
            assert!(
                l.iter().all(|s| sewn.binary_search(s).is_ok()),
                "{kind:?} {dither:?}: colour {k} leaves out a stitch it owns"
            );
        }
        assert!(
            layers
                .iter()
                .all(|l| l.iter().map(Vec::len).sum::<usize>() > 20),
            "{kind:?} {dither:?}: every colour sews"
        );
        // Reproducible.
        let (_, again) = gradient_layers(&c, &stops, from, to, kind, dither);
        assert_eq!(again, layers);
    }
}

#[test]
fn a_two_colour_blend_has_complementary_rows_and_bands_have_a_hard_edge() {
    use crate::{GradientDither as D, GradientKind as K};
    let c = vec![Contour::rectangle(100.0, 100.0, 400.0, 400.0)];
    // Across the rows, top to bottom: every row is one colour.
    let (r, layers) = gradient_layers(
        &c,
        &[0.0, 1.0],
        v2(300.0, 100.0),
        v2(300.0, 500.0),
        K::Linear,
        D::Rows,
    );
    let spacing = 10.0 / r.density;
    let rows_in = |l: &Vec<Run>, y0: f64, y1: f64| {
        let mut ys: Vec<i64> = l
            .iter()
            .flat_map(|run| run.windows(2))
            .filter(|w| (w[0].y - w[1].y).abs() < 1e-6 && w[0].y >= y0 && w[0].y < y1)
            .map(|w| (w[0].y / spacing).round() as i64)
            .collect();
        ys.sort_unstable();
        ys.dedup();
        ys.len() as f64
    };
    for (y0, y1, want) in [
        (100.0, 180.0, 0.1),
        (260.0, 340.0, 0.5),
        (420.0, 500.0, 0.9),
    ] {
        let (a, b) = (rows_in(&layers[0], y0, y1), rows_in(&layers[1], y0, y1));
        let all = 80.0 / spacing;
        assert!(((a + b) / all - 1.0).abs() < 0.1, "rows {a} + {b} of {all}");
        let share = b / (a + b);
        assert!(
            (share - want).abs() < 0.12,
            "{y0}-{y1}: second colour {share:.2}, want {want}"
        );
    }
    // Bands: the first colour alone before the middle, the second after it.
    let (_, bands) = gradient_layers(
        &c,
        &[0.0, 1.0],
        v2(300.0, 100.0),
        v2(300.0, 500.0),
        K::Linear,
        D::Bands,
    );
    let sewn_rows = |l: &Vec<Run>| -> Vec<f64> {
        l.iter()
            .flat_map(|run| run.windows(2))
            .filter(|w| (w[0].y - w[1].y).abs() < 1e-6 && (w[0].x - w[1].x).abs() > 1.0)
            .map(|w| w[0].y)
            .collect()
    };
    assert!(sewn_rows(&bands[0]).iter().all(|&y| y < 302.0));
    assert!(sewn_rows(&bands[1]).iter().all(|&y| y > 298.0));
    // Weights add up to 1; a colour too sparse to show gives its rows away; curves.
    use crate::gradient::{curve, weights};
    for t in [0.0, 0.1, 0.33, 0.5, 0.9, 1.0] {
        let w = weights(&[0.0, 0.4, 1.0], t, 2.2);
        assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-12, "{t}: {w:?}");
    }
    assert_eq!(weights(&[0.0, 1.0], 0.02, 2.2), vec![1.0, 0.0]);
    use crate::GradientCurve as C;
    for c in C::ALL {
        assert_eq!((curve(c, 0.0), curve(c, 1.0)), (0.0, 1.0), "{c:?}");
    }
    assert_eq!(curve(C::Linear, 0.5), 0.5);
    assert_eq!(curve(C::EaseIn, 0.5), 0.25);
    assert_eq!(curve(C::EaseOut, 0.5), 0.75);
    assert_eq!(curve(C::SCurve, 0.5), 0.5);
    // Saved only when set.
    let json = serde_json::to_string(&region(RegionType::Fill)).expect("json");
    assert!(!json.contains("gradient"));
}

#[test]
fn named_gradation_shapes_space_the_rows_by_their_profile() {
    use crate::{Gradation, GradationShape as S};
    // A 40 mm square (x, y, width, height), 2.2 rows/mm at full density.
    let c = vec![Contour::rectangle(100.0, 100.0, 400.0, 400.0)];
    let spacing = 10.0 / 4.5;
    for shape in S::ALL {
        let g = Gradation {
            max: 100.0,
            min: 10.0,
            steps: Some(9),
            shape,
        };
        // The profile: max at its dense points, min at its open ones.
        let f = |t: f64| g.fraction(t);
        match shape {
            S::Linear | S::Convex | S::Concave => assert_eq!((f(0.0), f(1.0)), (1.0, 0.1)),
            S::Centre => assert_eq!((f(0.0), f(0.5), f(1.0)), (0.1, 1.0, 0.1)),
            S::Edges => assert_eq!((f(0.0), f(0.5), f(1.0)), (1.0, 0.1, 1.0)),
            S::Wave => assert!((f(0.5) - 0.1).abs() < 1e-9 && f(0.0) == 1.0 && f(1.0) == 1.0),
        }
        let ys = crate::region::row_positions(&c, 0.0, spacing, Some(&g));
        assert!(ys.len() > 10, "{shape:?}");
        // First row on the start edge, last on the far edge.
        assert!((ys[0] - 100.0).abs() < 0.01 && (ys[ys.len() - 1] - 500.0).abs() < 0.01);
        // Equal steps of the line count: the profile's density integrated across every gap
        // is the same (within 2 %), so each gap is the spacing at the density there.
        let count = |y0: f64, y1: f64| {
            (0..200)
                .map(|i| {
                    let y = y0 + (y1 - y0) * (i as f64 + 0.5) / 200.0;
                    f((y - 100.0) / 400.0) * (y1 - y0) / 200.0 / spacing
                })
                .sum::<f64>()
        };
        let counts: Vec<f64> = ys.windows(2).map(|w| count(w[0], w[1])).collect();
        let mean = counts.iter().sum::<f64>() / counts.len() as f64;
        for (k, c) in counts.iter().enumerate() {
            assert!(
                (c / mean - 1.0).abs() < 0.02,
                "{shape:?}: gap {k} holds {c:.3} lines, mean {mean:.3}"
            );
        }
        assert!((mean - 1.0).abs() < 0.1, "{shape:?}: {mean:.3} lines a gap");
    }
    // Linear is PE-DESIGN's: the same rows as without a shape.
    let pe = Gradation::PE_DEFAULT;
    let linear = Gradation {
        shape: S::Linear,
        ..pe
    };
    assert_eq!(
        crate::region::row_positions(&c, 0.0, spacing, Some(&linear)),
        crate::region::row_positions(&c, 0.0, spacing, Some(&pe))
    );
    // Saved only when not linear, and read back.
    let json = serde_json::to_string(&pe).expect("json");
    assert!(!json.contains("shape"), "{json}");
    let centre = Gradation {
        shape: S::Centre,
        ..pe
    };
    let back: Gradation =
        serde_json::from_str(&serde_json::to_string(&centre).expect("json")).expect("load");
    assert_eq!(back, centre);
}

/// Stitch angle lines (#534): a horseshoe (U) from (c, 300 outer, 150 inner radius), its arms
/// the lower half of a ring.
fn horseshoe() -> Vec<Contour> {
    let c = v2(400.0, 200.0);
    let n = 48;
    let mut pts = Vec::new();
    for i in 0..=n {
        let a = std::f64::consts::PI * i as f64 / n as f64;
        pts.push(c + v2(-300.0 * a.cos(), 300.0 * a.sin()));
    }
    for i in (0..=n).rev() {
        let a = std::f64::consts::PI * i as f64 / n as f64;
        pts.push(c + v2(-150.0 * a.cos(), 150.0 * a.sin()));
    }
    vec![Contour::new(pts, true)]
}

/// Checks angle-line rows on a shape: no row crosses another, every sample point of the shape
/// lies within a spacing of a row, and rows stay at least 0.8 of a spacing apart away from
/// their ends.
fn check_angle_rows(c: &[Contour], holes: &[Contour], lines: &[[Vec2; 2]], spacing: f64) {
    let rows: Vec<Run> = crate::angle_lines::rows(lines, c, holes, spacing)
        .iter()
        .flat_map(|r| crate::paths::clip(r, c, holes))
        .filter(|r| r.len() >= 2)
        .collect();
    assert!(rows.len() > 20, "{} rows", rows.len());
    // Rows are separate runs here: a proper crossing between any two is a seam.
    assert_eq!(proper_crossings(&rows), 0, "rows cross");
    let near = |p: Vec2, skip: usize| {
        rows.iter()
            .enumerate()
            .filter(|(i, _)| *i != skip)
            .flat_map(|(_, r)| r.windows(2))
            .map(|w| segment_gap(p, p, w[0], w[1]))
            .fold(f64::MAX, f64::min)
    };
    let inside = |p: Vec2| crate::geometry::contains(p, c) && !crate::geometry::contains(p, holes);
    let b =
        crate::geometry::Rect::enclosing(c.iter().flat_map(Contour::outline_points)).expect("b");
    let mut far: f64 = 0.0;
    let mut y = b.min_y + 5.0;
    while y < b.max_y {
        let mut x = b.min_x + 5.0;
        while x < b.max_x {
            let p = v2(x, y);
            if inside(p) {
                far = far.max(near(p, usize::MAX));
            }
            x += 10.0;
        }
        y += 10.0;
    }
    assert!(
        far <= spacing * 1.05,
        "a point {far:.1} from the rows (spacing {spacing:.1})"
    );
    // Crowding: the middle of each row piece is at least 0.8 spacing from every other piece.
    for (i, r) in rows.iter().enumerate() {
        let mid = r[0].lerp(r[r.len() - 1], 0.5);
        let d = near(mid, i);
        assert!(d >= 0.8 * spacing - 1e-6, "row {i} {d:.2} from the next");
    }
}

#[test]
fn angle_lines_turn_the_rows_around_a_bend_without_crossing_or_crowding() {
    let c = horseshoe();
    // Across the left arm's end, across the bottom, across the right arm's end.
    let lines = [
        [v2(80.0, 220.0), v2(270.0, 220.0)],
        [v2(400.0, 330.0), v2(400.0, 520.0)],
        [v2(530.0, 220.0), v2(720.0, 220.0)],
    ];
    assert!(crate::angle_lines::check(&lines, &c, &[]).is_ok());
    check_angle_rows(&c, &[], &lines, 10.0 / 4.5 * 3.0);
    // Sewn as a fill: under sewing kept, the top stitching from the rows, reproducible; without
    // lines the plain fill.
    let mut r = region(RegionType::Fill);
    r.angle = 0.0;
    let plain = crate::region::layers(&c, &r, &[]);
    r.angle_lines = lines.to_vec();
    let turned = crate::region::layers(&c, &r, &[]);
    assert_eq!(turned.len(), plain.len());
    assert_ne!(turned.last(), plain.last());
    assert_eq!(&turned[..turned.len() - 1], &plain[..plain.len() - 1]);
    assert_eq!(crate::region::layers(&c, &r, &[]), turned);
    let json = serde_json::to_string(&region(RegionType::Fill)).expect("json");
    assert!(!json.contains("angleLines"));
    let back: RegionSewing =
        serde_json::from_str(&serde_json::to_string(&r).expect("json")).expect("load");
    assert_eq!(back, r);
}

#[test]
fn angle_lines_on_a_ring_wrap_round_and_bad_lines_are_refused() {
    // A ring as one object: outer circle and a hole.
    let circle = |r: f64| {
        Contour::new(
            (0..72)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / 72.0;
                    v2(400.0 + r * a.cos(), 400.0 + r * a.sin())
                })
                .collect(),
            true,
        )
    };
    let ring = vec![circle(300.0), circle(150.0)];
    // Four lines pointing at the centre, across the band.
    let spoke = |deg: f64| {
        let a = deg.to_radians();
        [
            v2(400.0 + 120.0 * a.cos(), 400.0 + 120.0 * a.sin()),
            v2(400.0 + 330.0 * a.cos(), 400.0 + 330.0 * a.sin()),
        ]
    };
    let lines = [spoke(0.0), spoke(90.0), spoke(180.0), spoke(270.0)];
    assert!(crate::angle_lines::check(&lines, &ring, &[]).is_ok());
    check_angle_rows(&ring, &[], &lines, 10.0 / 4.5 * 3.0);
    // Refused: one line, crossing lines, a line outside the shape.
    let c = horseshoe();
    assert!(crate::angle_lines::check(&lines[..1], &ring, &[]).is_err());
    let crossing = [
        [v2(80.0, 220.0), v2(270.0, 220.0)],
        [v2(100.0, 200.0), v2(260.0, 260.0)],
    ];
    let err = crate::angle_lines::check(&crossing, &c, &[]).expect_err("crossing");
    assert!(err.contains("cross"), "{err}");
    let outside = [
        [v2(80.0, 220.0), v2(270.0, 220.0)],
        [v2(380.0, 100.0), v2(420.0, 100.0)],
    ];
    assert!(crate::angle_lines::check(&outside, &c, &[]).is_err());
    // A fill whose lines are refused sews as without them.
    let mut r = region(RegionType::Fill);
    let plain = crate::region::layers(&c, &r, &[]);
    r.angle_lines = crossing.to_vec();
    assert_eq!(crate::region::layers(&c, &r, &[]), plain);
}

#[test]
fn spacing_at_measures_skeleton_satin_density_across_the_column() {
    // #685: a skeleton satin spaces its crossings along its centre line; spacingAt measures the
    // spacing that far across the column from the inner edge of a bend instead, so 0 sews fewer
    // crossings and 0.5 (the centre) as many. A straight column is the same either way.
    use crate::column::SatinShaping;
    use crate::skeleton::{Skeleton, satin_with};
    let arc: Vec<Vec2> = (0..=12)
        .map(|k| {
            let a = std::f64::consts::PI * k as f64 / 12.0;
            v2(300.0 * a.cos(), 300.0 * a.sin())
        })
        .collect();
    let bent = Skeleton {
        widths: vec![8.0; arc.len()],
        points: arc,
    };
    let spacing = 10.0 / 4.5;
    let count = |s: &Skeleton, at: Option<f64>| {
        let shaping = SatinShaping {
            spacing_at: at,
            ..Default::default()
        };
        satin_with(s, spacing, 0.0, &shaping).len() as f64
    };
    // 1 is the unset value.
    assert_eq!(count(&bent, None), count(&bent, Some(1.0)));
    let (centre, middle, inner) = (
        count(&bent, None),
        count(&bent, Some(0.5)),
        count(&bent, Some(0.0)),
    );
    assert!(inner < middle, "{inner} {middle}");
    assert!(
        (middle - centre).abs() <= centre * 0.02,
        "{middle} {centre}"
    );
    // Inner radius 26 mm, centre 30 mm: about 26/30 of the crossings.
    let ratio = inner / centre;
    assert!((0.8..0.92).contains(&ratio), "{ratio}");
    let straight = Skeleton {
        points: vec![v2(0.0, 0.0), v2(500.0, 0.0)],
        widths: vec![6.0, 6.0],
    };
    let (a, b) = (count(&straight, None), count(&straight, Some(0.0)));
    assert!((a - b).abs() <= 2.0, "{a} {b}");
}

/// The extent (min, max) along x and y of the needle points of `runs`.
fn extent(runs: &[Run]) -> ((f64, f64), (f64, f64)) {
    let pts = runs.iter().flatten();
    let fold = |f: fn(&Vec2) -> f64| {
        pts.clone()
            .map(f)
            .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
    };
    (fold(|p| p.x), fold(|p| p.y))
}

#[test]
fn push_compensation_moves_only_the_sides_along_the_rows_in() {
    // A 40 x 20 mm rectangle (x, y, width, height).
    let c = vec![Contour::rectangle(100.0, 100.0, 400.0, 200.0)];
    for angle in [0.0, 90.0] {
        let mut r = region(RegionType::Fill);
        r.angle = angle;
        r.underlay = false;
        let plain = crate::region::layers(&c, &r, &[]);
        r.push_compensation = Some(1.0);
        let pushed = crate::region::layers(&c, &r, &[]);
        let (px, py) = extent(plain.last().expect("top"));
        let (qx, qy) = extent(pushed.last().expect("top"));
        // Rows along x (0°): the top and bottom move in 1 mm, the row ends stay; at 90° the
        // other way round.
        let (along, across, along_p, across_p) = if angle == 0.0 {
            (px, py, qx, qy)
        } else {
            (py, px, qy, qx)
        };
        assert!(
            (along_p.0 - along.0).abs() < 2.0 && (along_p.1 - along.1).abs() < 2.0,
            "{angle}°: row ends {along:?} → {along_p:?}"
        );
        assert!(
            across_p.0 >= across.0 + 8.0 && across_p.1 <= across.1 - 8.0,
            "{angle}°: sides {across:?} → {across_p:?}"
        );
    }
    // With under sewing the first layers are the plain fill's.
    let mut r = region(RegionType::Fill);
    r.angle = 0.0;
    let plain = crate::region::layers(&c, &r, &[]);
    r.push_compensation = Some(1.0);
    let pushed = crate::region::layers(&c, &r, &[]);
    assert_eq!(&pushed[..pushed.len() - 1], &plain[..plain.len() - 1]);
    // Off: unchanged; saved only when set.
    r.push_compensation = None;
    assert_eq!(crate::region::layers(&c, &r, &[]), plain);
    assert!(
        !serde_json::to_string(&r)
            .expect("json")
            .contains("pushCompensation")
    );
}

#[test]
fn a_second_under_sewing_layer_on_a_fill_runs_at_its_own_angle() {
    use crate::column::SecondUnderlay;
    let c = vec![Contour::rectangle(100.0, 100.0, 400.0, 300.0)];
    let mut r = region(RegionType::Fill);
    r.angle = 0.0;
    let plain = crate::region::layers(&c, &r, &[]);
    // Crossed tatami by default: rows along the fill rows (0°), square to the first layer.
    r.underlay2 = Some(SecondUnderlay::default());
    let crossed = crate::region::layers(&c, &r, &[]);
    assert_eq!(crossed.len(), plain.len() + 1, "one more layer");
    assert_eq!(&crossed[..plain.len() - 1], &plain[..plain.len() - 1]);
    assert_eq!(
        crossed.last(),
        plain.last(),
        "the top stitching is unchanged"
    );
    let second = &crossed[plain.len() - 1];
    // Its long stitches run along x; inset 0.5 mm.
    let long: Vec<(Vec2, Vec2)> = second
        .iter()
        .flat_map(|r| r.windows(2).map(|w| (w[0], w[1])))
        .filter(|(a, b)| a.distance(*b) > 15.0)
        .collect();
    assert!(!long.is_empty());
    assert!(
        long.iter()
            .all(|(a, b)| (a.y - b.y).abs() < 0.1 * (a.x - b.x).abs()),
        "rows along the fill rows"
    );
    let ((x0, x1), (y0, y1)) = extent(second);
    assert!(
        x0 >= 104.0 && x1 <= 496.0 && y0 >= 104.0 && y1 <= 396.0,
        "inset"
    );
    // At 90° the rows turn square to the fill rows; an edge style runs round the outline.
    r.underlay2 = Some(SecondUnderlay {
        angle: 90.0,
        ..SecondUnderlay::default()
    });
    let turned = crate::region::layers(&c, &r, &[]);
    let long: Vec<(Vec2, Vec2)> = turned[plain.len() - 1]
        .iter()
        .flat_map(|r| r.windows(2).map(|w| (w[0], w[1])))
        .filter(|(a, b)| a.distance(*b) > 15.0)
        .collect();
    assert!(
        long.iter()
            .all(|(a, b)| (a.x - b.x).abs() < 0.1 * (a.y - b.y).abs()),
        "rows across"
    );
    r.underlay2 = Some(SecondUnderlay {
        style: crate::UnderlayStyle::Edge,
        ..SecondUnderlay::default()
    });
    let edged = crate::region::layers(&c, &r, &[]);
    let ((x0, x1), (y0, y1)) = extent(&edged[plain.len() - 1]);
    assert!(
        (x0 - 105.0).abs() < 1.0
            && (x1 - 495.0).abs() < 1.0
            && (y0 - 105.0).abs() < 1.0
            && (y1 - 395.0).abs() < 1.0
    );
    // Without under sewing there is no second layer either.
    r.underlay = false;
    let none = crate::region::layers(&c, &r, &[]);
    assert_eq!(none.len(), 1);
}

#[test]
fn piping_travel_keeps_off_the_rows_at_a_lobe_tip() {
    // #471: region-types' piping (1.2 lines/mm at 30 degrees) on its lobe. Where the outline
    // runs nearly along the rows, a short row's end saw the next row only round the lobe's tip,
    // and the travel ran out to the tip and back over the row just sewn: 1 mm cells with three
    // times the median thread. The travel now walks the outline there.
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/region-types.json");
    let steps: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("reads")).expect("json");
    let lobe: Vec<Vec2> = steps[5]["params"]["points"]
        .as_array()
        .expect("points")
        .iter()
        .map(|p| {
            v2(
                p[0].as_f64().unwrap_or(0.0) * 10.0,
                p[1].as_f64().unwrap_or(0.0) * 10.0,
            )
        })
        .collect();
    let c = vec![Contour::new(lobe, true)];
    let mut r = region(RegionType::Piping);
    r.density = 1.2;
    r.angle = 30.0;
    let runs = crate::region::layers(&c, &r, &[]);
    // Thread per 1 mm cell, the stitches sampled every 0.1 mm.
    let mut cells: std::collections::HashMap<(i64, i64), f64> = Default::default();
    for run in runs.iter().flatten() {
        for w in run.windows(2) {
            let l = w[0].distance(w[1]);
            let n = ((l / 1.0).ceil() as usize).max(1);
            for k in 0..n {
                let q = w[0].lerp(w[1], (k as f64 + 0.5) / n as f64);
                *cells
                    .entry(((q.x / 10.0).floor() as i64, (q.y / 10.0).floor() as i64))
                    .or_default() += l / n as f64;
            }
        }
    }
    let mut v: Vec<f64> = cells.into_values().collect();
    v.sort_by(f64::total_cmp);
    let median = v[v.len() / 2];
    let max = v[v.len() - 1];
    assert!(
        max < 2.6 * median,
        "densest cell {max} against the median {median}"
    );
}

#[test]
fn tartan_rows_lie_in_their_colours_stripes_and_share_out_every_row() {
    use crate::{Tartan, TartanStripe};
    // A 60 mm square (x, y, width, height); a sett of 10 mm colour 0 and 20 mm colour 1.
    let c = vec![Contour::rectangle(100.0, 100.0, 600.0, 600.0)];
    let sett = vec![
        TartanStripe {
            color: 0,
            width: 10.0,
        },
        TartanStripe {
            color: 1,
            width: 20.0,
        },
    ];
    let mut r = region(RegionType::Fill);
    r.angle = 0.0;
    r.underlay = false;
    let spacing = 10.0 / r.density;
    let step = spacing * 2.0;
    let sewn: Vec<Vec<Vec<Run>>> = (0..2)
        .map(|k| {
            let mut l = r.clone();
            l.tartan = Some(Tartan {
                sett: sett.clone(),
                symmetric: false,
                layer: k,
            });
            crate::region::layers(&c, &l, &[])
        })
        .collect();
    for (k, layers) in sewn.iter().enumerate() {
        assert_eq!(layers.len(), 2, "colour {k}: warp and weft");
        // No stitch over 7 mm, travel included.
        for run in layers.iter().flatten() {
            for w in run.windows(2) {
                assert!(
                    w[0].distance(w[1]) <= 70.0 + 1e-6,
                    "colour {k}: a stitch over 7 mm"
                );
            }
        }
    }
    // Each family's rows, by colour: the row positions across (warp: x, weft: y).
    let t = |k: usize| Tartan {
        sett: sett.clone(),
        symmetric: false,
        layer: k,
    };
    let rows =
        |k: usize| crate::tartan::layers(&c, &[], &t(k), 0.0, spacing, 40.0, r.fill_stagger());
    let (r0, r1) = (rows(0), rows(1));
    for family in [0usize, 1] {
        let p = crate::generate::RegionParams {
            angle: if family == 0 { 90.0 } else { 0.0 },
            row_spacing: step,
            stitch_length: 40.0,
            pull_compensation: 0.0,
        };
        let all = segments(&crate::generate::fill_shifted(
            &c,
            &p,
            &[],
            r.fill_stagger(),
        ));
        let (a, b) = (segments(&r0[family]), segments(&r1[family]));
        // Together every stitch of the family once.
        let mut both: Vec<_> = a.iter().chain(&b).copied().collect();
        both.sort_unstable();
        assert_eq!(both, all, "family {family}: every stitch once");
        // Stitches along the rows lie in their own colour's stripes; across (warp: x, weft: y).
        let across = |runs: &Vec<Run>| -> Vec<f64> {
            runs.iter()
                .flat_map(|run| run.windows(2))
                .filter(|w| w[0].distance(w[1]) > 10.0)
                .map(|w| {
                    let m = w[0].lerp(w[1], 0.5);
                    if family == 0 {
                        m.x - 100.0
                    } else {
                        m.y - 100.0
                    }
                })
                .collect()
        };
        let (pa, pb) = (across(&r0[family]), across(&r1[family]));
        assert!(!pa.is_empty() && !pb.is_empty());
        assert!(
            pa.iter().all(|&q| q.rem_euclid(300.0) < 100.0),
            "family {family}: colour 0 off its stripes"
        );
        assert!(
            pb.iter().all(|&q| q.rem_euclid(300.0) >= 100.0),
            "family {family}: colour 1 off its stripes"
        );
        // Stripe width: the first colour-0 stripe holds 10 mm of rows, within a row step.
        let mut first: Vec<i64> = pa
            .iter()
            .filter(|&&q| q < 100.0)
            .map(|q| (q * 10.0).round() as i64)
            .collect();
        first.sort_unstable();
        first.dedup();
        let width = first.len() as f64 * step;
        assert!(
            (width - 100.0).abs() <= step + 1e-6,
            "family {family}: stripe {width:.2} (step {step:.2})"
        );
    }
    // A symmetric sett mirrors about its first and last stripes.
    let sym = Tartan {
        sett: vec![
            TartanStripe {
                color: 0,
                width: 8.0,
            },
            TartanStripe {
                color: 1,
                width: 24.0,
            },
            TartanStripe {
                color: 2,
                width: 4.0,
            },
        ],
        symmetric: true,
        layer: 0,
    };
    let rep = crate::tartan::repeat(&sym);
    assert_eq!(rep, vec![(0, 80.0), (1, 240.0), (2, 40.0), (1, 240.0)]);
    assert_eq!(crate::tartan::colour_at(&rep, 590.0), 1);
    assert_eq!(crate::tartan::colour_at(&rep, 610.0), 0);
    // Off by default: nothing saved.
    assert!(
        !serde_json::to_string(&region(RegionType::Fill))
            .expect("json")
            .contains("tartan")
    );
}

/// The kit's L (#890): 7..57 × 7..80 mm with lines across its top arm, its corner and its foot.
fn kit_l() -> (Vec<Contour>, Vec<[Vec2; 2]>) {
    let p = |x: f64, y: f64| v2(x * 10.0, y * 10.0);
    let c = vec![Contour::new(
        vec![
            p(7.0, 7.0),
            p(57.0, 7.0),
            p(57.0, 25.0),
            p(25.0, 25.0),
            p(25.0, 80.0),
            p(7.0, 80.0),
        ],
        true,
    )];
    let lines = vec![
        [p(45.0, 5.0), p(45.0, 27.0)],
        [p(27.0, 27.0), p(5.0, 5.0)],
        [p(5.0, 60.0), p(27.0, 60.0)],
    ];
    (c, lines)
}

/// The 1 mm cells (a cell or more inside the outline) whose top stitching is thinner than half
/// what rows `spacing` apart lay there, by the length of stitching in each cell.
fn thin_cells(c: &[Contour], top: &[Run], spacing: f64) -> Vec<(i64, i64)> {
    let mut ink: std::collections::HashMap<(i64, i64), f64> = Default::default();
    for run in top {
        for w in run.windows(2) {
            let n = (w[0].distance(w[1]) / 0.5).ceil().max(1.0) as usize;
            for i in 0..n {
                let q = w[0].lerp(w[1], (i as f64 + 0.5) / n as f64);
                *ink.entry(((q.x / 10.0).floor() as i64, (q.y / 10.0).floor() as i64))
                    .or_default() += w[0].distance(w[1]) / n as f64;
            }
        }
    }
    let b =
        crate::geometry::Rect::enclosing(c.iter().flat_map(Contour::outline_points)).expect("b");
    let edges: Vec<(Vec2, Vec2)> = c.iter().flat_map(Contour::edges).collect();
    let mut thin = Vec::new();
    for j in (b.min_y / 10.0).floor() as i64..(b.max_y / 10.0).ceil() as i64 {
        for i in (b.min_x / 10.0).floor() as i64..(b.max_x / 10.0).ceil() as i64 {
            let m = v2(i as f64 * 10.0 + 5.0, j as f64 * 10.0 + 5.0);
            let clear = edges
                .iter()
                .map(|&(a, e)| segment_gap(m, m, a, e))
                .fold(f64::MAX, f64::min);
            if !crate::geometry::contains(m, c) || clear < 15.0 {
                continue;
            }
            if ink.get(&(i, j)).copied().unwrap_or(0.0) < 0.5 * 100.0 / spacing {
                thin.push((i, j));
            }
        }
    }
    thin
}

/// Where the top stitching of `top` crosses the probe from `a` to `b`, as distances along it.
fn crossings_along(top: &[Run], a: Vec2, b: Vec2) -> Vec<f64> {
    let mut at = Vec::new();
    let d = b - a;
    for run in top {
        for w in run.windows(2) {
            let e = w[1] - w[0];
            let den = d.x * e.y - d.y * e.x;
            if den.abs() < 1e-12 {
                continue;
            }
            let q = w[0] - a;
            let t = (q.x * e.y - q.y * e.x) / den;
            let s = (q.x * d.y - q.y * d.x) / den;
            if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&s) {
                at.push(t * d.length());
            }
        }
    }
    at.sort_by(f64::total_cmp);
    at
}

#[test]
fn angle_lines_cover_the_kit_l_at_the_set_density() {
    // #890: the kit's L. Every 1 mm cell carries at least half the stitching rows `spacing`
    // apart lay there; past the outer lines the rows keep the lines' direction at the set
    // spacing; and across the corner line, where the rows of both bands are thinned, no gap is
    // over twice the usual one (an odd row count left the last kept row three steps short of
    // the line: 3.6 mm/10 against 1.4 there).
    let (c, lines) = kit_l();
    let mut r = region(RegionType::Fill);
    r.angle_lines = lines;
    let layers = crate::region::layers(&c, &r, &[]);
    let top = layers.last().expect("top");
    let spacing = 10.0 / r.density;
    let thin = thin_cells(&c, top, spacing);
    assert!(thin.is_empty(), "{} thin 1 mm cells: {thin:?}", thin.len());
    for (what, a, b) in [
        ("top arm", v2(460.0, 160.0), v2(560.0, 160.0)),
        ("foot", v2(160.0, 610.0), v2(160.0, 790.0)),
    ] {
        let x = crossings_along(top, a, b);
        for w in x.windows(2) {
            let gap = w[1] - w[0];
            assert!(
                (gap - spacing).abs() < 0.05 * spacing || gap < 1e-6,
                "{what}: rows {gap:.2} apart at {:.0} (spacing {spacing:.2})",
                w[0]
            );
        }
    }
    for k in 3..=8 {
        // Square across the corner line, from 30 % to 80 % of the way to the inner corner.
        let m = v2(70.0, 70.0).lerp(v2(250.0, 250.0), f64::from(k) / 10.0);
        let x = crossings_along(top, m + v2(-40.0, 40.0), m + v2(40.0, -40.0));
        let mut gaps: Vec<f64> = x
            .windows(2)
            .map(|w| w[1] - w[0])
            .filter(|&g| g > 1e-6)
            .collect();
        let most = gaps.iter().copied().fold(0.0, f64::max);
        gaps.sort_by(f64::total_cmp);
        let median = gaps[gaps.len() / 2];
        assert!(
            most <= 2.0 * median,
            "{k}: a {most:.1} gap, the median {median:.1}"
        );
    }
}

/// Thread (mm) per 1 mm cell of the square x 56..94, y 56..92 mm (1 mm in from #927's 40 x 38
/// mm square), from every stitch of `runs`.
fn thread_per_cell(runs: &[Run]) -> Vec<f64> {
    let (nx, ny) = (38usize, 36usize);
    let mut cells = vec![0.0; nx * ny];
    for run in runs {
        for w in run.windows(2) {
            let n = ((w[0].distance(w[1]) / 1.0).ceil() as usize).max(1);
            for k in 0..n {
                let p = w[0].lerp(w[1], (k as f64 + 0.5) / n as f64);
                let (i, j) = ((p.x - 560.0) / 10.0, (p.y - 560.0) / 10.0);
                if i >= 0.0 && j >= 0.0 && (i as usize) < nx && (j as usize) < ny {
                    cells[j as usize * nx + i as usize] += w[0].distance(w[1]) / n as f64 / 10.0;
                }
            }
        }
    }
    cells
}

/// Whether segments a-b and c-d cross (not just touch).
fn segments_cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let cr = |o: Vec2, p: Vec2, q: Vec2| (p - o).x * (q - o).y - (p - o).y * (q - o).x;
    let (d1, d2, d3, d4) = (cr(c, d, a), cr(c, d, b), cr(a, b, c), cr(a, b, d));
    d1 * d2 < -1e-9 && d3 * d4 < -1e-9
}

/// Stitches of `runs` that run along a row of the guide `guide`: both ends a whole number of
/// `spacing` from it, the same number (travel, tie-ins and row ends left out).
fn row_stitches(runs: &[Run], guide: &crate::direction::Guide, spacing: f64) -> Vec<(Vec2, Vec2)> {
    runs.iter()
        .flat_map(|run| run.windows(2))
        .filter(|w| {
            let (a, b) = (guide.frame(w[0]).1 / spacing, guide.frame(w[1]).1 / spacing);
            (a - b).abs() < 0.1 && (a - a.round()).abs() < 0.1 && w[0].distance(w[1]) > 10.0
        })
        .map(|w| (w[0], w[1]))
        .collect()
}

/// #927: plain and programmable fill along a U-shaped direction curve (a C bulging down into
/// #927's 40 x 38 mm square) sewed no rows inside the U: 273 of the square's 1 mm cells stayed
/// empty, only travel crossing them, because the rows' distances from the guide were taken at
/// the outline's corners, and the middle of the U lies farther from the guide than any of them.
/// Now every cell has thread, the coverage is the same fill's without the curve (within 3 %), and
/// plain fill's rows never cross each other.
#[test]
fn fill_along_a_u_shaped_curve_covers_the_inside_of_the_u() {
    use crate::geometry::Bezier;
    let square = Contour::rectangle(550.0, 550.0, 400.0, 380.0);
    let mut curve = Contour::new(vec![v2(520.0, 600.0), v2(980.0, 600.0)], false);
    curve.set_curve(
        0,
        Some(Bezier {
            c1: v2(650.0, 950.0),
            c2: v2(850.0, 950.0),
        }),
    );
    for kind in [RegionType::Fill, RegionType::ProgrammableFill] {
        let mut plain = region(kind);
        plain.underlay = false;
        plain.base_sewing = false;
        let mut along = plain.clone();
        along.direction_curve = Some(curve.clone());
        let top = |r: &RegionSewing| {
            crate::region::layers(std::slice::from_ref(&square), r, &[])
                .pop()
                .expect("top")
        };
        let (u, flat) = (top(&along), top(&plain));
        let stats = |runs: &[Run]| {
            let mut c = thread_per_cell(runs);
            let empty = c.iter().filter(|&&v| v == 0.0).count();
            c.sort_by(f64::total_cmp);
            (empty, c[c.len() / 10], c[c.len() / 2])
        };
        let ((empty, p10, median), (_, flat_p10, flat_median)) = (stats(&u), stats(&flat));
        let spacing = 10.0 / along.density;
        let (_, guide) =
            crate::direction::guided_lines(std::slice::from_ref(&square), &[], &curve, spacing)
                .expect("guide");
        let rows = row_stitches(&u, &guide, spacing);
        let row_crossings = (0..rows.len())
            .flat_map(|i| (i + 1..rows.len()).map(move |j| (i, j)))
            .filter(|&(i, j)| segments_cross(rows[i].0, rows[i].1, rows[j].0, rows[j].1))
            .count();
        assert_eq!(empty, 0, "{kind:?}: {empty} empty 1 mm cells");
        // Within 3 %: rows across the 1 mm grid at an angle lay a little more or less thread in
        // each cell than rows along it (the plain fill's), about ±7 % cell to cell.
        assert!(
            p10 >= 0.97 * flat_p10 && median >= 0.97 * flat_median,
            "{kind:?}: p10 {p10:.2} / median {median:.2} against {flat_p10:.2} / {flat_median:.2} without the curve"
        );
        assert!(rows.len() > 300, "{kind:?}: {} row stitches", rows.len());
        // Plain fill's rows never cross. Programmable fill's tiles cross once, on the curve's
        // outer side below the U (about 65, 87 mm), which the rows inside the U do not reach.
        // The other crossings are travel between rows, sewn over them (#947).
        let most = if kind == RegionType::Fill { 0 } else { 1 };
        assert!(
            row_crossings <= most,
            "{kind:?}: {row_crossings} row crossings"
        );
    }
}

/// Length (0.1 mm) of `run`'s stitches whose midpoint lies within 0.15 mm of a stitch sewn more
/// than 200 stitches before: travel sewn on top of rows already sewn (#469).
fn sewn_on_top(run: &[Vec2]) -> f64 {
    let mut total = 0.0;
    for i in 1..run.len() {
        let mid = (run[i - 1] + run[i]) * 0.5;
        let hit = (1..i.saturating_sub(200)).any(|j| {
            let (a, b) = (run[j - 1], run[j]);
            let v = b - a;
            let l2 = v.x * v.x + v.y * v.y;
            let t = if l2 > 0.0 {
                (((mid.x - a.x) * v.x + (mid.y - a.y) * v.y) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            mid.distance(a + v * t) <= 1.5
        });
        if hit {
            total += run[i - 1].distance(run[i]);
        }
    }
    total
}

/// #469: a ∩ (a bar with two legs below it) sewn in horizontal rows has three sections: the
/// bar, joining the two legs. Inside sews the bar first, then a leg, and travels back over the
/// bar to the other leg; Inside, hidden under later sections sews each leg from its foot up to
/// the bar, then the bar, so that travel runs where the bar is still to be sewn.
#[test]
fn a_section_joining_others_is_sewn_after_them() {
    // A ∩: a bar 60 × 15 mm at the top, legs 15 mm wide and 45 mm tall at each end.
    let fork = Contour::new(
        vec![
            v2(0.0, 600.0),
            v2(150.0, 600.0),
            v2(150.0, 150.0),
            v2(450.0, 150.0),
            v2(450.0, 600.0),
            v2(600.0, 600.0),
            v2(600.0, 0.0),
            v2(0.0, 0.0),
        ],
        true,
    );
    let sewn = |path: RunningStitchPath| {
        let mut r = region(RegionType::Fill);
        r.angle = 0.0;
        r.underlay = false;
        r.density = 4.5;
        r.stitch_length = 6.5;
        r.running_stitch_path = Some(path);
        let run: Vec<Vec2> = crate::region::layers(std::slice::from_ref(&fork), &r, &[])
            .pop()
            .expect("top")
            .into_iter()
            .flatten()
            .collect();
        (sewn_on_top(&run), run.len())
    };
    let (inside, n_inside) = sewn(RunningStitchPath::Inside);
    let (hidden, n_hidden) = sewn(RunningStitchPath::InsideHidden);
    // Inside: 52 mm, the travel from the left leg's foot up its inner edge and along under the
    // bar sewn first. Hidden: that travel is gone; what is left (32 mm) is the bar's last row
    // laid along the legs' first rows at the seam, a fill row, not travel.
    assert!(
        inside > 400.0,
        "Inside travels over the sewn bar: {inside:.0}"
    );
    assert!(
        hidden < inside * 0.7,
        "hidden {hidden:.0} against inside {inside:.0}"
    );
    // The same rows, sewn in another order: the stitch count stays close.
    assert!(
        (n_hidden as f64 - n_inside as f64).abs() <= n_inside as f64 * 0.1,
        "{n_hidden} against {n_inside} stitches"
    );
}

/// #469: a ring (a hoop round a hole) sewn in horizontal rows has four sections that touch in a
/// ring: the top, both sides and the bottom. None joins the others, and going round sews them
/// with the least travel, so Inside, hidden under later sections sews it exactly as Inside.
#[test]
fn a_ring_of_sections_keeps_its_order() {
    let circle = |r: f64| {
        Contour::new(
            (0..72)
                .map(|i| {
                    let a = std::f64::consts::TAU * f64::from(i) / 72.0;
                    v2(
                        300.0 + r * crate::fmath::Det::dcos(a),
                        300.0 + r * crate::fmath::Det::dsin(a),
                    )
                })
                .collect(),
            true,
        )
    };
    let (outer, hole) = (circle(300.0), circle(150.0));
    let sewn = |path: RunningStitchPath| {
        let mut r = region(RegionType::Fill);
        r.angle = 0.0;
        r.underlay = false;
        r.density = 4.5;
        r.stitch_length = 6.5;
        r.running_stitch_path = Some(path);
        crate::region::layers(
            std::slice::from_ref(&outer),
            &r,
            std::slice::from_ref(&hole),
        )
    };
    assert_eq!(
        sewn(RunningStitchPath::InsideHidden),
        sewn(RunningStitchPath::Inside)
    );
}

/// #469's "best of": a hoop with a stem hanging below it (a ring with a branch) and the ∩ are
/// each sewn in whichever candidate order lays the least travel on sewn thread, never more
/// than Inside and never with more trims (gaps over 4 mm) than Inside.
#[test]
fn a_hidden_fill_never_lays_more_travel_on_top_than_inside() {
    let circle = |cx: f64, cy: f64, r: f64| {
        Contour::new(
            (0..72)
                .map(|i| {
                    let a = std::f64::consts::TAU * f64::from(i) / 72.0;
                    v2(
                        cx + r * crate::fmath::Det::dcos(a),
                        cy + r * crate::fmath::Det::dsin(a),
                    )
                })
                .collect(),
            true,
        )
    };
    // The hoop's outline with a 10 mm stem below it, as one polygon: the stem's sides meet the
    // circle at its bottom.
    let mut hoop: Vec<Vec2> = circle(300.0, 300.0, 300.0).points;
    let bottom = hoop
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.y.total_cmp(&b.1.y))
        .map(|(i, _)| i)
        .expect("points");
    hoop.splice(
        bottom - 1..=bottom + 1,
        [
            v2(350.0, 595.0),
            v2(350.0, 900.0),
            v2(250.0, 900.0),
            v2(250.0, 595.0),
        ],
    );
    let shapes = [
        (Contour::new(hoop, true), vec![circle(300.0, 300.0, 150.0)]),
        (
            Contour::new(
                vec![
                    v2(0.0, 600.0),
                    v2(150.0, 600.0),
                    v2(150.0, 150.0),
                    v2(450.0, 150.0),
                    v2(450.0, 600.0),
                    v2(600.0, 600.0),
                    v2(600.0, 0.0),
                    v2(0.0, 0.0),
                ],
                true,
            ),
            vec![],
        ),
    ];
    for (k, (outline, holes)) in shapes.iter().enumerate() {
        let sewn = |path: RunningStitchPath| {
            let mut r = region(RegionType::Fill);
            r.angle = 0.0;
            r.underlay = false;
            r.density = 4.5;
            r.stitch_length = 6.5;
            r.running_stitch_path = Some(path);
            let top = crate::region::layers(std::slice::from_ref(outline), &r, holes)
                .pop()
                .expect("top");
            (
                crate::region::travel_on_top(&top),
                crate::region::trims(&top),
            )
        };
        let (inside, runs_inside) = sewn(RunningStitchPath::Inside);
        let (hidden, runs_hidden) = sewn(RunningStitchPath::InsideHidden);
        assert!(
            hidden <= inside,
            "shape {k}: {hidden:.0} against {inside:.0}"
        );
        assert!(
            runs_hidden <= runs_inside,
            "shape {k}: {runs_hidden} trims against {runs_inside}"
        );
    }
}

/// #469: the on-top measure counts travel along a row sewn long before, not travel beside it,
/// nor rows (long stitches) laid over earlier travel.
#[test]
fn travel_on_top_counts_travel_over_old_rows() {
    // 600 short stitches along y = 0, then travel back along it 2 mm away and on it: on it, the
    // travel counts once it is more than 200 stitches past the row stitch below it.
    let row: Vec<Vec2> = (0..=600).map(|i| v2(f64::from(i) * 5.0, 0.0)).collect();
    let back = |y: f64| -> Vec<Vec2> {
        (0..=120)
            .map(|i| v2(3000.0 - f64::from(i) * 25.0, y))
            .collect()
    };
    let mut beside = row.clone();
    beside.extend(back(20.0));
    let mut over = row.clone();
    over.extend(back(0.0));
    assert_eq!(crate::region::travel_on_top(&[beside]), 0.0);
    let on = crate::region::travel_on_top(&[over]);
    assert!(on > 1000.0, "{on:.0}");
    // Long row stitches (6.5 mm) laid back over it are rows, not travel.
    let mut rows = row;
    rows.extend((0..=46).map(|i| v2(3000.0 - f64::from(i) * 65.0, 0.0)));
    assert_eq!(crate::region::travel_on_top(&[rows]), 0.0);
}
