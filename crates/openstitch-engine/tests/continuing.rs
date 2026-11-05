//! Continuing a line, satin skeleton or Manual Punch column from either end (#479).

use openstitch_core::Command;
use openstitch_design::{Compiler, DesignObject, v2};
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

fn id(v: &Value) -> u64 {
    v["id"].as_u64().expect("id")
}

fn object(s: &Session, id: u64) -> &DesignObject {
    let d = s.design();
    &d.objects[d.index_of(id).expect("object")]
}

#[test]
fn continuing_a_line_from_its_end_keeps_the_object() {
    let mut s = Session::new(std::env::temp_dir());
    let l = id(&run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[10, 10], [20, 10]], "closed": false, "outline": "#ff0000" }),
    ));
    let line = object(&s, l).line.clone();
    let count = s.design().objects.len();
    run(
        &mut s,
        "object.extendPath",
        json!({ "id": l, "end": "end", "points": [[30, 10], [30, 20]] }),
    );
    let o = object(&s, l);
    assert_eq!(o.contours[0].points.len(), 4);
    assert_eq!(o.contours[0].points[3], v2(300.0, 200.0));
    assert_eq!(o.line, line);
    assert_eq!(s.design().objects.len(), count);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(object(&s, l).contours[0].points.len(), 2);
}

#[test]
fn continuing_from_the_start_prepends_and_sews_from_the_new_start() {
    let mut s = Session::new(std::env::temp_dir());
    let l = id(&run(
        &mut s,
        "shape.path",
        json!({ "points": [[10, 10], [20, 10]], "closed": false }),
    ));
    run(
        &mut s,
        "object.setEntryExit",
        json!({ "id": l, "lineEntry": [10, 10] }),
    );
    run(
        &mut s,
        "object.extendPath",
        json!({ "id": l, "end": "start", "points": [[0, 10], [-10, 0]], "curves": [[[7, 13], [3, 13]], null] }),
    );
    let o = object(&s, l);
    let c = &o.contours[0];
    assert_eq!(
        c.points,
        vec![
            v2(-100.0, 0.0),
            v2(0.0, 100.0),
            v2(100.0, 100.0),
            v2(200.0, 100.0)
        ]
    );
    let z = c.curve(1).expect("curved segment");
    assert_eq!((z.c1, z.c2), (v2(30.0, 130.0), v2(70.0, 130.0)));
    assert_eq!(o.line.as_ref().expect("value").entry, Some(v2(-100.0, 0.0)));
    let d = s.design();
    let first = Compiler::for_design(d)
        .compile(d)
        .stitches
        .into_iter()
        .find(|st| st.command == Command::Stitch)
        .expect("stitches");
    // The first stitch is the tie-in beside the new start.
    assert!(
        v2(first.x, first.y).distance(v2(-100.0, 0.0)) < 5.0,
        "{first:?}"
    );
}

#[test]
fn closed_shapes_and_curve_counts_are_refused() {
    let mut s = Session::new(std::env::temp_dir());
    let r = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 10, "height": 10 }),
    ));
    assert!(
        fails(
            &mut s,
            "object.extendPath",
            json!({ "id": r, "end": "end", "points": [[20, 20]] })
        )
        .contains("closed")
    );
    let l = id(&run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[0, 0], [10, 0]], "closed": false }),
    ));
    assert!(
        fails(
            &mut s,
            "object.extendPath",
            json!({ "id": l, "end": "end", "points": [[20, 0]], "curves": [null, null] })
        )
        .contains("one entry per new segment")
    );
    assert!(
        fails(
            &mut s,
            "object.extendPath",
            json!({ "id": l, "end": "middle", "points": [[20, 0]] })
        )
        .contains("start or end")
    );
    assert!(
        fails(
            &mut s,
            "object.extendColumn",
            json!({ "id": l, "end": "end", "lines": [[[0, 0], [0, 5]]] })
        )
        .contains("Manual Punch")
    );
}

#[test]
fn continuing_a_satin_skeleton_keeps_its_widths() {
    let mut s = Session::new(std::env::temp_dir());
    let k = id(&run(
        &mut s,
        "shape.skeletonSatin",
        json!({ "points": [[10, 10], [30, 10]], "width": 4 }),
    ));
    run(
        &mut s,
        "object.extendPath",
        json!({ "id": k, "end": "end", "points": [[40, 20]] }),
    );
    let sk = object(&s, k)
        .region
        .as_ref()
        .expect("value")
        .skeleton
        .clone()
        .expect("value");
    assert_eq!(sk.points.len(), 3);
    assert_eq!(sk.widths, vec![4.0, 4.0, 4.0]);
    assert_eq!(
        object(&s, k).contours[0],
        openstitch_design::skeleton::outline(&sk).expect("value")
    );
    run(
        &mut s,
        "object.extendPath",
        json!({ "id": k, "end": "start", "points": [[0, 10]], "widths": [2] }),
    );
    let sk = object(&s, k)
        .region
        .as_ref()
        .expect("value")
        .skeleton
        .clone()
        .expect("value");
    assert_eq!(sk.points[0], v2(0.0, 100.0));
    assert_eq!(sk.widths[0], 2.0);
}

#[test]
fn continuing_a_column_adds_valid_direction_lines() {
    let mut s = Session::new(std::env::temp_dir());
    let c = id(&run(
        &mut s,
        "shape.column",
        json!({ "lines": [[[10, 10], [10, 14]], [[14, 10], [14, 14]]] }),
    ));
    run(
        &mut s,
        "object.extendColumn",
        json!({ "id": c, "end": "end", "lines": [[[18, 10], [18, 14]], [[22, 11], [22, 15]]] }),
    );
    run(
        &mut s,
        "object.extendColumn",
        json!({ "id": c, "end": "start", "lines": [[[6, 10], [6, 14]]] }),
    );
    let o = object(&s, c);
    let r = o.region.as_ref().expect("value");
    assert_eq!(r.direction_lines.len(), 5);
    assert_eq!(r.direction_lines[0], [v2(60.0, 100.0), v2(60.0, 140.0)]);
    assert_eq!(
        o.contours[0],
        openstitch_design::column::outline(&r.direction_lines)
    );
    let e = fails(
        &mut s,
        "object.extendColumn",
        json!({ "id": c, "end": "end", "lines": [[[26, 15], [26, 11]]] }),
    );
    assert!(e.contains("cross"), "{e}");
    // The refused continuation left the column as it was.
    assert_eq!(
        object(&s, c)
            .region
            .as_ref()
            .expect("value")
            .direction_lines
            .len(),
        5
    );
}
