//! A narrow window (#834: a phone, 390 × 844 points): the canvas keeps the whole width while
//! the panes float folded, and the ribbon's tabs scroll sideways under a finger so View,
//! Options and Help stay reachable. A wide window docks the panes as before. A child module of
//! `app`, so it drives the whole frame (`eframe::App::ui`) as the window does; the browser page
//! test (web/e2e/page.mjs) taps the point checked here.

use super::*;
use eframe::egui::accesskit::{Node, Role};

/// Where the page test taps on a phone: the middle of the screen, below the ribbon.
const PHONE_TAP: egui::Pos2 = egui::pos2(195.0, 520.0);

struct Window {
    app: EditorApp,
    ctx: egui::Context,
    frame: eframe::Frame,
    size: egui::Vec2,
    nodes: Vec<Node>,
    /// The clock, 60 frames a second, so egui's scroll animations run.
    time: f64,
}

impl Window {
    fn new(size: (f32, f32)) -> Window {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = EditorApp::new(Launch::default());
        // As the browser version, which has no Assistant panel (#766).
        app.assistant.panel.open = false;
        let mut w = Window {
            app,
            ctx,
            frame: eframe::Frame::_new_kittest(),
            size: egui::vec2(size.0, size.1),
            nodes: Vec::new(),
            time: 0.0,
        };
        w.frames(4, Vec::new());
        w
    }

    /// Runs `n` frames, the first one with `events`.
    fn frames(&mut self, n: usize, mut events: Vec<egui::Event>) {
        for _ in 0..n {
            self.time += 1.0 / 60.0;
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
                time: Some(self.time),
                events: std::mem::take(&mut events),
                ..Default::default()
            };
            let app = &mut self.app;
            let frame = &mut self.frame;
            let mut out = self.ctx.run_ui(raw, |ui| eframe::App::ui(app, ui, frame));
            out.textures_delta.clear();
            if let Some(update) = out.platform_output.accesskit_update {
                self.nodes = update.nodes.into_iter().map(|(_, n)| n).collect();
            }
        }
    }

    /// The bounds of the button labelled `label` (a ribbon tab, Options, Help).
    fn button(&self, label: &str) -> Option<egui::Rect> {
        self.nodes
            .iter()
            .filter(|n| n.role() == Role::Button)
            .filter(|n| n.label().is_some_and(|l| l.trim() == label))
            .filter_map(Node::bounds)
            // The widest: the Quick Access Toolbar has a small Help button of its own.
            .max_by(|a, b| (a.x1 - a.x0).total_cmp(&(b.x1 - b.x0)))
            .map(|b| {
                egui::Rect::from_min_max(
                    egui::pos2(b.x0 as f32, b.y0 as f32),
                    egui::pos2(b.x1 as f32, b.y1 as f32),
                )
            })
    }

    /// A finger dragged from `from` to `to`: touch events with the pointer events they make,
    /// as the browser page sends them (eframe's touch handling).
    fn drag(&mut self, from: egui::Pos2, to: egui::Pos2) {
        let touch = |pos, phase| egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase,
            pos,
            force: None,
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frames(
            1,
            vec![
                touch(from, egui::TouchPhase::Start),
                egui::Event::PointerMoved(from),
                press(from, true),
            ],
        );
        for i in 1..=8 {
            let p = from + (to - from) * (i as f32 / 8.0);
            self.frames(
                1,
                vec![touch(p, egui::TouchPhase::Move), egui::Event::PointerMoved(p)],
            );
        }
        self.frames(
            30,
            vec![
                touch(to, egui::TouchPhase::End),
                press(to, false),
                egui::Event::PointerGone,
            ],
        );
    }
}

#[test]
fn a_phone_keeps_the_canvas_and_the_ribbon_tabs_scroll() {
    let mut w = Window::new((390.0, 844.0));
    let canvas = w.app.workspace.canvas.expect("canvas");
    assert!(
        canvas.width() >= 360.0 && canvas.height() >= 250.0,
        "the canvas at phone width is {canvas:?}"
    );
    assert!(canvas.contains(PHONE_TAP), "{canvas:?}");
    // The panes are still on (the preferences are not changed), floating folded.
    let p = &w.app.ui.prefs.panes;
    assert!(p.sewing_order.visible && !p.sewing_order.floating);
    // Every tab, Options and Help are reachable at this width.
    let help = w.button("Help").expect("Help in the tab strip");
    assert!(help.max.x <= 390.0, "{help:?}");
    // A selection adds its context tabs, which push Help past the right edge; a finger that
    // drags the strip, starting on a tab, brings it into view (and opens no tab).
    w.app
        .execute(&openstitch_app::editor::CommandRequest::new(
            "shape.rectangle",
            serde_json::json!({ "x": 10, "y": 10, "width": 20, "height": 20 }),
        ))
        .expect("rectangle");
    let id = w.app.session.design().objects.last().map(|o| o.id).expect("id");
    w.app.selection = [id].into_iter().collect();
    w.frames(4, Vec::new());
    let help = w.button("Help").expect("Help in the tab strip");
    assert!(help.max.x > 390.0, "{help:?}");
    let home = w.button("Home").expect("Home tab").center();
    w.drag(home, home - egui::vec2(200.0, 0.0));
    let help = w.button("Help").expect("Help after the drag");
    assert!(help.max.x <= 390.0, "after a drag Help is at {help:?}");
    assert_eq!(w.app.ui.tab, openstitch_app::editor::ribbon::TabId::Home);
}

#[test]
fn a_wide_window_docks_the_panes_as_before() {
    let w = Window::new((1440.0, 900.0));
    let canvas = w.app.workspace.canvas.expect("canvas");
    // Sewing Order (250) on the left and Attributes (330) on the right are docked beside it.
    assert!(canvas.min.x >= 200.0 && canvas.max.x <= 1440.0 - 280.0, "{canvas:?}");
    let help = w.button("Help").expect("Help");
    assert!(help.max.x <= 1440.0 && help.min.x > 1200.0, "Help at the right: {help:?}");
}
