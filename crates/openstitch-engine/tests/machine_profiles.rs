//! The user's machine (#508 part 3): profiles in the user settings, the hoop new designs start
//! on, and the export warnings; files are written exactly as without a profile.

#![cfg(feature = "formats")]

use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// Two filled shapes far apart: a trim between them, and long travel.
fn design(s: &mut Session) {
    run(
        s,
        "design.new",
        json!({"name": "Two", "width": 130, "height": 180}),
    );
    run(
        s,
        "shape.rectangle",
        json!({"x": 5, "y": 5, "width": 20, "height": 20, "fill": "#ff0000"}),
    );
    run(
        s,
        "shape.rectangle",
        json!({"x": 90, "y": 140, "width": 20, "height": 20, "fill": "#ff0000"}),
    );
}

#[test]
fn a_profile_starts_new_designs_on_its_hoop_and_persists() {
    let dir = TempDir::new("machine-profile");
    let file = dir.join("user.json");
    let mut s = Session::new(&dir);
    s.with_user_settings(&file).expect("settings");
    assert_eq!(
        run(&mut s, "machine.profile", json!({}))["profile"],
        Value::Null
    );
    let hoops = run(&mut s, "machine.hoops", json!({"brand": "Janome"}));
    let hoop = hoops["groups"][0]["hoops"][0]["id"]
        .as_str()
        .expect("a Janome hoop")
        .to_string();
    let r = run(
        &mut s,
        "machine.setProfile",
        json!({"brand": "janome", "hoop": hoop, "cutter": true, "maxStitchMm": 7}),
    );
    assert_eq!(r["profile"]["brand"], "Janome");
    assert_eq!(r["profile"]["format"], "jef");
    // design.new without a size starts on the hoop; with a size, as asked.
    run(&mut s, "design.new", json!({"name": "A"}));
    assert_eq!(s.design().machine.hoop.as_deref(), Some(hoop.as_str()));
    let (w, h) = s.design().machine.hoop_size().expect("hoop size");
    assert_eq!(
        (s.design().page.width, s.design().page.height),
        (w * 10.0, h * 10.0)
    );
    run(
        &mut s,
        "design.new",
        json!({"name": "B", "width": 50, "height": 40}),
    );
    assert_eq!(s.design().machine.hoop, None);
    assert_eq!(s.design().page.width, 500.0);
    // The next start reads it back.
    let mut t = Session::new(&dir);
    t.with_user_settings(&file).expect("load");
    assert_eq!(
        run(&mut t, "machine.profile", json!({}))["profile"]["hoop"],
        json!(hoop)
    );
    // Refusals, and clearing.
    for bad in [
        json!({"brand": "Acme"}),
        json!({"brand": "Brother", "hoop": hoop}),
        json!({"brand": "Brother", "maxStitchMm": 40}),
    ] {
        assert!(
            t.execute("machine.setProfile", Some(&bad)).is_err(),
            "{bad}"
        );
    }
    run(&mut t, "machine.setProfile", json!({"brand": "none"}));
    assert_eq!(
        run(&mut t, "machine.profile", json!({}))["profile"],
        Value::Null
    );
    run(&mut t, "design.new", json!({"name": "C"}));
    assert_eq!(t.design().machine.hoop, None, "no profile: as before");
}

#[test]
fn exports_warn_for_the_users_machine_and_write_the_same_bytes() {
    let dir = TempDir::new("machine-profile-export");
    let mut s = Session::new(&dir);
    design(&mut s);
    let plain = run(&mut s, "design.export", json!({"path": "same.pes"}));
    let plain_bytes = std::fs::read(dir.join("same.pes")).expect("pes");
    // No cutter and a 1 mm longest stitch: both warnings.
    run(
        &mut s,
        "machine.setProfile",
        json!({"brand": "Brother", "cutter": false, "maxStitchMm": 1}),
    );
    let warned = run(&mut s, "design.export", json!({"path": "same.pes"}));
    assert_eq!(
        std::fs::read(dir.join("same.pes")).expect("pes"),
        plain_bytes,
        "a profile never changes the file (same name: PES keeps it in its label)"
    );
    let warnings: Vec<&str> = warned["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let before = plain["warnings"].as_array().map_or(0, Vec::len);
    assert_eq!(warnings.len(), before + 2, "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("longer than 1 mm")));
    assert!(warnings.iter().any(|w| w.contains("no thread cutter")));
    // A cutter and a generous limit: nothing to say.
    run(
        &mut s,
        "machine.setProfile",
        json!({"brand": "Brother", "cutter": true, "maxStitchMm": 12}),
    );
    let quiet = run(&mut s, "design.export", json!({"path": "quiet.pes"}));
    assert_eq!(quiet["warnings"], plain["warnings"]);
    // Stop at Jumps answers the missing cutter.
    run(&mut s, "machine.setProfile", json!({"brand": "Brother"}));
    run(&mut s, "design.setStopAtJumps", json!({"on": true}));
    let stops = run(&mut s, "design.export", json!({"path": "stops.pes"}));
    assert!(
        !stops["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|w| w.as_str().is_some_and(|w| w.contains("no thread cutter")))
    );
}
