//! The Image to Embroidery dialog (Design → Image to Embroidery…). The settings live in
//! [`openstitch_app::editor::wizard::ImageWizard`]; Convert runs its one `digitize.*` command,
//! so the whole conversion is one Undo.

use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use openstitch_app::editor::wizard::{ImageWizard, LINE_SEWS, Method, REGION_SEWS};

pub fn image_wizard_window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.wizard.open;
    let mut convert = false;
    // The user's thread charts, for the Threads and "I own" choices.
    let charts: Vec<String> = app
        .session
        .user_settings()
        .thread_charts
        .iter()
        .map(|c| c.name.clone())
        .collect();
    egui::Window::new("Image to Embroidery")
        .open(&mut open)
        .default_width(360.0)
        .resizable(false)
        .show(ctx, |ui| {
            let w = &mut app.wizard;
            crate::art::header(ui, crate::art::Art::WizardImage, |ui| {
                ui.add_space(4.0);
                if ui.button("Choose image…").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "bmp"])
                        .pick_file()
                {
                    w.path = Some(p);
                }
                let name = w
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map_or("No image chosen".into(), |n| {
                        n.to_string_lossy().into_owned()
                    });
                ui.label(name);
            });
            ui.separator();
            for m in Method::ALL {
                if ui
                    .radio_value(&mut w.method, m, m.label())
                    .on_hover_text(m.hint())
                    .changed()
                {
                    w.colors = w.colors_default();
                }
            }
            ui.weak(w.method.hint());
            ui.separator();
            egui::Grid::new("wizard").num_columns(2).show(ui, |ui| {
                ui.label("Width");
                ui.horizontal(|ui| {
                    let mut fit = w.width_mm.is_none();
                    if ui.checkbox(&mut fit, "Fit page").changed() {
                        w.width_mm = if fit { None } else { Some(80.0) };
                    }
                    if let Some(v) = &mut w.width_mm {
                        ui.add(egui::DragValue::new(v).range(1.0..=2000.0).suffix(" mm"));
                    }
                });
                ui.end_row();
                let colours = match w.method {
                    Method::DesignCenter => false,
                    Method::PhotoFill | Method::PhotoLines => !w.mono,
                    _ => true,
                };
                if matches!(w.method, Method::PhotoFill | Method::PhotoLines) {
                    ui.label("Mode");
                    ui.horizontal(|ui| {
                        let mut mono = w.mono;
                        ui.radio_value(&mut mono, false, "Colour");
                        ui.radio_value(&mut mono, true, "Mono");
                        w.change(|w| w.mono = mono);
                    });
                    ui.end_row();
                }
                if colours {
                    ui.label("Colours");
                    // 0 is "Auto" (Auto Punch, Cross Stitch): the image's own colour count.
                    let low = if matches!(w.method, Method::PhotoFill | Method::PhotoLines) {
                        1
                    } else {
                        0
                    };
                    w.colors = w.colors.max(low);
                    ui.add(
                        egui::Slider::new(&mut w.colors, low..=32).custom_formatter(|v, _| {
                            if v == 0.0 {
                                "Auto".into()
                            } else {
                                format!("{v}")
                            }
                        }),
                    );
                    ui.end_row();
                }
                match w.method {
                    Method::AutoPunch => {
                        ui.label("Smallest region");
                        ui.add(egui::Slider::new(&mut w.min_area, 0.0..=50.0).suffix(" mm²"));
                        ui.end_row();
                        ui.label("Satin up to");
                        ui.add(egui::Slider::new(&mut w.satin_max, 1.0..=12.0).suffix(" mm"));
                        ui.end_row();
                    }
                    Method::CrossStitch => {
                        ui.label("Cell size");
                        ui.add(egui::Slider::new(&mut w.cell, 1.0..=10.0).suffix(" mm"));
                        ui.end_row();
                        ui.label("Stitch");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut w.half_cross, false, "Full cross");
                            ui.radio_value(&mut w.half_cross, true, "Half cross");
                        });
                        ui.end_row();
                    }
                    Method::PhotoFill | Method::PhotoLines => {
                        ui.label("Row spacing");
                        ui.horizontal(|ui| {
                            let mut auto = w.row_spacing.is_none();
                            if ui.checkbox(&mut auto, "Default").changed() {
                                w.row_spacing = if auto { None } else { Some(0.6) };
                            }
                            if let Some(v) = &mut w.row_spacing {
                                ui.add(
                                    egui::DragValue::new(v)
                                        .range(0.2..=5.0)
                                        .speed(0.05)
                                        .suffix(" mm"),
                                );
                            }
                        });
                        ui.end_row();
                    }
                    Method::DesignCenter => {
                        ui.label("Line threshold");
                        ui.add(egui::Slider::new(&mut w.threshold, 1..=254));
                        ui.end_row();
                        ui.label("Lines");
                        combo(ui, "lineSew", &mut w.line_sew, &LINE_SEWS);
                        ui.end_row();
                        ui.label("Regions");
                        combo(ui, "regionSew", &mut w.region_sew, &REGION_SEWS);
                        ui.end_row();
                    }
                }
                if matches!(w.method, Method::AutoPunch | Method::CrossStitch) {
                    ui.label("");
                    ui.checkbox(&mut w.skip_background, "Leave out the background colour");
                    ui.end_row();
                }
                if w.improvable() {
                    let improved = if w.method == Method::AutoPunch {
                        "Picks colours and threads by how they look (CIELAB, CIEDE2000) and merges \
                         specks under 3 mm², for fewer trims; line art keeps its lines."
                    } else if w.method == Method::PhotoLines {
                        "Picks Auto Select's threads by how they look (CIELAB, CIEDE2000) and \
                         mixes the lines in linear light, for truer colour."
                    } else {
                        "Picks threads and mixes them by how they look (CIELAB, CIEDE2000), for \
                         truer colour; a few more trims."
                    };
                    ui.label("Mode");
                    ui.horizontal(|ui| {
                        let before = w.open_stitch;
                        let mut open_stitch = w.open_stitch;
                        ui.radio_value(&mut open_stitch, false, "PE-DESIGN")
                            .on_hover_text("Converts as PE-DESIGN 11.50 does (RGB colour reduction and thread matching).");
                        ui.radio_value(&mut open_stitch, true, "OpenStitch")
                            .on_hover_text(improved);
                        w.change(|w| w.open_stitch = open_stitch);
                        // Auto Punch's small-region default follows the mode unless it was changed.
                        if before != w.open_stitch && w.method == Method::AutoPunch {
                            let (from, to) = if w.open_stitch { (1.0, 3.0) } else { (3.0, 1.0) };
                            if w.min_area == from {
                                w.min_area = to;
                            }
                        }
                    });
                    ui.end_row();
                    if w.open_stitch {
                        ui.label("Fidelity");
                        ui.horizontal(|ui| {
                            let mut high = w.high_fidelity;
                            ui.radio_value(&mut high, false, "Standard")
                                .on_hover_text("Today's colour counts and speed.");
                            ui.radio_value(&mut high, true, "High").on_hover_text(
                                "Closer to the image: up to 24 colours by default and slower \
                                 passes. The needle density stays the same.",
                            );
                            w.change(|w| w.high_fidelity = high);
                        });
                        ui.end_row();
                        if w.method != Method::PhotoLines {
                            threads_rows(ui, w, &charts);
                        }
                    }
                }
                if matches!(w.method, Method::AutoPunch | Method::DesignCenter) {
                    ui.label("");
                    ui.checkbox(&mut w.curves, "Fit curves");
                    ui.end_row();
                }
            });
            ui.separator();
            ui.weak("Approximations of PE-DESIGN's wizards; results are editable and one Undo.");
            ui.add_enabled_ui(w.path.is_some(), |ui| {
                convert = ui.button("Convert").clicked();
            });
        });
    app.wizard.open = open;
    if convert {
        run(app);
    }
}

/// The Threads and "I own" rows (B16.1 T5): where OpenStitch mode chooses its threads (the
/// Brother chart, every built-in chart, or one of the user's charts), and which of the user's
/// charts holds the threads they own, so the result can list the rest to buy.
fn threads_rows(ui: &mut egui::Ui, w: &mut ImageWizard, charts: &[String]) {
    let label = |v: &str| match v {
        "brother" => "Brother".to_string(),
        "all" => "All built-in charts".to_string(),
        other => other.to_string(),
    };
    ui.label("Threads");
    egui::ComboBox::from_id_salt("wizardThreads")
        .selected_text(label(&w.threads))
        .show_ui(ui, |ui| {
            for v in ["brother", "all"]
                .into_iter()
                .map(str::to_string)
                .chain(charts.iter().cloned())
            {
                let text = label(&v);
                ui.selectable_value(&mut w.threads, v, text);
            }
        });
    ui.end_row();
    ui.label("I own");
    egui::ComboBox::from_id_salt("wizardOwned")
        .selected_text(w.owned.clone().unwrap_or_else(|| "(not set)".into()))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut w.owned, None, "(not set)");
            for c in charts {
                ui.selectable_value(&mut w.owned, Some(c.clone()), c);
            }
        })
        .response
        .on_hover_text("One of your thread charts: threads chosen outside it are listed to buy.");
    ui.end_row();
}

fn combo(ui: &mut egui::Ui, id: &str, value: &mut &'static str, options: &[&'static str]) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(*value)
        .show_ui(ui, |ui| {
            for o in options {
                ui.selectable_value(value, o, *o);
            }
        });
}

/// Runs the conversion, selects what it made and closes the dialog.
pub fn run(app: &mut EditorApp) {
    let Some(request) = app.wizard.request() else {
        return;
    };
    if let Some(result) = app.grouped(|a| a.execute(&request)) {
        app.selection = CommandRequest::created_ids(&result);
        app.status = app.wizard.summary(&result);
        app.wizard.open = false;
    }
}
