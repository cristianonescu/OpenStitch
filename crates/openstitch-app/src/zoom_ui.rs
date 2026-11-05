//! Zoom Box, display-calibrated real size and the background image's canvas handles (#518,
//! beyond PE-DESIGN). The logic without egui is in `openstitch_app::editor::viewport` and
//! `openstitch_app::editor::background`.

use crate::app::EditorApp;
use eframe::egui::{self, Color32, Pos2, Stroke};
use openstitch_app::editor::background::{self as bg, Grab};
use openstitch_app::editor::viewport::{self, ScreenRect, Viewport};
use openstitch_design::{Rect, v2};

const BOX: Color32 = Color32::from_rgb(0x1e, 0x88, 0xe5);

/// View ▸ Zoom ▸ Zoom Box (Z): drag a rectangle to zoom to it; Esc cancels.
#[derive(Clone, Copy, Debug, Default)]
pub struct ZoomBox {
    pub active: bool,
    from: Option<Pos2>,
    to: Option<Pos2>,
}

impl ZoomBox {
    pub fn toggle(&mut self) {
        *self = ZoomBox {
            active: !self.active,
            ..ZoomBox::default()
        };
    }
}

/// Options ▸ Calibrate Real Size: a line the user stretches to 100 mm on a ruler.
#[derive(Clone, Copy, Debug)]
pub struct Calibration {
    pub open: bool,
    /// The line's length, points.
    length: f32,
}

impl Default for Calibration {
    fn default() -> Self {
        Calibration {
            open: false,
            length: (100.0 * viewport::DEFAULT_POINTS_PER_MM) as f32,
        }
    }
}

/// A background-image handle drag in progress: what was grabbed and where (model).
#[derive(Clone, Copy, Debug)]
pub struct ImageDrag {
    grab: Grab,
    from: openstitch_design::Vec2,
    to: openstitch_design::Vec2,
}

fn screen(rect: egui::Rect) -> ScreenRect {
    crate::workspace_ui::screen_rect(rect)
}

/// The monitor the window is on, by its size in points (calibrations are kept per monitor).
pub fn monitor(ctx: &egui::Context) -> Option<String> {
    ctx.input(|i| i.viewport().monitor_size)
        .map(|s| format!("{:.0}x{:.0}", s.x, s.y))
}

impl EditorApp {
    /// The key real-size calibrations are kept under: the monitor, or "screen" when the
    /// platform does not say.
    pub fn monitor_key(&self) -> String {
        self.monitor.clone().unwrap_or_else(|| "screen".into())
    }

    /// Points per mm at 100 %: this monitor's calibration, else a 96 dpi screen.
    pub fn points_per_mm(&self) -> f64 {
        self.ui.prefs.points_per_mm(Some(&self.monitor_key()))
    }
}

/// Canvas pointer handling for the Zoom Box and the image handles; true when it took the
/// pointer this frame.
pub fn pointer(
    app: &mut EditorApp,
    response: &egui::Response,
    rect: egui::Rect,
    vp: Viewport,
) -> bool {
    if app.zoom_box.active {
        if response.drag_started() {
            app.zoom_box.from = response.interact_pointer_pos();
        }
        if response.dragged() {
            app.zoom_box.to = response.interact_pointer_pos();
        }
        if response.drag_stopped()
            && let (Some(a), Some(b)) = (app.zoom_box.from, app.zoom_box.to)
        {
            let (ma, mb) = (
                vp.to_model(v2(f64::from(a.x), f64::from(a.y))),
                vp.to_model(v2(f64::from(b.x), f64::from(b.y))),
            );
            let r = Rect {
                min_x: ma.x.min(mb.x),
                min_y: ma.y.min(mb.y),
                max_x: ma.x.max(mb.x),
                max_y: ma.y.max(mb.y),
            };
            if let Some(z) = Viewport::zoom_box(r, screen(rect)) {
                app.viewport = Some(z);
            }
            app.zoom_box = ZoomBox::default();
        }
        return true;
    }
    // Image handles while Modify Image is open.
    let Some(b) = app.session.design().background.clone() else {
        return false;
    };
    if !app.workspace.view.image_open || !b.visible {
        return false;
    }
    let model = |p: Pos2| vp.to_model(v2(f64::from(p.x), f64::from(p.y)));
    if response.drag_started()
        && let Some(p) = response.interact_pointer_pos()
        && let Some(grab) = bg::grab(&b, model(p), 6.0 / vp.scale)
    {
        let at = model(p);
        app.image_drag = Some(ImageDrag {
            grab,
            from: at,
            to: at,
        });
    }
    let Some(mut d) = app.image_drag else {
        return false;
    };
    if let Some(p) = response.interact_pointer_pos() {
        d.to = model(p);
        app.image_drag = Some(d);
    }
    if response.drag_stopped() {
        app.image_drag = None;
        if let Some(r) = bg::modify_request(&b, d.grab, d.to - d.from) {
            app.execute(&r);
        }
    }
    true
}

/// The zoom box being dragged, the image handles and the image's outline while dragged.
pub fn draw(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    if let (true, Some(a), Some(b)) = (app.zoom_box.active, app.zoom_box.from, app.zoom_box.to) {
        let r = egui::Rect::from_two_pos(a, b);
        painter.rect_filled(r, 0.0, BOX.gamma_multiply(0.1));
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, BOX), egui::StrokeKind::Middle);
    }
    let Some(b) = &app.session.design().background else {
        return;
    };
    if !app.workspace.view.image_open || !b.visible {
        return;
    }
    let (x, y, w, h) = match app.image_drag {
        Some(d) => bg::dragged(b, d.grab, d.to - d.from),
        None => (b.x, b.y, b.width, b.height),
    };
    let p = |x: f64, y: f64| {
        let s = vp.to_screen(v2(x, y));
        Pos2::new(s.x as f32, s.y as f32)
    };
    let r = egui::Rect::from_two_pos(p(x, y), p(x + w, y + h));
    if app.image_drag.is_some() {
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, BOX), egui::StrokeKind::Middle);
    }
    for c in [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
    ] {
        painter.rect_filled(
            egui::Rect::from_center_size(c, egui::vec2(8.0, 8.0)),
            0.0,
            Color32::WHITE,
        );
        painter.rect_stroke(
            egui::Rect::from_center_size(c, egui::vec2(8.0, 8.0)),
            0.0,
            Stroke::new(1.0, BOX),
            egui::StrokeKind::Middle,
        );
    }
}

/// Options ▸ Calibrate Real Size: stretch the line to 100 mm on a ruler held to the screen,
/// then Save; 100 % then shows real size on this monitor.
pub fn calibration_window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.calibration.open;
    let key = Some(app.monitor_key());
    egui::Window::new("Calibrate Real Size")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(
                "Hold a ruler to the screen and drag the end of the line until it measures 100 mm.",
            );
            let len = &mut app.calibration.length;
            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(len.max(40.0) + 24.0, 36.0), egui::Sense::drag());
            if resp.dragged() {
                *len = (*len + resp.drag_delta().x).clamp(40.0, 2400.0);
            }
            let y = rect.center().y;
            let (a, b) = (
                Pos2::new(rect.left() + 4.0, y),
                Pos2::new(rect.left() + 4.0 + *len, y),
            );
            let ink = ui.visuals().strong_text_color();
            ui.painter().line_segment([a, b], Stroke::new(2.0, ink));
            for x in [a, b] {
                ui.painter().line_segment(
                    [x - egui::vec2(0.0, 10.0), x + egui::vec2(0.0, 10.0)],
                    Stroke::new(2.0, ink),
                );
            }
            ui.painter().circle_filled(b, 6.0, BOX);
            ui.add(
                egui::DragValue::new(len)
                    .range(40.0..=2400.0)
                    .speed(0.25)
                    .suffix(" pt"),
            );
            let monitor = key.as_deref().unwrap_or("this screen");
            let now = app.ui.prefs.points_per_mm(key.as_deref());
            ui.weak(format!("{monitor}: 100 % is {:.2} points per mm", now));
            ui.horizontal(|ui| {
                let k = viewport::calibrated(f64::from(app.calibration.length), 100.0);
                if ui
                    .add_enabled(k.is_some() && key.is_some(), egui::Button::new("Save"))
                    .clicked()
                {
                    app.ui.prefs.calibrate(monitor, k);
                    app.ui.save_prefs();
                    app.status = format!("Real size calibrated for {monitor}");
                }
                if ui
                    .button("Use 96 dpi")
                    .on_hover_text("Forget this monitor's calibration")
                    .clicked()
                {
                    app.ui.prefs.calibrate(monitor, None);
                    app.ui.save_prefs();
                }
            });
        });
    app.calibration.open = open;
}
