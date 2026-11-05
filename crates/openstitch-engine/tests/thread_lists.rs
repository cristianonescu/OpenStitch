//! Thread list files (#482): the design's colours in sewing order as CSV and list files, a list
//! applied to colour blocks, user charts from lists, and the INF beside an EXP.

#![cfg(feature = "formats")]

use openstitch_core::{Command, Thread};
use openstitch_engine::Session;
use openstitch_formats::threadlist::ListType;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

const COLOURS: [&str; 5] = ["#ff0000", "#00a000", "#0000ff", "#ffa000", "#800080"];

/// Five filled squares, one colour each, sewn left to right.
fn five_colours(tag: &str) -> (Session, TempDir) {
    let dir = TempDir::new(&format!("thread-lists-{tag}"));
    let mut s = Session::new(&dir);
    for (i, c) in COLOURS.iter().enumerate() {
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": 10 + 25 * i, "y": 10, "width": 20, "height": 20, "fill": c}),
        );
    }
    (s, dir)
}

fn colours(s: &mut Session) -> Vec<String> {
    let info = run(s, "design.info", json!({}));
    info["stitchStats"]["colors"]
        .as_array()
        .expect("colors")
        .iter()
        .map(|c| c.as_str().expect("hex").to_owned())
        .collect()
}

#[test]
fn design_csv_lists_every_block_in_sewing_order_with_totals_matching_design_info() {
    let (mut s, dir) = five_colours("csv");
    run(&mut s, "design.exportThreads", json!({"path": "t.csv"}));
    let csv = std::fs::read_to_string(dir.join("t.csv")).expect("csv");
    let rows: Vec<Vec<&str>> = csv.lines().map(|l| l.split(',').collect()).collect();
    assert_eq!(
        rows[0].join(","),
        openstitch_engine::threadlists::CSV_HEADER
    );
    assert_eq!(rows.len(), 1 + 5 + 1);
    for (i, c) in COLOURS.iter().enumerate() {
        assert_eq!(rows[1 + i][0], (i + 1).to_string());
        assert_eq!(rows[1 + i][4], *c);
    }
    let info = run(&mut s, "design.info", json!({}));
    let total = &rows[6];
    assert_eq!(total[0], "total");
    assert_eq!(
        total[8].parse::<u64>().expect("stitches"),
        info["stitchStats"]["stitches"].as_u64().expect("stitches")
    );
    let per_colour: f64 = info["estimates"]["threadPerColorM"]
        .as_array()
        .expect("per colour")
        .iter()
        .filter_map(Value::as_f64)
        .sum();
    let written: f64 = total[9].parse().expect("metres");
    assert!(
        (written - per_colour).abs() < 0.01,
        "{written} vs {per_colour}"
    );
    let sum: u64 = rows[1..6]
        .iter()
        .map(|r| r[8].parse::<u64>().expect("n"))
        .sum();
    assert_eq!(sum, total[8].parse::<u64>().expect("total"));
}

#[test]
fn design_lists_write_the_block_colours_in_order() {
    let (mut s, dir) = five_colours("lists");
    for ty in ListType::ALL {
        let name = format!("t.{}", ty.extension());
        let out = run(&mut s, "design.exportThreads", json!({"path": name}));
        assert_eq!(out["colors"], json!(5));
        let back = ty
            .read(&std::fs::read(dir.join(&name)).expect("file"))
            .expect("read");
        let hexes: Vec<String> = back.iter().map(Thread::hex).collect();
        assert_eq!(hexes, COLOURS, "{ty:?}");
    }
}

#[test]
fn a_five_colour_inf_recolours_a_five_block_dst_in_one_undo_step() {
    let (mut s, dir) = five_colours("apply");
    run(&mut s, "design.export", json!({"path": "a.dst"}));
    // DST holds no colours: opened again, the blocks get default colours.
    let mut t = Session::new(&dir);
    run(&mut t, "stitches.import", json!({"path": "a.dst"}));
    assert_eq!(colours(&mut t).len(), 5);
    let before = colours(&mut t);
    let list: Vec<Thread> = COLOURS
        .iter()
        .map(|c| Thread::from_hex(c).expect("hex"))
        .collect();
    std::fs::write(
        dir.join("five.inf"),
        ListType::Inf.write(&list).expect("inf"),
    )
    .expect("w");
    let out = run(&mut t, "design.applyThreads", json!({"path": "five.inf"}));
    assert_eq!(out["colors"], json!(5));
    assert_eq!(colours(&mut t), COLOURS);
    run(&mut t, "history.undo", json!({}));
    assert_eq!(colours(&mut t), before);

    // A list of another length is refused and changes nothing.
    std::fs::write(
        dir.join("four.edr"),
        ListType::Edr.write(&list[..4]).expect("edr"),
    )
    .expect("w");
    let err = t
        .execute("design.applyThreads", Some(&json!({"path": "four.edr"})))
        .expect_err("mismatch");
    assert!(err.to_string().contains("4 colours"), "{err}");
    assert!(err.to_string().contains("5 colour blocks"), "{err}");
    assert_eq!(colours(&mut t), before);
}

#[test]
fn applying_a_design_csv_recolours_shapes_and_keeps_names() {
    let (mut s, dir) = five_colours("apply-csv");
    let mut csv = String::from("block,brand,code,name,hex,r,g,b,stitches,threadM\n");
    for (i, c) in ["#101010", "#202020", "#303030", "#404040", "#505050"]
        .iter()
        .enumerate()
    {
        csv.push_str(&format!("{},Acme,{i}0,Grey {i},{c},0,0,0,0,0\n", i + 1));
    }
    std::fs::write(dir.join("greys.csv"), csv).expect("w");
    run(&mut s, "design.applyThreads", json!({"path": "greys.csv"}));
    assert_eq!(
        colours(&mut s),
        ["#101010", "#202020", "#303030", "#404040", "#505050"]
    );
    let o = run(&mut s, "object.list", json!({}));
    let first = run(&mut s, "object.get", json!({"id": o[0]["id"]}));
    assert_eq!(first["region"]["thread"]["name"], json!("Grey 0"));
    assert_eq!(first["region"]["thread"]["brand"], json!("Acme"));
}

#[test]
fn imported_lists_become_user_charts_and_charts_export_as_lists() {
    let (mut s, dir) = five_colours("charts");
    run(&mut s, "design.exportThreads", json!({"path": "t.col"}));
    run(&mut s, "threads.createChart", json!({"name": "Sent"}));
    let out = run(
        &mut s,
        "threads.importList",
        json!({"chart": "Sent", "path": "t.col"}),
    );
    assert_eq!(out["added"], json!(5));
    run(
        &mut s,
        "threads.exportList",
        json!({"chart": "Sent", "path": "chart.txt"}),
    );
    let back = ListType::Txt
        .read(&std::fs::read(dir.join("chart.txt")).expect("txt"))
        .expect("read");
    assert_eq!(back.iter().map(Thread::hex).collect::<Vec<_>>(), COLOURS);
    let builtin = run(
        &mut s,
        "threads.exportList",
        json!({"builtin": "janomeJef", "path": "jef.edr"}),
    );
    assert!(builtin["colors"].as_u64().expect("n") > 50);
}

#[test]
fn exp_writes_an_inf_only_when_asked_with_one_colour_per_block() {
    let (mut s, dir) = five_colours("exp");
    let out = run(&mut s, "design.export", json!({"path": "plain.exp"}));
    assert_eq!(out["paths"].as_array().map(Vec::len), Some(1));
    assert!(!dir.join("plain.inf").exists());
    let plain = std::fs::read(dir.join("plain.exp")).expect("exp");

    let out = run(
        &mut s,
        "design.export",
        json!({"path": "b.exp", "companionInf": true}),
    );
    assert_eq!(out["paths"].as_array().map(Vec::len), Some(2), "{out}");
    let exp = std::fs::read(dir.join("b.exp")).expect("exp");
    assert_eq!(exp, plain, "the EXP itself is unchanged");
    let blocks = openstitch_formats::exp::read(&exp)
        .expect("read")
        .count(Command::ColorChange)
        + 1;
    let inf = ListType::Inf
        .read(&std::fs::read(dir.join("b.inf")).expect("inf"))
        .expect("read");
    assert_eq!(inf.len(), blocks);
    assert_eq!(inf.iter().map(Thread::hex).collect::<Vec<_>>(), COLOURS);

    let err = s
        .execute(
            "design.export",
            Some(&json!({"path": "c.dst", "companionInf": true})),
        )
        .expect_err("EXP only");
    assert!(err.to_string().contains("EXP"), "{err}");
}
