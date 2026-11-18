//! batch:stitch-engine-4: moved shapes sew the same stitches moved (#567), frames sewn in one
//! path (#467), programmable fill stitch length (#459), the appliqué wizard's kept original
//! (#470), guided fill rows (#471) and hairline satin (#368).

use openstitch_core::Command;
use openstitch_design::{Compiler, Vec2, v2};
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn session() -> Session {
    Session::new(std::env::temp_dir())
}

/// Every needle point and trim of the design, in order.
fn sewn(s: &Session) -> Vec<(Vec2, Command)> {
    Compiler::for_design(s.design())
        .compile(s.design())
        .stitches
        .iter()
        .filter(|x| matches!(x.command, Command::Stitch | Command::Trim))
        .map(|x| (v2(x.x, x.y), x.command))
        .collect()
}

fn trims(s: &Session) -> usize {
    sewn(s).iter().filter(|(_, c)| *c == Command::Trim).count()
}

/// Moves object 1 by `d` mm both ways and checks that every needle point moved by exactly that
/// and nothing else changed.
fn moves_as_a_translation(s: &mut Session, what: &str) {
    let before = sewn(s);
    assert!(before.len() > 20, "{what}: {} stitches", before.len());
    let mut at = 0.0;
    for d in [1.0, 0.5, 0.37, 3.5, 12.25, 35.0, 0.05, -7.13] {
        run(s, "object.move", json!({"ids": [1], "dx": d, "dy": d}));
        at += d;
        let after = sewn(s);
        assert_eq!(after.len(), before.len(), "{what} moved {at} mm");
        for (k, ((p, c), (q, e))) in before.iter().zip(&after).enumerate() {
            let off = *q - (*p + v2(at * 10.0, at * 10.0));
            assert!(
                c == e && off.length() < 1e-6,
                "{what} moved {at} mm: point {k} {p:?} -> {q:?}"
            );
        }
    }
}

#[test]
fn moving_a_shape_moves_its_stitches_and_nothing_else() {
    // The issue's 20 x 10 mm rectangle as satin, fill and running line, plus a programmable
    // fill, a rotated satin ellipse and a fill with a hole. The plain fills are moved along
    // their own 45° rows' diagonal here, which keeps them on the page's needle grid (#1131);
    // `a_moved_plain_fill_keeps_its_rows_and_takes_the_grid_of_its_new_place` moves one off it.
    let cases: [(&str, Value, Option<Value>); 6] = [
        (
            "satin",
            json!({"x": 10, "y": 10, "width": 20, "height": 10, "fill": "Blue"}),
            Some(json!({"id": 1, "type": "satin"})),
        ),
        (
            "fill",
            json!({"x": 10, "y": 10, "width": 20, "height": 10, "fill": "Blue"}),
            None,
        ),
        (
            "running",
            json!({"x": 10, "y": 10, "width": 20, "height": 10, "line": "Blue"}),
            None,
        ),
        (
            "programmable fill",
            json!({"x": 10, "y": 10, "width": 20, "height": 10, "fill": "Blue"}),
            Some(json!({"id": 1, "type": "programmableFill", "pattern": "diamonds"})),
        ),
        (
            "satin at 37 degrees",
            json!({"x": 10, "y": 10, "width": 20, "height": 10, "fill": "Blue"}),
            Some(json!({"id": 1, "type": "satin", "angle": 37, "density": 4.5})),
        ),
        (
            "piping",
            json!({"x": 10, "y": 10, "width": 20, "height": 10, "fill": "Blue"}),
            Some(json!({"id": 1, "type": "piping", "density": 1.2, "angle": 30})),
        ),
    ];
    for (what, shape, region) in cases {
        let mut s = session();
        run(&mut s, "shape.rectangle", shape);
        if let Some(r) = region {
            run(&mut s, "object.setRegion", r);
        }
        moves_as_a_translation(&mut s, what);
    }
    // Text is laid out where it stands, so it moves the same way.
    let mut s = session();
    run(
        &mut s,
        "text.add",
        json!({"text": "Hg", "font": "lato-bold", "size": 12, "x": 30, "y": 30, "fill": "Red"}),
    );
    moves_as_a_translation(&mut s, "text");
}

/// As [`moves_as_a_translation`], moved sideways and by unequal steps: whole, half and odd
/// fractions of a millimetre, so nothing hides behind a diagonal move.
fn moves_sideways_as_a_translation(s: &mut Session, what: &str) {
    let before = sewn(s);
    assert!(before.len() > 20, "{what}: {} stitches", before.len());
    let (mut x, mut y) = (0.0, 0.0);
    for (dx, dy) in [
        (1.0, 0.0),
        (0.5, 0.0),
        (0.37, 0.2),
        (3.5, -1.0),
        (12.25, 0.0),
        (0.05, 0.0),
    ] {
        run(s, "object.move", json!({"ids": [1], "dx": dx, "dy": dy}));
        x += dx;
        y += dy;
        let after = sewn(s);
        assert_eq!(after.len(), before.len(), "{what} moved ({x}, {y}) mm");
        for (k, ((p, c), (q, e))) in before.iter().zip(&after).enumerate() {
            let off = *q - (*p + v2(x * 10.0, y * 10.0));
            assert!(
                c == e && off.length() < 1e-6,
                "{what} moved ({x}, {y}) mm: point {k} {p:?} -> {q:?}"
            );
        }
    }
}

#[test]
fn only_a_plain_fills_needle_points_depend_on_its_place_on_the_page() {
    // #1131: PE-DESIGN's page-wide needle grid is for the plain Fill with a Frequency. Every
    // other sew type, the fills OpenStitch adds (the random stagger, the textures) and Stitch
    // Type 2 sew the same stitches wherever the shape lies (#567).
    let cases: [(&str, Value); 12] = [
        ("satin", json!({"id": 1, "type": "satin"})),
        (
            "satin at 37 degrees",
            json!({"id": 1, "type": "satin", "angle": 37, "density": 4.5}),
        ),
        (
            "programmable fill",
            json!({"id": 1, "type": "programmableFill", "pattern": "diamonds"}),
        ),
        ("motif fill", json!({"id": 1, "type": "motifFill"})),
        ("net fill", json!({"id": 1, "type": "netFill"})),
        ("zigzag net fill", json!({"id": 1, "type": "zigzagNetFill"})),
        (
            "piping",
            json!({"id": 1, "type": "piping", "density": 1.2, "angle": 30}),
        ),
        ("cross stitch", json!({"id": 1, "type": "crossStitch"})),
        ("stippling", json!({"id": 1, "type": "stippling"})),
        (
            "fill with the random stagger",
            json!({"id": 1, "type": "fill", "angle": 0, "stagger": "random"}),
        ),
        (
            "fill with a texture",
            json!({"id": 1, "type": "fill", "angle": 0, "texture": "ribs"}),
        ),
        (
            "fill, Stitch Type 2",
            json!({"id": 1, "type": "fill", "angle": 0, "stitchType": 2}),
        ),
    ];
    for (what, region) in cases {
        let mut s = session();
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": 10, "y": 10, "width": 20, "height": 10, "fill": "Blue"}),
        );
        run(&mut s, "object.setRegion", region);
        moves_sideways_as_a_translation(&mut s, what);
    }
    let mut s = session();
    run(
        &mut s,
        "shape.polygon",
        json!({"points": [[10, 20], [40, 20], [40, 30]], "closed": false, "line": "Blue"}),
    );
    moves_sideways_as_a_translation(&mut s, "running line");
    let mut s = session();
    run(
        &mut s,
        "text.add",
        json!({"text": "Hg", "font": "lato-bold", "size": 12, "x": 30, "y": 30, "fill": "Red"}),
    );
    run(&mut s, "object.setRegion", json!({"id": 1, "type": "fill"}));
    moves_sideways_as_a_translation(&mut s, "filled text");
}

/// The rows of a 0° fill: the heights (0.1 mm) that hold a stitch along the row at least 1 mm
/// long, and every needle point of those stitches as (row height, x).
fn fill_rows(s: &Session) -> (Vec<f64>, Vec<(f64, f64)>) {
    let pts: Vec<Vec2> = sewn(s)
        .into_iter()
        .filter(|(_, c)| *c == Command::Stitch)
        .map(|(p, _)| p)
        .collect();
    let mut rows: Vec<f64> = Vec::new();
    let mut on_rows = Vec::new();
    for w in pts.windows(2) {
        if (w[0].y - w[1].y).abs() < 1e-6 && (w[0].x - w[1].x).abs() >= 10.0 {
            if !rows.iter().any(|y| (y - w[0].y).abs() < 1e-6) {
                rows.push(w[0].y);
            }
            on_rows.extend([(w[0].y, w[0].x), (w[1].y, w[1].x)]);
        }
    }
    rows.sort_by(f64::total_cmp);
    (rows, on_rows)
}

/// A 40 mm square of plain Fill at 0° with its top-left corner at (`x`, `y`) mm on a page of
/// `page` mm, without under sewing.
fn plain_fill(page: (f64, f64), x: f64, y: f64) -> Session {
    let mut s = session();
    run(
        &mut s,
        "design.new",
        json!({"width": page.0, "height": page.1}),
    );
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": x, "y": y, "width": 40, "height": 40, "fill": "Blue"}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "fill", "angle": 0, "underlay": false}),
    );
    s
}

/// Checks that every needle point inside the rows of `s`'s 40 mm fill (left edge at `left`
/// mm) lies on PE-DESIGN's grid: row k's points (k + 1) × 30 % of the 4 mm stitch from the
/// origin of PE-DESIGN's coordinates, 100 mm left of the page's centre at `centre` mm.
fn assert_on_the_page_grid(s: &Session, left: f64, centre: f64, what: &str) {
    let (rows, on_rows) = fill_rows(s);
    assert_eq!(rows.len(), 181, "{what}");
    let gap = (rows[180] - rows[0]) / 180.0;
    let origin = (centre - 100.0) * 10.0;
    let mut checked = 0;
    for (y, x) in on_rows {
        let k = ((y - rows[0]) / gap).round();
        // The edge points, and the first and last row (sewn out and back, walked to the
        // start point), are not grid points.
        if x < left * 10.0 + 1.0 || x > left * 10.0 + 399.0 || k < 1.0 || k > 179.0 {
            continue;
        }
        let off = (x - origin - ((k + 1.0) * 0.3).fract() * 40.0).rem_euclid(40.0);
        assert!(
            !(1e-4..=40.0 - 1e-4).contains(&off),
            "{what}: row {k}, x {x}: {off} off the grid"
        );
        checked += 1;
    }
    assert!(checked > 1500, "{what}: {checked} grid points");
}

#[test]
fn a_moved_plain_fill_keeps_its_rows_and_takes_the_grid_of_its_new_place() {
    // #1131, the new rule for the plain Fill (PE-DESIGN 11.50: the sweep squares at five
    // angles, the 30 degree square at four positions): the rows move with the shape, and the
    // needle points along them lie on a grid fixed to the page, so their number may change
    // by a few.
    let mut s = plain_fill((100.0, 100.0), 10.0, 10.0);
    let (rows, _) = fill_rows(&s);
    let count = sewn(&s).len();
    assert_on_the_page_grid(&s, 10.0, 50.0, "at 10 mm");
    let mut at = 10.0;
    for dx in [1.0, 0.5, 0.37, 3.5, 12.25, 0.05] {
        run(
            &mut s,
            "object.move",
            json!({"ids": [1], "dx": dx, "dy": 0}),
        );
        at += dx;
        let (moved, _) = fill_rows(&s);
        assert_eq!(moved.len(), rows.len(), "at {at} mm");
        assert!(
            rows.iter().zip(&moved).all(|(a, b)| (a - b).abs() < 1e-6),
            "the rows stay where they are when the fill moves along them, at {at} mm"
        );
        assert_on_the_page_grid(&s, at, 50.0, &format!("at {at} mm"));
        let now = sewn(&s).len();
        assert!(
            now.abs_diff(count) <= 25,
            "at {at} mm: {now} stitches, {count} before the move"
        );
    }
    // Moved across the rows, the rows move with the shape and the grid stays.
    run(
        &mut s,
        "object.move",
        json!({"ids": [1], "dx": 0, "dy": 7.3}),
    );
    let (moved, _) = fill_rows(&s);
    assert!(
        rows.iter()
            .zip(&moved)
            .all(|(a, b)| (b - a - 73.0).abs() < 1e-6),
        "the rows move with the shape"
    );
    assert_on_the_page_grid(&s, at, 50.0, "moved down");
}

#[test]
fn a_plain_fill_sews_the_same_at_the_same_distance_from_the_page_centre() {
    // PE-DESIGN's coordinates start 100 mm left of and above the page's centre whatever the
    // page's size (the stamp capture on a 180 x 130 mm page fits the grid the 100 mm pages
    // gave): the same shape at the same distance from the centre sews the same stitches.
    // (The place is chosen so that no grid point lies exactly 0.3 mm from a row end, where
    // the last digit of the shape's page coordinates would decide whether it is sewn.)
    // Compared on the fill's rows: outside them the two pages' stitch lists differ by one
    // needle point (2006 against 2005), which is not the fill's.
    let small = plain_fill((100.0, 100.0), 30.65, 30.0);
    let large = plain_fill((180.0, 130.0), 70.65, 45.0);
    let ((rows, on_small), (_, on_large)) = (fill_rows(&small), fill_rows(&large));
    assert_eq!(rows.len(), 181);
    assert_eq!(on_small.len(), on_large.len());
    for (k, ((y, x), (v, u))) in on_small.iter().zip(&on_large).enumerate() {
        assert!(
            (u - x - 400.0).abs() < 1e-6 && (v - y - 150.0).abs() < 1e-6,
            "row point {k}: ({x}, {y}) / ({u}, {v})"
        );
    }
    // At the same page coordinates on a page 5 mm wider, the centre lies 2.5 mm further right:
    // the rows are the same and the needle points follow the new centre.
    let wider = plain_fill((105.0, 100.0), 30.65, 30.0);
    let (same, on_wider) = fill_rows(&wider);
    assert!(rows.iter().zip(&same).all(|(a, b)| (a - b).abs() < 1e-6));
    assert_on_the_page_grid(&wider, 30.65, 52.5, "on the wider page");
    assert_ne!(on_wider, on_small);
}

#[test]
fn moving_a_line_moves_its_exit_point() {
    // object.move left a line's exit point behind (DesignObject::apply), so a moved line with
    // an end point sewed back towards where it had been.
    let mut s = session();
    run(
        &mut s,
        "shape.polygon",
        json!({"points": [[10, 20], [40, 20]], "closed": false, "line": "Blue"}),
    );
    run(
        &mut s,
        "object.setEntryExit",
        json!({"id": 1, "lineEntry": [12, 20], "lineExit": [30, 20]}),
    );
    let exit = |s: &Session| s.design().objects[0].line.as_ref().and_then(|l| l.exit);
    let before = exit(&s).expect("exit");
    run(
        &mut s,
        "object.move",
        json!({"ids": [1], "dx": 5, "dy": 2.5}),
    );
    assert_eq!(exit(&s), Some(before + v2(50.0, 25.0)));
}

#[test]
fn laurel_and_star_frames_sew_each_piece_in_one_path() {
    // The design's last trim, plus for the laurel the one across its opening (two sides).
    for (frame, most) in [("laurel", 2), ("stars", 1)] {
        for size in [14.0, 40.0] {
            let mut s = session();
            run(
                &mut s,
                "decoration.add",
                json!({"frame": frame, "x": 20, "y": 20, "width": size, "height": size}),
            );
            assert!(
                trims(&s) <= most,
                "{frame} round a {size} mm box: {} trims",
                trims(&s)
            );
            // One object per piece (before: one per leaf and per star).
            let objects = &s.design().objects;
            assert!(objects.len() <= 2, "{frame}: {} objects", objects.len());
        }
    }
    // A monogram's laurel, as in the kit's lettering/monograms.pes.
    let mut s = session();
    run(
        &mut s,
        "monogram.add",
        json!({"letters": "ABC", "style": "script", "size": 16, "x": 110, "y": 40,
               "frame": "laurel", "fill": 5, "frameColor": 37}),
    );
    let p = Compiler::for_design(s.design()).compile(s.design());
    // The frame is the first colour block: at most the one trim across its opening.
    let first_block: Vec<_> = p
        .stitches
        .iter()
        .take_while(|x| x.command != Command::ColorChange)
        .collect();
    let frame_trims = first_block
        .iter()
        .filter(|x| x.command == Command::Trim)
        .count();
    assert!(frame_trims <= 2, "monogram laurel: {frame_trims} trims");
}

/// Longest needle-to-needle stitch of the design, mm.
fn longest(s: &Session) -> f64 {
    let p = Compiler::for_design(s.design()).compile(s.design());
    p.stitches
        .windows(2)
        .filter(|w| w[0].command == Command::Stitch && w[1].command == Command::Stitch)
        .map(|w| v2(w[0].x, w[0].y).distance(v2(w[1].x, w[1].y)) / 10.0)
        .fold(0.0, f64::max)
}

#[test]
fn programmable_fill_splits_long_stitches_on_request() {
    // The issue's repro: 12 mm diamonds at 2.2 lines/mm sew 8-12 mm stitches, as PE-DESIGN
    // sews crossing to crossing; splitLength keeps them to 4 mm.
    let mut s = session();
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 2, "y": 2, "width": 40, "height": 36, "fill": "Blue"}),
    );
    let set = json!({"id": 1, "type": "programmableFill", "pattern": "diamonds",
                     "patternWidth": 12, "patternHeight": 12, "density": 2.2, "stitchLength": 4,
                     "underlay": false});
    run(&mut s, "object.setRegion", set);
    let crossings = sewn(&s);
    assert!(
        longest(&s) > 8.0,
        "PE-DESIGN's own stitches: {}",
        longest(&s)
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "splitLength": 4}),
    );
    assert!(longest(&s) <= 4.0 + 1e-6, "split: {}", longest(&s));
    // Every crossing is still a needle point: the pattern's lines still read.
    let split = sewn(&s);
    assert!(split.len() > crossings.len());
    let on = crossings
        .iter()
        .filter(|(p, c)| *c == Command::Stitch && split.iter().any(|(q, _)| q.distance(*p) < 0.5))
        .count();
    let all = crossings
        .iter()
        .filter(|(_, c)| *c == Command::Stitch)
        .count();
    assert!(
        on as f64 >= 0.97 * all as f64,
        "{on} of {all} crossings kept"
    );
    // 0 turns it off again.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "splitLength": 0}),
    );
    assert_eq!(sewn(&s), crossings);
}

/// Signed distance from `p` to the polyline `g` extended straight past both ends, 0.1 mm.
fn guide_distance(g: &[Vec2], p: Vec2) -> f64 {
    let n = g.len();
    let unit = |d: Vec2| d * (1.0 / d.length());
    let mut ext = vec![g[0] - unit(g[1] - g[0]) * 5000.0];
    ext.extend_from_slice(g);
    ext.push(g[n - 1] + unit(g[n - 1] - g[n - 2]) * 5000.0);
    let mut best = (f64::INFINITY, 0.0);
    for w in ext.windows(2) {
        let ab = w[1] - w[0];
        let t = (((p - w[0]).x * ab.x + (p - w[0]).y * ab.y) / (ab.x * ab.x + ab.y * ab.y))
            .clamp(0.0, 1.0);
        let d = p.distance(w[0] + ab * t);
        if d < best.0 {
            let side = ab.y * (p - w[0]).x - ab.x * (p - w[0]).y;
            best = (d, if side >= 0.0 { d } else { -d });
        }
    }
    best.1
}

fn segments_cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let cr = |o: Vec2, p: Vec2, q: Vec2| (p - o).x * (q - o).y - (p - o).y * (q - o).x;
    let (d1, d2, d3, d4) = (cr(c, d, a), cr(c, d, b), cr(a, b, c), cr(a, b, d));
    d1 * d2 < -1e-9 && d3 * d4 < -1e-9
}

#[test]
fn a_direction_curve_fills_with_rows_a_spacing_apart_that_never_cross() {
    // examples/sewing-direction.json's heart and its arched direction curve, as fill and as
    // piping (#471): rows are the curve offset by whole row spacings.
    for (kind, density) in [("fill", 4.0), ("piping", 1.2)] {
        let mut s = session();
        run(
            &mut s,
            "shape.preset",
            json!({"kind": "heart", "cx": 27, "cy": 27, "width": 40, "fill": "Red"}),
        );
        run(
            &mut s,
            "object.setRegion",
            json!({"id": 1, "type": kind, "density": density, "underlay": false}),
        );
        run(
            &mut s,
            "object.setDirectionCurve",
            json!({"id": 1, "points": [[2, 30], [52, 30]], "curves": [[[15, 5], [40, 5]]]}),
        );
        let o = &s.design().objects[0];
        let guide = o
            .region
            .as_ref()
            .and_then(|r| r.direction_curve.as_ref())
            .expect("curve")
            .path();
        let spacing = 10.0 / density;
        let pts: Vec<Vec2> = sewn(&s)
            .into_iter()
            .filter(|(_, c)| *c == Command::Stitch)
            .map(|(p, _)| p)
            .collect();
        // Needle points lie on rows a whole number of spacings from the guide: off by under
        // 10 % of the spacing, except travel along the outline and tie-ins.
        let off: Vec<f64> = pts
            .iter()
            .map(|&p| {
                let d = guide_distance(&guide, p) / spacing;
                (d - d.round()).abs()
            })
            .collect();
        let on = off.iter().filter(|&&e| e < 0.1).count();
        assert!(
            on as f64 > 0.85 * pts.len() as f64,
            "{kind}: {on} of {} needle points on a row",
            pts.len()
        );
        // Long stitches along a row never cross another row's: no two stitches cross where
        // both run along rows (travel and tie-ins left out).
        let along: Vec<(Vec2, Vec2)> = pts
            .windows(2)
            .filter(|w| {
                let (a, b) = (guide_distance(&guide, w[0]), guide_distance(&guide, w[1]));
                (a - b).abs() < 0.1 * spacing
                    && ((a / spacing) - (a / spacing).round()).abs() < 0.1
                    && w[0].distance(w[1]) > 1.0
            })
            .map(|w| (w[0], w[1]))
            .collect();
        assert!(along.len() > 100, "{kind}: {} row stitches", along.len());
        let crossings = (0..along.len())
            .flat_map(|i| (i + 1..along.len()).map(move |j| (i, j)))
            .filter(|&(i, j)| segments_cross(along[i].0, along[i].1, along[j].0, along[j].1))
            .count();
        assert_eq!(crossings, 0, "{kind}: rows cross");
    }
}

/// Longest run of stitches under 1 mm that turn back each time, as `tools/sew_check.py` counts
/// a hairline zigzag.
fn hairline_run(s: &Session) -> usize {
    let p = Compiler::for_design(s.design()).compile(s.design());
    let (mut best, mut run, mut step): (usize, usize, Option<Vec2>) = (0, 0, None);
    for w in p.stitches.windows(2) {
        if w[0].command == Command::Stitch && w[1].command == Command::Stitch {
            let v = v2(w[1].x - w[0].x, w[1].y - w[0].y);
            let turned = step.is_some_and(|u| u.x * v.x + u.y * v.y < 0.0);
            run = if v.length() > 0.5 && v.length() < 10.0 && turned {
                run + 1
            } else {
                0
            };
            best = best.max(run);
            step = Some(v);
        } else {
            (run, step) = (0, None);
        }
    }
    best
}

#[test]
fn a_hairline_satin_sews_as_a_running_line() {
    // #368: a 0.4 mm satin column 30 mm long crossed itself 150 times in one spot. Its hairline
    // stretch is now a running stitch along its middle; a 1.5 mm column keeps its satin.
    let mut s = session();
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 30, "height": 0.4, "fill": "Black"}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "satin", "angle": 90, "underlay": false}),
    );
    assert!(hairline_run(&s) <= 30, "{}", hairline_run(&s));
    let line: Vec<Vec2> = sewn(&s).into_iter().map(|(p, _)| p).collect();
    // Still sewn end to end, along the column's middle.
    let xs = line
        .iter()
        .map(|p| p.x)
        .fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
    assert!(xs.0 < 110.0 && xs.1 > 390.0, "{xs:?}");
    assert!(
        line.iter().all(|p| (p.y - 102.0).abs() < 3.0),
        "off the column"
    );
    let mut wide = session();
    run(
        &mut wide,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 30, "height": 1.5, "fill": "Black"}),
    );
    run(
        &mut wide,
        "object.setRegion",
        json!({"id": 1, "type": "satin", "angle": 90, "underlay": false}),
    );
    let crossings = sewn(&wide).len();
    assert!(
        crossings > 100,
        "a 1.5 mm satin keeps its {crossings} crossings"
    );
}

#[test]
fn a_run_ending_in_tiny_stitches_locks_a_full_step() {
    // #368: a running line whose last stitch is 0.1 mm locked 0.1 mm back and forth (with the
    // tie-in, six stitches under 0.3 mm in a row). The lock now steps back 0.3 mm along the
    // line.
    let mut s = session();
    run(
        &mut s,
        "shape.polygon",
        json!({"points": [[10, 10], [20, 10], [20.1, 10]], "closed": false, "line": "Black"}),
    );
    let pts: Vec<Vec2> = sewn(&s)
        .into_iter()
        .filter(|(_, c)| *c == Command::Stitch)
        .map(|(p, _)| p)
        .collect();
    // The lock: out 0.3 mm and back to the line's end.
    let n = pts.len();
    assert!(pts[n - 1].distance(v2(201.0, 100.0)) < 0.5, "{pts:?}");
    for w in pts[n - 3..].windows(2) {
        assert!(
            (w[0].distance(w[1]) - 3.0).abs() < 0.2,
            "lock step: {pts:?}"
        );
    }
}
