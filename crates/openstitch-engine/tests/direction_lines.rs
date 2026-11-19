//! PE-DESIGN's direction lines on a fill (#1273): `object.addDirectionLine`,
//! `object.deleteDirectionLine`, the fan rule of `object.setAngleLines`, the stitches against
//! what PE-DESIGN 11.50 sews for the same lines, and PES 0110.

use openstitch_design::{AngleRule, Compiler, DesignObject, Vec2, v2};
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn fails(s: &mut Session, id: &str, params: Value) -> String {
    match s.execute(id, Some(&params)) {
        Ok(v) => panic!("{id} should fail, gave {v}"),
        Err(e) => e.to_string(),
    }
}

fn object(s: &Session, id: u64) -> &DesignObject {
    let d = s.design();
    &d.objects[d.index_of(id).expect("object")]
}

/// The 40 mm square of PE-DESIGN's reference designs (30 to 70 mm), filled at 45 degrees.
fn square(s: &mut Session) -> u64 {
    let id = run(
        s,
        "shape.rectangle",
        json!({ "x": 30, "y": 30, "width": 40, "height": 40, "fill": "#2e8b57" }),
    )["id"]
        .as_u64()
        .expect("id");
    run(s, "object.setRegion", json!({ "id": id, "angle": 45 }));
    id
}

/// The second line of PE-DESIGN's two-line design: across the top-left corner, from left of
/// the left side (it crosses it at 36.4 mm) to above the top side (crossing at 45.37 mm).
const SECOND: [[f64; 2]; 2] = [[25.2, 38.4], [50.17, 28.0]];
/// The third line of the three-line design: across the bottom-right corner, parallel to the
/// 45 degree line (crossing the bottom side at 61.05 mm and the right side at 61.13 mm).
const THIRD: [[f64; 2]; 2] = [[57.05, 74.0], [74.0, 57.2]];

/// The rows of the fill, found by the stitches' turn-arounds: each as its two ends (mm).
fn rows(s: &Session) -> Vec<[Vec2; 2]> {
    let st = Compiler::for_design(s.design())
        .compile(s.design())
        .stitches;
    let p: Vec<Vec2> = st.iter().map(|q| v2(q.x / 10.0, q.y / 10.0)).collect();
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..p.len() {
        let d = p[i] - p[i - 1];
        let len = d.length();
        let whole = p[i - 1] - p[start];
        let run = whole.length();
        if len > 1e-9 && run > 1e-9 && (d.x * whole.x + d.y * whole.y) / (len * run) < 0.3 {
            out.push([p[start], p[i - 1]]);
            start = i - 1;
        }
    }
    out.push([p[start], p[p.len() - 1]]);
    out.retain(|r| r[0].distance(r[1]) > 2.0);
    out
}

fn angle(r: &[Vec2; 2]) -> f64 {
    let d = r[1] - r[0];
    (-d.y).atan2(d.x).to_degrees().rem_euclid(180.0)
}

/// Rows and the median angle of the rows longer than 10 mm, in the three zones of the
/// two-line design: beyond the second line (the corner), between the lines, beyond the 45
/// degree line.
fn zones(s: &Session) -> [(usize, f64); 3] {
    let mut by: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut count = [0_usize; 3];
    for r in rows(s) {
        let m = r[0].lerp(r[1], 0.5);
        let k = if m.x + m.y > 100.4 {
            2
        } else if (m.y - 36.4) + 0.4164 * (m.x - 30.0) < -0.15 {
            0
        } else {
            1
        };
        count[k] += 1;
        if r[0].distance(r[1]) > 10.0 {
            by[k].push(angle(&r));
        }
    }
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(f64::NAN)
    };
    [
        (count[0], median(&mut by[0])),
        (count[1], median(&mut by[1])),
        (count[2], median(&mut by[2])),
    ]
}

/// The fill against PE-DESIGN 11.50's for the same square and lines (its files `g1-two-lines`
/// and `g2-three-lines`, kept outside the repository; read on 2026-11-19): 2773 needle points
/// in the fill with two lines and with three; rows 24 / 143 / 140 in the three zones, with
/// median row angles 21.8 / 34.9 / 45.0 degrees. Tolerance, stated once: the stitch count
/// within 3 %, the rows of a zone within 2, the median angle of a zone within 1.5 degrees.
#[test]
fn the_fill_between_direction_lines_is_pe_designs_within_the_stated_tolerance() {
    const PE_STITCHES: f64 = 2773.0;
    const PE_ZONES: [(usize, f64); 3] = [(24, 21.8), (143, 34.9), (140, 45.0)];
    let count = |s: &Session| {
        Compiler::for_design(s.design())
            .compile(s.design())
            .stitches
            .len() as f64
    };
    let mut s = Session::new(std::env::temp_dir());
    let id = square(&mut s);
    let plain = count(&s);
    let r = run(
        &mut s,
        "object.addDirectionLine",
        json!({ "id": id, "line": SECOND }),
    );
    assert_eq!(
        (r["lines"].clone(), r["index"].clone()),
        (json!(2), json!(1))
    );
    let two = count(&s);
    assert!(
        (two / PE_STITCHES - 1.0).abs() <= 0.03,
        "two lines: {two} stitches against PE-DESIGN's {PE_STITCHES}"
    );
    assert!(two > plain * 1.1, "the fan adds rows: {plain} -> {two}");
    for (k, ((rows, median), (pe_rows, pe_median))) in
        zones(&s).into_iter().zip(PE_ZONES).enumerate()
    {
        assert!(
            rows.abs_diff(pe_rows) <= 2,
            "zone {k}: {rows} rows against PE-DESIGN's {pe_rows}"
        );
        assert!(
            (median - pe_median).abs() <= 1.5,
            "zone {k}: median row angle {median:.1} against PE-DESIGN's {pe_median}"
        );
    }
    // Every needle point is in the square (to the fill's own overshoot).
    let st = Compiler::for_design(s.design())
        .compile(s.design())
        .stitches;
    assert!(
        st.iter()
            .all(|q| (295.0..=705.0).contains(&q.x) && (295.0..=705.0).contains(&q.y)),
        "the rows stay inside the outline"
    );
    // A third line, parallel to the first: it goes first in the order across the shape, and
    // the rows between two parallel lines are parallel (PE-DESIGN's count is the same).
    let r = run(
        &mut s,
        "object.addDirectionLine",
        json!({ "id": id, "line": THIRD }),
    );
    assert_eq!(
        (r["lines"].clone(), r["index"].clone()),
        (json!(3), json!(0))
    );
    let three = count(&s);
    assert!(
        (three / PE_STITCHES - 1.0).abs() <= 0.03,
        "three lines: {three} stitches against PE-DESIGN's {PE_STITCHES}"
    );
    // One Undo each.
    s.execute("history.undo", None).expect("undo");
    assert_eq!(count(&s), two);
    s.execute("history.undo", None).expect("undo");
    assert_eq!(count(&s), plain);
    assert!(
        object(&s, id)
            .region
            .as_ref()
            .expect("region")
            .angle_lines
            .is_empty()
    );
}

#[test]
fn add_direction_line_refuses_what_pe_design_refuses_and_says_why() {
    let mut s = Session::new(std::env::temp_dir());
    let id = square(&mut s);
    let before = s.design().clone();
    // An end inside the shape (PE-DESIGN capture g: no line is added).
    let e = fails(
        &mut s,
        "object.addDirectionLine",
        json!({ "id": id, "line": [[25, 38], [50, 50]] }),
    );
    assert!(e.contains("second end lies inside the shape"), "{e}");
    // A line that would cross the 45 degree line inside the shape (the manual).
    let e = fails(
        &mut s,
        "object.addDirectionLine",
        json!({ "id": id, "line": [[25, 40], [75, 60]] }),
    );
    assert!(e.contains("cross inside the shape"), "{e}");
    // A line that passes the shape by.
    let e = fails(
        &mut s,
        "object.addDirectionLine",
        json!({ "id": id, "line": [[10, 10], [90, 12]] }),
    );
    assert!(e.contains("does not cross the shape"), "{e}");
    assert_eq!(*s.design(), before, "nothing changed");
    // Hole sewing (the manual): refused for the outer and the inner shape.
    let inner = run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 45, "y": 45, "width": 10, "height": 10, "fill": "#c0504d" }),
    )["id"]
        .as_u64()
        .expect("id");
    run(
        &mut s,
        "object.setHoleSewing",
        json!({ "ids": [id, inner], "enabled": true }),
    );
    for shape in [id, inner] {
        let e = fails(
            &mut s,
            "object.addDirectionLine",
            json!({ "id": shape, "line": SECOND }),
        );
        assert!(
            e.contains("hole sewing") && e.contains("object.setHoleSewing"),
            "{e}"
        );
    }
    run(
        &mut s,
        "object.setHoleSewing",
        json!({ "ids": [id], "enabled": false }),
    );
    // OpenStitch's own angle lines (rule sides) are not converted, either way.
    run(
        &mut s,
        "object.setAngleLines",
        json!({ "id": id, "lines": [[[25, 40], [75, 40]], [[25, 60], [75, 60]]] }),
    );
    assert_eq!(
        object(&s, id).region.as_ref().expect("region").angle_rule,
        AngleRule::Sides
    );
    let e = fails(
        &mut s,
        "object.addDirectionLine",
        json!({ "id": id, "line": SECOND }),
    );
    assert!(
        e.contains("rule sides") && e.contains("not converted"),
        "{e}"
    );
    let e = fails(
        &mut s,
        "object.setAngleLines",
        json!({ "id": id, "lines": [[[25, 40], [75, 40]], [[25, 60], [75, 60]]], "rule": "fan" }),
    );
    assert!(e.contains("not converted"), "{e}");
    run(
        &mut s,
        "object.setAngleLines",
        json!({ "id": id, "lines": [] }),
    );
    run(
        &mut s,
        "object.addDirectionLine",
        json!({ "id": id, "line": SECOND }),
    );
    let e = fails(
        &mut s,
        "object.setAngleLines",
        json!({ "id": id, "lines": [[[25, 40], [75, 40]], [[25, 60], [75, 60]]], "rule": "sides" }),
    );
    assert!(e.contains("rule fan") && e.contains("not converted"), "{e}");
    // A curved direction, and a region that is not a fill.
    let other = square(&mut s);
    run(
        &mut s,
        "object.setDirectionCurve",
        json!({ "id": other, "points": [[31, 50], [50, 45], [69, 50]] }),
    );
    let e = fails(
        &mut s,
        "object.addDirectionLine",
        json!({ "id": other, "line": SECOND }),
    );
    assert!(e.contains("curved direction"), "{e}");
    let satin = square(&mut s);
    run(
        &mut s,
        "object.setRegion",
        json!({ "id": satin, "type": "satin" }),
    );
    let e = fails(
        &mut s,
        "object.addDirectionLine",
        json!({ "id": satin, "line": SECOND }),
    );
    assert!(e.contains("not a fill"), "{e}");
}

/// PE-DESIGN capture g: Delete removes the selected line; Delete Lines Not Selected keeps
/// only it; the last line cannot be deleted. Reverse turns every line round (the manual).
#[test]
fn delete_and_reverse_direction_lines() {
    let mut s = Session::new(std::env::temp_dir());
    let id = square(&mut s);
    let region = |s: &Session| object(s, id).region.clone().expect("region");
    let e = fails(
        &mut s,
        "object.deleteDirectionLine",
        json!({ "id": id, "index": 0 }),
    );
    assert!(e.contains("last line cannot be deleted"), "{e}");
    run(
        &mut s,
        "object.addDirectionLine",
        json!({ "id": id, "line": SECOND }),
    );
    run(
        &mut s,
        "object.addDirectionLine",
        json!({ "id": id, "line": THIRD }),
    );
    let three = region(&s);
    assert_eq!(
        (three.angle_lines.len(), three.angle_rule),
        (3, AngleRule::Fan)
    );
    // Delete one of three: two stay, still fan lines.
    let r = run(
        &mut s,
        "object.deleteDirectionLine",
        json!({ "id": id, "index": 0 }),
    );
    assert_eq!(r["lines"], 2);
    assert_eq!(region(&s).angle_lines, three.angle_lines[1..].to_vec());
    s.execute("history.undo", None).expect("undo");
    assert_eq!(region(&s), three, "one Undo");
    // Keep only the corner line (it is last in the order): it becomes the fill angle, the
    // way it points (about 22.6 degrees), and no line is stored.
    let r = run(
        &mut s,
        "object.deleteDirectionLine",
        json!({ "id": id, "keep": 2 }),
    );
    assert_eq!(r["lines"], 1);
    let one = region(&s);
    assert!(one.angle_lines.is_empty() && one.angle_rule == AngleRule::Sides);
    assert!((one.angle - 22.6).abs() < 0.2, "{}", one.angle);
    let e = fails(
        &mut s,
        "object.deleteDirectionLine",
        json!({ "id": id, "index": 0 }),
    );
    assert!(e.contains("last line cannot be deleted"), "{e}");
    s.execute("history.undo", None).expect("undo");
    // Reverse: every line turned round, the angle by 180 degrees.
    run(&mut s, "object.reverseDirection", json!({ "ids": [id] }));
    let back = region(&s);
    for (a, b) in back.angle_lines.iter().zip(&three.angle_lines) {
        assert_eq!((a[0], a[1]), (b[1], b[0]));
    }
    assert_eq!(back.angle, (three.angle + 180.0).rem_euclid(360.0));
}

/// PES 0110 keeps the lines: each as its two crossings of the outline, the way PE-DESIGN's
/// own files hold them (read from `g1-two-lines` and `g2-three-lines`: the first list
/// [(0, 1.0), (1, 0.84)], the second [(2, 1.0), (2, 0.384)] for the two-line design).
#[test]
fn pes_0110_keeps_direction_lines() {
    let dir =
        std::env::temp_dir().join(format!("openstitch-direction-lines-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch folder");
    let mut s = Session::new(dir.clone());
    let id = square(&mut s);
    run(
        &mut s,
        "design.export",
        json!({ "path": "plain.pes", "pesVersion": "11" }),
    );
    run(
        &mut s,
        "object.addDirectionLine",
        json!({ "id": id, "line": SECOND }),
    );
    run(
        &mut s,
        "design.export",
        json!({ "path": "two.pes", "pesVersion": "11" }),
    );
    // The file's two lists, as PE-DESIGN writes them for these lines.
    let bytes = std::fs::read(dir.join("two.pes")).expect("file");
    let shapes = openstitch_formats::pes::read_objects(&bytes)
        .expect("objects")
        .shapes;
    let lines = &shapes[0].direction_lines;
    assert_eq!(lines.len(), 2);
    let near = |got: (u32, f64), want: (u32, f64)| got.0 == want.0 && (got.1 - want.1).abs() < 2e-3;
    assert!(
        near(lines[0][0], (0, 1.0)) && near(lines[0][1], (2, 1.0)),
        "{lines:?}"
    );
    assert!(
        near(lines[1][0], (1, 0.84)) && near(lines[1][1], (2, 0.3843)),
        "{lines:?}"
    );
    // Read back: the same lines (as where they cross the outline), the same rule, and the
    // same stitches to within the file's rounding.
    let sewn = Compiler::for_design(s.design())
        .compile(s.design())
        .stitches
        .len();
    let mut back = Session::new(dir.clone());
    run(&mut back, "import.pes", json!({ "path": "two.pes" }));
    let r = back.design().objects[0].region.clone().expect("region");
    assert_eq!((r.angle_lines.len(), r.angle_rule), (2, AngleRule::Fan));
    let crossings = [
        [v2(300.0, 700.0), v2(700.0, 300.0)],
        [v2(300.0, 364.0), v2(453.7, 300.0)],
    ];
    for (got, want) in r.angle_lines.iter().zip(crossings) {
        assert!(
            got[0].distance(want[0]) < 1.0 && got[1].distance(want[1]) < 1.0,
            "{got:?} / {want:?}"
        );
    }
    let again = Compiler::for_design(back.design())
        .compile(back.design())
        .stitches
        .len();
    assert!(sewn.abs_diff(again) <= sewn / 100, "{sewn} / {again}");
    // A shape without added lines is written as before: one entry in each list.
    let plain = std::fs::read(dir.join("plain.pes")).expect("file");
    assert!(
        openstitch_formats::pes::read_objects(&plain)
            .expect("objects")
            .shapes[0]
            .direction_lines
            .is_empty()
    );
    // OpenStitch's own angle lines (rule sides) are still refused in PES 0110.
    let mut sides = Session::new(dir.clone());
    let id = square(&mut sides);
    run(
        &mut sides,
        "object.setAngleLines",
        json!({ "id": id, "lines": [[[25, 40], [75, 40]], [[25, 60], [75, 60]]] }),
    );
    let e = fails(
        &mut sides,
        "design.export",
        json!({ "path": "sides.pes", "pesVersion": "11" }),
    );
    assert!(e.contains("stitch angle lines"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// What a model reads: the commands say what is stored, and what happens with lines of the
/// other rule.
#[test]
fn the_registry_says_what_the_direction_line_commands_store() {
    let s = Session::new(std::env::temp_dir());
    let commands = s.commands();
    let command = |id: &str| {
        commands
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("{id} is registered"))
    };
    for words in [
        "fill angle is its first direction line",
        "a fan through the point where the two lines meet",
        "rule fan",
        "in order across the shape",
        "an end lies inside the shape",
        "cross another direction line inside the shape",
        "hole sewing",
        "rule sides",
        "not converted",
        "One undo step",
    ] {
        assert!(
            command("object.addDirectionLine").summary.contains(words),
            "add: {words}"
        );
    }
    for words in [
        "Delete Selected Line",
        "Delete Lines Not Selected",
        "becomes the shape's fill angle again",
        "last line, its fill angle, cannot be deleted",
    ] {
        assert!(
            command("object.deleteDirectionLine")
                .summary
                .contains(words),
            "delete: {words}"
        );
    }
    let rule = command("object.setAngleLines")
        .params
        .iter()
        .find(|p| p.name == "rule")
        .expect("rule");
    for words in ["sides", "fan", "far ends", "not converted"] {
        assert!(rule.summary.contains(words), "rule: {words}");
    }
}
