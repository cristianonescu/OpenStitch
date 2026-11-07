//! View, Image and Text ribbon commands that need their own windows or canvas drawing: Solid
//! View, Realistic View Settings, guidelines dragged out of the rulers, Save / Modify / Rotate
//! Image, Font Filter Settings and the Fabric Selector. Only the image, guideline and fabric
//! commands change the design, through `background.*`, `guides.set` and `fabric.apply`.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use crate::workspace_ui::RULER;
use eframe::egui::{self, Color32, Pos2, Stroke};
use openstitch_app::editor::prefs::FontFilter;
use openstitch_app::editor::workspace::{
    GuideDrag, RealisticSettings, SOLID_WIDTH_MM, guide_near, remove_guide, snap_guide,
};
use openstitch_app::editor::{CommandRequest, Viewport};
use openstitch_design::{Guides, ImageAdjust, Vec2, v2};
use serde_json::{Value, json};

const GUIDE: Color32 = Color32::from_rgb(0x00, 0x9e, 0xc8);
const IMAGE_FRAME: Color32 = Color32::from_rgb(0xe5, 0x39, 0x35);
/// Pointer distance (points) that picks a guideline marker or its ×.
const PICK: f32 = 5.0;

/// Open windows and the guideline being dragged.
#[derive(Default)]
pub struct ViewState {
    pub realistic_open: bool,
    pub image_open: bool,
    image_draft: Option<ImageDraft>,
    pub fonts_open: bool,
    fonts_draft: Option<FontFilter>,
    pub fabric_open: bool,
    fabric: Option<String>,
    /// Fabric Selector: every shape and text pattern (else the selection).
    fabric_all: Option<bool>,
    /// Fabric Selector: thread weight No. (#508; default 40).
    fabric_weight: Option<u32>,
    /// Fabric Selector: new shapes and texts start from this fabric (`fabric.setDefault`).
    fabric_new: Option<bool>,
    pub guide_drag: Option<GuideDrag>,
}

fn pos(p: Vec2) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

fn mm(list: &[f64]) -> Vec<f64> {
    list.iter()
        .map(|v| (v / 10.0 * 100.0).round() / 100.0)
        .collect()
}

// MARK: canvas

/// Solid View: each colour block drawn as wide, unshaded thread so the rows merge into flat
/// areas of colour; round joins keep satin turns from spiking.
pub fn draw_solid(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let w = (SOLID_WIDTH_MM * 10.0 * vp.scale).max(1.0) as f32;
    for layer in &app.render.layers {
        let [r, g, b] = layer.rgb;
        let color = Color32::from_rgb(r, g, b);
        for s in &layer.strokes {
            let pts: Vec<Pos2> = s.iter().map(|&p| pos(vp.to_screen(p))).collect();
            for pair in pts.windows(2) {
                painter.line_segment([pair[0], pair[1]], Stroke::new(w, color));
            }
            if w >= 2.0 {
                for &p in &pts {
                    painter.circle_filled(p, w / 2.0, color);
                }
            }
        }
    }
}

fn guides_shown(app: &EditorApp) -> bool {
    app.workspace.rulers && app.workspace.guides
}

/// The guidelines, with the one being dragged at its new place.
fn current_guides(app: &EditorApp) -> Guides {
    let g = &app.session.design().guides;
    app.workspace
        .view
        .guide_drag
        .map_or_else(|| g.clone(), |d| d.placed(g))
}

/// Guidelines across the canvas (drawn under the rulers).
pub fn draw_guides(app: &EditorApp, painter: &egui::Painter, rect: egui::Rect, vp: &Viewport) {
    if !guides_shown(app) {
        return;
    }
    let g = current_guides(app);
    let stroke = Stroke::new(1.0, GUIDE);
    for &x in &g.vertical {
        let sx = vp.to_screen(v2(x, 0.0)).x as f32;
        painter.line_segment(
            [Pos2::new(sx, rect.min.y), Pos2::new(sx, rect.max.y)],
            stroke,
        );
    }
    for &y in &g.horizontal {
        let sy = vp.to_screen(v2(0.0, y)).y as f32;
        painter.line_segment(
            [Pos2::new(rect.min.x, sy), Pos2::new(rect.max.x, sy)],
            stroke,
        );
    }
}

/// Where a guideline's marker and its delete × sit in the ruler.
fn marker_points(rect: egui::Rect, vertical: bool, at: f32) -> (Pos2, Pos2) {
    if vertical {
        (
            Pos2::new(at, rect.min.y + RULER - 4.0),
            Pos2::new(at + 8.0, rect.min.y + 6.0),
        )
    } else {
        (
            Pos2::new(rect.min.x + RULER - 4.0, at),
            Pos2::new(rect.min.x + 6.0, at + 8.0),
        )
    }
}

/// Guideline markers (drag to move) and × (click to delete) in the rulers.
pub fn draw_guide_markers(
    app: &EditorApp,
    painter: &egui::Painter,
    rect: egui::Rect,
    vp: &Viewport,
) {
    if !guides_shown(app) {
        return;
    }
    let g = current_guides(app);
    let ruler = crate::theme::canvas(painter.ctx()).ruler_fg;
    let draw = |vertical: bool, at: f32| {
        let (m, x) = marker_points(rect, vertical, at);
        let tri = if vertical {
            vec![
                m + egui::vec2(-4.0, -4.0),
                m + egui::vec2(4.0, -4.0),
                m + egui::vec2(0.0, 4.0),
            ]
        } else {
            vec![
                m + egui::vec2(-4.0, -4.0),
                m + egui::vec2(-4.0, 4.0),
                m + egui::vec2(4.0, 0.0),
            ]
        };
        painter.add(egui::Shape::convex_polygon(tri, GUIDE, Stroke::NONE));
        let s = Stroke::new(1.2, ruler);
        painter.line_segment([x + egui::vec2(-2.5, -2.5), x + egui::vec2(2.5, 2.5)], s);
        painter.line_segment([x + egui::vec2(-2.5, 2.5), x + egui::vec2(2.5, -2.5)], s);
    };
    for &x in &g.vertical {
        let sx = vp.to_screen(v2(x, 0.0)).x as f32;
        if sx > rect.min.x + RULER {
            draw(true, sx);
        }
    }
    for &y in &g.horizontal {
        let sy = vp.to_screen(v2(0.0, y)).y as f32;
        if sy > rect.min.y + RULER {
            draw(false, sy);
        }
    }
}

fn set_guides(app: &mut EditorApp, g: &Guides) {
    app.execute(&CommandRequest::new(
        "guides.set",
        json!({ "vertical": mm(&g.vertical), "horizontal": mm(&g.horizontal) }),
    ));
}

/// Pointer handling for guidelines: a press on a ruler adds one (or picks a marker, or
/// deletes with ×), dragging moves it, release saves it (`guides.set`, one Undo); released
/// outside the canvas, it is removed. Returns whether the pointer was used.
pub fn guide_pointer(
    app: &mut EditorApp,
    ui: &egui::Ui,
    response: &egui::Response,
    rect: egui::Rect,
    vp: &Viewport,
) -> bool {
    if !guides_shown(app) {
        app.workspace.view.guide_drag = None;
        return false;
    }
    let (pressed, released, at) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.pointer.interact_pos().or(i.pointer.hover_pos()),
        )
    });
    let Some(at) = at else { return false };
    let model = vp.to_model(v2(f64::from(at.x), f64::from(at.y)));
    if let Some(mut d) = app.workspace.view.guide_drag {
        d.at = snap_guide(
            if d.vertical { model.x } else { model.y },
            &app.workspace.grid,
        );
        app.status = format!("Guideline {:.1} mm", d.at / 10.0);
        if released {
            app.workspace.view.guide_drag = None;
            let g = &app.session.design().guides;
            let next = match (rect.contains(at), d.index) {
                (true, _) => d.placed(g),
                (false, Some(i)) => remove_guide(g, d.vertical, i),
                (false, None) => g.clone(),
            };
            if next != *g {
                set_guides(app, &next);
            }
        } else {
            app.workspace.view.guide_drag = Some(d);
        }
        return true;
    }
    if !(pressed && response.hovered()) {
        return false;
    }
    let vertical = if at.y < rect.min.y + RULER && at.x > rect.min.x + RULER {
        true
    } else if at.x < rect.min.x + RULER && at.y > rect.min.y + RULER {
        false
    } else {
        return false;
    };
    let g = app.session.design().guides.clone();
    let list = if vertical { &g.vertical } else { &g.horizontal };
    let screen = |v: f64| {
        if vertical {
            vp.to_screen(v2(v, 0.0)).x as f32
        } else {
            vp.to_screen(v2(0.0, v)).y as f32
        }
    };
    // The delete × of a marker.
    if let Some(i) = list.iter().position(|&v| {
        let (_, x) = marker_points(rect, vertical, screen(v));
        x.distance(at) <= PICK
    }) {
        set_guides(app, &remove_guide(&g, vertical, i));
        return true;
    }
    let here = if vertical { model.x } else { model.y };
    let tolerance = f64::from(PICK) * vp.units_per_point();
    let index = guide_near(list, here, tolerance);
    if index.is_none() && list.len() >= Guides::MAX {
        app.status = format!("At most {} guidelines per ruler", Guides::MAX);
        return true;
    }
    app.workspace.view.guide_drag = Some(GuideDrag {
        vertical,
        index,
        at: index.map_or_else(|| snap_guide(here, &app.workspace.grid), |i| list[i]),
    });
    true
}

/// Modify Image: the image's outline in red while the window is open.
pub fn draw_image_frame(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    if !app.workspace.view.image_open {
        return;
    }
    let Some(b) = &app.session.design().background else {
        return;
    };
    let r = egui::Rect::from_two_pos(
        pos(vp.to_screen(v2(b.x, b.y))),
        pos(vp.to_screen(v2(b.x + b.width, b.y + b.height))),
    );
    painter.rect_stroke(
        r,
        0.0,
        Stroke::new(1.5, IMAGE_FRAME),
        egui::StrokeKind::Middle,
    );
}

// MARK: image commands

pub fn rotate_image(app: &mut EditorApp, direction: &str) {
    if app
        .execute(&CommandRequest::new(
            "background.rotate",
            json!({ "direction": direction }),
        ))
        .is_some()
    {
        app.status = format!("Background image turned {direction}");
    }
}

/// Image ▸ Save Image: to a file or to the clipboard, as shown on the Design Page.
pub fn save_image_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    if ui.button(t("ui-to-file")).clicked() {
        ui.close();
        if let Some(p) = crate::files::FileDialog::new()
            .add_filter("PNG", &["png"])
            .add_filter("Windows bitmap", &["bmp"])
            .add_filter("JPEG", &["jpg", "jpeg"])
            .set_file_name("background.png")
            .save_file()
            && app
                .execute(&CommandRequest::new(
                    "background.save",
                    json!({ "path": p.display().to_string() }),
                ))
                .is_some()
        {
            app.status = format!("Image saved to {}", p.display());
        }
    }
    if ui.button(t("ui-to-clipboard")).clicked() {
        ui.close();
        app.status = match copy_image(app) {
            Ok(()) => "Background image copied to the clipboard".into(),
            Err(e) => format!("Copy image: {e}"),
        };
    }
}

/// The background image as shown, decoded from its file (size-limited decoder).
pub fn shown_image(app: &EditorApp) -> Result<openstitch_digitize::Raster, String> {
    let b = app
        .session
        .design()
        .background
        .as_ref()
        .ok_or("no background image")?;
    let meta = std::fs::metadata(&b.path).map_err(|e| e.to_string())?;
    if meta.len() > openstitch_digitize::raster::MAX_FILE_BYTES as u64 {
        return Err("the image file is too large".into());
    }
    let data = std::fs::read(&b.path).map_err(|e| e.to_string())?;
    let r = openstitch_digitize::Raster::decode(&data).map_err(|e| e.to_string())?;
    Ok(r.with_adjust(&b.adjust))
}

fn copy_image(app: &EditorApp) -> Result<(), String> {
    if !app.ui.system_clipboard {
        return Err("the system clipboard is not used here".into());
    }
    let r = shown_image(app)?;
    let mut bytes = Vec::with_capacity(r.width() as usize * r.height() as usize * 4);
    for y in 0..r.height() {
        for x in 0..r.width() {
            bytes.extend_from_slice(&r.pixel(x, y));
        }
    }
    crate::ribbon::os_clipboard().and_then(|()| {
        openstitch_app::platform::set_clipboard_image(openstitch_app::platform::ClipboardImage {
            width: r.width() as usize,
            height: r.height() as usize,
            bytes,
        })
    })
}

/// Modify Image window values while they are being edited.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ImageDraft {
    x: f64,
    y: f64,
    width: f64,
    /// Percent of each side.
    crop: [f64; 4],
    brightness: f64,
    contrast: f64,
    saturation: f64,
}

impl ImageDraft {
    fn of(b: &openstitch_design::BackgroundImage) -> Self {
        ImageDraft {
            x: b.x / 10.0,
            y: b.y / 10.0,
            width: b.width / 10.0,
            crop: b.adjust.crop.map(|c| (c * 1000.0).round() / 10.0),
            brightness: b.adjust.brightness,
            contrast: b.adjust.contrast,
            saturation: b.adjust.saturation,
        }
    }

    /// `background.modify` parameters for what differs from `now`; a crop change wins over a
    /// move or resize made at the same time (the crop keeps the image's place and scale).
    fn changes(&self, now: &ImageDraft) -> Option<Value> {
        let mut p = serde_json::Map::new();
        if self.crop != now.crop {
            p.insert("crop".into(), json!(self.crop));
        } else {
            for (k, a, b) in [
                ("x", self.x, now.x),
                ("y", self.y, now.y),
                ("width", self.width, now.width),
            ] {
                if a != b {
                    p.insert(k.into(), json!(a));
                }
            }
        }
        for (k, a, b) in [
            ("brightness", self.brightness, now.brightness),
            ("contrast", self.contrast, now.contrast),
            ("saturation", self.saturation, now.saturation),
        ] {
            if a != b {
                p.insert(k.into(), json!(a));
            }
        }
        (!p.is_empty()).then_some(Value::Object(p))
    }
}

fn modify_image_window(app: &mut EditorApp, ctx: &egui::Context) {
    let units = app.ui.prefs.units;
    let mut open = app.workspace.view.image_open;
    let mut commit: Option<Value> = None;
    let mut turn: Option<&str> = None;
    egui::Window::new("Modify Image")
        .open(&mut open)
        .resizable(false)
        .default_width(300.0)
        .show(ctx, |ui| {
            let Some(b) = app.session.design().background.clone() else {
                crate::art::header(ui, crate::art::Art::NoImage, |ui| {
                    ui.label(t("ui-no-background-image-use-image-open"));
                });
                return;
            };
            let now = ImageDraft::of(&b);
            let mut d = app.workspace.view.image_draft.unwrap_or(now);
            let mut done = false;
            let mut finish = |r: &egui::Response| {
                done |= r.drag_stopped() || r.lost_focus() || (r.changed() && !r.dragged());
            };
            egui::Grid::new("modify-image")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(t("ui-position"));
                    ui.horizontal(|ui| {
                        finish(
                            &ui.add(
                                crate::units_ui::drag(&mut d.x, units)
                                    .speed(0.5)
                                    .prefix("x "),
                            ),
                        );
                        finish(
                            &ui.add(
                                crate::units_ui::drag(&mut d.y, units)
                                    .speed(0.5)
                                    .prefix("y "),
                            ),
                        );
                    });
                    ui.end_row();
                    ui.label(t("ui-width"));
                    ui.horizontal(|ui| {
                        finish(
                            &ui.add(
                                crate::units_ui::drag(&mut d.width, units)
                                    .range(1.0..=2000.0)
                                    .speed(0.5),
                            ),
                        );
                        ui.label(format!("height {:.1} mm", b.height / 10.0));
                    });
                    ui.end_row();
                    let max = ImageAdjust::MAX_CROP * 100.0;
                    for (i, name) in ["Crop left", "Crop top", "Crop right", "Crop bottom"]
                        .into_iter()
                        .enumerate()
                    {
                        ui.label(name);
                        finish(&ui.add(egui::Slider::new(&mut d.crop[i], 0.0..=max).suffix(" %")));
                        ui.end_row();
                    }
                    for (name, v) in [
                        ("Brightness", &mut d.brightness),
                        ("Contrast", &mut d.contrast),
                        ("Saturation", &mut d.saturation),
                    ] {
                        ui.label(name);
                        finish(&ui.add(egui::Slider::new(v, -100.0..=100.0)));
                        ui.end_row();
                    }
                });
            // Keep at least 5 % of the image in each direction.
            let keep = 100.0 * (1.0 - ImageAdjust::MIN_KEPT);
            for (a, b) in [(0, 2), (1, 3)] {
                if d.crop[a] + d.crop[b] > keep {
                    d.crop[b] = (keep - d.crop[a]).max(0.0);
                }
            }
            ui.horizontal(|ui| {
                let left = (
                    crate::icons::ui_image(ui, "image_rotate_left"),
                    "Rotate Left",
                );
                if ui.button(left).clicked() {
                    turn = Some("left");
                }
                let right = (
                    crate::icons::ui_image(ui, "image_rotate_right"),
                    "Rotate Right",
                );
                if ui.button(right).clicked() {
                    turn = Some("right");
                }
                if ui
                    .button(t("ui-reset"))
                    .on_hover_text(t("ui-no-crop-original-colours-tip"))
                    .clicked()
                {
                    d.crop = [0.0; 4];
                    d.brightness = 0.0;
                    d.contrast = 0.0;
                    d.saturation = 0.0;
                    done = true;
                }
            });
            ui.label(
                egui::RichText::new(format!(
                    "{} × {} px; the image file is not changed.",
                    b.pixel_width, b.pixel_height
                ))
                .weak()
                .small(),
            );
            if done {
                commit = d.changes(&now);
                app.workspace.view.image_draft = None;
            } else {
                app.workspace.view.image_draft = (d != now).then_some(d);
            }
        });
    app.workspace.view.image_open = open;
    if !open {
        app.workspace.view.image_draft = None;
    }
    if let Some(p) = commit
        && app
            .execute(&CommandRequest::new("background.modify", p))
            .is_some()
    {
        app.status = "Background image changed".into();
    }
    if let Some(t) = turn {
        rotate_image(app, t);
    }
}

// MARK: windows

pub fn windows(app: &mut EditorApp, ctx: &egui::Context) {
    realistic_window(app, ctx);
    modify_image_window(app, ctx);
    font_filter_window(app, ctx);
    fabric_window(app, ctx);
}

/// View ▸ Realistic View Settings: thread width, contrast and brightness; changes show at once
/// and are kept in the window preferences.
fn realistic_window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.workspace.view.realistic_open;
    let mut changed = false;
    egui::Window::new("Realistic View Settings")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let r = &mut app.ui.prefs.realistic;
            egui::Grid::new("realistic").num_columns(2).show(ui, |ui| {
                ui.label(t("ui-thread-width"));
                changed |= ui
                    .add(
                        egui::Slider::new(&mut r.thread_width_mm, RealisticSettings::WIDTH_RANGE)
                            .step_by(0.05)
                            .suffix(" mm"),
                    )
                    .changed();
                ui.end_row();
                ui.label(t("ui-contrast"));
                changed |= ui
                    .add(egui::Slider::new(&mut r.contrast, -100.0..=100.0))
                    .changed();
                ui.end_row();
                ui.label(t("ui-brightness"));
                changed |= ui
                    .add(egui::Slider::new(&mut r.brightness, -100.0..=100.0))
                    .changed();
                ui.end_row();
            });
            ui.horizontal(|ui| {
                if ui.button(t("ui-default")).clicked() {
                    *r = RealisticSettings::default();
                    changed = true;
                }
                if !app.workspace.realistic && ui.button(t("ui-show-realistic-view")).clicked() {
                    app.workspace.realistic = true;
                    app.workspace.solid = false;
                }
            });
        });
    app.workspace.view.realistic_open = open;
    if changed {
        app.ui.save_prefs();
    }
}

/// Text ▸ Font Filter Settings: tick the fonts the lists show while Font Filter is on.
fn font_filter_window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.workspace.view.fonts_open;
    if !open {
        app.workspace.view.fonts_draft = None;
        return;
    }
    let list = app
        .session
        .execute("fonts.list", Some(&json!({})))
        .unwrap_or(Value::Null);
    let rows: Vec<(String, String, &'static str)> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| {
            let id = f["id"].as_str()?.to_string();
            let label = format!(
                "{} {}",
                f["family"].as_str().unwrap_or(&id),
                f["style"].as_str().unwrap_or_default()
            );
            let group = if f["source"] == "design" {
                "Fonts in this design"
            } else {
                "Built-in fonts"
            };
            Some((id, label.trim().to_string(), group))
        })
        .collect();
    let mut draft = app
        .workspace
        .view
        .fonts_draft
        .clone()
        .unwrap_or_else(|| app.ui.prefs.font_filter.clone());
    let mut apply = false;
    egui::Window::new("Font Filter Settings")
        .open(&mut open)
        .default_width(300.0)
        .show(ctx, |ui| {
            ui.label(t("ui-tick-the-fonts-to-show-clear"));
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for group in ["Built-in fonts", "Fonts in this design"] {
                        let ids: Vec<&(String, String, &str)> =
                            rows.iter().filter(|r| r.2 == group).collect();
                        if ids.is_empty() {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.strong(group);
                            for (label, shown) in [("All", true), ("None", false)] {
                                if ui.small_button(label).clicked() {
                                    for r in &ids {
                                        draft.set_shown(&r.0, shown);
                                    }
                                }
                            }
                        });
                        for (id, label, _) in ids {
                            let mut shown = !draft.hidden.contains(id);
                            if ui.checkbox(&mut shown, label).changed() {
                                draft.set_shown(id, shown);
                            }
                        }
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .button(t("ui-default"))
                    .on_hover_text(t("ui-show-every-font-tip"))
                    .clicked()
                {
                    draft.hidden.clear();
                }
                if ui.button(t("ui-ok")).clicked() {
                    apply = true;
                }
            });
        });
    if apply {
        draft.on = true;
        app.ui.prefs.font_filter = draft;
        app.ui.save_prefs();
        app.status = "Font filter on".into();
        open = false;
    } else {
        app.workspace.view.fonts_draft = Some(draft);
    }
    app.workspace.view.fonts_open = open;
    if !open {
        app.workspace.view.fonts_draft = None;
    }
}

/// Selected shape and text patterns.
fn outline_selection(app: &EditorApp) -> Vec<u64> {
    let d = app.session.design();
    d.objects
        .iter()
        .filter(|o| app.selection.contains(&o.id) && o.is_outline())
        .map(|o| o.id)
        .collect()
}

/// Application menu ▸ Fabric Selector: pick a fabric, read about it, apply its recommended
/// sewing attributes to every shape and text pattern (or the selection).
fn fabric_window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.workspace.view.fabric_open;
    if !open {
        return;
    }
    let list = app
        .session
        .execute("fabric.list", Some(&json!({})))
        .unwrap_or(Value::Null);
    let fabrics: Vec<Value> = list.as_array().cloned().unwrap_or_default();
    let selected = outline_selection(app);
    // The default for new shapes, when one is set (#508).
    let current_default = app
        .session
        .execute("options.get", Some(&json!({})))
        .map(|o| o["defaultFabric"].clone())
        .unwrap_or(Value::Null);
    let v = &mut app.workspace.view;
    let mut chosen = v.fabric.clone().or_else(|| {
        current_default["fabric"]
            .as_str()
            .or_else(|| fabrics.first().and_then(|f| f["id"].as_str()))
            .map(str::to_string)
    });
    let mut all = v.fabric_all.unwrap_or(selected.is_empty());
    let mut weight = v.fabric_weight.unwrap_or_else(|| {
        current_default["weight"]
            .as_u64()
            .map_or(40, |w| w as u32)
    });
    let mut for_new = v.fabric_new.unwrap_or(!current_default.is_null());
    let advice = chosen.as_ref().and_then(|id| {
        app.session
            .execute("fabric.advise", Some(&json!({ "fabric": id, "weight": weight })))
            .ok()
    });
    let v = &mut app.workspace.view;
    let mut ok = false;
    egui::Window::new("Fabric Selector")
        .open(&mut open)
        .default_width(420.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(190.0);
                    ui.strong("Fabric");
                    for f in &fabrics {
                        let id = f["id"].as_str().unwrap_or_default();
                        let name = f["name"].as_str().unwrap_or(id);
                        if ui
                            .selectable_label(chosen.as_deref() == Some(id), name)
                            .clicked()
                        {
                            chosen = Some(id.to_string());
                        }
                    }
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_width(200.0);
                    let Some(f) = fabrics
                        .iter()
                        .find(|f| f["id"].as_str() == chosen.as_deref())
                    else {
                        return;
                    };
                    ui.label(f["description"].as_str().unwrap_or_default());
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(f["advice"].as_str().unwrap_or_default()).italics());
                    ui.add_space(6.0);
                    let combo = egui::ComboBox::from_id_salt("fabric-weight")
                        .selected_text(format!("No. {weight}"))
                        .show_ui(ui, |ui| {
                            for w in [60, 40, 30, 12] {
                                ui.selectable_value(&mut weight, w, format!("No. {w}"));
                            }
                        })
                        .response
                        .on_hover_text(t("ui-beyond-pe-design-a-thinner-thread-tip"));
                    crate::a11y::name(&combo, "Thread weight");
                    ui.add_space(4.0);
                    egui::Grid::new("fabric-values").num_columns(2).show(ui, |ui| {
                        let n = |k: &str| f[k].as_f64().unwrap_or_default();
                        let a = |k: &str| advice.as_ref().map_or(Value::Null, |a| a[k].clone());
                        // The density fabric.apply gives for this weight (#508).
                        let scaled = |k: &str| {
                            let d = n(k) * openstitch_engine::fabric::weight_factor(weight);
                            format!("{:.2} rows/mm", d)
                        };
                        for (label, value) in [
                            ("Thread weight", format!("No. {weight}")),
                            ("Fill density", scaled("fillDensity")),
                            ("Satin density", scaled("satinDensity")),
                            ("Fill stitch length", format!("{} mm", n("stitchLength"))),
                            ("Pull compensation", format!("{} mm", n("pullCompensation"))),
                            (
                                "Underlay",
                                f["underlay"].as_str().unwrap_or("none").to_string(),
                            ),
                            // The makers' advice for this thread weight (#508; fabric.advise).
                            ("Backing", f["backing"].as_str().unwrap_or("-").to_string()),
                            ("Topping", f["topping"].as_str().unwrap_or("-").to_string()),
                            (
                                "Needle",
                                a("needle")["size"]
                                    .as_str()
                                    .map_or_else(
                                        || f["needle"].as_str().unwrap_or("-").to_string(),
                                        |s| format!("{s}, {}", a("needle")["point"].as_str().unwrap_or("")),
                                    ),
                            ),
                            (
                                "Smallest text",
                                format!(
                                    "{} mm (No. {weight} thread)",
                                    a("minTextMm").as_f64().unwrap_or(n("minTextMm"))
                                ),
                            ),
                        ] {
                            ui.label(label);
                            ui.label(value);
                            ui.end_row();
                        }
                    });
                });
            });
            ui.separator();
            ui.checkbox(&mut all, t("ui-apply-to-all-shape-and-text"));
            if !all {
                ui.label(format!("Applies to the {} selected pattern(s).", selected.len()));
            }
            ui.checkbox(&mut for_new, t("ui-new-shapes-and-texts-start-from"))
                .on_hover_text(t("ui-beyond-pe-design-shapes-and-texts-tip"));
            ui.label(
                egui::RichText::new(
                    "OpenStitch's own approximate settings for fill and satin regions. Perform a trial sewing before embroidering your project.",
                )
                .weak()
                .small(),
            );
            ui.horizontal(|ui| {
                let can = chosen.is_some() && (all || !selected.is_empty());
                if ui.add_enabled(can, egui::Button::new(t("ui-ok"))).clicked() {
                    ok = true;
                }
            });
        });
    v.fabric = chosen.clone();
    v.fabric_all = Some(all);
    v.fabric_weight = Some(weight);
    v.fabric_new = Some(for_new);
    if ok && let Some(id) = chosen {
        let mut params = json!({ "fabric": id, "weight": weight });
        if !all {
            params["ids"] = json!(selected);
        }
        if let Some(r) = app.execute(&CommandRequest::new("fabric.apply", params)) {
            let n = r["changed"].as_array().map_or(0, Vec::len);
            app.status = format!("Fabric settings applied to {n} pattern(s)");
        }
        // New shapes follow (or stop following) the fabric (#508); a user setting.
        let default = if for_new {
            Some(json!({ "fabric": id, "weight": weight }))
        } else if !current_default.is_null() {
            Some(json!({ "fabric": "none" }))
        } else {
            None
        };
        if let Some(p) = default {
            app.execute(&CommandRequest::new("fabric.setDefault", p));
        }
        open = false;
    }
    app.workspace.view.fabric_open = open;
}

// MARK: View ▸ Show (#517)

/// View ▸ Show: view-only toggles, beyond PE-DESIGN and off by default; kept in `ui.json`.
pub fn show_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    use openstitch_design::view::{ColorBy, Layers};
    let s = &mut app.ui.prefs.show;
    let mut changed = false;
    changed |= ui
        .checkbox(&mut s.needle_points, t("ui-needle-points"))
        .on_hover_text(t("ui-a-dot-at-every-needle-drop-tip"))
        .changed();
    ui.separator();
    ui.label(egui::RichText::new(t("menu-underlay")).weak());
    for (l, label) in [
        (Layers::Both, "Underlay and top stitching"),
        (Layers::UnderlayOnly, "Underlay only"),
        (Layers::TopOnly, "Top stitching only"),
    ] {
        changed |= ui.radio_value(&mut s.layers, l, label).changed();
    }
    ui.separator();
    changed |= ui
        .checkbox(&mut s.trim_marks, t("ui-trim-marks"))
        .on_hover_text(
            "A cut mark where the machine trims; trimmed moves drawn red, untrimmed jumps grey",
        )
        .changed();
    changed |= ui
        .checkbox(&mut s.hide_unselected, t("ui-hide-unselected"))
        .on_hover_text(t("ui-only-the-selected-objects-stitches-tip"))
        .changed();
    ui.separator();
    ui.label(egui::RichText::new(t("ui-colour-by")).weak());
    for (c, label) in [
        (ColorBy::Thread, "Thread"),
        (ColorBy::SewingOrder, "Sewing order"),
        (ColorBy::Monochrome, "Monochrome"),
    ] {
        changed |= ui.radio_value(&mut s.color_by, c, label).changed();
    }
    changed |= ui
        .checkbox(&mut s.solo, t("ui-solo-colour"))
        .on_hover_text(
            "Fade every colour but the current one (the simulator's, else the selection's)",
        )
        .changed();
    if changed {
        app.ui.save_prefs();
    }
}

/// Needle points and trim marks over the stitches (#517).
pub fn draw_show(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let s = app.ui.prefs.show;
    let p = |q: Vec2| {
        let v = vp.to_screen(q);
        Pos2::new(v.x as f32, v.y as f32)
    };
    if s.needle_points {
        let r = (vp.scale * 3.0).clamp(1.0, 2.5) as f32;
        for layer in &app.render.layers {
            let [cr, cg, cb] = layer.rgb;
            let c = Color32::from_rgb(cr, cg, cb).gamma_multiply(0.9);
            for stroke in &layer.strokes {
                for &q in stroke {
                    painter.circle_filled(p(q), r, c);
                }
            }
        }
    }
    if s.trim_marks {
        let cut = Color32::from_rgb(0xd3, 0x2f, 0x2f);
        for (i, &(a, b)) in app.render.travel.iter().enumerate() {
            let trimmed = app.render.travel_cut.get(i).copied().unwrap_or(false);
            let (c, w) = if trimmed {
                (cut.gamma_multiply(0.8), 1.2)
            } else {
                (Color32::from_gray(110), 1.0)
            };
            painter.add(egui::Shape::dashed_line(
                &[p(a), p(b)],
                Stroke::new(w, c),
                5.0,
                3.0,
            ));
        }
        for t in openstitch_design::view::trim_points(&app.pattern) {
            let c = p(t);
            painter.circle_stroke(c, 4.0, Stroke::new(1.5, cut));
            painter.line_segment(
                [c + egui::vec2(-5.0, -5.0), c + egui::vec2(5.0, 5.0)],
                Stroke::new(1.5, cut),
            );
        }
    }
}
