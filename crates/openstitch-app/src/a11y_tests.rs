//! Screen-reader audit (#499): every interactive node of the AccessKit tree the window sends to
//! the screen reader (buttons, check boxes, combo boxes, text fields, sliders, links, tabs, menu
//! items) has a name, in the main window and in each dialog the `--show` scenes open. A child
//! module of `app`, so it drives the whole frame (`eframe::App::ui`) as the window does.

use super::*;
use eframe::egui::accesskit::{Node, NodeId, Role};

const INTERACTIVE: &[Role] = &[
    Role::Button,
    Role::CheckBox,
    Role::ComboBox,
    Role::TextInput,
    Role::Slider,
    Role::SpinButton,
    Role::Link,
    Role::MenuItem,
    Role::MenuItemCheckBox,
    Role::RadioButton,
    Role::Tab,
];

/// Runs a few frames of `app` with AccessKit on; returns the last tree's nodes.
fn tree(app: &mut EditorApp) -> Vec<(NodeId, Node)> {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut frame = eframe::Frame::_new_kittest();
    let mut nodes = Vec::new();
    for _ in 0..4 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| eframe::App::ui(app, ui, &mut frame));
        out.textures_delta.clear();
        if let Some(update) = out.platform_output.accesskit_update {
            nodes = update.nodes;
        }
    }
    nodes
}

/// Interactive nodes without a name (label, description or placeholder), each with the text of
/// the node before it under the same parent (usually the label the control sits beside).
fn unnamed(nodes: &[(NodeId, Node)]) -> Vec<String> {
    let by_id: std::collections::HashMap<NodeId, &Node> =
        nodes.iter().map(|(i, n)| (*i, n)).collect();
    let mut parent = std::collections::HashMap::new();
    for (id, n) in nodes {
        for c in n.children() {
            parent.insert(*c, *id);
        }
    }
    let text = |n: &Node| -> Option<String> {
        n.label()
            .or(n.value())
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
    };
    let named = |s: Option<&str>| s.is_some_and(|s| !s.trim().is_empty());
    nodes
        .iter()
        .filter(|(_, n)| INTERACTIVE.contains(&n.role()))
        .filter(|(_, n)| !(named(n.label()) || named(n.description()) || named(n.placeholder())))
        .map(|(id, n)| {
            let before = parent.get(id).and_then(|p| by_id.get(p)).and_then(|p| {
                let kids = p.children();
                let i = kids.iter().position(|c| c == id)?;
                kids[..i]
                    .iter()
                    .rev()
                    .find_map(|c| by_id.get(c).and_then(|n| text(n)))
            });
            format!("{:?} after {:?}", n.role(), before.unwrap_or_default())
        })
        .collect()
}

#[test]
fn every_interactive_control_has_a_name_for_screen_readers() {
    let mut scenes: Vec<(&str, Launch)> = vec![(
        "main window",
        Launch {
            demo: true,
            select: vec![1],
            ..Launch::default()
        },
    )];
    // `bare` draws the canvas alone, with no controls to name: kit preview tiles only (#1051).
    for show in SHOW.into_iter().filter(|&s| s != "bare") {
        scenes.push((
            show,
            Launch {
                demo: true,
                select: vec![1],
                show: vec![show.to_string()],
                ..Launch::default()
            },
        ));
    }
    let mut problems = Vec::new();
    for (name, launch) in scenes {
        let mut app = EditorApp::new(launch);
        // The window's own close path is not part of the audit.
        app.splash = None;
        let nodes = tree(&mut app);
        assert!(
            nodes.len() > 20,
            "{name}: the tree has {} nodes; AccessKit is not on",
            nodes.len()
        );
        let bad = unnamed(&nodes);
        if !bad.is_empty() {
            problems.push(format!("{name}: {} unnamed: {}", bad.len(), bad.join("; ")));
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn text_size_scales_the_whole_window_through_ui_set() {
    let mut app = EditorApp::new(Launch::default());
    assert_eq!(app.ui.prefs.text_size, 100);
    app.ui_command("ui.set", Some(&json!({ "name": "textSize", "value": 150 })))
        .expect("set");
    assert!(
        app.ui_command("ui.set", Some(&json!({ "name": "textSize", "value": 300 })))
            .is_err(),
        "100-200"
    );
    let ctx = egui::Context::default();
    let mut frame = eframe::Frame::_new_kittest();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        eframe::App::ui(&mut app, ui, &mut frame)
    });
    out.textures_delta.clear();
    assert!(
        (ctx.zoom_factor() - 1.5).abs() < 1e-6,
        "{}",
        ctx.zoom_factor()
    );
}

/// #1109: a zigzag net from a version-5 file with a 0.5 mm line pitch loads with a 0.1 mm gap and
/// a 0.4 mm band, below the inspector's ranges (Spacing 2.0-10.0, Zigzag width 1.0-10.0).
/// Selecting it and drawing the inspector shows those values without changing them: only the
/// user's edit may (egui clamped a value outside the range on drawing, and that was written back).
#[test]
fn the_inspector_shows_a_stored_value_outside_its_range_without_changing_it() {
    use openstitch_design::geometry::Contour;
    use openstitch_design::model::{Design, DesignObject, DesignPage, RegionSewing, RegionType};
    let mut d = Design::new("net", DesignPage::default());
    let mut r = RegionSewing::new(RegionType::ZigzagNetFill, openstitch_core::Thread::rgb(0xff0000));
    r.spacing = Some(0.5);
    d.add(DesignObject::shape(
        vec![Contour::rectangle(100.0, 100.0, 200.0, 200.0)],
        Some(r),
        None,
    ));
    let mut v: serde_json::Value =
        serde_json::from_slice(&openstitch_design::project::encode(&d).expect("encode"))
            .expect("json");
    v["version"] = json!(5);
    let path = std::env::temp_dir().join(format!("openstitch-1109-{}.osd", std::process::id()));
    std::fs::write(&path, serde_json::to_vec(&v).expect("bytes")).expect("write");
    let mut app = EditorApp::new(Launch {
        open: Some(path.clone()),
        select: vec![1],
        ..Launch::default()
    });
    std::fs::remove_file(&path).ok();
    app.splash = None;
    let stored = |app: &EditorApp| {
        let r = app.session.design().objects[0]
            .region
            .clone()
            .expect("region");
        (r.spacing, r.pattern_width)
    };
    let loaded = stored(&app);
    let design = app.session.design().clone();
    assert!(
        loaded.0.is_some_and(|g| g < 0.5) && loaded.1.is_some_and(|w| w < 1.0),
        "migrated below the ranges: {loaded:?}"
    );
    let nodes = tree(&mut app);
    let shows = |s: &str| {
        nodes
            .iter()
            .any(|(_, n)| n.label() == Some(s) || n.value() == Some(s))
    };
    assert!(shows("Zigzag width"), "the inspector shows the zigzag net's fields");
    assert_eq!(stored(&app), loaded, "drawing the inspector kept the stored values");
    assert!(*app.session.design() == design, "nothing else was edited");
}

/// #1109: with a stored value outside the field's range shown as it is, the user's own edit is
/// still kept to the range. A Spacing field (2.0-10.0) holding 0.6 returns nothing while it is
/// drawn and focused; typing 0.5 and Enter returns 2.0, typing 50 returns 10.0.
#[test]
fn typing_into_an_inspector_field_still_keeps_to_its_range() {
    let ctx = egui::Context::default();
    let mut app = EditorApp::new(Launch::default());
    app.splash = None;
    let run = |app: &mut EditorApp, events: Vec<egui::Event>, focus: bool| {
        let mut got = None;
        let mut out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                let id = ui.next_auto_id();
                if focus {
                    ui.memory_mut(|m| m.request_focus(id));
                }
                got = crate::panels::number(
                    app,
                    ui,
                    "region.spacing",
                    0.6,
                    (2.0, 10.0),
                    0.05,
                    " mm",
                );
            },
        );
        out.textures_delta.clear();
        got
    };
    let enter = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    for (typed, kept) in [("0.5", 2.0), ("50", 10.0)] {
        let shown = [
            run(&mut app, Vec::new(), false),
            run(&mut app, Vec::new(), true),
            run(&mut app, Vec::new(), false),
        ];
        assert!(
            shown.iter().all(Option::is_none),
            "drawn and focused, 0.6 is kept: {shown:?}"
        );
        let typed_in = run(&mut app, vec![egui::Event::Text(typed.into())], false);
        let entered = run(&mut app, vec![enter.clone()], false);
        let after = run(&mut app, Vec::new(), false);
        assert_eq!(
            [typed_in, entered, after].into_iter().flatten().collect::<Vec<_>>(),
            vec![kept],
            "typing {typed} gives {kept}"
        );
    }
}
