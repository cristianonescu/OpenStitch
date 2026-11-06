//! Coverage for `stitchtool commands`, `exec` and `serve` through the binary (#318).

use openstitch_testutil::TempDir;
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn temp_dir(tag: &str) -> TempDir {
    TempDir::new(&format!("stitchtool-{tag}"))
}

fn stitchtool(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_stitchtool"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .expect("run stitchtool")
}

/// Runs `stitchtool serve` with `flags`, sends `lines` and returns one parsed reply per line out.
fn serve(dir: &Path, flags: &[&str], lines: &[&str]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_stitchtool"))
        .arg("serve")
        .args(flags)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn serve");
    let mut stdin = child.stdin.take().expect("stdin");
    for l in lines {
        writeln!(stdin, "{l}").expect("write");
    }
    drop(stdin);
    let out = child.wait_with_output().expect("output");
    assert!(out.status.success(), "serve exits cleanly at end of input");
    replies(&out)
}

fn replies(out: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("reply {l:?}: {e}")))
        .collect()
}

#[test]
fn commands_prints_the_catalogue_as_json() {
    let out = stitchtool(&temp_dir("commands"), &["commands"]);
    assert!(out.status.success());
    let catalogue: Value = serde_json::from_slice(&out.stdout).expect("JSON catalogue");
    let text = catalogue.to_string();
    for id in [
        "shape.ellipse",
        "design.export",
        "project.save",
        "history.undo",
    ] {
        assert!(text.contains(id), "catalogue lists {id}");
    }
}

#[test]
fn exec_runs_the_badge_example_and_writes_beside_the_script() {
    let dir = temp_dir("badge");
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/badge.json");
    let script = dir.join("badge.json");
    std::fs::copy(&example, &script).expect("copy example");
    let steps: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(&script).expect("read")).expect("array");

    // Run from another folder: relative paths resolve against the script's folder.
    let out = stitchtool(
        &temp_dir("badge-cwd"),
        &["exec", script.to_str().expect("utf-8")],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r = replies(&out);
    assert_eq!(r.len(), steps.len(), "one reply per step");
    assert!(r.iter().all(|v| v["ok"] == json!(true)), "{r:?}");
    for f in ["badge.pes", "badge.osd"] {
        let len = std::fs::metadata(dir.join(f)).map(|m| m.len()).unwrap_or(0);
        assert!(len > 0, "{f} written beside the script");
    }
    let pes = std::fs::read(dir.join("badge.pes")).expect("pes");
    assert!(pes.starts_with(b"#PES"), "badge.pes is a PES file");
}

#[test]
fn exec_stops_at_the_first_failure_with_exit_code_2() {
    let dir = temp_dir("fail");
    let script = dir.join("s.json");
    std::fs::write(
        &script,
        r#"[{"command":"shape.ellipse","params":{"cx":0,"cy":0,"rx":5,"ry":5}},
            {"command":"object.get","params":{"id":99}},
            {"command":"project.save","params":{"path":"never.osd"}}]"#,
    )
    .expect("write");
    let out = stitchtool(&dir, &["exec", "s.json"]);
    assert_eq!(out.status.code(), Some(2));
    let r = replies(&out);
    assert_eq!(r.len(), 2, "the step after the failure is not run");
    assert_eq!(r[0]["ok"], json!(true));
    assert_eq!(r[1]["ok"], json!(false));
    assert!(r[1]["error"].is_string());
    assert!(!dir.join("never.osd").exists());
}

#[test]
fn exec_reports_every_parameter_error_before_running_anything() {
    // #494: an unknown parameter, an out-of-range value, a wrong type and an unknown command,
    // all reported at once with their step numbers; the first step, which is fine, never runs.
    let dir = temp_dir("check");
    std::fs::write(
        dir.join("s.json"),
        r#"[{"command":"project.save","params":{"path":"never.osd"}},
            {"command":"shape.rectangle","params":{"x":0,"y":0,"width":5,"height":5,"colour":"red"}},
            {"command":"object.arrangeCopies","params":{"ids":[1],"mode":"matrix","rows":500}},
            {"command":"no.such.command"},
            {"command":"object.get","params":{"id":"one"}}]"#,
    )
    .expect("write");
    let out = stitchtool(&dir, &["exec", "s.json"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "nothing ran");
    assert!(!dir.join("never.osd").exists());
    let err = String::from_utf8_lossy(&out.stderr);
    for want in [
        "step 2 (shape.rectangle): parameter 'colour' is not a parameter",
        "step 4: unknown command 'no.such.command'",
        "step 3 (object.arrangeCopies): parameter 'rows' must be an integer in 1...100",
        "step 5 (object.get): parameter 'id' must be an integer",
        "nothing was run",
    ] {
        assert!(
            err.contains(want),
            "{want} in:
{err}"
        );
    }
}

#[test]
fn exec_refuses_a_script_that_is_not_an_array_or_missing() {
    let dir = temp_dir("bad");
    std::fs::write(dir.join("obj.json"), r#"{"command":"design.info"}"#).expect("write");
    let out = stitchtool(&dir, &["exec", "obj.json"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("JSON array"));
    assert!(out.stdout.is_empty());

    let missing = stitchtool(&dir, &["exec", "missing.json"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("cannot read"));
}

#[test]
fn serve_answers_one_line_per_request_and_skips_blank_lines() {
    let dir = temp_dir("serve");
    let r = serve(
        &dir,
        &[],
        &[
            r#"{"id":1,"command":"shape.ellipse","params":{"cx":0,"cy":0,"rx":5,"ry":5}}"#,
            "",
            "   ",
            "not json",
            r#"{"id":"x"}"#,
            r#"{"id":3,"command":"object.list"}"#,
            r#"{"id":4,"command":"project.save","params":{"path":"s.osd"}}"#,
        ],
    );
    assert_eq!(r.len(), 5, "blank lines get no reply: {r:?}");
    assert_eq!(r[0]["id"], json!(1));
    assert_eq!(r[0]["ok"], json!(true));
    assert_eq!(r[1], json!({"ok": false, "error": "invalid JSON request"}));
    assert_eq!(r[2]["id"], json!("x"));
    assert_eq!(r[2]["ok"], json!(false));
    assert_eq!(r[3]["result"].as_array().map(Vec::len), Some(1));
    assert_eq!(r[4]["ok"], json!(true));
    assert!(
        dir.join("s.osd").exists(),
        "serve resolves paths in its working folder"
    );
}

#[test]
fn serve_read_only_refuses_edits_and_writes_but_reads() {
    let dir = temp_dir("ro");
    let r = serve(
        &dir,
        &["--read-only"],
        &[
            r#"{"id":1,"command":"shape.ellipse","params":{"cx":0,"cy":0,"rx":5,"ry":5}}"#,
            r#"{"id":2,"command":"history.undo"}"#,
            r#"{"id":3,"command":"project.save","params":{"path":"ro.osd"}}"#,
            r#"{"id":4,"command":"design.info"}"#,
        ],
    );
    assert_eq!(r.len(), 4);
    for v in &r[..3] {
        assert_eq!(v["ok"], json!(false), "refused: {v}");
    }
    assert_eq!(r[3]["ok"], json!(true), "reads still work: {}", r[3]);
    assert!(!dir.join("ro.osd").exists());
}

#[test]
fn serve_no_files_refuses_file_reads_and_writes_but_edits() {
    let dir = temp_dir("nofiles");
    std::fs::write(dir.join("in.osd"), "{}").expect("write");
    let r = serve(
        &dir,
        &["--no-files"],
        &[
            r#"{"id":1,"command":"shape.ellipse","params":{"cx":0,"cy":0,"rx":5,"ry":5}}"#,
            r#"{"id":2,"command":"project.save","params":{"path":"nf.osd"}}"#,
            r#"{"id":3,"command":"project.open","params":{"path":"in.osd"}}"#,
            r#"{"id":4,"command":"design.export","params":{"path":"nf.pes"}}"#,
        ],
    );
    assert_eq!(r.len(), 4);
    assert_eq!(r[0]["ok"], json!(true), "edits still work: {}", r[0]);
    for v in &r[1..] {
        assert_eq!(v["ok"], json!(false), "refused: {v}");
    }
    assert!(!dir.join("nf.osd").exists());
    assert!(!dir.join("nf.pes").exists());
}

#[test]
fn unknown_arguments_print_the_usage_and_fail() {
    let dir = temp_dir("usage");
    for args in [&["frobnicate"][..], &["serve", "--bogus"], &[]] {
        let out = stitchtool(&dir, args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).starts_with("usage: stitchtool"),
            "{args:?}"
        );
    }
}
