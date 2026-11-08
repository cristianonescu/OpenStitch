//! Send ▸ Test Swatches… (beyond PE-DESIGN, `batch:mac-sewout`): sew a test before the real
//! thing. Three kinds, each one engine command that writes a new design file and leaves the open
//! design alone:
//!
//! - the selection alone in the smallest hoop it fits (`design.testSwatch`, #452);
//! - one selected object copied with a setting stepped, or two (`design.testSwatches`, #507);
//! - a fabric calibration card (`fabric.calibrationSwatch`), and the card's best pieces saved as
//!   a custom fabric (`fabric.calibrate`, #453).
//!
//! The dialog keeps its state in egui's memory. After a swatch is saved it offers to open it.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Selection,
    Steps,
    Card,
}

#[derive(Clone, Debug)]
struct SwatchDialog {
    open: bool,
    kind: Kind,
    label: bool,
    // Parameter swatches.
    attribute: String,
    start: f64,
    step: f64,
    count: u32,
    second: bool,
    attribute2: String,
    start2: f64,
    step2: f64,
    count2: u32,
    gap: f64,
    // Calibration card.
    fabric: String,
    fill: u32,
    satin: u32,
    pull: u32,
    name: String,
    /// The last swatch saved, offered to open.
    saved: Option<PathBuf>,
}

impl Default for SwatchDialog {
    fn default() -> Self {
        SwatchDialog {
            open: false,
            kind: Kind::Selection,
            label: true,
            attribute: "density".into(),
            start: 3.0,
            step: 0.5,
            count: 5,
            second: false,
            attribute2: "pullCompensation".into(),
            start2: 0.0,
            step2: 0.2,
            count2: 3,
            gap: 4.0,
            fabric: "standard".into(),
            fill: 3,
            satin: 3,
            pull: 3,
            name: String::new(),
            saved: None,
        }
    }
}

fn id() -> egui::Id {
    egui::Id::unique("swatch-dialog")
}

pub fn open(ctx: &egui::Context) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<SwatchDialog>(id()).open = true);
}

/// The command and its parameters (all but `path`) for the dialog's current kind, or why it
/// cannot run with this selection.
fn request(
    d: &SwatchDialog,
    selection: &[u64],
) -> Result<(&'static str, serde_json::Map<String, Value>), String> {
    let mut p = serde_json::Map::new();
    let command = match d.kind {
        Kind::Selection => {
            if selection.is_empty() {
                return Err("Select the objects to sew as a test.".into());
            }
            p.insert("ids".into(), json!(selection));
            p.insert("label".into(), json!(d.label));
            "design.testSwatch"
        }
        Kind::Steps => {
            let [one] = selection else {
                return Err("Select one object to vary.".into());
            };
            p.insert("id".into(), json!(one));
            p.insert("attribute".into(), json!(d.attribute));
            p.insert("start".into(), json!(d.start));
            p.insert("step".into(), json!(d.step));
            p.insert("count".into(), json!(d.count));
            if d.second {
                p.insert("attribute2".into(), json!(d.attribute2));
                p.insert("start2".into(), json!(d.start2));
                p.insert("step2".into(), json!(d.step2));
                p.insert("count2".into(), json!(d.count2));
            }
            p.insert("gap".into(), json!(d.gap));
            p.insert("label".into(), json!(d.label));
            "design.testSwatches"
        }
        Kind::Card => {
            p.insert("fabric".into(), json!(d.fabric));
            "fabric.calibrationSwatch"
        }
    };
    Ok((command, p))
}

/// The `fabric.calibrate` parameters for the card's chosen rows.
fn calibrate_params(d: &SwatchDialog) -> Value {
    json!({
        "fabric": d.fabric, "name": d.name.trim(),
        "fill": d.fill, "satin": d.satin, "pull": d.pull,
    })
}

fn attribute_combo(ui: &mut egui::Ui, salt: &str, value: &mut String, valid: &[&'static str]) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(value.as_str())
        .show_ui(ui, |ui| {
            for a in valid {
                ui.selectable_value(value, (*a).to_string(), *a);
            }
        });
}

fn steps_row(ui: &mut egui::Ui, start: &mut f64, step: &mut f64, count: &mut u32) {
    ui.horizontal(|ui| {
        ui.label(t("ui-start-tip"));
        ui.add(egui::DragValue::new(start).speed(0.05).max_decimals(3));
        ui.label(t("ui-step"));
        ui.add(egui::DragValue::new(step).speed(0.05).max_decimals(3));
        ui.label(t("ui-copies"));
        ui.add(egui::DragValue::new(count).range(2..=25));
    });
}

fn row_pick(ui: &mut egui::Ui, label: &str, row: char, value: &mut u32) {
    ui.horizontal(|ui| {
        ui.label(label);
        for k in 1..=5 {
            ui.selectable_value(value, k, format!("{row}{k}"));
        }
    });
}

pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    if std::mem::take(&mut app.ui.open_swatches) {
        open(ctx);
    }
    let mut d: SwatchDialog =
        ctx.data_mut(|m| m.get_temp_mut_or_default::<SwatchDialog>(id()).clone());
    if !d.open {
        return;
    }
    let selection: Vec<u64> = app.selection.iter().copied().collect();
    // The settings the one selected object's sewing uses.
    let valid: Vec<&'static str> = match selection.as_slice() {
        [one] => app
            .session
            .design()
            .index_of(*one)
            .map(|i| openstitch_engine::swatch::attributes_for(&app.session.design().objects[i]))
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let fabrics = app
        .execute(&CommandRequest::new("fabric.list", json!({})))
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let mut save = false;
    let mut calibrate = false;
    let mut open_saved = false;
    let mut open = true;
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-test-swatches")).id(egui::Id::unique("window-test-swatches"))
        .open(&mut open)
        .resizable(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            ui.radio_value(
                &mut d.kind,
                Kind::Selection,
                "Selection alone, in the smallest hoop",
            );
            ui.radio_value(
                &mut d.kind,
                Kind::Steps,
                "One object with a setting stepped",
            );
            ui.radio_value(&mut d.kind, Kind::Card, t("ui-fabric-calibration-card"));
            ui.separator();
            match d.kind {
                Kind::Selection => {
                    ui.checkbox(&mut d.label, t("ui-sew-its-settings-as-a-label"));
                }
                Kind::Steps => {
                    if valid.is_empty() {
                        ui.label(t("ui-select-one-shape-or-text-object"));
                    } else {
                        if !valid.contains(&d.attribute.as_str()) {
                            d.attribute = valid[0].to_string();
                        }
                        ui.horizontal(|ui| {
                            ui.label(t("ui-setting"));
                            attribute_combo(ui, "swatch-attr", &mut d.attribute, &valid);
                        });
                        steps_row(ui, &mut d.start, &mut d.step, &mut d.count);
                        ui.checkbox(&mut d.second, t("ui-and-a-second-setting-across-the"));
                        if d.second {
                            ui.horizontal(|ui| {
                                ui.label(t("ui-setting"));
                                attribute_combo(ui, "swatch-attr2", &mut d.attribute2, &valid);
                            });
                            steps_row(ui, &mut d.start2, &mut d.step2, &mut d.count2);
                        }
                        ui.horizontal(|ui| {
                            ui.label(t("ui-gap"));
                            ui.add(
                                egui::DragValue::new(&mut d.gap)
                                    .range(1.0..=50.0)
                                    .speed(0.5)
                                    .suffix(" mm"),
                            );
                        });
                        ui.checkbox(&mut d.label, t("ui-sew-each-value-as-a-label"));
                    }
                }
                Kind::Card => {
                    let name_of = |id: &str| {
                        fabrics
                            .iter()
                            .find(|f| f["id"] == id)
                            .and_then(|f| f["name"].as_str())
                            .unwrap_or(id)
                            .to_string()
                    };
                    egui::ComboBox::from_label("Fabric")
                        .selected_text(name_of(&d.fabric))
                        .show_ui(ui, |ui| {
                            for f in &fabrics {
                                let id = f["id"].as_str().unwrap_or_default().to_string();
                                let name = f["name"].as_str().unwrap_or_default();
                                ui.selectable_value(&mut d.fabric, id, name);
                            }
                        });
                    ui.label(
                        egui::RichText::new(
                            "F1–F5: fill density 0.8–1.2 ×, S1–S5: satin density 0.8–1.2 ×, \
                             P1–P5: pull compensation 0–0.6 mm, and a 10 mm square to measure.",
                        )
                        .weak()
                        .small(),
                    );
                }
            }
            if ui.button(t("ui-save-swatch")).clicked() {
                save = true;
            }
            if d.kind == Kind::Card {
                ui.separator();
                ui.label(t("ui-after-sewing-it-the-best-piece"));
                row_pick(ui, "Fill", 'F', &mut d.fill);
                row_pick(ui, "Satin", 'S', &mut d.satin);
                row_pick(ui, "Pull", 'P', &mut d.pull);
                ui.horizontal(|ui| {
                    ui.label(t("ui-name"));
                    ui.text_edit_singleline(&mut d.name);
                });
                if ui
                    .add_enabled(
                        !d.name.trim().is_empty(),
                        egui::Button::new(t("ui-save-as-fabric")),
                    )
                    .on_hover_text(t("ui-a-custom-fabric-in-the-fabric-tip"))
                    .clicked()
                {
                    calibrate = true;
                }
            }
            if let Some(p) = &d.saved {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(format!("Saved {}", p.display()));
                    if ui
                        .button(t("ui-open-it"))
                        .on_hover_text(
                            "Opens the swatch in place of this design (Undo brings it back)",
                        )
                        .clicked()
                    {
                        open_saved = true;
                    }
                });
            }
        });
    d.open = open;
    if save {
        match request(&d, &selection) {
            Err(why) => app.status = why,
            Ok((command, mut params)) => {
                let name = app.session.design().name.clone();
                if let Some(path) = crate::files::FileDialog::new()
                    .add_filter("Brother PES", &["pes"])
                    .add_filter(
                        "OpenStitch design",
                        &[openstitch_engine::project::EXTENSION],
                    )
                    .set_file_name(format!("{name} swatch.pes"))
                    .save_file()
                {
                    params.insert("path".into(), json!(path.display().to_string()));
                    if app
                        .execute(&CommandRequest::new(command, Value::Object(params)))
                        .is_some()
                    {
                        app.status = openstitch_app::tf!("ui-status-saved-the-test-swatch", path = path.display());
                        d.saved = Some(path);
                    }
                }
            }
        }
    }
    if calibrate
        && let Some(r) = app.execute(&CommandRequest::new(
            "fabric.calibrate",
            calibrate_params(&d),
        ))
    {
        app.status = openstitch_app::tf!("ui-status-saved-the-fabric", fabric = r["fabric"]["name"].as_str().unwrap_or(""));
    }
    if open_saved && let Some(p) = d.saved.take() {
        app.open_path(&p);
    }
    ctx.data_mut(|m| m.insert_temp(id(), d));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_asks_its_command_with_the_dialog_settings() {
        let mut d = SwatchDialog::default();
        assert!(request(&d, &[]).is_err());
        let (c, p) = request(&d, &[3, 4]).expect("selection");
        assert_eq!(c, "design.testSwatch");
        assert_eq!(
            (p["ids"].clone(), p["label"].clone()),
            (json!([3, 4]), json!(true))
        );
        d.kind = Kind::Steps;
        assert!(request(&d, &[3, 4]).is_err(), "one object only");
        d.second = true;
        let (c, p) = request(&d, &[3]).expect("one object");
        assert_eq!(c, "design.testSwatches");
        assert_eq!(p["attribute2"], json!("pullCompensation"));
        assert_eq!(p["count"], json!(5));
        d.kind = Kind::Card;
        let (c, p) = request(&d, &[]).expect("no selection needed");
        assert_eq!(
            (c, p["fabric"].clone()),
            ("fabric.calibrationSwatch", json!("standard"))
        );
        d.name = "  Denim ".into();
        d.fill = 2;
        assert_eq!(calibrate_params(&d)["name"], json!("Denim"));
        assert_eq!(calibrate_params(&d)["fill"], json!(2));
    }

    /// The Assistant opens the dialog with `ui.run {action: "testSwatches"}`; it shows on the next
    /// frame.
    #[test]
    fn ui_run_opens_the_test_swatches_dialog() {
        let mut app = EditorApp::new(crate::app::Launch {
            demo: true,
            ..crate::app::Launch::default()
        });
        assert!(!app.ui.open_swatches);
        app.ui_command("ui.run", Some(&json!({ "action": "testSwatches" })))
            .expect("runs");
        assert!(app.ui.open_swatches);
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| window(&mut app, ui.ctx()));
        out.textures_delta.clear();
        assert!(!app.ui.open_swatches, "taken by the window");
    }
}
