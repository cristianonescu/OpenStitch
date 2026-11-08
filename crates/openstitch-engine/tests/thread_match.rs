//! Matching a design to a thread chart by perceived colour (#484), merging similar colours
//! (#485) and finding threads by code or name (#486).

use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn rect(s: &mut Session, x: f64, fill: &str) {
    run(
        s,
        "shape.rectangle",
        json!({"x": x, "y": 0, "width": 8, "height": 8, "fill": fill}),
    );
}

fn fills(s: &Session) -> Vec<String> {
    s.design()
        .objects
        .iter()
        .filter_map(|o| o.region.as_ref().map(|r| r.thread.hex()))
        .collect()
}

#[test]
fn rgb_stays_the_default_and_dry_run_changes_nothing() {
    let dir = TempDir::new("match");
    let mut s = Session::new(&dir);
    run(&mut s, "threads.createChart", json!({"name": "C"}));
    for c in ["#202040", "#0000c0"] {
        run(
            &mut s,
            "threads.addColor",
            json!({"chart": "C", "color": c}),
        );
    }
    // A navy where RGB and CIEDE2000 disagree.
    rect(&mut s, 0.0, "#000060");
    let before = s.design().clone();
    let plan = run(
        &mut s,
        "threads.remap",
        json!({"chart": "C", "metric": "ciede2000", "dryRun": true}),
    );
    assert_eq!(s.design(), &before, "dryRun changes nothing");
    assert_eq!(plan["applied"], json!(false));
    assert_eq!(plan["metric"], json!("ciede2000"));
    let row = &plan["plan"][0];
    assert_eq!(row["from"]["color"], json!("#000060"));
    assert_eq!(row["to"]["color"], json!("#0000c0"));
    assert_eq!(row["objects"], json!(1));
    assert!(row["deltaE"].as_f64().expect("ΔE") > 0.0);
    // The default is PE-DESIGN's RGB rule.
    let r = run(&mut s, "threads.remap", json!({"chart": "C"}));
    assert_eq!(r["metric"], json!("rgb"));
    assert_eq!(fills(&s), ["#202040"]);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &before);
    // Perceptual, applied as one undo step.
    run(
        &mut s,
        "threads.remap",
        json!({"chart": "C", "metric": "ciede2000"}),
    );
    assert_eq!(fills(&s), ["#0000c0"]);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &before);
    // The dry run added no undo step: the next undo removes the rectangle.
    run(&mut s, "history.undo", json!({}));
    assert!(s.design().objects.is_empty());
    for bad in [
        json!({"chart": "C", "metric": "lab"}),
        json!({"chart": "C", "builtin": "brother"}),
        json!({"builtin": "nope"}),
        json!({}),
    ] {
        assert!(s.execute("threads.remap", Some(&bad)).is_err(), "{bad}");
    }
}

#[test]
fn keep_distinct_separates_two_distinct_blues_and_lists_forced_merges() {
    let dir = TempDir::new("match");
    let mut s = Session::new(&dir);
    run(&mut s, "threads.createChart", json!({"name": "Blues"}));
    for c in ["#4070e0", "#a0d0ff", "#000000"] {
        run(
            &mut s,
            "threads.addColor",
            json!({"chart": "Blues", "color": c}),
        );
    }
    // Two close blues (ΔE00 < 2) and two clearly different ones.
    for (i, c) in ["#2040c0", "#2141c1", "#6090f0", "#6191f1"]
        .iter()
        .enumerate()
    {
        rect(&mut s, i as f64 * 10.0, c);
    }
    let plain = run(
        &mut s,
        "threads.remap",
        json!({"chart": "Blues", "metric": "ciede2000", "dryRun": true}),
    );
    let tos = |v: &Value| -> Vec<String> {
        v["plan"]
            .as_array()
            .expect("plan")
            .iter()
            .map(|r| r["to"]["color"].as_str().expect("hex").to_string())
            .collect()
    };
    assert_eq!(tos(&plain), ["#4070e0"; 4]);
    assert_eq!(plain["merged"].as_array().expect("merged").len(), 1);
    run(
        &mut s,
        "threads.remap",
        json!({"chart": "Blues", "metric": "ciede2000", "keepDistinct": true}),
    );
    assert_eq!(fills(&s), ["#4070e0", "#4070e0", "#a0d0ff", "#a0d0ff"]);
    // A chart of one thread forces the merge, which is listed.
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "threads.createChart", json!({"name": "One"}));
    run(
        &mut s,
        "threads.addColor",
        json!({"chart": "One", "color": "#4070e0"}),
    );
    let r = run(
        &mut s,
        "threads.remap",
        json!({"chart": "One", "metric": "ciede2000", "keepDistinct": true, "dryRun": true}),
    );
    assert_eq!(r["merged"][0]["forced"], json!(true));
}

/// Built-in charts work as targets: the Brother chart's entries 1–61 and the brand charts.
#[test]
fn built_in_charts_are_remap_targets() {
    let dir = TempDir::new("match");
    let mut s = Session::new(&dir);
    rect(&mut s, 0.0, "#ff8000");
    let r = run(
        &mut s,
        "threads.remap",
        json!({"builtin": "brother", "dryRun": true}),
    );
    // PE-DESIGN's pick for (255, 128, 0) is 11 Deep Gold, not 62 Orange (#119).
    let brother = openstitch_core::nearest_brother_thread(&openstitch_core::Thread::rgb(0xff8000));
    assert_eq!(r["plan"][0]["to"]["color"], json!(brother.hex()));
    let r = run(
        &mut s,
        "threads.remap",
        json!({"builtin": "isacord40", "metric": "ciede2000"}),
    );
    assert_eq!(r["applied"], json!(true));
    let o = &s.design().objects[0];
    let t = &o.region.as_ref().expect("fill").thread;
    assert_eq!(t.brand.as_deref(), Some("Isacord 40"));
    assert!(t.catalog_number.is_some());
}

/// #484 acceptance (a): over the 61 Brother colours matched to every brand chart, CIEDE2000
/// matching gives a lower mean ΔE00 than RGB matching (the numbers are in FEATURE_MATRIX, from
/// tools/thread_match_table.py).
#[test]
fn ciede2000_matching_lowers_mean_delta_e_over_the_brother_colours() {
    let dir = TempDir::new("match");
    let mut s = Session::new(&dir);
    for i in 1..=61 {
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": i % 10 * 10, "y": i / 10 * 10, "width": 8, "height": 8, "fill": i}),
        );
    }
    for (id, _, _) in openstitch_core::brands::brand_charts() {
        let mean = |metric: &str| {
            let mut s2 = Session::new(&dir);
            for o in &s.design().objects {
                let t = &o.region.as_ref().expect("fill").thread;
                rect(&mut s2, 0.0, &t.hex());
            }
            run(
                &mut s2,
                "threads.remap",
                json!({"builtin": id, "metric": metric, "dryRun": true}),
            )["meanDeltaE"]
                .as_f64()
                .expect("mean")
        };
        let (rgb, de) = (mean("rgb"), mean("ciede2000"));
        if id == "newBrothread" {
            // The kit holds the Brother colours themselves (#796): both find exact matches.
            assert_eq!((rgb, de), (0.0, 0.0), "{id}");
            continue;
        }
        assert!(de < rgb, "{id}: ΔE00 {de} vs RGB {rgb}");
    }
}

#[test]
fn merge_similar_joins_near_identical_blues_and_leaves_red() {
    let dir = TempDir::new("merge");
    let mut s = Session::new(&dir);
    rect(&mut s, 0.0, "#2050c0");
    rect(&mut s, 10.0, "#e01010");
    // A bigger near-identical blue: sewn most, so it is kept.
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 20, "y": 0, "width": 20, "height": 20, "fill": "#2252c2"}),
    );
    let before = s.design().clone();
    let plan = run(&mut s, "threads.mergeSimilar", json!({"dryRun": true}));
    assert_eq!(s.design(), &before);
    assert_eq!(plan["colors"], json!({"before": 3, "after": 2}));
    assert_eq!(plan["groups"][0]["keep"]["color"], json!("#2252c2"));
    let cc = &plan["colorChanges"];
    assert!(cc["after"].as_u64() <= cc["before"].as_u64());
    assert!(cc["afterSortByColor"].as_u64() <= cc["before"].as_u64());
    assert_eq!(cc["afterSortByColor"], json!(1));
    run(&mut s, "threads.mergeSimilar", json!({"maxDeltaE": 5}));
    assert_eq!(fills(&s), ["#2252c2", "#e01010", "#2252c2"]);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &before, "undo restores the exact threads");
    // Too strict a threshold merges nothing.
    let r = run(&mut s, "threads.mergeSimilar", json!({"maxDeltaE": 0.1}));
    assert_eq!(r["colors"]["after"], json!(3));
    assert_eq!(s.design(), &before);
    assert!(
        s.execute("threads.mergeSimilar", Some(&json!({"metric": "rgb"})))
            .is_err()
    );
}

#[test]
fn merged_threads_keep_the_kept_threads_code() {
    let dir = TempDir::new("merge");
    let mut s = Session::new(&dir);
    run(&mut s, "threads.createChart", json!({"name": "Mine"}));
    run(
        &mut s,
        "threads.addColor",
        json!({"chart": "Mine", "color": "#2252c2", "code": "1234", "brand": "Acme"}),
    );
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 20, "height": 20, "fill": "#2252c2"}),
    );
    run(&mut s, "threads.remap", json!({"chart": "Mine"}));
    rect(&mut s, 30.0, "#2050c0");
    run(&mut s, "threads.mergeSimilar", json!({}));
    for o in &s.design().objects {
        let t = &o.region.as_ref().expect("fill").thread;
        assert_eq!(
            (t.hex().as_str(), t.catalog_number.as_deref()),
            ("#2252c2", Some("1234"))
        );
    }
}

#[test]
fn find_searches_every_chart_by_code_or_name() {
    let dir = TempDir::new("find");
    let mut s = Session::new(&dir);
    run(&mut s, "threads.createChart", json!({"name": "Mine"}));
    run(
        &mut s,
        "threads.addColor",
        json!({"chart": "Mine", "color": "#aa0022", "code": "1800", "description": "Ruby Red"}),
    );
    let started = std::time::Instant::now();
    let r = run(&mut s, "threads.find", json!({"query": "1800"}));
    let took = started.elapsed();
    let threads = r["threads"].as_array().expect("threads");
    assert!(!threads.is_empty());
    // Exact codes first.
    assert_eq!(threads[0]["code"], json!("1800"));
    assert!(
        threads
            .iter()
            .any(|t| t["chart"] == json!("Mine") && t["builtin"] == json!(false))
    );
    assert!(took.as_millis() < 1000, "{took:?}");
    let r = run(
        &mut s,
        "threads.find",
        json!({"query": "ruby", "charts": ["Mine"]}),
    );
    assert_eq!(r["total"], json!(1));
    assert_eq!(r["threads"][0]["description"], json!("Ruby Red"));
    let r = run(&mut s, "threads.find", json!({"query": "red", "limit": 3}));
    assert_eq!(r["threads"].as_array().map(Vec::len), Some(3));
    assert!(r["total"].as_u64() > Some(3));
    for bad in [
        json!({"query": ""}),
        json!({"query": "a", "limit": 0}),
        json!({}),
    ] {
        assert!(s.execute("threads.find", Some(&bad)).is_err(), "{bad}");
    }
}

#[test]
fn builtin_charts_list_the_brand_charts() {
    let dir = TempDir::new("find");
    let mut s = Session::new(&dir);
    let r = run(&mut s, "threads.builtinCharts", json!({}));
    let charts = r.as_array().expect("charts");
    for (id, _, threads) in openstitch_core::brands::brand_charts() {
        let c = charts
            .iter()
            .find(|c| c["id"] == json!(id))
            .unwrap_or_else(|| panic!("{id} listed"));
        assert_eq!(c["kind"], json!("brand"));
        assert_eq!(c["threads"].as_array().map(Vec::len), Some(threads.len()));
    }
    assert_eq!(charts[0]["kind"], json!("machine"));
    let out = run(
        &mut s,
        "threads.exportCsv",
        json!({"builtin": "sulkyRayon40", "path": "sulky.csv"}),
    );
    assert_eq!(out["colors"], json!(335));
}

/// The editor's PEC chart picker filters with `threads.find`'s matching (#486, #682): a code or
/// name search there and `threads.find {charts: ["brother"]}` find the same threads (the picker
/// also takes a chart number).
#[test]
fn find_in_the_brother_chart_is_the_pickers_search() {
    let dir = TempDir::new("find-brother");
    let mut s = Session::new(&dir);
    let r = run(
        &mut s,
        "threads.find",
        json!({"query": "deep rose", "charts": ["brother"]}),
    );
    let threads = r["threads"].as_array().expect("threads");
    assert!(!threads.is_empty());
    for t in threads {
        assert_eq!(t["chart"], json!("brother"));
        let name = t["description"].as_str().unwrap_or("").to_lowercase();
        assert!(name.contains("deep rose"), "{t}");
        let pec = openstitch_core::PEC_CHART[t["index"].as_u64().expect("an index") as usize]
            .as_ref()
            .expect("a PEC thread");
        assert_eq!(
            openstitch_engine::thread_match::find_rank(pec, "deep rose"),
            Some(2)
        );
    }
    // The chart's threads carry their Brother Embroidery codes: a code finds that thread first.
    let code = threads[0]["code"].as_str().expect("a code").to_string();
    let r = run(
        &mut s,
        "threads.find",
        json!({"query": code, "charts": ["brother"]}),
    );
    assert_eq!(r["threads"][0]["index"], threads[0]["index"], "{r}");
}

/// #796: New Brothread is a built-in chart, listed first among the makers' charts, its Madeira
/// cross-references are returned and searchable, and `threads.find` finds its threads by code,
/// name or Madeira code.
#[test]
fn new_brothread_is_built_in_and_searchable_by_madeira_code() {
    let dir = TempDir::new("new-brothread-find");
    let mut s = Session::new(&dir);
    let charts = run(&mut s, "threads.builtinCharts", json!({}));
    let brands: Vec<&Value> = charts
        .as_array()
        .expect("charts")
        .iter()
        .filter(|c| c["kind"] == "brand")
        .collect();
    let nb = brands[0];
    assert_eq!(nb["id"], "newBrothread");
    let threads = nb["threads"].as_array().expect("threads");
    assert_eq!(threads.len(), 63);
    assert_eq!(threads[0]["code"], "001");
    assert_eq!(threads[0]["description"], "White");
    assert_eq!(
        threads[0]["crossReferences"],
        json!([{"brand": "Madeira Polyneon", "code": "1801"}, {"brand": "Madeira Rayon", "code": "1002"}])
    );
    let find = |s: &mut Session, q: &str| {
        run(
            s,
            "threads.find",
            json!({"query": q, "charts": ["newBrothread"]}),
        )["threads"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    let by_madeira = find(&mut s, "1801");
    assert_eq!(by_madeira[0]["code"], "001", "{by_madeira:?}");
    assert_eq!(find(&mut s, "1002")[0]["code"], "001", "Madeira Rayon");
    assert_eq!(find(&mut s, "901")[0]["description"], "Flesh");
    assert_eq!(find(&mut s, "ivory")[0]["code"], "902");
    // 1861 is printed for both 812 Cream Yellow and 902 Ivory.
    let both: Vec<Value> = find(&mut s, "1861")
        .iter()
        .map(|t| t["code"].clone())
        .collect();
    assert!(
        both.contains(&json!("812")) && both.contains(&json!("902")),
        "{both:?}"
    );
}

/// #796: the default chart is New Brothread for a new profile, with "My Threads" holding the
/// kit; a choice round-trips through the settings file and a saved choice is kept.
#[test]
fn default_chart_is_new_brothread_and_a_saved_choice_stays() {
    let dir = TempDir::new("default-chart");
    let file = dir.join("user.json");
    let mut s = Session::new(&dir);
    s.with_user_settings(&file).expect("first run");
    let d = run(&mut s, "threads.defaultChart", json!({}));
    assert_eq!(d["builtin"], "newBrothread");
    assert_eq!(d["chosen"], false);
    let mine = run(&mut s, "threads.charts", json!({}));
    assert_eq!(mine[0]["name"], "My Threads");
    assert_eq!(mine[0]["threads"].as_array().map(Vec::len), Some(63));
    let r = run(&mut s, "threads.setDefault", json!({"builtin": "brother"}));
    assert_eq!(r["defaultChart"]["builtin"], "brother");
    let mut t = Session::new(&dir);
    t.with_user_settings(&file).expect("saved");
    let d = run(&mut t, "threads.defaultChart", json!({}));
    assert_eq!(
        (d["builtin"].clone(), d["chosen"].clone()),
        (json!("brother"), json!(true))
    );
    run(&mut t, "threads.setDefault", json!({"chart": "My Threads"}));
    run(
        &mut t,
        "threads.renameChart",
        json!({"chart": "My Threads", "name": "Shelf"}),
    );
    assert_eq!(
        run(&mut t, "threads.defaultChart", json!({}))["chart"],
        "Shelf",
        "follows a rename"
    );
    run(&mut t, "threads.deleteChart", json!({"chart": "Shelf"}));
    assert_eq!(
        run(&mut t, "threads.defaultChart", json!({}))["builtin"],
        "newBrothread",
        "a deleted chart falls back"
    );
    for bad in [
        json!({"builtin": "nope"}),
        json!({"chart": "nope"}),
        json!({}),
    ] {
        assert!(
            t.execute("threads.setDefault", Some(&bad)).is_err(),
            "{bad}"
        );
    }
}

/// #796: a New Brothread thread is applied by chart and code, keeps its code and brand, and is
/// written to PES as the Brother/PEC thread of the same code (901 and 902 as the nearest PEC
/// colour); the file reads back with those threads.
#[test]
fn new_brothread_threads_apply_and_write_pes_as_brother_codes() {
    let dir = TempDir::new("new-brothread-pes");
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    for (x, code) in [(0.0, "800"), (20.0, "901")] {
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": x, "y": 0, "width": 10, "height": 10,
                   "fill": {"builtin": "newBrothread", "code": code}}),
        );
    }
    let region =
        |s: &Session, i: usize| s.design().objects[i].region.clone().expect("region").thread;
    let red = region(&s, 0);
    assert_eq!(
        (
            red.catalog_number.as_deref(),
            red.brand.as_deref(),
            red.name.as_deref()
        ),
        (Some("800"), Some("New Brothread"), Some("Red"))
    );
    assert_eq!(red.hex(), "#ed171f", "Brother 800's PEC colour");
    assert_eq!(region(&s, 1).hex(), "#e2bfba");
    let e = s
        .execute(
            "shape.rectangle",
            Some(&json!({"x": 40, "y": 0, "width": 5, "height": 5,
                         "fill": {"builtin": "newBrothread", "code": "999"}})),
        )
        .expect_err("no such code");
    assert!(e.to_string().contains("999"), "{e}");
    run(&mut s, "design.export", json!({"path": "nb.pes"}));
    let bytes = std::fs::read(dir.join("nb.pes")).expect("pes");
    let pattern = openstitch_formats::FileType::Pes
        .read(&bytes)
        .expect("reads back");
    // The PEC colour section: Brother 800's own entry for 800, the nearest PEC colour for 901
    // (as for any other brand's thread), as the PES writer picks them.
    let threads: Vec<openstitch_core::Thread> = pattern.threads.clone();
    let pec = openstitch_formats::pec::pec_color_indexes(&threads);
    assert_eq!(
        pec[0],
        openstitch_core::pec_index_of_brother_code("800").expect("800")
    );
    let flesh = openstitch_core::Thread::rgb(0xE2BFBA);
    let nearest = openstitch_formats::pec::pec_color_indexes(std::slice::from_ref(&flesh))[0];
    assert_eq!(pec[1], nearest);
    assert!((1..=64).contains(&nearest));
    // Re-opening keeps the threads: the PES thread table holds each thread's own colour.
    let got: Vec<u32> = pattern.threads.iter().map(|t| t.rgb_value()).collect();
    assert_eq!(got, [0xED171F, 0xE2BFBA]);
}
