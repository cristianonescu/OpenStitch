//! The Startup Wizard (#657): PE-DESIGN 11's "How do you want to produce embroidery?" dialog,
//! in its layout, for users who want the PE-compatible start-up instead of the start cards.
//! Application menu ▸ Wizard ▸ Startup Wizard opens it; with "Always show Wizard at Startup"
//! ticked (`ui.set startupWizard`, kept in `ui.json`) it also opens on every interactive start.
//!
//! Every choice closes the wizard and runs the matching OpenStitch action ([`CHOICES`]). Where
//! OpenStitch has no exact equivalent (PE's Import pane categories) the nearest action is used,
//! and the mapping is documented in docs/UI_LAYOUT.md. Unlike PE-DESIGN, a choice does not first
//! replace the open design with an empty page: the action asks about unsaved changes itself
//! where it replaces the design (Open, a sample).

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui::{self, RichText};
use openstitch_app::editor::close_prompt::Pending;
use openstitch_app::editor::ribbon::Action;

/// The wizard's choices: key (`ui.startupWizard {choice}`), label, and PE-DESIGN's section.
pub const CHOICES: &[(&str, &str, &str)] = &[
    ("manual", "Open Instruction Manual", ""),
    ("template", "Use Template Design", "Create With Ease"),
    ("images", "Create Embroidery Patterns Using Images", "Create With Ease"),
    ("hoopAndFabric", "Set Hoop Size and Fabric", "Set and Create New Page"),
    ("importText", "Text", "Import Embroidery Patterns"),
    ("importShapes", "Outline Shapes", "Import Embroidery Patterns"),
    ("importLibrary", "Design Library", "Import Embroidery Patterns"),
    ("open", "Open Embroidery Design", ""),
    ("database", "Start Design Database to Manage Embroidery Files", ""),
];

#[derive(Default)]
pub struct StartupWizard {
    pub open: bool,
    /// The recent file chosen in the grid, opened with Open.
    recent: Option<std::path::PathBuf>,
}

/// Whether this start opens the wizard: an interactive start with the setting ticked.
pub fn opens_at_start(
    l: &crate::app::Launch,
    prefs: &openstitch_app::editor::prefs::UiPrefs,
) -> bool {
    l.interactive() && prefs.startup_wizard
}

/// Closes the wizard and runs choice `key`.
pub fn choose(app: &mut EditorApp, key: &str, ctx: &egui::Context) -> Result<(), String> {
    if !CHOICES.iter().any(|(k, _, _)| *k == key) {
        let keys: Vec<_> = CHOICES.iter().map(|c| c.0).collect();
        return Err(format!("choice takes {}", keys.join(", ")));
    }
    app.startup_wizard.open = false;
    match key {
        "manual" => app.ui.start.page_open = true,
        "template" => crate::lettering_ui::open_template_wizard(app),
        "images" => app.wizard.open = true,
        "hoopAndFabric" => {
            // PE-DESIGN: Design Settings, then the Fabric Selector.
            crate::ribbon::run(app, Action::DesignSettings, ctx);
            app.workspace.view.fabric_open = true;
        }
        "importText" => app.lettering.text.open = true,
        "importShapes" => crate::ribbon::run(app, Action::PaneImport, ctx),
        "importLibrary" => app.ui.start.samples_open = true,
        "open" => app.guarded(Pending::Open(None)),
        "database" => crate::ribbon::run(app, Action::DesignDatabase, ctx),
        _ => unreachable!("checked above"),
    }
    Ok(())
}

fn label(key: &str) -> &'static str {
    CHOICES
        .iter()
        .find(|c| c.0 == key)
        .map_or("", |c| c.1)
}

pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    if !app.startup_wizard.open {
        return;
    }
    let mut picked: Option<&'static str> = None;
    let mut open_recent: Option<std::path::PathBuf> = None;
    let mut close = false;
    let recent: Vec<std::path::PathBuf> = app
        .ui
        .prefs
        .recent
        .iter()
        .filter(|p| p.exists())
        .cloned()
        .collect();
    egui::Window::new("How do you want to produce embroidery?")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(250.0);
                    if ui.button(RichText::new(label("manual")).strong()).clicked() {
                        picked = Some("manual");
                    }
                    ui.separator();
                    let mut section = "";
                    for &(key, text, heading) in &CHOICES[1..7] {
                        if heading != section {
                            ui.add_space(4.0);
                            ui.label(RichText::new(heading).strong());
                            section = heading;
                        }
                        if key.starts_with("import") {
                            continue;
                        }
                        if ui.button(text).clicked() {
                            picked = Some(key);
                        }
                    }
                    ui.horizontal(|ui| {
                        for key in ["importText", "importShapes", "importLibrary"] {
                            if ui.button(label(key)).clicked() {
                                picked = Some(key);
                            }
                        }
                    });
                });
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.set_width(230.0);
                    ui.label(RichText::new(t("ui-most-recently-used-files")).strong());
                    egui::ScrollArea::vertical()
                        .max_height(140.0)
                        .show(ui, |ui| {
                            if recent.is_empty() {
                                ui.label(RichText::new(t("ui-no-recent-files")).weak());
                            }
                            for p in &recent {
                                let name = p
                                    .file_name()
                                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                                let chosen = app.startup_wizard.recent.as_ref() == Some(p);
                                if ui
                                    .selectable_label(chosen, name)
                                    .on_hover_text(p.display().to_string())
                                    .clicked()
                                {
                                    app.startup_wizard.recent = Some(p.clone());
                                }
                            }
                        });
                    let can = app.startup_wizard.recent.is_some();
                    if ui.add_enabled(can, egui::Button::new(t("ui-open"))).clicked() {
                        open_recent = app.startup_wizard.recent.clone();
                    }
                    ui.add_space(6.0);
                    for key in ["open", "database"] {
                        if ui.button(label(key)).clicked() {
                            picked = Some(key);
                        }
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .checkbox(
                        &mut app.ui.prefs.startup_wizard,
                        "Always show Wizard at Startup",
                    )
                    .changed()
                {
                    app.ui.save_prefs();
                }
                if ui.button(t("ui-close-this-wizard")).clicked() {
                    close = true;
                }
            });
        });
    if let Some(key) = picked {
        let _ = choose(app, key, ctx);
    }
    if let Some(p) = open_recent {
        app.startup_wizard.open = false;
        app.guarded(Pending::Open(Some(p)));
    }
    if close {
        app.startup_wizard.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;
    use openstitch_app::editor::prefs::UiPrefs;
    use serde_json::json;

    #[test]
    fn the_wizard_opens_at_start_when_ticked_and_each_choice_runs_its_action() {
        let mut prefs = UiPrefs::default();
        let l = Launch::default();
        assert!(!opens_at_start(&l, &prefs), "off by default: the start cards");
        prefs.startup_wizard = true;
        assert!(opens_at_start(&l, &prefs));
        assert!(UiPrefs::from_json(&prefs.to_json()).startup_wizard, "kept in ui.json");
        let shot = Launch {
            screenshot: Some("s.png".into()),
            ..Launch::default()
        };
        assert!(!opens_at_start(&shot, &prefs), "scripted runs never open it");

        let mut app = EditorApp::new(Launch::default());
        let ctx = egui::Context::default();
        let wizard = |app: &mut EditorApp, p: serde_json::Value| {
            app.ui_command("ui.startupWizard", Some(&p))
        };
        wizard(&mut app, json!({})).expect("open");
        assert!(app.startup_wizard.open);
        assert!(wizard(&mut app, json!({ "choice": "sock" })).is_err());
        type Opened = fn(&EditorApp) -> bool;
        let checks: [(&str, Opened); 6] = [
            ("manual", |a| a.ui.start.page_open),
            ("images", |a| a.wizard.open),
            ("hoopAndFabric", |a| a.workspace.view.fabric_open),
            ("importText", |a| a.lettering.text.open),
            ("importLibrary", |a| a.ui.start.samples_open),
            ("importShapes", |a| a.ui.pane_tab == crate::ribbon::PaneTab::Import),
        ];
        for (key, opened) in checks {
            app.startup_wizard.open = true;
            choose(&mut app, key, &ctx).expect(key);
            assert!(!app.startup_wizard.open, "{key} closes the wizard");
            assert!(opened(&app), "{key} runs its action");
        }
        // The setting through ui.set.
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "startupWizard", "value": true })),
        )
        .expect("set");
        assert!(app.ui.prefs.startup_wizard);
    }
}
