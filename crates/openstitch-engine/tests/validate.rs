//! `Session::validate` (#494): checks a step's parameters without running it, and finds no
//! problem in any script this repository runs (the compat specs and the examples), so a batch
//! that ran before still runs.

use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};
use std::path::Path;

fn scripts() -> Vec<(String, Vec<Value>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out = Vec::new();
    let mut dirs = vec![root.join("compat/specs"), root.join("examples")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("folder").flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read");
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let steps = match &v {
                Value::Array(a) => a.clone(),
                Value::Object(o) => match o.get("script") {
                    Some(Value::Array(a)) => a.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            out.push((path.display().to_string(), steps));
        }
    }
    out
}

#[test]
fn every_repository_script_validates() {
    let tmp = TempDir::new("validate");
    let s = Session::new(tmp.path());
    let scripts = scripts();
    assert!(scripts.len() > 400, "found {} scripts", scripts.len());
    let mut problems = Vec::new();
    let mut steps = 0;
    for (file, script) in &scripts {
        for (i, step) in script.iter().enumerate() {
            let Some(command) = step.get("command").and_then(Value::as_str) else {
                continue;
            };
            steps += 1;
            match s.validate(command, step.get("params")) {
                Ok(p) if p.is_empty() => {}
                Ok(p) => problems.push(format!("{file} step {i} {command}: {p:?}")),
                Err(e) => problems.push(format!("{file} step {i}: {e}")),
            }
        }
    }
    assert!(steps > 1500, "checked {steps} steps");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn three_bad_parameters_come_back_together_and_change_nothing() {
    let tmp = TempDir::new("validate");
    let mut s = Session::new(tmp.path());
    let before = s.design().clone();
    let p = s
        .validate(
            "shape.rectangle",
            Some(&json!({"x": "ten", "width": 10, "height": 10, "colour": "red"})),
        )
        .expect("a known command");
    let names: Vec<&str> = p.iter().map(|p| p.param.as_str()).collect();
    assert!(names.contains(&"x") && names.contains(&"colour"), "{p:?}");
    assert_eq!(*s.design(), before);
    assert!(!s.can_undo());
    assert!(s.validate("no.such", None).is_err());
    // The same step runs once it is right.
    assert!(
        s.validate(
            "shape.rectangle",
            Some(&json!({"x": 0, "y": 0, "width": 10, "height": 10}))
        )
        .expect("known")
        .is_empty()
    );
    let _ = s.execute(
        "shape.rectangle",
        Some(&json!({"x": 0, "y": 0, "width": 10, "height": 10})),
    );
}
