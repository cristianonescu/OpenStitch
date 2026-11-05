//! Templates and the Template Wizard: slots are Name Drop texts; built-in and user templates
//! are listed, opened and filled.

use openstitch_design::Rect;
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn temp_dir() -> TempDir {
    TempDir::new("templates")
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn text_of(s: &Session, id: u64) -> String {
    let d = s.design();
    d.objects[d.index_of(id).expect("o")]
        .text
        .as_ref()
        .expect("text")
        .text
        .clone()
}

fn bounds(s: &Session, id: u64) -> Rect {
    let d = s.design();
    d.objects[d.index_of(id).expect("o")]
        .bounds()
        .expect("bounds")
}

#[test]
fn builtin_templates_open_fill_and_sew() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    let list = run(&mut s, "template.list", json!({}));
    let templates = list["templates"].as_array().expect("templates");
    assert_eq!(templates.len(), 4, "{list}");
    for t in templates {
        let id = t["id"].as_str().expect("id");
        let opened = run(&mut s, "template.open", json!({ "template": id }));
        // The built-ins are version 2 (regenerated with the current engine, #450: the
        // version-1 files sewed with old defaults), so their texts use PE-DESIGN's metrics.
        assert!(
            s.design()
                .objects
                .iter()
                .filter_map(|o| o.text.as_ref())
                .all(|t| t.metrics == openstitch_design::TextMetrics::Pe),
            "{id}"
        );
        let slots = opened["slots"].as_array().expect("slots");
        assert!(!slots.is_empty(), "{id}");
        let values: Vec<String> = (0..slots.len()).map(|k| format!("Row {k}")).collect();
        let filled = run(&mut s, "template.fill", json!({ "values": values }));
        for (k, slot) in filled["slots"]
            .as_array()
            .expect("slots")
            .iter()
            .enumerate()
        {
            assert_eq!(slot["text"], format!("Row {k}"));
        }
        let stats = run(&mut s, "design.generate", json!({}));
        assert!(stats["stitches"].as_u64() > Some(200), "{id}: {stats}");
    }
}

#[test]
fn slots_are_substituted_by_label_and_keep_their_box() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    let opened = run(
        &mut s,
        "template.open",
        json!({"template": "builtin:team-badge"}),
    );
    let slots = opened["slots"].as_array().expect("slots");
    assert_eq!(slots[0]["label"], "Name");
    assert_eq!(slots[1]["label"], "Team");
    let name = slots[0]["id"].as_u64().expect("id");
    let team = slots[1]["id"].as_u64().expect("id");
    let before = bounds(&s, name);
    run(
        &mut s,
        "template.fill",
        json!({"values": {"Name": "Maximilian"}}),
    );
    assert_eq!(text_of(&s, name), "Maximilian");
    assert_eq!(text_of(&s, team), "TEAM", "unnamed slots keep their text");
    let after = bounds(&s, name);
    assert!(after.min_x >= before.min_x - 0.5 && after.max_x <= before.max_x + 0.5);
    assert!((after.center().x - before.center().x).abs() < 0.5);
    // One undo step per fill.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(text_of(&s, name), "Player");
    for bad in [
        json!({"values": {"Nickname": "x"}}),
        json!({"values": {"Name": ""}}),
        json!({"values": {"Name": 5}}),
        json!({"values": ["only one"]}),
        json!({"values": "Max"}),
    ] {
        let d = s.design().clone();
        assert!(s.execute("template.fill", Some(&bad)).is_err(), "{bad}");
        assert_eq!(s.design(), &d);
    }
}

#[test]
fn designs_save_as_templates_and_user_folders_are_listed() {
    let dir = temp_dir();
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"name": "Club"}));
    let t = run(&mut s, "text.add", json!({"text": "Club name"}))["id"]
        .as_u64()
        .expect("id");
    assert!(
        s.execute("template.save", Some(&json!({"path": "x.ostemplate"})))
            .is_err(),
        "no slots yet"
    );
    run(
        &mut s,
        "text.setNameDrop",
        json!({"id": t, "label": "Club"}),
    );
    std::fs::create_dir_all(dir.join("lib/Sports")).expect("dir");
    run(
        &mut s,
        "template.save",
        json!({"path": "lib/Sports/club.ostemplate", "category": "Sports"}),
    );
    run(
        &mut s,
        "template.save",
        json!({"path": "lib/loose.ostemplate", "name": "Loose"}),
    );
    // Hostile and unrelated files are skipped with a reason.
    std::fs::write(dir.join("lib/Sports/broken.ostemplate"), b"{\"format\":").expect("w");
    std::fs::write(
        dir.join("lib/Sports/design.ostemplate"),
        br#"{"format":"openstitch.design","version":1}"#,
    )
    .expect("w");
    std::fs::write(dir.join("lib/readme.txt"), b"hello").expect("w");
    let list = run(&mut s, "template.list", json!({"dir": "lib"}));
    let names: Vec<&str> = list["templates"]
        .as_array()
        .expect("t")
        .iter()
        .filter(|t| t["source"] == "file")
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names.len(), 2, "{list}");
    assert!(names.contains(&"Club") && names.contains(&"Loose"));
    assert_eq!(list["skipped"].as_array().expect("s").len(), 2);

    let mut t2 = Session::new(&dir);
    let opened = run(
        &mut t2,
        "template.open",
        json!({"template": "lib/Sports/club.ostemplate"}),
    );
    assert_eq!(opened["category"], "Sports");
    run(
        &mut t2,
        "template.fill",
        json!({"values": {"Club": "Harriers"}}),
    );
    run(&mut t2, "design.export", json!({"path": "club.pes"}));
    assert!(dir.join("club.pes").exists());
}

#[test]
fn template_bad_input_fails_cleanly() {
    let dir = temp_dir();
    std::fs::write(dir.join("bad.ostemplate"), b"garbage").expect("w");
    let mut s = Session::new(&dir);
    let before = s.design().clone();
    for (id, params) in [
        ("template.open", json!({})),
        ("template.open", json!({"template": "builtin:nope"})),
        ("template.open", json!({"template": "bad.ostemplate"})),
        ("template.open", json!({"template": "missing.ostemplate"})),
        ("template.fill", json!({"values": ["x"]})),
        ("template.list", json!({"dir": "missing"})),
        ("template.save", json!({"path": "x.ostemplate"})),
    ] {
        assert!(s.execute(id, Some(&params)).is_err(), "{id} {params}");
        assert_eq!(s.design(), &before);
    }
    let mut t = Session::sandboxed(&dir);
    assert!(
        t.execute("template.open", Some(&json!({"template": "/etc/passwd"})))
            .is_err()
    );
}

#[test]
fn version_1_templates_keep_their_capital_height_metrics() {
    let v2 = include_str!("../templates/team-badge.ostemplate");
    let v1 = v2.replacen("\"version\": 2", "\"version\": 1", 1);
    assert_ne!(v1, v2);
    let t = openstitch_engine::templates::decode(v1.as_bytes()).expect("decode");
    assert!(
        t.design
            .objects
            .iter()
            .filter_map(|o| o.text.as_ref())
            .all(|t| t.metrics == openstitch_design::TextMetrics::CapHeight)
    );
}
