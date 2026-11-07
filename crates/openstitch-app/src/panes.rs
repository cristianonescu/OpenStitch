//! The window's panes: Sewing Order (left), Import / Color / Sewing Attributes / Text
//! Attributes (right), Stitch Simulator (under the work area) and the status bar. Each pane
//! can be hidden from View ▸ Show/Hide and floated as a window or docked again.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use crate::ribbon::{self, PaneTab};
use eframe::egui::{self, Color32, RichText, Sense, Stroke, vec2};
use openstitch_app::editor::prefs::PaneState;
use openstitch_app::editor::ribbon::Action;
use openstitch_design::{DesignObject, ObjectKind};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    SewingOrder,
    Attributes,
    Simulator,
}

impl Pane {
    fn title(self) -> &'static str {
        match self {
            Pane::SewingOrder => "Sewing Order",
            Pane::Attributes => "Attributes",
            Pane::Simulator => "Stitch Simulator",
        }
    }

    fn state(self, app: &mut EditorApp) -> &mut PaneState {
        let p = &mut app.ui.prefs.panes;
        match self {
            Pane::SewingOrder => &mut p.sewing_order,
            Pane::Attributes => &mut p.attributes,
            Pane::Simulator => &mut p.simulator,
        }
    }

    fn contents(self, app: &mut EditorApp, ui: &mut egui::Ui) {
        match self {
            Pane::SewingOrder => crate::panels::object_list(app, ui),
            Pane::Attributes => attributes(app, ui),
            Pane::Simulator => crate::panels::simulator(app, ui),
        }
    }
}

/// Title row of a pane with the float/dock and close buttons.
fn header(app: &mut EditorApp, ui: &mut egui::Ui, pane: Pane) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(pane.title()).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut changed = false;
            let close = egui::Button::new(crate::icons::ui_image(ui, "close")).small();
            if ui
                .add(close)
                .on_hover_text(t("ui-hide-view-show-hide-shows-it-tip"))
                .clicked()
            {
                pane.state(app).visible = false;
                changed = true;
            }
            let floating = pane.state(app).floating;
            let (icon, tip) = if floating {
                ("pane_dock", "Dock to the window")
            } else {
                ("pane_float", "Float as a window")
            };
            let button = egui::Button::new(crate::icons::ui_image(ui, icon)).small();
            if ui.add(button).on_hover_text(tip).clicked() {
                pane.state(app).floating = !floating;
                changed = true;
            }
            if changed {
                app.ui.save_prefs();
            }
        });
    });
}

fn floating(
    app: &mut EditorApp,
    ctx: &egui::Context,
    pane: Pane,
    pos: egui::Pos2,
    size: egui::Vec2,
) {
    let s = *pane.state(app);
    if !(s.visible && s.floating) {
        return;
    }
    let mut open = true;
    egui::Window::new(pane.title())
        .open(&mut open)
        .default_pos(pos)
        .default_size(size)
        .resizable(true)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                let dock =
                    egui::Button::new((crate::icons::ui_image(ui, "pane_dock"), "Dock")).small();
                if ui.add(dock).on_hover_text(t("ui-dock-to-the-window-tip")).clicked() {
                    pane.state(app).floating = false;
                    app.ui.save_prefs();
                }
            });
            pane.contents(app, ui);
        });
    if !open {
        pane.state(app).visible = false;
        app.ui.save_prefs();
    }
}

/// An egui panel on `side` (#551: panes dock on either side).
fn panel(id: &'static str, side: openstitch_app::editor::prefs::Side) -> egui::Panel {
    use openstitch_app::editor::prefs::Side;
    match side {
        Side::Left => egui::Panel::left(id),
        Side::Right => egui::Panel::right(id),
        Side::Top => egui::Panel::top(id),
        Side::Bottom => egui::Panel::bottom(id),
    }
}

/// Docks the visible panes around the work area (call before the central panel).
pub fn docked(app: &mut EditorApp, ui: &mut egui::Ui) {
    let docked = |app: &mut EditorApp, p: Pane| {
        let s = *p.state(app);
        s.visible && !s.floating
    };
    if docked(app, Pane::Simulator) {
        panel("simulator", app.ui.prefs.panes.simulator.side).show(ui, |ui| {
            header(app, ui, Pane::Simulator);
            crate::panels::simulator(app, ui);
        });
    }
    if docked(app, Pane::SewingOrder) {
        panel("objects", app.ui.prefs.panes.sewing_order.side)
            .default_size(250.0)
            .show(ui, |ui| {
                header(app, ui, Pane::SewingOrder);
                ui.separator();
                crate::panels::object_list(app, ui);
            });
    }
    if docked(app, Pane::Attributes) {
        panel("inspector", app.ui.prefs.panes.attributes.side)
            .default_size(330.0)
            .show(ui, |ui| attributes(app, ui));
    }
}

/// The floating panes (call after the central panel).
pub fn floating_panes(app: &mut EditorApp, ctx: &egui::Context) {
    let screen = ctx.content_rect();
    let top = screen.min.y + 190.0;
    floating(
        app,
        ctx,
        Pane::SewingOrder,
        egui::pos2(screen.min.x + 12.0, top),
        vec2(260.0, 420.0),
    );
    floating(
        app,
        ctx,
        Pane::Attributes,
        egui::pos2(screen.max.x - 340.0, top),
        vec2(320.0, 480.0),
    );
    floating(
        app,
        ctx,
        Pane::Simulator,
        egui::pos2(screen.min.x + 300.0, screen.max.y - 180.0),
        vec2(720.0, 110.0),
    );
}

/// Import / Color / Sewing Attributes / Text Attributes, as tabs.
pub fn attributes(app: &mut EditorApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        ui.spacing_mut().button_padding.x = 3.0;
        for t in PaneTab::ALL {
            if ui
                .selectable_label(app.ui.pane_tab == t, RichText::new(t.label()).size(12.0))
                .clicked()
            {
                app.ui.pane_tab = t;
            }
        }
        if !app.ui.prefs.panes.attributes.floating {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let close = egui::Button::new(crate::icons::ui_image(ui, "close")).small();
                if ui.add(close).on_hover_text(t("ui-hide-tip")).clicked() {
                    app.ui.prefs.panes.attributes.visible = false;
                    app.ui.save_prefs();
                }
                let float = egui::Button::new(crate::icons::ui_image(ui, "pane_float")).small();
                if ui.add(float).on_hover_text(t("ui-float-as-a-window-tip")).clicked() {
                    app.ui.prefs.panes.attributes.floating = true;
                    app.ui.save_prefs();
                }
            });
        }
    });
    ui.separator();
    match app.ui.pane_tab {
        PaneTab::Import => import_tab(app, ui),
        PaneTab::Color => color_tab(app, ui),
        PaneTab::Sewing => crate::panels::sewing_attributes(app, ui),
        PaneTab::Text => text_tab(app, ui),
    }
}

fn pane_command(app: &mut EditorApp, ui: &mut egui::Ui, a: Action) {
    let info = a.info();
    let r = ui
        .add_enabled(ribbon::enabled(app, a), egui::Button::new(info.label))
        .on_hover_text(info.screen_tip(cfg!(target_os = "macos")));
    if r.clicked() {
        ribbon::run(app, a, ui.ctx());
    }
}

fn import_tab(app: &mut EditorApp, ui: &mut egui::Ui) {
    ui.label(t("ui-add-designs-artwork-or-a-lettering"));
    ui.add_space(4.0);
    ui.vertical(|ui| {
        pane_command(app, ui, Action::ImportPatterns);
        pane_command(app, ui, Action::ImportSvg);
        pane_command(app, ui, Action::ImportPesObjects);
        pane_command(app, ui, Action::OpenImage);
        pane_command(app, ui, Action::DesignDatabase);
        if ui.button(t("ui-template-wizard")).clicked() {
            crate::lettering_ui::open_template_wizard(app);
        }
    });
    let recent = app.ui.prefs.recent.clone();
    if !recent.is_empty() {
        ui.separator();
        ui.label(RichText::new(t("ui-import-a-recent-file")).strong());
        for p in recent
            .iter()
            .filter(|p| openstitch_formats::FileType::from_path(p).is_some() && p.exists())
        {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
            if ui
                .add(egui::Button::new(name.unwrap_or_default()).frame(false))
                .on_hover_text(p.display().to_string())
                .clicked()
                && app.grouped(|a| a.import(p))
            {
                app.status = format!("Imported {}", p.display());
            }
        }
    }
}

fn color_tab(app: &mut EditorApp, ui: &mut egui::Ui) {
    if !app.selection.is_empty() {
        ui.label(RichText::new(t("ui-selection")).strong());
        ui.horizontal(|ui| {
            pane_menu(app, ui, Action::LineColor);
            pane_menu(app, ui, Action::RegionColor);
        });
        ui.separator();
    }
    ui.label(RichText::new(t("ui-thread-colours-in-sewing-order")).strong());
    let colors = app.pattern.threads.clone();
    let blocks = app.stats.colors.clone();
    if blocks.is_empty() {
        crate::art::notice(ui, crate::art::Art::SewEmpty, 64.0, "Nothing is sewn yet");
    }
    egui::ScrollArea::vertical()
        .max_height(ui.available_height() - 40.0)
        .show(ui, |ui| {
            for (k, c) in blocks.iter().enumerate() {
                ui.horizontal(|ui| {
                    crate::panels::swatch(ui, *c, 16.0);
                    let name = colors
                        .get(k)
                        .and_then(|t| t.name.as_deref().map(str::to_string))
                        .unwrap_or_else(|| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
                    ui.label(format!("{}. {name}", k + 1));
                });
            }
        });
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        pane_command(app, ui, Action::UserThreadCharts);
        if ui
            .button(t("ui-sort-by-colour"))
            .on_hover_text(t("ui-same-colour-objects-sew-together-tip"))
            .clicked()
        {
            app.execute(&openstitch_app::editor::CommandRequest::new(
                "order.byColor",
                serde_json::json!({}),
            ));
        }
    });
}

/// A ribbon dropdown command shown as a button in a pane.
fn pane_menu(app: &mut EditorApp, ui: &mut egui::Ui, a: Action) {
    let info = a.info();
    let r = ui
        .add_enabled(
            ribbon::enabled(app, a),
            egui::Button::new((info.label, crate::icons::ui_image(ui, "menu_caret"))),
        )
        .on_hover_text(info.tip);
    egui::Popup::menu(&r).show(|ui| ribbon::menu(app, a, ui));
}

/// Text Attributes ▸ Transform: the check box and PE-DESIGN's 26 presets (the two arches,
/// then four a row), each drawn from its own envelope. Ticking it applies preset 3, as in
/// PE-DESIGN; fitted text cannot be transformed.
fn transform_palette(app: &mut EditorApp, ui: &mut egui::Ui, spec: &openstitch_design::TextSpec) {
    use openstitch_design::text;
    let fitted = spec.path.is_some();
    let mut on = spec.envelope.is_some();
    let r = ui
        .add_enabled(!fitted, egui::Checkbox::new(&mut on, "Transform"))
        .on_disabled_hover_text("Release the text from its outline first");
    if r.changed() {
        let v = if on {
            serde_json::json!(3)
        } else {
            serde_json::Value::Null
        };
        ribbon::set_text(app, serde_json::json!({ "envelope": v }));
    }
    if !on || fitted {
        return;
    }
    let fg = ui.visuals().text_color();
    let accent = ui.visuals().selection.bg_fill;
    let mut chosen = None;
    let rows: [&[u8]; 7] = [
        &[1, 2],
        &[3, 4, 5, 6],
        &[7, 8, 9, 10],
        &[11, 12, 13, 14],
        &[15, 16, 17, 18],
        &[19, 20, 21, 22],
        &[23, 24, 25, 26],
    ];
    for row in rows {
        ui.horizontal(|ui| {
            for &n in row {
                let edges = text::envelope_edges(n);
                let size = egui::vec2(38.0, 22.0);
                let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
                let selected = spec.envelope == Some(n);
                let p = ui.painter();
                if selected {
                    p.rect_filled(rect, 3.0, accent.gamma_multiply(0.35));
                } else if resp.hovered() && edges.is_some() {
                    p.rect_filled(rect, 3.0, ui.visuals().widgets.hovered.weak_bg_fill);
                }
                let inner = rect.shrink2(egui::vec2(5.0, 4.0));
                let at = |q: openstitch_design::Vec2| {
                    egui::pos2(
                        inner.min.x + q.x as f32 * inner.width(),
                        inner.min.y + q.y as f32 * inner.height(),
                    )
                };
                match &edges {
                    Some((top, bottom)) => {
                        let stroke = egui::Stroke::new(1.4, fg);
                        let line = |e: &[openstitch_design::Vec2]| -> Vec<egui::Pos2> {
                            e.iter().map(|&q| at(q)).collect()
                        };
                        p.add(egui::Shape::line(line(top), stroke));
                        p.add(egui::Shape::line(line(bottom), stroke));
                        for (a, b) in [
                            (top[0], bottom[0]),
                            (top[top.len() - 1], bottom[bottom.len() - 1]),
                        ] {
                            p.line_segment([at(a), at(b)], stroke);
                        }
                    }
                    None => {
                        p.rect_stroke(
                            inner,
                            2.0,
                            egui::Stroke::new(1.0, fg.gamma_multiply(0.3)),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                let tip = if edges.is_some() {
                    format!("Preset {n}")
                } else {
                    format!("Preset {n}: not yet available in OpenStitch")
                };
                if resp.on_hover_text(tip).clicked() && edges.is_some() {
                    chosen = Some(n);
                }
            }
        });
    }
    if let Some(n) = chosen.filter(|&n| spec.envelope != Some(n)) {
        ribbon::set_text(app, serde_json::json!({ "envelope": n }));
    }
}

fn text_tab(app: &mut EditorApp, ui: &mut egui::Ui) {
    let units = app.ui.prefs.units;
    let d = app.session.design();
    let first = crate::convert_ui::texts(app)
        .first()
        .and_then(|&id| d.index_of(id))
        .and_then(|i| d.objects[i].text.clone());
    let Some(spec) = first else {
        ui.label(RichText::new(t("ui-select-a-text-object-to-change")).weak());
        ui.add_space(6.0);
        crate::lettering_ui::text_tools_menu(app, ui);
        return;
    };
    ui.label(RichText::new(t("ui-text")).strong());
    if app.ui.text_draft.is_empty() {
        app.ui.text_draft = spec.text.clone();
    }
    ui.text_edit_multiline(&mut app.ui.text_draft);
    if ui
        .add_enabled(
            app.ui.text_draft != spec.text,
            egui::Button::new(t("ui-apply-text")),
        )
        .clicked()
    {
        let t = app.ui.text_draft.clone();
        ribbon::set_text(app, serde_json::json!({ "text": t }));
    }
    ui.separator();
    egui::Grid::new("text-attributes")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(t("ui-font"));
            pane_menu(app, ui, Action::Font);
            ui.end_row();
            ui.label("");
            ui.label(RichText::new(&spec.font).weak());
            ui.end_row();
            ui.label(t("ui-size"));
            let mut size = spec.size;
            let warning = crate::lettering_ui::size_warning(app, &spec);
            let r = ui
                .scope(|ui| {
                    if warning.is_some() {
                        // Amber: outside the sizes the font sews well at (#527).
                        ui.visuals_mut().override_text_color =
                            Some(egui::Color32::from_rgb(0xE0, 0x9A, 0x1A));
                    }
                    ui.add(crate::units_ui::drag(&mut size, units).range(1.0..=200.0))
                })
                .inner;
            let r = match &warning {
                Some(w) => r.on_hover_text(w),
                None => r,
            };
            if (r.drag_stopped() || r.lost_focus()) && (size - spec.size).abs() > 1e-9 {
                ribbon::set_text(app, serde_json::json!({ "size": size }));
            }
            ui.end_row();
            ui.label(t("ui-character-spacing"));
            let mut letter = spec.letter_spacing;
            let r = ui.add(
                crate::units_ui::drag(&mut letter, units)
                    .range(-100.0..=100.0)
                    .speed(0.1),
            );
            if (r.drag_stopped() || r.lost_focus()) && (letter - spec.letter_spacing).abs() > 1e-9 {
                ribbon::set_text(app, serde_json::json!({ "letterSpacing": letter }));
            }
            ui.end_row();
            ui.label(t("ui-line-spacing"));
            let mut line = spec.line_spacing_percent;
            let r = ui.add(
                egui::DragValue::new(&mut line)
                    .range(0.0..=500.0)
                    .suffix(" %"),
            );
            if (r.drag_stopped() || r.lost_focus())
                && (line - spec.line_spacing_percent).abs() > 1e-9
            {
                ribbon::set_text(app, serde_json::json!({ "lineSpacingPercent": line }));
            }
            ui.end_row();
            if spec.path.is_none() && spec.direction == openstitch_design::TextDirection::Horizontal
            {
                crate::arc_ui::arc_row(app, ui, &spec);
                ui.end_row();
            }
        });
    if spec.font == "openstitch-pixel" {
        ui.horizontal(|ui| {
            ui.label(t("ui-pixels"));
            for (v, label) in [("run", "Run"), ("cross", "Cross"), ("block", "Block")] {
                if ui
                    .selectable_label(spec.pixel_style.as_str() == v, label)
                    .clicked()
                {
                    ribbon::set_text(app, serde_json::json!({ "pixelStyle": v }));
                }
            }
        });
    }
    egui::CollapsingHeader::new("Font list")
        .default_open(true)
        .show(ui, |ui| crate::lettering_ui::font_browser(app, ui, &spec));
    transform_palette(app, ui, &spec);
    crate::envelope_ui::pane(app, ui, &spec);
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        for a in [
            Action::LineColor,
            Action::LineSewType,
            Action::RegionColor,
            Action::RegionSewType,
        ] {
            pane_menu(app, ui, a);
        }
    });
    ui.horizontal_wrapped(|ui| {
        pane_menu(app, ui, Action::FitTextToOutline);
        pane_command(app, ui, Action::ConvertToOutline);
        pane_command(app, ui, Action::ConvertToStitches);
    });
}

/// A small drawing of an object's outlines or stitches for the Sewing Order pane.
pub fn thumbnail(ui: &mut egui::Ui, o: &DesignObject, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, Color32::WHITE);
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    let Some(b) = o.bounds() else { return resp };
    let inner = rect.shrink(3.0);
    let k = (f64::from(inner.width()) / b.width().max(1.0))
        .min(f64::from(inner.height()) / b.height().max(1.0));
    let ox = f64::from(inner.center().x) - (b.min_x + b.width() / 2.0) * k;
    let oy = f64::from(inner.center().y) - (b.min_y + b.height() / 2.0) * k;
    let at = |x: f64, y: f64| egui::pos2((x * k + ox) as f32, (y * k + oy) as f32);
    let rgb = |t: &openstitch_core::Thread| Color32::from_rgb(t.red, t.green, t.blue);
    if o.kind == ObjectKind::Stitches {
        if let Some(block) = &o.stitch_block {
            let step = (block.stitches.len() / 400).max(1);
            let color = block.threads.first().map_or(Color32::GRAY, rgb);
            let pts: Vec<egui::Pos2> = block
                .stitches
                .iter()
                .step_by(step)
                .map(|s| at(s.x, s.y))
                .collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.0, color)));
        }
        return resp;
    }
    let fill = o.region.as_ref().map(|r| rgb(&r.thread));
    let line = o
        .line
        .as_ref()
        .map_or(Color32::from_gray(90), |l| rgb(&l.thread));
    for c in &o.contours {
        let pts: Vec<egui::Pos2> = c.points.iter().map(|p| at(p.x, p.y)).collect();
        if pts.len() < 2 {
            continue;
        }
        if c.closed {
            if let Some(f) = fill {
                // Outline only filled lightly: holes and concave shapes stay readable.
                painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(2.5, f)));
            }
            painter.add(egui::Shape::closed_line(pts, Stroke::new(1.0, line)));
        } else {
            painter.add(egui::Shape::line(pts, Stroke::new(1.0, line)));
        }
    }
    resp
}

/// Design size and stitch count (or the command under the pointer) on the left; view mode,
/// grid, Design Property and zoom on the right.
pub fn status_bar(app: &mut EditorApp, ui: &mut egui::Ui) {
    // A narrow window or a large Text size (#499): the controls take a row of their own
    // instead of covering the size, count and hints.
    let narrow = ui.available_width() < STATUS_CONTROLS_WIDTH * 1.6;
    ui.horizontal(|ui| {
        let s = &app.stats;
        ui.label(openstitch_app::editor::units::size(
            app.ui.prefs.units,
            s.width_mm,
            s.height_mm,
        ))
        .on_hover_text(t("ui-design-size-tip"));
        ui.separator();
        ui.label(format!("{} stitches", s.stitches));
        ui.separator();
        // The hints line (#550): what clicks and keys do with this tool and the keys held.
        let held = ui.input(|i| openstitch_app::editor::hints::Held {
            shift: i.modifiers.shift,
            command: i.modifiers.command,
            alt: i.modifiers.alt,
        });
        let hint = openstitch_app::editor::hints::hint(app.tools.tool(), held);
        let any_held = held != openstitch_app::editor::hints::Held::default();
        let text = match app.ui.hover {
            Some(h) => h.to_string(),
            None if any_held || app.status == "Ready" => hint.to_string(),
            None => format!("{}  ·  {hint}", app.status),
        };
        if narrow {
            ui.add(egui::Label::new(RichText::new(text).weak()).truncate());
            return;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            status_controls(app, ui);
            ui.separator();
            // The message and hints take what is left, cut short rather than under the buttons.
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(text).weak()).truncate());
            });
        });
    });
    if narrow {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            status_controls(app, ui);
        });
    }
}

/// About how wide the status bar's controls are (points); below 1.6 times that they get a row
/// of their own.
const STATUS_CONTROLS_WIDTH: f32 = 640.0;

/// The status bar's right-hand controls (drawn right to left): zoom, problems, Design
/// Property, Show Grid and the views.
fn status_controls(app: &mut EditorApp, ui: &mut egui::Ui) {
    zoom_controls(app, ui);
    ui.separator();
    crate::quality_ui::indicator(app, ui);
    ui.separator();
    for a in [Action::DesignProperty, Action::ShowGrid] {
        let info = a.info();
        let on = ribbon::active(app, a);
        if ui
            .add(egui::Button::new(info.label).selected(on))
            .on_hover_text(info.tip)
            .clicked()
        {
            ribbon::run(app, a, ui.ctx());
        }
    }
    ui.separator();
    for a in [Action::RealisticView, Action::StitchView, Action::SolidView] {
        let info = a.info();
        let label = info.label.trim_end_matches(" View");
        let r = ui
            .add_enabled(
                ribbon::enabled(app, a),
                egui::Button::new(label).selected(ribbon::active(app, a)),
            )
            .on_hover_text(info.screen_tip(cfg!(target_os = "macos")))
            .on_disabled_hover_text(info.screen_tip(cfg!(target_os = "macos")));
        if r.clicked() {
            ribbon::run(app, a, ui.ctx());
        }
    }
}

fn zoom_controls(app: &mut EditorApp, ui: &mut egui::Ui) {
    let Some(vp) = app.viewport else { return };
    let ppm = app.points_per_mm();
    let now = vp.zoom_percent(ppm);
    // Slider on a log scale, 10 %–2000 %.
    let mut log = now.clamp(10.0, 2000.0).log10();
    ui.spacing_mut().slider_width = 110.0;
    if crate::a11y::named(
        ui.add(egui::Slider::new(&mut log, 1.0..=(2000f64).log10()).show_value(false)),
        "Zoom",
    )
    .on_hover_text(t("ui-zoom-tip"))
    .changed()
    {
        ribbon::zoom_to_percent(app, 10f64.powf(log));
    }
    let mut p = now.round();
    if ui
        .add(
            egui::DragValue::new(&mut p)
                .range(10.0..=2000.0)
                .suffix(" %")
                .speed(1.0),
        )
        .on_hover_text(t("ui-magnification-type-a-value-or-drag-tip"))
        .changed()
    {
        ribbon::zoom_to_percent(app, p);
    }
}

/// Design Property, Keyboard Shortcuts and About windows.
pub fn windows(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.ui.property_open;
    egui::Window::new("Design Property")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            crate::panels::design_info(app, ui);
            ui.separator();
            let s = &app.stats;
            egui::Grid::new("property").num_columns(2).show(ui, |ui| {
                for (k, v) in [
                    (
                        "Size",
                        openstitch_app::editor::units::size(
                            app.ui.prefs.units,
                            s.width_mm,
                            s.height_mm,
                        ),
                    ),
                    ("Stitches", s.stitches.to_string()),
                    ("Colours", s.colors.len().to_string()),
                    ("Colour changes", s.color_changes.to_string()),
                    ("Jumps", s.jumps.to_string()),
                    ("Trims", s.trims.to_string()),
                ] {
                    ui.label(k);
                    ui.label(v);
                    ui.end_row();
                }
            });
            ui.separator();
            property_fields(app, ui);
        });
    if !open {
        app.ui.property_draft = None;
    }
    app.ui.property_open = open;
    crate::keymap_ui::window(app, ctx);
    about_window(app, ctx);
}

/// Design Property: category, author, keywords and comments, saved into PES files.
fn property_fields(app: &mut EditorApp, ui: &mut egui::Ui) {
    let current = app.session.design().properties.clone();
    let draft = app.ui.property_draft.get_or_insert_with(|| current.clone());
    egui::Grid::new("property_fields")
        .num_columns(2)
        .show(ui, |ui| {
            for (label, key) in [
                ("Category", "category"),
                ("Author", "author"),
                ("Keywords", "keywords"),
                ("Comments", "comments"),
            ] {
                ui.label(label);
                if let Some(field) = draft.field_mut(key) {
                    let edit = if key == "comments" {
                        egui::TextEdit::multiline(field).desired_rows(3)
                    } else {
                        egui::TextEdit::singleline(field)
                    };
                    let r = ui.add(edit.char_limit(openstitch_design::DesignProperties::MAX_BYTES));
                    crate::a11y::name(&r, label);
                }
                ui.end_row();
            }
        });
    let changed = *draft != current;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(changed, egui::Button::new(t("menu-apply")))
            .clicked()
        {
            apply_property_draft(app);
        }
        if ui
            .add_enabled(changed, egui::Button::new(t("ui-revert")))
            .clicked()
        {
            app.ui.property_draft = None;
        }
    });
}

/// Writes the Design Property draft to the design (`design.setProperties`, one Undo step).
/// Keeps the draft when the engine refuses it; the status bar says why.
pub fn apply_property_draft(app: &mut EditorApp) -> bool {
    let Some(draft) = app.ui.property_draft.clone() else {
        return false;
    };
    let params = serde_json::Value::Object(
        draft
            .fields()
            .iter()
            .map(|&(k, v)| (k.to_string(), serde_json::Value::from(v)))
            .collect(),
    );
    let request = openstitch_app::editor::CommandRequest::new("design.setProperties", params);
    let ok = app.execute(&request).is_some();
    if ok {
        app.ui.property_draft = None;
    }
    ok
}

/// Help ▸ About (every window).
pub fn about_window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.ui.about_open;
    egui::Window::new("About OpenStitch")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let (name, src) = if ui.visuals().dark_mode {
                ("wordmark-dark", crate::icons::WORDMARK_DARK)
            } else {
                ("wordmark", crate::icons::WORDMARK)
            };
            let (r, _) = ui.allocate_exact_size(egui::vec2(260.0, 58.0), egui::Sense::hover());
            if !crate::icons::paint_svg_logo(ui.painter(), r, name, src) {
                ui.heading(t("ui-openstitch"));
            }
            ui.label(crate::update_ui::version_line());
            ui.label(t("ui-an-open-source-embroidery-design-suite"));
            ui.label(t("ui-reads-and-writes-pes-pec-dst"));
            ui.hyperlink("https://github.com/cristianonescu/OpenStitch");
        });
    app.ui.about_open = open;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;

    #[test]
    fn design_property_edits_apply_as_one_undo_step() {
        let mut app = EditorApp::new(Launch::default());
        let mut draft = app.session.design().properties.clone();
        draft.author = "Ana".into();
        draft.comments = "two\nlines".into();
        app.ui.property_draft = Some(draft.clone());
        assert!(apply_property_draft(&mut app));
        assert_eq!(app.session.design().properties, draft);
        assert_eq!(app.ui.property_draft, None);

        // A refused edit keeps the draft and leaves the design alone.
        let mut bad = draft.clone();
        bad.category = "x".repeat(300);
        app.ui.property_draft = Some(bad.clone());
        assert!(!apply_property_draft(&mut app));
        assert_eq!(app.ui.property_draft, Some(bad));
        assert_eq!(app.session.design().properties, draft);
    }
}
