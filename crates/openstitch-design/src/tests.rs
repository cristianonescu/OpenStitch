use crate::Compiler;
use crate::generate::{self, RegionParams};
use crate::geometry::{self, Contour, Transform, v2};
use crate::model::*;
use openstitch_core::{Command, StitchPattern, Thread};

const RED: Thread = Thread::rgb(0xED171F);
const BLUE: Thread = Thread::rgb(0x0A55A3);

fn params(angle: f64, spacing: f64, len: f64, pull: f64) -> RegionParams {
    RegionParams {
        angle,
        row_spacing: spacing,
        stitch_length: len,
        pull_compensation: pull,
    }
}

#[test]
fn scanlines_of_square_are_evenly_spaced() {
    let rows = geometry::scan_rows(
        &[Contour::rectangle(0.0, 0.0, 100.0, 100.0)],
        0.0,
        10.0,
        &[],
    );
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0].y, 5.0);
    assert!(rows.iter().all(|r| r.spans == vec![0.0..=100.0]));
}

#[test]
fn hole_contour_splits_rows() {
    let outer = Contour::rectangle(0.0, 0.0, 100.0, 100.0);
    let hole = Contour::rectangle(40.0, 40.0, 20.0, 20.0);
    let rows = geometry::scan_rows(&[outer.clone(), hole.clone()], 0.0, 10.0, &[]);
    let middle = rows.iter().find(|r| r.y == 45.0).expect("row at 45");
    assert_eq!(middle.spans, vec![0.0..=40.0, 60.0..=100.0]);
    assert!(!geometry::contains(
        v2(50.0, 50.0),
        &[outer.clone(), hole.clone()]
    ));
    assert!(geometry::contains(v2(10.0, 50.0), &[outer, hole]));
}

#[test]
fn span_subtraction() {
    assert_eq!(
        geometry::subtract(&[2.0..=3.0, 5.0..=20.0], &[0.0..=10.0]),
        vec![0.0..=2.0, 3.0..=5.0]
    );
    assert_eq!(geometry::subtract(&[], &[0.0..=10.0]), vec![0.0..=10.0]);
    assert!(geometry::subtract(&[-5.0..=15.0], &[0.0..=10.0]).is_empty());
}

#[test]
fn rotated_rows_map_back_inside() {
    let rows = geometry::scan_rows(
        &[Contour::rectangle(0.0, 0.0, 100.0, 100.0)],
        45.0,
        10.0,
        &[],
    );
    for r in &rows {
        for s in &r.spans {
            for x in [*s.start(), *s.end()] {
                let p = geometry::from_rotated(v2(x, r.y), 45.0);
                assert!(
                    p.x > -1e-3 && p.x < 100.001 && p.y > -1e-3 && p.y < 100.001,
                    "{p:?}"
                );
            }
        }
    }
}

#[test]
fn transforms() {
    let r = Transform::rotation(90.0, v2(0.0, 0.0)).apply(v2(10.0, 0.0));
    assert!(r.x.abs() < 1e-9 && (r.y - 10.0).abs() < 1e-9);
    assert_eq!(
        Transform::scaling(2.0, 3.0, v2(10.0, 10.0)).apply(v2(20.0, 20.0)),
        v2(30.0, 40.0)
    );
}

#[test]
fn running_keeps_corners_and_pitch() {
    let run = generate::running(&Contour::rectangle(0.0, 0.0, 100.0, 50.0), 25.0, 1);
    for c in [v2(0.0, 0.0), v2(100.0, 0.0), v2(100.0, 50.0), v2(0.0, 50.0)] {
        assert!(run.contains(&c));
    }
    assert_eq!(run.first(), run.last());
    assert!(run.windows(2).all(|w| w[0].distance(w[1]) <= 25.0 + 1e-9));
    assert_eq!(run.len(), 13);
}

#[test]
fn running_passes_and_triple() {
    let line = Contour::new(vec![v2(0.0, 0.0), v2(100.0, 0.0)], false);
    let xs: Vec<f64> = generate::running(&line, 50.0, 3)
        .iter()
        .map(|p| p.x)
        .collect();
    assert_eq!(xs, [0.0, 50.0, 100.0, 50.0, 0.0, 50.0, 100.0]);
    let short = Contour::new(vec![v2(0.0, 0.0), v2(60.0, 0.0)], false);
    let xs: Vec<f64> = generate::triple(&short, 30.0).iter().map(|p| p.x).collect();
    assert_eq!(xs, [0.0, 30.0, 0.0, 30.0, 60.0, 30.0, 60.0]);
}

#[test]
fn zigzag_alternates_sides() {
    let run = generate::zigzag(
        &Contour::new(vec![v2(0.0, 0.0), v2(100.0, 0.0)], false),
        20.0,
        5.0,
    );
    assert_eq!(run.len(), 21);
    assert!(
        run.windows(2)
            .all(|w| w[0].y == -w[1].y && w[0].y.abs() == 10.0)
    );
}

#[test]
fn fill_inside_rows_bounded_and_staggered() {
    let runs = generate::fill(
        &[Contour::rectangle(0.0, 0.0, 200.0, 100.0)],
        &params(0.0, 4.0, 40.0, 0.0),
        &[],
    );
    assert_eq!(runs.len(), 1);
    let pts: Vec<_> = runs.concat();
    assert!(
        pts.iter()
            .all(|p| p.x >= -1e-9 && p.x <= 200.0 + 1e-9 && (0.0..=100.0).contains(&p.y))
    );
    let mut ys: Vec<i64> = pts.iter().map(|p| (p.y * 1000.0) as i64).collect();
    ys.dedup();
    ys.sort();
    ys.dedup();
    assert_eq!(ys.len(), 25);
    assert!(
        pts.windows(2)
            .filter(|w| w[0].y == w[1].y)
            .all(|w| w[0].distance(w[1]) <= 40.0 + 1e-9)
    );
    let row = |y: f64| -> Vec<f64> {
        pts.iter()
            .filter(|p| p.y == y)
            .map(|p| p.x)
            .filter(|x| *x != 0.0 && *x != 200.0)
            .collect()
    };
    let (r0, r1) = (row(2.0), row(6.0));
    assert!(r0.iter().all(|x| !r1.contains(x)));
}

#[test]
fn separated_parts_become_separate_runs() {
    let shape = [
        Contour::rectangle(0.0, 0.0, 50.0, 50.0),
        Contour::rectangle(200.0, 0.0, 50.0, 50.0),
    ];
    assert_eq!(
        generate::fill(&shape, &params(0.0, 4.0, 30.0, 0.0), &[]).len(),
        2
    );
}

#[test]
fn satin_one_needle_point_per_row_with_pull_compensation() {
    let runs = generate::satin(
        &[Contour::rectangle(0.0, 0.0, 40.0, 20.0)],
        &params(90.0, 4.0, 40.0, 2.0),
        &[],
    );
    let pts: Vec<_> = runs.concat();
    // Ten rows: the first sewn across, then one needle point per row on alternate edges.
    assert_eq!(pts.len(), 11);
    for w in pts.windows(2) {
        assert!(
            ((w[0].y - w[1].y).abs() - 24.0).abs() < 1e-9,
            "{w:?} runs along an edge"
        );
        assert!((w[0].x - w[1].x).abs() <= 4.0 + 1e-9);
    }
    let ys: Vec<f64> = pts
        .iter()
        .map(|p| (p.y * 1000.0).round() / 1000.0)
        .collect();
    assert_eq!(ys.iter().cloned().fold(f64::INFINITY, f64::min), -2.0);
    assert_eq!(ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 22.0);
}

#[test]
fn deterministic() {
    let shape = [Contour::ellipse(v2(100.0, 100.0), 80.0, 50.0, 72)];
    let p = params(30.0, 4.0, 35.0, 1.0);
    assert_eq!(
        generate::fill(&shape, &p, &[]),
        generate::fill(&shape, &p, &[])
    );
}

fn two_squares() -> Design {
    let mut d = Design::new("two", DesignPage::default());
    d.add(DesignObject::shape(
        vec![Contour::rectangle(0.0, 0.0, 100.0, 100.0)],
        Some(RegionSewing::new(RegionType::Fill, RED)),
        None,
    ));
    d.add(DesignObject::shape(
        vec![Contour::rectangle(300.0, 0.0, 100.0, 100.0)],
        Some(RegionSewing::new(RegionType::Fill, BLUE)),
        Some(LineSewing::new(LineType::Running, BLUE)),
    ));
    d
}

#[test]
fn compiler_colour_changes_trims_end_and_max_stitch() {
    let p = Compiler::default().compile(&two_squares());
    assert_eq!(p.threads, vec![RED, BLUE]);
    assert_eq!(p.count(Command::ColorChange), 1);
    assert_eq!(p.stitches.last().map(|s| s.command), Some(Command::End));
    assert_eq!(p.stitches.first().map(|s| s.command), Some(Command::Jump));
    assert!(p.count(Command::Trim) >= 2);
    let max = p
        .stitches
        .windows(2)
        .filter(|w| w[1].command == Command::Stitch)
        .map(|w| v2(w[0].x, w[0].y).distance(v2(w[1].x, w[1].y)))
        .fold(0.0, f64::max);
    assert!(max <= 120.0 + 1e-9);
}

#[test]
fn short_moves_between_objects_are_sewn_across_and_longer_ones_trimmed() {
    // PE-DESIGN 11.50 sews a 2.24 mm needle-to-needle move between same-colour parts as a
    // plain stitch and trims a 2.33 mm one (compat machine-trim-fine, machine-trim-bound).
    let squares = |gap: f64| {
        let mut d = Design::new("gap", DesignPage::default());
        for x in [0.0, 100.0 + gap] {
            let mut fill = RegionSewing::new(RegionType::Fill, RED);
            fill.underlay = false;
            // Entry on the left edge, exit on the right: the move between the squares is the gap.
            fill.entry = Some(v2(x, 50.0));
            fill.exit = Some(v2(x + 100.0, 50.0));
            d.add(DesignObject::shape(
                vec![Contour::rectangle(x, 0.0, 100.0, 100.0)],
                Some(fill),
                None,
            ));
        }
        d
    };
    for (gap, trims) in [(10.0, 0), (22.0, 0), (24.0, 1), (60.0, 1)] {
        let p = Compiler::default().compile(&squares(gap));
        assert_eq!(p.count(Command::ColorChange), 0);
        // One trim at the end of the design, plus the one between the squares.
        assert_eq!(p.count(Command::Trim), trims + 1, "gap {gap}");
        let interior_jumps = p
            .stitches
            .iter()
            .skip(1)
            .filter(|s| s.command == Command::Jump)
            .count();
        assert_eq!(
            interior_jumps, trims,
            "gap {gap}: a short move is a stitch, not a jump"
        );
    }
}

#[test]
fn no_zero_length_stitch_where_a_jump_lands_on_a_custom_size_page() {
    // On a Custom Size page the jump's end is the first needle point: PE-DESIGN 11 writes no
    // zero-length stitch there (every reference saved on one; compat pes-default-v6: 932 → 930
    // stitches). On a Hoop Size page it starts each one-step tie-in on a stitch in place (#1013).
    let mut custom = two_squares();
    custom.machine.custom_page = true;
    let p = Compiler::default().compile(&custom);
    assert!(p.count(Command::Jump) >= 2);
    for w in p.stitches.windows(2) {
        if w[0].command == Command::Jump && w[1].command == Command::Stitch {
            assert_ne!((w[0].x, w[0].y), (w[1].x, w[1].y), "{w:?}");
        }
    }
    let hoop = two_squares();
    assert!(hoop.hoop_size_page());
    let p = Compiler::default().compile(&hoop);
    let in_place = p
        .stitches
        .windows(2)
        .filter(|w| {
            w[0].command == Command::Jump
                && w[1].command == Command::Stitch
                && (w[0].x, w[0].y) == (w[1].x, w[1].y)
        })
        .count();
    assert!(in_place >= 1, "a stitch in place where a jump lands");
    assert_eq!(
        p.count(Command::Stitch),
        Compiler::default().compile(&custom).count(Command::Stitch) + in_place
    );
}

#[test]
fn hidden_objects_are_not_sewn() {
    let mut d = two_squares();
    d.objects[1].visible = false;
    assert_eq!(Compiler::default().compile(&d).threads, vec![RED]);
}

#[test]
fn imported_stitches_keep_colour_blocks() {
    let mut p = StitchPattern {
        threads: vec![RED, BLUE],
        ..Default::default()
    };
    p.stitch_to(0.0, 0.0);
    p.stitch_to(10.0, 0.0);
    p.push(Command::ColorChange);
    p.stitch_to(10.0, 10.0);
    p.stitch_to(0.0, 10.0);
    p.push(Command::End);
    let mut d = Design::default();
    d.add(DesignObject::stitches(StitchBlock::from(&p)));
    let out = Compiler::default().compile(&d);
    assert_eq!(out.threads, vec![RED, BLUE]);
    // Each block's first point is where its jump lands, not a zero-length stitch after it
    // (PE-DESIGN 11 counts it the same way, compat pes-default-v6); every point is still there.
    assert_eq!(out.count(Command::Stitch), 2);
    let points: Vec<(f64, f64)> = out
        .stitches
        .iter()
        .filter(|s| matches!(s.command, Command::Stitch | Command::Jump))
        .map(|s| (s.x, s.y))
        .collect();
    assert_eq!(points, [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
}

#[test]
fn empty_design_compiles_to_nothing() {
    assert!(
        Compiler::default()
            .compile(&Design::default())
            .stitches
            .is_empty()
    );
}

#[test]
fn hole_sewing_skips_upper_shape() {
    let mut d = Design::default();
    let base = d.add(DesignObject::shape(
        vec![Contour::rectangle(0.0, 0.0, 200.0, 200.0)],
        Some(RegionSewing::new(RegionType::Fill, RED)),
        None,
    ));
    d.add(DesignObject::shape(
        vec![Contour::rectangle(50.0, 50.0, 100.0, 100.0)],
        Some(RegionSewing::new(RegionType::Fill, BLUE)),
        None,
    ));
    let inside =
        |s: &openstitch_core::Stitch| s.x > 51.0 && s.x < 149.0 && s.y > 51.0 && s.y < 149.0;
    let first_block = |d: &Design| -> Vec<openstitch_core::Stitch> {
        Compiler::default().compile(d).color_blocks()[0]
            .1
            .iter()
            .filter(|s| s.command == Command::Stitch)
            .copied()
            .collect()
    };
    let plain = first_block(&d);
    assert!(plain.iter().any(inside));
    let i = d.index_of(base).expect("base");
    d.objects[i].hole_sewing = true;
    let holed = first_block(&d);
    assert!(!holed.iter().any(inside));
    assert!(holed.len() < plain.len());
}

#[test]
fn design_json_uses_osd_field_names() {
    let json = serde_json::to_value(two_squares()).expect("serialize");
    assert_eq!(json["nextObjectID"], 3);
    assert_eq!(json["objects"][0]["region"]["type"], "fill");
    assert_eq!(json["objects"][0]["region"]["stitchLength"], 4.0);
    let back: Design = serde_json::from_value(json).expect("deserialize");
    assert_eq!(back, two_squares());
}

// MARK: Bézier segments

/// Circle of radius `r` as four cubic segments (handle length k·r, k = 0.5523).
fn bezier_circle(c: crate::Vec2, r: f64) -> Contour {
    use crate::Bezier;
    let k = 0.552_284_75 * r;
    let pts = vec![
        v2(c.x + r, c.y),
        v2(c.x, c.y + r),
        v2(c.x - r, c.y),
        v2(c.x, c.y - r),
    ];
    let mut contour = Contour::new(pts, true);
    let handles = [
        (v2(c.x + r, c.y + k), v2(c.x + k, c.y + r)),
        (v2(c.x - k, c.y + r), v2(c.x - r, c.y + k)),
        (v2(c.x - r, c.y - k), v2(c.x - k, c.y - r)),
        (v2(c.x + k, c.y - r), v2(c.x + r, c.y - k)),
    ];
    for (i, (c1, c2)) in handles.into_iter().enumerate() {
        contour.set_curve(i, Some(Bezier { c1, c2 }));
    }
    contour
}

#[test]
fn curve_halving_has_no_chord_ceiling_and_keeps_equal_parameters() {
    use crate::generate::halve_curve;
    // A quarter arc of an 800 mm circle (0.1 mm units) at a 2 mm pitch: 2^8 chords of 2.45 mm
    // were the old ceiling; now the halving goes on until every chord fits.
    let k = 0.552_284_75 * 4000.0;
    let (a, c1, c2, b) = (
        v2(4000.0, 0.0),
        v2(4000.0, k),
        v2(k, 4000.0),
        v2(0.0, 4000.0),
    );
    let pts = halve_curve(a, c1, c2, b, 20.0);
    assert_eq!(pts.len(), 512 + 1);
    assert!(pts.windows(2).all(|w| w[0].distance(w[1]) <= 20.0 + 1e-9));
    assert!(
        pts.windows(2).all(|w| w[0].distance(w[1]) > 10.0),
        "halving, not flattening"
    );
    // Equal parameter steps, as PE-DESIGN's circle: point k lies at t = k / n.
    let at = |t: f64| {
        let u = 1.0 - t;
        a * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + b * (t * t * t)
    };
    for (i, p) in pts.iter().enumerate() {
        assert!(p.distance(at(i as f64 / 512.0)) < 1e-6, "{i}");
    }
    // The 60 mm circle of text-fit-default keeps its 4 × 32 chords.
    let k = 0.552_284_75 * 300.0;
    let small = halve_curve(
        v2(300.0, 0.0),
        v2(300.0, k),
        v2(k, 300.0),
        v2(0.0, 300.0),
        20.0,
    );
    assert_eq!(small.len(), 33);
}

#[test]
fn bezier_circle_flattens_within_tolerance() {
    let centre = v2(500.0, 500.0);
    let circle = bezier_circle(centre, 200.0);
    let path = circle.path();
    assert!(path.len() > 20, "curves are subdivided");
    assert_eq!(path.first(), path.last());
    for w in path.windows(2) {
        let mid = w[0].lerp(w[1], 0.5);
        for q in [w[0], mid] {
            assert!((q.distance(centre) - 200.0).abs() < 0.6, "{q:?}");
        }
    }
}

#[test]
fn straight_contours_are_unchanged_and_serialize_without_curves() {
    let r = Contour::rectangle(0.0, 0.0, 10.0, 10.0);
    assert_eq!(r.path().len(), 5);
    let json = serde_json::to_value(&r).expect("json");
    assert!(json.get("curves").is_none());
    let mut curved = r.clone();
    curved.set_curve(
        1,
        Some(crate::Bezier {
            c1: v2(15.0, 3.0),
            c2: v2(15.0, 7.0),
        }),
    );
    let json = serde_json::to_value(&curved).expect("json");
    assert_eq!(json["curves"].as_array().map(Vec::len), Some(4));
    let back: Contour = serde_json::from_value(json).expect("back");
    assert_eq!(back, curved);
    curved.set_curve(1, None);
    assert!(
        curved.curves.is_empty(),
        "clearing the last curve empties the list"
    );
}

#[test]
fn bounds_include_curve_bulge() {
    let mut c = Contour::new(vec![v2(0.0, 0.0), v2(100.0, 0.0)], false);
    c.set_curve(
        0,
        Some(crate::Bezier {
            c1: v2(0.0, -60.0),
            c2: v2(100.0, -60.0),
        }),
    );
    let o = DesignObject::shape(vec![c], None, None);
    let b = o.bounds().expect("bounds");
    assert!((b.min_y + 45.0).abs() < 0.6, "{}", b.min_y);
}

#[test]
fn running_on_curves_keeps_vertices_and_pitch() {
    let circle = bezier_circle(v2(0.0, 0.0), 200.0);
    let run = generate::running(&circle, 25.0, 1);
    for v in &circle.points {
        assert!(run.contains(v), "vertex {v:?} kept");
    }
    assert!(run.windows(2).all(|w| w[0].distance(w[1]) <= 25.0 + 0.5));
    // Each quarter arc (314 mm / 10) is halved until its chords fit the 2.5 mm pitch: 16
    // chords, as PE-DESIGN samples a circle (4 × 32 on its 60 mm circle at 2.0 mm).
    assert_eq!(run.len() - 1, 4 * 16, "{} stitches", run.len() - 1);
    let triple = generate::triple(&circle, 25.0);
    assert_eq!(triple.len(), 1 + 3 * (run.len() - 1));
}

#[test]
fn fill_stays_inside_curved_region() {
    let circle = bezier_circle(v2(300.0, 300.0), 150.0);
    let p = params(30.0, 4.0, 35.0, 0.0);
    let pts: Vec<_> = generate::fill(std::slice::from_ref(&circle), &p, &[]).concat();
    assert!(!pts.is_empty());
    assert!(
        pts.iter()
            .all(|q| q.distance(v2(300.0, 300.0)) <= 150.0 + 0.6)
    );
}

#[test]
fn insert_point_splits_curves_exactly() {
    let mut c = Contour::new(vec![v2(0.0, 0.0), v2(100.0, 0.0)], false);
    let b = crate::Bezier {
        c1: v2(0.0, -60.0),
        c2: v2(100.0, -60.0),
    };
    c.set_curve(0, Some(b));
    let before = c.path();
    let i = c.insert_point(0, 0.5);
    assert_eq!(i, 1);
    assert_eq!(c.points.len(), 3);
    assert!((c.points[1].x - 50.0).abs() < 1e-9 && (c.points[1].y + 45.0).abs() < 1e-9);
    assert!(c.curve(0).is_some() && c.curve(1).is_some());
    // Every new flattened point lies on the original curve (within flattening tolerance).
    for q in c.path() {
        let nearest = before
            .windows(2)
            .map(|w| seg_dist(q, w[0], w[1]))
            .fold(f64::INFINITY, f64::min);
        assert!(nearest < 1.0, "{q:?} off curve by {nearest}");
    }
    let mut straight = Contour::rectangle(0.0, 0.0, 100.0, 100.0);
    assert_eq!(straight.insert_point(3, 0.25), 4);
    assert_eq!(straight.points[4], v2(0.0, 75.0));
    assert!(straight.curves.is_empty());
}

fn seg_dist(p: crate::Vec2, a: crate::Vec2, b: crate::Vec2) -> f64 {
    let ab = b - a;
    let t = (((p - a).x * ab.x + (p - a).y * ab.y) / (ab.x * ab.x + ab.y * ab.y)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

#[test]
fn delete_point_keeps_curve_bookkeeping() {
    let mut circle = bezier_circle(v2(0.0, 0.0), 100.0);
    circle.delete_point(0);
    assert_eq!(circle.points.len(), 3);
    assert_eq!(circle.segment_count(), 3);
    assert_eq!(circle.curves.len(), 3);
    assert!(circle.curves.iter().all(Option::is_some));
    // The joined closing segment keeps the outer handles of its two neighbours.
    let joined = circle.curve(2).expect("closing curve");
    let k = 0.552_284_75 * 100.0;
    // Removed vertex (r, 0) sat between handles (k, -r) and (k, r); the outer ones remain.
    assert!(joined.c1.distance(v2(k, -100.0)) < 1e-9, "{joined:?}");
    assert!(joined.c2.distance(v2(k, 100.0)) < 1e-9, "{joined:?}");
    let mut square = Contour::rectangle(0.0, 0.0, 10.0, 10.0);
    square.delete_point(2);
    assert_eq!(
        square.points,
        vec![v2(0.0, 0.0), v2(10.0, 0.0), v2(0.0, 10.0)]
    );
    assert!(square.curves.is_empty());
}

#[test]
fn transform_moves_handles() {
    let c = bezier_circle(v2(0.0, 0.0), 10.0).transformed(&Transform::translation(5.0, 0.0));
    let b = c.curve(0).expect("curve");
    assert!((b.c1.x - 15.0).abs() < 1e-9);
}

#[test]
fn programmable_fill_sews_embossed_and_engraved_tile_areas() {
    use crate::patterns::{PatternRegion, RegionEffect, parse};
    let tile = |regions: &[PatternRegion]| {
        let mut p = parse(
            r#"{"format": "openstitch.pattern", "version": 1, "kind": "fill", "id": "strips",
            "name": "Strips", "paths": [[[0, 0.25], [1, 0.25]], [[0, 0.75], [1, 0.75]]]}"#,
        )
        .expect("valid");
        p.regions = regions.to_vec();
        p
    };
    let sew = |regions: &[PatternRegion]| {
        let mut d = Design::new(
            "t",
            DesignPage {
                width: 600.0,
                height: 400.0,
            },
        );
        let mut fill = RegionSewing::new(RegionType::ProgrammableFill, Thread::rgb(0));
        fill.pattern = Some("strips".into());
        fill.pattern_width = Some(24.0);
        fill.pattern_height = Some(24.0);
        fill.stitch_length = 2.0;
        d.add(DesignObject::shape(
            vec![Contour::rectangle(20.0, 20.0, 560.0, 360.0)],
            Some(fill),
            None,
        ));
        d.patterns.push(tile(regions));
        Compiler::default().compile(&d).count(Command::Stitch)
    };
    let at = |effect| PatternRegion {
        effect,
        at: v2(0.5, 0.5),
    };
    let plain = sew(&[]);
    let embossed = sew(&[at(RegionEffect::Emboss)]);
    let engraved = sew(&[at(RegionEffect::Engrave)]);
    // PE-DESIGN's programmable fill has no needle points between the pattern lines, so an
    // embossed area (no needle points inside) never adds any.
    assert!(embossed <= plain, "{embossed} vs {plain}");
    assert!(engraved > plain * 11 / 10, "{engraved} vs {plain}");
}

/// Longest row of consecutive stitches shorter than `limit` (0.1 mm).
fn longest_short_run(runs: &[generate::Run], limit: f64) -> usize {
    let mut best = 0;
    for run in runs {
        let mut n = 0;
        for w in run.windows(2) {
            n = if w[0].distance(w[1]) < limit {
                n + 1
            } else {
                0
            };
            best = best.max(n);
        }
    }
    best
}

#[test]
fn fill_rows_in_a_sliver_are_joined() {
    // #303: a 20 × 10 mm fill with a 0.35 mm wide, 5 mm tall sliver on its top edge (the
    // notch an Auto Punch outline left on the storybook picture) and a hole, so that rows have
    // two spans and are chained row by row, as the storybook's white region is. Each row
    // through the sliver is a 0.35 mm span, the next one 0.22 mm on: rounded to the PEC's 0.1 mm
    // grid, stitch after stitch there is under 0.3 mm.
    let notched = [
        Contour::new(
            vec![
                v2(0.0, 0.0),
                v2(100.0, 0.0),
                v2(100.0, -50.0),
                v2(103.5, -50.0),
                v2(103.5, 0.0),
                v2(200.0, 0.0),
                v2(200.0, 100.0),
                v2(0.0, 100.0),
            ],
            true,
        ),
        Contour::rectangle(40.0, 30.0, 40.0, 40.0),
    ];
    let p = params(0.0, 10.0 / 4.5, 40.0, 0.0);
    let stagger = generate::Stagger::Regular(0.3);
    let entry = Some(v2(100.0, 100.0));
    let raw = generate::edge_to_edge_rows(&notched, &p, &[], stagger, entry, false);
    assert!(
        longest_short_run(&raw, 5.0) > 20,
        "{}",
        longest_short_run(&raw, 5.0)
    );
    let joined = generate::fill_edge_to_edge(&notched, &p, &[], stagger, entry, false);
    assert!(
        longest_short_run(&joined, 5.0) <= 6,
        "{}",
        longest_short_run(&joined, 5.0)
    );
    // The sliver is still sewn, up to its tip.
    let top = joined
        .concat()
        .iter()
        .map(|q| q.y)
        .fold(f64::INFINITY, f64::min);
    assert!(top < -45.0, "{top}");
    // A fill without slivers is left as it was, short corner rows and all.
    for angle in [0.0, 30.0, 45.0] {
        let square = [Contour::rectangle(0.0, 0.0, 400.0, 400.0)];
        let p = params(angle, 10.0 / 4.5, 40.0, 0.0);
        let entry = Some(v2(200.0, 400.0));
        assert_eq!(
            generate::fill_edge_to_edge(&square, &p, &[], stagger, entry, false),
            generate::edge_to_edge_rows(&square, &p, &[], stagger, entry, false),
            "{angle}°"
        );
    }
}

#[test]
fn fill_rows_take_the_nearest_whole_number_of_gaps() {
    // PE-DESIGN 11.50 at 4.5 rows/mm (#1131): 148.5 gaps over 33 mm sew as 148, 31.8 across a
    // 5 mm square at 45° as 32, 245.9 and 254.6 across a 40 mm square at 30° and 45° as 246
    // and 255.
    let gaps = |w: f64, h: f64, angle: f64| {
        let c = [Contour::rectangle(0.0, 0.0, w, h)];
        let p = params(angle, 10.0 / 4.5, 40.0, 0.0);
        let (_, gap) = generate::fill_row_frame(&c, &p).expect("rows");
        let height = crate::region::rotated_bounds(&c, angle)
            .expect("bounds")
            .height();
        (height / gap).round() as usize
    };
    assert_eq!(gaps(400.0, 330.0, 0.0), 148);
    assert_eq!(gaps(400.0, 400.0, 0.0), 180);
    assert_eq!(gaps(50.0, 50.0, -45.0), 32);
    assert_eq!(gaps(400.0, 400.0, -30.0), 246);
    assert_eq!(gaps(400.0, 400.0, -45.0), 255);
}

#[test]
fn a_fill_from_a_corner_keeps_the_edge_points_of_the_rows_sewn_back() {
    // PE-DESIGN 11.50's lone 5 mm square at 45° (fill-square-5mm-under-sewing, #1131), started
    // at the middle of its bottom edge: the top-left corner is one needle point, then every
    // other row runs from the top edge to the left edge with both its edge points.
    let square = [Contour::rectangle(0.0, 0.0, 50.0, 50.0)];
    let p = params(-45.0, 10.0 / 4.5, 40.0, 0.0);
    let stagger = generate::Stagger::Regular(0.3);
    let entry = Some(v2(25.0, 50.0));
    let run = generate::edge_to_edge_rows(&square, &p, &[], stagger, entry, false).concat();
    let near = |a: geometry::Vec2, b: geometry::Vec2| a.distance(b) < 0.02;
    let at = |q| run.iter().position(|r| near(*r, q));
    let count = |q| run.iter().filter(|r| near(**r, q)).count();
    let corner = at(v2(0.0, 0.0)).expect("the corner is sewn");
    assert_eq!(count(v2(0.0, 0.0)), 1);
    // 32 gaps: the rows are 3.125 apart along the edges.
    assert!(
        near(run[corner + 1], v2(6.25, 0.0)),
        "{:?}",
        run[corner + 1]
    );
    assert!(
        near(run[corner + 2], v2(0.0, 6.25)),
        "{:?}",
        run[corner + 2]
    );
    let fourth = at(v2(12.5, 0.0)).expect("row 4 starts on the top edge");
    assert!(
        near(run[fourth + 1], v2(0.0, 12.5)),
        "{:?}",
        run[fourth + 1]
    );
    // The row through the start point closes the first half only: sewn once, towards it.
    assert_eq!(count(v2(50.0, 25.0)), 1);
    // The far corner is one needle point too, and the second half ends on the row next to the
    // start point, one stitch from it.
    assert_eq!(count(v2(50.0, 50.0)), 1);
    let n = run.len();
    assert!(near(run[n - 2], v2(28.125, 50.0)), "{:?}", run[n - 2]);
    assert!(
        run[n - 1].distance(v2(25.0, 50.0)) < 1e-9,
        "{:?}",
        run[n - 1]
    );
}

#[test]
fn the_halves_of_a_fill_meet_at_its_end_point() {
    // PE-DESIGN 11.50, machine-jumps' squares (#1088): entered at the top-left corner and left
    // at the top-right one, the fill sews the corner half from where it is, ends it on the end
    // point, travels to the far corner and ends the other half there as well. The diagonal
    // through the end point closes both halves.
    let square = [Contour::rectangle(0.0, 0.0, 50.0, 50.0)];
    let p = params(-45.0, 10.0 / 4.5, 40.0, 0.0);
    let run = generate::fill_edge_to_edge_travel(
        &square,
        &p,
        &[],
        generate::Stagger::Regular(0.3),
        Some(v2(0.0, 0.0)),
        Some(v2(50.0, 0.0)),
        false,
        false,
        None,
    )
    .concat();
    let count = |q: geometry::Vec2| run.iter().filter(|r| r.distance(q) < 0.02).count();
    assert!(run[0].distance(v2(0.0, 0.0)) < 0.02, "{:?}", run[0]);
    let n = run.len();
    assert!(
        run[n - 1].distance(v2(50.0, 0.0)) < 1e-9,
        "{:?}",
        run[n - 1]
    );
    assert_eq!(count(v2(0.0, 0.0)), 1);
    assert_eq!(count(v2(50.0, 50.0)), 1);
    assert_eq!(count(v2(0.0, 50.0)), 2);
    // Without an end point both halves end on the start point, as before.
    let run = generate::fill_edge_to_edge_travel(
        &square,
        &p,
        &[],
        generate::Stagger::Regular(0.3),
        Some(v2(0.0, 0.0)),
        None,
        false,
        false,
        None,
    )
    .concat();
    let n = run.len();
    assert!(
        run[0].distance(run[n - 1]) < 0.02,
        "{:?} {:?}",
        run[0],
        run[n - 1]
    );
}

#[test]
fn a_type_1_fills_needle_grid_starts_at_the_origin_it_is_given() {
    // PE-DESIGN 11.50 lays the grid out from the origin of its page coordinates (#1131): row
    // 1's grid lies 2 × 30 % of a 4 mm stitch from it along the rows. Without an origin the
    // grid starts at the shape, and Stitch Type 2's does either way.
    let square = [Contour::rectangle(0.0, 0.0, 400.0, 400.0)];
    let p = params(0.0, 10.0 / 4.5, 40.0, 0.0);
    let fill = |sloped: bool, origin| {
        generate::fill_edge_to_edge_travel(
            &square,
            &p,
            &[],
            generate::Stagger::Regular(0.3),
            Some(v2(200.0, 400.0)),
            None,
            sloped,
            false,
            origin,
        )
        .concat()
    };
    let row_1 = |run: &[geometry::Vec2]| -> Vec<f64> {
        run.iter()
            .filter(|q| (q.y - 400.0 / 180.0).abs() < 0.01 && q.x > 1.0 && q.x < 399.0)
            .map(|q| q.x.rem_euclid(40.0))
            .collect()
    };
    let at_shape = row_1(&fill(false, None));
    assert!(at_shape.len() > 5, "{at_shape:?}");
    assert!(
        at_shape.iter().all(|x| (x - 24.0).abs() < 1e-6),
        "{at_shape:?}"
    );
    let moved = row_1(&fill(false, Some(v2(-3.0, 7.0))));
    assert!(moved.len() > 5, "{moved:?}");
    assert!(moved.iter().all(|x| (x - 21.0).abs() < 1e-6), "{moved:?}");
    assert_eq!(fill(true, Some(v2(-3.0, 7.0))), fill(true, None));
}

#[test]
fn join_short_stitches_keeps_short_corners_and_the_last_point() {
    // Six short stitches in a row stay (PE-DESIGN's most in its drawn fills).
    let corner: Vec<_> = (0..7)
        .map(|i| v2(f64::from(i % 2) * 2.0, f64::from(i) * 2.0))
        .chain([v2(40.0, 12.0)])
        .collect();
    let mut run = corner.clone();
    generate::join_short_stitches(&mut run);
    assert_eq!(run, corner);
    // Nine are joined into stitches of at least 0.5 mm, ending on the run's last point.
    let mut run: Vec<_> = (0..10)
        .map(|i| v2(f64::from(i % 2) * 2.0, f64::from(i) * 2.0))
        .collect();
    let last = run[run.len() - 1];
    generate::join_short_stitches(&mut run);
    assert_eq!(run.last(), Some(&last));
    assert_eq!(run[0], v2(0.0, 0.0));
    assert!(longest_short_run(&[run], 5.0) <= 1);
}

// MARK: point kinds (#1258)

#[test]
fn a_contour_without_kinds_reads_and_writes_as_before() {
    let old =
        r#"{"points":[{"x":0.0,"y":0.0},{"x":10.0,"y":0.0},{"x":10.0,"y":10.0}],"closed":true}"#;
    let c: Contour = serde_json::from_str(old).expect("an older file's contour");
    assert!(c.kinds.is_empty());
    assert_eq!(c.kind(1), crate::PointKind::Unset);
    let again = serde_json::to_string(&c).expect("json");
    assert!(!again.contains("kinds"), "{again}");
    // Storing no kind stores nothing; a kind is kept and read back.
    let mut c = bezier_circle(v2(0.0, 0.0), 100.0);
    c.set_kind(2, crate::PointKind::Unset);
    assert!(c.kinds.is_empty());
    c.set_kind(2, crate::PointKind::Symmetric);
    let json = serde_json::to_string(&c).expect("json");
    assert!(
        json.contains(r#""kinds":["unset","unset","symmetric","unset"]"#),
        "{json}"
    );
    let back: Contour = serde_json::from_str(&json).expect("back");
    assert_eq!(back, c);
    // A list of the wrong length is ignored, not misapplied.
    let mut odd = c.clone();
    odd.kinds.pop();
    assert_eq!(odd.kind(2), crate::PointKind::Unset);
}

#[test]
fn point_kinds_never_change_the_outline_on_their_own() {
    let plain = bezier_circle(v2(0.0, 0.0), 100.0);
    let mut tagged = plain.clone();
    for i in 0..4 {
        tagged.set_kind(i, crate::PointKind::Smooth);
    }
    assert_eq!(plain.path(), tagged.path());
    assert_eq!(plain.curves, tagged.curves);
}

/// PE-DESIGN 11.50's own rectangle of the #1258 captures (mm; `a1-to-curve.pes`): bottom
/// centre, bottom left, top left, top centre, top right, bottom right.
fn capture_rectangle() -> Contour {
    Contour::new(
        vec![
            v2(50.0, 70.5),
            v2(30.0, 70.5),
            v2(30.0, 30.5),
            v2(50.0, 30.5),
            v2(70.0, 30.5),
            v2(70.0, 70.5),
        ],
        true,
    )
}

fn near(a: Option<crate::Vec2>, x: f64, y: f64) -> bool {
    a.is_some_and(|a| a.distance(v2(x, y)) < 0.006)
}

/// To Curve as measured on PE-DESIGN 11.50 (2026-11-18): the handles lie at
/// `p ∓ (next − previous) / 4`, the point is smooth, its neighbours stay straight points.
#[test]
fn to_curve_places_the_handles_where_pe_design_does() {
    // a1-to-curve.pes: the top left corner, arriving (25, 40.5), leaving (35, 20.5).
    let mut c = capture_rectangle();
    assert!(!c.is_curve_point(2));
    assert!(c.make_curve(2));
    let (arriving, leaving) = c.point_handles(2);
    assert!(
        near(arriving, 25.0, 40.5) && near(leaving, 35.0, 20.5),
        "{arriving:?} {leaving:?}"
    );
    assert_eq!(c.kind(2), crate::PointKind::Smooth);
    // Both sides at the corner are curves; the far ends have no handle of their own, so
    // each side is drawn from the corner's handle alone (both control points on it).
    let (before, after) = (
        c.curve(1).expect("arriving side"),
        c.curve(2).expect("leaving side"),
    );
    assert_eq!((before.c1, after.c2), (before.c2, after.c1));
    assert!(c.borrows_handle(1, true) && c.borrows_handle(2, false));
    assert!(c.owns_handle(1, false) && c.owns_handle(2, true));
    assert!(c.curve(0).is_none() && c.curve(3).is_none());
    assert!(c.is_curve_point(2) && !c.is_curve_point(1) && !c.is_curve_point(3));
    assert!(!c.make_curve(2), "a curve point is left alone");
    // The outline bends at once. PE-DESIGN saved this shape's bounds as 26.2 mm on the left
    // and 23.0 mm at the top: the middle of each side, where its curve is furthest out.
    let middle = |s: usize| {
        let ((a, b), z) = (c.segment_ends(s), c.curve(s).expect("curve"));
        (a + z.c1 * 3.0 + z.c2 * 3.0 + b) * 0.125
    };
    assert!((middle(1).x - 26.25).abs() < 1e-9 && (middle(2).y - 23.0).abs() < 1e-9);
    // Moving the straight neighbour leaves the corner's handle (and its copy) where it is;
    // moving the corner takes both along.
    let mut moved = c.clone();
    moved.move_point(1, v2(28.0, 72.0));
    assert_eq!(moved.curve(1), c.curve(1));
    moved.move_point(2, v2(31.0, 31.5));
    let z = moved.curve(1).expect("curve");
    assert_eq!((z.c1, z.c2), (v2(26.0, 41.5), v2(26.0, 41.5)));

    // a8-two-to-curve.pes: two neighbouring corners at once, each from the straight outline.
    let mut two = Contour::new(
        vec![
            v2(50.0, 70.5),
            v2(30.0, 70.5),
            v2(30.0, 30.5),
            v2(70.0, 30.5),
            v2(70.0, 70.5),
        ],
        true,
    );
    assert!(two.make_curve(2) && two.make_curve(3));
    let (a2, l2) = two.point_handles(2);
    let (a3, l3) = two.point_handles(3);
    assert!(
        near(a2, 20.0, 40.5) && near(l2, 40.0, 20.5),
        "{a2:?} {l2:?}"
    );
    assert!(
        near(a3, 60.0, 20.5) && near(l3, 80.0, 40.5),
        "{a3:?} {l3:?}"
    );

    // b1-end-to-curve.pes: the first point of an open line; the line stays straight.
    let mut line = Contour::new(vec![v2(32.5, 57.0), v2(62.0, 57.0), v2(62.0, 27.0)], false);
    assert!(line.make_curve(0));
    assert!(near(line.point_handles(0).1, 39.875, 57.0));
    assert!(line.is_curve_point(0) && !line.is_curve_point(1));
    assert!(
        line.path()
            .iter()
            .all(|q| (q.y - 57.0).abs() < 1e-9 || q.x == 62.0)
    );
}

/// The captures' sequence on the corner: drag a smooth point's handle, To Symmetry, drag,
/// To Cusp, drag, To Straight (a2 … a5 of 2026-11-18).
#[test]
fn smooth_symmetric_and_cusp_points_follow_a_drag_as_pe_design_does() {
    let mut c = capture_rectangle();
    c.make_curve(2);
    // a2-smooth-drag.pes: the leaving handle dragged; the arriving one turns, 11.18 mm long.
    c.move_handle(2, true, v2(40.5, 15.5));
    let (arriving, leaving) = c.point_handles(2);
    assert!(
        near(arriving, 23.59, 39.66) && near(leaving, 40.5, 15.5),
        "{arriving:?}"
    );
    // a3-symmetry.pes: both handles at the mean length (14.75 mm), direction kept.
    c.apply_kind(2, crate::PointKind::Symmetric);
    let (arriving, leaving) = c.point_handles(2);
    assert!(
        near(arriving, 21.54, 42.58) && near(leaving, 38.46, 18.42),
        "{arriving:?} {leaving:?}"
    );
    // a3-symmetry-drag.pes: the other handle mirrors the dragged one.
    c.move_handle(2, true, v2(43.5, 13.0));
    assert!(near(c.point_handles(2).0, 16.5, 48.0));
    assert!(
        c.borrows_handle(1, true),
        "the straight neighbour's copy followed"
    );
    // a4-cusp.pes: To Cusp moves nothing; a4-cusp-drag.pes: nor does a drag move the other.
    let before = c.curves.clone();
    c.apply_kind(2, crate::PointKind::Cusp);
    assert_eq!(c.curves, before);
    c.move_handle(2, true, v2(49.0, 8.0));
    let (arriving, leaving) = c.point_handles(2);
    assert!(near(arriving, 16.5, 48.0) && near(leaving, 49.0, 8.0));
    // a9-cusp-to-smooth.pes and a9-cusp-to-symmetry.pes, both from this cusp whose handles
    // are not in line: onto the line parallel to the one between them, each keeping its
    // length (22.10 and 29.45 mm), or both at the mean (25.776 mm).
    let mut smooth = c.clone();
    smooth.apply_kind(2, crate::PointKind::Smooth);
    let (arriving, leaving) = smooth.point_handles(2);
    assert!(
        near(arriving, 30.0 - 13.9374, 30.5 + 17.1537)
            && near(leaving, 30.0 + 18.5704, 30.5 - 22.8559),
        "{arriving:?} {leaving:?}"
    );
    let mut symmetric = c.clone();
    symmetric.apply_kind(2, crate::PointKind::Symmetric);
    let (arriving, leaving) = symmetric.point_handles(2);
    assert!(
        near(arriving, 30.0 - 16.2539, 30.5 + 20.0048)
            && near(leaving, 30.0 + 16.2539, 30.5 - 20.0048),
        "{arriving:?} {leaving:?}"
    );
    // The arriving handle is the arriving side's: dragging it leaves the leaving one.
    c.move_handle(1, false, v2(20.0, 40.0));
    assert!(near(c.point_handles(2).1, 49.0, 8.0));
    // a5-to-straight.pes: the rectangle is back.
    c.make_straight(2);
    assert_eq!(c, capture_rectangle());
}

/// To Straight next to a curve point (PE-DESIGN's circle, c2 of 2026-11-18): the point
/// becomes a corner, its neighbours keep their own handles, so both sides still bend.
#[test]
fn to_straight_beside_curve_points_keeps_their_handles() {
    let mut c = bezier_circle(v2(0.0, 0.0), 100.0);
    let far = (
        c.curve(3).expect("arriving").c1,
        c.curve(0).expect("leaving").c2,
    );
    c.make_straight(0);
    assert!(!c.is_curve_point(0) && c.is_curve_point(1) && c.is_curve_point(3));
    let (arriving, leaving) = (
        c.curve(3).expect("still a curve"),
        c.curve(0).expect("still a curve"),
    );
    assert_eq!((arriving.c1, leaving.c2), far);
    assert_eq!(
        (arriving.c2, leaving.c1),
        far,
        "drawn from the neighbours' handles alone"
    );
    // And back: To Curve gives it PE's quarter handles again.
    assert!(c.make_curve(0));
    assert_eq!(c.kind(0), crate::PointKind::Smooth);
}

#[test]
fn inserting_and_deleting_points_keeps_each_points_kind() {
    let mut c = bezier_circle(v2(0.0, 0.0), 100.0);
    c.set_kind(3, crate::PointKind::Symmetric);
    let i = c.insert_point(0, 0.5);
    assert_eq!(
        c.kind(i),
        crate::PointKind::Smooth,
        "a point put on a curve"
    );
    assert_eq!(c.kind(4), crate::PointKind::Symmetric);
    c.delete_point(0);
    assert_eq!(c.kind(3), crate::PointKind::Symmetric);
    assert_eq!(c.kinds.len(), c.points.len());
    let moved = c.transformed(&Transform::translation(5.0, 0.0));
    assert_eq!(moved.kinds, c.kinds);
}

#[test]
fn split_at_point_opens_a_closed_outline_and_cuts_an_open_line() {
    let mut ring = bezier_circle(v2(0.0, 0.0), 100.0);
    ring.set_kind(1, crate::PointKind::Smooth);
    let opened = ring.split_at(2).expect("closed");
    assert_eq!(opened.len(), 1);
    let o = &opened[0];
    assert!(!o.closed);
    assert_eq!(o.points.len(), 5);
    assert_eq!(o.points[0], ring.points[2]);
    assert_eq!(o.points[4], ring.points[2]);
    assert_eq!(o.segment_count(), 4);
    assert!(o.curves.iter().all(Option::is_some));
    assert_eq!(o.path(), {
        let mut p = ring.starting_at(ring.points[2]).path();
        p.dedup();
        p
    });
    assert_eq!(
        o.kind(3),
        crate::PointKind::Smooth,
        "vertex 1, three steps on"
    );

    let line = Contour::new(vec![v2(0.0, 0.0), v2(30.0, 0.0), v2(30.0, 30.0)], false);
    assert!(line.split_at(0).is_none() && line.split_at(2).is_none());
    let halves = line.split_at(1).expect("inner point");
    assert_eq!(halves[0].points, vec![v2(0.0, 0.0), v2(30.0, 0.0)]);
    assert_eq!(halves[1].points, vec![v2(30.0, 0.0), v2(30.0, 30.0)]);
    assert!(halves.iter().all(|h| !h.closed));
}

#[test]
fn a_lock_steps_back_once_after_a_last_stitch_of_4_3_mm_and_twice_after_5_mm() {
    // #1246, machine-trim-bound: PE-DESIGN 11.50 ends a 5 mm fill square at 45 degrees on a
    // stitch of 4.24 to 4.3 mm and locks it with one step back and home; the satin references
    // lock a 5 mm last stitch with two.
    let b = v2(50.0, 0.0);
    let once = crate::region::lock_points(v2(19.6, 30.4), b);
    assert_eq!(once.len(), 2, "{once:?}");
    assert!((once[0].distance(b) - 3.0).abs() < 1e-9 && once[1] == b);
    let twice = crate::region::lock_points(v2(0.0, 0.0), b);
    assert_eq!(twice.len(), 4, "{twice:?}");
    assert!((twice[1].distance(b) - 6.0).abs() < 1e-9 && twice[3] == b);
}
