//! Coverage for editing claims FEATURE_MATRIX makes that no other test asserts (#327): flip
//! geometry, handles following a moved vertex, the attribute selections of Copy Sewing
//! Attributes, pasted text following its pasted path, reversing a curved direction, the export's
//! sewing area, thread length per colour, Start/End Type and line exit in `.osd`, and the
//! decorative frame's ornament size.

use openstitch_design::{Compiler, LineType, StartEnd, v2};
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn fails(s: &mut Session, id: &str, params: Value) {
    assert!(s.execute(id, Some(&params)).is_err(), "{id} {params} fails");
}

fn temp_dir(tag: &str) -> TempDir {
    TempDir::new(&format!("coverage-editing-{tag}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().expect("id")
}

fn ids(v: &Value) -> Vec<u64> {
    v["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect()
}

fn object(s: &Session, id: u64) -> &openstitch_design::DesignObject {
    let d = s.design();
    &d.objects[d.index_of(id).expect("object")]
}

fn stitch_points(s: &Session) -> Vec<(i64, i64)> {
    Compiler::default()
        .compile(s.design())
        .stitches
        .iter()
        .map(|st| (st.x.round() as i64, st.y.round() as i64))
        .collect()
}

#[test]
fn flip_mirrors_the_points_about_the_selection_centre_in_one_undo_step() {
    let mut s = Session::new(std::env::temp_dir());
    // A right triangle: its mirror image is not itself.
    let tri = id(&run(
        &mut s,
        "shape.polygon",
        json!({"points": [[10, 10], [40, 10], [10, 30]], "closed": true}),
    ));
    let before = object(&s, tri).contours[0].points.clone();
    let b = object(&s, tri).bounds().expect("bounds");
    let (cx, cy) = (b.center().x, b.center().y);

    run(
        &mut s,
        "object.flip",
        json!({"ids": [tri], "axis": "horizontal"}),
    );
    let flipped = object(&s, tri).contours[0].points.clone();
    assert_eq!(flipped.len(), before.len());
    for (p, q) in before.iter().zip(&flipped) {
        assert!((q.x - (2.0 * cx - p.x)).abs() < 1e-6, "{p:?} → {q:?}");
        assert!((q.y - p.y).abs() < 1e-6, "{p:?} → {q:?}");
    }
    assert_eq!(object(&s, tri).bounds(), Some(b), "same extent");

    run(&mut s, "history.undo", json!({}));
    assert_eq!(object(&s, tri).contours[0].points, before, "one undo step");

    run(
        &mut s,
        "object.flip",
        json!({"id": tri, "axis": "vertical"}),
    );
    for (p, q) in before.iter().zip(&object(&s, tri).contours[0].points) {
        assert!((q.x - p.x).abs() < 1e-6 && (q.y - (2.0 * cy - p.y)).abs() < 1e-6);
    }
}

#[test]
fn a_moved_vertex_takes_its_curve_handles_with_it() {
    let mut s = Session::new(std::env::temp_dir());
    // A lens: two vertices, two curved segments; vertex 0 ends segment 1 and starts segment 0.
    let lens = id(&run(
        &mut s,
        "shape.path",
        json!({"points": [[10, 20], [50, 20]], "closed": true,
               "curves": [[[20, 5], [40, 5]], [[40, 35], [20, 35]]]}),
    ));
    let c = object(&s, lens).contours[0].clone();
    let (out0, in0) = (c.curve(0).expect("c0"), c.curve(1).expect("c1"));
    run(
        &mut s,
        "object.setPoint",
        json!({"id": lens, "index": 0, "x": 13, "y": 24}),
    );
    let c = object(&s, lens).contours[0].clone();
    let delta = v2(30.0, 40.0);
    assert_eq!(c.points[0], v2(130.0, 240.0));
    let (out1, in1) = (c.curve(0).expect("c0"), c.curve(1).expect("c1"));
    assert_eq!(
        out1.c1,
        out0.c1 + delta,
        "outgoing handle moved with the vertex"
    );
    assert_eq!(
        out1.c2, out0.c2,
        "the far handle of the outgoing segment stays"
    );
    assert_eq!(
        in1.c2,
        in0.c2 + delta,
        "incoming handle moved with the vertex"
    );
    assert_eq!(
        in1.c1, in0.c1,
        "the far handle of the incoming segment stays"
    );
}

#[test]
fn copy_sewing_attributes_copies_only_the_chosen_line_and_hole_attributes() {
    let mut s = Session::new(std::env::temp_dir());
    let src = id(&run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 30, "height": 30}),
    ));
    let inner = id(&run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 10, "height": 10}),
    ));
    run(
        &mut s,
        "object.setHoleSewing",
        json!({"ids": [inner, src], "enabled": true}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": src, "type": "zigzag", "color": "Red", "width": 3}),
    );
    let dst = id(&run(
        &mut s,
        "shape.rectangle",
        json!({"x": 50, "y": 0, "width": 20, "height": 20}),
    ));
    run(
        &mut s,
        "object.setLine",
        json!({"id": dst, "type": "running", "color": "Blue"}),
    );
    let blue = object(&s, dst).line.clone().expect("line").thread;
    let red = object(&s, src).line.clone().expect("line").thread;
    assert_ne!(red, blue);

    // Line colour only: the thread changes, the sew type does not.
    run(
        &mut s,
        "object.pasteAttributes",
        json!({"from": src, "id": dst, "attributes": ["lineColor"]}),
    );
    let l = object(&s, dst).line.clone().expect("line");
    assert_eq!((l.kind, &l.thread), (LineType::Running, &red));
    assert!(!object(&s, dst).hole_sewing);
    run(&mut s, "history.undo", json!({}));

    // Line sewing only: the sew type and width change, the thread stays.
    run(
        &mut s,
        "object.pasteAttributes",
        json!({"from": src, "id": dst, "attributes": ["lineSewing"]}),
    );
    let l = object(&s, dst).line.clone().expect("line");
    assert_eq!((l.kind, &l.thread), (LineType::Zigzag, &blue));
    assert!((l.width - 3.0).abs() < 1e-6, "width {} mm", l.width);
    assert!(!object(&s, dst).hole_sewing);

    // Hole sewing only: the flag is copied; the copy pairs with whatever it encloses later.
    run(
        &mut s,
        "object.pasteAttributes",
        json!({"from": src, "id": dst, "attributes": ["holeSewing"]}),
    );
    let o = object(&s, dst);
    assert!(o.hole_sewing && o.hole_inner.is_none());
    assert_eq!(o.line.as_ref().map(|l| l.kind), Some(LineType::Zigzag));

    // A stitch pattern can be neither the source nor a target.
    let stitches = ids(&run(&mut s, "object.toStitches", json!({"ids": [dst]})));
    let pattern = stitches.first().copied().unwrap_or(dst);
    fails(
        &mut s,
        "object.pasteAttributes",
        json!({"from": pattern, "id": inner}),
    );
    fails(
        &mut s,
        "object.pasteAttributes",
        json!({"from": src, "id": pattern}),
    );
    fails(
        &mut s,
        "object.pasteAttributes",
        json!({"from": src, "id": inner, "attributes": ["everything"]}),
    );
}

#[test]
fn pasted_text_follows_the_path_pasted_with_it() {
    let mut s = Session::new(std::env::temp_dir());
    let path = id(&run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 50, "rx": 30, "ry": 20}),
    ));
    let text = id(&run(
        &mut s,
        "text.add",
        json!({"text": "ABC", "size": 8, "path": path}),
    ));
    assert_eq!(
        object(&s, text).text.as_ref().and_then(|t| t.path),
        Some(path)
    );
    let copied: Vec<Value> = [path, text]
        .into_iter()
        .map(|i| run(&mut s, "object.get", json!({"id": i})))
        .collect();

    let pasted = ids(&run(
        &mut s,
        "object.paste",
        json!({"objects": copied, "dx": 5}),
    ));
    assert_eq!(pasted.len(), 2);
    let new_path = pasted[0];
    let new_text = pasted[1];
    assert!(new_path != path && new_text != text);
    assert_eq!(
        object(&s, new_text).text.as_ref().and_then(|t| t.path),
        Some(new_path),
        "the pasted text follows the pasted path"
    );
    // The original text still follows the original path.
    assert_eq!(
        object(&s, text).text.as_ref().and_then(|t| t.path),
        Some(path)
    );

    // Pasted alone, the text keeps following the path that is still in the design.
    let alone = ids(&run(
        &mut s,
        "object.paste",
        json!({"objects": [copied[1].clone()]}),
    ));
    assert_eq!(
        object(&s, alone[0]).text.as_ref().and_then(|t| t.path),
        Some(path)
    );
}

#[test]
fn reverse_direction_reverses_a_curved_direction() {
    let mut s = Session::new(std::env::temp_dir());
    let r = id(&run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 40, "height": 30}),
    ));
    run(
        &mut s,
        "object.setRegion",
        json!({"id": r, "type": "fill", "underlay": false}),
    );
    run(
        &mut s,
        "object.setDirectionCurve",
        json!({"id": r, "points": [[0, 10], [20, 20], [40, 10]]}),
    );
    let curve = object(&s, r)
        .region
        .as_ref()
        .and_then(|g| g.direction_curve.clone())
        .expect("curve");
    let forward = stitch_points(&s);

    run(&mut s, "object.reverseDirection", json!({"id": r}));
    let reversed = object(&s, r)
        .region
        .as_ref()
        .and_then(|g| g.direction_curve.clone())
        .expect("still curved");
    let mut expected = curve.points.clone();
    expected.reverse();
    assert_eq!(reversed.points, expected, "the curve runs the other way");
    assert_ne!(stitch_points(&s), forward, "and sews the other way");

    run(&mut s, "object.reverseDirection", json!({"id": r}));
    assert_eq!(stitch_points(&s), forward, "twice is back where it was");
}

#[test]
fn export_reports_the_sewing_area_it_wrote() {
    let dir = temp_dir("area");
    let mut s = Session::new(&dir);
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 20, "height": 20}),
    );
    let page = run(&mut s, "design.export", json!({"path": "a.dst"}));
    assert_eq!(page["sewingArea"]["mode"], "page", "{}", page["sewingArea"]);

    run(&mut s, "machine.set", json!({"sewingArea": "design"}));
    let design = run(&mut s, "design.export", json!({"path": "b.dst"}));
    let area = &design["sewingArea"];
    assert_eq!(area["mode"], "design", "{area}");
    assert!(area["widthMM"].as_f64() < Some(25.0), "{area}");
    assert_eq!(
        area,
        &run(&mut s, "design.info", json!({}))["sewingArea"],
        "export and design.info agree"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn design_info_lists_needle_thread_per_colour() {
    let mut s = Session::new(std::env::temp_dir());
    for (x, colour) in [(0, "Red"), (40, "Blue")] {
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": x, "y": 0, "width": 30, "height": 30, "fill": colour}),
        );
    }
    let e = &run(&mut s, "design.info", json!({}))["estimates"];
    let per: Vec<f64> = e["threadPerColorM"]
        .as_array()
        .expect("threadPerColorM")
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    assert_eq!(per.len(), 2, "{e}");
    assert!(per.iter().all(|&m| m > 0.1), "{per:?}");
    let total = e["needleThreadM"].as_f64().expect("needle");
    assert!(
        (per.iter().sum::<f64>() - total).abs() <= 0.02,
        "{per:?} vs {total}"
    );
}

#[test]
fn start_end_type_and_line_exit_are_validated_and_saved() {
    let dir = temp_dir("startend");
    let mut s = Session::new(&dir);
    let o = id(&run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 40, "height": 20}),
    ));
    run(&mut s, "object.setLine", json!({"id": o, "type": "stem"}));
    fails(
        &mut s,
        "object.setLine",
        json!({"id": o, "startEnd": "sideways"}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": o, "startEnd": "run"}),
    );
    run(
        &mut s,
        "object.setEntryExit",
        json!({"id": o, "lineEntry": [0, 10], "lineExit": [40, 10]}),
    );
    let line = object(&s, o).line.clone().expect("line");
    assert_eq!(line.start_end, StartEnd::Run);
    assert!(line.entry.is_some() && line.exit.is_some());

    run(&mut s, "project.save", json!({"path": "se.osd"}));
    let mut t = Session::new(&dir);
    run(&mut t, "project.open", json!({"path": "se.osd"}));
    let back = t.design().objects[0].line.clone().expect("line");
    assert_eq!(back.start_end, StartEnd::Run);
    assert_eq!(back.entry, line.entry);
    assert_eq!(back.exit, line.exit);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_larger_ornament_size_makes_fewer_larger_ornaments() {
    let mut s = Session::new(std::env::temp_dir());
    let box_id = id(&run(
        &mut s,
        "shape.rectangle",
        json!({"x": 30, "y": 30, "width": 40, "height": 40}),
    ));
    let mut frames = Vec::new();
    for size in [3, 10] {
        let f = id(&run(
            &mut s,
            "shape.decorative",
            json!({"kind": "zigzag", "id": box_id, "margin": 2, "size": size}),
        ));
        let o = object(&s, f);
        frames.push((o.contours[0].points.len(), o.bounds().expect("bounds")));
    }
    let (small, large) = (&frames[0], &frames[1]);
    assert!(
        large.0 < small.0,
        "fewer zigzags at size 10: {} vs {}",
        large.0,
        small.0
    );
    assert!(
        large.1.width() > small.1.width(),
        "deeper zigzags reach further out: {:?} vs {:?}",
        large.1,
        small.1
    );
}
