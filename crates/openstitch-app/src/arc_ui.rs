//! Text Attributes ▸ Arc (#456, beyond PE-DESIGN): every line of a text on a ring round one
//! centre. The pane's row and the canvas's centre handle each send one `object.setText`, one
//! undo step.

use crate::app::EditorApp;
use eframe::egui::{self, Color32, Stroke};
use openstitch_app::editor::{CommandRequest, Phase, Viewport};
use openstitch_design::text::TextSpec;
use openstitch_design::{Vec2, v2};
use serde_json::{Value, json};

/// The `object.setText` parameters the arc row sends: `on` arcs straight text at `default`
/// mm (arching up) or straightens it; otherwise the radius or the direction changes.
pub fn arc_params(spec: &TextSpec, on: bool, radius: f64, up: bool) -> Option<Value> {
    match (spec.arc, on) {
        (None, false) => None,
        (Some(_), false) => Some(json!({ "arc": null })),
        (None, true) => Some(json!({ "arc": radius })),
        (Some(r), true) => {
            let next = if up { radius.abs() } else { -radius.abs() };
            ((next - r).abs() > 1e-9).then(|| json!({ "arc": next }))
        }
    }
}

/// The Text Attributes pane's Arc row (in its grid): on/off, radius and Up/Down.
pub fn arc_row(app: &mut EditorApp, ui: &mut egui::Ui, spec: &TextSpec) {
    ui.label("Arc").on_hover_text(
        "Beyond PE-DESIGN: every line on a ring round one centre, the first line's radius set here; further lines step one line spacing inward (Up) or outward (Down)",
    );
    let mut params = None;
    ui.horizontal(|ui| {
        let mut on = spec.arc.is_some();
        let r = spec.arc.unwrap_or(40.0);
        if ui.checkbox(&mut on, "").changed() {
            params = arc_params(spec, on, r.abs(), r > 0.0);
        }
        if spec.arc.is_some() {
            let mut radius = r.abs();
            let d = ui.add(
                egui::DragValue::new(&mut radius)
                    .range(5.0..=1000.0)
                    .speed(0.5)
                    .suffix(" mm"),
            );
            if (d.drag_stopped() || d.lost_focus())
                && let Some(p) = arc_params(spec, true, radius, r > 0.0)
            {
                params = Some(p);
            }
            for (up, name) in [(true, "Up"), (false, "Down")] {
                if ui.selectable_label((r > 0.0) == up, name).clicked()
                    && let Some(p) = arc_params(spec, true, r.abs(), up)
                {
                    params = Some(p);
                }
            }
        }
    });
    if let Some(p) = params {
        crate::ribbon::set_text(app, p);
    }
}

/// The single selected text with an arc: its id, its spec and the ring's centre and the top
/// of the first ring on the page (0.1 mm).
fn arc_target(app: &EditorApp) -> Option<(u64, TextSpec, Vec2, Vec2)> {
    if app.selection.len() != 1 {
        return None;
    }
    let id = *app.selection.first()?;
    let d = app.session.design();
    let spec = d.objects[d.index_of(id)?].text.clone()?;
    let r = spec.arc.filter(|_| spec.path.is_none())?;
    let centre = spec.transform.apply(v2(0.0, r * 10.0));
    let top = spec.transform.apply(v2(0.0, 0.0));
    Some((id, spec, centre, top))
}

/// The arc radius (mm, signed as [`TextSpec::arc`]) when the ring's centre is dragged to `at`:
/// its distance from the first line's top along the text's up-down axis.
pub fn arc_for_centre(spec: &TextSpec, at: Vec2) -> f64 {
    let t = spec.transform;
    let axis = v2(t.c, t.d);
    let len2 = (axis.x * axis.x + axis.y * axis.y).max(1e-12);
    let local = ((at.x - t.tx) * axis.x + (at.y - t.ty) * axis.y) / len2;
    let mm = local / 10.0;
    let sign = if mm < 0.0 { -1.0 } else { 1.0 };
    sign * mm.abs().clamp(
        openstitch_design::text::MIN_ARC,
        openstitch_design::text::MAX_ARC,
    )
}

/// The canvas pointer on an arched text's centre handle: a drag moves the centre, so the ring
/// tightens or opens (and flips from up to down past the text); one `object.setText` per drag.
pub fn arc_pointer(app: &mut EditorApp, phase: Phase, at: Vec2, tolerance: f64) -> bool {
    match phase {
        Phase::Down => {
            let Some((id, _, centre, _)) = arc_target(app) else {
                return false;
            };
            if centre.distance(at) > tolerance {
                return false;
            }
            app.ui.arc_drag = Some((id, at));
            true
        }
        Phase::Dragged => match app.ui.arc_drag.as_mut() {
            Some(d) => {
                d.1 = at;
                true
            }
            None => false,
        },
        Phase::Up => {
            let Some((id, _)) = app.ui.arc_drag.take() else {
                return false;
            };
            if let Some((_, spec, _, _)) = arc_target(app).filter(|t| t.0 == id) {
                let r = arc_for_centre(&spec, at);
                if spec.arc.is_some_and(|old| (old - r).abs() > 1e-6) {
                    app.execute(&CommandRequest::new(
                        "object.setText",
                        json!({ "ids": [id], "arc": r }),
                    ));
                }
            }
            true
        }
        Phase::Moved => false,
    }
}

/// Draws an arched text's first ring (dashed) and its centre handle, following a drag.
pub fn arc_draw(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let Some((_, spec, centre, top)) = arc_target(app) else {
        return;
    };
    let colour = Color32::from_rgb(0xE0, 0x6C, 0x1A);
    let centre = app.ui.arc_drag.map_or(centre, |d| {
        let r = arc_for_centre(&spec, d.1);
        spec.transform.apply(v2(0.0, r * 10.0))
    });
    let screen = |p: Vec2| {
        let s = vp.to_screen(p);
        egui::pos2(s.x as f32, s.y as f32)
    };
    let radius = centre.distance(top);
    let ring: Vec<egui::Pos2> = (0..=96)
        .map(|k| {
            let a = k as f64 / 96.0 * std::f64::consts::TAU;
            screen(v2(centre.x + radius * a.cos(), centre.y + radius * a.sin()))
        })
        .collect();
    painter.add(egui::Shape::dashed_line(
        &ring,
        Stroke::new(1.0, colour),
        6.0,
        4.0,
    ));
    let r = egui::Rect::from_center_size(screen(centre), egui::vec2(9.0, 9.0));
    painter.rect_filled(r, 4.5, Color32::WHITE);
    painter.rect_stroke(r, 4.5, Stroke::new(1.5, colour), egui::StrokeKind::Inside);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_row_sends_radius_and_direction() {
        let mut spec = TextSpec::new("A", "lato-bold", 10.0);
        assert_eq!(
            arc_params(&spec, true, 40.0, true),
            Some(json!({"arc": 40.0}))
        );
        assert_eq!(arc_params(&spec, false, 40.0, true), None);
        spec.arc = Some(40.0);
        assert_eq!(
            arc_params(&spec, true, 40.0, false),
            Some(json!({"arc": -40.0}))
        );
        assert_eq!(
            arc_params(&spec, true, 55.0, true),
            Some(json!({"arc": 55.0}))
        );
        assert_eq!(arc_params(&spec, true, 40.0, true), None);
        assert_eq!(
            arc_params(&spec, false, 40.0, true),
            Some(json!({"arc": null}))
        );
    }

    #[test]
    fn dragging_the_centre_sets_the_radius_and_side() {
        let mut spec = TextSpec::new("A", "lato-bold", 10.0);
        spec.arc = Some(40.0);
        spec.transform = openstitch_design::Transform::translation(500.0, 500.0);
        assert!((arc_for_centre(&spec, v2(510.0, 800.0)) - 30.0).abs() < 1e-9);
        assert!((arc_for_centre(&spec, v2(500.0, 100.0)) + 40.0).abs() < 1e-9);
        // Never tighter than the smallest ring.
        assert!((arc_for_centre(&spec, v2(500.0, 501.0)) - 5.0).abs() < 1e-9);
    }
}
