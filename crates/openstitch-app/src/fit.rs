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
    let frame = egui::Frame::menu(ui.style()).total_margin().sum().x;
    let wide = (ui.ctx().content_rect().width() - frame - 2.0 * EDGE).max(LEAST);
    visible_bars(ui);
    // Sideways only for a list wider than the window (its last frame says so): a scroll area
    // clips at its edge, which a list that fits has no use for.
    let key = ui.scope_id().with("fit-scroll-width");
    let last: f32 = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0.0);
    let out = egui::ScrollArea::new([last > wide + 1.0, true])
        .id_salt("fit-scroll")
        .max_height(max)
        .max_width(wide)
        .min_scrolled_height(max)
        .min_scrolled_width(wide)
        .auto_shrink([true, true])
        .show(ui, add);
    if (out.content_size.x - last).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(key, out.content_size.x));
        ui.ctx().request_repaint();
    }
    out.inner
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

/// Scroll bars that show while at rest, thin and over the content, so a list that goes on is
/// seen to go on.
fn visible_bars(ui: &mut egui::Ui) {
    let mut bars = egui::style::ScrollStyle::floating();
    bars.dormant_handle_opacity = 0.5;
    ui.spacing_mut().scroll = bars;
}

/// What a dialog measured in its last frame.
#[derive(Clone, Copy, Default, PartialEq)]
struct Sizes {
    /// The height of its content without the button row.
    content: f32,
    /// The height of the button row ([`buttons`]), 0 without one.
    buttons: f32,
    /// The width of the content's view.
    width: f32,
    /// The width of the content itself.
    content_width: f32,
}

/// The dialog being drawn, for [`buttons`].
#[derive(Clone, Copy)]
struct Open {
    /// The dialog's own clip rectangle (the scrolled content has a smaller one).
    clip: egui::Rect,
    /// Where the button row goes when it is kept below the scrolled content.
    pinned: Option<egui::Rect>,
    /// The height the row took this frame.
    measured: f32,
    used: bool,
}

fn open_id() -> egui::Id {
    egui::Id::unique("fit-open-dialog")
}

/// Shows a dialog window: its content scrolls once it is taller (or wider) than the window
/// leaves room for, and the row given to [`buttons`] stays in view below it.
pub fn show<R>(
    window: egui::Window<'_>,
    ctx: &egui::Context,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<Option<R>>> {
    window.show(ctx, |ui| dialog(ui, add))
}

/// Shows a modal prompt as [`show`] does a window.
pub fn modal<R>(
    modal: egui::Modal,
    ctx: &egui::Context,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::ModalResponse<R> {
    modal.show(ctx, |ui| dialog(ui, add))
}

/// A window that fills its own size with a list that scrolls by itself (a table of rows, a
/// page of text): shown as it is, limited to the window by the toolkit. The reach test checks
/// it like every other.
pub fn show_filling<R>(
    window: egui::Window<'_>,
    ctx: &egui::Context,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<Option<R>>> {
    window.show(ctx, |ui| {
        visible_bars(ui);
        add(ui)
    })
}

fn dialog<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let key = ui.scope_id().with("fit-dialog");
    let last: Sizes = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    let screen = ctx.content_rect();
    let top = ui.cursor().top();
    // The title bar and whatever else lies above the content; the frame's margin below it.
    let area = ctx.memory(|m| m.area_rect(ui.layer_id().id));
    let above = area.map_or(32.0, |r| (top - r.top()).max(0.0));
    let margin = ui.spacing().window_margin;
    let below = f32::from(margin.bottom) + 2.0;
    let room = (screen.height() - 2.0 * EDGE - above - below).max(LEAST);
    let sides = f32::from(margin.left) + f32::from(margin.right) + 2.0;
    let width = (screen.width() - 2.0 * EDGE - sides).max(LEAST);
    // The buttons leave the scrolled content only when both do not fit, so a dialog that fits
    // is laid out as it always was.
    let pin = last.buttons > 0.0 && last.content + last.buttons > room + 0.5;
    let view = if pin {
        (room - last.buttons).max(LEAST)
    } else {
        room
    };
    let gap = ui.spacing().item_spacing.y;
    let pinned = pin.then(|| {
        egui::Rect::from_min_size(
            egui::pos2(ui.cursor().left(), top + view.min(last.content) + gap),
            egui::vec2(last.width.min(width), last.buttons - gap),
        )
    });
    ctx.data_mut(|d| {
        d.insert_temp(
            open_id(),
            Open {
                clip: ui.clip_rect(),
                pinned,
                measured: 0.0,
                used: false,
            },
        )
    });
    visible_bars(ui);
    let out = egui::ScrollArea::new([last.content_width > width + 1.0, true])
        .id_salt("fit-dialog")
        .max_height(view)
        .max_width(width)
        // As tall as the content asks for, up to that: without this a window that opened
        // small would keep the size of its first frame.
        .min_scrolled_height(view)
        .min_scrolled_width(width)
        .auto_shrink([true, true])
        .show(ui, add);
    let buttons = ctx
        .data(|d| d.get_temp::<Open>(open_id()))
        .map_or(0.0, |o| o.measured);
    ctx.data_mut(|d| d.remove::<Open>(open_id()));
    if let Some(rect) = pinned.filter(|_| buttons > 0.0) {
        // The row was drawn there already: keep its place and rule it off the content.
        let line = ui.visuals().widgets.noninteractive.bg_stroke;
        ui.painter()
            .hline(rect.x_range(), rect.top() - gap / 2.0, line);
        ui.allocate_space(egui::vec2(out.inner_rect.width(), buttons - gap));
    }
    let now = Sizes {
        content: out.content_size.y - if pin { 0.0 } else { buttons },
        buttons,
        width: out.inner_rect.width(),
        content_width: out.content_size.x,
    };
    if now != last {
        ctx.data_mut(|d| d.insert_temp(key, now));
        ctx.request_repaint();
    }
    out.inner
}

/// The button row of a dialog shown with [`show`] (OK, Cancel, Apply): drawn where it stands
/// while the dialog fits, and kept in view below the scrolled content when it does not.
/// Anywhere else it is drawn in place.
pub fn buttons<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let open = ctx
        .data(|d| d.get_temp::<Open>(open_id()))
        .filter(|o| !o.used);
    let Some(mut open) = open else {
        return add(ui);
    };
    let gap = ui.spacing().item_spacing.y;
    let builder = egui::UiBuilder::new().id_salt("fit-buttons");
    let inner = match open.pinned {
        Some(rect) => {
            let mut row = ui.new_child(builder.max_rect(rect));
            row.set_clip_rect(open.clip);
            let inner = add(&mut row);
            open.measured = row.min_rect().height() + gap;
            inner
        }
        None => {
            let mut row = ui.new_child(builder.max_rect(ui.available_rect_before_wrap()));
            let inner = add(&mut row);
            let used = row.min_rect();
            ui.allocate_rect(used, egui::Sense::hover());
            open.measured = used.height() + gap;
            inner
        }
    };
    open.used = true;
    ctx.data_mut(|d| d.insert_temp(open_id(), open));
    inner
}

/// What a pane measured in its last frame.
#[derive(Clone, Copy, Default, PartialEq)]
struct PaneSizes {
    /// Its content did not fit: it is drawn scrolling.
    scrolls: bool,
    /// The height the content takes when it scrolls (its lists at their least height).
    needed: f32,
}

/// The content of a pane (docked or floating). While it fits it is laid out as it is; once the
/// pane is lower than its content, the whole content scrolls, so the rows below the edge can
/// be reached. Lists inside that fill the pane are made with [`list`].
pub fn pane<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let key = ui.scope_id().with("fit-pane");
    let last: PaneSizes = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    let room = ui.available_height();
    let scrolls = last.scrolls && last.needed > room + 0.5;
    let (inner, now) = if scrolls {
        visible_bars(ui);
        let out = egui::ScrollArea::vertical()
            .id_salt("fit-pane")
            .auto_shrink([false, false])
            .show(ui, add);
        let now = PaneSizes {
            scrolls: true,
            needed: out.content_size.y,
        };
        (out.inner, now)
    } else {
        let bottom = ui.max_rect().bottom();
        let inner = add(ui);
        let over = ui.min_rect().bottom() > bottom + 1.0;
        let now = PaneSizes {
            scrolls: over,
            needed: if over { f32::INFINITY } else { 0.0 },
        };
        (inner, now)
    };
    if now != last {
        ctx.data_mut(|d| d.insert_temp(key, now));
        ctx.request_repaint();
    }
    inner
}

/// A list that fills the rest of its pane, at least `least` points high: in a pane too low for
/// that (or one that scrolls as a whole, see [`pane`]) it keeps that height.
pub fn list(ui: &egui::Ui, least: f32) -> egui::ScrollArea {
    let room = ui.available_height();
    let area = egui::ScrollArea::vertical().auto_shrink([false, false]);
    if room.is_finite() && room >= least {
        area
    } else {
        area.max_height(least)
    }
}

/// The most a pane docked at the top or bottom may take of `height`, so the work area and
/// the panes beside it keep `rest` points.
pub fn pane_limit(height: f32, rest: f32) -> f32 {
    (height - rest).max(LEAST)
}
