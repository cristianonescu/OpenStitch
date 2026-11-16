//! Colours are named by thread code, never by hex value (#1184): every reply a person or the
//! Assistant reads names a colour by brand, code and name, a custom colour by the nearest thread
//! of the default chart, and a colour parameter takes a thread code.

use openstitch_core::{PEC_CHART, Thread};
use openstitch_design::Compiler;
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn refused(s: &mut Session, id: &str, params: Value) -> String {
    match s.execute(id, Some(&params)) {
        Ok(v) => panic!("{id} took {params}: {v}"),
        Err(e) => e.to_string(),
    }
}

/// A `#rrggbb` in `text`.
fn hex_in(text: &str) -> Option<&str> {
    text.char_indices()
        .filter(|&(_, c)| c == '#')
        .map(|(i, _)| &text[i..text.len().min(i + 7)])
        .find(|h| h.len() == 7 && h[1..].chars().all(|c| c.is_ascii_hexdigit()))
}

/// Every `text` and `label` field anywhere in `v`.
fn names(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if (k == "text" || k == "label" || k == "thread") && x.is_string() {
                    out.push(x.as_str().unwrap_or_default().to_string());
                }
                names(x, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| names(x, out)),
        _ => {}
    }
}

/// A design with a chart thread (Brother's Red) and a custom colour a little off New
/// Brothread's 205 Yellow, the default chart's.
fn design(dir: &TempDir) -> Session {
    let mut s = Session::new(dir);
    run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 20, "height": 12, "fill": "Red" }),
    );
    run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 40, "y": 10, "width": 20, "height": 12, "fill": "#fffd03" }),
    );
    s
}

const NEAREST: &str = "Custom colour, nearest New Brothread 205 Yellow";

#[test]
fn what_the_assistant_reads_names_every_colour_by_thread() {
    let dir = TempDir::new("thread-names");
    let mut s = design(&dir);
    let list = run(&mut s, "object.list", json!({}));
    let red = &list[0]["regionThread"];
    let custom = &list[1]["regionThread"];
    // The hex stays for scripts; the thread is beside it.
    assert_eq!(list[1]["regionColor"], "#fffd03");
    assert_eq!(red["code"], "800");
    assert!(red["text"].as_str().is_some_and(|t| t.contains("800")), "{red}");
    assert_eq!(custom["code"], Value::Null);
    assert_eq!(custom["text"], NEAREST);
    assert_eq!(custom["label"], "≈205");
    assert_eq!(custom["nearest"]["code"], "205");
    assert!(custom["nearest"]["deltaE"].as_f64().is_some_and(|d| d > 0.0 && d < 2.0));

    let info = run(&mut s, "design.info", json!({}));
    let timeline = run(&mut s, "design.timeline", json!({}));
    let route = run(&mut s, "design.route", json!({}));
    let blocks = timeline["blocks"].as_array().expect("blocks");
    // The block's code is the one on the spool; `number` stays the stored catalogue number.
    assert_eq!(blocks[0]["code"], "800");
    assert_eq!(blocks[1]["text"], NEAREST);
    assert!(
        route["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .any(|st| st["thread"] == NEAREST),
        "{route}"
    );
    let mut all = Vec::new();
    for v in [&list, &info, &timeline, &route] {
        names(v, &mut all);
    }
    assert!(all.iter().any(|t| t == NEAREST), "{all:?}");
    for t in &all {
        assert_eq!(hex_in(t), None, "a colour named by hex: {t}");
    }
}

#[test]
fn a_job_sheet_names_a_custom_colour_by_its_nearest_spool() {
    let dir = TempDir::new("thread-names-sheet");
    let s = design(&dir);
    let d = s.design().clone();
    let opts = openstitch_print::Options {
        reference: openstitch_core::brands::NEW_BROTHREAD.to_vec(),
        ..Default::default()
    };
    let job = openstitch_print::Job::default();
    for (sheet, detailed) in [
        (openstitch_print::Sheet::Client, false),
        (openstitch_print::Sheet::Operator, true),
    ] {
        let pages = openstitch_print::job_sheet(&d, &job, sheet, detailed, &opts);
        let text: String = pages.iter().map(|p| p.content.as_str()).collect();
        assert!(text.contains(NEAREST), "{sheet:?}: no '{NEAREST}'");
        assert_eq!(hex_in(&text), None, "{sheet:?} prints a hex colour");
    }
}

#[test]
fn a_design_files_colours_are_named_in_the_database_and_the_catalogue() {
    let dir = TempDir::new("thread-names-library");
    let mut s = design(&dir);
    // A DST carries no thread data: every colour in it is a custom colour.
    run(&mut s, "design.export", json!({ "path": "plain.dst" }));
    let info = run(&mut s, "library.info", json!({ "path": "plain.dst" }));
    let mut all = Vec::new();
    names(&info, &mut all);
    assert!(!all.is_empty(), "{info}");
    for t in &all {
        assert_eq!(hex_in(t), None, "library.info names a colour by hex: {t}");
    }
    run(
        &mut s,
        "library.catalog",
        json!({ "paths": ["plain.dst"], "output": "plain.csv" }),
    );
    let csv = std::fs::read_to_string(dir.path().join("plain.csv")).expect("the catalogue");
    assert!(csv.contains("ustom colour, nearest"), "{csv}");
    assert_eq!(hex_in(&csv), None, "the catalogue lists a hex colour: {csv}");
}

#[test]
fn a_colour_parameter_takes_a_thread_code() {
    let dir = TempDir::new("thread-codes");
    let mut s = Session::new(&dir);
    let fill = |s: &mut Session, fill: Value| -> Thread {
        run(
            s,
            "shape.rectangle",
            json!({ "x": 0, "y": 0, "width": 8, "height": 8, "fill": fill }),
        );
        let d = s.design();
        d.objects.last().and_then(|o| o.region.as_ref()).expect("a fill").thread.clone()
    };
    // A bare code: the default chart's (New Brothread) when it has one.
    let t = fill(&mut s, json!("205"));
    assert_eq!(t.catalog_number.as_deref(), Some("205"));
    assert_eq!(t.brand.as_deref(), Some("New Brothread"));
    // Brand and code: that brand's thread (Brother's chart writes BROTHER EMBROIDERY codes).
    let t = fill(&mut s, json!("Brother 205"));
    assert_eq!(openstitch_core::thread_code(&t).as_deref(), Some("205"));
    assert_eq!(openstitch_core::thread_brand(&t), Some("Brother"));
    // The forms it took before still work: a Brother name, a hex value, a PEC chart number.
    let t = fill(&mut s, json!("Red"));
    assert_eq!(openstitch_core::thread_code(&t).as_deref(), Some("800"));
    assert_eq!(fill(&mut s, json!("#123456")).rgb_value(), 0x123456);
    // A JSON number is a PEC chart number, not a code: 7 is entry 7.
    let seven = PEC_CHART[7].clone().expect("entry 7");
    assert_eq!(fill(&mut s, json!(7)).rgb_value(), seven.rgb_value());
    // Two threads with the code in the same chart: refused, naming both.
    run(&mut s, "threads.createChart", json!({ "name": "Mine" }));
    for (c, d) in [("#111111", "Night"), ("#222222", "Coal")] {
        run(
            &mut s,
            "threads.addColor",
            json!({ "chart": "Mine", "color": c, "code": "500", "brand": "Mine", "description": d }),
        );
    }
    run(&mut s, "threads.setDefault", json!({ "chart": "Mine" }));
    let e = refused(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 8, "height": 8, "fill": "500" }),
    );
    assert!(e.contains("ambiguous") && e.contains("Mine 500 Night") && e.contains("Mine 500 Coal"), "{e}");
    // A code or name no chart has: refused with the nearest, never guessed.
    let e = refused(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 8, "height": 8, "fill": "Yello" }),
    );
    assert!(e.contains("no thread has") && e.contains("nearest") && e.contains("Yellow"), "{e}");
    let e = refused(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 8, "height": 8, "fill": "qq-0" }),
    );
    assert!(e.contains("no thread has"), "{e}");
}

/// The compiled design's threads keep their stored values: naming changes nothing sewn.
#[test]
fn naming_a_colour_changes_nothing_stored() {
    let dir = TempDir::new("thread-names-stored");
    let mut s = design(&dir);
    let d = s.design().clone();
    let before = Compiler::for_design(&d).compile(&d).threads;
    run(&mut s, "object.list", json!({}));
    let d = s.design().clone();
    assert_eq!(Compiler::for_design(&d).compile(&d).threads, before);
    assert_eq!(before[1].hex(), "#fffd03");
}

#[test]
fn the_colour_match_plan_names_merged_design_colours_by_thread() {
    // Two clearly different design colours that one chart thread must take: the plan's
    // `merged` entry names them by their swatch captions as well as by hex (#1184).
    let dir = TempDir::new("thread-names-merge");
    let mut s = Session::new(&dir);
    for (x, fill) in [(0, "#fffd03"), (20, "#1030c0")] {
        run(
            &mut s,
            "shape.rectangle",
            json!({ "x": x, "y": 0, "width": 8, "height": 8, "fill": fill }),
        );
    }
    run(&mut s, "threads.createChart", json!({ "name": "One" }));
    run(
        &mut s,
        "threads.addColor",
        json!({ "chart": "One", "color": "#808080", "code": "1", "brand": "Grey", "description": "Grey" }),
    );
    let plan = run(
        &mut s,
        "threads.remap",
        json!({ "chart": "One", "dryRun": true }),
    );
    let merged = plan["merged"].as_array().expect("merged");
    assert_eq!(merged.len(), 1, "{plan}");
    let labels: Vec<&str> = merged[0]["fromThreads"]
        .as_array()
        .expect("fromThreads")
        .iter()
        .filter_map(|t| t["label"].as_str())
        .collect();
    assert!(labels.contains(&"≈205"), "{labels:?}");
    for l in &labels {
        assert!(l.starts_with('≈') && hex_in(l).is_none(), "{l}");
    }
}
