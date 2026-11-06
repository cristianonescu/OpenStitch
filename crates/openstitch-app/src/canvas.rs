//! Design canvas: draws the Design Page and stitches, feeds pointer events to the tool
//! controller and handles zoom and pan.

use crate::app::EditorApp;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Sense, Shape, Stroke, StrokeKind};
use openstitch_app::editor::hit::bounds_of;
use openstitch_app::editor::points::handle_lines;
use openstitch_app::editor::{
    Handle, Phase, PointerEvent, ScreenRect, SelectionHandles, ToolPreview, Viewport,
};
use openstitch_design::{BackgroundLayer, Contour, ObjectKind, Rect, Transform, Vec2, v2};

fn pos(p: Vec2) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

fn model_rect(v: &Viewport, r: &Rect) -> egui::Rect {
    egui::Rect::from_two_pos(
        pos(v.to_screen(v2(r.min_x, r.min_y))),
        pos(v.to_screen(v2(r.max_x, r.max_y))),
    )
}

/// The part of the canvas left of a dialog that reaches into it from the right (its whole
/// width when there is none, or when that part would be under half the canvas).
pub(crate) fn beside_dialog(screen: ScreenRect, dialog: Option<egui::Rect>) -> ScreenRect {
    match dialog {
        Some(d) => {
            let left = f64::from(d.min.x);
            let free = left - screen.x;
            if left < screen.x + screen.width && free >= screen.width / 2.0 {
                ScreenRect {
                    width: free,
                    ..screen
                }
            } else {
                screen
            }
        }
        None => screen,
    }
}

pub fn canvas(app: &mut EditorApp, ui: &mut egui::Ui) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let screen = ScreenRect {
        x: f64::from(rect.min.x),
        y: f64::from(rect.min.y),
        width: f64::from(rect.width()),
        height: f64::from(rect.height()),
    };
    let page = app.session.design().page;
    let page_rect = Rect {
        min_x: 0.0,
        min_y: 0.0,
        max_x: page.width,
        max_y: page.height,
    };
    let fit_screen = beside_dialog(screen, app.dialog_rect.filter(|_| app.screenshot_run()));
    let untouched_fit = app
        .auto_fit
        .filter(|&(s, v, _)| s != fit_screen && app.viewport == Some(v));
    if app.viewport.is_none() || app.fit_requested || untouched_fit.is_some() {
        let mut fitted = Viewport::fit(page_rect, fit_screen, 24.0);
        let zoom = match untouched_fit {
            Some((_, _, z)) if !app.fit_requested => z,
            _ => app.initial_zoom.take(),
        };
        if let Some(z) = zoom {
            fitted.zoom_about(fit_screen.center(), z);
        }
        app.viewport = Some(fitted);
        app.auto_fit = Some((fit_screen, fitted, zoom));
        app.fit_requested = false;
    }
    let mut vp = app.viewport.unwrap_or_default();

    navigate(ui, &response, rect, &mut vp, app.ui.pan);
    app.viewport = Some(vp);
    if !app.ui.pan
        && !crate::view_ui::guide_pointer(app, ui, &response, rect, &vp)
        && !crate::zoom_ui::pointer(app, &response, rect, vp)
    {
        pointer(app, ui, &response, vp);
    }
    let page_colour = crate::theme::canvas(ui.ctx()).page;
    app.page_rgb = [page_colour.r(), page_colour.g(), page_colour.b()];
    app.ensure_render_model();
    // Follow the needle while the player shows part of the design (#516).
    if app.stitch_player.follow
        && app.simulate.is_some()
        && let Some(n) = app.render.needle
        && let Some(moved) = openstitch_app::editor::player::follow(&vp, screen, n)
    {
        vp = moved;
        app.viewport = Some(vp);
    }

    app.workspace.canvas = Some(rect);
    let painter = ui.painter_at(rect);
    let colors = crate::theme::canvas(ui.ctx());
    painter.rect_filled(rect, CornerRadius::ZERO, colors.work_area);
    draw_page(&painter, &vp, &page_rect, colors.page);
    if app.workspace.realistic {
        draw_fabric(ui.ctx(), &painter, &vp, &page_rect, colors.page);
    }
    draw_hoop_field(app, &painter, &vp, &page_rect);
    // The background image's layer (#518): behind the design (PE-DESIGN's place, the
    // default), above an object, or on top.
    let layer = app
        .session
        .design()
        .background
        .as_ref()
        .map(|b| b.layer)
        .unwrap_or_default();
    if layer == BackgroundLayer::Bottom {
        draw_background(app, ui.ctx(), &painter, &vp);
    }
    crate::quality_ui::draw_background(app, &painter, &vp, &page_rect);
    crate::workspace_ui::draw_grid(&painter, &vp, &page_rect, &app.workspace.grid);
    if app.session.design().objects.is_empty() && app.session.design().background.is_none() {
        draw_empty_hint(&painter, &vp, &page_rect);
    }
    match layer {
        BackgroundLayer::AfterObject { after } if split_design(app, after) => {
            let full = std::mem::take(&mut app.render);
            if let Some((_, lower, upper)) = app.background_split.take() {
                app.render = lower;
                draw_design(app, &painter, &vp);
                draw_background(app, ui.ctx(), &painter, &vp);
                let lower = std::mem::replace(&mut app.render, upper);
                draw_design(app, &painter, &vp);
                let upper = std::mem::replace(&mut app.render, full);
                if let Some(key) = split_key(app, after) {
                    app.background_split = Some((key, lower, upper));
                }
            } else {
                app.render = full;
            }
        }
        BackgroundLayer::Top => {
            draw_design(app, &painter, &vp);
            draw_background(app, ui.ctx(), &painter, &vp);
        }
        BackgroundLayer::AfterObject { .. } => {
            // The object is gone or hidden: the image stays behind the design.
            draw_background(app, ui.ctx(), &painter, &vp);
            draw_design(app, &painter, &vp);
        }
        BackgroundLayer::Bottom => draw_design(app, &painter, &vp),
    }
    crate::view_ui::draw_show(app, &painter, &vp);
    draw_unsewn_outlines(app, &painter, &vp);
    crate::quality_ui::draw(app, &painter, &vp);
    draw_selection(app, &painter, &vp);
    crate::envelope_ui::draw(app, &painter, &vp);
    draw_preview(app, &painter, &vp);
    draw_points(app, &painter, &vp);
    crate::edit_ui::draw_arrange_preview(app, &painter, &vp);
    crate::edit_ui::draw_markers(app, &painter, &vp);
    crate::sections_ui::draw_sections(app, &painter, &vp);
    crate::workspace_ui::draw_measure(&painter, &vp, &app.workspace.measure, app.ui.prefs.units);
    crate::view_ui::draw_image_frame(app, &painter, &vp);
    crate::zoom_ui::draw(app, &painter, &vp);
    crate::view_ui::draw_guides(app, &painter, rect, &vp);
    if let (Some(_), Some(n)) = (app.simulate, app.render.needle) {
        let c = pos(vp.to_screen(n));
        painter.circle_stroke(
            c,
            6.0,
            Stroke::new(2.0, Color32::from_rgb(0xff, 0x3d, 0x00)),
        );
        painter.line_segment(
            [c - egui::vec2(10.0, 0.0), c + egui::vec2(10.0, 0.0)],
            Stroke::new(1.0, Color32::from_rgb(0xff, 0x3d, 0x00)),
        );
        painter.line_segment(
            [c - egui::vec2(0.0, 10.0), c + egui::vec2(0.0, 10.0)],
            Stroke::new(1.0, Color32::from_rgb(0xff, 0x3d, 0x00)),
        );
    }
    if let Some(h) = response.hover_pos() {
        let m = vp.to_model(v2(f64::from(h.x), f64::from(h.y)));
        let u = app.ui.prefs.units;
        let text = format!(
            "{}, {}{}   {:.0}%",
            openstitch_app::editor::units::number(u, m.x / 10.0),
            openstitch_app::editor::units::number(u, m.y / 10.0),
            openstitch_app::editor::units::suffix(u),
            vp.zoom_percent(app.points_per_mm())
        );
        painter.text(
            rect.right_bottom() + egui::vec2(-8.0, -6.0),
            egui::Align2::RIGHT_BOTTOM,
            text,
            egui::FontId::monospace(11.0),
            colors.overlay_text,
        );
    }
    if app.workspace.rulers {
        crate::workspace_ui::draw_rulers(
            &painter,
            rect,
            &vp,
            response.hover_pos(),
            app.ui.prefs.units,
        );
        crate::view_ui::draw_guide_markers(app, &painter, rect, &vp);
    }
    // Over an end of the selected line or column the drawing tool can continue (#479).
    let continues = response.hover_pos().is_some_and(|h| {
        let m = vp.to_model(v2(f64::from(h.x), f64::from(h.y)));
        let d = app.session.design();
        app.tools.continue_target(&app.tool_context(d), m).is_some()
    });
    let cursor = match app.tools.tool() {
        _ if app.ui.pan => egui::CursorIcon::Grab,
        _ if app.workspace.measure.active || app.zoom_box.active => egui::CursorIcon::Crosshair,
        _ if continues => egui::CursorIcon::PointingHand,
        openstitch_app::editor::Tool::Select | openstitch_app::editor::Tool::Point => {
            egui::CursorIcon::Default
        }
        _ => egui::CursorIcon::Crosshair,
    };
    if response.hovered() {
        ui.ctx().set_cursor_icon(cursor);
    }
}

/// Scroll pans, pinch / Ctrl+scroll zooms at the pointer, middle-drag or Space+drag pans.
fn navigate(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: egui::Rect,
    vp: &mut Viewport,
    pan_tool: bool,
) {
    if let Some(h) = response.hover_pos() {
        let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta(), i.zoom_delta()));
        if zoom != 1.0 {
            vp.zoom_about(v2(f64::from(h.x), f64::from(h.y)), f64::from(zoom));
        } else if scroll != egui::Vec2::ZERO {
            vp.pan(f64::from(scroll.x), f64::from(scroll.y));
        }
    }
    let space = pan_tool || ui.input(|i| i.key_down(egui::Key::Space));
    if response.dragged_by(egui::PointerButton::Middle)
        || (space && response.dragged_by(egui::PointerButton::Primary))
    {
        let d = response.drag_delta();
        vp.pan(f64::from(d.x), f64::from(d.y));
    }
    let _ = rect;
}

fn pointer(app: &mut EditorApp, ui: &egui::Ui, response: &egui::Response, vp: Viewport) {
    if ui.input(|i| i.key_down(egui::Key::Space)) {
        return;
    }
    let (pressed, down, released, at, shift, moved, free, alt) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.pointer.interact_pos().or(i.pointer.hover_pos()),
            i.modifiers.shift,
            // The first move after the pointer enters has no delta; the event still counts.
            i.pointer.delta() != egui::Vec2::ZERO
                || i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::PointerMoved(_))),
            i.modifiers.command,
            i.modifiers.alt,
        )
    });
    let Some(at) = at else { return };
    let mut clicks = 1;
    let phase = if pressed && response.hovered() {
        app.canvas_pointer_captured = true;
        let time = ui.input(|i| i.time);
        clicks = app.clicks.press(time, v2(f64::from(at.x), f64::from(at.y)));
        Phase::Down
    } else if app.canvas_pointer_captured && released {
        app.canvas_pointer_captured = false;
        Phase::Up
    } else if app.canvas_pointer_captured && down && moved {
        Phase::Dragged
    } else if !down && moved && response.hovered() {
        Phase::Moved
    } else {
        if response.double_clicked() {
            let effects = app.tools.finish();
            app.apply_effects(effects);
        }
        return;
    };
    let model = vp.to_model(v2(f64::from(at.x), f64::from(at.y)));
    if app.workspace.measure.active {
        let m = &mut app.workspace.measure;
        match phase {
            Phase::Down => m.press(model),
            Phase::Dragged => m.drag(model, shift),
            Phase::Up => m.release(model, shift),
            Phase::Moved => {}
        }
        return;
    }
    if app.workspace.sections.active {
        let tolerance = 6.0 * vp.units_per_point();
        let design = app.session.design().clone();
        let s = &mut app.workspace.sections;
        match phase {
            Phase::Down if s.press(&design, model, tolerance) => return,
            Phase::Dragged if s.is_dragging() => {
                s.drag(&design, model);
                return;
            }
            Phase::Up if s.is_dragging() => {
                if let Some(r) = s.release(&design, model) {
                    app.execute(&r);
                }
                return;
            }
            _ => {}
        }
    }
    let tolerance = 8.0 * vp.units_per_point();
    if crate::envelope_ui::pointer(app, phase, model, shift, tolerance) {
        return;
    }
    if matches!(phase, Phase::Down) && crate::edit_ui::pick(app, model) {
        return;
    }
    if crate::edit_ui::drag(app, phase, model) {
        return;
    }
    let mut e = PointerEvent::new(phase, model);
    e.shift = shift;
    e.clicks = clicks;
    e.free = free;
    e.alt = alt;
    app.with_tools(|t, ctx| t.handle(e, ctx));
}

/// The stitches in the chosen view, from `app.render`.
fn draw_design(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    if app.workspace.realistic {
        crate::workspace_ui::draw_realistic(app, painter, vp);
        draw_travel_left_on_fabric(app, painter, vp);
    } else if app.workspace.solid {
        crate::view_ui::draw_solid(app, painter, vp);
        draw_travel(app, painter, vp);
    } else {
        draw_stitches(app, painter, vp);
    }
}

/// The cache key of the design split around object `after`: (revision, simulator limit,
/// stitches sewn up to the end of that object).
fn split_key(app: &EditorApp, after: u64) -> Option<(u64, usize, usize)> {
    let k =
        openstitch_app::editor::background::split_stitch(&app.pattern, &app.quality.spans, after)?;
    Some((app.revision, app.simulate.unwrap_or(usize::MAX), k))
}

/// Builds (or keeps) the two halves of the design around object `after`; false when that
/// object is not sewn (deleted or hidden).
fn split_design(app: &mut EditorApp, after: u64) -> bool {
    use openstitch_app::editor::render::RenderModel;
    let Some(key) = split_key(app, after) else {
        return false;
    };
    if app
        .background_split
        .as_ref()
        .is_none_or(|(k, _, _)| *k != key)
    {
        let (_, limit, k) = key;
        app.background_split = Some((
            key,
            RenderModel::build(&app.pattern, k.min(limit)),
            RenderModel::build_range(&app.pattern, k, limit),
        ));
    }
    true
}

fn draw_page(painter: &egui::Painter, vp: &Viewport, page: &Rect, fill: Color32) {
    let r = model_rect(vp, page);
    painter.rect_filled(
        r.translate(egui::vec2(3.0, 4.0)),
        CornerRadius::ZERO,
        Color32::from_black_alpha(70),
    );
    painter.rect_filled(r, CornerRadius::ZERO, fill);
    painter.rect_stroke(
        r,
        CornerRadius::ZERO,
        Stroke::new(1.0, Color32::from_gray(150)),
        StrokeKind::Outside,
    );
}

/// An oval or round hoop (#514): its sewing field inside the Design Page, the shape the hoop-fit
/// check uses.
fn draw_hoop_field(app: &EditorApp, painter: &egui::Painter, vp: &Viewport, page: &Rect) {
    let shape = app.session.design().machine.hoop_shape();
    if shape == openstitch_design::HoopShape::Rectangle {
        return;
    }
    let pts: Vec<Pos2> = shape
        .outline(page.width(), page.height(), 96)
        .into_iter()
        .map(|(x, y)| pos(vp.to_screen(v2(x, y))))
        .collect();
    painter.add(Shape::closed_line(
        pts,
        Stroke::new(1.5, Color32::from_rgb(0x43, 0x8a, 0x5e)),
    ));
}

/// The background reference image, decoded once per path (size limits as for the wizard).
/// Realistic view: woven cloth on the Design Page, one repeat per `art::FABRIC_TILE`.
fn draw_fabric(
    ctx: &egui::Context,
    painter: &egui::Painter,
    vp: &Viewport,
    page: &Rect,
    tint: Color32,
) {
    let Some(t) = crate::art::fabric(ctx) else {
        return;
    };
    let uv = egui::Rect::from_min_max(
        Pos2::ZERO,
        Pos2::new(
            (page.width() / crate::art::FABRIC_TILE) as f32,
            (page.height() / crate::art::FABRIC_TILE) as f32,
        ),
    );
    painter.image(t.id(), model_rect(vp, page), uv, tint);
}

/// A new, empty design: a faded hoop and where to start, in the middle of the Design Page.
fn draw_empty_hint(painter: &egui::Painter, vp: &Viewport, page: &Rect) {
    let r = model_rect(vp, page);
    let side = (r.width().min(r.height()) * 0.28).clamp(48.0, 150.0);
    let art = egui::Rect::from_center_size(
        r.center() - egui::vec2(0.0, side * 0.2),
        egui::vec2(side, side),
    );
    crate::art::paint(
        painter,
        art,
        crate::art::Art::EmptyCanvas,
        crate::art::OUTLINE,
        0.45,
    );
    if r.width() > 260.0 {
        let grey = Color32::from_gray(120);
        let font = egui::FontId::proportional(14.0);
        let y = art.bottom() + 12.0;
        painter.text(
            egui::pos2(r.center().x, y),
            egui::Align2::CENTER_TOP,
            "An empty design",
            egui::FontId::proportional(16.0),
            Color32::from_gray(90),
        );
        painter.text(
            egui::pos2(r.center().x, y + 24.0),
            egui::Align2::CENTER_TOP,
            "Draw with the Shapes or Text tools, or open a design from File.",
            font,
            grey,
        );
    }
}

fn draw_background(
    app: &mut EditorApp,
    ctx: &egui::Context,
    painter: &egui::Painter,
    vp: &Viewport,
) {
    let Some(b) = app.session.design().background.clone() else {
        return;
    };
    if !b.visible || b.opacity <= 0.0 {
        return;
    }
    if app
        .background_texture
        .as_ref()
        .is_none_or(|(p, a, _)| *p != b.path || *a != b.adjust)
    {
        let texture = crate::view_ui::shown_image(app).ok().map(|r| {
            let size = [r.width() as usize, r.height() as usize];
            let mut rgba = Vec::with_capacity(size[0] * size[1] * 4);
            for y in 0..r.height() {
                for x in 0..r.width() {
                    rgba.extend_from_slice(&r.pixel(x, y));
                }
            }
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
            ctx.load_texture("background", image, egui::TextureOptions::LINEAR)
        });
        app.background_texture = Some((b.path.clone(), b.adjust, texture));
    }
    if let Some((_, _, Some(texture))) = &app.background_texture {
        let r = model_rect(
            vp,
            &Rect {
                min_x: b.x,
                min_y: b.y,
                max_x: b.x + b.width,
                max_y: b.y + b.height,
            },
        );
        let uv = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        let tint = Color32::from_white_alpha((b.opacity.clamp(0.0, 1.0) * 255.0) as u8);
        painter.image(texture.id(), r, uv, tint);
    }
}

fn draw_stitches(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    // Thinner than real thread (~0.4 mm) so rows stay distinguishable.
    let width = (2.5 * vp.scale).clamp(0.5, 3.0) as f32;
    for layer in &app.render.layers {
        let [r, g, b] = layer.rgb;
        let stroke = Stroke::new(width, Color32::from_rgb(r, g, b));
        for s in &layer.strokes {
            let pts: Vec<Pos2> = s.iter().map(|&p| pos(vp.to_screen(p))).collect();
            painter.add(Shape::line(pts, stroke));
        }
    }
    draw_travel(app, painter, vp);
}

fn draw_travel(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    if app.show_travel {
        let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 90, 90, 110));
        for &(a, b) in &app.render.travel {
            painter.add(Shape::dashed_line(
                &[pos(vp.to_screen(a)), pos(vp.to_screen(b))],
                stroke,
                4.0,
                3.0,
            ));
        }
    }
}

/// The Realistic view's travel: only jumps whose thread stays on the fabric (no trim or colour
/// change between the stitches they join); a trimmed jump leaves no thread on the sewn piece.
fn draw_travel_left_on_fabric(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    if app.show_travel {
        let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 90, 90, 110));
        let cut = &app.render.travel_cut;
        for (i, &(a, b)) in app.render.travel.iter().enumerate() {
            if !cut.get(i).copied().unwrap_or(false) {
                painter.add(Shape::dashed_line(
                    &[pos(vp.to_screen(a)), pos(vp.to_screen(b))],
                    stroke,
                    4.0,
                    3.0,
                ));
            }
        }
    }
}

/// Shapes without any sewing whose outlines the canvas shows (they would be invisible): all of
/// them, except in the Realistic view, which shows only what is sewn (a text path's guide circle
/// read as a sewn grey ring in the kit's preview, #441), so there only the selected ones.
pub(crate) fn unsewn_outlines_shown(app: &EditorApp) -> Vec<u64> {
    app.session
        .design()
        .objects
        .iter()
        .filter(|o| {
            o.kind == ObjectKind::Shape && o.visible && o.region.is_none() && o.line.is_none()
        })
        .filter(|o| !app.workspace.realistic || app.selection.contains(&o.id))
        .map(|o| o.id)
        .collect()
}

fn draw_unsewn_outlines(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let shown = unsewn_outlines_shown(app);
    for o in &app.session.design().objects {
        if shown.contains(&o.id) {
            for c in &o.contours {
                draw_contour(
                    painter,
                    vp,
                    c,
                    Stroke::new(1.0, Color32::from_gray(120)),
                    None,
                );
            }
        }
    }
}

fn draw_contour(
    painter: &egui::Painter,
    vp: &Viewport,
    c: &Contour,
    stroke: Stroke,
    t: Option<&Transform>,
) {
    let pts: Vec<Pos2> = c
        .path()
        .iter()
        .map(|&p| pos(vp.to_screen(t.map_or(p, |t| t.apply(p)))))
        .collect();
    painter.add(Shape::line(pts, stroke));
}

fn draw_selection(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    if app.selection.is_empty() || matches!(app.tools.preview(), Some(ToolPreview::Transform(_))) {
        return;
    }
    let design = app.session.design();
    for o in design
        .objects
        .iter()
        .filter(|o| app.selection.contains(&o.id))
    {
        for c in &o.contours {
            draw_contour(
                painter,
                vp,
                c,
                Stroke::new(1.0, crate::theme::accent().gamma_multiply(0.7)),
                None,
            );
        }
    }
    // The Point and Satin Skeleton tools show their own handles instead.
    if matches!(
        app.tools.tool(),
        openstitch_app::editor::Tool::Point | openstitch_app::editor::Tool::Skeleton
    ) {
        return;
    }
    let Some(bounds) = bounds_of(design, &app.selection) else {
        return;
    };
    let h = SelectionHandles {
        bounds,
        units_per_point: vp.units_per_point(),
    };
    draw_box_handles(painter, vp, &h);
}

/// A dashed box with the scale handles around it and the rotate handle above.
fn draw_box_handles(painter: &egui::Painter, vp: &Viewport, h: &SelectionHandles) {
    let r = model_rect(vp, &h.bounds);
    painter.add(Shape::dashed_line(
        &[
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ],
        Stroke::new(1.0, crate::theme::accent()),
        5.0,
        3.0,
    ));
    let top = pos(vp.to_screen(h.position(Handle::Top)));
    let rot = pos(vp.to_screen(h.position(Handle::Rotate)));
    painter.line_segment([top, rot], Stroke::new(1.0, crate::theme::accent()));
    painter.circle(
        rot,
        SelectionHandles::RADIUS as f32,
        Color32::WHITE,
        Stroke::new(1.5, crate::theme::accent()),
    );
    for handle in Handle::SCALE {
        let c = pos(vp.to_screen(h.position(handle)));
        let s = SelectionHandles::RADIUS as f32 * 0.8;
        painter.rect(
            egui::Rect::from_center_size(c, egui::vec2(2.0 * s, 2.0 * s)),
            CornerRadius::same(1),
            Color32::WHITE,
            Stroke::new(1.5, crate::theme::accent()),
            StrokeKind::Middle,
        );
    }
}

/// Point tool: the target's outline, curve handles and vertices (the selected one filled);
/// Pen tool: the handles of the anchor being placed.
fn draw_points(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let handle_stroke = Stroke::new(1.0, crate::theme::accent().gamma_multiply(0.8));
    let handle = |at: Pos2| {
        painter.circle(
            at,
            3.5,
            Color32::WHITE,
            Stroke::new(1.5, crate::theme::accent()),
        );
    };
    for (a, h) in app.tools.pen_handles() {
        let (a, h) = (pos(vp.to_screen(a)), pos(vp.to_screen(h)));
        painter.line_segment([a, h], handle_stroke);
        handle(h);
    }
    let design = app.session.design().clone();
    let ctx = app.tool_context(&design);
    if let Some(sk) = app.tools.skeleton_overlay(&ctx) {
        // Satin Skeleton: the column, its centre line and a width handle on either side.
        if let Some(c) = &sk.column {
            draw_contour(
                painter,
                vp,
                c,
                Stroke::new(1.0, crate::theme::accent().gamma_multiply(0.6)),
                None,
            );
        }
        draw_contour(
            painter,
            vp,
            &sk.centre,
            Stroke::new(1.5, crate::theme::accent()),
            None,
        );
        for (at, left, right) in sk.widths {
            let (a, l, r) = (
                pos(vp.to_screen(at)),
                pos(vp.to_screen(left)),
                pos(vp.to_screen(right)),
            );
            painter.line_segment([l, r], handle_stroke);
            handle(l);
            handle(r);
            painter.circle_filled(a, 3.0, crate::theme::accent());
        }
    }
    if let Some(ch) = app.tools.character_overlay(&ctx) {
        // Point Edit on a text: the selected character, its box and handles.
        for c in &ch.outlines {
            draw_contour(
                painter,
                vp,
                c,
                Stroke::new(1.5, crate::theme::accent()),
                None,
            );
        }
        draw_box_handles(painter, vp, &ch.handles);
    }
    if let Some(needles) = app.tools.needle_overlay(&ctx) {
        // Stitch pattern: needle points as small squares (jump ends as circles), selected
        // ones filled.
        for (_, p, selected, jump) in needles.points {
            let at = pos(vp.to_screen(p));
            let fill = if selected {
                crate::theme::accent()
            } else {
                Color32::WHITE
            };
            if jump {
                painter.circle(at, 3.0, fill, Stroke::new(1.0, crate::theme::accent()));
            } else {
                painter.rect(
                    egui::Rect::from_center_size(at, egui::vec2(5.0, 5.0)),
                    CornerRadius::ZERO,
                    fill,
                    Stroke::new(1.0, crate::theme::accent()),
                    StrokeKind::Middle,
                );
            }
        }
        return;
    }
    let Some(overlay) = app.tools.point_overlay(&ctx) else {
        return;
    };
    for (ci, c) in overlay.contours.iter().enumerate() {
        draw_contour(
            painter,
            vp,
            c,
            Stroke::new(1.5, crate::theme::accent()),
            None,
        );
        for h in handle_lines(c) {
            let (a, at) = (pos(vp.to_screen(h.anchor)), pos(vp.to_screen(h.at)));
            painter.line_segment([a, at], handle_stroke);
            handle(at);
        }
        for (i, &p) in c.points.iter().enumerate() {
            let selected = overlay.selected == Some((ci, i));
            let s = SelectionHandles::RADIUS as f32 * 0.8;
            painter.rect(
                egui::Rect::from_center_size(pos(vp.to_screen(p)), egui::vec2(2.0 * s, 2.0 * s)),
                CornerRadius::ZERO,
                if selected {
                    crate::theme::accent()
                } else {
                    Color32::WHITE
                },
                Stroke::new(1.5, crate::theme::accent()),
                StrokeKind::Middle,
            );
        }
    }
}

/// Snap to Objects: dashed lines through what the moving selection lined up with.
fn draw_snap_guides(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    use openstitch_app::editor::snap::SnapGuide;
    let page = &app.session.design().page;
    for g in app.tools.guides() {
        let (a, b) = match *g {
            SnapGuide::Vertical(x) => (v2(x, 0.0), v2(x, page.height)),
            SnapGuide::Horizontal(y) => (v2(0.0, y), v2(page.width, y)),
        };
        let shapes = egui::Shape::dashed_line(
            &[pos(vp.to_screen(a)), pos(vp.to_screen(b))],
            Stroke::new(1.0, Color32::from_rgb(0xd8, 0x43, 0x3a)),
            6.0,
            4.0,
        );
        painter.extend(shapes);
    }
}

fn draw_preview(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    draw_snap_guides(app, painter, vp);
    match app.tools.preview() {
        Some(ToolPreview::Marquee(r)) => {
            let r = model_rect(vp, r);
            painter.rect(
                r,
                CornerRadius::ZERO,
                crate::theme::accent().gamma_multiply(0.12),
                Stroke::new(1.0, crate::theme::accent()),
                StrokeKind::Middle,
            );
        }
        Some(ToolPreview::Outline(c)) => {
            draw_contour(
                painter,
                vp,
                c,
                Stroke::new(1.5, crate::theme::accent()),
                None,
            );
            for &p in &c.points {
                painter.circle_filled(pos(vp.to_screen(p)), 3.0, crate::theme::accent());
            }
        }
        Some(ToolPreview::Transform(t)) => {
            let design = app.session.design();
            for o in design
                .objects
                .iter()
                .filter(|o| app.selection.contains(&o.id))
            {
                match o.kind {
                    ObjectKind::Shape | ObjectKind::Text => {
                        for c in &o.contours {
                            draw_contour(
                                painter,
                                vp,
                                c,
                                Stroke::new(1.5, crate::theme::accent()),
                                Some(t),
                            );
                        }
                    }
                    ObjectKind::Stitches => {
                        if let Some(b) = o.bounds() {
                            let c = Contour::rectangle(b.min_x, b.min_y, b.width(), b.height());
                            draw_contour(
                                painter,
                                vp,
                                &c,
                                Stroke::new(1.5, crate::theme::accent()),
                                Some(t),
                            );
                        }
                    }
                }
            }
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas() -> ScreenRect {
        ScreenRect {
            x: 300.0,
            y: 160.0,
            width: 900.0,
            height: 600.0,
        }
    }

    #[test]
    fn page_is_fitted_left_of_a_dialog_reaching_in_from_the_right() {
        let dialog = egui::Rect::from_min_max(egui::pos2(1000.0, 170.0), egui::pos2(1440.0, 700.0));
        let free = beside_dialog(canvas(), Some(dialog));
        assert_eq!(free.x, 300.0);
        assert_eq!(free.width, 700.0);
        assert_eq!(free.height, 600.0);
    }

    #[test]
    fn dialogs_outside_or_over_most_of_the_canvas_leave_it_whole() {
        assert_eq!(beside_dialog(canvas(), None), canvas());
        let outside =
            egui::Rect::from_min_max(egui::pos2(1250.0, 170.0), egui::pos2(1440.0, 700.0));
        assert_eq!(beside_dialog(canvas(), Some(outside)), canvas());
        let wide = egui::Rect::from_min_max(egui::pos2(500.0, 170.0), egui::pos2(1440.0, 700.0));
        assert_eq!(beside_dialog(canvas(), Some(wide)), canvas());
    }
}
