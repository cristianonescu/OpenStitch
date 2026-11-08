//! The one-line commands (#496): `convert`, `info`, `optimize`, `digitize`, `formats`, through
//! the binary, compared with the `exec` scripts doing the same.

use openstitch_testutil::TempDir;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn temp_dir(tag: &str) -> TempDir {
    TempDir::new(&format!("stitchtool-oneline-{tag}"))
}

fn stitchtool(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_stitchtool"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .expect("run stitchtool")
}

fn ok(out: &Output) {
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The replies an `exec` script printed, one per line.
fn replies(out: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("reply {l:?}: {e}")))
        .collect()
}

/// Runs `steps` as an `exec` script written in `dir`.
fn exec(dir: &Path, steps: Value) -> Vec<Value> {
    let script = dir.join("script.json");
    std::fs::write(&script, steps.to_string()).expect("write script");
    let out = stitchtool(dir, &["exec", "script.json"]);
    ok(&out);
    replies(&out)
}

/// A small design from the ready-to-sew kit, copied into `dir` as `name`.
fn kit_file(dir: &Path, name: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/sew/machine-checks/machine-formats.pes");
    let to = dir.join(name);
    std::fs::copy(src, &to).expect("copy kit file");
    to
}

/// `design.info`'s stitch statistics of `file`, through `exec`.
fn stats(dir: &Path, file: &str) -> Value {
    let r = exec(
        dir,
        json!([
            {"command": "stitches.import", "params": {"path": file}},
            {"command": "design.info"}
        ]),
    );
    r[1]["result"]["stitchStats"].clone()
}

#[test]
fn convert_writes_the_same_file_as_the_exec_script() {
    let dir = temp_dir("convert");
    kit_file(&dir, "a.pes");
    // A DST to start from, then DST to PES both ways.
    ok(&stitchtool(&dir, &["convert", "a.pes", "a.dst"]));
    // The same file name in two folders: a PES holds its own name.
    std::fs::create_dir_all(dir.join("one-line")).expect("mkdir");
    std::fs::create_dir_all(dir.join("script")).expect("mkdir");
    let out = stitchtool(&dir, &["convert", "a.dst", "one-line/out.pes"]);
    ok(&out);
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("out.pes"),
        "says what it wrote"
    );
    exec(
        &dir,
        json!([
            {"command": "stitches.import", "params": {"path": "a.dst"}},
            {"command": "design.export", "params": {"path": "script/out.pes"}}
        ]),
    );
    let a = std::fs::read(dir.join("one-line/out.pes")).expect("one-line output");
    let b = std::fs::read(dir.join("script/out.pes")).expect("script output");
    assert!(!a.is_empty());
    assert_eq!(a, b, "convert and the exec script write the same bytes");
}

#[test]
fn info_json_is_design_info_and_the_text_lists_every_figure() {
    let dir = temp_dir("info");
    kit_file(&dir, "x.pes");
    let out = stitchtool(&dir, &["info", "x.pes", "--json"]);
    ok(&out);
    let one_line = replies(&out);
    let script = exec(
        &dir,
        json!([
            {"command": "stitches.import", "params": {"path": "x.pes"}},
            {"command": "design.info"}
        ]),
    );
    assert_eq!(one_line, script, "--json prints exec's replies");

    let out = stitchtool(&dir, &["info", "x.pes"]);
    ok(&out);
    let text = String::from_utf8_lossy(&out.stdout);
    let st = &script[1]["result"]["stitchStats"];
    for label in [
        "Size",
        "Stitches",
        "Colours",
        "Colour changes",
        "Trims",
        "Jumps",
        "Thread",
        "Sewing time",
    ] {
        assert!(text.contains(label), "{label} in {text}");
    }
    assert!(
        text.contains(&format!("Stitches       {}", st["stitches"])),
        "{text}"
    );
    assert!(text.contains(" m top, "), "{text}");
}

#[test]
fn folder_convert_skips_files_already_in_the_format() {
    let dir = temp_dir("folder");
    let src = dir.join("in");
    std::fs::create_dir_all(&src).expect("mkdir");
    kit_file(&src, "keep.pes");
    ok(&stitchtool(
        &dir,
        &["convert", "in/keep.pes", "in/other.dst"],
    ));
    // In place, as library.convert does: the PES already in the format is skipped.
    let out = stitchtool(&dir, &["convert", "in", "in", "--format", "pes"]);
    ok(&out);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("1 written, 1 skipped, 0 failed"), "{text}");
    assert!(dir.join("in/other.pes").is_file());
    // To a new folder (made when missing): converted, and the PES copied unchanged.
    let out = stitchtool(&dir, &["convert", "in", "out", "--format", "pes"]);
    ok(&out);
    assert_eq!(
        std::fs::read(dir.join("out/keep.pes")).expect("copied"),
        std::fs::read(src.join("keep.pes")).expect("source"),
        "same format copied unchanged"
    );
    // Again: every target exists, so everything is skipped.
    let out = stitchtool(&dir, &["convert", "in", "out", "--format", "pes"]);
    ok(&out);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("0 written,"), "{text}");
    assert!(text.contains(" 0 failed"), "{text}");
    // A folder convert needs its format.
    assert!(!stitchtool(&dir, &["convert", "in", "out"]).status.success());
}

#[test]
fn optimize_raises_neither_trims_nor_colour_changes() {
    let dir = temp_dir("optimize");
    kit_file(&dir, "in.pes");
    for (out_name, extra) in [("opt.pes", None), ("bycolor.pes", Some("--by-color"))] {
        let mut args = vec!["optimize", "in.pes", out_name];
        args.extend(extra);
        ok(&stitchtool(&dir, &args));
        let before = stats(&dir, "in.pes");
        let after = stats(&dir, out_name);
        assert_eq!(after["stitches"], before["stitches"], "{out_name}");
        for k in ["trims", "colorChanges"] {
            assert!(
                after[k].as_u64() <= before[k].as_u64(),
                "{out_name}: {k} {} -> {}",
                before[k],
                after[k]
            );
        }
    }
    let bad = stitchtool(&dir, &["optimize", "in.pes", "o.pes", "--mode", "fastest"]);
    assert_eq!(bad.status.code(), Some(2));
}

#[test]
fn optimize_sets_the_start_rule_with_auto_start() {
    // #547: `--auto-start nearest` sets the design's start rule before the reorder, so an open
    // line drawn away from the square starts at its nearer end: less travel in the export.
    let dir = temp_dir("autostart");
    exec(
        &dir,
        json!([
            {"command": "design.new", "params": {"width": 100, "height": 100}},
            {"command": "shape.rectangle",
             "params": {"x": 10, "y": 10, "width": 20, "height": 20, "fill": "#ff0000"}},
            {"command": "shape.path",
             "params": {"points": [[90, 20], [40, 20]], "closed": false, "outline": "#ff0000"}},
            {"command": "project.save", "params": {"path": "in.osd"}}
        ]),
    );
    // By colour, which keeps the objects' own entry points (order.optimize moves them too).
    ok(&stitchtool(
        &dir,
        &["optimize", "in.osd", "off.pes", "--by-color"],
    ));
    ok(&stitchtool(
        &dir,
        &[
            "optimize",
            "in.osd",
            "near.pes",
            "--by-color",
            "--auto-start",
            "nearest",
        ],
    ));
    let travel = |file: &str| stats(&dir, file)["travelMM"].as_f64().expect("present");
    let (off, near) = (travel("off.pes"), travel("near.pes"));
    assert!(near < off, "{off} -> {near}");
    let bad = stitchtool(
        &dir,
        &["optimize", "in.osd", "o.pes", "--auto-start", "far"],
    );
    assert!(!bad.status.success());
    assert!(!dir.join("o.pes").exists());
}

#[test]
fn digitize_auto_punches_an_image_at_the_given_width() {
    let dir = temp_dir("digitize");
    let image =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/images/d02-emblem-small.png");
    std::fs::copy(image, dir.join("logo.png")).expect("copy image");
    std::fs::create_dir_all(dir.join("one-line")).expect("mkdir");
    std::fs::create_dir_all(dir.join("script")).expect("mkdir");
    ok(&stitchtool(
        &dir,
        &[
            "digitize",
            "logo.png",
            "one-line/logo.pes",
            "--size",
            "60mm",
            "--colors",
            "4",
            "--method",
            "pe",
        ],
    ));
    let st = stats(&dir, "one-line/logo.pes");
    assert!(st["stitches"].as_u64().unwrap_or(0) > 100, "{st}");
    // The image is placed 60 mm wide; the stitches (background left out) fit inside it.
    let width = st["widthMM"].as_f64().expect("width");
    assert!(width > 20.0 && width <= 60.5, "within 60 mm: {width}");
    exec(
        &dir,
        json!([
            {"command": "digitize.autoPunch", "params": {
                "path": "logo.png", "width": 60.0, "colors": 4, "compatMode": "peDesign"}},
            {"command": "design.export", "params": {"path": "script/logo.pes"}}
        ]),
    );
    assert_eq!(
        std::fs::read(dir.join("one-line/logo.pes")).expect("one-line"),
        std::fs::read(dir.join("script/logo.pes")).expect("script"),
        "digitize and the exec script write the same bytes"
    );
    for bad in [
        vec!["digitize", "logo.png", "x.pes", "--method", "magic"],
        vec!["digitize", "logo.png", "x.pes", "--size", "wide"],
    ] {
        assert_eq!(stitchtool(&dir, &bad).status.code(), Some(1), "{bad:?}");
    }
}

#[test]
fn formats_lists_every_file_type_with_read_and_write() {
    let dir = temp_dir("formats");
    let out = stitchtool(&dir, &["formats", "--json"]);
    ok(&out);
    let list = &replies(&out)[0]["result"];
    let machine = list["machine"].as_array().expect("machine formats");
    assert_eq!(machine.len(), 22, "every FileType");
    let find = |ext: &str| {
        machine
            .iter()
            .find(|f| f["extension"] == ext)
            .unwrap_or_else(|| panic!("{ext} listed"))
    };
    assert_eq!(find("pes")["write"], true);
    assert_eq!(find("dst")["write"], true);
    assert_eq!(find("phc")["write"], false, "PHC is read only");
    assert!(machine.iter().all(|f| f["read"] == true));
    assert_eq!(list["exports"], json!(["csv", "txt", "gcode"]));

    let out = stitchtool(&dir, &["formats"]);
    ok(&out);
    let text = String::from_utf8_lossy(&out.stdout);
    for f in machine {
        let ext = f["extension"].as_str().expect("ext").to_uppercase();
        assert!(text.contains(&ext), "{ext} in {text}");
    }
    assert!(text.contains("PHC     yes   -"), "{text}");
}

#[test]
fn errors_exit_non_zero_with_one_line() {
    let dir = temp_dir("errors");
    kit_file(&dir, "a.pes");
    for (args, code) in [
        (vec!["convert", "a.pes", "a.xyz"], 2),
        (vec!["convert", "missing.pes", "b.pes"], 2),
        (vec!["info", "a.xyz"], 2),
        (vec!["info"], 1),
        (vec!["convert", "a.pes", "b.pes", "--colour"], 1),
    ] {
        let out = stitchtool(&dir, &args);
        assert_eq!(out.status.code(), Some(code), "{args:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        if code == 2 {
            assert_eq!(err.trim().lines().count(), 1, "{args:?}: {err}");
            assert!(err.starts_with("stitchtool: "), "{err}");
        }
    }
    // `help` shows the one-line commands.
    let out = stitchtool(&dir, &["help"]);
    ok(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("stitchtool convert <in> <out>"));
}

#[test]
fn thumbnail_writes_a_square_png_of_the_stitches() {
    let dir = temp_dir("thumbnail");
    kit_file(&dir, "a.pes");
    let out = stitchtool(&dir, &["thumbnail", "a.pes", "a.png", "--size", "96"]);
    ok(&out);
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("wrote a.png (96 x 96 px)"),
        "{out:?}"
    );
    let png = std::fs::read(dir.join("a.png")).expect("thumbnail written");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "a PNG");
    // IHDR width and height, big-endian, right after the signature and chunk header.
    assert_eq!(&png[16..24], &[0, 0, 0, 96, 0, 0, 0, 96], "96 x 96");
    // The same bytes as the shared renderer (the macOS and Windows handlers' code).
    let pattern = openstitch_render::thumbnail::load(&dir.join("a.pes")).expect("load");
    assert_eq!(
        png,
        openstitch_render::thumbnail::png(&pattern, 96).expect("png")
    );
    // --json and the default size.
    let out = stitchtool(&dir, &["thumbnail", "a.pes", "b.png", "--json"]);
    ok(&out);
    let reply: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(reply, json!({"path": "b.png", "width": 256, "height": 256}));
}

#[test]
fn thumbnail_refuses_malformed_files_and_bad_sizes() {
    let dir = temp_dir("thumbnail-bad");
    // A file named .pes that is not one: a one-line error, exit 2, nothing written, no panic.
    std::fs::write(dir.join("junk.pes"), b"#PES0001 not really").expect("write");
    let out = stitchtool(&dir, &["thumbnail", "junk.pes", "junk.png"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(!dir.join("junk.png").exists());
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("stitchtool: thumbnail:"));
    // An extension no reader takes.
    std::fs::write(dir.join("x.txt"), b"hello").expect("write");
    let out = stitchtool(&dir, &["thumbnail", "x.txt", "x.png"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    // Sizes outside 16..=1024 and a missing output are usage errors.
    kit_file(&dir, "a.pes");
    for args in [
        &["thumbnail", "a.pes", "a.png", "--size", "8"][..],
        &["thumbnail", "a.pes", "a.png", "--size", "5000"],
        &["thumbnail", "a.pes"],
    ] {
        assert_eq!(stitchtool(&dir, args).status.code(), Some(1), "{args:?}");
    }
}
