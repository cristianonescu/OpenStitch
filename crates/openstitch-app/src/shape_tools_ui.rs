//! Shapes ▸ Smooth Shape with its tolerance, and Stitches ▸ Smooth Stitches (#478). Requests
//! come from `openstitch_app::editor::shape_tools`; each action is one command, so one Undo.

use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::shape_tools;

/// Smooth Shape's tolerance, mm.
pub fn smooth_tolerance(app: &EditorApp) -> f64 {
    app.edit
        .smooth_tolerance
        .unwrap_or(shape_tools::SMOOTH_TOLERANCE)
}

pub fn can_smooth_shape(app: &EditorApp) -> bool {
    !shape_tools::shapes(app.session.design(), &app.selection).is_empty()
}

/// One `shape.simplify` of the selected shapes; the status line says how many nodes went.
pub fn smooth_shape(app: &mut EditorApp) {
    let ids = shape_tools::shapes(app.session.design(), &app.selection);
    if ids.is_empty() {
        return;
    }
    let r = shape_tools::simplify(&ids, smooth_tolerance(app));
    if let Some(v) = app.execute(&r) {
        let count = |k: &str| {
            v["objects"]
                .as_array()
                .map_or(0, |a| a.iter().filter_map(|o| o[k].as_u64()).sum::<u64>())
        };
        let (before, after) = (count("before"), count("after"));
        app.status = if before == after {
            "Smooth Shape: no node is within the tolerance".into()
        } else {
            format!("Smooth Shape: {before} nodes → {after}")
        };
    }
}

/// Shapes ▸ Smooth tolerance.
pub fn smooth_tolerance_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    let mut t = smooth_tolerance(app);
    ui.horizontal(|ui| {
        ui.label("Tolerance");
        ui.add(
            egui::DragValue::new(&mut t)
                .range(0.01..=10.0)
                .speed(0.01)
                .suffix(" mm"),
        );
    });
    for mm in [0.1, 0.2, 0.5, 1.0] {
        if ui.selectable_label(t == mm, format!("{mm} mm")).clicked() {
            t = mm;
        }
    }
    app.edit.smooth_tolerance = Some(t.clamp(0.01, 10.0));
}

/// One `stitches.smooth` on the selected stitch pattern (its selected needle points' span, or
/// all of it).
pub fn smooth_stitches(app: &mut EditorApp) {
    let (_, one, needles) = crate::convert_ui::stitch_selection(app);
    let Some(id) = one else { return };
    if let Some(v) = app.execute(&shape_tools::smooth_stitches(id, &needles)) {
        app.status = format!("Smooth Stitches: {} points moved", v["moved"]);
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{EditorApp, Launch};
    use openstitch_app::editor::CommandRequest;
    use serde_json::{Value, json};
    use std::collections::BTreeSet;

    fn ui_run(app: &mut EditorApp, key: &str) -> Result<Value, String> {
        app.ui_command("ui.run", Some(&json!({ "action": key })))
    }

    fn add(app: &mut EditorApp, command: &'static str, params: Value) -> u64 {
        app.execute(&CommandRequest::new(command, params))
            .expect(command)["id"]
            .as_u64()
            .expect("id")
    }

    #[test]
    fn smooth_shape_runs_from_the_ribbon_with_its_tolerance() {
        let mut app = EditorApp::new(Launch::default());
        let pts: Vec<Value> = (0..120)
            .map(|i| {
                let t = (i as f64 * 3.0).to_radians();
                json!([50.0 + 20.0 * t.cos(), 50.0 + 20.0 * t.sin()])
            })
            .collect();
        let o = add(&mut app, "shape.polygon", json!({ "points": pts }));
        app.selection = BTreeSet::new();
        assert!(ui_run(&mut app, "smoothShape").is_err(), "nothing selected");
        app.selection = BTreeSet::from([o]);
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "smoothTolerance", "value": 0.5 })),
        )
        .expect("ui.set smoothTolerance");
        assert_eq!(super::smooth_tolerance(&app), 0.5);
        ui_run(&mut app, "smoothShape").expect("smooth");
        let n = app.session.design().objects[0].contours[0].points.len();
        assert!(n < 60, "{n}");
        assert!(app.status.contains("120 nodes"), "{}", app.status);
        app.undo();
        assert_eq!(
            app.session.design().objects[0].contours[0].points.len(),
            120
        );
    }

    #[test]
    fn smooth_stitches_runs_on_the_selected_pattern() {
        let mut app = EditorApp::new(Launch::default());
        let l = add(
            &mut app,
            "shape.polygon",
            json!({ "points": [[0, 0], [10, 3], [20, 0], [30, 3], [40, 0]], "closed": false }),
        );
        app.execute(&CommandRequest::new(
            "object.toStitches",
            json!({ "ids": [l] }),
        ))
        .expect("to stitches");
        let st = app.session.design().objects[0].id;
        app.selection = BTreeSet::from([st]);
        let before = app.session.design().objects[0].stitch_block.clone();
        ui_run(&mut app, "smoothStitches").expect("smooth");
        let after = app.session.design().objects[0].stitch_block.clone();
        assert_ne!(before, after);
        assert_eq!(
            before.map(|b| b.stitches.len()),
            after.map(|b| b.stitches.len())
        );
        assert!(app.status.contains("points moved"), "{}", app.status);
    }
}
