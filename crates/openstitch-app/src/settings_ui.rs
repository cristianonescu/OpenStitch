//! Saved sewing-attribute sets and user thread charts (PE-DESIGN's custom sewing attributes and
//! user thread colour lists). They live in the user settings file, not in the design; each
//! change is one `attributes.*` / `threads.*` command.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui::{self, Color32};
use openstitch_app::editor::CommandRequest;
use serde_json::json;

#[derive(Default)]
pub struct SettingsState {
    pub sets_open: bool,
    pub charts_open: bool,
    /// Name typed for a new attribute set.
    pub set_name: String,
    /// Selected chart, new chart name, the colour being added and its details.
    pub chart: Option<String>,
    pub chart_name: String,
    pub color: [u8; 3],
    pub code: String,
    pub brand: String,
    pub description: String,
}

fn single(app: &EditorApp) -> Option<u64> {
    (app.selection.len() == 1)
        .then(|| app.selection.iter().next().copied())
        .flatten()
}

pub fn windows(app: &mut EditorApp, ctx: &egui::Context) {
    sets_window(app, ctx);
    charts_window(app, ctx);
}

fn sets_window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.settings.sets_open;
    let mut request = None;
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-sewing-attribute-sets")).id(egui::Id::unique("window-sewing-attribute-sets"))
        .open(&mut open)
        .default_width(320.0)
        .show(ctx, |ui| {
            let sets: Vec<String> = app
                .session
                .user_settings()
                .attribute_sets
                .iter()
                .map(|s| s.name.clone())
                .collect();
            if sets.is_empty() {
                ui.label(egui::RichText::new(t("ui-no-saved-sets-yet")).weak());
            }
            let any = !app.selection.is_empty();
            for name in sets {
                ui.horizontal(|ui| {
                    ui.label(&name);
                    if ui
                        .add_enabled(any, egui::Button::new(t("menu-apply")))
                        .on_hover_text(t("ui-apply-to-the-selected-objects-their-tip"))
                        .clicked()
                    {
                        request = Some(CommandRequest::new(
                            "attributes.apply",
                            json!({ "name": name, "ids": CommandRequest::ids(&app.selection) }),
                        ));
                    }
                    if ui.button(t("ui-delete")).clicked() {
                        request = Some(CommandRequest::new(
                            "attributes.delete",
                            json!({ "name": name }),
                        ));
                    }
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                crate::a11y::name(
                    &ui.text_edit_singleline(&mut app.settings.set_name),
                    "Attribute set name",
                );
                let from = single(app);
                if ui
                    .add_enabled(
                        from.is_some() && !app.settings.set_name.trim().is_empty(),
                        egui::Button::new(t("ui-save-selected")),
                    )
                    .on_hover_text(t("ui-save-the-selected-object-s-sewing-tip"))
                    .clicked()
                {
                    request = Some(CommandRequest::new(
                        "attributes.save",
                        json!({ "name": app.settings.set_name.trim(), "from": from }),
                    ));
                }
            });
        });
    if let Some(r) = request
        && app.execute(&r).is_some()
    {
        app.status = openstitch_app::tf!("ui-status-done", command = r.command);
    }
    app.settings.sets_open = open;
}

fn charts_window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.settings.charts_open;
    let mut request: Option<CommandRequest> = None;
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-user-thread-charts")).id(egui::Id::unique("window-user-thread-charts"))
        .open(&mut open)
        .default_width(380.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button(t("ui-match-to-thread-chart"))
                    .on_hover_text(
                        "Change the design's colours to a chart's threads, with a preview",
                    )
                    .clicked()
                {
                    app.colour_match.match_open = true;
                }
                if ui
                    .button(t("ui-merge-similar-colours"))
                    .on_hover_text(t("ui-join-near-identical-thread-colours-to-tip"))
                    .clicked()
                {
                    app.colour_match.merge_open = true;
                }
            });
            // #796: the chart the colour picker and the image wizards start from.
            ui.horizontal(|ui| {
                use openstitch_engine::user::ChartRef;
                let user = app.session.user_settings();
                let current = user.default_chart();
                let (name, _) = crate::panels::picker_chart(&current, user);
                let mut choices: Vec<(ChartRef, String)> = openstitch_engine::thread_match::builtins()
                    .into_iter()
                    .map(|b| (ChartRef::Builtin(b.id.into()), b.name.to_string()))
                    .collect();
                choices.extend(
                    user.thread_charts
                        .iter()
                        .map(|c| (ChartRef::Chart(c.name.clone()), c.name.clone())),
                );
                let mut chosen = current.clone();
                let r = egui::ComboBox::from_label(t("ui-default-thread-chart"))
                    .selected_text(name)
                    .show_ui(ui, |ui| {
                        for (c, label) in choices {
                            ui.selectable_value(&mut chosen, c, label);
                        }
                    });
                r.response.on_hover_text(t("ui-default-thread-chart-tip"));
                if chosen != current {
                    request = Some(CommandRequest::new("threads.setDefault", chosen.to_json()));
                }
            });
            ui.separator();
            let charts = app.session.user_settings().thread_charts.clone();
            let st = &mut app.settings;
            if st
                .chart
                .as_ref()
                .is_some_and(|c| !charts.iter().any(|x| &x.name == c))
            {
                st.chart = None;
            }
            if st.chart.is_none() {
                st.chart = charts.first().map(|c| c.name.clone());
            }
            ui.horizontal(|ui| {
                let r = egui::ComboBox::from_label("Chart")
                    .selected_text(st.chart.clone().unwrap_or_else(|| "—".into()))
                    .show_ui(ui, |ui| {
                        for c in &charts {
                            ui.selectable_value(&mut st.chart, Some(c.name.clone()), &c.name);
                        }
                    });
                crate::a11y::name(&r.response, "Chart");
            });
            ui.horizontal(|ui| {
                crate::a11y::name(
                    &ui.text_edit_singleline(&mut st.chart_name),
                    "New chart name",
                );
                let name = st.chart_name.trim().to_string();
                if ui
                    .add_enabled(!name.is_empty(), egui::Button::new(t("ui-new")))
                    .clicked()
                {
                    request = Some(CommandRequest::new(
                        "threads.createChart",
                        json!({ "name": name }),
                    ));
                    st.chart = Some(name.clone());
                }
                if ui
                    .button(t("ui-import-palette"))
                    .on_hover_text(t("ui-a-gimp-or-inkscape-palette-gpl-tip"))
                    .clicked()
                {
                    let name = name.clone();
                    crate::files::pick_then(
                        crate::files::FileDialog::new()
                            .add_filter("GIMP / Inkscape palette", &["gpl"]),
                        move |app, path| {
                            let mut p = json!({ "path": path.to_string_lossy() });
                            if !name.is_empty() {
                                p["name"] = json!(name);
                            }
                            app.execute(&CommandRequest::new("threads.importPalette", p));
                        },
                    );
                }
                if let Some(c) = st.chart.clone() {
                    if ui
                        .add_enabled(!name.is_empty(), egui::Button::new(t("ui-rename")))
                        .clicked()
                    {
                        request = Some(CommandRequest::new(
                            "threads.renameChart",
                            json!({ "chart": c, "name": name }),
                        ));
                        st.chart = Some(name.clone());
                    }
                    if ui.button(t("ui-delete")).clicked() {
                        request = Some(CommandRequest::new(
                            "threads.deleteChart",
                            json!({ "chart": c }),
                        ));
                    }
                }
            });
            let Some(name) = st.chart.clone() else {
                return;
            };
            let Some(chart) = charts.iter().find(|c| c.name == name) else {
                return;
            };
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    let n = chart.threads.len();
                    for (i, t) in chart.threads.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let (r, _) = ui
                                .allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            ui.painter().rect_filled(
                                r,
                                2.0,
                                Color32::from_rgb(t.red, t.green, t.blue),
                            );
                            ui.label(format!(
                                "{}  {}  {}  {}",
                                t.catalog_number.as_deref().unwrap_or(""),
                                t.brand.as_deref().unwrap_or(""),
                                t.name.as_deref().unwrap_or(""),
                                t.hex()
                            ));
                            let mv = |to: usize| {
                                CommandRequest::new(
                                    "threads.moveColor",
                                    json!({ "chart": name, "index": i, "to": to }),
                                )
                            };
                            let up = egui::Button::new(crate::icons::ui_image(ui, "move_up"));
                            if ui.add_enabled(i > 0, up).on_hover_text(openstitch_app::editor::ui_texts::t("ui-move-up-tip")).clicked() {
                                request = Some(mv(i - 1));
                            }
                            let down = egui::Button::new(crate::icons::ui_image(ui, "move_down"));
                            if ui
                                .add_enabled(i + 1 < n, down)
                                .on_hover_text(openstitch_app::editor::ui_texts::t("ui-move-down-tip"))
                                .clicked()
                            {
                                request = Some(mv(i + 1));
                            }
                            if ui.button(openstitch_app::editor::ui_texts::t("ui-remove")).clicked() {
                                request = Some(CommandRequest::new(
                                    "threads.removeColor",
                                    json!({ "chart": name, "index": i }),
                                ));
                            }
                        });
                    }
                });
            ui.separator();
            egui::Grid::new("new-thread").num_columns(2).show(ui, |ui| {
                ui.label(t("ui-colour"));
                ui.color_edit_button_srgb(&mut st.color);
                ui.end_row();
                ui.label(t("ui-code"));
                ui.text_edit_singleline(&mut st.code);
                ui.end_row();
                ui.label(t("ui-brand"));
                ui.text_edit_singleline(&mut st.brand);
                ui.end_row();
                ui.label(t("ui-description"));
                ui.text_edit_singleline(&mut st.description);
                ui.end_row();
            });
            ui.horizontal(|ui| {
                if ui.button(t("ui-add-colour")).clicked() {
                    let [r, g, b] = st.color;
                    let mut p =
                        json!({ "chart": name, "color": format!("#{r:02x}{g:02x}{b:02x}") });
                    for (k, v) in [
                        ("code", &st.code),
                        ("brand", &st.brand),
                        ("description", &st.description),
                    ] {
                        if !v.trim().is_empty() {
                            p[k] = json!(v.trim());
                        }
                    }
                    request = Some(CommandRequest::new("threads.addColor", p));
                }
                if ui.button(t("ui-import-csv")).clicked() {
                    let name = name.clone();
                    crate::files::pick_then(
                        crate::files::FileDialog::new().add_filter("CSV", &["csv"]),
                        move |app, path| {
                            app.execute(&CommandRequest::new(
                                "threads.importCsv",
                                json!({ "chart": name, "path": path.to_string_lossy() }),
                            ));
                        },
                    );
                }
                if ui.button(t("ui-export-palette")).clicked()
                    && let Some(path) = crate::files::FileDialog::new()
                        .add_filter("GIMP / Inkscape palette", &["gpl"])
                        .set_file_name(format!("{name}.gpl"))
                        .save_file()
                {
                    request = Some(CommandRequest::new(
                        "threads.exportPalette",
                        json!({ "chart": name, "path": path.to_string_lossy() }),
                    ));
                }
                if ui.button(t("ui-export-csv")).clicked()
                    && let Some(path) = crate::files::FileDialog::new()
                        .add_filter("CSV", &["csv"])
                        .set_file_name(format!("{name}.csv"))
                        .save_file()
                {
                    request = Some(CommandRequest::new(
                        "threads.exportCsv",
                        json!({ "chart": name, "path": path.to_string_lossy() }),
                    ));
                }
                if ui
                    .button(t("ui-remap-design"))
                    .on_hover_text(t("ui-change-every-thread-to-the-nearest-tip"))
                    .clicked()
                {
                    request = Some(CommandRequest::new(
                        "threads.remap",
                        json!({ "chart": name }),
                    ));
                }
                if ui
                    .button(t("ui-match"))
                    .on_hover_text(t("ui-preview-the-match-to-this-chart-tip"))
                    .clicked()
                {
                    app.colour_match.target = crate::colour_match_ui::Target::User(name.clone());
                    app.colour_match.match_open = true;
                }
            });
        });
    if let Some(r) = request
        && app.execute(&r).is_some()
    {
        app.status = openstitch_app::tf!("ui-status-done", command = r.command);
    }
    app.settings.charts_open = open;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;

    #[test]
    fn settings_file_is_loaded_and_written() {
        let dir = openstitch_testutil::TempDir::new("app-user");
        let file = dir.join("user.json");
        let _ = std::fs::remove_file(&file);
        let mut app = EditorApp::new(Launch {
            user_settings: Some(file.clone()),
            ..Default::default()
        });
        app.execute(&CommandRequest::new(
            "threads.createChart",
            json!({ "name": "Mine" }),
        ));
        assert!(file.exists());
        let again = EditorApp::new(Launch {
            user_settings: Some(file.clone()),
            ..Default::default()
        });
        // A first run starts with "My Threads" (the New Brothread kit, #796), then "Mine".
        let names: Vec<&str> = again
            .session
            .user_settings()
            .thread_charts
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, ["My Threads", "Mine"]);
        // A broken file is reported, not fatal.
        std::fs::write(&file, "nonsense").expect("write");
        let broken = EditorApp::new(Launch {
            user_settings: Some(file.clone()),
            ..Default::default()
        });
        assert!(broken.status.contains("user settings"), "{}", broken.status);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
