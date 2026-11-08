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
    GuideDrag, RealisticSettings, SOLID_WIDTH_MM, guide_label, guide_near, remove_guide,
    snap_guide,
};
use openstitch_app::editor::{CommandRequest, Viewport};
use openstitch_design::{Guides, ImageAdjust, Vec2, v2};
use serde_json::{Value, json};

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
    /// Where the guideline press began (a click without a move selects, #799).
    guide_press: Option<Pos2>,
    /// The selected guideline (vertical?, index; #799).
    pub selected_guide: Option<(bool, usize)>,
    /// Set Position… (#799).
    pub guide_dialog: Option<GuideDialog>,
    /// The right-click menu of a guideline or a ruler (#799).
    pub guide_menu: Option<GuideMenu>,
    /// New Guide… (#799).
    pub new_guide: Option<NewGuideDialog>,
    /// New Guide Layout… (#799).
    pub guide_layout: Option<LayoutDialog>,
}

/// The guidelines' colour (Options ▸ Guide colour, #799).
fn guide_colour(app: &EditorApp) -> Color32 {
    let [r, g, b] = app.ui.prefs.guide_color;
    Color32::from_rgb(r, g, b)
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
    let selected = selected_guide(app);
    let colour = guide_colour(app);
    // New Guide Layout…'s guidelines, dashed, while it is open (#799).
    if let Some(Ok(l)) = app.workspace.view.guide_layout.as_ref().map(|d| d.layout(app)) {
        let page = app.session.design().page;
        if let Ok((xs, ys)) = l.lines(page.width, page.height) {
            let s = Stroke::new(1.0, colour);
            for x in xs {
                let sx = vp.to_screen(v2(x, 0.0)).x as f32;
                painter.extend(egui::Shape::dashed_line(
                    &[Pos2::new(sx, rect.min.y), Pos2::new(sx, rect.max.y)],
                    s,
                    6.0,
                    4.0,
                ));
            }
            for y in ys {
                let sy = vp.to_screen(v2(0.0, y)).y as f32;
                painter.extend(egui::Shape::dashed_line(
                    &[Pos2::new(rect.min.x, sy), Pos2::new(rect.max.x, sy)],
                    s,
                    6.0,
                    4.0,
                ));
            }
        }
    }
    // The selected guideline drawn thicker (#799).
    let stroke = |vertical: bool, i: usize| {
        Stroke::new(if selected == Some((vertical, i)) { 2.5 } else { 1.0 }, colour)
    };
    for (i, &x) in g.vertical.iter().enumerate() {
        let sx = vp.to_screen(v2(x, 0.0)).x as f32;
        painter.line_segment(
            [Pos2::new(sx, rect.min.y), Pos2::new(sx, rect.max.y)],
            stroke(true, i),
        );
    }
    for (i, &y) in g.horizontal.iter().enumerate() {
        let sy = vp.to_screen(v2(0.0, y)).y as f32;
        painter.line_segment(
            [Pos2::new(rect.min.x, sy), Pos2::new(rect.max.x, sy)],
            stroke(false, i),
        );
    }
    // Its position beside the guideline being dragged, or the one under the pointer (#799).
    let label = app.workspace.view.guide_drag.map(|d| (d.vertical, d.at)).or_else(|| {
        let h = painter.ctx().input(|i| i.pointer.hover_pos())?;
        if !rect.contains(h) {
            return None;
        }
        let near = |list: &[f64], vertical: bool| {
            list.iter().copied().find(|&v| {
                let s = if vertical {
                    vp.to_screen(v2(v, 0.0)).x as f32 - h.x
                } else {
                    vp.to_screen(v2(0.0, v)).y as f32 - h.y
                };
                s.abs() <= PICK
            })
        };
        near(&g.vertical, true)
            .map(|x| (true, x))
            .or_else(|| near(&g.horizontal, false).map(|y| (false, y)))
    });
    if let Some((vertical, at)) = label {
        let zero = app.ruler_zero();
        let text = guide_label(
            at - if vertical { zero.x } else { zero.y },
            app.ui.prefs.units,
        );
        let (p, align) = if vertical {
            let sx = vp.to_screen(v2(at, 0.0)).x as f32;
            (
                Pos2::new(sx + 4.0, rect.min.y + RULER + 4.0),
                egui::Align2::LEFT_TOP,
            )
        } else {
            let sy = vp.to_screen(v2(0.0, at)).y as f32;
            (
                Pos2::new(rect.min.x + RULER + 4.0, sy - 4.0),
                egui::Align2::LEFT_BOTTOM,
            )
        };
        let font = egui::FontId::monospace(11.0);
        let galley = painter.layout_no_wrap(text, font, Color32::WHITE);
        let r = align.anchor_size(p, galley.size()).expand(2.0);
        painter.rect_filled(r, 2.0, colour);
        painter.galley(r.min + egui::vec2(2.0, 2.0), galley, Color32::WHITE);
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

/// Guideline markers (drag to move) and × (click to delete; none while locked, #799) in the
/// rulers.
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
    let colour = guide_colour(app);
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
        painter.add(egui::Shape::convex_polygon(tri, colour, Stroke::NONE));
        if g.locked {
            return;
        }
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
/// on a ruler or outside the canvas, it is removed. With the Select tool a press on a
/// guideline on the canvas picks it too (#799). A click without a move selects it (Delete
/// removes it), a double-click opens Set Position…, Alt-click turns it vertical ↔
/// horizontal. Locked guidelines are not picked; a ruler press still adds one. Returns
/// whether the pointer was used.
pub fn guide_pointer(
    app: &mut EditorApp,
    ui: &egui::Ui,
    response: &egui::Response,
    rect: egui::Rect,
    vp: &Viewport,
) -> bool {
    // A guideline can be dragged out of a ruler even while they are hidden; that shows them
    // (#799).
    if !app.workspace.rulers {
        app.workspace.view.guide_drag = None;
        return false;
    }
    let shown = guides_shown(app);
    let (pressed, released, double, alt, at) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.pointer.button_double_clicked(egui::PointerButton::Primary),
            i.modifiers.alt,
            i.pointer.interact_pos().or(i.pointer.hover_pos()),
        )
    });
    let Some(at) = at else { return false };
    let model = vp.to_model(v2(f64::from(at.x), f64::from(at.y)));
    if response.secondary_clicked()
        && let Some(target) = menu_target(app, at, model, rect, vp)
    {
        if let MenuTarget::Guide(v, i) = target
            && !app.session.design().guides.locked
        {
            app.workspace.view.selected_guide = Some((v, i));
        }
        app.workspace.view.guide_menu = Some(GuideMenu {
            at,
            target,
            fresh: true,
        });
        return true;
    }
    if let Some(mut d) = app.workspace.view.guide_drag {
        // A click without a move leaves the guideline where it is.
        let still = d.index.is_some()
            && app
                .workspace
                .view
                .guide_press
                .is_some_and(|p| p.distance(at) < 3.0);
        if !still {
            app.workspace.view.guide_press = None;
            d.at = snap_guide(
                if d.vertical { model.x } else { model.y },
                &app.workspace.grid,
            );
        }
        let zero = app.ruler_zero();
        app.status = openstitch_app::tf!(
            "ui-status-guideline-mm",
            position = guide_label(
                d.at - if d.vertical { zero.x } else { zero.y },
                app.ui.prefs.units
            )
        );
        if released {
            app.workspace.view.guide_drag = None;
            app.workspace.view.guide_press = None;
            let g = &app.session.design().guides;
            // Dropped on either ruler or off the canvas, a guideline goes (#799).
            let on_canvas = rect.contains(at)
                && at.x > rect.min.x + RULER
                && at.y > rect.min.y + RULER;
            // A click without a move (on its marker or its line) leaves it where it is.
            let next = match (on_canvas || still, d.index) {
                (true, _) => d.placed(g),
                (false, Some(i)) => remove_guide(g, d.vertical, i),
                (false, None) => g.clone(),
            };
            app.workspace.view.selected_guide = d
                .index
                .filter(|_| on_canvas || still)
                .map(|i| (d.vertical, i));
            if next != *g {
                set_guides(app, &next);
            } else if let (Some(i), true) = (d.index, double) {
                open_guide_dialog(app, d.vertical, i);
            }
        } else {
            app.workspace.view.guide_drag = Some(d);
        }
        return true;
    }
    if !(pressed && response.hovered()) {
        return false;
    }
    let locked = app.session.design().guides.locked;
    let vertical = if at.y < rect.min.y + RULER && at.x > rect.min.x + RULER {
        true
    } else if at.x < rect.min.x + RULER && at.y > rect.min.y + RULER {
        false
    } else {
        return canvas_press(app, at, model, alt, vp);
    };
    if !shown {
        app.workspace.guides = true;
        app.ui.prefs.editing.show_guides = true;
        app.ui.save_prefs();
    }
    let g = app.session.design().guides.clone();
    // Hidden or locked guidelines cannot be picked: a press starts a new one.
    let list: &[f64] = match (shown && !locked, vertical) {
        (false, _) => &[],
        (true, true) => &g.vertical,
        (true, false) => &g.horizontal,
    };
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
        app.workspace.view.selected_guide = None;
        set_guides(app, &remove_guide(&g, vertical, i));
        return true;
    }
    let here = if vertical { model.x } else { model.y };
    let tolerance = f64::from(PICK) * vp.units_per_point();
    let index = guide_near(list, here, tolerance);
    if let (Some(i), true) = (index, alt) {
        // Alt-click on its marker: turned, crossing the page centre.
        turn_guide(app, vertical, i, None);
        return true;
    }
    if index.is_none() && list.len() >= Guides::MAX {
        app.status = openstitch_app::tf!("ui-status-at-most-guidelines-per-ruler", uides = Guides::MAX);
        return true;
    }
    app.workspace.view.guide_press = Some(at);
    app.workspace.view.guide_drag = Some(GuideDrag {
        vertical,
        index,
        at: index.map_or_else(|| snap_guide(here, &app.workspace.grid), |i| list[i]),
    });
    true
}

/// A press on the canvas: picks the guideline under the pointer with the Select tool (#799);
/// anywhere else it drops the guideline selection and lets the canvas have the press.
fn canvas_press(app: &mut EditorApp, at: Pos2, model: Vec2, alt: bool, vp: &Viewport) -> bool {
    use openstitch_app::editor::tools::Tool;
    let g = app.session.design().guides.clone();
    let pickable = guides_shown(app) && !g.locked && app.tools.tool() == Tool::Select;
    let hit = line_at(&g, at, vp).filter(|_| pickable);
    let Some((vertical, i)) = hit else {
        app.workspace.view.selected_guide = None;
        return false;
    };
    if alt {
        // Alt-click: turned where it was clicked.
        turn_guide(app, vertical, i, Some(if vertical { model.y } else { model.x }));
        return true;
    }
    app.workspace.view.guide_press = Some(at);
    app.workspace.view.guide_drag = Some(GuideDrag {
        vertical,
        index: Some(i),
        at: if vertical { g.vertical[i] } else { g.horizontal[i] },
    });
    true
}

/// The guideline whose line passes within [`PICK`] of `at` on the canvas, the nearest first.
fn line_at(g: &Guides, at: Pos2, vp: &Viewport) -> Option<(bool, usize)> {
    let near = |list: &[f64], vertical: bool| {
        list.iter()
            .enumerate()
            .map(|(i, &v)| {
                let s = if vertical {
                    vp.to_screen(v2(v, 0.0)).x as f32 - at.x
                } else {
                    vp.to_screen(v2(0.0, v)).y as f32 - at.y
                };
                (s.abs(), vertical, i)
            })
            .filter(|&(d, ..)| d <= PICK)
            .min_by(|a, b| a.0.total_cmp(&b.0))
    };
    [near(&g.vertical, true), near(&g.horizontal, false)]
        .into_iter()
        .flatten()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, v, i)| (v, i))
}

/// What a right-click at `at` opens (#799): a guideline's menu on its line or its ruler
/// marker, the ruler menu elsewhere on a ruler (or its corner), nothing elsewhere.
fn menu_target(
    app: &EditorApp,
    at: Pos2,
    model: Vec2,
    rect: egui::Rect,
    vp: &Viewport,
) -> Option<MenuTarget> {
    let g = &app.session.design().guides;
    let shown = guides_shown(app);
    let in_top = at.y < rect.min.y + RULER;
    let in_left = at.x < rect.min.x + RULER;
    if in_top || in_left {
        if shown && in_top != in_left {
            let (list, here) = if in_top {
                (&g.vertical, model.x)
            } else {
                (&g.horizontal, model.y)
            };
            let tolerance = f64::from(PICK) * vp.units_per_point();
            if let Some(i) = guide_near(list, here, tolerance) {
                return Some(MenuTarget::Guide(in_top, i));
            }
        }
        return Some(MenuTarget::Ruler);
    }
    if !shown {
        return None;
    }
    line_at(g, at, vp).map(|(v, i)| MenuTarget::Guide(v, i))
}

/// What a guideline right-click menu is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuTarget {
    /// A ruler or the corner between them.
    Ruler,
    /// Guideline (vertical?, index).
    Guide(bool, usize),
}

/// An open guideline or ruler right-click menu (#799).
pub struct GuideMenu {
    at: Pos2,
    pub target: MenuTarget,
    /// Opened this frame: the click that opened it does not close it.
    fresh: bool,
}

/// The right-click menu, until an entry is chosen, Esc or a click elsewhere.
fn guide_menu_window(app: &mut EditorApp, ctx: &egui::Context) {
    let Some(mut m) = app.workspace.view.guide_menu.take() else {
        return;
    };
    let mut chosen = false;
    let r = egui::Area::new(egui::Id::unique("guide-context-menu"))
        .fixed_pos(m.at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);
                chosen = match m.target {
                    MenuTarget::Ruler => ruler_menu(app, ui),
                    MenuTarget::Guide(v, i) => guide_entries(app, ui, Some((v, i))),
                };
            });
        });
    let (pressed, esc) = ctx.input(|i| (i.pointer.any_pressed(), i.key_pressed(egui::Key::Escape)));
    let outside = pressed && !r.response.contains_pointer();
    if !chosen && !esc && (m.fresh || !outside) {
        m.fresh = false;
        app.workspace.view.guide_menu = Some(m);
    }
}

/// The ruler right-click menu (#799): units, ruler origin, show and snap to guidelines, New
/// Guide…, New Guide Layout…, Lock Guides, Clear Guides. True when an entry was chosen.
fn ruler_menu(app: &mut EditorApp, ui: &mut egui::Ui) -> bool {
    use openstitch_app::editor::units::{RulerOrigin, Units};
    let mut changed = false;
    ui.label(egui::RichText::new(t("menu-units")).weak());
    let u = &mut app.ui.prefs.units;
    changed |= ui.radio_value(u, Units::Mm, t("menu-millimetres")).changed();
    changed |= ui.radio_value(u, Units::Inch, t("menu-inches")).changed();
    ui.label(egui::RichText::new(t("menu-ruler-origin")).weak());
    let o = &mut app.ui.prefs.ruler_origin;
    changed |= ui
        .radio_value(o, RulerOrigin::Centre, t("menu-ruler-origin-centre"))
        .changed();
    changed |= ui
        .radio_value(o, RulerOrigin::TopLeft, t("menu-ruler-origin-top-left"))
        .changed();
    ui.separator();
    let mut shown = app.workspace.guides;
    if ui.checkbox(&mut shown, t("ui-guide-show")).changed() {
        app.workspace.guides = shown;
        app.ui.prefs.editing.show_guides = shown;
        changed = true;
    }
    changed |= ui
        .checkbox(&mut app.ui.prefs.editing.snap_guides, t("menu-snap-to-guides"))
        .changed();
    if changed {
        app.database.units = app.ui.prefs.units;
        app.ui.save_prefs();
    }
    ui.separator();
    let mut chosen = changed;
    if ui.button(t("ui-guide-new")).clicked() {
        open_new_guide(app);
        chosen = true;
    }
    if ui.button(t("ui-guide-layout")).clicked() {
        open_guide_layout(app);
        chosen = true;
    }
    chosen | guide_entries(app, ui, None)
}

/// A guideline's right-click menu entries (`Some`), or the lock and clear entries alone
/// (`None`): Set Position…, Clear Selected Guide, Lock Guides, Clear Guides. True when one was
/// chosen.
fn guide_entries(app: &mut EditorApp, ui: &mut egui::Ui, guide: Option<(bool, usize)>) -> bool {
    let g = app.session.design().guides.clone();
    let mut chosen = false;
    if let Some((v, i)) = guide {
        if ui
            .add_enabled(!g.locked, egui::Button::new(t("ui-guide-set-position")))
            .clicked()
        {
            open_guide_dialog(app, v, i);
            chosen = true;
        }
        if ui
            .add_enabled(!g.locked, egui::Button::new(t("ui-clear-selected-guide")))
            .clicked()
        {
            app.workspace.view.selected_guide = Some((v, i));
            delete_selected_guide(app);
            chosen = true;
        }
        ui.separator();
    }
    let mut locked = g.locked;
    if ui
        .checkbox(&mut locked, t("ui-lock-guides"))
        .on_hover_text(t("ui-lock-guides-tip"))
        .changed()
    {
        app.execute(&CommandRequest::new("view.lockGuides", json!({ "locked": locked })));
        chosen = true;
    }
    if ui
        .add_enabled(!g.is_empty(), egui::Button::new(t("ui-clear-guides")))
        .on_hover_text(t("ui-clear-guides-tip"))
        .clicked()
    {
        app.workspace.view.selected_guide = None;
        app.execute(&CommandRequest::new("view.clearGuides", json!({})));
        chosen = true;
    }
    chosen
}

/// Guidelines shown, with the rulers they come out of.
fn show_guides(app: &mut EditorApp) {
    if !(app.workspace.guides && app.workspace.rulers) {
        app.workspace.rulers = true;
        app.workspace.guides = true;
        app.ui.prefs.editing.show_guides = true;
        app.ui.save_prefs();
    }
}

/// The New Guide… dialog (#799).
pub struct NewGuideDialog {
    vertical: bool,
    text: String,
    error: Option<String>,
}

fn open_new_guide(app: &mut EditorApp) {
    app.workspace.view.new_guide = Some(NewGuideDialog {
        vertical: true,
        text: "0".into(),
        error: None,
    });
}

/// New Guide…: a guideline at `text`, a length from the ruler origin in the chosen units (a
/// unit typed with it wins), one undo step (`view.newGuide`); the guidelines are shown.
pub fn new_guide_at(app: &mut EditorApp, vertical: bool, text: &str) -> Result<(), String> {
    let mm = openstitch_app::editor::units::parse(text, app.ui.prefs.units)
        .ok_or_else(|| t("ui-guide-position-invalid").to_string())?;
    let zero = app.ruler_zero();
    let at = mm + if vertical { zero.x } else { zero.y } / 10.0;
    app.execute(&CommandRequest::new(
        "view.newGuide",
        json!({ "orientation": orientation(vertical), "position": at }),
    ))
    .ok_or_else(|| app.status.clone())?;
    show_guides(app);
    Ok(())
}

fn new_guide_window(app: &mut EditorApp, ctx: &egui::Context) {
    let Some(mut d) = app.workspace.view.new_guide.take() else {
        return;
    };
    let mut open = true;
    let mut done = false;
    egui::Window::new(t("ui-guide-new-title"))
        .id(egui::Id::unique("window-new-guide"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.radio_value(&mut d.vertical, true, t("ui-guide-vertical"));
                ui.radio_value(&mut d.vertical, false, t("ui-guide-horizontal"));
            });
            ui.label(t("ui-guide-position"));
            let r = ui.horizontal(|ui| {
                let r = ui.text_edit_singleline(&mut d.text);
                ui.label(openstitch_app::editor::units::suffix(app.ui.prefs.units));
                r
            });
            let enter = r.inner.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if let Some(e) = &d.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.horizontal(|ui| {
                if ui.button(t("ui-ok")).clicked() || enter {
                    match new_guide_at(app, d.vertical, &d.text) {
                        Ok(()) => done = true,
                        Err(e) => d.error = Some(e),
                    }
                }
                if ui.button(t("ui-cancel")).clicked() {
                    done = true;
                }
            });
        });
    if open && !done {
        app.workspace.view.new_guide = Some(d);
    }
}

/// The New Guide Layout… dialog (#799): lengths as typed, in the chosen units.
pub struct LayoutDialog {
    pub columns: u32,
    pub rows: u32,
    pub column_gutter: String,
    pub row_gutter: String,
    /// Top, left, bottom, right.
    pub margins: [String; 4],
    pub clear: bool,
    error: Option<String>,
}

impl LayoutDialog {
    /// The layout it describes (0.1 mm), or the message for a length that is not one.
    pub fn layout(&self, app: &EditorApp) -> Result<openstitch_design::GuideLayout, String> {
        let u = app.ui.prefs.units;
        let len = |s: &str| {
            openstitch_app::editor::units::parse(s, u)
                .filter(|v| *v >= 0.0)
                .map(|v| v * 10.0)
                .ok_or_else(|| t("ui-guide-position-invalid").to_string())
        };
        let [top, left, bottom, right] = &self.margins;
        Ok(openstitch_design::GuideLayout {
            columns: self.columns,
            rows: self.rows,
            column_gutter: len(&self.column_gutter)?,
            row_gutter: len(&self.row_gutter)?,
            margin_top: len(top)?,
            margin_left: len(left)?,
            margin_bottom: len(bottom)?,
            margin_right: len(right)?,
        })
    }
}

fn open_guide_layout(app: &mut EditorApp) {
    let zero = || "0".to_string();
    app.workspace.view.guide_layout = Some(LayoutDialog {
        columns: 2,
        rows: 2,
        column_gutter: zero(),
        row_gutter: zero(),
        margins: [zero(), zero(), zero(), zero()],
        clear: false,
        error: None,
    });
    show_guides(app);
}

/// New Guide Layout… applied: `view.guideLayout`, one undo step.
pub fn apply_guide_layout(app: &mut EditorApp, d: &LayoutDialog) -> Result<(), String> {
    let l = d.layout(app)?;
    let page = app.session.design().page;
    l.lines(page.width, page.height).map_err(str::to_string)?;
    let mm = |v: f64| v / 10.0;
    app.execute(&CommandRequest::new(
        "view.guideLayout",
        json!({
            "columns": l.columns,
            "rows": l.rows,
            "columnGutter": mm(l.column_gutter),
            "rowGutter": mm(l.row_gutter),
            "marginTop": mm(l.margin_top),
            "marginLeft": mm(l.margin_left),
            "marginBottom": mm(l.margin_bottom),
            "marginRight": mm(l.margin_right),
            "clear": d.clear,
        }),
    ))
    .map(|_| ())
    .ok_or_else(|| app.status.clone())
}

fn guide_layout_window(app: &mut EditorApp, ctx: &egui::Context) {
    let Some(mut d) = app.workspace.view.guide_layout.take() else {
        return;
    };
    let mut open = true;
    let mut done = false;
    let unit = openstitch_app::editor::units::suffix(app.ui.prefs.units);
    egui::Window::new(t("ui-guide-layout-title"))
        .id(egui::Id::unique("window-guide-layout"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            egui::Grid::new("guide-layout-grid").num_columns(3).show(ui, |ui| {
                ui.label(t("ui-guide-columns"));
                ui.add(egui::DragValue::new(&mut d.columns).range(0..=50));
                ui.label("");
                ui.end_row();
                ui.label(t("ui-guide-column-gutter"));
                ui.text_edit_singleline(&mut d.column_gutter);
                ui.label(unit);
                ui.end_row();
                ui.label(t("ui-guide-rows"));
                ui.add(egui::DragValue::new(&mut d.rows).range(0..=50));
                ui.label("");
                ui.end_row();
                ui.label(t("ui-guide-row-gutter"));
                ui.text_edit_singleline(&mut d.row_gutter);
                ui.label(unit);
                ui.end_row();
                for (key, m) in [
                    "ui-guide-margin-top",
                    "ui-guide-margin-left",
                    "ui-guide-margin-bottom",
                    "ui-guide-margin-right",
                ]
                .into_iter()
                .zip(d.margins.iter_mut())
                {
                    ui.label(t(key));
                    ui.text_edit_singleline(m);
                    ui.label(unit);
                    ui.end_row();
                }
            });
            ui.checkbox(&mut d.clear, t("ui-guide-clear-existing"));
            // The preview is drawn on the canvas; a layout that does not fit says why.
            let page = app.session.design().page;
            let problem = match d.layout(app) {
                Ok(l) => l.lines(page.width, page.height).err().map(str::to_string),
                Err(e) => Some(e),
            };
            if let Some(e) = d.error.as_ref().or(problem.as_ref()) {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.horizontal(|ui| {
                if ui.button(t("ui-ok")).clicked() {
                    match apply_guide_layout(app, &d) {
                        Ok(()) => done = true,
                        Err(e) => d.error = Some(e),
                    }
                }
                if ui.button(t("ui-cancel")).clicked() {
                    done = true;
                }
            });
        });
    if open && !done {
        app.workspace.view.guide_layout = Some(d);
    }
}

fn orientation(vertical: bool) -> &'static str {
    if vertical { "vertical" } else { "horizontal" }
}

/// The selected guideline (vertical?, index), while it exists, shows and is not locked.
pub fn selected_guide(app: &EditorApp) -> Option<(bool, usize)> {
    let g = &app.session.design().guides;
    app.workspace.view.selected_guide.filter(|&(v, i)| {
        guides_shown(app) && !g.locked && i < if v { g.vertical.len() } else { g.horizontal.len() }
    })
}

/// Clear Selected Guide (Delete or Backspace while a guideline is selected, #799); false when
/// none is.
pub fn delete_selected_guide(app: &mut EditorApp) -> bool {
    let Some((v, i)) = selected_guide(app) else {
        return false;
    };
    app.workspace.view.selected_guide = None;
    app.execute(&CommandRequest::new(
        "view.deleteGuide",
        json!({ "orientation": orientation(v), "index": i }),
    ));
    true
}

/// Alt-click: guideline `index` turned vertical ↔ horizontal at `position` (0.1 mm; default
/// the page centre), one undo step (`view.editGuide`).
fn turn_guide(app: &mut EditorApp, vertical: bool, index: usize, position: Option<f64>) {
    let mut p = json!({ "orientation": orientation(vertical), "index": index, "turn": true });
    if let Some(at) = position {
        p["position"] = json!(at / 10.0);
    }
    app.workspace.view.selected_guide = None;
    app.execute(&CommandRequest::new("view.editGuide", p));
}

/// Set Position…: guideline `index` moved to `text`, a length from the ruler origin in the
/// chosen units (a unit typed with it wins: `12.5`, `12.5 mm`, `1/2"`), one undo step
/// (`view.editGuide`). Err with the message to show when the text is not a length or the
/// command refuses it.
pub fn set_guide_position(
    app: &mut EditorApp,
    vertical: bool,
    index: usize,
    text: &str,
) -> Result<(), String> {
    let mm = openstitch_app::editor::units::parse(text, app.ui.prefs.units)
        .ok_or_else(|| t("ui-guide-position-invalid").to_string())?;
    let zero = app.ruler_zero();
    let at = mm + if vertical { zero.x } else { zero.y } / 10.0;
    app.execute(&CommandRequest::new(
        "view.editGuide",
        json!({ "orientation": orientation(vertical), "index": index, "position": at }),
    ))
    .map(|_| ())
    .ok_or_else(|| app.status.clone())
}

/// The Set Position… dialog of one guideline (#799).
pub struct GuideDialog {
    vertical: bool,
    index: usize,
    text: String,
    error: Option<String>,
}

/// Opens Set Position… for guideline `index`, filled with its offset from the ruler origin.
pub fn open_guide_dialog(app: &mut EditorApp, vertical: bool, index: usize) {
    let g = &app.session.design().guides;
    let list = if vertical { &g.vertical } else { &g.horizontal };
    let Some(&at) = list.get(index) else { return };
    let zero = app.ruler_zero();
    let from_zero = (at - if vertical { zero.x } else { zero.y }) / 10.0;
    let text = openstitch_app::editor::units::number(app.ui.prefs.units, from_zero);
    app.workspace.view.selected_guide = Some((vertical, index));
    app.workspace.view.guide_dialog = Some(GuideDialog {
        vertical,
        index,
        text,
        error: None,
    });
}

fn guide_window(app: &mut EditorApp, ctx: &egui::Context) {
    let Some(mut d) = app.workspace.view.guide_dialog.take() else {
        return;
    };
    let mut open = true;
    let mut done = false;
    egui::Window::new(t("ui-guide-set-position-title"))
        .id(egui::Id::unique("window-guide-set-position"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            let unit = openstitch_app::editor::units::suffix(app.ui.prefs.units);
            ui.label(if d.vertical {
                t("ui-guide-position-x")
            } else {
                t("ui-guide-position-y")
            });
            let r = ui.horizontal(|ui| {
                let r = ui.text_edit_singleline(&mut d.text);
                ui.label(unit);
                r
            });
            let enter = r.inner.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if let Some(e) = &d.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.horizontal(|ui| {
                if ui.button(t("ui-ok")).clicked() || enter {
                    match set_guide_position(app, d.vertical, d.index, &d.text) {
                        Ok(()) => done = true,
                        Err(e) => d.error = Some(e),
                    }
                }
                if ui.button(t("ui-cancel")).clicked() {
                    done = true;
                }
            });
        });
    if open && !done {
        app.workspace.view.guide_dialog = Some(d);
    }
}

/// View ▸ Guides (#799): New Guide…, New Guide Layout…, Set Position…, Clear Selected Guide,
/// Lock Guides, Clear Guides, New Guides From Selection.
pub fn guides_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    if ui.button(t("ui-guide-new")).clicked() {
        open_new_guide(app);
        ui.close();
    }
    if ui.button(t("ui-guide-layout")).clicked() {
        open_guide_layout(app);
        ui.close();
    }
    ui.separator();
    let selected = selected_guide(app);
    if ui
        .add_enabled(selected.is_some(), egui::Button::new(t("ui-guide-set-position")))
        .clicked()
        && let Some((v, i)) = selected
    {
        open_guide_dialog(app, v, i);
        ui.close();
    }
    if ui
        .add_enabled(selected.is_some(), egui::Button::new(t("ui-clear-selected-guide")))
        .clicked()
    {
        delete_selected_guide(app);
        ui.close();
    }
    if guide_entries(app, ui, None) {
        ui.close();
    }
    if ui
        .add_enabled(
            !app.selection.is_empty(),
            egui::Button::new(t("ui-new-guides-from-selection")),
        )
        .on_hover_text(t("ui-new-guides-from-selection-tip"))
        .clicked()
    {
        new_guides_from_selection(app);
        ui.close();
    }
}

/// New Guides From Selection (#799): guidelines on the selection's edges and centre lines,
/// shown (with the rulers) so they can be seen.
pub fn new_guides_from_selection(app: &mut EditorApp) -> bool {
    let ids: Vec<u64> = app.selection.iter().copied().collect();
    let ok = app
        .execute(&CommandRequest::new(
            "view.newGuidesFromSelection",
            json!({ "ids": ids }),
        ))
        .is_some();
    if ok {
        app.workspace.rulers = true;
        app.workspace.guides = true;
        app.ui.prefs.editing.show_guides = true;
        app.ui.save_prefs();
    }
    ok
}

/// `ui.guides` (#799): the guidelines as shown (each one's position from the ruler origin,
/// as its label reads), and the selection, lock and snapping; `orientation` + `index` select
/// one as a click does, then `position` (a length typed as in Set Position…), `turn`
/// (Alt-click) or `delete` (the Delete key) act on the selected one.
pub fn guides_command(app: &mut EditorApp, params: Option<&Value>) -> Result<Value, String> {
    let get = |k: &str| params.and_then(|p| p.get(k));
    if let Some(o) = get("orientation") {
        let vertical = match o.as_str() {
            Some("vertical") => true,
            Some("horizontal") => false,
            _ => return Err("orientation must be vertical or horizontal".into()),
        };
        let index = get("index")
            .and_then(Value::as_u64)
            .ok_or("give index with orientation")? as usize;
        let g = &app.session.design().guides;
        let n = if vertical { g.vertical.len() } else { g.horizontal.len() };
        if index >= n {
            return Err(format!("no {} guideline {index}", orientation(vertical)));
        }
        if g.locked {
            return Err("the guidelines are locked (view.lockGuides false to unlock)".into());
        }
        app.workspace.rulers = true;
        app.workspace.guides = true;
        app.workspace.view.selected_guide = Some((vertical, index));
    }
    if let Some(n) = get("newGuide") {
        let vertical = match n.get("orientation").and_then(Value::as_str) {
            Some("vertical") => true,
            Some("horizontal") => false,
            _ => return Err("newGuide.orientation must be vertical or horizontal".into()),
        };
        let text = match n.get("position") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(x)) => x.to_string(),
            _ => return Err("newGuide.position is a length from the ruler origin".into()),
        };
        new_guide_at(app, vertical, &text)?;
    }
    let acts = ["position", "turn", "delete"].iter().any(|k| get(k).is_some());
    if acts {
        let (v, i) = selected_guide(app).ok_or("select a guideline first (orientation, index)")?;
        if let Some(text) = get("position") {
            let text = match text {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => return Err("position is a length, e.g. 12.5, \"12.5 mm\" or \"1/2\\\"\"".into()),
            };
            set_guide_position(app, v, i, &text)?;
        }
        if get("turn").and_then(Value::as_bool) == Some(true) {
            turn_guide(app, v, i, None);
        } else if get("delete").and_then(Value::as_bool) == Some(true) {
            delete_selected_guide(app);
        }
    }
    Ok(guides_state(app))
}

fn guides_state(app: &EditorApp) -> Value {
    let g = &app.session.design().guides;
    let zero = app.ruler_zero();
    let units = app.ui.prefs.units;
    let list = |l: &[f64], z: f64| {
        l.iter()
            .enumerate()
            .map(|(i, &v)| {
                json!({
                    "index": i,
                    "position": ((v - z) / 10.0 * 100.0).round() / 100.0,
                    "label": guide_label(v - z, units),
                })
            })
            .collect::<Vec<_>>()
    };
    json!({
        "shown": guides_shown(app),
        "locked": g.locked,
        "snapping": app.ui.prefs.editing.snap_guides && guides_shown(app),
        "origin": app.ui.prefs.ruler_origin.key(),
        "selected": selected_guide(app).map(|(v, i)| json!({ "orientation": orientation(v), "index": i })),
        "colour": openstitch_app::editor::prefs::hex_color(app.ui.prefs.guide_color),
        "menu": app.workspace.view.guide_menu.as_ref().map(|m| match m.target {
            MenuTarget::Ruler => json!({ "for": "ruler" }),
            MenuTarget::Guide(v, i) => json!({ "for": "guide", "orientation": orientation(v), "index": i }),
        }),
        "dialog": app.workspace.view.guide_dialog.as_ref().map(|d| json!({ "orientation": orientation(d.vertical), "index": d.index, "text": d.text, "error": d.error })),
        "vertical": list(&g.vertical, zero.x),
        "horizontal": list(&g.horizontal, zero.y),
    })
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
        app.status = openstitch_app::tf!("ui-status-background-image-turned", direction = direction);
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
            app.status = openstitch_app::tf!("ui-status-image-saved-to", path = p.display());
        }
    }
    if ui.button(t("ui-to-clipboard")).clicked() {
        ui.close();
        if crate::limits_ui::browser_limit(
            app,
            openstitch_app::editor::browser_limits::Limit::ClipboardImages,
        ) {
            return;
        }
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
    let data = openstitch_app::platform::read_limited(std::path::Path::new(&b.path), openstitch_digitize::raster::MAX_FILE_BYTES as u64)?;
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
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-modify-image")).id(egui::Id::unique("window-modify-image"))
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
                    openstitch_app::editor::ui_texts::tr("Rotate Left"),
                );
                if ui.button(left).clicked() {
                    turn = Some("left");
                }
                let right = (
                    crate::icons::ui_image(ui, "image_rotate_right"),
                    openstitch_app::editor::ui_texts::tr("Rotate Right"),
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
        app.status = openstitch_app::editor::ui_texts::t("ui-status-background-image-changed").into();
    }
    if let Some(t) = turn {
        rotate_image(app, t);
    }
}

// MARK: windows

pub fn windows(app: &mut EditorApp, ctx: &egui::Context) {
    guide_menu_window(app, ctx);
    new_guide_window(app, ctx);
    guide_layout_window(app, ctx);
    guide_window(app, ctx);
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
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-realistic-view-settings")).id(egui::Id::unique("window-realistic-view-settings"))
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
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-font-filter-settings")).id(egui::Id::unique("window-font-filter-settings"))
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
        app.status = openstitch_app::editor::ui_texts::t("ui-status-font-filter-on").into();
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
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-fabric-selector")).id(egui::Id::unique("window-fabric-selector"))
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
            app.status = openstitch_app::tf!("ui-status-fabric-settings-applied-to-pattern-n", n = n);
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
        changed |= ui.radio_value(&mut s.layers, l, openstitch_app::editor::ui_texts::tr(label)).changed();
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
        changed |= ui.radio_value(&mut s.color_by, c, openstitch_app::editor::ui_texts::tr(label)).changed();
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

#[cfg(test)]
mod guide_tests {
    use super::*;
    use crate::app::Launch;

    /// One frame of the canvas (400 × 300 points, 1 point per 0.1 mm, model origin at (50, 50))
    /// with `events`, through [`guide_pointer`].
    fn frame(ctx: &egui::Context, app: &mut EditorApp, events: Vec<egui::Event>) {
        let rect = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
        let raw = egui::RawInput {
            screen_rect: Some(rect),
            events,
            ..Default::default()
        };
        let vp = Viewport {
            scale: 1.0,
            offset: v2(50.0, 50.0),
        };
        let mut out = ctx.run_ui(raw, |ui| {
            let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
            guide_pointer(app, ui, &response, rect, &vp);
        });
        out.textures_delta.clear();
    }

    fn button(at: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Presses at `from`, drags to `to` and releases there.
    fn drag(ctx: &egui::Context, app: &mut EditorApp, from: Pos2, to: Pos2) {
        frame(ctx, app, vec![egui::Event::PointerMoved(from)]);
        frame(ctx, app, vec![button(from, true)]);
        frame(ctx, app, vec![egui::Event::PointerMoved(to)]);
        frame(ctx, app, vec![button(to, false)]);
    }

    /// #799: a guideline is dragged out of the top ruler even with guidelines hidden, which
    /// shows them; dragged back onto either ruler it goes; each is one undo step.
    #[test]
    fn a_guideline_comes_out_of_a_ruler_and_goes_back_onto_one() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(Launch::default());
        app.workspace.guides = false;
        app.ui.prefs.editing.show_guides = false;
        drag(&ctx, &mut app, Pos2::new(200.0, 10.0), Pos2::new(200.0, 150.0));
        assert_eq!(app.session.design().guides.vertical, vec![150.0]);
        assert!(app.workspace.guides && app.ui.prefs.editing.show_guides);
        // Its marker dragged onto the left ruler removes it.
        drag(&ctx, &mut app, Pos2::new(200.0, 10.0), Pos2::new(8.0, 150.0));
        assert!(app.session.design().guides.vertical.is_empty());
        app.undo();
        assert_eq!(app.session.design().guides.vertical, vec![150.0]);
        // A new one dropped on a ruler is not added.
        drag(&ctx, &mut app, Pos2::new(10.0, 120.0), Pos2::new(10.0, 160.0));
        assert!(app.session.design().guides.horizontal.is_empty());
    }

    fn frame_with(
        ctx: &egui::Context,
        app: &mut EditorApp,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) {
        let rect = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
        let raw = egui::RawInput {
            screen_rect: Some(rect),
            events: std::iter::once(egui::Event::ModifiersChanged(modifiers))
                .chain(events)
                .collect(),
            ..Default::default()
        };
        let vp = Viewport {
            scale: 1.0,
            offset: v2(50.0, 50.0),
        };
        let mut out = ctx.run_ui(raw, |ui| {
            let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
            guide_pointer(app, ui, &response, rect, &vp);
        });
        out.textures_delta.clear();
    }

    /// A press and release at `at` without a move.
    fn click(ctx: &egui::Context, app: &mut EditorApp, at: Pos2, modifiers: egui::Modifiers) {
        frame_with(ctx, app, vec![egui::Event::PointerMoved(at)], modifiers);
        frame_with(ctx, app, vec![button(at, true)], modifiers);
        frame_with(ctx, app, vec![button(at, false)], modifiers);
    }

    fn guides(app: &EditorApp) -> (Vec<f64>, Vec<f64>) {
        let g = &app.session.design().guides;
        (g.vertical.clone(), g.horizontal.clone())
    }

    fn with_guides(vertical: &[f64], horizontal: &[f64]) -> EditorApp {
        let mut app = EditorApp::new(Launch::default());
        app.execute(&CommandRequest::new(
            "guides.set",
            json!({ "vertical": mm(vertical), "horizontal": mm(horizontal) }),
        ))
        .expect("guides");
        app
    }

    /// #799: a click on a guideline (its marker, or the line itself with the Select tool)
    /// selects it without moving it; Delete removes the selected one in one undo step; a click
    /// elsewhere on the canvas drops the selection.
    #[test]
    fn a_clicked_guideline_is_selected_and_delete_removes_it() {
        let ctx = egui::Context::default();
        let mut app = with_guides(&[150.0], &[100.0]);
        click(&ctx, &mut app, Pos2::new(201.0, 10.0), egui::Modifiers::NONE);
        assert_eq!(selected_guide(&app), Some((true, 0)));
        assert_eq!(guides(&app).0, vec![150.0], "not moved by the click");
        // The horizontal line on the canvas.
        click(&ctx, &mut app, Pos2::new(300.0, 151.0), egui::Modifiers::NONE);
        assert_eq!(selected_guide(&app), Some((false, 0)));
        assert!(delete_selected_guide(&mut app));
        assert!(guides(&app).1.is_empty());
        assert!(!delete_selected_guide(&mut app), "nothing selected now");
        app.undo();
        assert_eq!(guides(&app).1, vec![100.0]);
        click(&ctx, &mut app, Pos2::new(300.0, 151.0), egui::Modifiers::NONE);
        click(&ctx, &mut app, Pos2::new(300.0, 250.0), egui::Modifiers::NONE);
        assert_eq!(selected_guide(&app), None, "a click away drops it");
    }

    /// #799: a guideline on the canvas is dragged with the Select tool, one undo step; with
    /// another tool the canvas keeps the press.
    #[test]
    fn a_guideline_is_dragged_on_the_canvas_with_the_select_tool() {
        let ctx = egui::Context::default();
        let mut app = with_guides(&[150.0], &[]);
        drag(&ctx, &mut app, Pos2::new(200.0, 150.0), Pos2::new(240.0, 150.0));
        assert_eq!(guides(&app).0, vec![190.0]);
        app.tools.set_tool(openstitch_app::editor::tools::Tool::Rectangle);
        drag(&ctx, &mut app, Pos2::new(240.0, 150.0), Pos2::new(280.0, 150.0));
        assert_eq!(guides(&app).0, vec![190.0], "the drawing tool keeps the press");
    }

    /// #799: Alt-click turns a guideline: on the canvas where it was clicked, on its marker
    /// across the page centre; one undo step each.
    #[test]
    fn alt_click_turns_a_guideline() {
        let ctx = egui::Context::default();
        let mut app = with_guides(&[150.0], &[]);
        click(&ctx, &mut app, Pos2::new(200.0, 130.0), egui::Modifiers::ALT);
        assert_eq!(guides(&app), (vec![], vec![80.0]));
        click(&ctx, &mut app, Pos2::new(10.0, 130.0), egui::Modifiers::ALT);
        let centre = app.session.design().page.width / 2.0;
        assert_eq!(guides(&app), (vec![centre], vec![]));
        app.undo();
        assert_eq!(guides(&app), (vec![], vec![80.0]));
    }

    /// #799: locked guidelines are not moved, selected or deleted from the canvas or the
    /// rulers (no ×), but a ruler still adds a new one; the lock is one undo step.
    #[test]
    fn locked_guidelines_stay_put_and_a_ruler_still_adds_one() {
        let ctx = egui::Context::default();
        let mut app = with_guides(&[150.0], &[]);
        app.execute(&CommandRequest::new("view.lockGuides", json!({ "locked": true })))
            .expect("lock");
        drag(&ctx, &mut app, Pos2::new(200.0, 150.0), Pos2::new(240.0, 150.0));
        assert_eq!(guides(&app).0, vec![150.0], "not moved on the canvas");
        click(&ctx, &mut app, Pos2::new(200.0, 150.0), egui::Modifiers::NONE);
        assert_eq!(selected_guide(&app), None, "not selected");
        // Where its × would be: a new guideline instead of a deletion.
        let (_, x) = marker_points(
            egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0)),
            true,
            200.0,
        );
        click(&ctx, &mut app, x, egui::Modifiers::NONE);
        assert_eq!(guides(&app).0, vec![150.0], "no × while locked");
        drag(&ctx, &mut app, Pos2::new(300.0, 10.0), Pos2::new(300.0, 200.0));
        assert_eq!(guides(&app).0.len(), 2, "{:?}", guides(&app));
        assert!(app.session.design().guides.locked);
    }

    /// #799: Set Position… takes a length from the ruler origin in the chosen units (a unit
    /// typed with it wins) in one undo step; anything else keeps the dialog open with an
    /// error. A double-click on a guideline opens it.
    #[test]
    fn set_position_types_an_offset_from_the_ruler_origin() {
        let ctx = egui::Context::default();
        let mut app = with_guides(&[150.0], &[]);
        let zero = app.ruler_zero();
        set_guide_position(&mut app, true, 0, "12.5").expect("12.5 mm");
        assert_eq!(guides(&app).0, vec![zero.x + 125.0]);
        set_guide_position(&mut app, true, 0, "-1/2\"").expect("inches");
        assert!((guides(&app).0[0] - (zero.x - 127.0)).abs() < 1e-6);
        app.undo();
        assert_eq!(guides(&app).0, vec![zero.x + 125.0]);
        assert!(set_guide_position(&mut app, true, 0, "twelve").is_err());
        assert!(set_guide_position(&mut app, true, 0, "9000").is_err(), "off the range");
        // The dialog: a double-click opens it with the offset; bad text keeps it open.
        // Back at x 150 (screen 200), inside the test canvas.
        let offset = openstitch_app::editor::units::number(app.ui.prefs.units, (150.0 - zero.x) / 10.0);
        set_guide_position(&mut app, true, 0, &offset).expect("back");
        let at = Pos2::new(200.0, 10.0);
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(at)]);
        for _ in 0..2 {
            frame(&ctx, &mut app, vec![button(at, true)]);
            frame(&ctx, &mut app, vec![button(at, false)]);
        }
        let d = app.workspace.view.guide_dialog.as_ref().expect("Set Position… open");
        assert_eq!(d.text, offset);
        app.workspace.view.guide_dialog.as_mut().expect("open").text = "abc".into();
        let s = guides_command(&mut app, None).expect("ui.guides");
        assert_eq!(s["dialog"]["text"], "abc");
    }

    /// #799: `ui.guides` reads the guidelines as shown (from the ruler origin, with their
    /// labels) and selects, sets the position of, turns and deletes one as the pointer does.
    #[test]
    fn ui_guides_reads_and_edits_the_guidelines_as_shown() {
        let mut app = with_guides(&[150.0], &[100.0]);
        let zero = app.ruler_zero();
        let s = guides_command(&mut app, None).expect("read");
        assert_eq!(s["shown"], true);
        assert_eq!(s["vertical"][0]["position"], json!((150.0 - zero.x) / 10.0));
        assert_eq!(s["vertical"][0]["label"], json!(guide_label(150.0 - zero.x, app.ui.prefs.units)));
        let s = guides_command(
            &mut app,
            Some(&json!({ "orientation": "vertical", "index": 0, "position": "10 mm" })),
        )
        .expect("set");
        assert_eq!(s["vertical"][0]["position"], json!(10.0));
        assert_eq!(s["selected"]["orientation"], "vertical");
        guides_command(&mut app, Some(&json!({ "turn": true }))).expect("turn");
        assert_eq!(guides(&app).1.len(), 2);
        guides_command(&mut app, Some(&json!({ "orientation": "horizontal", "index": 0, "delete": true })))
            .expect("delete");
        assert_eq!(guides(&app).1.len(), 1);
        assert!(guides_command(&mut app, Some(&json!({ "delete": true }))).is_err(), "none selected");
        assert!(guides_command(&mut app, Some(&json!({ "orientation": "vertical", "index": 5 }))).is_err());
    }

    /// #799: View ▸ Guides ▸ New Guides From Selection lines the selection's box and shows them.
    #[test]
    fn new_guides_from_selection_shows_them() {
        let mut app = EditorApp::new(Launch::default());
        app.workspace.guides = false;
        assert!(!new_guides_from_selection(&mut app), "nothing selected");
        let r = app
            .execute(&CommandRequest::new(
                "shape.rectangle",
                json!({ "x": 10, "y": 20, "width": 30, "height": 40 }),
            ))
            .expect("rect");
        app.select_all();
        assert!(new_guides_from_selection(&mut app), "{r}");
        assert_eq!(guides(&app).0.len(), 3);
        assert_eq!(guides(&app).1.len(), 3);
        assert!(app.workspace.guides && app.workspace.rulers);
    }

    fn right_click(ctx: &egui::Context, app: &mut EditorApp, at: Pos2) {
        let b = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(ctx, app, vec![egui::Event::PointerMoved(at)]);
        frame(ctx, app, vec![b(true)]);
        frame(ctx, app, vec![b(false)]);
    }

    /// #799: a right-click on a guideline (its line or its ruler marker) opens its menu and
    /// selects it; on a ruler elsewhere, the ruler menu; on the empty canvas, nothing.
    #[test]
    fn right_clicks_open_the_guide_and_ruler_menus() {
        let ctx = egui::Context::default();
        let mut app = with_guides(&[150.0], &[100.0]);
        right_click(&ctx, &mut app, Pos2::new(201.0, 220.0));
        let target = app.workspace.view.guide_menu.as_ref().map(|m| m.target);
        assert_eq!(target, Some(MenuTarget::Guide(true, 0)));
        assert_eq!(selected_guide(&app), Some((true, 0)));
        right_click(&ctx, &mut app, Pos2::new(8.0, 151.0));
        let target = app.workspace.view.guide_menu.as_ref().map(|m| m.target);
        assert_eq!(target, Some(MenuTarget::Guide(false, 0)), "its marker");
        right_click(&ctx, &mut app, Pos2::new(320.0, 8.0));
        let target = app.workspace.view.guide_menu.as_ref().map(|m| m.target);
        assert_eq!(target, Some(MenuTarget::Ruler));
        app.workspace.view.guide_menu = None;
        right_click(&ctx, &mut app, Pos2::new(320.0, 250.0));
        assert!(app.workspace.view.guide_menu.is_none());
        let s = guides_command(&mut app, None).expect("ui.guides");
        assert!(s["menu"].is_null());
    }

    /// #799: New Guide… places a guideline at a length from the ruler origin (a unit typed
    /// with it wins), shows the guidelines, and refuses what is not a length.
    #[test]
    fn new_guide_types_a_position_from_the_ruler_origin() {
        let mut app = EditorApp::new(Launch::default());
        app.workspace.guides = false;
        let zero = app.ruler_zero();
        new_guide_at(&mut app, false, "-1 cm").expect("new guide");
        assert_eq!(guides(&app).1, vec![zero.y - 100.0]);
        assert!(app.workspace.guides);
        assert!(new_guide_at(&mut app, true, "left").is_err());
        let s = guides_command(
            &mut app,
            Some(&json!({ "newGuide": { "orientation": "vertical", "position": "5" } })),
        )
        .expect("ui.guides newGuide");
        assert_eq!(s["vertical"][0]["position"], json!(5.0));
        app.undo();
        assert!(guides(&app).0.is_empty(), "one undo step");
    }

    /// #799: New Guide Layout…'s fields (in the chosen units) make the same guidelines as
    /// `view.guideLayout`, one undo step; a layout that does not fit says why.
    #[test]
    fn the_guide_layout_dialog_applies_its_layout() {
        let mut app = with_guides(&[10.0], &[]);
        open_guide_layout(&mut app);
        let mut d = app.workspace.view.guide_layout.take().expect("open");
        d.columns = 3;
        d.rows = 0;
        d.margins = ["5".into(), "5".into(), "5".into(), "5".into()];
        d.clear = true;
        let page = app.session.design().page;
        let (xs, ys) = d.layout(&app).expect("layout").lines(page.width, page.height).expect("fits");
        apply_guide_layout(&mut app, &d).expect("apply");
        assert_eq!(guides(&app), (xs.clone(), ys));
        assert_eq!(xs.len(), 4, "three columns without gutters share their edges: {xs:?}");
        app.undo();
        assert_eq!(guides(&app).0, vec![10.0]);
        d.margins[1] = "five".into();
        assert!(apply_guide_layout(&mut app, &d).is_err());
        d.margins[1] = "5000".into();
        assert!(apply_guide_layout(&mut app, &d).is_err(), "no room");
    }

    /// #799: the guide colour is an Options setting through `ui.set guideColor`.
    #[test]
    fn the_guide_colour_is_set_through_ui_set() {
        let mut app = EditorApp::new(Launch::default());
        app.ui_command("ui.set", Some(&json!({ "name": "guideColor", "value": "#ff0000" })))
            .expect("set");
        assert_eq!(guide_colour(&app), Color32::from_rgb(255, 0, 0));
        assert!(
            app.ui_command("ui.set", Some(&json!({ "name": "guideColor", "value": "red" })))
                .is_err()
        );
        assert_eq!(guides_command(&mut app, None).expect("ui.guides")["colour"], "#ff0000");
    }
}
