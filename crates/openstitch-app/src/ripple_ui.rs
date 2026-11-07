//! The ripple line's target on the canvas (#541, beyond PE-DESIGN): the selected ripple line
//! shows the point its copies shrink towards; dragging it sends one `object.setLine`, one
//! undo step.

use crate::app::EditorApp;
use eframe::egui::{self, Color32, Stroke};
use openstitch_app::editor::{CommandRequest, Phase, Viewport};
use openstitch_design::{LineType, Vec2};
use serde_json::json;

/// The single selected object with a ripple line: its id and its target (the line's centre
/// when none is set).
fn target(app: &EditorApp) -> Option<(u64, Vec2)> {
    if app.selection.len() != 1 {
        return None;
    }
    let id = *app.selection.first()?;
    let d = app.session.design();
    let o = &d.objects[d.index_of(id)?];
    let l = o.line.as_ref().filter(|l| l.kind == LineType::Ripple)?;
    let at = l.target.unwrap_or_else(|| {
        let pts: Vec<Vec2> = o
            .contours
            .iter()
            .flat_map(|c| c.points.iter().copied())
            .collect();
        openstitch_design::ripple::centre(&pts)
    });
    Some((id, at))
}

/// The `object.setLine` a drag of the target to `at` (0.1 mm) sends.
pub fn target_request(id: u64, at: Vec2) -> CommandRequest {
    CommandRequest::new(
        "object.setLine",
        json!({ "ids": [id], "target": [at.x / 10.0, at.y / 10.0] }),
    )
}

/// The canvas pointer on the target marker; whether it took the event.
pub fn pointer(app: &mut EditorApp, phase: Phase, at: Vec2, tolerance: f64) -> bool {
    match phase {
        Phase::Down => {
            let Some((id, t)) = target(app) else {
                return false;
            };
            if t.distance(at) > tolerance {
                return false;
            }
            app.ui.ripple_drag = Some((id, at));
            true
        }
        Phase::Dragged => match app.ui.ripple_drag.as_mut() {
            Some(d) => {
                d.1 = at;
                true
            }
            None => false,
        },
        Phase::Up => {
            let Some((id, _)) = app.ui.ripple_drag.take() else {
                return false;
            };
            if target(app).is_some_and(|t| t.0 == id && t.1.distance(at) > 1e-6) {
                app.execute(&target_request(id, at));
            }
            true
        }
        Phase::Moved => false,
    }
}

/// Draws the target marker (a ring with a cross), following a drag.
pub fn draw(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let Some((_, t)) = target(app) else {
        return;
    };
    let at = app.ui.ripple_drag.map_or(t, |d| d.1);
    let s = vp.to_screen(at);
    let c = egui::pos2(s.x as f32, s.y as f32);
    let colour = Color32::from_rgb(0xE0, 0x6C, 0x1A);
    painter.circle_filled(c, 5.0, Color32::WHITE);
    painter.circle_stroke(c, 5.0, Stroke::new(1.5, colour));
    for (dx, dy) in [(7.0, 0.0), (0.0, 7.0)] {
        painter.line_segment(
            [
                egui::pos2(c.x - dx, c.y - dy),
                egui::pos2(c.x + dx, c.y + dy),
            ],
            Stroke::new(1.0, colour),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_design::v2;

    #[test]
    fn a_target_drag_is_one_set_line_in_millimetres() {
        let r = target_request(4, v2(250.0, 125.0));
        assert_eq!(r.command, "object.setLine");
        assert_eq!(r.params["ids"], json!([4]));
        assert_eq!(r.params["target"], json!([25.0, 12.5]));
    }
}
