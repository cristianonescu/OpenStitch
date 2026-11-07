//! Text Attributes ▸ Free envelope (#535, beyond PE-DESIGN): start a free envelope from the
//! text's Text Transform preset or a shape, then drag its six handles on the canvas. Each drag
//! is one `text.bendEnvelope` (one Undo); Shift mirrors the move on the other side.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui::{self, Color32, Stroke};
use openstitch_app::editor::{CommandRequest, Phase, Viewport};
use openstitch_design::Vec2;
use openstitch_design::text::{TextSpec, envelope_handles, envelope_outline};
use serde_json::json;

const EDGES: [&str; 2] = ["top", "bottom"];
const HANDLES: [&str; 3] = ["left", "middle", "right"];

/// A handle being dragged: the text, which handle, where the drag started and where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvelopeDrag {
    id: u64,
    edge: usize,
    handle: usize,
    from: Vec2,
    to: Vec2,
}

/// The single selected text with a free envelope, when handle editing is on.
fn target(app: &EditorApp) -> Option<(u64, TextSpec)> {
    if !app.ui.envelope_edit || app.selection.len() != 1 {
        return None;
    }
    let id = *app.selection.first()?;
    let d = app.session.design();
    let spec = d.objects[d.index_of(id)?].text.clone()?;
    spec.custom_envelope.is_some().then_some((id, spec))
}

/// The pane's controls, under the Text Transform palette.
pub fn pane(app: &mut EditorApp, ui: &mut egui::Ui, spec: &TextSpec) {
    ui.label(egui::RichText::new(t("ui-free-envelope")).strong())
        .on_hover_text(t("ui-bend-the-text-with-handles-beyond-tip"));
    ui.horizontal_wrapped(|ui| {
        let preset = spec.envelope.is_some_and(|n| n > 2);
        if ui
            .add_enabled(preset, egui::Button::new(t("ui-from-preset")))
            .on_hover_text(t("ui-the-text-transform-preset-as-a-tip"))
            .clicked()
        {
            set(app, "preset");
        }
        for (shape, label) in [
            ("bridge", "Bridge"),
            ("pennant", "Pennant"),
            ("perspective", "Perspective"),
            ("diamond", "Diamond"),
        ] {
            if ui.button(label).clicked() {
                set(app, shape);
            }
        }
        if spec.custom_envelope.is_some() && ui.button(t("ui-remove")).clicked() {
            set(app, "none");
        }
    });
    if spec.custom_envelope.is_some() {
        ui.checkbox(&mut app.ui.envelope_edit, t("ui-edit-handles"))
            .on_hover_text(t("ui-drag-a-handle-on-the-canvas-tip"));
    }
}

fn set(app: &mut EditorApp, shape: &str) {
    let ids: Vec<u64> = crate::convert_ui::texts(app);
    if ids.is_empty() {
        return;
    }
    if app
        .execute(&CommandRequest::new(
            "text.setEnvelope",
            json!({ "ids": ids, "shape": shape }),
        ))
        .is_some()
        && shape != "none"
    {
        app.ui.envelope_edit = true;
    }
}

/// Handles the canvas pointer while handle editing is on; whether it took the event.
pub fn pointer(app: &mut EditorApp, phase: Phase, at: Vec2, shift: bool, tolerance: f64) -> bool {
    match phase {
        Phase::Down => {
            let Some((id, spec)) = target(app) else {
                return false;
            };
            let Some(h) = envelope_handles(&spec, &app.session.design().fonts) else {
                return false;
            };
            for (edge, row) in h.iter().enumerate() {
                for (handle, p) in row.iter().enumerate() {
                    if p.distance(at) <= tolerance {
                        app.ui.envelope_drag = Some(EnvelopeDrag {
                            id,
                            edge,
                            handle,
                            from: *p,
                            to: at,
                        });
                        return true;
                    }
                }
            }
            false
        }
        Phase::Dragged => match app.ui.envelope_drag.as_mut() {
            Some(d) => {
                d.to = at;
                true
            }
            None => false,
        },
        Phase::Up => {
            let Some(d) = app.ui.envelope_drag.take() else {
                return false;
            };
            let delta = at - d.from;
            if delta.x.abs() > 1e-6 || delta.y.abs() > 1e-6 {
                app.execute(&bend_request(&d, at, shift));
            }
            true
        }
        Phase::Moved => false,
    }
}

/// The `text.bendEnvelope` a finished drag sends.
pub fn bend_request(d: &EnvelopeDrag, at: Vec2, shift: bool) -> CommandRequest {
    CommandRequest::new(
        "text.bendEnvelope",
        json!({
            "id": d.id,
            "edge": EDGES[d.edge],
            "handle": HANDLES[d.handle],
            "dx": (at.x - d.from.x) / 10.0,
            "dy": (at.y - d.from.y) / 10.0,
            "symmetric": shift,
        }),
    )
}

/// Draws the envelope's edges and handles (and the handle being dragged).
pub fn draw(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let Some((_, spec)) = target(app) else {
        return;
    };
    let fonts = &app.session.design().fonts;
    let colour = Color32::from_rgb(0xE0, 0x6C, 0x1A);
    let screen = |p: Vec2| {
        let s = vp.to_screen(p);
        egui::pos2(s.x as f32, s.y as f32)
    };
    if let Some(edges) = envelope_outline(&spec, fonts, 48) {
        for e in edges {
            painter.add(egui::Shape::dashed_line(
                &e.into_iter().map(screen).collect::<Vec<_>>(),
                Stroke::new(1.0, colour),
                6.0,
                4.0,
            ));
        }
    }
    if let Some(h) = envelope_handles(&spec, fonts) {
        for p in h.iter().flatten() {
            let r = egui::Rect::from_center_size(screen(*p), egui::vec2(8.0, 8.0));
            painter.rect_filled(r, 1.0, Color32::WHITE);
            painter.rect_stroke(r, 1.0, Stroke::new(1.5, colour), egui::StrokeKind::Inside);
        }
    }
    if let Some(d) = app.ui.envelope_drag {
        painter.line_segment([screen(d.from), screen(d.to)], Stroke::new(1.0, colour));
        painter.circle_filled(screen(d.to), 4.0, colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_design::v2;

    #[test]
    fn a_drag_becomes_one_bend_in_millimetres() {
        let d = EnvelopeDrag {
            id: 7,
            edge: 1,
            handle: 2,
            from: v2(100.0, 50.0),
            to: v2(120.0, 20.0),
        };
        let r = bend_request(&d, v2(120.0, 20.0), true);
        assert_eq!(r.command, "text.bendEnvelope");
        let p = r.params;
        assert_eq!(p["edge"], "bottom");
        assert_eq!(p["handle"], "right");
        assert_eq!(p["dx"], 2.0);
        assert_eq!(p["dy"], -3.0);
        assert_eq!(p["symmetric"], true);
    }
}
