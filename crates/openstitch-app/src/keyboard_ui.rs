//! Keyboard-only drawing (#499) and widths while drawing (#707), beyond PE-DESIGN.
//!
//! With a drawing tool active, an arrow key shows a cross-hair keyboard cursor on the canvas
//! and moves it: 0.1 mm a press (Options ▸ nudge step), Shift ten times that, one grid step
//! while snapping with Nudge by Grid. Space clicks there (a corner point, a polygon vertex, a
//! Manual Punch line end, a Knife point); Shift+Space places a smooth Pen or Satin Skeleton
//! point (a 3 mm handle along the line); for the Rectangle and Ellipse tools the first Space
//! starts the drag and the second ends it. Return finishes, Esc cancels, Backspace (or Delete) removes
//! the last point. The keys reach the tools as the same pointer events a mouse makes, so each
//! drawing ends in the same command.
//!
//! While the Pen or Satin Skeleton draws, `[` and `]` (and the scroll wheel; Ctrl+scroll still
//! zooms) narrow and widen the next point by 0.5 mm, or the point under the pointer. A Pen
//! line drawn with widths becomes a satin line with a width key at each point.
//!
//! The Assistant drives the same steps with `ui.cursor`.

use crate::app::EditorApp;
use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, Stroke};
use openstitch_app::editor::tools::keyboard::WIDTH_STEP;
use openstitch_app::editor::tools::{Phase, PointerEvent};
use openstitch_app::editor::{Tool, Viewport};
use openstitch_design::{Vec2, v2};
use serde_json::{Value, json};

/// Length of a smooth point's handle (model units, 3 mm).
const SMOOTH_HANDLE: f64 = 30.0;

/// Keyboard drawing state of the window.
#[derive(Clone, Debug, Default)]
pub struct KeyboardCursor {
    /// Where Space clicks (model units); `None` until an arrow key is pressed.
    pub at: Option<Vec2>,
    /// Rectangle / Ellipse: the first Space started a drag.
    dragging: bool,
}

/// Tools the keyboard cursor draws with.
pub fn draws(t: Tool) -> bool {
    !matches!(t, Tool::Select | Tool::Point | Tool::ShapeBuilder)
}

impl EditorApp {
    /// Where the keyboard cursor starts: the last point placed, else the middle of the view,
    /// else the middle of the Design Page.
    fn cursor_start(&self) -> Vec2 {
        let t = &self.tools;
        if let Some(p) = t
            .pen()
            .last()
            .map(|n| n.at)
            .or(t.polygon().last().copied())
            .or(t.punch().last().copied())
        {
            return p;
        }
        if let (Some(rect), Some(vp)) = (self.workspace.canvas, self.viewport) {
            let c = rect.center();
            return vp.to_model(v2(f64::from(c.x), f64::from(c.y)));
        }
        self.session.design().page.center()
    }

    /// Shows the cursor (where it starts) or moves it by (`dx`, `dy`) model units.
    pub fn cursor_move(&mut self, dx: f64, dy: f64) {
        let at = self.keyboard.at.unwrap_or_else(|| self.cursor_start()) + v2(dx, dy);
        let at = self.tools.snapped(at);
        self.keyboard.at = Some(at);
        // The tools' preview follows, as it follows the mouse.
        let phase = if self.keyboard.dragging {
            Phase::Dragged
        } else {
            Phase::Moved
        };
        self.cursor_event(phase, at, false);
    }

    fn cursor_event(&mut self, phase: Phase, at: Vec2, shift: bool) {
        let mut e = PointerEvent::new(phase, at);
        e.shift = shift;
        self.with_tools(|t, c| t.handle(e, c));
    }

    /// Space: a click at the cursor (a drag start or end for Rectangle and Ellipse).
    pub fn cursor_place(&mut self) {
        let at = self.keyboard.at.unwrap_or_else(|| self.cursor_start());
        self.keyboard.at = Some(at);
        if matches!(self.tools.tool(), Tool::Rectangle | Tool::Ellipse) {
            if self.keyboard.dragging {
                self.cursor_event(Phase::Dragged, at, false);
                self.cursor_event(Phase::Up, at, false);
            } else {
                self.cursor_event(Phase::Down, at, false);
            }
            self.keyboard.dragging = !self.keyboard.dragging;
            return;
        }
        self.cursor_event(Phase::Down, at, false);
        self.cursor_event(Phase::Up, at, false);
    }

    /// Shift+Space: a smooth Pen or Satin Skeleton point, its handle 3 mm along the line from
    /// the previous point (a corner point for other tools).
    pub fn cursor_smooth(&mut self) {
        if !matches!(self.tools.tool(), Tool::Pen | Tool::Skeleton) {
            self.cursor_place();
            return;
        }
        let at = self.keyboard.at.unwrap_or_else(|| self.cursor_start());
        self.keyboard.at = Some(at);
        let dir = self
            .tools
            .pen()
            .last()
            .map(|n| at - n.at)
            .filter(|d| d.length() > 1e-9)
            .map_or(v2(1.0, 0.0), |d| d * (1.0 / d.length()));
        self.cursor_event(Phase::Down, at, false);
        self.cursor_event(Phase::Dragged, at + dir * SMOOTH_HANDLE, false);
        self.cursor_event(Phase::Up, at + dir * SMOOTH_HANDLE, false);
    }

    /// Return: finish the drawing.
    pub fn cursor_finish(&mut self) {
        let effects = self.tools.finish();
        self.apply_effects(effects);
        self.keyboard.dragging = false;
    }

    /// Esc: cancel the drawing and hide the cursor.
    pub fn cursor_cancel(&mut self) {
        self.tools.cancel();
        self.keyboard = KeyboardCursor::default();
    }

    /// `[` / `]` / scroll: the next point's width (Pen, Satin Skeleton), or the point under
    /// `at`'s. The status bar shows it.
    pub fn width_step(&mut self, delta: f64, at: Option<Vec2>) -> Option<f64> {
        let radius = 8.0 * self.viewport.unwrap_or_default().units_per_point();
        let w = self.tools.step_width(delta, at, radius)?;
        self.status = format!("Satin width {w:.1} mm");
        Some(w)
    }

    /// `ui.cursor`: the keyboard drawing steps for the Assistant.
    pub(crate) fn cursor_command(&mut self, params: Option<&Value>) -> Result<Value, String> {
        let num = |k: &str| params.and_then(|v| v.get(k)).and_then(Value::as_f64);
        if !draws(self.tools.tool()) {
            return Err(format!(
                "the {} tool does not draw: pick a drawing tool with ui.run first",
                self.tools.tool().label()
            ));
        }
        if let (Some(x), Some(y)) = (num("x"), num("y")) {
            if !(x.is_finite() && y.is_finite()) || x.abs() > 10_000.0 || y.abs() > 10_000.0 {
                return Err("x and y are mm on the Design Page, within ±10000".into());
            }
            let now = self.keyboard.at.unwrap_or_else(|| self.cursor_start());
            self.cursor_move(x * 10.0 - now.x, y * 10.0 - now.y);
        } else if num("dx").is_some() || num("dy").is_some() {
            let (dx, dy) = (num("dx").unwrap_or(0.0), num("dy").unwrap_or(0.0));
            if !(dx.is_finite() && dy.is_finite()) || dx.abs() > 10_000.0 || dy.abs() > 10_000.0 {
                return Err("dx and dy are mm, within ±10000".into());
            }
            self.cursor_move(dx * 10.0, dy * 10.0);
        }
        if let Some(w) = num("width") {
            self.tools
                .set_next_width(w)
                .ok_or("width applies to the Pen and Satin Skeleton tools")?;
        }
        match params.and_then(|v| v.get("action")).and_then(Value::as_str) {
            None => {}
            Some("place") => self.cursor_place(),
            Some("smooth") => self.cursor_smooth(),
            Some("back") => {
                self.tools.remove_last_point();
            }
            Some("finish") => self.cursor_finish(),
            Some("cancel") => self.cursor_cancel(),
            Some(other) => {
                return Err(format!(
                    "action takes place, smooth, back, finish or cancel, not '{other}'"
                ));
            }
        }
        let at = self.keyboard.at;
        Ok(json!({
            "tool": self.tools.tool().label(),
            "cursor": at.map(|p| json!([p.x / 10.0, p.y / 10.0])),
            "drawing": self.tools.is_busy(),
            "nextWidth": self.tools.next_width(),
            "selection": self.selection,
        }))
    }
}

/// Keyboard drawing keys for this frame; `true` when the arrow keys moved the cursor (so they
/// do not nudge the selection too).
pub fn keys(app: &mut EditorApp, ctx: &egui::Context) -> bool {
    let tool = app.tools.tool();
    if !draws(tool) {
        app.keyboard = KeyboardCursor::default();
        return false;
    }
    let take = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
    let e = app.ui.prefs.editing;
    let base = match app.workspace.grid.snap_step() {
        Some(grid) if e.nudge_grid => grid,
        _ => e.nudge_mm * 10.0,
    };
    let mut moved = false;
    for (key, dx, dy) in [
        (Key::ArrowLeft, -1.0, 0.0),
        (Key::ArrowRight, 1.0, 0.0),
        (Key::ArrowUp, 0.0, -1.0),
        (Key::ArrowDown, 0.0, 1.0),
    ] {
        for (mods, step) in [(Modifiers::SHIFT, base * 10.0), (Modifiers::NONE, base)] {
            if take(KeyboardShortcut::new(mods, key)) {
                app.cursor_move(dx * step, dy * step);
                moved = true;
            }
        }
    }
    if app.keyboard.at.is_some() {
        if take(KeyboardShortcut::new(Modifiers::SHIFT, Key::Space)) {
            app.cursor_smooth();
        } else if take(KeyboardShortcut::new(Modifiers::NONE, Key::Space)) {
            app.cursor_place();
        }
    }
    // `[` and `]` arrive as typed text.
    if matches!(tool, Tool::Pen | Tool::Skeleton) {
        let hover = ctx
            .pointer_hover_pos()
            .zip(app.viewport)
            .map(|(p, vp)| vp.to_model(v2(f64::from(p.x), f64::from(p.y))));
        let typed: Vec<char> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) if t == "[" || t == "]" => t.chars().next(),
                    _ => None,
                })
                .collect()
        });
        for c in typed {
            let delta = if c == '[' { -WIDTH_STEP } else { WIDTH_STEP };
            app.width_step(delta, hover);
        }
    }
    moved
}

/// The scroll wheel over the canvas while the Pen or Satin Skeleton draws: a width step per
/// notch. `true` when it took the scroll (the canvas then does not pan).
pub fn scroll_width(app: &mut EditorApp, scroll_y: f32, at: Vec2) -> bool {
    if !matches!(app.tools.tool(), Tool::Pen | Tool::Skeleton)
        || !app.tools.is_busy()
        || scroll_y == 0.0
    {
        return false;
    }
    let delta = if scroll_y > 0.0 {
        WIDTH_STEP
    } else {
        -WIDTH_STEP
    };
    app.width_step(delta, Some(at));
    true
}

/// The cross-hair at the keyboard cursor.
pub fn draw(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let Some(at) = app.keyboard.at else { return };
    if !draws(app.tools.tool()) {
        return;
    }
    let s = vp.to_screen(at);
    let c = egui::pos2(s.x as f32, s.y as f32);
    let colour = crate::theme::accent();
    let halo = Stroke::new(3.0, Color32::from_white_alpha(200));
    let line = Stroke::new(1.5, colour);
    for stroke in [halo, line] {
        painter.line_segment(
            [c - egui::vec2(10.0, 0.0), c + egui::vec2(10.0, 0.0)],
            stroke,
        );
        painter.line_segment(
            [c - egui::vec2(0.0, 10.0), c + egui::vec2(0.0, 10.0)],
            stroke,
        );
        painter.circle_stroke(c, 4.0, stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;
    use openstitch_app::editor::CommandRequest;

    fn app_with(tool: Tool) -> EditorApp {
        let mut app = EditorApp::new(Launch::default());
        app.tools.set_tool(tool);
        app
    }

    fn at(app: &mut EditorApp, x: f64, y: f64) {
        app.cursor_command(Some(&json!({ "x": x, "y": y })))
            .expect("move");
    }

    fn place(app: &mut EditorApp) {
        app.cursor_command(Some(&json!({ "action": "place" })))
            .expect("place");
    }

    /// The command a mouse drawing of the same points makes.
    fn mouse(tool: Tool, points: &[(f64, f64)], drag: bool) -> EditorApp {
        let mut app = app_with(tool);
        if drag {
            let (a, b) = (points[0], points[1]);
            app.with_tools(|t, c| t.handle(PointerEvent::new(Phase::Down, v2(a.0, a.1)), c));
            app.with_tools(|t, c| t.handle(PointerEvent::new(Phase::Dragged, v2(b.0, b.1)), c));
            app.with_tools(|t, c| t.handle(PointerEvent::new(Phase::Up, v2(b.0, b.1)), c));
        } else {
            for &(x, y) in points {
                app.with_tools(|t, c| t.handle(PointerEvent::new(Phase::Down, v2(x, y)), c));
                app.with_tools(|t, c| t.handle(PointerEvent::new(Phase::Up, v2(x, y)), c));
            }
            let effects = app.tools.finish();
            app.apply_effects(effects);
        }
        app
    }

    fn objects(app: &EditorApp) -> Vec<openstitch_design::DesignObject> {
        app.session.design().objects.clone()
    }

    #[test]
    fn a_polygon_from_the_keyboard_is_the_mouse_polygon() {
        let mut app = app_with(Tool::Polygon);
        for (x, y) in [(10.0, 10.0), (40.0, 10.0), (40.0, 30.0), (10.0, 30.0)] {
            at(&mut app, x, y);
            place(&mut app);
        }
        app.cursor_command(Some(&json!({ "action": "finish" })))
            .expect("finish");
        let m = mouse(
            Tool::Polygon,
            &[
                (100.0, 100.0),
                (400.0, 100.0),
                (400.0, 300.0),
                (100.0, 300.0),
            ],
            false,
        );
        assert_eq!(objects(&app).len(), 1);
        assert_eq!(objects(&app), objects(&m), "the same shape.polygon");
        assert_eq!(app.undo_groups.len(), 1, "one undo step");
    }

    #[test]
    fn arrows_move_the_cursor_by_the_nudge_step_and_space_places() {
        let mut app = app_with(Tool::Line);
        app.ui.prefs.editing.nudge_mm = 0.1;
        let ctx = egui::Context::default();
        let press = |app: &mut EditorApp, key: Key, mods: Modifiers| {
            let mut raw = egui::RawInput::default();
            raw.events.push(egui::Event::ModifiersChanged(mods));
            raw.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: mods,
            });
            let mut out = ctx.run_ui(raw, |ui| {
                keys(app, ui.ctx());
            });
            out.textures_delta.clear();
        };
        press(&mut app, Key::ArrowRight, Modifiers::NONE);
        let start = app.keyboard.at.expect("shown");
        press(&mut app, Key::ArrowRight, Modifiers::SHIFT);
        let moved = app.keyboard.at.expect("cursor");
        assert!((moved.x - start.x - 10.0).abs() < 1e-9, "Shift: 1 mm");
        press(&mut app, Key::Space, Modifiers::NONE);
        press(&mut app, Key::ArrowDown, Modifiers::SHIFT);
        press(&mut app, Key::Space, Modifiers::NONE);
        assert!(app.tools.is_busy(), "two points placed");
        app.cursor_command(Some(&json!({ "action": "back" })))
            .expect("back");
        assert!(app.tools.is_busy(), "one point left");
        press(&mut app, Key::Space, Modifiers::NONE);
        app.cursor_finish();
        assert_eq!(objects(&app).len(), 1, "an open line");
        // With the Select tool the arrows are left to the nudge.
        app.tools.set_tool(Tool::Select);
        press(&mut app, Key::ArrowRight, Modifiers::NONE);
        assert!(app.keyboard.at.is_none());
    }

    #[test]
    fn a_pen_curve_and_a_rectangle_from_the_keyboard() {
        let mut app = app_with(Tool::Pen);
        at(&mut app, 10.0, 10.0);
        place(&mut app);
        at(&mut app, 30.0, 20.0);
        app.cursor_command(Some(&json!({ "action": "smooth" })))
            .expect("smooth");
        at(&mut app, 50.0, 10.0);
        place(&mut app);
        app.cursor_command(Some(&json!({ "action": "finish" })))
            .expect("finish");
        let pen = objects(&app);
        assert_eq!(pen.len(), 1, "one shape.path");
        assert!(
            pen[0].contours.iter().any(|c| c.has_curves()),
            "the smooth point curves the path"
        );

        let mut app = app_with(Tool::Rectangle);
        at(&mut app, 10.0, 10.0);
        place(&mut app);
        at(&mut app, 40.0, 30.0);
        place(&mut app);
        let m = mouse(Tool::Rectangle, &[(100.0, 100.0), (400.0, 300.0)], true);
        assert_eq!(objects(&app), objects(&m), "the same shape.rectangle");
    }

    #[test]
    fn a_three_line_manual_punch_column_from_the_keyboard() {
        let mut app = app_with(Tool::Punch);
        for (x, y) in [
            (10.0, 10.0),
            (10.0, 20.0),
            (20.0, 10.0),
            (20.0, 20.0),
            (30.0, 10.0),
            (30.0, 20.0),
        ] {
            at(&mut app, x, y);
            place(&mut app);
        }
        app.cursor_command(Some(&json!({ "action": "finish" })))
            .expect("finish");
        let m = mouse(
            Tool::Punch,
            &[
                (100.0, 100.0),
                (100.0, 200.0),
                (200.0, 100.0),
                (200.0, 200.0),
                (300.0, 100.0),
                (300.0, 200.0),
            ],
            false,
        );
        assert_eq!(objects(&app).len(), 1);
        assert_eq!(objects(&app), objects(&m), "the same shape.column");
    }

    #[test]
    fn a_pen_line_with_widths_becomes_a_satin_line_in_one_undo_step() {
        let mut app = app_with(Tool::Pen);
        at(&mut app, 10.0, 10.0);
        app.cursor_command(Some(&json!({ "width": 2, "action": "place" })))
            .expect("place");
        at(&mut app, 60.0, 10.0);
        app.width_step(WIDTH_STEP * 4.0, None);
        place(&mut app);
        app.cursor_command(Some(&json!({ "action": "finish" })))
            .expect("finish");
        let o = &objects(&app)[0];
        let line = o.line.as_ref().expect("line");
        assert_eq!(line.kind, openstitch_design::LineType::Satin);
        let keys = &line.width_keys;
        assert_eq!(keys.len(), 2, "{keys:?}");
        assert_eq!(
            (keys[0].left, keys[1].left),
            (1.0, 2.0),
            "half of 2 mm and 4 mm"
        );
        assert_eq!(app.undo_groups.len(), 1, "path and satin are one undo step");
        app.undo();
        assert!(objects(&app).is_empty());
    }

    #[test]
    fn the_scroll_wheel_sets_the_width_only_while_pen_or_skeleton_draw() {
        let mut app = app_with(Tool::Skeleton);
        assert!(
            !scroll_width(&mut app, 1.0, v2(0.0, 0.0)),
            "not drawing yet: pans"
        );
        at(&mut app, 10.0, 10.0);
        place(&mut app);
        let before = app.tools.next_width().expect("width");
        assert!(scroll_width(&mut app, 1.0, v2(900.0, 900.0)));
        assert_eq!(app.tools.next_width(), Some(before + WIDTH_STEP));
        app.tools.set_tool(Tool::Polygon);
        assert!(!scroll_width(&mut app, 1.0, v2(0.0, 0.0)));
    }

    #[test]
    fn ui_cursor_refuses_tools_that_do_not_draw() {
        let mut app = app_with(Tool::Select);
        assert!(
            app.cursor_command(Some(&json!({ "action": "place" })))
                .is_err()
        );
        app.tools.set_tool(Tool::Polygon);
        assert!(
            app.cursor_command(Some(&json!({ "action": "jump" })))
                .is_err()
        );
        assert!(
            app.cursor_command(Some(&json!({ "width": 3 }))).is_err(),
            "no widths"
        );
        let r = app
            .ui_command("ui.cursor", Some(&json!({ "x": 5, "y": 5 })))
            .expect("through ui.cursor");
        assert_eq!(r["cursor"], json!([5.0, 5.0]));
        let _ = CommandRequest::new("design.info", json!({}));
    }
}
