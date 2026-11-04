//! Coverage for smaller editor claims (#351): the ready-to-sew kit found in the Linux and
//! Windows package layouts (the existing test covers macOS), and every inspector choice and
//! toggle control of the main line and region types reaching the engine.

use openstitch_app::editor::inspector::{self, Control, InspectorModel};
use openstitch_app::editor::{CommandRequest, sew_kit};
use openstitch_design::{Design, RegionType};
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

fn temp_dir(tag: &str) -> TempDir {
    TempDir::new(&format!("coverage-editor-more-{tag}"))
}

fn kit_in(root: &Path, exe: &str, kit: &str) {
    let exe = root.join(exe);
    std::fs::create_dir_all(exe.parent().expect("parent")).expect("dir");
    assert_eq!(sew_kit::bundled(&exe), None, "{kit}: no kit packaged yet");
    std::fs::create_dir_all(root.join(kit)).expect("kit dir");
    std::fs::write(root.join(kit).join("manifest.json"), "{}").expect("manifest");
    let found = sew_kit::bundled(&exe).expect("the packaged kit");
    let want = std::fs::canonicalize(root.join(kit)).expect("canonical");
    assert_eq!(std::fs::canonicalize(&found).expect("found"), want, "{kit}");
    let s = found.to_string_lossy();
    assert!(!s.contains(".."), "a plain path: {s}");
    assert!(!s.starts_with(r"\\?\"), "no verbatim prefix: {s}");
}

#[test]
fn the_kit_is_found_in_the_linux_and_windows_layouts() {
    // Linux (AppImage, tarball): bin/openstitch beside share/openstitch/sew.
    kit_in(&temp_dir("linux"), "bin/openstitch", "share/openstitch/sew");
    // Windows (MSI, zip): the kit folder beside openstitch.exe.
    kit_in(&temp_dir("windows"), "openstitch.exe", "sew");
}

#[test]
fn a_folder_without_its_manifest_is_not_the_kit() {
    let root = temp_dir("nomanifest");
    std::fs::create_dir_all(root.join("sew")).expect("dir");
    assert_eq!(sew_kit::bundled(&root.join("openstitch.exe")), None);
}

fn run(s: &mut Session, r: &CommandRequest) -> Value {
    s.execute(r.command, Some(&r.params))
        .unwrap_or_else(|e| panic!("{} {}: {e}", r.command, r.params))
}

/// Reads a control's value back from the next model.
type Reader = fn(&Control) -> Option<Value>;

/// Sets each choice to another option and each toggle the other way, and checks the next
/// model shows it.
fn round_trip(
    s: &mut Session,
    id: u64,
    controls: impl Fn(&Design) -> Vec<Control>,
    set: impl Fn(&InspectorModel, Value) -> CommandRequest,
    what: &str,
) -> usize {
    let sel = BTreeSet::from([id]);
    let mut checked = 0;
    for c in controls(s.design()) {
        let param = c.param();
        let (wanted, was, read): (Value, Value, Reader) = match &c {
            Control::Choice { value, options, .. } => {
                let Some((other, _)) = options.iter().find(|(v, _)| v != value) else {
                    continue;
                };
                (json!(other), json!(value), |c| match c {
                    Control::Choice { value, .. } => Some(json!(value)),
                    _ => None,
                })
            }
            Control::Toggle { value, .. } => (json!(!value), json!(value), |c| match c {
                Control::Toggle { value, .. } => Some(json!(value)),
                _ => None,
            }),
            Control::Number { .. } => continue,
        };
        let m = InspectorModel::new(s.design(), &sel);
        run(s, &set(&m, c.params(wanted.clone())));
        let back = controls(s.design())
            .iter()
            .find(|c| c.param() == param)
            .and_then(read);
        assert_eq!(back, Some(wanted.clone()), "{what}: {param}");
        // Put it back: a toggle can hide the controls that depend on it (under sewing off hides
        // its style), and the next ones are checked from the original settings.
        let m = InspectorModel::new(s.design(), &sel);
        run(s, &set(&m, c.params(was)));
        checked += 1;
    }
    checked
}

#[test]
fn every_line_and_region_choice_and_toggle_reaches_the_engine() {
    let mut s = Session::new(std::env::temp_dir());
    let id = s
        .execute(
            "shape.rectangle",
            Some(&json!({"x": 10, "y": 10, "width": 40, "height": 20})),
        )
        .expect("rectangle")["id"]
        .as_u64()
        .expect("id");
    let sel = BTreeSet::from([id]);
    let mut total = 0;
    for (kind, name) in inspector::line_types() {
        let Some(kind) = kind else { continue };
        let m = InspectorModel::new(s.design(), &sel);
        run(&mut s, &m.set_line_type(Some(kind)));
        let line = |d: &Design| d.objects[0].line.clone().expect("line");
        total += round_trip(
            &mut s,
            id,
            |d| inspector::line_controls(&line(d)),
            |m, p| m.set_line(p),
            name,
        );
    }
    for kind in [
        RegionType::Fill,
        RegionType::Satin,
        RegionType::ProgrammableFill,
        RegionType::MotifFill,
    ] {
        let m = InspectorModel::new(s.design(), &sel);
        run(&mut s, &m.set_region_type(Some(kind)));
        let region = |d: &Design| d.objects[0].region.clone().expect("region");
        total += round_trip(
            &mut s,
            id,
            |d| inspector::region_controls(&region(d)),
            |m, p| m.set_region(p),
            inspector::region_name(Some(kind)),
        );
    }
    assert!(total >= 6, "choice and toggle controls checked: {total}");
}
