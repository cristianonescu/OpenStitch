//! GIMP / Inkscape `.gpl` palettes as user thread charts (#483).

use openstitch_engine::{Capabilities, Session};
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A palette in the layout GIMP and Inkscape write (the bundled palettes themselves are GPL and
/// are not copied here): header, Name, Columns, comments, tab before the name.
const SPOOLS: &str = "GIMP Palette\nName: My spools\nColumns: 3\n#\n# owned on 2026-11-05\n255   0   0\t1800 Ruby\n  0  80 160\tNavy\n250 250 250\t1001\n";

#[test]
fn import_remap_export_and_import_back() {
    let dir = TempDir::new("palette");
    let mut s = Session::new(&dir);
    std::fs::write(dir.join("spools.gpl"), SPOOLS).expect("write");
    let r = run(
        &mut s,
        "threads.importPalette",
        json!({"path": "spools.gpl"}),
    );
    assert_eq!(r["chart"], json!("My spools"));
    assert_eq!(r["added"], json!(3));
    let chart = &s.user_settings().thread_charts[0];
    assert_eq!(chart.name, "My spools");
    let t = &chart.threads[0];
    assert_eq!(
        (
            t.hex().as_str(),
            t.catalog_number.as_deref(),
            t.name.as_deref()
        ),
        ("#ff0000", Some("1800"), Some("Ruby"))
    );
    assert_eq!(chart.threads[1].catalog_number, None);
    assert_eq!(chart.threads[2].catalog_number.as_deref(), Some("1001"));
    // The imported chart drives threads.remap.
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 10, "height": 10, "fill": "#e01020"}),
    );
    run(&mut s, "threads.remap", json!({"chart": "My spools"}));
    let fill = &s.design().objects[0].region.as_ref().expect("fill").thread;
    assert_eq!(fill.catalog_number.as_deref(), Some("1800"));
    // Export, import under another name: identical colours, codes, names and order.
    run(
        &mut s,
        "threads.exportPalette",
        json!({"chart": "My spools", "path": "out.gpl"}),
    );
    let r = run(
        &mut s,
        "threads.importPalette",
        json!({"path": "out.gpl", "name": "Again"}),
    );
    assert_eq!(r["added"], json!(3));
    let charts = &s.user_settings().thread_charts;
    assert_eq!(charts[0].threads, charts[1].threads);
    // A built-in chart exports too.
    let r = run(
        &mut s,
        "threads.exportPalette",
        json!({"builtin": "janomeJef", "path": "jef.gpl"}),
    );
    assert!(r["colors"].as_u64() > Some(70));
    let text = std::fs::read_to_string(dir.join("jef.gpl")).expect("read");
    assert!(text.starts_with("GIMP Palette\nName: Janome (JEF chart)\n"));
}

#[test]
fn name_clashes_bad_lines_and_partial_imports() {
    let dir = TempDir::new("palette");
    let mut s = Session::new(&dir);
    std::fs::write(dir.join("spools.gpl"), SPOOLS).expect("write");
    run(
        &mut s,
        "threads.importPalette",
        json!({"path": "spools.gpl"}),
    );
    // The same name again is refused, never replaced.
    let e = s
        .execute(
            "threads.importPalette",
            Some(&json!({"path": "spools.gpl"})),
        )
        .expect_err("clash");
    assert!(e.to_string().contains("exists"), "{e}");
    assert_eq!(s.user_settings().thread_charts.len(), 1);
    // Bad lines: reported with their numbers, nothing imported unless partial.
    std::fs::write(
        dir.join("bad.gpl"),
        "GIMP Palette\n1 2 3 ok\n300 0 0 no\n4 5 6\n",
    )
    .expect("write");
    let e = s
        .execute("threads.importPalette", Some(&json!({"path": "bad.gpl"})))
        .expect_err("bad line");
    assert!(e.to_string().contains("line 3"), "{e}");
    assert_eq!(s.user_settings().thread_charts.len(), 1);
    let r = run(
        &mut s,
        "threads.importPalette",
        json!({"path": "bad.gpl", "partial": true}),
    );
    // No Name line: the chart is named after the file.
    assert_eq!(r["chart"], json!("bad"));
    assert_eq!(r["added"], json!(2));
    assert_eq!(r["skipped"][0]["line"], json!(3));
    std::fs::write(dir.join("x.gpl"), "JASC-PAL\n0100\n1\n1 2 3\n").expect("write");
    for (cmd, params) in [
        ("threads.importPalette", json!({"path": "x.gpl"})),
        ("threads.importPalette", json!({"path": "missing.gpl"})),
        (
            "threads.exportPalette",
            json!({"chart": "nope", "path": "o.gpl"}),
        ),
        (
            "threads.exportPalette",
            json!({"builtin": "nope", "path": "o.gpl"}),
        ),
        ("threads.exportPalette", json!({"path": "o.gpl"})),
    ] {
        assert!(s.execute(cmd, Some(&params)).is_err(), "{cmd} {params}");
    }
}

#[test]
fn palette_import_needs_file_and_settings_rights() {
    let dir = TempDir::new("palette");
    std::fs::write(dir.join("spools.gpl"), SPOOLS).expect("write");
    let mut s = Session::new(&dir);
    s.set_capabilities(Capabilities::FULL.without_files());
    assert!(
        s.execute(
            "threads.importPalette",
            Some(&json!({"path": "spools.gpl"}))
        )
        .is_err()
    );
    let mut s = Session::new(&dir);
    s.set_capabilities(Capabilities::FULL.read_only());
    assert!(
        s.execute(
            "threads.importPalette",
            Some(&json!({"path": "spools.gpl"}))
        )
        .is_err()
    );
    assert!(s.user_settings().thread_charts.is_empty());
}
