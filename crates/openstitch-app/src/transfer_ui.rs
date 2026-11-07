//! File ▸ Send to USB Media: pick a removable volume, a format and a file name; the engine's
//! `transfer.send` writes the file to the volume root. A name that already exists asks before
//! it is replaced, as PE-DESIGN does.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use openstitch_formats::FileType;
use serde_json::{Value, json};

#[derive(Default)]
pub struct UsbDialog {
    pub open: bool,
    volumes: Vec<(String, String)>,
    selected: usize,
    format: String,
    name: String,
    /// The last send failed because the file exists.
    exists: Option<String>,
    message: Option<String>,
}

impl UsbDialog {
    fn refresh(&mut self, app_volumes: Option<Value>) {
        self.volumes = app_volumes
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| {
                Some((
                    v["path"].as_str()?.to_string(),
                    v["name"].as_str()?.to_string(),
                ))
            })
            .collect();
        self.selected = 0;
    }
}

/// Opens the dialog with the current volumes and the design name.
pub fn open(app: &mut EditorApp) {
    let volumes = app.execute(&CommandRequest::new("transfer.volumes", json!({})));
    let name = app.session.design().name.clone();
    let d = &mut app.workspace.usb;
    d.refresh(volumes);
    d.open = true;
    d.name = name;
    if d.format.is_empty() {
        d.format = "pes".into();
    }
    d.exists = None;
    d.message = None;
}

pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    if !app.workspace.usb.open {
        return;
    }
    let mut open = true;
    let mut action: Option<bool> = None; // Some(overwrite)
    let mut refresh = false;
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-send-to-usb-media")).id(egui::Id::unique("window-send-to-usb-media"))
        .open(&mut open)
        .resizable(false)
        .default_width(360.0)
        .show(ctx, |ui| {
            let d = &mut app.workspace.usb;
            if d.volumes.is_empty() {
                crate::art::header(ui, crate::art::Art::NoMedia, |ui| {
                    ui.label(t("ui-no-usb-stick-or-sd-card"));
                });
            } else {
                for (i, (path, name)) in d.volumes.iter().enumerate() {
                    ui.radio_value(&mut d.selected, i, format!("{name}   {path}"));
                }
            }
            if ui.button(t("ui-refresh")).clicked() {
                refresh = true;
            }
            ui.separator();
            egui::Grid::new("usb").num_columns(2).show(ui, |ui| {
                ui.label(t("ui-format"));
                let r = egui::ComboBox::from_id_salt("usb-format")
                    .selected_text(d.format.to_uppercase())
                    .show_ui(ui, |ui| {
                        for t in FileType::writable() {
                            ui.selectable_value(
                                &mut d.format,
                                t.extension().to_string(),
                                t.extension().to_uppercase(),
                            );
                        }
                    });
                crate::a11y::name(&r.response, "Format");
                ui.end_row();
                ui.label(t("ui-file-name"));
                crate::a11y::name(&ui.text_edit_singleline(&mut d.name), "File name");
                ui.end_row();
            });
            ui.separator();
            if let Some(path) = &d.exists {
                ui.label(format!("{path} already exists."));
                ui.horizontal(|ui| {
                    if ui.button(t("ui-overwrite")).clicked() {
                        action = Some(true);
                    }
                    if ui.button(t("ui-cancel")).clicked() {
                        d.exists = None;
                    }
                });
            } else if ui
                .add_enabled(!d.volumes.is_empty(), egui::Button::new(t("ui-send")))
                .clicked()
            {
                action = Some(false);
            }
            if let Some(m) = &d.message {
                ui.label(egui::RichText::new(m).weak());
            }
        });
    if refresh {
        let v = app.execute(&CommandRequest::new("transfer.volumes", json!({})));
        app.workspace.usb.refresh(v);
    }
    if let Some(overwrite) = action {
        let d = &app.workspace.usb;
        let Some((volume, _)) = d.volumes.get(d.selected).cloned() else {
            return;
        };
        let params = json!({
            "volume": volume, "format": d.format, "name": d.name, "overwrite": overwrite,
        });
        let result = app.execute(&CommandRequest::new("transfer.send", params));
        let d = &mut app.workspace.usb;
        match result {
            Some(r) => {
                d.exists = None;
                d.message = Some(format!("Finished: {}", r["path"].as_str().unwrap_or("")));
                app.status = openstitch_app::editor::ui_texts::t("ui-status-sent-to-usb-media").into();
            }
            None if app.status.contains("exists") => {
                d.exists = Some(format!("{}.{}", d.name, d.format));
            }
            None => d.message = Some(app.status.clone()),
        }
    }
    if !open {
        app.workspace.usb.open = false;
    }
}
