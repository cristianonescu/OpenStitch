//! Text ▸ Ink/Stitch Fonts… (#529, beyond PE-DESIGN): the user's folder of Ink/Stitch-format
//! embroidery fonts (kept in the preferences), listed by `fonts.scanFolder` (Rescan), and
//! loaded into the design by `fonts.importInkStitch` (the design keeps its copy).

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use serde_json::{Value, json};

/// The window's state: open, and the last scan's rows.
#[derive(Clone, Debug, Default)]
pub struct InkStitchFonts {
    pub open: bool,
    pub rows: Vec<Value>,
    pub scanned: bool,
}

/// The `fonts.importInkStitch` request for font folder `path`.
pub fn import_request(path: &std::path::Path) -> CommandRequest {
    CommandRequest::new(
        "fonts.importInkStitch",
        json!({ "path": path.display().to_string() }),
    )
}

/// A one-line summary of an import result for the status bar.
pub fn summary(r: &Value) -> String {
    let mut s = format!(
        "Loaded {} ({} glyphs, sewn as {}); it is saved with the design",
        r["family"].as_str().unwrap_or_default(),
        r["glyphs"],
        r["sewing"].as_str().unwrap_or("satin")
    );
    let not_read = r["notRead"].as_object().map_or(0, |m| m.len());
    if not_read > 0 {
        s.push_str(&format!(
            "; {not_read} kinds of Ink/Stitch settings are not read"
        ));
    }
    if let Some(l) = r["license"].as_str() {
        s.push_str(&format!(
            "; licence: {}",
            l.lines().next().unwrap_or_default()
        ));
    }
    s
}

fn import(app: &mut EditorApp, path: &std::path::Path) {
    if let Some(r) = app.execute(&import_request(path)) {
        app.status = summary(&r);
    }
}

fn rescan(app: &mut EditorApp) {
    let Some(dir) = app.ui.prefs.inkstitch_folder.clone() else {
        return;
    };
    app.ui.inkstitch.rows = app
        .session
        .execute(
            "fonts.scanFolder",
            Some(&json!({ "path": dir.display().to_string() })),
        )
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    app.ui.inkstitch.scanned = true;
}

/// Text tools menu entries.
pub fn menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    if ui.button(t("ui-ink-stitch-fonts")).clicked() {
        app.ui.inkstitch.open = true;
        rescan(app);
    }
    if ui.button(t("ui-import-ink-stitch-font-folder")).clicked()
        && let Some(dir) = crate::files::FileDialog::new().pick_folder()
    {
        import(app, &dir);
    }
}

pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    if !app.ui.inkstitch.open {
        return;
    }
    let mut open = true;
    let mut pick: Option<std::path::PathBuf> = None;
    let (mut choose, mut scan) = (false, false);
    let folder = app.ui.prefs.inkstitch_folder.clone();
    let rows = app.ui.inkstitch.rows.clone();
    app.dialog_window(ctx, "Ink/Stitch Fonts", None)
        .open(&mut open)
        .default_width(380.0)
        .show(ctx, |ui| {
            ui.weak("Embroidery fonts you installed in Ink/Stitch's folder format. None are bundled; check each font's licence.");
            ui.horizontal(|ui| {
                ui.label(folder.as_ref().map_or("No folder chosen".into(), |p| p.display().to_string()));
                choose = ui.button(t("ui-choose")).clicked();
                scan = ui.add_enabled(folder.is_some(), egui::Button::new(t("ui-rescan"))).clicked();
            });
            ui.separator();
            if rows.is_empty() {
                ui.weak(if app.ui.inkstitch.scanned {
                    "No font folders (a folder with font.json) here."
                } else {
                    "Choose the folder that holds your font folders."
                });
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for r in &rows {
                    ui.horizontal(|ui| {
                        let name = r["name"].as_str().filter(|s| !s.is_empty());
                        let folder_name = r["folder"].as_str().unwrap_or_default();
                        ui.label(name.unwrap_or(folder_name));
                        if let Some(l) = r["license"].as_str() {
                            ui.weak(l);
                        }
                        if ui
                            .add_enabled(r["variant"] == true, egui::Button::new(t("ui-import")))
                            .clicked()
                            && let Some(dir) = &folder
                        {
                            pick = Some(dir.join(folder_name));
                        }
                    });
                }
            });
        });
    if choose && let Some(dir) = crate::files::FileDialog::new().pick_folder() {
        app.ui.prefs.inkstitch_folder = Some(dir);
        app.ui.save_prefs();
        scan = true;
    }
    if scan {
        rescan(app);
    }
    if let Some(dir) = pick {
        import(app, &dir);
    }
    if !open {
        app.ui.inkstitch.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_names_the_licence_and_what_was_not_read() {
        let s = summary(&json!({
            "family": "Tiny", "glyphs": 5, "sewing": "satin",
            "notRead": { "angle": 3, "zigzag_spacing_mm": 2 },
            "license": "SIL Open Font License 1.1\nmore",
        }));
        assert!(s.contains("Tiny") && s.contains("5 glyphs"), "{s}");
        assert!(
            s.contains("2 kinds") && s.contains("SIL Open Font License 1.1"),
            "{s}"
        );
        assert!(!s.contains("more"));
        let r = import_request(std::path::Path::new("fonts/tiny"));
        assert_eq!(r.command, "fonts.importInkStitch");
    }
}
