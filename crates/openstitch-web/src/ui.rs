//! The web editor's window: File (Open, Save, Export), Undo/Redo, the Design Page with the
//! stitches, and the stitch simulator. Every button runs the command named in its tooltip,
//! the same command `openstitch.run` takes.

use crate::web::{download, pick_and_open, with_doc};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};

pub struct Editor {
    format: String,
    playing: bool,
    /// Stitch records per second while playing.
    speed: f32,
    carry: f32,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            format: "pes".into(),
            playing: false,
            speed: 400.0,
            carry: 0.0,
        }
    }
}

impl eframe::App for Editor {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("file").show(ui, |ui| self.file_bar(ui));
        egui::Panel::bottom("simulator").show(ui, |ui| self.simulator(ui));
        egui::CentralPanel::default().show(ui, canvas);
    }
}

impl Editor {
    fn file_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Open…")
                .on_hover_text("Open a design (.osd) or an embroidery file (ui.openFile)")
                .clicked()
            {
                pick_and_open();
            }
            if ui
                .button("Save")
                .on_hover_text("Download the editable design as .osd (project.save)")
                .clicked()
            {
                with_doc(|d| match d.save() {
                    Ok(f) => {
                        if let Err(e) = download(&[f]) {
                            d.status = e;
                        }
                    }
                    Err(e) => d.status = e,
                });
            }
            ui.separator();
            egui::ComboBox::from_id_salt("format")
                .selected_text(self.format.to_uppercase())
                .show_ui(ui, |ui| {
                    for f in crate::session::export_formats() {
                        ui.selectable_value(&mut self.format, f.to_string(), f.to_uppercase());
                    }
                })
                .response
                .on_hover_text("Export format");
            if ui
                .button("Export")
                .on_hover_text("Download the design in this format (design.export)")
                .clicked()
            {
                let format = self.format.clone();
                with_doc(|d| match d.export(&format) {
                    Ok(files) => {
                        if let Err(e) = download(&files) {
                            d.status = e;
                        }
                    }
                    Err(e) => d.status = e,
                });
            }
            ui.separator();
            let (undo, redo) = with_doc(|d| (d.can_undo(), d.can_redo()));
            if ui
                .add_enabled(undo, egui::Button::new("Undo"))
                .on_hover_text("history.undo")
                .clicked()
            {
                with_doc(|d| d.history(false).err().map(|e| d.status = e));
            }
            if ui
                .add_enabled(redo, egui::Button::new("Redo"))
                .on_hover_text("history.redo")
                .clicked()
            {
                with_doc(|d| d.history(true).err().map(|e| d.status = e));
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Whole design:");
            let edits = [
                (
                    "Rotate 90°",
                    "object.rotate {degrees: 90}",
                    "object.rotate",
                    serde_json::json!({ "degrees": 90 }),
                ),
                (
                    "Flip horizontally",
                    "object.flip {axis: horizontal}",
                    "object.flip",
                    serde_json::json!({ "axis": "horizontal" }),
                ),
                (
                    "Flip vertically",
                    "object.flip {axis: vertical}",
                    "object.flip",
                    serde_json::json!({ "axis": "vertical" }),
                ),
                (
                    "Centre on the page",
                    "object.center",
                    "object.center",
                    serde_json::json!({}),
                ),
            ];
            for (label, help, id, params) in edits {
                if ui.button(label).on_hover_text(help).clicked() {
                    with_doc(|d| d.edit_all(id, params).err().map(|e| d.status = e));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            let (status, title) = with_doc(|d| {
                let (w, h) = d.page_mm();
                let p = d.pattern();
                (
                    d.status.clone(),
                    format!(
                        "{} · {w:.0} × {h:.0} mm · {} stitches · {} colours",
                        d.session().design().name,
                        p.count(openstitch_core::Command::Stitch),
                        p.color_block_count()
                    ),
                )
            });
            ui.strong(title);
            ui.label(status);
        });
        ui.collapsing("What the web version leaves out", |ui| {
            ui.label(crate::session::DESKTOP_ONLY);
        });
    }

    fn simulator(&mut self, ui: &mut egui::Ui) {
        let (mut shown, total) = with_doc(|d| d.simulated());
        if self.playing {
            self.carry += self.speed * ui.input(|i| i.stable_dt).min(0.1);
            let step = self.carry as usize;
            self.carry -= step as f32;
            shown = (shown + step).min(total);
            if shown >= total {
                self.playing = false;
            }
            with_doc(|d| d.simulate(shown));
            ui.ctx().request_repaint();
        }
        ui.horizontal(|ui| {
            ui.label("Stitch simulator");
            let play = if self.playing { "Pause" } else { "Play" };
            if ui.button(play).on_hover_text("ui.simulate").clicked() {
                if !self.playing && shown >= total {
                    with_doc(|d| d.simulate(0));
                }
                self.playing = !self.playing;
            }
            if ui
                .button("Next colour")
                .on_hover_text("ui.simulate {step: \"block\"}")
                .clicked()
            {
                with_doc(|d| d.next_block());
            }
            let mut at = shown;
            if ui
                .add(egui::Slider::new(&mut at, 0..=total).text("stitch"))
                .on_hover_text("ui.simulate {stitch}")
                .changed()
            {
                self.playing = false;
                with_doc(|d| d.simulate(at));
            }
            ui.add(
                egui::Slider::new(&mut self.speed, 50.0..=5000.0)
                    .logarithmic(true)
                    .text("per second"),
            );
        });
    }
}

/// The Design Page, fitted to the panel, with the stitches the simulator has reached.
fn canvas(ui: &mut egui::Ui) {
    let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
    let area = response.rect.shrink(12.0);
    let (segments, page) = with_doc(|d| {
        let (shown, _) = d.simulated();
        let (w, h) = d.page_mm();
        (
            d.segments(shown),
            Vec2::new(w as f32 * 10.0, h as f32 * 10.0),
        )
    });
    if page.x <= 0.0 || page.y <= 0.0 || area.width() <= 0.0 || area.height() <= 0.0 {
        return;
    }
    let scale = (area.width() / page.x).min(area.height() / page.y);
    let origin = area.center() - page * scale / 2.0;
    let at = |p: [f32; 2]| Pos2::new(origin.x + p[0] * scale, origin.y + p[1] * scale);
    let sheet = Rect::from_min_size(origin, page * scale);
    painter.rect_filled(sheet, 0.0, Color32::WHITE);
    painter.rect_stroke(
        sheet,
        0.0,
        Stroke::new(1.0, Color32::GRAY),
        egui::StrokeKind::Outside,
    );
    let width = (scale * 3.0).clamp(0.5, 3.0);
    for s in segments {
        let colour = Color32::from_rgb(s.rgb[0], s.rgb[1], s.rgb[2]);
        painter.line_segment([at(s.from), at(s.to)], Stroke::new(width, colour));
    }
}
