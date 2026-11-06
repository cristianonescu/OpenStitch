//! Job sheets (#513): the job fields, kept in the project only, and the client and operator
//! sheets `design.print` writes. Made-up job data only.

use openstitch_core::Command;
use openstitch_design::{Compiler, JobInfo};
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// Two colours, a stop between them, and a job.
fn design(dir: &TempDir) -> Session {
    let mut s = Session::new(dir);
    run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 30, "height": 20, "fill": "Red" }),
    );
    run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 60, "y": 40, "width": 20, "height": 20, "fill": "Blue" }),
    );
    run(
        &mut s,
        "design.setJob",
        json!({
            "customer": "Example Shop", "order": "A-100", "due": "Friday",
            "notes": "Left chest.\nTwo pieces.", "fabric": "Pique knit", "stabiliser": "Cut-away",
        }),
    );
    s
}

#[test]
fn the_job_round_trips_in_the_project_and_undoes() {
    let dir = TempDir::new("job-osd");
    let mut s = design(&dir);
    let job = s.design().job.clone().expect("a job");
    assert_eq!(job.customer, "Example Shop");
    assert_eq!(job.notes, "Left chest.\nTwo pieces.");
    let path = dir.path().join("job.osd").display().to_string();
    run(&mut s, "project.save", json!({ "path": path }));
    let mut t = Session::new(&dir);
    run(&mut t, "project.open", json!({ "path": path }));
    assert_eq!(t.design().job.as_ref(), Some(&job));
    // An empty field clears it; clear empties the job, which then leaves the project.
    let r = run(&mut s, "design.setJob", json!({ "due": "" }));
    assert_eq!(r["due"], Value::Null);
    run(&mut s, "design.setJob", json!({ "clear": true }));
    assert_eq!(s.design().job, None);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(
        s.design().job.as_ref().map(|j| j.order.as_str()),
        Some("A-100")
    );
    for bad in [
        json!({ "customer": "x".repeat(201) }),
        json!({ "order": "a\u{7}b" }),
    ] {
        assert!(s.execute("design.setJob", Some(&bad)).is_err(), "{bad}");
    }
}

/// A project without a job (every project saved before #513) loads with none and saves byte for
/// byte as before: the field is left out while empty, so nothing in the file changes.
#[test]
fn projects_without_a_job_load_and_save_byte_for_byte() {
    let golden = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../openstitch-design/tests/fixtures/badge-golden.osd"
    ))
    .expect("fixture");
    let d = openstitch_engine::project::decode(&golden).expect("decodes");
    assert_eq!(d.job, None);
    let once = openstitch_engine::project::encode(&d).expect("encodes");
    assert!(!String::from_utf8_lossy(&once).contains("\"job\""));
    let again = openstitch_engine::project::decode(&once).expect("decodes");
    assert_eq!(
        openstitch_engine::project::encode(&again).expect("encodes"),
        once
    );
}

#[test]
fn the_job_never_reaches_a_machine_file() {
    let dir = TempDir::new("job-machine");
    let mut s = design(&dir);
    let with = |s: &mut Session, ext: &str| {
        let path = dir.path().join(format!("j.{ext}"));
        run(
            s,
            "design.export",
            json!({ "path": path.display().to_string() }),
        );
        std::fs::read(&path).expect("written")
    };
    let exts = ["pes", "pec", "dst", "exp", "jef", "vp3", "xxx"];
    let a: Vec<Vec<u8>> = exts.iter().map(|e| with(&mut s, e)).collect();
    run(&mut s, "design.setJob", json!({ "clear": true }));
    let b: Vec<Vec<u8>> = exts.iter().map(|e| with(&mut s, e)).collect();
    for ((e, x), y) in exts.iter().zip(&a).zip(&b) {
        assert_eq!(x, y, "{e}: the job changed the machine file");
        for word in ["Example Shop", "A-100", "Cut-away", "Pique"] {
            assert!(
                !x.windows(word.len()).any(|w| w == word.as_bytes()),
                "{e} holds {word}"
            );
        }
    }
}

/// Every text a page draws (the PDF literals of its content stream).
fn texts(pages: &[openstitch_print::Page]) -> String {
    pages
        .iter()
        .map(|p| p.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn operator_sheet_lists_the_colour_sequence_and_the_sewing_time() {
    let dir = TempDir::new("job-operator");
    let s = design(&dir);
    let d = s.design().clone();
    let job = |j: &JobInfo| openstitch_print::Job {
        customer: j.customer.clone(),
        order: j.order.clone(),
        fabric: j.fabric.clone(),
        stabiliser: j.stabiliser.clone(),
        shop: "Example Embroidery".into(),
        ..Default::default()
    };
    let job = job(d.job.as_ref().expect("job"));
    let opts = openstitch_print::Options::default();
    let pattern = Compiler::for_design(&d).compile(&d);
    let steps = openstitch_print::jobsheet::steps(&pattern);
    // The table's steps are the compiled pattern's colour blocks.
    assert_eq!(steps.len(), pattern.color_blocks().len());
    assert_eq!(
        steps.iter().map(|s| s.stitches).sum::<usize>(),
        pattern.count(Command::Stitch)
    );
    let pages =
        openstitch_print::job_sheet(&d, &job, openstitch_print::Sheet::Operator, false, &opts);
    assert_eq!(pages.len(), 1);
    let t = texts(&pages);
    for want in [
        "(Example Embroidery)",
        "(Customer: Example Shop)",
        "(Fabric: Pique knit)",
        "(Stabiliser: Cut-away)",
    ] {
        assert!(t.contains(want), "{want}");
    }
    // The sewing time is design.info's.
    let mut s = s;
    let info = run(&mut s, "design.info", json!({}));
    let minutes = info["estimates"]["sewingMinutes"]
        .as_f64()
        .expect("minutes");
    assert!(
        t.contains(&format!("Sewing time about {minutes} min")),
        "{minutes}"
    );
    // Detailed: one thumbnail per step on a further page.
    let detailed =
        openstitch_print::job_sheet(&d, &job, openstitch_print::Sheet::Operator, true, &opts);
    assert_eq!(detailed.len(), 2);
    assert!(texts(&detailed[1..]).contains("(2. "));
}

#[test]
fn design_print_writes_the_client_and_operator_sheets() {
    let dir = TempDir::new("job-print");
    let mut s = design(&dir);
    for (mode, detail) in [
        ("client", "overview"),
        ("client", "detailed"),
        ("operator", "overview"),
        ("operator", "detailed"),
    ] {
        for paper in ["a4", "letter"] {
            let path = dir.path().join(format!("{mode}-{detail}-{paper}.pdf"));
            let r = run(
                &mut s,
                "design.print",
                json!({
                    "path": path.display().to_string(), "mode": mode, "detail": detail,
                    "paper": paper, "shop": "Example Embroidery",
                }),
            );
            let pages = r["pages"].as_u64().expect("pages");
            assert!(pages >= 1, "{mode} {detail}");
            if detail == "overview" {
                assert_eq!(pages, 1, "{mode} {paper}");
            }
            assert!(std::fs::read(&path).expect("pdf").starts_with(b"%PDF"));
        }
    }
    assert!(
        s.execute(
            "design.print",
            Some(&json!({ "path": dir.path().join("x.pdf").display().to_string(), "mode": "client", "detail": "huge" }))
        )
        .is_err()
    );
}

/// A PE-DESIGN file opened and given a job is still written back byte for byte (#375): the job
/// is not part of what the export compares.
#[test]
fn a_job_on_an_opened_pe_design_file_keeps_it_unchanged() {
    let dir = TempDir::new("job-resave");
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../openstitch-formats/tests/fixtures/pe11");
    let mut files: Vec<_> = std::fs::read_dir(&fixtures)
        .expect("fixtures")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "pes"))
        .filter(|p| std::fs::read(p).is_ok_and(|b| b.starts_with(b"#PES0110")))
        .collect();
    files.sort();
    let file = files.first().expect("a PE-DESIGN 11 fixture");
    std::fs::copy(file, dir.path().join("in.pes")).expect("copy");
    let mut s = Session::new(&dir);
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
    run(
        &mut s,
        "design.setJob",
        json!({ "customer": "Example Shop" }),
    );
    let r = run(&mut s, "design.export", json!({ "path": "out.pes" }));
    assert_eq!(r["source"], "unchanged", "{r}");
    assert_eq!(
        std::fs::read(dir.path().join("out.pes")).expect("out"),
        std::fs::read(file).expect("in")
    );
}
