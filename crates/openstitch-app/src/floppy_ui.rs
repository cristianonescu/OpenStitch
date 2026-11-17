//! Sew Embroidery ▸ Floppy Image… (#876, beyond PE-DESIGN): make or open a 1.44 MB floppy disk
//! image, see its files and free space, add the design (in the format the machine reads) or
//! other files, and remove files. Every change is one `transfer.diskImage` request, which
//! writes the image file only; copying it onto a floppy or a USB floppy emulator stays the
//! user's step. A file whose 8.3 name is already on the disk asks before it is replaced, as Send
//! to USB Media does.

use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use openstitch_app::editor::ui_texts::t;
use openstitch_formats::FileType;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// File extensions the image dialogs offer.
const IMAGE_EXTENSIONS: [&str; 4] = ["img", "ima", "vfd", "flp"];

#[derive(Default)]
pub struct FloppyDialog {
    pub open: bool,
    /// The image file being edited.
    pub path: Option<String>,
    /// `transfer.diskImageList`'s answer for it.
    listing: Option<Value>,
    /// 8.3 names ticked for removal.
    selected: BTreeSet<String>,
    format: String,
    name: String,
    /// A request refused because a name is on the disk: asked again with overwrite.
    replace: Option<(String, Value)>,
    message: Option<String>,
}

/// Opens the dialog (refused in a browser, which cannot write a machine's media).
pub fn open(app: &mut EditorApp) {
    if crate::limits_ui::browser_limit(app, openstitch_app::editor::browser_limits::Limit::Transfer)
    {
        return;
    }
    let name = app.session.design().name.clone();
    let d = &mut app.workspace.floppy;
    d.open = true;
    d.name = name;
    if d.format.is_empty() {
        d.format = "dst".into();
    }
    d.replace = None;
    d.message = None;
    if d.path.is_some() {
        refresh(app);
    }
}

/// Uses the image at `path` (made empty and formatted when it does not exist yet).
pub fn use_image(app: &mut EditorApp, path: String) {
    let exists = std::path::Path::new(&path).exists();
    if !exists
        && app
            .execute(&CommandRequest::new(
                "transfer.diskImage",
                json!({ "path": path }),
            ))
            .is_none()
    {
        app.workspace.floppy.message = Some(app.status.clone());
        return;
    }
    app.workspace.floppy.path = Some(path);
    app.workspace.floppy.selected.clear();
    refresh(app);
}

fn refresh(app: &mut EditorApp) {
    let Some(path) = app.workspace.floppy.path.clone() else {
        return;
    };
    let listing = app.execute(&CommandRequest::new(
        "transfer.diskImageList",
        json!({ "path": path }),
    ));
    let d = &mut app.workspace.floppy;
    if listing.is_none() {
        d.message = Some(app.status.clone());
    }
    d.listing = listing;
}

/// Runs one edit of the image; a name already on the disk asks before replacing.
pub fn change(app: &mut EditorApp, params: Value) {
    let Some(path) = app.workspace.floppy.path.clone() else {
        return;
    };
    let mut params = params;
    params["path"] = json!(path);
    let result = app.execute(&CommandRequest::new("transfer.diskImage", params.clone()));
    let d = &mut app.workspace.floppy;
    match result {
        Some(r) => {
            d.replace = None;
            d.selected.clear();
            let added: Vec<&str> = r["added"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect())
                .unwrap_or_default();
            d.message = Some(if added.is_empty() {
                t("ui-floppy-updated").to_string()
            } else {
                format!("{} {}", t("ui-floppy-added"), added.join(", "))
            });
            app.status = t("ui-floppy-updated").into();
        }
        None if app.status.contains("already on the disk") => {
            d.replace = Some((app.status.clone(), params));
        }
        None => d.message = Some(app.status.clone()),
    }
    refresh(app);
}

fn new_image(app: &mut EditorApp) {
    let name = format!("{}.img", app.session.design().name);
    if let Some(p) = crate::files::FileDialog::new()
        .add_filter(t("ui-floppy-disk-image"), &IMAGE_EXTENSIONS)
        .set_file_name(name)
        .save_file()
    {
        use_image(app, p.display().to_string());
    }
}

fn open_image() {
    crate::files::pick_then(
        crate::files::FileDialog::new().add_filter(t("ui-floppy-disk-image"), &IMAGE_EXTENSIONS),
        |app, p| use_image(app, p.display().to_string()),
    );
}

fn add_files() {
    crate::files::pick_many_then(crate::files::FileDialog::new(), |app, files| {
        let add: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
        change(app, json!({ "add": add }));
    });
}

/// Kilobytes as the disk counts them (1,024 bytes), one decimal.
fn kb(bytes: u64) -> String {
    format!("{:.1} KB", bytes as f64 / 1024.0)
}

enum Act {
    New,
    Open,
    AddDesign,
    AddFiles,
    Remove,
    Replace,
}

pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    if !app.workspace.floppy.open {
        return;
    }
    let mut open = true;
    let mut act = None;
    egui::Window::new(t("ui-window-floppy-image"))
        .id(egui::Id::unique("window-floppy-image"))
        .open(&mut open)
        .resizable(false)
        .default_width(400.0)
        .show(ctx, |ui| {
            let d = &mut app.workspace.floppy;
            ui.label(egui::RichText::new(t("ui-floppy-intro")).weak());
            ui.horizontal(|ui| {
                if ui.button(t("ui-floppy-new")).clicked() {
                    act = Some(Act::New);
                }
                if ui.button(t("ui-floppy-open")).clicked() {
                    act = Some(Act::Open);
                }
            });
            let Some(path) = d.path.clone() else {
                return;
            };
            ui.label(path);
            ui.separator();
            if let Some(l) = &d.listing {
                let files = l["files"].as_array().cloned().unwrap_or_default();
                if files.is_empty() {
                    ui.label(t("ui-floppy-empty"));
                }
                egui::Grid::new("floppy-files").num_columns(2).show(ui, |ui| {
                    for f in &files {
                        let name = f["name"].as_str().unwrap_or("").to_string();
                        let folder = f["folder"].as_bool().unwrap_or(false);
                        let mut on = d.selected.contains(&name);
                        let r = ui.add_enabled(!folder, egui::Checkbox::new(&mut on, &name));
                        crate::a11y::name(&r, &name);
                        if r.changed() {
                            if on {
                                d.selected.insert(name.clone());
                            } else {
                                d.selected.remove(&name);
                            }
                        }
                        ui.label(kb(f["bytes"].as_u64().unwrap_or(0)));
                        ui.end_row();
                    }
                });
                ui.label(format!(
                    "{} {} / {}",
                    t("ui-floppy-free"),
                    kb(l["free"].as_u64().unwrap_or(0)),
                    kb(l["capacity"].as_u64().unwrap_or(0))
                ));
            }
            ui.separator();
            egui::Grid::new("floppy-design").num_columns(2).show(ui, |ui| {
                ui.label(t("ui-format"));
                let r = egui::ComboBox::from_id_salt("floppy-format")
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
            if let Some((why, _)) = &d.replace {
                ui.label(why.as_str());
                ui.horizontal(|ui| {
                    if ui.button(t("ui-overwrite")).clicked() {
                        act = Some(Act::Replace);
                    }
                    if ui.button(t("ui-cancel")).clicked() {
                        d.replace = None;
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    if ui.button(t("ui-floppy-add-design")).clicked() {
                        act = Some(Act::AddDesign);
                    }
                    if ui.button(t("ui-floppy-add-files")).clicked() {
                        act = Some(Act::AddFiles);
                    }
                    if ui
                        .add_enabled(!d.selected.is_empty(), egui::Button::new(t("ui-floppy-remove")))
                        .clicked()
                    {
                        act = Some(Act::Remove);
                    }
                });
            }
            if let Some(m) = &d.message {
                ui.label(egui::RichText::new(m).weak());
            }
        });
    match act {
        Some(Act::New) => new_image(app),
        Some(Act::Open) => open_image(),
        Some(Act::AddDesign) => {
            let d = &app.workspace.floppy;
            let params = json!({ "addDesign": true, "format": d.format, "name": d.name });
            change(app, params);
        }
        Some(Act::AddFiles) => add_files(),
        Some(Act::Remove) => {
            let names: Vec<String> = app.workspace.floppy.selected.iter().cloned().collect();
            change(app, json!({ "remove": names }));
        }
        Some(Act::Replace) => {
            if let Some((_, mut params)) = app.workspace.floppy.replace.take() {
                params["overwrite"] = json!(true);
                change(app, params);
            }
        }
        None => {}
    }
    if !open {
        app.workspace.floppy.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dialog's requests make an image, add the design, ask before replacing it, and
    /// remove it again, each through `transfer.diskImage`.
    #[test]
    fn the_dialog_makes_an_image_adds_the_design_and_asks_before_replacing() {
        let dir = openstitch_testutil::TempDir::new("floppy-ui");
        let mut app = EditorApp::new(crate::app::Launch::default());
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({"x": 0, "y": 0, "width": 20, "height": 20, "fill": "Red"}),
        ));
        open(&mut app);
        assert!(app.workspace.floppy.open);
        assert_eq!(app.workspace.floppy.format, "dst");
        let img = dir.path().join("machine.img").display().to_string();
        use_image(&mut app, img.clone());
        assert_eq!(app.workspace.floppy.path.as_deref(), Some(img.as_str()));
        let files = |app: &EditorApp| {
            app.workspace.floppy.listing.as_ref().expect("listing")["files"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0)
        };
        assert_eq!(files(&app), 0);
        app.workspace.floppy.name = "rose".into();
        change(&mut app, json!({ "addDesign": true, "format": "dst", "name": "rose" }));
        assert_eq!(files(&app), 1);
        // The same name again asks first; replacing it keeps one file.
        change(&mut app, json!({ "addDesign": true, "format": "dst", "name": "rose" }));
        assert!(app.workspace.floppy.replace.is_some());
        let (_, mut params) = app.workspace.floppy.replace.take().expect("asks");
        params["overwrite"] = json!(true);
        change(&mut app, params);
        assert!(app.workspace.floppy.replace.is_none());
        assert_eq!(files(&app), 1);
        change(&mut app, json!({ "remove": ["ROSE.DST"] }));
        assert_eq!(files(&app), 0);
    }

    /// #830: in a browser (pretended here) Floppy Image opens nothing and says why, as Send to
    /// USB Media does.
    #[test]
    fn the_browser_does_not_open_floppy_images() {
        let mut app = EditorApp::new(crate::app::Launch::default());
        openstitch_app::platform::pretend_browser(true);
        open(&mut app);
        openstitch_app::platform::pretend_browser(false);
        assert!(!app.workspace.floppy.open);
        assert_eq!(
            app.ui.limit_notice,
            Some(openstitch_app::editor::browser_limits::Limit::Transfer)
        );
    }
}
