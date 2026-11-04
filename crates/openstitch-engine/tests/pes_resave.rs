//! Re-saving PE-DESIGN's own PES files (#375, docs/COMPATIBILITY_AUDIT.md item 9). Opening a
//! PE-DESIGN 11 file (`stitches.import` with `keepSource`, as File ▸ Open does) and exporting
//! it unchanged as PES writes the file back byte for byte: its editable objects, Design
//! Property, page, hoop and thread table stay as PE-DESIGN wrote them. Evidence: every
//! `#PES0110` file of PE-DESIGN's in compat/reference and the pe11 fixtures.
#![cfg(feature = "formats")]

use openstitch_engine::Session;
use openstitch_formats::pes;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("dir").flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pes")) {
            out.push(p);
        }
    }
}

/// PE-DESIGN's `#PES0110` files with a PEC section OpenStitch reads.
fn pe_design_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut all = Vec::new();
    walk(&root.join("compat/reference"), &mut all);
    walk(
        &root.join("crates/openstitch-formats/tests/fixtures/pe11"),
        &mut all,
    );
    all.sort();
    all.retain(|p| {
        let b = std::fs::read(p).expect("read");
        b.starts_with(b"#PES0110") && pes::read(&b).is_ok_and(|p| !p.stitches.is_empty())
    });
    all
}

/// File ▸ Open: a new design with the file kept.
fn open(dir: &Path, file: &Path) -> Session {
    std::fs::copy(file, dir.join("in.pes")).expect("copy");
    let mut s = Session::new(dir);
    run(
        &mut s,
        "design.new",
        json!({"name": "in", "width": 100, "height": 100}),
    );
    run(
        &mut s,
        "stitches.import",
        json!({"path": "in.pes", "keepSource": true}),
    );
    s
}

#[test]
fn an_unchanged_pe_design_file_is_written_back_byte_for_byte() {
    let files = pe_design_files();
    assert!(files.len() > 400, "{} files", files.len());
    let dir = TempDir::canonical("resave-all");
    for file in &files {
        let mut s = open(&dir, file);
        let original = std::fs::read(file).expect("read");
        // No version asked for, and PE-DESIGN 11's own: the file as it was.
        for params in [
            json!({"path": "a.pes"}),
            json!({"path": "a.pes", "pesVersion": "11"}),
        ] {
            let r = run(&mut s, "design.export", params);
            assert_eq!(r["source"], "unchanged", "{}", file.display());
            assert!(
                std::fs::read(dir.join("a.pes")).expect("out") == original,
                "{} not written back unchanged",
                file.display()
            );
        }
    }
}

#[test]
fn the_kept_file_survives_a_project_save_and_another_version_is_converted() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let file = root.join("compat/reference/pes/design-property-native.pes");
    let original = std::fs::read(&file).expect("read");
    let dir = TempDir::canonical("resave-osd");
    let mut s = open(&dir, &file);
    run(&mut s, "project.save", json!({"path": "a.osd"}));
    let mut s = Session::new(dir.path());
    run(&mut s, "project.open", json!({"path": "a.osd"}));
    run(&mut s, "design.export", json!({"path": "a.pes"}));
    assert!(std::fs::read(dir.join("a.pes")).expect("a") == original);
    // Version 6 asked for: written from the design, with the Design Property it adopted.
    let r = run(
        &mut s,
        "design.export",
        json!({"path": "b.pes", "pesVersion": "6"}),
    );
    assert!(r.get("source").is_none());
    let b = pes::read_objects(&std::fs::read(dir.join("b.pes")).expect("b")).expect("b");
    assert_eq!(b.version, "0060");
    let pe = pes::read_objects(&original).expect("pe");
    for key in ["category", "author", "keywords", "comments"] {
        assert_eq!(b.metadata.get(key), pe.metadata.get(key), "{key}");
    }
    // The thread table's colours reach the stitches: PE's catalogue numbers and RGB.
    let pe_threads: Vec<_> = pe
        .threads
        .iter()
        .map(|t| (t.rgb_value(), t.catalog_number.clone()))
        .collect();
    for t in &b.threads {
        assert!(
            pe_threads.contains(&(t.rgb_value(), t.catalog_number.clone())),
            "{t:?}"
        );
    }
}

#[test]
fn a_changed_design_is_written_from_the_design_with_a_warning() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let file = root.join("crates/openstitch-formats/tests/fixtures/pe11/r01-rect.pes");
    let dir = TempDir::canonical("resave-changed");
    let mut s = open(&dir, &file);
    let id = s.design().objects[0].id;
    run(&mut s, "object.move", json!({"ids": [id], "dx": 1}));
    let r = run(&mut s, "design.export", json!({"path": "a.pes"}));
    assert!(r.get("source").is_none());
    assert!(
        r["warnings"].as_array().expect("warnings").iter().any(|w| w
            .as_str()
            .is_some_and(|w| w.contains("changed since it was opened"))),
        "{r}"
    );
    assert_ne!(
        std::fs::read(dir.join("a.pes")).expect("a"),
        std::fs::read(&file).expect("pe")
    );
    // Undo brings the design back to what was opened: the file again.
    run(&mut s, "history.undo", json!({}));
    let r = run(&mut s, "design.export", json!({"path": "b.pes"}));
    assert_eq!(r["source"], "unchanged");
    // A plain import (Import, not Open) keeps no file.
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "design.new",
        json!({"name": "in", "width": 100, "height": 100}),
    );
    run(&mut s, "stitches.import", json!({"path": "in.pes"}));
    let r = run(&mut s, "design.export", json!({"path": "c.pes"}));
    assert!(r.get("source").is_none());
}

#[test]
fn import_pes_reads_the_other_region_types_and_the_design_property() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = TempDir::canonical("resave-import");
    let fixture = |name: &str| {
        root.join(format!(
            "crates/openstitch-formats/tests/fixtures/pe11/{name}.pes"
        ))
    };
    for (name, kind) in [
        ("o-region-spiral", "spiral"),
        ("o-region-concentric", "concentricCircle"),
        ("o-region-radial", "radial"),
        ("o-region-stippling", "stippling"),
        ("o-region-net-fill", "netFill"),
        ("o-region-zigzag-net", "zigzagNetFill"),
        ("o-region-cross", "crossStitch"),
        ("o-region-piping", "piping"),
        ("o-region-flexible-spiral", "flexibleSpiral"),
    ] {
        std::fs::copy(fixture(name), dir.join("in.pes")).expect("copy");
        let mut s = Session::new(dir.path());
        run(
            &mut s,
            "design.new",
            json!({"name": "t", "width": 100, "height": 100}),
        );
        let r = run(&mut s, "import.pes", json!({"path": "in.pes"}));
        assert_eq!(r["skipped"], json!([]), "{name}");
        let kinds: Vec<Value> = s
            .design()
            .objects
            .iter()
            .filter_map(|o| o.region.as_ref())
            .map(|r| serde_json::to_value(r.kind).expect("kind"))
            .collect();
        assert!(kinds.contains(&json!(kind)), "{name}: {kinds:?}");
    }
    std::fs::copy(
        root.join("compat/reference/pes/design-property-native.pes"),
        dir.join("in.pes"),
    )
    .expect("copy");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "design.new",
        json!({"name": "t", "width": 100, "height": 100}),
    );
    let r = run(&mut s, "import.pes", json!({"path": "in.pes"}));
    assert_eq!(r["propertiesAdopted"], true);
    assert!(!s.design().properties.is_empty());
}
