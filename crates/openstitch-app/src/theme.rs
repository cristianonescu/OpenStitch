//! Colour themes. Light (the default) resembles a desktop application on Windows: a light
//! ribbon, white panes and a white Design Page on a grey work area. Dark keeps the previous
//! look; System follows the operating system.

use eframe::egui::{self, Color32};

/// Colours the canvas, rulers and Reference window paint with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasColors {
    pub work_area: Color32,
    pub page: Color32,
    pub grid_minor: Color32,
    pub grid_major: Color32,
    pub ruler_bg: Color32,
    pub ruler_fg: Color32,
    pub overlay_text: Color32,
}

pub const DARK: CanvasColors = CanvasColors {
    work_area: Color32::from_rgb(0x3a, 0x3d, 0x42),
    page: Color32::from_rgb(0xf6, 0xf3, 0xec),
    grid_minor: Color32::from_rgb(0xe2, 0xde, 0xd4),
    grid_major: Color32::from_rgb(0xcf, 0xca, 0xbe),
    ruler_bg: Color32::from_rgb(0x2b, 0x2d, 0x31),
    ruler_fg: Color32::from_gray(190),
    overlay_text: Color32::from_gray(220),
};

pub const LIGHT: CanvasColors = CanvasColors {
    work_area: Color32::from_rgb(0xb4, 0xb8, 0xbe),
    page: Color32::WHITE,
    grid_minor: Color32::from_rgb(0xe4, 0xe7, 0xec),
    grid_major: Color32::from_rgb(0xc9, 0xcf, 0xd8),
    ruler_bg: Color32::from_rgb(0xf3, 0xf4, 0xf6),
    ruler_fg: Color32::from_gray(70),
    overlay_text: Color32::from_gray(25),
};

/// The canvas colours of the theme in use.
pub fn canvas(ctx: &egui::Context) -> CanvasColors {
    match ctx.theme() {
        egui::Theme::Dark => DARK,
        egui::Theme::Light => LIGHT,
    }
}

/// The canvas accent before #559: selection outlines, handles and point markers.
pub const DEFAULT_ACCENT: Color32 = Color32::from_rgb(0x1e, 0x88, 0xe5);

static CANVAS_ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x1e88e5);

/// The accent the canvas paints the selection, handles and points with.
pub fn accent() -> Color32 {
    let v = CANVAS_ACCENT.load(std::sync::atomic::Ordering::Relaxed);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Installs the styles with the accent colour (#559): `None` keeps each theme's own colours.
/// Selected buttons get a shade dark enough for white text (4.5:1); links and the canvas use the
/// colour itself.
pub fn set_accent(ctx: &egui::Context, accent: Option<[u8; 3]>) {
    // The themes' own colours first, so going back to the default undoes a custom accent.
    for (theme, base) in [
        (egui::Theme::Light, egui::Visuals::light()),
        (egui::Theme::Dark, egui::Visuals::dark()),
    ] {
        ctx.style_mut_of(theme, |s| {
            let v = &mut s.visuals;
            v.selection = base.selection;
            v.hyperlink_color = base.hyperlink_color;
            v.widgets.hovered.weak_bg_fill = base.widgets.hovered.weak_bg_fill;
        });
    }
    install(ctx);
    let canvas = accent.map_or(DEFAULT_ACCENT, |[r, g, b]| Color32::from_rgb(r, g, b));
    let [r, g, b, _] = canvas.to_array();
    CANVAS_ACCENT.store(
        u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b),
        std::sync::atomic::Ordering::Relaxed,
    );
    let Some(c) = accent else { return };
    let [r, g, b] = openstitch_app::editor::accent::for_white_text(c);
    let fill = Color32::from_rgb(r, g, b);
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        ctx.style_mut_of(theme, |s| {
            let v = &mut s.visuals;
            v.selection.bg_fill = fill;
            v.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);
            v.hyperlink_color = canvas;
            v.widgets.hovered.weak_bg_fill = fill.gamma_multiply(0.25);
        });
    }
}

/// Adjusts egui's light style towards a desktop application (once, at start-up).
pub fn install(ctx: &egui::Context) {
    ctx.style_mut_of(egui::Theme::Light, |s| {
        let v = &mut s.visuals;
        v.panel_fill = Color32::from_rgb(0xf3, 0xf4, 0xf6);
        v.window_fill = Color32::WHITE;
        v.extreme_bg_color = Color32::from_rgb(0xfb, 0xfb, 0xfc);
        v.faint_bg_color = Color32::from_rgb(0xee, 0xf0, 0xf3);
        v.selection.bg_fill = Color32::from_rgb(0x2a, 0x7a, 0xd8);
        v.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);
        v.hyperlink_color = Color32::from_rgb(0x1f, 0x5f, 0xb4);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xdc, 0xe9, 0xf8);
        v.widgets.hovered.bg_fill = Color32::from_rgb(0xdc, 0xe9, 0xf8);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xe8, 0xea, 0xee);
    });
}
