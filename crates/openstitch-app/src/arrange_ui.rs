//! Home ▸ Edit ▸ Align (#472), Home ▸ Select ▸ Select Same (#475) and the precision bar
//! (#474). Every change goes through an engine command, so each is one undo step and the
//! Assistant reaches the same edits with `object.align`, `object.distribute`, `object.list`
//! (filters), `object.scale`, `object.rotate` and `object.skew`.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::hit::bounds_of;
use openstitch_app::editor::precision::{self, Field, Options};
use openstitch_app::editor::request::CommandRequest;
use openstitch_app::editor::ribbon::{ALIGN_MENU, Action, SELECT_SAME_MENU};
use serde_json::{Value, json};

/// Align ▸ relative to: the three references and their labels.
pub const ALIGN_TO: [(&str, &str); 3] = [
    ("selection", "Selection"),
    ("key", "Last Selected"),
    ("page", "Design Page"),
];

/// Whether an Align dropdown entry can run on the selection.
pub fn can_align(app: &EditorApp, a: Action) -> bool {
    let n = app.selection.len();
    match a {
        Action::DistributeHorizontal | Action::DistributeVertical => n >= 3,
        _ => n >= 2 || (n == 1 && app.align_to == "page"),
    }
}

/// Runs an Align dropdown entry on the selection.
pub fn align(app: &mut EditorApp, a: Action) {
    let Some(edge) = a.align_edge() else { return };
    let ids = CommandRequest::ids(&app.selection);
    let r = match a {
        Action::DistributeHorizontal | Action::DistributeVertical => CommandRequest::new(
            "object.distribute",
            json!({ "ids": ids, "axis": edge, "by": "gaps" }),
        ),
        _ => {
            let mut p = json!({ "ids": ids, "edge": edge, "to": app.align_to });
            if app.align_to == "key"
                && let Some(k) = app.key_object.filter(|k| app.selection.contains(k))
            {
                p["keyId"] = json!(k);
            }
            CommandRequest::new("object.align", p)
        }
    };
    app.execute(&r);
}

/// Selects every visible object like the selection (Select Same ▸ …).
pub fn select_same(app: &mut EditorApp, a: Action) {
    let Some(how) = a.same_match() else { return };
    if app.selection.is_empty() {
        return;
    }
    let r = CommandRequest::new(
        "object.list",
        json!({ "sameAs": CommandRequest::ids(&app.selection), "match": how, "visible": true }),
    );
    if let Some(Value::Array(list)) = app.execute(&r) {
        app.selection = list.iter().filter_map(|o| o["id"].as_u64()).collect();
        app.status = openstitch_app::tf!("ui-status-objects-selected", count = app.selection.len());
    }
}

pub fn align_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new(t("ui-relative-to")).weak());
    for (key, label) in ALIGN_TO {
        ui.radio_value(&mut app.align_to, key, label);
    }
    ui.separator();
    for (i, a) in ALIGN_MENU.into_iter().enumerate() {
        if i == 6 {
            ui.separator();
        }
        crate::ribbon::menu_item(app, ui, a);
    }
}

pub fn select_same_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    for a in SELECT_SAME_MENU {
        crate::ribbon::menu_item(app, ui, a);
    }
}

/// The precision bar's state: the text of the field being typed in.
#[derive(Default)]
pub struct PrecisionState {
    pub options: Options,
    editing: Option<(usize, String)>,
}

/// Commits a typed value: one command group, one undo step.
pub fn commit(app: &mut EditorApp, f: Field, v: f64) {
    let Some(b) = bounds_of(app.session.design(), &app.selection) else {
        return;
    };
    let o = precision::Options {
        zero: app.ruler_zero(),
        ..app.precision.options
    };
    let rs = precision::requests(f, v, &app.selection, b, o);
    if !rs.is_empty() {
        app.execute_all(&rs);
    }
}

/// View ▸ Precision Bar: X, Y, W, H, rotation and skew of the selection, typed exactly.
/// Enter commits, Esc reverts.
pub fn precision_bar(app: &mut EditorApp, ui: &mut egui::Ui) {
    let bounds = bounds_of(app.session.design(), &app.selection);
    ui.horizontal(|ui| {
        let opts = &mut app.precision.options;
        ui.add_enabled_ui(bounds.is_some(), |ui| {
            egui::ComboBox::from_id_salt("precision-ref")
                .selected_text(if opts.centre { "Centre" } else { "Top-left" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut opts.centre, false, "Top-left");
                    ui.selectable_value(&mut opts.centre, true, "Centre");
                })
                .response
                .on_hover_text(t("ui-the-point-x-and-y-give-tip"));
        });
        let Some(b) = bounds else {
            ui.weak("Select objects to type their position, size, rotation or skew");
            return;
        };
        let mut done: Option<(Field, f64)> = None;
        for (i, f) in Field::ALL.into_iter().enumerate() {
            if f == Field::Height {
                let opts = &mut app.precision.options;
                ui.toggle_value(&mut opts.lock, "Lock")
                    .on_hover_text(t("ui-lock-aspect-width-and-height-scale-tip"));
            }
            ui.label(f.label());
            let units = app.ui.prefs.units;
            let o = precision::Options {
                zero: app.ruler_zero(),
                ..app.precision.options
            };
            let shown = precision::shown(f, b, o, units);
            let mut text = match &app.precision.editing {
                Some((k, t)) if *k == i => t.clone(),
                _ => shown,
            };
            let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(56.0));
            ui.weak(f.unit_in(units));
            if r.has_focus() {
                app.precision.editing = Some((i, text.clone()));
            }
            if r.lost_focus() {
                let (enter, esc) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::Enter),
                        i.key_pressed(egui::Key::Escape),
                    )
                });
                if enter
                    && !esc
                    && let Some(v) = precision::parse(f, &text, units)
                {
                    done = Some((f, v));
                }
                app.precision.editing = None;
            }
        }
        if let Some((f, v)) = done {
            commit(app, f, v);
        }
    });
}
