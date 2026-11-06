//! Shapes ▸ Edit Outline ▸ Turning Gap (#497, beyond PE-DESIGN): part of the selected closed
//! line left unsewn, one `object.setLineGap` (one Undo).

use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use openstitch_design::LineGap;
use serde_json::json;

/// The single selected shape with a gap-able line around a closed outline.
pub fn target(app: &EditorApp) -> Option<u64> {
    if app.selection.len() != 1 {
        return None;
    }
    let id = *app.selection.first()?;
    let d = app.session.design();
    let o = &d.objects[d.index_of(id)?];
    (o.line.as_ref().is_some_and(|l| LineGap::applies_to(l.kind))
        && o.contours.iter().any(|c| c.closed))
    .then_some(id)
}

/// Offset and length (mm) the dropdown shows: the line's gap, else the last one typed.
fn values(app: &EditorApp, id: u64) -> (f64, f64, bool, f64) {
    let d = app.session.design();
    let o = &d.objects[d.index_of(id).expect("target")];
    let total = o.contours.iter().find(|c| c.closed).map_or(0.0, |c| {
        c.path()
            .windows(2)
            .map(|w| w[0].distance(w[1]))
            .sum::<f64>()
            / 10.0
    });
    match o.line.as_ref().and_then(|l| l.gap) {
        Some(g) => (g.offset, g.length, true, total),
        None => {
            let (off, len) = app.edit.line_gap.unwrap_or((0.0, 40.0f64.min(total / 3.0)));
            (off, len, false, total)
        }
    }
}

pub fn gap_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    let Some(id) = target(app) else {
        ui.label("Select one closed running, triple, zigzag, E or V line.");
        return;
    };
    let (mut offset, mut length, has, total) = values(app, id);
    ui.label(format!("Outline {total:.1} mm"));
    ui.horizontal(|ui| {
        ui.label("Starts at");
        ui.add(
            egui::DragValue::new(&mut offset)
                .range(0.0..=total.max(0.0))
                .speed(0.5)
                .suffix(" mm"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Length");
        ui.add(
            egui::DragValue::new(&mut length)
                .range(0.1..=(total - 1.0).max(0.1))
                .speed(0.5)
                .suffix(" mm"),
        );
    });
    app.edit.line_gap = Some((offset, length));
    let round = |v: f64| (v * 100.0).round() / 100.0;
    if ui
        .button(if has { "Update gap" } else { "Add gap" })
        .clicked()
    {
        let r = CommandRequest::new(
            "object.setLineGap",
            json!({ "ids": [id], "offset": round(offset), "length": round(length) }),
        );
        if app.execute(&r).is_some() {
            app.status = format!("Turning gap of {length:.1} mm");
        }
        ui.close();
    }
    if has && ui.button("Remove gap").clicked() {
        let r = CommandRequest::new("object.setLineGap", json!({ "ids": [id], "length": null }));
        app.execute(&r);
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{EditorApp, Launch};
    use openstitch_app::editor::CommandRequest;
    use serde_json::json;
    use std::collections::BTreeSet;

    #[test]
    fn the_gap_needs_one_closed_line_and_the_builder_is_a_tool() {
        let mut app = EditorApp::new(Launch::default());
        let r = app
            .execute(&CommandRequest::new(
                "shape.rectangle",
                json!({ "x": 10, "y": 10, "width": 40, "height": 20, "outline": "Black" }),
            ))
            .expect("rectangle")["id"]
            .as_u64()
            .expect("id");
        assert_eq!(super::target(&app), None);
        app.selection = BTreeSet::from([r]);
        assert_eq!(super::target(&app), Some(r));
        let (_, len, has, total) = super::values(&app, r);
        assert!(!has && (total - 120.0).abs() < 1e-6 && len == 40.0);
        app.ui_command("ui.run", Some(&json!({ "action": "shapeBuilder" })))
            .expect("picks the tool");
        assert_eq!(app.tools.tool(), openstitch_app::editor::Tool::ShapeBuilder);
    }
}
