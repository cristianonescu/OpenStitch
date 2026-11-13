//! `--px-per-mm` (#1051): a fixed screenshot scale, the `--center` point mid-canvas on whole
//! pixels, whatever the window's size, so kit preview tiles do not depend on the layout. A child
//! module of `app`, so it drives the whole frame as the window does.

use super::*;

/// The view after a few frames of a `size` window, launched with `launch`.
fn view(launch: Launch, size: (f32, f32)) -> (Viewport, egui::Rect) {
    let mut app = EditorApp::new(launch);
    let ctx = egui::Context::default();
    let mut frame = eframe::Frame::_new_kittest();
    for _ in 0..4 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size.0, size.1),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| eframe::App::ui(&mut app, ui, &mut frame));
        out.textures_delta.clear();
    }
    (
        app.viewport.expect("viewport"),
        app.workspace.canvas.expect("canvas"),
    )
}

#[test]
fn a_fixed_scale_puts_the_centre_mid_canvas_on_whole_pixels_in_any_window() {
    let launch = || Launch {
        demo: true,
        px_per_mm: Some(16.0),
        center: Some((31.3, 47.9)),
        ..Launch::default()
    };
    for size in [(1440.0, 900.0), (1180.0, 800.0), (1600.0, 1000.0)] {
        let (vp, canvas) = view(launch(), size);
        // 16 px/mm at one pixel per point: 1.6 points per 0.1 mm.
        assert!((vp.scale - 1.6).abs() < 1e-12, "{size:?}: {}", vp.scale);
        assert_eq!(vp.offset.x, vp.offset.x.round(), "{size:?}");
        assert_eq!(vp.offset.y, vp.offset.y.round(), "{size:?}");
        let at = vp.to_screen(openstitch_design::v2(313.0, 479.0));
        let mid = canvas.center();
        assert!(
            (at.x - f64::from(mid.x)).abs() <= 0.5 && (at.y - f64::from(mid.y)).abs() <= 0.5,
            "{size:?}: the centre lands at {at:?}, the canvas's middle is {mid:?}"
        );
    }
    // Without --px-per-mm the view still fits the page, so its scale follows the window.
    let fit = |size| view(Launch::default(), size).0.scale;
    assert!((fit((1440.0, 900.0)) - fit((1180.0, 800.0))).abs() > 1e-3);
}
