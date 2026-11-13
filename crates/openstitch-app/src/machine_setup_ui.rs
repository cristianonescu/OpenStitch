//! Help ▸ Machine Setup (#508 part 3, beyond PE-DESIGN): which machine the user sews on, asked
//! once on the first interactive start and re-run from the Help menu. It stores what the user
//! says through `machine.setProfile`: new designs start on the chosen hoop, exports warn about
//! stitches longer than the machine sews and trims it cannot cut, and the brand's own format is
//! named. Nothing is asked again once answered or skipped (`machineSetupDone` in `ui.json`).

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui::{self, RichText};
use openstitch_app::editor::CommandRequest;
use openstitch_engine::machine_profile::{BRAND_FORMATS, format_of};
use serde_json::json;

/// The window and what the user has chosen so far.
#[derive(Default)]
pub struct MachineSetup {
    pub open: bool,
    draft: Option<Draft>,
}

#[derive(Clone)]
struct Draft {
    brand: String,
    hoop: Option<String>,
    cutter: bool,
    max_stitch: Option<f64>,
    /// Needles on a multi-needle machine (#866); `None` for a single needle.
    needles: Option<usize>,
}

/// Whether this start asks: an interactive start with the user's own settings, not answered
/// or skipped before. Scripted runs, screenshots and tests never ask.
pub fn asks_at_start(l: &crate::app::Launch, prefs: &openstitch_app::editor::prefs::UiPrefs) -> bool {
    l.interactive() && l.user_settings.is_some() && !prefs.machine_setup_done
}

impl MachineSetup {
    pub fn open(&mut self) {
        self.open = true;
        self.draft = None;
    }
}

/// The current profile as a draft (Brother, no hoop, no cutter when none is set).
fn draft_from(app: &mut EditorApp) -> Draft {
    let p = app
        .session
        .execute("machine.profile", Some(&json!({})))
        .map(|v| v["profile"].clone())
        .unwrap_or_default();
    Draft {
        brand: p["brand"].as_str().unwrap_or("Brother").to_string(),
        hoop: p["hoop"].as_str().map(str::to_string),
        cutter: p["cutter"].as_bool().unwrap_or(false),
        max_stitch: p["maxStitchMm"].as_f64(),
        needles: p["needles"].as_u64().map(|n| n as usize),
    }
}

pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    if !app.machine_setup.open {
        return;
    }
    let mut d = match app.machine_setup.draft.clone() {
        Some(d) => d,
        None => draft_from(app),
    };
    let (mut save, mut skip) = (false, false);
    let mut open = true;
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-machine-setup")).id(egui::Id::unique("window-machine-setup"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            ui.label(t("ui-which-machine-do-you-sew-on"));
            ui.add_space(6.0);
            egui::Grid::new("machine-setup").num_columns(2).show(ui, |ui| {
                ui.label(t("ui-brand"));
                let brand = egui::ComboBox::from_id_salt("machine-brand")
                    .selected_text(d.brand.as_str())
                    .show_ui(ui, |ui| {
                        for (b, _) in BRAND_FORMATS {
                            if ui.selectable_label(d.brand == b, b).clicked() && d.brand != b {
                                d.brand = b.to_string();
                                d.hoop = None;
                            }
                        }
                    })
                    .response;
                crate::a11y::name(&brand, "Brand");
                ui.end_row();

                ui.label(t("ui-hoop"));
                let hoops: Vec<_> =
                    openstitch_design::machine::brand_hoops(&d.brand).collect();
                let shown = d
                    .hoop
                    .as_deref()
                    .and_then(|id| hoops.iter().find(|h| h.id == id))
                    .map_or("Choose a hoop", |h| h.label);
                let hoop = egui::ComboBox::from_id_salt("machine-hoop")
                    .selected_text(shown)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for h in &hoops {
                            if ui
                                .selectable_label(d.hoop.as_deref() == Some(h.id), h.label)
                                .clicked()
                            {
                                d.hoop = Some(h.id.to_string());
                            }
                        }
                    })
                    .response;
                crate::a11y::name(&hoop, "Hoop");
                ui.end_row();

                ui.label("");
                ui.checkbox(&mut d.cutter, t("ui-the-machine-cuts-threads-thread-cutter"));
                ui.end_row();

                ui.label(t("ui-longest-stitch"));
                ui.horizontal(|ui| {
                    let mut known = d.max_stitch.is_some();
                    if ui
                        .checkbox(&mut known, t("ui-from-the-manual"))
                        .on_hover_text(t("ui-leave-it-off-when-the-manual-tip"))
                        .changed()
                    {
                        d.max_stitch = known.then_some(12.0);
                    }
                    if let Some(v) = d.max_stitch.as_mut() {
                        let r = ui.add(egui::DragValue::new(v).range(1.0..=15.0).speed(0.1).suffix(" mm"));
                        crate::a11y::name(&r, "Longest stitch (mm)");
                    }
                });
                ui.end_row();

                ui.label(t("ui-needles"));
                ui.horizontal(|ui| {
                    let mut multi = d.needles.is_some();
                    if ui
                        .checkbox(&mut multi, t("ui-multi-needle-machine"))
                        .on_hover_text(t("ui-multi-needle-machine-tip"))
                        .changed()
                    {
                        d.needles = multi.then_some(6);
                    }
                    if let Some(n) = d.needles.as_mut() {
                        let r = ui.add(egui::DragValue::new(n).range(1..=15));
                        crate::a11y::name(&r, "Needles");
                    }
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "{} machines read {} files.",
                    d.brand,
                    format_of(&d.brand).to_uppercase()
                ))
                .weak(),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                save = ui.button(t("ui-save")).clicked();
                skip = ui
                    .button(t("ui-not-now"))
                    .on_hover_text(t("ui-help-machine-setup-asks-again-tip"))
                    .clicked();
            });
        });
    app.machine_setup.draft = Some(d.clone());
    if save {
        let mut params = json!({ "brand": d.brand, "cutter": d.cutter });
        if let Some(h) = &d.hoop {
            params["hoop"] = json!(h);
        }
        if let Some(m) = d.max_stitch {
            params["maxStitchMm"] = json!(m);
        }
        if let Some(n) = d.needles {
            params["needles"] = json!(n);
        }
        if app
            .execute(&CommandRequest::new("machine.setProfile", params))
            .is_some()
        {
            app.status = openstitch_app::tf!("ui-status-machine-set-new-designs-start-on", brand = d.brand);
            open = false;
        }
    }
    if skip {
        open = false;
    }
    if !open {
        app.machine_setup.open = false;
        app.machine_setup.draft = None;
        // Answered or skipped: not asked again at start.
        if !app.ui.prefs.machine_setup_done {
            app.ui.prefs.machine_setup_done = true;
            app.ui.save_prefs();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;
    use openstitch_app::editor::prefs::UiPrefs;

    #[test]
    fn the_first_interactive_start_asks_once_and_help_asks_again() {
        let mut prefs = UiPrefs::default();
        let mut l = Launch {
            user_settings: Some("user.json".into()),
            ..Launch::default()
        };
        assert!(asks_at_start(&l, &prefs), "first start");
        prefs.machine_setup_done = true;
        assert!(!asks_at_start(&l, &prefs), "answered or skipped");
        let back = UiPrefs::from_json(&prefs.to_json());
        assert!(back.machine_setup_done, "kept in ui.json");
        prefs.machine_setup_done = false;
        l.screenshot = Some("shot.png".into());
        assert!(!asks_at_start(&l, &prefs), "screenshots never ask");
        l.screenshot = None;
        l.user_settings = None;
        assert!(!asks_at_start(&l, &prefs), "tests and runs without settings never ask");

        // Help ▸ Machine Setup (ui.help) opens it any time.
        let mut app = crate::app::EditorApp::new(Launch::default());
        assert!(!app.machine_setup.open);
        app.ui_command("ui.help", Some(&json!({ "item": "machineSetup" })))
            .expect("help");
        assert!(app.machine_setup.open);
        assert_eq!(draft_from(&mut app).brand, "Brother", "no profile yet");
        app.execute(&CommandRequest::new(
            "machine.setProfile",
            json!({ "brand": "Pfaff", "cutter": true }),
        ))
        .expect("profile");
        let d = draft_from(&mut app);
        assert_eq!((d.brand.as_str(), d.cutter), ("Pfaff", true));
    }
}
