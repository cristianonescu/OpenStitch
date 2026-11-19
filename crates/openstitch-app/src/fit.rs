//! Surfaces that fit the window (#1276): a menu, a list or a dialog taller than the window
//! scrolls inside it instead of running past its edge, so every entry and every button can be
//! reached at a small window or a large Text size.
//!
//! Every menu and pop-up of the app opens through [`menu`], [`popup`], [`sub_menu`] or
//! [`context_menu`], and every dialog puts its content in [`body`]; `fit_tests.rs` fails for a
//! menu or a window made without them.

use eframe::egui;

/// The gap kept between a pop-up or a dialog and the window's edge.
pub const EDGE: f32 = 4.0;
/// A list is never squeezed below this height, however small the window.
const LEAST: f32 = 40.0;

/// The height the content of a pop-up may take in this window: the window's, less the gap at
/// both edges and the pop-up's own frame.
pub fn room(ctx: &egui::Context) -> f32 {
    let style = ctx.global_style();
    let frame = egui::Frame::menu(&style);
    let chrome = frame.total_margin().sum().y + 2.0 * EDGE;
    (ctx.content_rect().height() - chrome).max(LEAST)
}

/// The content of a menu, list or pop-up: as tall as it needs, and scrolling (wheel, touchpad,
/// the bar at its side; a row focused from the keyboard is brought into view) once it is
/// taller than the window.
pub fn scroll<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let max = room(ui.ctx());
    egui::ScrollArea::vertical()
        .id_salt("fit-scroll")
        .max_height(max)
        .auto_shrink([true, true])
        .show(ui, add)
        .inner
}

/// Shows `popup` with its content in [`scroll`].
pub fn popup<R>(
    popup: egui::Popup<'_>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<R>> {
    popup.show(|ui| scroll(ui, add))
}

/// The menu of a button: opens on its click, below it or where there is room.
pub fn menu<R>(
    button: &egui::Response,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<R>> {
    popup(egui::Popup::menu(button), add)
}

/// A row of a menu that opens a sub-menu, on the side that has room.
pub fn sub_menu<'a, R>(
    ui: &mut egui::Ui,
    label: impl egui::IntoAtoms<'a>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    ui.menu_button(label, |ui| scroll(ui, add))
}

/// The menu a right click on `on` opens.
pub fn context_menu(
    on: &egui::Response,
    add: impl FnOnce(&mut egui::Ui),
) -> Option<egui::InnerResponse<()>> {
    on.context_menu(|ui| scroll(ui, add))
}
