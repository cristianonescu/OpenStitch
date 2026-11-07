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
    for show in SHOW {
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
