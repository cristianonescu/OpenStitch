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
    // Written in the opened file's own version, not downgraded to version 6 (#120), unless
    // another version is asked for.
    assert!(
        std::fs::read(dir.join("a.pes"))
            .expect("a")
            .starts_with(b"#PES0110")
    );
    run(
        &mut s,
        "design.export",
        json!({"path": "v6.pes", "pesVersion": "6"}),
    );
    assert!(
        std::fs::read(dir.join("v6.pes"))
            .expect("v6")
            .starts_with(b"#PES0060")
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

/// #982: an E or V line's spacing goes out to PES 0110 (+88, a V's opening +92) and comes
/// back. A V and an E line at 3.0 mm (not PE-DESIGN's default 5.0), set with `object.setLine`,
/// exported with `design.export` `pesVersion` 11 and read with `import.pes` in a new design:
/// each reads back its type and spacing 3.0 and sews the same number of stitches.
#[test]
fn e_and_v_spacing_survives_a_pes_0110_round_trip() {
    let dir = TempDir::canonical("resave-ev-spacing");
    for kind in ["vStitch", "eStitch"] {
        let mut s = Session::new(dir.path());
        run(
            &mut s,
            "design.new",
            json!({"name": "t", "width": 100, "height": 100}),
        );
        let r = run(
            &mut s,
            "shape.path",
            json!({"points": [[10, 20], [60, 20], [60, 50]], "closed": false}),
        );
        let id = r["id"].as_u64().expect("id");
        run(
            &mut s,
            "object.setLine",
            json!({"ids": [id], "type": kind, "width": 2, "spacing": 3}),
        );
        let stitches =
            |s: &mut Session| run(s, "design.info", json!({}))["stitchStats"]["stitches"].clone();
        let before = stitches(&mut s);
        assert!(before.as_u64().is_some_and(|n| n > 20), "{kind}: {before}");
        run(
            &mut s,
            "design.export",
            json!({"path": "ev.pes", "pesVersion": "11"}),
        );
        let mut back = Session::new(dir.path());
        run(
            &mut back,
            "design.new",
            json!({"name": "t", "width": 100, "height": 100}),
        );
        let r = run(&mut back, "import.pes", json!({"path": "ev.pes"}));
        assert_eq!(r["skipped"], json!([]), "{kind}");
        let lines: Vec<Value> = back
            .design()
            .objects
            .iter()
            .filter_map(|o| o.line.as_ref())
            .map(|l| serde_json::to_value(l).expect("line"))
            .collect();
        assert_eq!(lines.len(), 1, "{kind}: {lines:?}");
        assert_eq!(lines[0]["type"], json!(kind), "{kind}");
        assert_eq!(lines[0]["spacing"], json!(3.0), "{kind}: {}", lines[0]);
        assert_eq!(stitches(&mut back), before, "{kind}: the same stitches");
    }
}

/// PE-DESIGN 11.50's cross stitch Times (#614, round 8: a 20 mm square at Single, Double,
/// Triple; 410, 666 and 922 stitches): the import reads Times, the region sewn again adds the
/// same 256 stitches per step as PE-DESIGN's files, and a PES 0110 export writes Times and
/// Size where PE-DESIGN does.
#[test]
fn cross_stitch_times_import_sew_and_export_as_pe_design() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = TempDir::canonical("resave-cross-times");
    let mut counts = Vec::new();
    for (name, times, passes) in [("single", 1, 2), ("double", 2, 4), ("triple", 3, 6)] {
        let reference = root.join(format!("compat/reference/stitches/cross-{name}.pes"));
        let pe = pes::read(&std::fs::read(&reference).expect("reference"))
            .expect("PES")
            .count(openstitch_core::Command::Stitch);
        std::fs::copy(&reference, dir.join("in.pes")).expect("copy");
        let mut s = Session::new(dir.path());
        run(
            &mut s,
            "design.new",
            json!({"name": "t", "width": 100, "height": 100}),
        );
        run(&mut s, "import.pes", json!({"path": "in.pes"}));
        let r = s
            .design()
            .objects
            .iter()
            .find_map(|o| o.region.clone())
            .expect("region");
        assert_eq!(r.cross_times().count(), times, "{name}");
        assert_eq!(r.spacing_mm(), 2.5, "{name}");
        let ours = openstitch_design::Compiler::default()
            .compile(s.design())
            .count(openstitch_core::Command::Stitch);
        counts.push((pe as i64, ours as i64));
        run(
            &mut s,
            "design.export",
            json!({"path": "out.pes", "pesVersion": "11"}),
        );
        let section =
            pes::read_objects(&std::fs::read(dir.join("out.pes")).expect("read")).expect("objects");
        let shape = &section.shapes[0];
        assert_eq!(
            (shape.cross_passes, shape.cross_size),
            (passes, 2.5),
            "{name}"
        );
    }
    for w in counts.windows(2) {
        assert_eq!(w[1].0 - w[0].0, 256, "PE-DESIGN: {counts:?}");
        assert_eq!(w[1].1 - w[0].1, 256, "OpenStitch: {counts:?}");
    }
    // OpenStitch's route between the rows differs from PE-DESIGN's (#463), by a few stitches.
    for (pe, ours) in &counts {
        assert!((ours - pe).abs() * 100 <= pe * 5, "{counts:?}");
    }
}

/// PE-DESIGN 11.50's stamps converted from an outline (#959, round-10 captures for #471: the
/// 40 mm square with a 14.5 x 14.0 mm circle made an Engraved Line, an Embossed Region or an
/// Engraved Region): the import gives the square's fill the stamp with its effect and the
/// circle's outline where PE-DESIGN placed it, and the fill sewn again comes within 5 % of
/// PE-DESIGN's stitch count.
#[test]
fn import_pes_reads_a_shape_that_carries_a_stamp() {
    use openstitch_design::stamp::StampEffect;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = TempDir::canonical("resave-stamps");
    for (name, effect) in [
        ("engraved-line", StampEffect::EngravedLine),
        ("embossed-region", StampEffect::EmbossedRegion),
        ("engraved-region", StampEffect::EngravedRegion),
    ] {
        let reference = root.join(format!("compat/reference/stitches/stamp-{name}-circle.pes"));
        let pe = pes::read(&std::fs::read(&reference).expect("reference"))
            .expect("PES")
            .count(openstitch_core::Command::Stitch);
        std::fs::copy(&reference, dir.join("in.pes")).expect("copy");
        let mut s = Session::new(dir.path());
        run(
            &mut s,
            "design.new",
            json!({"name": "t", "width": 100, "height": 100}),
        );
        let r = run(&mut s, "import.pes", json!({"path": "in.pes"}));
        assert_eq!(r["skipped"], json!([]), "{name}");
        let region = s
            .design()
            .objects
            .iter()
            .find_map(|o| o.region.clone())
            .expect("region");
        let [stamp] = region.stamps.as_slice() else {
            panic!("{name}: {} stamps", region.stamps.len());
        };
        assert_eq!((stamp.effect, stamp.contours.len()), (effect, 1), "{name}");
        // The circle's outline: 14.5 x 14.0 mm around (50.2, 50.5) mm.
        let b = stamp.bounds().expect("bounds");
        for (got, want) in [
            (b.min_x, 429.5),
            (b.max_x, 574.5),
            (b.min_y, 435.0),
            (b.max_y, 575.0),
        ] {
            assert!((got - want).abs() < 0.5, "{name}: {b:?}");
        }
        let ours = openstitch_design::Compiler::default()
            .compile(s.design())
            .count(openstitch_core::Command::Stitch);
        assert!(
            (ours as i64 - pe as i64).abs() * 100 <= pe as i64 * 5,
            "{name}: {ours} against PE-DESIGN's {pe}"
        );
    }
}

/// A region's stamp exported as PES 0110 the way PE-DESIGN's Convert from Outline writes it
/// (#608): the stamp record after the geometry marks and its outline as the header's listed
/// pattern, with the same layout as PE-DESIGN's three round-10 captures (`stamp-*-circle`),
/// and read back as the same stamp. Before, the exporter dropped stamps without a word.
#[test]
fn stamps_export_to_pes_0110_as_pe_design_writes_them() {
    use openstitch_design::stamp::StampEffect;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = TempDir::canonical("export-stamps");
    for (name, effect, id) in [
        ("engraved-line", StampEffect::EngravedLine, "engravedLine"),
        (
            "embossed-region",
            StampEffect::EmbossedRegion,
            "embossedRegion",
        ),
        (
            "engraved-region",
            StampEffect::EngravedRegion,
            "engravedRegion",
        ),
    ] {
        let mut s = Session::new(dir.path());
        run(
            &mut s,
            "design.new",
            json!({"name": "t", "width": 100, "height": 100}),
        );
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": 30, "y": 30, "width": 40, "height": 40}),
        );
        run(
            &mut s,
            "shape.ellipse",
            json!({"cx": 50, "cy": 50, "rx": 7.25, "ry": 7.0}),
        );
        run(
            &mut s,
            "shape.emboss",
            json!({"id": 1, "tool": 2, "effect": id}),
        );
        run(
            &mut s,
            "design.export",
            json!({"path": "out.pes", "pesVersion": "11"}),
        );
        let ours =
            pes::read_objects(&std::fs::read(dir.join("out.pes")).expect("read")).expect("objects");
        let pe = pes::read_objects(
            &std::fs::read(root.join(format!("compat/reference/stitches/stamp-{name}-circle.pes")))
                .expect("reference"),
        )
        .expect("objects");
        let (a, b) = (&ours.shapes[0], &pe.shapes[0]);
        assert_eq!(a.stamps.len(), 1, "{name}");
        // Our circle is centred at (50, 50) mm, PE-DESIGN's was drawn at (50.2, 50.5).
        assert_eq!(a.stamps[0].centre, [1000, 1000], "{name}");
        assert_eq!(
            (a.stamps[0].size, a.stamps[0].rest),
            (b.stamps[0].size, b.stamps[0].rest),
            "{name}"
        );
        let ([p], [q]) = (ours.fill_patterns.as_slice(), pe.fill_patterns.as_slice()) else {
            panic!("{name}: {} patterns listed", ours.fill_patterns.len());
        };
        assert_eq!(
            (p.size, p.polylines.len(), &p.heads),
            (q.size, q.polylines.len(), &q.heads),
            "{name}"
        );
        // Closed by its first point twice, and filling the cell, as PE-DESIGN's.
        let line = &p.polylines[0];
        assert_eq!(line[line.len() - 1], line[0], "{name}");
        assert_eq!(line[line.len() - 2], line[0], "{name}");
        let xs = line.iter().map(|q| q.0);
        assert_eq!(
            (xs.clone().min(), xs.max()),
            (Some(0), Some(1024)),
            "{name}"
        );
        // Imported again: the same stamp.
        let mut t = Session::new(dir.path());
        run(
            &mut t,
            "design.new",
            json!({"name": "t", "width": 100, "height": 100}),
        );
        let r = run(&mut t, "import.pes", json!({"path": "out.pes"}));
        assert_eq!(r["notes"], json!([]), "{name}");
        let region = t
            .design()
            .objects
            .iter()
            .find_map(|o| o.region.clone())
            .expect("region");
        assert_eq!(region.stamps.len(), 1, "{name}");
        assert_eq!(region.stamps[0].effect, effect, "{name}");
        let bounds = region.stamps[0].bounds().expect("bounds");
        for (got, want) in [
            (bounds.min_x, 427.5),
            (bounds.max_x, 572.5),
            (bounds.min_y, 430.0),
            (bounds.max_y, 570.0),
        ] {
            assert!((got - want).abs() < 1.0, "{name}: {bounds:?}");
        }
    }
}

#[test]
fn import_pes_takes_the_objects_of_every_pe_design_file() {
    // #397: every PE-DESIGN `#PES0110` file imports as editable objects: shapes, texts and
    // stitch groups (315 of them were refused for having no shapes). What is still skipped is
    // listed with its reason.
    let dir = TempDir::canonical("import-all");
    let mut skipped = std::collections::BTreeMap::<String, usize>::new();
    let mut files = 0;
    for file in pe_design_files() {
        std::fs::copy(&file, dir.join("in.pes")).expect("copy");
        let mut s = Session::new(dir.path());
        run(
            &mut s,
            "design.new",
            json!({"name": "t", "width": 100, "height": 100}),
        );
        let r = s
            .execute("import.pes", Some(&json!({"path": "in.pes"})))
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        files += 1;
        assert_eq!(r["unparsed"], Value::Null, "{}", file.display());
        for k in r["skipped"].as_array().expect("skipped") {
            let why = k["reason"].as_str().unwrap_or_default().to_string();
            println!("{}: {why}", file.display());
            *skipped.entry(why).or_default() += 1;
        }
    }
    println!("{files} files, skipped {skipped:?}");
    assert!(files > 400, "{files}");
    assert_eq!(skipped, std::collections::BTreeMap::new());
}

#[test]
fn opening_a_pe_design_11_file_takes_its_page_mode() {
    // PE-DESIGN's page mode (#1013): sweep-fill-density-2p5 was saved on a 100 x 100 Custom
    // Size page, stroke-satin-K-8 on its Hoop Size page. Opened (File > Open, or its objects
    // imported into an empty design) into a design started on the other one, the design takes
    // the file's page mode, reports it, and writes it back to a PES 0110 written from the design.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (file, mode) in [
        (
            "compat/reference/stitches/sweep-fill-density-2p5.pes",
            "custom",
        ),
        ("compat/reference/text/stroke-satin-K-8.pes", "hoop"),
    ] {
        let other = if mode == "hoop" { "custom" } else { "hoop" };
        let dir = TempDir::canonical("page-mode");
        std::fs::copy(root.join(file), dir.join("in.pes")).expect("copy");
        for (command, params) in [
            (
                "stitches.import",
                json!({"path": "in.pes", "keepSource": true}),
            ),
            ("import.pes", json!({"path": "in.pes"})),
        ] {
            let mut s = Session::new(dir.path());
            run(
                &mut s,
                "design.new",
                json!({"width": 100, "height": 100, "pageMode": other}),
            );
            let r = run(&mut s, command, params);
            assert_eq!(r["pageMode"], mode, "{file} {command}");
            assert_eq!(run(&mut s, "design.info", json!({}))["pageMode"], mode);
            run(
                &mut s,
                "design.export",
                json!({"path": "out.pes", "pesVersion": "11"}),
            );
            let back =
                pes::read_objects(&std::fs::read(dir.join("out.pes")).expect("out")).expect("read");
            assert_eq!(back.hoop_page, mode == "hoop", "{file} {command}");
        }
    }
}
