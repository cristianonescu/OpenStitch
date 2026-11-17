//! Join Lines and Break Apart (#744): `shape.joinLines` and `object.breakApart`.

use openstitch_core::Command;
use openstitch_design::{Compiler, DesignObject, ObjectKind, v2};
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

fn line(s: &mut Session, points: Value) -> u64 {
    id(&run(
        s,
        "shape.polygon",
        json!({ "points": points, "closed": false, "outline": "#203040" }),
    ))
}

fn compiled(s: &Session) -> openstitch_core::StitchPattern {
    Compiler::for_design(s.design()).compile(s.design())
}

fn round_trip(s: &Session) {
    let saved = openstitch_engine::project::encode(s.design()).expect("encode");
    let back = openstitch_engine::project::decode(&saved).expect("decode");
    assert_eq!(&back, s.design());
}

#[test]
fn joining_two_lines_sews_one_path_without_a_trim() {
    let mut s = Session::new(std::env::temp_dir());
    let a = line(&mut s, json!([[10, 10], [40, 10]]));
    // Drawn the other way, its end 0.5 mm from a's end.
    let b = line(&mut s, json!([[40, 40], [40.5, 10]]));
    let before = compiled(&s);
    assert!(
        before.count(Command::Trim) + before.count(Command::Jump) > 0,
        "two lines start apart"
    );
    let line_attrs = object(&s, a).line.clone();
    let r = run(&mut s, "shape.joinLines", json!({ "ids": [a, b] }));
    assert_eq!(id(&r), a);
    assert_eq!(r["removed"], json!([b]));
    assert_eq!(s.design().objects.len(), 1);
    let o = object(&s, a);
    assert_eq!(o.contours.len(), 1);
    let c = &o.contours[0];
    assert!(!c.closed);
    // The nearest ends meet: a's end is bridged to b's (reversed) start.
    assert_eq!(
        c.points,
        vec![
            v2(100.0, 100.0),
            v2(400.0, 100.0),
            v2(405.0, 100.0),
            v2(400.0, 400.0)
        ]
    );
    assert_eq!(o.line, line_attrs, "the first line's attributes win");
    let after = compiled(&s);
    // Sewn exactly as the same line drawn in one go: one tie-in, one tie-off, no trim or
    // jump between the parts.
    let mut one = Session::new(std::env::temp_dir());
    line(&mut one, json!([[10, 10], [40, 10], [40.5, 10], [40, 40]]));
    let reference = compiled(&one);
    assert_eq!(after.stitches, reference.stitches);
    assert!(after.count(Command::Trim) < before.count(Command::Trim));
    round_trip(&s);
    // One undo step brings both back.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design().objects.len(), 2);
    assert_eq!(object(&s, b).contours[0].points[0], v2(400.0, 400.0));
}

#[test]
fn joining_chains_several_lines_at_either_end() {
    let mut s = Session::new(std::env::temp_dir());
    let a = line(&mut s, json!([[20, 10], [30, 10]]));
    let b = line(&mut s, json!([[10, 10], [20, 10]]));
    let c = line(&mut s, json!([[40, 10], [30, 10]]));
    run(&mut s, "shape.joinLines", json!({ "ids": [a, b, c] }));
    let pts = &object(&s, a).contours[0].points;
    assert_eq!(
        pts,
        &vec![
            v2(100.0, 100.0),
            v2(200.0, 100.0),
            v2(300.0, 100.0),
            v2(400.0, 100.0)
        ],
        "touching ends are one point, the first line's direction kept"
    );
}

#[test]
fn ends_beyond_the_tolerance_are_refused_with_the_distance() {
    let mut s = Session::new(std::env::temp_dir());
    let a = line(&mut s, json!([[10, 10], [20, 10]]));
    let b = line(&mut s, json!([[25, 10], [35, 10]]));
    let e = fails(&mut s, "shape.joinLines", json!({ "ids": [a, b] }));
    assert!(e.contains("5.00 mm"), "{e}");
    assert_eq!(s.design().objects.len(), 2, "nothing changed");
    run(
        &mut s,
        "shape.joinLines",
        json!({ "ids": [a, b], "tolerance": 6 }),
    );
    assert_eq!(s.design().objects.len(), 1);
}

#[test]
fn only_open_lines_of_one_kind_join() {
    let mut s = Session::new(std::env::temp_dir());
    let a = line(&mut s, json!([[10, 10], [20, 10]]));
    let r = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 20, "y": 10, "width": 5, "height": 5, "fill": "Red" }),
    ));
    let k = id(&run(
        &mut s,
        "shape.skeletonSatin",
        json!({ "points": [[20, 10], [30, 10]], "width": 3 }),
    ));
    assert!(fails(&mut s, "shape.joinLines", json!({ "ids": [a, r] })).contains("closed"));
    assert!(fails(&mut s, "shape.joinLines", json!({ "ids": [a, k] })).contains("one kind"));
    assert!(fails(&mut s, "shape.joinLines", json!({ "ids": [a] })).contains("at least 2"));
    assert!(fails(&mut s, "shape.joinLines", json!({ "ids": [a, a] })).contains("at least 2"));
}

#[test]
fn joining_satin_skeletons_keeps_the_column_continuous() {
    let mut s = Session::new(std::env::temp_dir());
    let a = id(&run(
        &mut s,
        "shape.skeletonSatin",
        json!({ "points": [[10, 10], [30, 10]], "width": 4 }),
    ));
    let b = id(&run(
        &mut s,
        "shape.skeletonSatin",
        json!({ "points": [[50, 20], [30, 10]], "width": 2 }),
    ));
    run(&mut s, "shape.joinLines", json!({ "ids": [a, b] }));
    assert_eq!(s.design().objects.len(), 1);
    let o = object(&s, a);
    let sk = o
        .region
        .as_ref()
        .and_then(|r| r.skeleton.clone())
        .expect("skeleton");
    // The shared point is kept once; b is turned to continue from a's end.
    assert_eq!(
        sk.points,
        vec![v2(100.0, 100.0), v2(300.0, 100.0), v2(500.0, 200.0)]
    );
    assert_eq!(sk.widths, vec![4.0, 4.0, 2.0]);
    // One outline, made from the joined centre line: both rails run on through the seam.
    assert_eq!(o.contours.len(), 1);
    assert_eq!(
        o.contours[0],
        openstitch_design::skeleton::outline(&sk).expect("outline")
    );
    let p = compiled(&s);
    // One column: the closing trim and the move to its start only.
    assert_eq!(p.count(Command::Trim), 1);
    assert!(p.count(Command::Jump) <= 1);
    round_trip(&s);
}

#[test]
fn joining_punch_columns_continues_the_direction_lines() {
    let mut s = Session::new(std::env::temp_dir());
    let a = id(&run(
        &mut s,
        "shape.column",
        json!({ "lines": [[[10, 10], [10, 14]], [[14, 10], [14, 14]]] }),
    ));
    // Drawn from the far end, its ends on the other sides.
    let b = id(&run(
        &mut s,
        "shape.column",
        json!({ "lines": [[[22, 14], [22, 10]], [[18, 14], [18, 10]], [[14, 14], [14, 10]]] }),
    ));
    run(&mut s, "shape.joinLines", json!({ "ids": [a, b] }));
    let o = object(&s, a);
    let r = o.region.as_ref().expect("region");
    assert_eq!(
        r.direction_lines,
        vec![
            [v2(100.0, 100.0), v2(100.0, 140.0)],
            [v2(140.0, 100.0), v2(140.0, 140.0)],
            [v2(180.0, 100.0), v2(180.0, 140.0)],
            [v2(220.0, 100.0), v2(220.0, 140.0)],
        ]
    );
    assert_eq!(
        o.contours[0],
        openstitch_design::column::outline(&r.direction_lines)
    );
}

#[test]
fn breaking_text_apart_gives_one_text_per_letter_in_place() {
    let mut s = Session::new(std::env::temp_dir());
    let t = id(&run(
        &mut s,
        "text.add",
        json!({ "text": "AB C", "size": 10, "x": 50, "y": 50, "fill": "Red", "name": "word" }),
    ));
    run(
        &mut s,
        "text.setCharacter",
        json!({ "id": t, "index": 1, "rotation": 15, "y": 1 }),
    );
    let whole = object(&s, t).clone();
    let groups: Vec<Vec<_>> = whole.glyph_groups().iter().map(|g| g.to_vec()).collect();
    assert_eq!(groups.len(), 3);
    let r = run(&mut s, "object.breakApart", json!({ "ids": [t] }));
    let ids: Vec<u64> = r["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .map(|v| v.as_u64().expect("id"))
        .collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(ids[0], t, "the first letter keeps the text's id");
    assert_eq!(r["text"], json!([{ "id": t, "as": "text" }]));
    for (k, (&id, want)) in ids.iter().zip(&groups).enumerate() {
        let o = object(&s, id);
        assert_eq!(o.kind, ObjectKind::Text);
        let spec = o.text.as_ref().expect("text");
        assert_eq!(spec.text, ["A", "B", "C"][k]);
        assert_eq!(o.region, whole.region, "same sewing attributes");
        assert_eq!(o.line, whole.line);
        assert_eq!(
            o.name.as_deref(),
            Some(format!("word '{}'", spec.text).as_str())
        );
        for (a, b) in o.contours.iter().zip(want) {
            for (p, q) in a.points.iter().zip(&b.points) {
                assert!(p.distance(*q) <= 0.5, "letter {k} moved: {p:?} vs {q:?}");
            }
        }
    }
    // The B keeps its own rotation and offset.
    let b = object(&s, ids[1]).text.as_ref().expect("text");
    assert_eq!(b.characters.len(), 1);
    assert_eq!(b.characters[0].index, 0);
    assert_eq!(b.characters[0].rotation, 15.0);
    // Sewing order: the letters stand where the text stood.
    let order: Vec<u64> = s.design().objects.iter().map(|o| o.id).collect();
    assert_eq!(order, ids);
    round_trip(&s);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design().objects.len(), 1);
    assert_eq!(object(&s, t).text.as_ref().expect("text").text, "AB C");
}

#[test]
fn text_in_a_text_transform_breaks_into_letter_shapes() {
    let mut s = Session::new(std::env::temp_dir());
    let t = id(&run(
        &mut s,
        "text.add",
        json!({ "text": "WAVE", "size": 10, "x": 50, "y": 50 }),
    ));
    run(&mut s, "object.setText", json!({ "id": t, "envelope": 3 }));
    let groups = object(&s, t).glyph_groups().len();
    let r = run(&mut s, "object.breakApart", json!({ "ids": [t] }));
    assert_eq!(r["text"], json!([{ "id": t, "as": "shapes" }]));
    assert_eq!(r["ids"].as_array().expect("ids").len(), groups);
    assert!(
        s.design()
            .objects
            .iter()
            .all(|o| o.kind == ObjectKind::Shape && o.text.is_none())
    );
}

#[test]
fn a_region_with_two_islands_breaks_into_two_objects() {
    let mut s = Session::new(std::env::temp_dir());
    let a = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 20, "height": 20, "fill": "Blue" }),
    ));
    // A ring: its hole stays with it.
    let b = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 50, "y": 10, "width": 20, "height": 20 }),
    ));
    let hole = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 55, "y": 15, "width": 10, "height": 10 }),
    ));
    run(
        &mut s,
        "shape.combine",
        json!({ "ids": [b, hole], "op": "difference" }),
    );
    run(
        &mut s,
        "shape.combine",
        json!({ "ids": [a, b], "op": "union" }),
    );
    assert_eq!(s.design().objects.len(), 1);
    let whole = object(&s, a).clone();
    assert_eq!(whole.contours.len(), 3);
    let r = run(&mut s, "object.breakApart", json!({ "ids": [a] }));
    let ids: Vec<u64> = r["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .map(|v| v.as_u64().expect("id"))
        .collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], a);
    let (left, right) = (object(&s, ids[0]), object(&s, ids[1]));
    assert_eq!(left.contours.len(), 1, "the square");
    assert_eq!(right.contours.len(), 2, "the ring keeps its hole");
    // Same attributes; an entry or exit point stays only on the piece whose outline it is on.
    let plain = |o: &DesignObject| {
        let mut r = o.region.clone().expect("region");
        (r.entry, r.exit) = (None, None);
        r
    };
    for o in [left, right] {
        assert_eq!(plain(o), plain(&whole), "same attributes");
        assert_eq!(o.line, whole.line);
    }
    round_trip(&s);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design().objects.len(), 1);
    assert_eq!(object(&s, a).contours.len(), 3);
}

#[test]
fn grouped_copies_along_a_line_break_into_separate_copies() {
    let mut s = Session::new(std::env::temp_dir());
    let m1 = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 4, "height": 4, "fill": "Red" }),
    ));
    let m2 = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 5, "y": 0, "width": 4, "height": 4, "fill": "Red" }),
    ));
    run(&mut s, "object.group", json!({ "ids": [m1, m2] }));
    let guide = line(&mut s, json!([[0, 50], [80, 50]]));
    let r = run(
        &mut s,
        "object.arrangeCopies",
        json!({ "ids": [m1, m2], "mode": "path", "pathId": guide, "count": 3 }),
    );
    let copies: Vec<u64> = r["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .map(|v| v.as_u64().expect("id"))
        .collect();
    assert!(object(&s, copies[0]).group.is_some());
    let r = run(&mut s, "object.breakApart", json!({ "ids": copies }));
    assert!(!r["ungrouped"].as_array().expect("groups").is_empty());
    for id in copies {
        assert_eq!(object(&s, id).group, None);
    }
}

#[test]
fn a_single_piece_is_refused() {
    let mut s = Session::new(std::env::temp_dir());
    let a = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 20, "height": 20, "fill": "Blue" }),
    ));
    let e = fails(&mut s, "object.breakApart", json!({ "ids": [a] }));
    assert!(e.contains("nothing to break apart"), "{e}");
}
