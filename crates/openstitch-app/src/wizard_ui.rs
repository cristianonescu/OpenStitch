//! The Image to Embroidery dialog (Design → Image to Embroidery…). The settings live in
//! [`openstitch_app::editor::wizard::ImageWizard`]; Convert runs its one `digitize.*` command,
//! so the whole conversion is one Undo.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use openstitch_app::editor::wizard::{
    ImageWizard, LINE_SEWS, MARK_WIDTHS, Mark, Method, PRESETS, REGION_SEWS, REGION_TYPES,
    image_fraction, min_area_default, satin_default,
};

pub fn image_wizard_window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.wizard.open;
    let mut convert = false;
    let mut preview = false;
    // The user's thread charts, for the Threads and "I own" choices.
    let charts: Vec<String> = app
        .session
        .user_settings()
        .thread_charts
        .iter()
        .map(|c| c.name.clone())
        .collect();
    // OpenStitch mode starts from the user's default chart (#796) until a Threads choice.
    if !app.wizard.threads_chosen {
        app.wizard.threads = openstitch_app::editor::wizard::threads_value(
            &app.session.user_settings().default_chart(),
        );
    }
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-image-to-embroidery")).id(egui::Id::unique("window-image-to-embroidery"))
        .open(&mut open)
        .default_width(360.0)
        .resizable(false)
        .show(ctx, |ui| {
            let w = &mut app.wizard;
            crate::art::header(ui, crate::art::Art::WizardImage, |ui| {
                ui.add_space(4.0);
                if ui.button(t("ui-choose-image")).clicked() {
                    crate::files::pick_then(
                        crate::files::FileDialog::new()
                            .add_filter("Images", &["png", "jpg", "jpeg", "bmp"]),
                        |app, p| {
                            app.wizard.path = Some(p);
                            // Strokes belong to the image they were drawn on.
                            app.wizard.marks.clear();
                        },
                    );
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
                let mut method = w.method;
                if ui
                    .radio_value(&mut method, m, m.label())
                    .on_hover_text(m.hint())
                    .changed()
                {
                    w.set_method(method);
                }
            }
            ui.weak(w.method.hint());
            ui.separator();
            egui::Grid::new("wizard").num_columns(2).show(ui, |ui| {
                ui.label(t("ui-width"));
                ui.horizontal(|ui| {
                    let mut fit = w.width_mm.is_none();
                    if ui.checkbox(&mut fit, t("ui-fit-page")).changed() {
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
                    ui.label(t("ui-mode"));
                    ui.horizontal(|ui| {
                        let mut mono = w.mono;
                        ui.radio_value(&mut mono, false, t("ui-colour"));
                        ui.radio_value(&mut mono, true, t("ui-mono"));
                        w.change(|w| w.mono = mono);
                    });
                    ui.end_row();
                }
                if colours {
                    ui.label(t("ui-colours"));
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
                        ui.label(t("ui-smallest-region"));
                        if ui
                            .add(egui::Slider::new(&mut w.min_area, 0.0..=50.0).suffix(" mm²"))
                            .changed()
                        {
                            w.min_area_set = true;
                        }
                        ui.end_row();
                        ui.label(t("ui-satin-up-to"));
                        if ui
                            .add(egui::Slider::new(&mut w.satin_max, 1.0..=12.0).suffix(" mm"))
                            .changed()
                        {
                            w.satin_set = true;
                        }
                        ui.end_row();
                    }
                    Method::CrossStitch => {
                        ui.label(t("ui-cell-size"));
                        ui.add(egui::Slider::new(&mut w.cell, 1.0..=10.0).suffix(" mm"));
                        ui.end_row();
                        ui.label(t("ui-stitch"));
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut w.half_cross, false, t("ui-full-cross"));
                            ui.radio_value(&mut w.half_cross, true, t("ui-half-cross"));
                        });
                        ui.end_row();
                    }
                    Method::PhotoFill | Method::PhotoLines => {
                        ui.label(t("ui-row-spacing"));
                        ui.horizontal(|ui| {
                            let mut auto = w.row_spacing.is_none();
                            if ui.checkbox(&mut auto, t("ui-default")).changed() {
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
                        // OpenStitch mode may choose the threshold and line sewing itself.
                        if !(w.open_stitch && w.auto_lines) {
                            ui.label(t("ui-line-threshold"));
                            ui.add(egui::Slider::new(&mut w.threshold, 1..=254));
                            ui.end_row();
                            ui.label(t("ui-lines"));
                            combo(ui, "lineSew", &mut w.line_sew, &LINE_SEWS);
                            ui.end_row();
                        }
                        ui.label(t("ui-regions"));
                        combo(ui, "regionSew", &mut w.region_sew, &REGION_SEWS);
                        ui.end_row();
                    }
                }
                if w.method == Method::AutoPunch && w.open_stitch {
                    ui.label("");
                    ui.checkbox(&mut w.isolate_subject, t("ui-isolate-the-subject")).on_hover_text(
                        "Finds the subject by a graph cut on its colours and leaves out \
                         everything around it, whatever its colour. The image border is the \
                         background it learns from.",
                    );
                    ui.end_row();
                    if w.isolate_subject {
                        ui.label("");
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(w.path.is_some(), |ui| {
                                if ui
                                    .button(t("ui-mark-subject"))
                                    .on_hover_text(
                                        "Brush over parts that are subject or background for \
                                         certain, such as a bowl the colour of its table.",
                                    )
                                    .clicked()
                                {
                                    w.marking = true;
                                }
                            });
                            if !w.marks.is_empty() {
                                ui.weak(format!("{} strokes", w.marks.len()));
                            }
                        });
                        ui.end_row();
                    }
                    ui.label("");
                    ui.add_enabled_ui(!w.isolate_subject, |ui| {
                        ui.checkbox(&mut w.sew_background, t("ui-sew-the-background"))
                            .on_hover_text(
                                "Everything in the picture is sewn. Off leaves out the colour \
                                 covering most of the border; a background of the fabric's \
                                 colour is never sewn.",
                            );
                    });
                    ui.end_row();
                } else if matches!(w.method, Method::AutoPunch | Method::CrossStitch) {
                    ui.label("");
                    ui.checkbox(&mut w.skip_background, t("ui-leave-out-the-background-colour"));
                    ui.end_row();
                }
                if w.improvable() {
                    let improved = if w.method == Method::AutoPunch {
                        "Picks colours and threads by how they look (CIELAB, CIEDE2000) and merges \
                         specks under 3 mm², for fewer trims; line art keeps its lines."
                    } else if w.method == Method::CrossStitch {
                        "Each cell takes its mean colour, threads chosen by how they look, and \
                         crosses fill their cells."
                    } else if w.method == Method::DesignCenter {
                        "Finds the line threshold itself and sews lines as wide as the drawing's; \
                         region threads by how they look."
                    } else if w.method == Method::PhotoLines {
                        "Picks Auto Select's threads by how they look (CIELAB, CIEDE2000) and \
                         mixes the lines in linear light, for truer colour."
                    } else {
                        "Picks threads and mixes them by how they look (CIELAB, CIEDE2000), for \
                         truer colour; a few more trims."
                    };
                    // "Method", not "Mode": Photo Stitch's Colour / Mono row is its Mode.
                    ui.label(t("ui-method"));
                    ui.horizontal(|ui| {
                        let before = w.open_stitch;
                        let mut open_stitch = w.open_stitch;
                        ui.radio_value(&mut open_stitch, false, t("ui-pe-design-compatible"))
                            .on_hover_text(t("ui-converts-as-pe-design-11-50-tip"));
                        ui.radio_value(&mut open_stitch, true, t("ui-openstitch"))
                            .on_hover_text(improved);
                        w.change(|w| w.open_stitch = open_stitch);
                        // Auto Punch's small-region area shows the mode's default until it is set.
                        if before != w.open_stitch && !w.min_area_set {
                            w.min_area = min_area_default(w.open_stitch);
                        }
                        if before != w.open_stitch && !w.satin_set {
                            w.satin_max = satin_default(w.open_stitch);
                        }
                    });
                    ui.end_row();
                    if w.open_stitch {
                        open_stitch_rows(ui, w);
                    }
                    if w.open_stitch && w.has_fidelity() {
                        ui.label(t("ui-fidelity"));
                        ui.horizontal(|ui| {
                            let mut high = w.high_fidelity;
                            ui.radio_value(&mut high, false, t("ui-standard"))
                                .on_hover_text(t("ui-today-s-colour-counts-and-speed-tip"));
                            ui.radio_value(&mut high, true, t("ui-high")).on_hover_text(
                                "Closer to the image: up to 24 colours by default and slower \
                                 passes. The needle density stays the same.",
                            );
                            w.change(|w| w.high_fidelity = high);
                        });
                        ui.end_row();
                        if matches!(w.method, Method::AutoPunch | Method::PhotoFill) {
                            threads_rows(ui, w, &charts);
                        }
                    }
                }
                if matches!(w.method, Method::AutoPunch | Method::DesignCenter) {
                    ui.label("");
                    ui.checkbox(&mut w.curves, t("ui-fit-curves"));
                    ui.end_row();
                }
            });
            if w.method == Method::AutoPunch {
                ui.separator();
                preview = regions_section(ui, w);
            }
            ui.separator();
            ui.weak("Approximations of PE-DESIGN's wizards; results are editable and one Undo.");
            ui.add_enabled_ui(w.path.is_some(), |ui| {
                convert = ui.button(t("ui-convert")).clicked();
            });
        });
    app.wizard.open = open;
    marking_window(app, ctx);
    fixing_window(app, ctx);
    if preview {
        run_preview(app);
    }
    if convert {
        run(app);
    }
}

/// Auto Punch's preview (#489): the conversion as a dry run; its regions fill the table and
/// the click-to-fix picture. Nothing is added to the design.
pub fn run_preview(app: &mut EditorApp) {
    let Some(request) = app.wizard.preview_request() else {
        return;
    };
    if let Some(result) = app.execute(&request) {
        app.wizard.set_plan(&result);
        let n = app.wizard.plan.as_ref().map_or(0, |p| p.regions.len());
        app.status = openstitch_app::tf!("ui-status-auto-punch-preview-regions", n = n);
    }
}

/// An egui colour from `#rrggbb`.
fn hex_colour(hex: &str) -> egui::Color32 {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    egui::Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// A colour sample: `label` (the thread's code, #798, or the hex value) on its own colour.
fn colour_text(hex: &str, label: &str) -> egui::RichText {
    let c = hex_colour(hex);
    let light = u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b()) > 380;
    egui::RichText::new(label)
        .monospace()
        .background_color(c)
        .color(if light {
            egui::Color32::BLACK
        } else {
            egui::Color32::WHITE
        })
}

/// Auto Punch's regions (#489): Preview, the table of regions with the stitch each got and
/// why, where any row can be changed, the "review" note, click-to-fix and the colour order.
/// Returns whether Preview was pressed.
fn regions_section(ui: &mut egui::Ui, w: &mut ImageWizard) -> bool {
    let mut preview = false;
    ui.horizontal(|ui| {
        ui.add_enabled_ui(w.path.is_some(), |ui| {
            preview = ui
                .button(if w.plan.is_some() {
                    "Preview again"
                } else {
                    "Preview regions"
                })
                .on_hover_text(
                    "Traces the picture without adding anything: every region with the stitch \
                     it gets and why, to change before Convert.",
                )
                .clicked();
        });
        let can_fix = w.plan.as_ref().is_some_and(|p| p.map_size.0 > 0) && w.plan_current();
        ui.add_enabled_ui(can_fix, |ui| {
            if ui
                .button(t("ui-fix-on-the-picture"))
                .on_hover_text(t("ui-click-a-part-of-the-picture-tip"))
                .clicked()
            {
                w.fixing = true;
            }
        });
    });
    if w.plan.is_none() {
        return preview;
    }
    if !w.plan_current() {
        ui.weak("The settings changed: preview again to use the table.");
        return preview;
    }
    if let Some(note) = w.review_note() {
        ui.colored_label(ui.visuals().warn_fg_color, note);
    }
    let regions = w
        .plan
        .as_ref()
        .map(|p| p.regions.clone())
        .unwrap_or_default();
    egui::CollapsingHeader::new(format!("Regions ({})", regions.len()))
        .default_open(true)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    egui::Grid::new("wizardRegions")
                        .num_columns(5)
                        .striped(true)
                        .show(ui, |ui| {
                            for h in ["Region", "Colour", "Size", "Stitch", "Why"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for r in &regions {
                                ui.label(format!("{}", r.id));
                                let label = if r.code.is_empty() { &r.color } else { &r.code };
                                ui.label(colour_text(&r.color, label))
                                    .on_hover_text(format!("{} ({})", r.thread, r.color));
                                // Lines (thin lines, outlines) have no width or area of their own.
                                let line = r.width_mm <= 0.0;
                                ui.label(if !line {
                                    format!("{:.1} mm, {:.0} mm²", r.width_mm, r.area_mm2)
                                } else {
                                    "line".to_string()
                                });
                                let mut kind: &'static str =
                                    w.overrides.get(&r.id).copied().unwrap_or("auto");
                                let shown = if kind == "auto" {
                                    format!("auto ({})", r.kind)
                                } else {
                                    kind.to_string()
                                };
                                // Lines (thin lines, outlines) can only be left out.
                                let choices: &[&'static str] = if !line {
                                    &REGION_TYPES
                                } else {
                                    &["auto", "skip"]
                                };
                                egui::ComboBox::from_id_salt(("wizardRegion", r.id))
                                    .selected_text(shown)
                                    .show_ui(ui, |ui| {
                                        for c in choices {
                                            ui.selectable_value(&mut kind, c, *c);
                                        }
                                    });
                                w.set_region(r.id, kind);
                                ui.weak(&r.why);
                                ui.end_row();
                            }
                        });
                });
        });
    let colours = w.colour_order();
    if colours.len() > 1 {
        egui::CollapsingHeader::new("Colour order").show(ui, |ui| {
            let mut moved = None;
            for (i, c) in colours.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}.", i + 1));
                    ui.label(colour_text(c, &w.colour_label(c))).on_hover_text(c);
                    if ui
                        .add_enabled(i > 0, egui::Button::new(t("ui-earlier")))
                        .clicked()
                    {
                        moved = Some((i, true));
                    }
                    if ui
                        .add_enabled(i + 1 < colours.len(), egui::Button::new(t("ui-later")))
                        .clicked()
                    {
                        moved = Some((i, false));
                    }
                });
            }
            if let Some((i, earlier)) = moved {
                w.move_colour(i, earlier);
            }
            ui.weak("Region numbers follow the sewing order: preview again after a change.");
            if !w.order.is_empty() && ui.button(t("ui-pe-design-s-order")).clicked() {
                w.order.clear();
            }
        });
    }
    preview
}

/// Click-to-fix (#489): the picture tinted by each region's stitch (fill blue, satin orange,
/// line green, left out dimmed); a click flips the region under it between sewn and left out.
fn fixing_window(app: &mut EditorApp, ctx: &egui::Context) {
    let w = &mut app.wizard;
    if !(w.open && w.fixing && w.method == Method::AutoPunch && w.plan_current()) {
        w.fixing = false;
        return;
    }
    let Some(path) = w.path.clone() else {
        w.fixing = false;
        return;
    };
    if app.wizard_preview.as_ref().is_none_or(|p| p.0 != path) {
        app.wizard_preview = crate::creator_ui::load_image(&path, ctx, "wizard-marking")
            .ok()
            .map(|(size, t)| (path.clone(), size, t));
    }
    // The tint, rebuilt when the plan or the choices change.
    let w = &app.wizard;
    let Some(plan) = &w.plan else {
        return;
    };
    let (mw, mh) = plan.map_size;
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        plan.map.hash(&mut h);
        format!("{:?}", w.overrides).hash(&mut h);
        h.finish()
    };
    let id = egui::Id::unique("wizardFixTint");
    let cached: Option<(u64, egui::TextureHandle)> = ctx.data(|d| d.get_temp(id));
    let tint = match cached {
        Some((k, t)) if k == key => t,
        _ => {
            let pixels: Vec<egui::Color32> = plan
                .map
                .iter()
                .map(|&r| match usize::try_from(r).map(|r| w.region_kind(r)) {
                    Ok("fill") => egui::Color32::from_rgba_unmultiplied(40, 110, 230, 90),
                    Ok("satin") => egui::Color32::from_rgba_unmultiplied(240, 140, 30, 110),
                    Ok("line") => egui::Color32::from_rgba_unmultiplied(40, 180, 70, 110),
                    Ok("skip") => egui::Color32::from_rgba_unmultiplied(255, 255, 255, 200),
                    _ => egui::Color32::TRANSPARENT,
                })
                .collect();
            let image = egui::ColorImage {
                size: [mw.max(1), mh.max(1)],
                pixels,
                source_size: egui::vec2(mw as f32, mh as f32),
            };
            let t = ctx.load_texture("wizard-fix-tint", image, egui::TextureOptions::NEAREST);
            ctx.data_mut(|d| d.insert_temp(id, (key, t.clone())));
            t
        }
    };
    let mut open = true;
    let mut clicked = None;
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-fix-on-the-picture")).id(egui::Id::unique("window-fix-on-the-picture"))
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let Some((_, (iw, ih), texture)) = &app.wizard_preview else {
                ui.label(t("ui-the-image-could-not-be-read"));
                return;
            };
            let scale = 480.0 / (*iw).max(*ih).max(1) as f32;
            let size = egui::vec2(*iw as f32 * scale, *ih as f32 * scale);
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
            let painter = ui.painter_at(rect);
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(texture.id(), rect, uv, egui::Color32::WHITE);
            painter.image(tint.id(), rect, uv, egui::Color32::WHITE);
            if response.clicked()
                && let Some(f) = response
                    .interact_pointer_pos()
                    .and_then(|p| image_fraction([p.x, p.y], rect.min.into(), size.into()))
            {
                clicked = Some(f);
            }
            if let Some(f) = response
                .hover_pos()
                .and_then(|p| image_fraction([p.x, p.y], rect.min.into(), size.into()))
                && let Some(r) = app.wizard.region_at(f)
            {
                response.on_hover_text(format!("Region {r}: {}", app.wizard.region_kind(r)));
            }
            ui.weak(
                "Blue fill, orange satin, green line, pale: left out. Click a part to leave it \
                 out or put it back; the Regions table changes with it.",
            );
        });
    if let Some(f) = clicked {
        app.wizard.toggle_at(f);
    }
    app.wizard.fixing = open;
}

/// Auto Punch's subject marking (#350): the chosen image, on which the user brushes parts that
/// are subject (green) or background (red) for certain. The strokes are kept as fractions of
/// the image and sent with the conversion.
fn marking_window(app: &mut EditorApp, ctx: &egui::Context) {
    let w = &mut app.wizard;
    if !(w.open && w.marking && w.isolate_subject) {
        return;
    }
    let Some(path) = w.path.clone() else {
        w.marking = false;
        return;
    };
    if app.wizard_preview.as_ref().is_none_or(|p| p.0 != path) {
        app.wizard_preview = crate::creator_ui::load_image(&path, ctx, "wizard-marking")
            .ok()
            .map(|(size, t)| (path.clone(), size, t));
    }
    let mut open = true;
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-mark-the-subject")).id(egui::Id::unique("window-mark-the-subject"))
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let w = &mut app.wizard;
            ui.horizontal(|ui| {
                ui.radio_value(&mut w.mark_subject, true, t("ui-subject"));
                ui.radio_value(&mut w.mark_subject, false, t("ui-background"));
                ui.label(t("ui-brush"));
                ui.add(egui::Slider::new(&mut w.mark_width, MARK_WIDTHS).show_value(false));
            });
            ui.horizontal(|ui| {
                if ui.button(t("ui-undo-stroke")).clicked() {
                    w.marks.pop();
                }
                if ui.button(t("menu-clear")).clicked() {
                    w.marks.clear();
                }
            });
            let Some((_, (iw, ih), texture)) = &app.wizard_preview else {
                ui.label(t("ui-the-image-could-not-be-read"));
                return;
            };
            // Fit the image in 480 × 480 points.
            let scale = 480.0 / (*iw).max(*ih).max(1) as f32;
            let size = egui::vec2(*iw as f32 * scale, *ih as f32 * scale);
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
            let painter = ui.painter_at(rect);
            painter.image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            let at = |p: egui::Pos2| image_fraction([p.x, p.y], rect.min.into(), size.into());
            if response.drag_started()
                && let Some(f) = response.interact_pointer_pos().and_then(at)
            {
                w.marks.push(Mark {
                    subject: w.mark_subject,
                    points: vec![f],
                    width: w.mark_width,
                });
            } else if response.dragged()
                && let Some(f) = response.interact_pointer_pos().and_then(at)
                && let Some(m) = w.marks.last_mut()
                && m.points.last() != Some(&f)
            {
                m.points.push(f);
            }
            let short = size.x.min(size.y);
            for m in &w.marks {
                let colour = if m.subject {
                    egui::Color32::from_rgba_unmultiplied(40, 200, 60, 140)
                } else {
                    egui::Color32::from_rgba_unmultiplied(220, 40, 40, 140)
                };
                let pts: Vec<egui::Pos2> = m
                    .points
                    .iter()
                    .map(|p| rect.min + egui::vec2(p[0] as f32 * size.x, p[1] as f32 * size.y))
                    .collect();
                let width = (m.width as f32 * short).max(1.0);
                for p in &pts {
                    painter.circle_filled(*p, width / 2.0, colour);
                }
                painter.add(egui::Shape::line(pts, egui::Stroke::new(width, colour)));
            }
            ui.weak("Drag to brush. Convert runs the cut with these strokes.");
        });
    app.wizard.marking = open;
}

/// OpenStitch mode's own options per tool (B16.1): Cross Stitch's filled cells, Design
/// Center's automatic threshold and lines, Photo Stitch 1's trims.
fn open_stitch_rows(ui: &mut egui::Ui, w: &mut ImageWizard) {
    match w.method {
        Method::AutoPunch => {
            ui.label(t("ui-preset"));
            let label = |p: Option<&str>| {
                PRESETS
                    .iter()
                    .find(|k| Some(k.0) == p)
                    .map_or("None", |k| k.1)
            };
            egui::ComboBox::from_id_salt("wizardPreset")
                .selected_text(label(w.preset))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut w.preset, None, "None");
                    for (key, name) in PRESETS {
                        ui.selectable_value(&mut w.preset, Some(key), name);
                    }
                })
                .response
                .on_hover_text(
                    "Sets several options at once for what the picture is for; options you set \
                     yourself here still win.",
                );
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut w.vary_angles, t("ui-vary-angles-of-touching-fills"))
                .on_hover_text(
                    "A fill within 20° of a touching fill sewn before it (both at least 20 mm²) is \
                     turned as far from it as it can be, so fitted panels read apart.",
                );
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut w.flow_fill, t("ui-rows-follow-the-picture"))
                .on_hover_text(
                    "Each fill's rows curve along the picture's own strokes (fur, petals, hair) \
                     instead of one straight angle; a flat shape, or a fill without clear strokes \
                     inside it, keeps straight rows.",
                );
            ui.end_row();
        }
        Method::CrossStitch => {
            ui.label("");
            ui.checkbox(&mut w.cross_fill, t("ui-fill-each-cell"))
                .on_hover_text(t("ui-parallel-diagonals-about-0-3-mm-tip"));
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut w.part_crosses, t("ui-part-crosses-on-edges"))
                .on_hover_text(
                    "A cell split by an edge becomes a three-quarter and a quarter cross.",
                );
            ui.end_row();
        }
        Method::DesignCenter => {
            ui.label("");
            ui.checkbox(&mut w.auto_lines, t("ui-automatic-threshold-and-lines"))
                .on_hover_text(
                    "The threshold by Otsu's method; lines sewn as wide as the drawing's.",
                );
            ui.end_row();
        }
        Method::PhotoFill if !w.mono => {
            ui.label(t("ui-trims"));
            ui.horizontal(|ui| {
                let mut all = w.all_trims();
                ui.radio_value(&mut all, false, t("ui-fewer")).on_hover_text(
                    "Short travel stitches up to 2 mm instead of about half the trims.",
                );
                ui.radio_value(&mut all, true, t("ui-all"))
                    .on_hover_text(t("ui-every-jump-trimmed-the-best-likeness-tip"));
                if all != w.all_trims() {
                    w.all_trims = Some(all);
                }
            });
            ui.end_row();
        }
        _ => {}
    }
}

/// The Threads and "I own" rows (B16.1 T5): where OpenStitch mode chooses its threads (the
/// Brother chart, every built-in chart, or one of the user's charts), and which of the user's
/// charts holds the threads they own, so the result can list the rest to buy.
fn threads_rows(ui: &mut egui::Ui, w: &mut ImageWizard, charts: &[String]) {
    // The thread makers' charts (New Brothread first, #796) by name.
    let brands: Vec<(&'static str, &'static str)> = openstitch_core::brands::brand_charts()
        .iter()
        .map(|(id, name, _)| (*id, *name))
        .collect();
    let label = |v: &str| match v {
        "brother" => "Brother".to_string(),
        "all" => "All built-in charts".to_string(),
        other => brands
            .iter()
            .find(|(id, _)| *id == other)
            .map_or(other.to_string(), |(_, n)| n.to_string()),
    };
    ui.label(t("ui-threads"));
    egui::ComboBox::from_id_salt("wizardThreads")
        .selected_text(label(&w.threads))
        .show_ui(ui, |ui| {
            for v in ["brother", "all"]
                .into_iter()
                .chain(brands.iter().map(|(id, _)| *id))
                .map(str::to_string)
                .chain(charts.iter().cloned())
            {
                let text = label(&v);
                if ui.selectable_value(&mut w.threads, v, text).clicked() {
                    w.threads_chosen = true;
                }
            }
        });
    ui.end_row();
    ui.label(t("ui-i-own"));
    egui::ComboBox::from_id_salt("wizardOwned")
        .selected_text(w.owned.clone().unwrap_or_else(|| "(not set)".into()))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut w.owned, None, "(not set)");
            for c in charts {
                ui.selectable_value(&mut w.owned, Some(c.clone()), c);
            }
        })
        .response
        .on_hover_text(t("ui-one-of-your-thread-charts-threads-tip"));
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
