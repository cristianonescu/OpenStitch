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
            // Links in the accent, darkened on the light theme until they read on white (#499).
            v.hyperlink_color = match theme {
                egui::Theme::Light => readable_on(fill, LIGHT_DARKEST_BG),
                egui::Theme::Dark => canvas,
            };
            v.widgets.hovered.weak_bg_fill = fill.gamma_multiply(0.25);
        });
    }
}

/// The darkest background text sits on in the light theme (striped rows).
const LIGHT_DARKEST_BG: [u8; 3] = [0xee, 0xf0, 0xf3];

/// `c`, darkened just enough to read on `bg` (WCAG 4.5:1).
fn readable_on(c: Color32, bg: [u8; 3]) -> Color32 {
    use openstitch_app::editor::accent::contrast;
    let base = [c.r(), c.g(), c.b()];
    let mut out = base;
    let mut f = 1.0;
    while contrast(out, bg) < 4.5 && f > 0.0 {
        f -= 0.02;
        out = base.map(|v| (f64::from(v) * f).round() as u8);
    }
    Color32::from_rgb(out[0], out[1], out[2])
}

/// Adjusts egui's light style towards a desktop application (once, at start-up).
pub fn install(ctx: &egui::Context) {
    ctx.style_mut_of(egui::Theme::Light, |s| {
        let v = &mut s.visuals;
        v.panel_fill = Color32::from_rgb(0xf3, 0xf4, 0xf6);
        v.window_fill = Color32::WHITE;
        v.extreme_bg_color = Color32::from_rgb(0xfb, 0xfb, 0xfc);
        v.faint_bg_color = Color32::from_rgb(0xee, 0xf0, 0xf3);
        // White text on it reaches 4.5:1 (#499).
        v.selection.bg_fill = Color32::from_rgb(0x23, 0x6b, 0xc2);
        v.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);
        v.hyperlink_color = Color32::from_rgb(0x1f, 0x5f, 0xb4);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xdc, 0xe9, 0xf8);
        v.widgets.hovered.bg_fill = Color32::from_rgb(0xdc, 0xe9, 0xf8);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xe8, 0xea, 0xee);
        // egui's light weak text is too faint to read (2.8:1); this is 4.5:1 or more (#499).
        v.weak_text_color = Some(Color32::from_gray(100));
        // Keyboard focus shows as the hovered look: a 1.5 pt outline visible on the panel
        // (WCAG non-text contrast 3:1, #499).
        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, Color32::from_rgb(0x1f, 0x5f, 0xb4));
    });
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        s.visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.5, Color32::from_rgb(0x8a, 0xb4, 0xf8));
        // egui's dark weak text is too faint to read (2.3:1); this is 4.5:1 or more.
        s.visuals.weak_text_color = Some(Color32::from_gray(150));
    });
}

#[cfg(test)]
mod contrast_tests {
    //! WCAG contrast of the themes (#499): text 4.5:1 on every background it is drawn on,
    //! focus and selection outlines 3:1 (non-text), in Light and Dark, with the default accent
    //! and with a light custom one.
    use super::*;
    use openstitch_app::editor::accent::contrast;

    /// `c` (premultiplied, egui's colours) drawn over the opaque `under`.
    fn over(c: Color32, under: [u8; 3]) -> [u8; 3] {
        let a = f64::from(c.a()) / 255.0;
        let ch = |v: u8, u: u8| (f64::from(v) + f64::from(u) * (1.0 - a)).round().min(255.0) as u8;
        [
            ch(c.r(), under[0]),
            ch(c.g(), under[1]),
            ch(c.b(), under[2]),
        ]
    }

    /// `fg` on `bg`, both as drawn on the opaque `panel`.
    fn check_on(
        problems: &mut Vec<String>,
        what: &str,
        fg: Color32,
        bg: Color32,
        panel: Color32,
        min: f64,
    ) {
        let base = over(bg, over(panel, [0, 0, 0]));
        let r = contrast(over(fg, base), base);
        if r < min {
            problems.push(format!(
                "{what}: {fg:?} on {bg:?} is {r:.2}:1 (needs {min}:1)"
            ));
        }
    }

    #[test]
    fn every_theme_colour_pair_meets_wcag_aa() {
        let mut problems = Vec::new();
        for accent in [None, Some([0xff, 0xd5, 0x4f])] {
            let ctx = egui::Context::default();
            set_accent(&ctx, accent);
            for (theme, canvas) in [(egui::Theme::Light, LIGHT), (egui::Theme::Dark, DARK)] {
                let v = ctx.style_of(theme).visuals.clone();
                let tag = |s: &str| format!("{theme:?} {accent:?} {s}");
                let panel = v.panel_fill;
                let check =
                    |p: &mut Vec<String>, what: &str, fg: Color32, bg: Color32, min: f64| {
                        check_on(p, what, fg, bg, panel, min)
                    };
                for (name, bg) in [
                    ("panel", v.panel_fill),
                    ("window", v.window_fill),
                    ("text field", v.extreme_bg_color),
                    ("striped row", v.faint_bg_color),
                ] {
                    check(
                        &mut problems,
                        &tag(&format!("text on {name}")),
                        v.text_color(),
                        bg,
                        4.5,
                    );
                    check(
                        &mut problems,
                        &tag(&format!("weak text on {name}")),
                        v.weak_text_color(),
                        bg,
                        4.5,
                    );
                    check(
                        &mut problems,
                        &tag(&format!("link on {name}")),
                        v.hyperlink_color,
                        bg,
                        4.5,
                    );
                }
                for (name, w) in [
                    ("button", &v.widgets.inactive),
                    ("hovered or focused button", &v.widgets.hovered),
                    ("pressed button", &v.widgets.active),
                    ("label", &v.widgets.noninteractive),
                ] {
                    check(
                        &mut problems,
                        &tag(&format!("{name} text")),
                        w.fg_stroke.color,
                        w.weak_bg_fill,
                        4.5,
                    );
                }
                check(
                    &mut problems,
                    &tag("selected text"),
                    v.selection.stroke.color,
                    v.selection.bg_fill,
                    4.5,
                );
                // Keyboard focus shows as the hovered style's outline: visible on the panel.
                check(
                    &mut problems,
                    &tag("focus outline on panel"),
                    v.widgets.hovered.bg_stroke.color,
                    v.panel_fill,
                    3.0,
                );
                check(
                    &mut problems,
                    &tag("ruler numbers"),
                    canvas.ruler_fg,
                    canvas.ruler_bg,
                    4.5,
                );
                check(
                    &mut problems,
                    &tag("canvas overlay text"),
                    canvas.overlay_text,
                    canvas.work_area,
                    4.5,
                );
            }
        }
        assert!(problems.is_empty(), "\n{}", problems.join("\n"));
    }

    #[test]
    fn focused_controls_have_a_visible_outline() {
        let ctx = egui::Context::default();
        set_accent(&ctx, None);
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let v = ctx.style_of(theme).visuals.clone();
            assert!(
                v.widgets.hovered.bg_stroke.width >= 1.5,
                "{theme:?}: focus outline {} pt",
                v.widgets.hovered.bg_stroke.width
            );
        }
    }
}
