//! The view across a change of the window's size (#1266): a window made smaller and larger
//! again shows the page where it was, a canvas of no size changes nothing, the view cannot be
//! lost and Fit to Window always brings it back. A child module of `app`, so it drives the
//! whole frame as the window does.

use super::*;
use openstitch_app::editor::ScreenRect;
use openstitch_design::{Rect, Vec2, v2};

/// The window the tests start from and return to.
const BIG: (f32, f32) = (1440.0, 900.0);
/// Smaller windows: the desktop's minimum (its docked panes leave a canvas 111 points wide), a
/// small browser window (a canvas 10 points wide), one that leaves no canvas, and none (a
/// minimised window).
const SMALL: [(f32, f32); 4] = [(900.0, 600.0), (390.0, 300.0), (120.0, 80.0), (0.0, 0.0)];
/// A maximised window on a large screen.
const MAXIMISED: (f32, f32) = (2560.0, 1400.0);

struct Window {
    app: EditorApp,
    ctx: egui::Context,
    frame: eframe::Frame,
}

impl Window {
    fn new(size: (f32, f32)) -> Window {
        let mut w = Window {
            app: EditorApp::new(Launch {
                demo: true,
                ..Launch::default()
            }),
            ctx: egui::Context::default(),
            frame: eframe::Frame::_new_kittest(),
        };
        w.size(size);
        w
    }

    /// A few frames at `size`, the first with `events`.
    fn frames(&mut self, size: (f32, f32), mut events: Vec<egui::Event>) {
        for _ in 0..4 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size.0, size.1),
                )),
                events: std::mem::take(&mut events),
                ..Default::default()
            };
            let (app, frame) = (&mut self.app, &mut self.frame);
            let mut out = self.ctx.run_ui(raw, |ui| eframe::App::ui(app, ui, frame));
            out.textures_delta.clear();
        }
    }

    fn size(&mut self, size: (f32, f32)) {
        self.frames(size, Vec::new());
    }

    fn view(&self) -> Viewport {
        self.app.viewport.expect("viewport")
    }

    fn canvas(&self) -> ScreenRect {
        crate::workspace_ui::screen_rect(self.app.workspace.canvas.expect("canvas"))
    }

    fn page(&self) -> Rect {
        let page = self.app.session.design().page;
        Rect {
            min_x: 0.0,
            min_y: 0.0,
            max_x: page.width,
            max_y: page.height,
        }
    }

    /// The design point in the middle of the canvas.
    fn centre(&self) -> Vec2 {
        self.view().to_model(self.canvas().center())
    }

    /// The page's width and height inside the canvas, in points.
    fn page_in_canvas(&self) -> (f64, f64) {
        let (vp, c, p) = (self.view(), self.canvas(), self.page());
        let (a, b) = (
            vp.to_screen(v2(p.min_x, p.min_y)),
            vp.to_screen(v2(p.max_x, p.max_y)),
        );
        (
            (b.x.min(c.x + c.width) - a.x.max(c.x)).max(0.0),
            (b.y.min(c.y + c.height) - a.y.max(c.y)).max(0.0),
        )
    }

    /// Whether the view is the automatic fit of the page into the canvas.
    fn assert_fitted(&self, what: &str) {
        let (vp, c) = (self.view(), self.canvas());
        let fit = Viewport::fit(self.page(), c, 24.0);
        assert!(
            (vp.scale - fit.scale).abs() < 1e-9
                && (vp.offset.x - fit.offset.x).abs() < 1e-6
                && (vp.offset.y - fit.offset.y).abs() < 1e-6,
            "{what}: the view is {vp:?}, the fit of the page into {c:?} is {fit:?}"
        );
    }
}

fn close(a: Vec2, b: Vec2) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

#[test]
fn a_window_made_smaller_and_larger_again_fits_the_page_again() {
    for small in SMALL {
        let mut w = Window::new(BIG);
        w.assert_fitted("at the start");
        let before = (w.view(), w.canvas());
        w.size(small);
        if w.canvas().usable() {
            w.assert_fitted(&format!("at {small:?}"));
        }
        w.size(BIG);
        w.assert_fitted(&format!("back from {small:?}"));
        // The same canvas again shows the very same view. (A narrow window folds the panes
        // away, and they stay folded: then the canvas is wider than before, and the page fits
        // that.)
        if w.canvas() == before.1 {
            assert_eq!(w.view(), before.0, "back from {small:?}");
        }
        // Up to a maximised window, down to this size and back.
        w.size(MAXIMISED);
        w.assert_fitted("maximised");
        w.size(small);
        w.size(MAXIMISED);
        w.assert_fitted(&format!("maximised again after {small:?}"));
        w.size(BIG);
        w.assert_fitted("restored");
    }
}

#[test]
fn a_view_the_user_set_keeps_its_centre_and_zoom_across_window_sizes() {
    for small in SMALL {
        let mut w = Window::new(BIG);
        // Zoomed in twice about the canvas's middle and panned, as with the wheel.
        let mid = w.canvas().center();
        let vp = w.app.viewport.as_mut().expect("viewport");
        vp.zoom_about(mid, 2.0);
        vp.pan(-150.0, 80.0);
        w.size(BIG);
        let (scale, centre) = (w.view().scale, w.centre());
        assert!(
            (centre.x - 500.0).abs() > 50.0,
            "the view is off the page's middle: {centre:?}"
        );
        for size in [small, BIG, MAXIMISED, small, BIG] {
            w.size(size);
            // (After the narrow sizes the panes stay folded and the canvas is 549 points wide
            // where it was 429: the middle still shows the same point.)
            assert_eq!(w.view().scale, scale, "the zoom at {size:?} (via {small:?})");
            // In a canvas too small to show anything the stored view waits unchanged.
            if w.canvas().usable() {
                assert!(
                    close(w.centre(), centre),
                    "at {size:?} (via {small:?}) the canvas's middle shows {:?}, it showed {centre:?}",
                    w.centre()
                );
            }
        }
    }
}

#[test]
fn a_canvas_of_no_size_changes_nothing_in_the_stored_view() {
    for touched in [false, true] {
        let mut w = Window::new(BIG);
        if touched {
            w.app.viewport.as_mut().expect("viewport").pan(40.0, -25.0);
            w.size(BIG);
        }
        let before = w.view();
        for none in [(0.0, 0.0), (120.0, 80.0)] {
            w.size(none);
            assert!(!w.canvas().usable(), "{none:?} leaves {:?}", w.canvas());
            assert_eq!(w.view(), before, "touched {touched}, window {none:?}");
            // A fit asked for meanwhile waits for a canvas.
            w.app.fit_requested = true;
            w.size(none);
            assert_eq!(w.view(), before, "touched {touched}, window {none:?}, fit asked");
        }
        w.size(BIG);
        w.assert_fitted("the fit asked for while there was no canvas");
        assert!(!w.app.fit_requested);
    }
}

#[test]
fn the_zoom_field_does_not_write_a_view_under_its_range_back() {
    // An automatic fit can be under the zoom field's 10 % (a large page, a small canvas). The
    // field wrote 10 % back a frame later; the view was then no longer the fit, and the page
    // stayed tiny when the window grew.
    let mut w = Window::new(BIG);
    let c = w.canvas().center();
    let low = Viewport {
        scale: Viewport::MIN_SCALE,
        offset: c - v2(500.0, 500.0) * Viewport::MIN_SCALE,
    };
    assert!(low.zoom_percent(w.app.points_per_mm()) < 10.0);
    w.app.viewport = Some(low);
    w.size(BIG);
    assert_eq!(w.view(), low);
}

#[test]
fn the_view_cannot_be_panned_away_from_the_page() {
    let mut w = Window::new(BIG);
    for (dx, dy) in [
        (1.0e7, 0.0),
        (-1.0e7, 0.0),
        (0.0, 1.0e7),
        (0.0, -1.0e7),
        (-3.0e5, 4.0e5),
    ] {
        w.app.viewport.as_mut().expect("viewport").pan(dx, dy);
        w.size(BIG);
        let (px, py) = w.page_in_canvas();
        assert!(
            px >= Viewport::KEEP_IN_VIEW - 1e-6 && py >= Viewport::KEEP_IN_VIEW - 1e-6,
            "panned by ({dx}, {dy}): {px} x {py} points of the page are inside the canvas"
        );
    }
    // Zoomed all the way in and out at a corner of the canvas, the page is still there.
    for factor in [1.0e9, 1.0e-9] {
        let c = w.canvas();
        w.app
            .viewport
            .as_mut()
            .expect("viewport")
            .zoom_about(v2(c.x, c.y), factor);
        w.size(BIG);
        let vp = w.view();
        assert!((Viewport::MIN_SCALE..=Viewport::MAX_SCALE).contains(&vp.scale));
        let (px, py) = w.page_in_canvas();
        assert!(px > 0.0 && py > 0.0, "zoomed by {factor}: {vp:?}");
    }
}

/// Views no drawing can be made with, and one far outside the zoom limits.
fn broken_views() -> Vec<Viewport> {
    let at = |scale: f64, x: f64, y: f64| Viewport {
        scale,
        offset: v2(x, y),
    };
    vec![
        at(0.0, 0.0, 0.0),
        at(f64::NAN, 100.0, 100.0),
        at(f64::INFINITY, 100.0, 100.0),
        at(-1.0, 100.0, 100.0),
        at(0.5, f64::NAN, 0.0),
        at(0.5, 0.0, f64::NEG_INFINITY),
        at(1.0e-12, 1.0e30, -1.0e30),
        at(1.0e12, -1.0e30, 1.0e30),
    ]
}

#[test]
fn a_broken_view_is_made_usable_in_the_next_frame() {
    for bad in broken_views() {
        let mut w = Window::new(BIG);
        w.app.viewport = Some(bad);
        w.size(BIG);
        let vp = w.view();
        assert!(
            vp.is_sane() && (Viewport::MIN_SCALE..=Viewport::MAX_SCALE).contains(&vp.scale),
            "{bad:?} became {vp:?}"
        );
        let (px, py) = w.page_in_canvas();
        assert!(px > 0.0 && py > 0.0, "{bad:?} became {vp:?}");
    }
}

#[test]
fn fit_to_window_brings_the_page_back_from_any_view() {
    let key = egui::Event::Key {
        key: egui::Key::Num0,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    };
    for bad in broken_views() {
        // The shortcut (Ctrl+0, the menu's and the zoom tools' command).
        let mut w = Window::new(BIG);
        w.app.viewport = Some(bad);
        w.frames(BIG, vec![key.clone()]);
        w.assert_fitted(&format!("Ctrl+0 from {bad:?}"));
        // The command the Assistant and scripts use.
        w.app.viewport = Some(bad);
        w.app
            .ui_command("ui.zoom", Some(&serde_json::json!({ "fit": "page" })))
            .expect("ui.zoom");
        w.size(BIG);
        w.assert_fitted(&format!("ui.zoom fit from {bad:?}"));
        // The ribbon's and the menu's entry set the same request.
        w.app.viewport = Some(bad);
        w.app.fit_requested = true;
        w.size(BIG);
        w.assert_fitted(&format!("Fit from {bad:?}"));
    }
}

#[test]
fn ui_state_says_where_the_canvas_looks() {
    let mut w = Window::new(BIG);
    w.size(BIG);
    let state = w.app.ui_command("ui.state", None).expect("ui.state");
    let view = &state["view"];
    let c = w.canvas();
    assert_eq!(view["centerMm"], serde_json::json!([50.0, 50.0]), "{view}");
    assert_eq!(
        view["canvasPoints"],
        serde_json::json!([c.width as f32, c.height as f32])
    );
    let (px, py) = w.page_in_canvas();
    let got = |i: usize| view["pagePointsInCanvas"][i].as_f64().expect("number");
    assert!((got(0) - px).abs() < 0.01 && (got(1) - py).abs() < 0.01, "{view}");
    // The command says what the entry holds.
    let summary = openstitch_app::editor::assistant::ui_command("ui.state")
        .expect("ui.state")
        .summary;
    for part in [
        "view: centerMm, the design point in the middle of the canvas in mm",
        "canvasPoints, the canvas's width and height in points",
        "pagePointsInCanvas, the width and height in points of the Design Page's part inside the canvas",
    ] {
        assert!(summary.contains(part), "{part}: {summary}");
    }
    // Out of sight is 0: the view is kept from getting there, so the entry is checked on a
    // view set by hand and read before the next frame.
    w.app.viewport.as_mut().expect("viewport").pan(1.0e6, 0.0);
    let away = w.app.ui_command("ui.state", None).expect("ui.state");
    assert_eq!(away["view"]["pagePointsInCanvas"][0], serde_json::json!(0.0));
}

#[test]
fn a_scroll_step_while_the_window_leaves_no_canvas_does_not_keep_a_tiny_page() {
    // On main the page was fitted into the few points left (scale 0.02 to 0.04); one scroll
    // step there made that the user's view, and the page stayed 20 to 40 points wide, outside
    // the canvas, when the window was large again.
    for none in [(390.0, 300.0), (120.0, 80.0), (0.0, 0.0)] {
        let mut w = Window::new(BIG);
        let before = w.view();
        w.size(none);
        assert!(!w.canvas().usable(), "{none:?} leaves {:?}", w.canvas());
        w.app.viewport.as_mut().expect("viewport").pan(0.0, -10.0);
        w.size(none);
        w.size(BIG);
        assert_eq!(w.view().scale, before.scale, "back from {none:?}");
        let (px, py) = w.page_in_canvas();
        let side = 1000.0 * before.scale;
        assert!(
            px > side - 1.0 && py > side - 11.0,
            "back from {none:?}: {px} x {py} points of the page's {side} are in the canvas"
        );
    }
}
