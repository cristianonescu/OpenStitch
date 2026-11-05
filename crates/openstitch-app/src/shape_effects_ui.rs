//! Shapes ▸ Edit Outline ▸ Shape Effects dropdown with a live preview, and the Rectangle tool's
//! per-corner Edge radius (#497). Requests come from `openstitch_app::editor::shape_effects`.

use crate::app::EditorApp;
use eframe::egui::{self, Color32, Pos2, Stroke};
use openstitch_app::editor::shape_effects::{self, EffectForm};
use openstitch_design::effects::Effect;
use openstitch_design::{Contour, Rect};

pub fn can_apply(app: &EditorApp) -> bool {
    !shape_effects::targets(app.session.design(), &app.selection).is_empty()
}

/// The selected outlines, before and after the effect, drawn small.
fn preview(ui: &mut egui::Ui, before: &[Contour], after: &[Contour]) {
    let (resp, painter) = ui.allocate_painter(egui::vec2(220.0, 140.0), egui::Sense::hover());
    let r = resp.rect;
    painter.rect_filled(r, 4.0, ui.visuals().extreme_bg_color);
    let all = before.iter().chain(after).flat_map(|c| c.outline_points());
    let Some(b) = Rect::enclosing(all) else {
        return;
    };
    let scale = ((r.width() - 16.0) as f64 / b.width().max(1e-9))
        .min((r.height() - 16.0) as f64 / b.height().max(1e-9));
    let to = |p: openstitch_design::Vec2| {
        Pos2::new(
            r.center().x + ((p.x - b.center().x) * scale) as f32,
            r.center().y + ((p.y - b.center().y) * scale) as f32,
        )
    };
    let draw = |cs: &[Contour], stroke: Stroke| {
        for c in cs {
            let pts: Vec<Pos2> = c.path().into_iter().map(to).collect();
            painter.add(egui::Shape::line(pts, stroke));
        }
    };
    draw(before, Stroke::new(1.0, Color32::from_gray(140)));
    draw(after, Stroke::new(1.5, ui.visuals().selection.stroke.color));
}

/// The dropdown: effect, amount, size, seed, a preview of the selection and Apply.
pub fn effects_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    let ids = shape_effects::targets(app.session.design(), &app.selection);
    let mut form: EffectForm = app.edit.effect.clone();
    egui::ComboBox::from_label("Effect")
        .selected_text(form.effect.label())
        .show_ui(ui, |ui| {
            for e in Effect::ALL {
                if ui.selectable_label(form.effect == e, e.label()).clicked() {
                    form.set_effect(e);
                }
            }
        });
    let (unit, _) = form.effect.amount();
    let range = match form.effect {
        Effect::Twist => -720.0..=720.0,
        Effect::PuckerBloat => -100.0..=100.0,
        Effect::Warp => -50.0..=50.0,
        _ => 0.0..=50.0,
    };
    ui.horizontal(|ui| {
        ui.label("Amount");
        ui.add(
            egui::DragValue::new(&mut form.amount)
                .range(range)
                .speed(0.1)
                .suffix(format!(" {unit}")),
        );
    });
    if matches!(form.effect, Effect::Roughen | Effect::Zigzag) {
        ui.horizontal(|ui| {
            ui.label("Size");
            ui.add(
                egui::DragValue::new(&mut form.size)
                    .range(0.2..=50.0)
                    .speed(0.1)
                    .suffix(" mm"),
            );
        });
    }
    if form.effect == Effect::Roughen {
        ui.horizontal(|ui| {
            ui.label(format!("Seed {}", form.seed));
            if ui.button("Shuffle").clicked() {
                form.seed = form.seed.wrapping_mul(6364136223846793005).wrapping_add(1) % 1_000_000;
            }
        });
    }
    let before: Vec<Contour> = {
        let d = app.session.design();
        ids.iter()
            .filter_map(|&id| d.index_of(id))
            .flat_map(|i| d.objects[i].contours.clone())
            .collect()
    };
    if !before.is_empty() {
        let after = form.preview(&before);
        preview(ui, &before, &after);
    }
    if ui
        .add_enabled(!ids.is_empty(), egui::Button::new("Apply"))
        .clicked()
    {
        if app.execute(&form.request(&ids)).is_some() {
            app.status = format!("{} applied", form.effect.label());
        }
        ui.close();
    }
    app.edit.effect = form;
}

/// Shapes (Rectangle) ▸ Edge radius: one radius per corner instead of one for all.
pub fn corner_radii_ui(app: &mut EditorApp, ui: &mut egui::Ui) {
    let mut per_corner = app.tools.corner_radii().is_some();
    ui.checkbox(&mut per_corner, "Different radius per corner");
    if !per_corner {
        app.tools.set_corner_radii(None);
        return;
    }
    let r0 = app.tools.corner_radius();
    let mut r = app.tools.corner_radii().unwrap_or([r0; 4]);
    for (label, v) in ["Top left", "Top right", "Bottom right", "Bottom left"]
        .into_iter()
        .zip(r.iter_mut())
    {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(
                egui::DragValue::new(v)
                    .range(0.0..=100.0)
                    .speed(0.1)
                    .suffix(" mm"),
            );
        });
    }
    app.tools.set_corner_radii(Some(r));
}
