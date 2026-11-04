//! Coverage for the Design Database search criteria no other test asserts (#331): date
//! modified, and PES properties needing every word (`property_search_needs_every_word` checks
//! the words only when the file's summary has keywords).
#![cfg(feature = "library")]

use openstitch_engine::Session;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "openstitch-coverage-library-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.canonicalize().expect("canonical")
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn names(result: &Value) -> Vec<String> {
    let mut v: Vec<String> = result["designs"]
        .as_array()
        .expect("designs")
        .iter()
        .filter_map(|d| d["name"].as_str().map(str::to_string))
        .collect();
    v.sort();
    v
}

fn p(path: &Path) -> String {
    path.display().to_string()
}

/// Writes `name` as a one-square PES with `keywords`, last modified at `secs` after 1970.
fn design(dir: &Path, name: &str, keywords: &str, secs: u64) {
    let mut s = Session::new(dir);
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 20, "height": 20}),
    );
    if !keywords.is_empty() {
        run(
            &mut s,
            "design.setProperties",
            json!({"keywords": keywords}),
        );
    }
    run(&mut s, "design.export", json!({"path": name}));
    std::fs::File::options()
        .write(true)
        .open(dir.join(name))
        .and_then(|f| f.set_modified(UNIX_EPOCH + Duration::from_secs(secs)))
        .expect("set modified");
}

const Y2020: u64 = 1_577_836_800; // 2020-01-01
const Y2022: u64 = 1_640_995_200; // 2022-01-01
const Y2023: u64 = 1_685_577_600; // 2023-06-01
const Y2024: u64 = 1_704_067_200; // 2024-01-01

#[test]
fn search_by_date_modified_keeps_the_designs_in_the_range() {
    let dir = temp_dir("modified");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    design(&dir, "old.pes", "", Y2020);
    design(&dir, "mid.pes", "", Y2023);
    design(&dir, "new.pes", "", now);
    let mut s = Session::new(&dir);
    let search = |s: &mut Session, range: Value| {
        names(&run(
            s,
            "library.search",
            json!({"path": p(&dir), "modified": range}),
        ))
    };
    assert_eq!(
        search(&mut s, json!({"min": Y2022, "max": Y2024})),
        ["mid.pes"]
    );
    assert_eq!(
        search(&mut s, json!({"min": Y2022})),
        ["mid.pes", "new.pes"]
    );
    assert_eq!(search(&mut s, json!({"max": Y2022})), ["old.pes"]);
    assert_eq!(search(&mut s, json!({})), ["mid.pes", "new.pes", "old.pes"]);
    // Combined with another criterion, both must hold.
    let both = run(
        &mut s,
        "library.search",
        json!({"path": p(&dir), "modified": {"min": Y2022}, "name": "n*"}),
    );
    assert_eq!(names(&both), ["new.pes"]);
    for bad in [json!({"min": -1}), json!({"max": "2023"}), json!(5)] {
        assert!(
            s.execute(
                "library.search",
                Some(&json!({"path": p(&dir), "modified": bad}))
            )
            .is_err(),
            "{bad} refused"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn property_search_needs_every_word_in_any_order_and_case() {
    let dir = temp_dir("properties");
    design(&dir, "flower.pes", "Flower Spring red", Y2023);
    design(&dir, "plain.pes", "", Y2023);
    let mut s = Session::new(&dir);
    let info = run(&mut s, "library.info", json!({"path": "flower.pes"}));
    assert!(
        info.to_string().contains("Flower Spring red"),
        "the keywords reach the file: {info}"
    );
    let search = |s: &mut Session, words: &str| {
        names(&run(
            s,
            "library.search",
            json!({"path": p(&dir), "properties": {"keywords": words}}),
        ))
    };
    assert_eq!(search(&mut s, "spring flower"), ["flower.pes"]);
    assert_eq!(search(&mut s, "RED"), ["flower.pes"]);
    assert!(search(&mut s, "spring winter").is_empty(), "every word");
    assert!(search(&mut s, "summer").is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
