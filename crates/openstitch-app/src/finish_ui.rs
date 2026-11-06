//! Sewing Order pane ▸ Finish (#548, beyond PE-DESIGN): trim after, ties and a text's trims
//! between letters. Every control sends one `object.setFinish`, one undo step.

use crate::app::EditorApp;
use eframe::egui::{self, Color32, Stroke};
use openstitch_app::editor::{CommandRequest, Phase, Viewport};
use openstitch_design::ObjectKind;
use openstitch_design::finish::{Finish, FinishMode, LetterTrim};
use openstitch_design::{Vec2, v2};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const MODES: [(FinishMode, &str); 3] = [
    (FinishMode::Auto, "When far"),
    (FinishMode::Always, "Always"),
    (FinishMode::Off, "Off"),
];

const BETWEEN: [(LetterTrim, &str); 5] = [
    (LetterTrim::Auto, "When far"),
    (LetterTrim::Never, "Never"),
    (LetterTrim::Letter, "After each letter"),
    (LetterTrim::Word, "Between words"),
    (LetterTrim::Line, "Between lines"),
];

/// The `object.setFinish` request setting `key` to `value` on `ids`.
pub fn finish_request(ids: &BTreeSet<u64>, key: &str, value: Value) -> CommandRequest {
    let mut params = json!({ "ids": CommandRequest::ids(ids) });
    params[key] = value;
    CommandRequest::new("object.setFinish", params)
}

/// The Finish section of the Sewing Order pane for the selected objects: the first selected
/// object's settings are shown, a change applies to all of them.
pub fn finish_controls(app: &EditorApp, ui: &mut egui::Ui) -> Option<CommandRequest> {
    let d = app.session.design();
    let selected: Vec<_> = d
        .objects
        .iter()
        .filter(|o| app.selection.contains(&o.id))
        .collect();
    let first = selected.first()?;
    let ids = &app.selection;
    let texts: BTreeSet<u64> = selected
        .iter()
        .filter(|o| o.kind == ObjectKind::Text)
        .map(|o| o.id)
        .collect();
    let f = first.finish.clone().unwrap_or_default();
    let mut out = None;
    egui::CollapsingHeader::new("Finish")
        .id_salt("object-finish")
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Beyond PE-DESIGN; When far is PE-DESIGN's rule").weak(),
            );
            egui::Grid::new("finish-grid").num_columns(2).show(ui, |ui| {
                if let Some(m) = mode_combo(ui, "Trim after", "finish-trim", f.trim, "When far: trim when the next object starts farther than the trim distance. Always: trim however short the move. Off: never trim (the frame jumps; colour changes still cut)")
                {
                    out = Some(finish_request(ids, "trim", json!(m.as_str())));
                }
                ui.end_row();
                ui.label("Trim distance");
                ui.horizontal(|ui| {
                    let mut set = f.trim_distance.is_some();
                    if ui
                        .checkbox(&mut set, "")
                        .on_hover_text("Off: the design's trim distance")
                        .changed()
                    {
                        let v = if set { json!(2.0) } else { Value::Null };
                        out = Some(finish_request(ids, "trimDistance", v));
                    }
                    if let Some(mut mm) = f.trim_distance {
                        let r = ui.add(
                            egui::DragValue::new(&mut mm)
                                .range(0.0..=50.0)
                                .speed(0.1)
                                .suffix(" mm"),
                        );
                        if (r.drag_stopped() || r.lost_focus())
                            && f.trim_distance.is_some_and(|o| (o - mm).abs() > 1e-9)
                        {
                            out = Some(finish_request(ids, "trimDistance", json!(mm)));
                        }
                    }
                });
                ui.end_row();
                if let Some(m) = mode_combo(ui, "Tie in / off", "finish-tie", f.tie, "When far: tie the runs a trim separates (PE-DESIGN). Always: also at the object's start and end. Off: no ties")
                {
                    out = Some(finish_request(ids, "tie", json!(m.as_str())));
                }
                ui.end_row();
                ui.label("Tie stitches");
                ui.horizontal(|ui| {
                    let (count, len) = f.tie_steps();
                    let mut count = count as u8;
                    let mut len = len / 10.0;
                    let a = ui
                        .add(egui::DragValue::new(&mut count).range(1..=5).suffix(" ×"))
                        .on_hover_text("Out-and-back steps of each tie-in and lock");
                    if (a.drag_stopped() || a.lost_focus()) && Some(count) != f.tie_count {
                        out = Some(finish_request(ids, "tieCount", json!(count)));
                    }
                    let b = ui
                        .add(
                            egui::DragValue::new(&mut len)
                                .range(0.2..=2.0)
                                .speed(0.05)
                                .suffix(" mm"),
                        )
                        .on_hover_text("Length of each tie stitch");
                    if (b.drag_stopped() || b.lost_focus())
                        && (len - f.tie_length.unwrap_or(0.3)).abs() > 1e-9
                    {
                        out = Some(finish_request(ids, "tieLength", json!(len)));
                    }
                    if (f.tie_count.is_some() || f.tie_length.is_some())
                        && ui.small_button("PE").on_hover_text("PE-DESIGN's own tie").clicked()
                    {
                        let mut p = json!({ "ids": CommandRequest::ids(ids) });
                        p["tieCount"] = Value::Null;
                        p["tieLength"] = Value::Null;
                        out = Some(CommandRequest::new("object.setFinish", p));
                    }
                });
                ui.end_row();
                if !texts.is_empty() {
                    ui.label("Between letters");
                    let label = BETWEEN
                        .iter()
                        .find(|b| b.0 == f.between)
                        .map_or("When far", |b| b.1);
                    egui::ComboBox::from_id_salt("finish-between")
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for (v, name) in BETWEEN {
                                if ui.selectable_label(v == f.between, name).clicked()
                                    && v != f.between
                                {
                                    out = Some(finish_request(
                                        &texts,
                                        "between",
                                        json!(v.as_str()),
                                    ));
                                }
                            }
                        })
                        .response
                        .on_hover_text("Where a text trims between its letters; letters not trimmed apart are joined by a stitch or a jump");
                    ui.end_row();
                }
            });
            if f != Finish::default() && ui.button("PE-DESIGN's rules").clicked() {
                out = Some(finish_request(ids, "reset", json!(true)));
            }
        });
    out
}

fn mode_combo(
    ui: &mut egui::Ui,
    label: &str,
    salt: &str,
    current: FinishMode,
    hover: &str,
) -> Option<FinishMode> {
    ui.label(label);
    let mut chosen = None;
    let name = MODES.iter().find(|m| m.0 == current).map_or("", |m| m.1);
    egui::ComboBox::from_id_salt(salt)
        .selected_text(name)
        .show_ui(ui, |ui| {
            for (m, n) in MODES {
                if ui.selectable_label(m == current, n).clicked() && m != current {
                    chosen = Some(m);
                }
            }
        })
        .response
        .on_hover_text(hover);
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_requests_name_the_setting() {
        let ids: BTreeSet<u64> = [3, 5].into();
        let r = finish_request(&ids, "trim", json!("off"));
        assert_eq!(r.command, "object.setFinish");
        assert_eq!(r.params["trim"], "off");
        assert_eq!(r.params["ids"], json!([3, 5]));
    }
}
