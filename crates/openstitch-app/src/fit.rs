//! Surfaces that fit the window (#1276): a menu, a list or a dialog taller than the window
//! scrolls inside it instead of running past its edge, so every entry and every button can be
//! reached at a small window or a large Text size.
//!
//! Every menu and pop-up of the app opens through [`menu`], [`popup`], [`sub_menu`] or
//! [`context_menu`], every dialog through [`show`] or [`modal`], every pane puts its content
//! in [`pane`], and the window's own content is a [`page`]; `reach_tests.rs` fails for a menu
//! or a window made without them, and walks every one of them at a small window.

use eframe::egui;

/// The gap kept between a pop-up or a dialog and the window's edge.
pub const EDGE: f32 = 4.0;
/// A list is never squeezed below this height, however small the window.
const LEAST: f32 = 40.0;
/// A menu stays below (or above) its button while that leaves it this height.
const BESIDE_LEAST: f32 = 100.0;
/// A dialog keeps its button row in view only while that leaves the content this height: in
/// less (a window under about 130 points high) the row scrolls with the content, below it.
pub const PIN_LEAST: f32 = 60.0;

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
    scroll_in(ui, max, add)
}

/// The height the content of a pop-up opened from `button` may take: the room below the
/// button, or above it, so the pop-up does not open over its own button and the pointer; the
/// whole window's where neither leaves [`BESIDE_LEAST`].
fn room_beside(ctx: &egui::Context, button: egui::Rect) -> f32 {
    let screen = ctx.content_rect();
    let all = room(ctx);
    let chrome = screen.height() - all;
    let side = (screen.bottom() - button.bottom()).max(button.top() - screen.top()) - chrome;
    if side >= BESIDE_LEAST { side.min(all) } else { all }
}

/// The lists being drawn, innermost last: a sub-menu's list is drawn inside the frame of the
/// list its entry is in, so the one before it is the list it passes the wheel on to.
fn lists_id() -> egui::Id {
    egui::Id::unique("fit-lists-open")
}

/// Where the wheel passed on to the list `list` waits: written by a sub-menu of that list in
/// this frame, taken by the list at the end of its own frame. A delta nobody took (its list
/// closed) is cleared when a list of that name is next drawn.
fn wheel_id(list: egui::Id) -> egui::Id {
    list.with("fit-wheel-passed-on")
}

fn scroll_in<R>(ui: &mut egui::Ui, max: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let frame = egui::Frame::menu(ui.style()).total_margin().sum().x;
    let wide = (ui.ctx().content_rect().width() - frame - 2.0 * EDGE).max(LEAST);
    visible_bars(ui);
    // Sideways only for a list wider than the window (its last frame says so): a scroll area
    // clips at its edge, which a list that fits has no use for.
    let key = ui.scope_id().with("fit-scroll-width");
    let last: f32 = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0.0);
    let me = ui.scope_id().with("fit-list");
    let parent = ui.ctx().data_mut(|d| {
        // (Nothing waits for this list from before it was drawn.)
        d.remove::<egui::Vec2>(wheel_id(me));
        let open: &mut Vec<egui::Id> = d.get_temp_mut_or_default(lists_id());
        let parent = open.last().copied();
        open.push(me);
        parent
    });
    // A list opens at its top: the toolkit keeps a scroll area's place by its name, so a menu
    // scrolled and closed would open scrolled the next time.
    let frame = ui.ctx().cumulative_frame_nr();
    let drawn_at = me.with("drawn-at");
    let before: Option<u64> = ui.ctx().data(|d| d.get_temp(drawn_at));
    ui.ctx().data_mut(|d| d.insert_temp(drawn_at, frame));
    let mut area = egui::ScrollArea::new([last > wide + 1.0, true])
        .id_salt("fit-scroll")
        .max_height(max)
        .max_width(wide)
        .min_scrolled_height(max)
        .min_scrolled_width(wide)
        .auto_shrink([true, true]);
    if before.is_none_or(|f| frame > f + 1) {
        area = area.vertical_scroll_offset(0.0);
    }
    let out = area
        .show(ui, |ui| {
            let inner = add(ui);
            follow_focus(ui);
            // The wheel turned over a sub-menu of this list that had nothing to scroll.
            let passed = ui.ctx().data_mut(|d| d.remove_temp::<egui::Vec2>(wheel_id(me)));
            if let Some(delta) = passed {
                ui.scroll_with_delta(delta);
            }
            inner
        });
    ui.ctx().data_mut(|d| {
        let open: &mut Vec<egui::Id> = d.get_temp_mut_or_default(lists_id());
        open.retain(|id| *id != me);
    });
    // In a narrow window a sub-menu opens over the list it belongs to, under the resting
    // pointer, and takes the wheel: when it has nothing (more) to scroll that way, the wheel
    // goes on to the list under it (read where that list finishes its frame, above).
    if let Some(parent) = parent.filter(|_| ui.rect_contains_pointer(out.inner_rect)) {
        let delta = ui.input(|i| i.smooth_scroll_delta);
        let offset = out.state.clamped_offset().y;
        let end = (out.content_size.y - out.inner_rect.height()).max(0.0);
        let stuck = (delta.y > 0.0 && offset <= 0.5) || (delta.y < 0.0 && offset >= end - 0.5);
        if stuck {
            ui.ctx()
                .data_mut(|d| d.insert_temp(wheel_id(parent), egui::vec2(0.0, delta.y)));
        }
    }
    note(
        ui.ctx(),
        Surface {
            kind: "menu",
            // (A pop-up's first frame is as high as it may get; its content says how high
            // it is.)
            rect: egui::Rect::from_min_size(
                out.inner_rect.min,
                egui::vec2(out.inner_rect.width(), out.inner_rect.height().min(out.content_size.y)),
            ),
            content: out.content_size.y,
            offset: out.state.clamped_offset().y,
            buttons: None,
        },
    );
    if (out.content_size.x - last).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(key, out.content_size.x));
        ui.ctx().request_repaint();
    }
    out.inner
}

/// Shows `popup`, opened from `button`, with its content scrolling in the room beside the
/// button.
pub fn popup<R>(
    popup: egui::Popup<'_>,
    button: &egui::Response,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<R>> {
    let max = room_beside(&button.ctx, button.rect);
    popup.show(|ui| scroll_in(ui, max, add))
}

/// The menu of a button: opens on its click, below it or where there is room.
pub fn menu<R>(
    button: &egui::Response,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<R>> {
    popup(egui::Popup::menu(button), button, add)
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

/// Shows a combo box: its list is the toolkit's own (it scrolls by itself), held to the
/// window: no higher than the window leaves room for, as wide as the window at most, an entry
/// too long for that wrapping. Every combo box of the app is shown through this.
pub fn combo<R>(
    combo: egui::ComboBox,
    ui: &mut egui::Ui,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let height = room(ui.ctx()).min(ui.spacing().combo_height);
    combo.height(height).show_ui(ui, |ui| {
        combo_list(ui);
        add(ui)
    })
}

fn combo_list(ui: &mut egui::Ui) {
    let frame = egui::Frame::menu(ui.style()).total_margin().sum().x;
    let wide = (ui.ctx().content_rect().width() - frame - 2.0 * EDGE).max(LEAST);
    ui.set_max_width(wide);
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
}

/// A colour button: a click opens the toolkit's colour picker in a pop-up that scrolls in a
/// window too small for it (the toolkit's own colour button opens one that cannot). Every
/// colour button of the app is this one.
pub fn color_button(ui: &mut egui::Ui, rgb: &mut [u8; 3]) -> egui::Response {
    let popup_id = ui.auto_id_with("fit-colour");
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let size = ui.spacing().interact_size;
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    let [r, g, b] = *rgb;
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::Role::ColorWell, ui.is_enabled(), "Color");
        info.current_text_value = Some(format!("#{r:02x}{g:02x}{b:02x}"));
        info
    });
    if ui.is_rect_visible(rect) {
        // Drawn as the toolkit draws its own: the colour inside a one-point frame.
        let visuals = if open {
            ui.visuals().widgets.open
        } else {
            *ui.style().interact(&response)
        };
        let rect = rect.expand(visuals.expansion);
        ui.painter().rect_filled(
            rect.shrink(1.0),
            visuals.corner_radius,
            egui::Color32::from_rgb(r, g, b),
        );
        ui.painter().rect_stroke(
            rect,
            visuals.corner_radius,
            (1.0, visuals.bg_fill),
            egui::StrokeKind::Inside,
        );
    }
    let mut color = egui::Color32::from_rgb(r, g, b);
    let mut changed = false;
    popup(
        egui::Popup::menu(&response)
            .id(popup_id)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
        &response,
        |ui| {
            ui.spacing_mut().slider_width = 275.0;
            changed = egui::color_picker::color_picker_color32(
                ui,
                &mut color,
                egui::color_picker::Alpha::Opaque,
            );
        },
    );
    if changed {
        *rgb = [color.r(), color.g(), color.b()];
        response.mark_changed();
    }
    response
}

/// Scroll bars that show while at rest, thin and over the content, so a list that goes on is
/// seen to go on.
pub fn visible_bars(ui: &mut egui::Ui) {
    let mut bars = egui::style::ScrollStyle::floating();
    bars.dormant_handle_opacity = 0.6;
    bars.dormant_background_opacity = 0.15;
    bars.floating_width = 4.0;
    ui.spacing_mut().scroll = bars;
}

/// A menu, pop-up or dialog drawn in a frame, as `ui.state` reports it (`surfaces`).
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    /// `menu` (a menu, list or pop-up) or `dialog`.
    pub kind: &'static str,
    /// The part of the window its content is seen through.
    pub rect: egui::Rect,
    /// The height of the content; more than the height of `rect` when it scrolls.
    pub content: f32,
    /// How far the content is scrolled down.
    pub offset: f32,
    /// The dialog's button row, when it is kept in view below the scrolled content.
    pub buttons: Option<egui::Rect>,
}

/// What one frame drew, for `ui.state`: its menus, pop-ups and dialogs, and where the named
/// buttons that open one are.
#[derive(Clone, Default)]
pub struct Drawn {
    frame: u64,
    /// The window's size in points.
    pub window: egui::Vec2,
    pub list: Vec<Surface>,
    pub anchors: Vec<(&'static str, egui::Rect)>,
}

fn drawn_mut(ctx: &egui::Context, change: impl FnOnce(&mut Drawn)) {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| {
        let drawn: &mut Drawn = d.get_temp_mut_or_default(drawn_id());
        if drawn.frame != frame {
            *drawn = Drawn {
                frame,
                ..Drawn::default()
            };
        }
        change(drawn);
    });
}

/// Notes where the button called `name` is in this frame (the top bar's Options).
pub fn anchor(ctx: &egui::Context, name: &'static str, rect: egui::Rect) {
    drawn_mut(ctx, |d| d.anchors.push((name, rect)));
}

fn drawn_id() -> egui::Id {
    egui::Id::unique("fit-surfaces")
}

fn note(ctx: &egui::Context, surface: Surface) {
    drawn_mut(ctx, |d| d.list.push(surface));
}

/// What this frame has drawn so far (call at the end of the frame).
pub fn drawn(ctx: &egui::Context) -> Drawn {
    let frame = ctx.cumulative_frame_nr();
    let mut drawn = ctx
        .data(|d| d.get_temp::<Drawn>(drawn_id()))
        .filter(|d| d.frame == frame)
        .unwrap_or_default();
    drawn.window = ctx.content_rect().size();
    drawn
}

/// Brings the control that has just taken the keyboard focus through the keyboard (Tab, the
/// arrow keys) into view, when it is inside the scrolled content `ui`. The toolkit moves the
/// focus without scrolling. A focus that moved without a key (an entry under the resting
/// pointer opening its sub-menu) is left alone: following it would turn the list back under
/// the wheel.
fn follow_focus(ui: &egui::Ui) {
    let ctx = ui.ctx();
    let key = ui.scope_id().with("fit-focus");
    let frame = ctx.cumulative_frame_nr();
    let keys_id = egui::Id::unique("fit-focus-keys");
    let pressed = ctx.input(|i| {
        use egui::Key::{ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Tab};
        [Tab, ArrowDown, ArrowUp, ArrowLeft, ArrowRight]
            .iter()
            .any(|k| i.key_pressed(*k))
    });
    if pressed {
        ctx.data_mut(|d| d.insert_temp(keys_id, frame));
    }
    let by_key = ctx
        .data(|d| d.get_temp::<u64>(keys_id))
        .is_some_and(|at| frame <= at + 2);
    let now = ctx.memory(|m| m.focused());
    let last = ctx.data(|d| d.get_temp::<Option<egui::Id>>(key)).flatten();
    if now == last {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(key, now));
    if !by_key {
        return;
    }
    let Some(id) = now else { return };
    let inside = ui.unique_id();
    let rect = ctx.viewport(|v| {
        let all = &v.this_pass.widgets;
        let found = all.get(id)?;
        let mut up = found.parent_id;
        for _ in 0..40 {
            if up == inside {
                return Some(found.rect);
            }
            let parent = all.get(up)?;
            if parent.parent_id == up {
                break;
            }
            up = parent.parent_id;
        }
        None
    });
    if let Some(rect) = rect {
        ui.scroll_to_rect(rect.expand(2.0), None);
    }
}

/// Two measured lengths that are the same for the layout: fractions of a point come and go
/// with rounding, and a layout that asked for another frame over them would never rest.
fn same(a: f32, b: f32) -> bool {
    a == b || (a - b).abs() <= 0.5
}

/// What a dialog measured in its last frame.
#[derive(Clone, Copy, Default)]
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
    let most = most_of(ctx);
    held(window, ctx)
        // (And it does not fold up around a list that would take any size.)
        .min_width(most.x.min(200.0))
        .min_height(most.y.min(140.0))
        .show(ctx, |ui| {
            raise_when_opened(ui);
            visible_bars(ui);
            add(ui)
        })
}

/// The most a window of the app may take of the app's window.
fn most_of(ctx: &egui::Context) -> egui::Vec2 {
    let most = ctx.content_rect().size() - egui::vec2(2.0 * EDGE, 2.0 * EDGE + 32.0);
    most.max(egui::vec2(LEAST, LEAST))
}

/// (A window that remembers a size larger than the app's window is held to that.)
fn held<'a>(window: egui::Window<'a>, ctx: &egui::Context) -> egui::Window<'a> {
    let most = most_of(ctx);
    window.max_width(most.x).max_height(most.y)
}

/// A pane shown as a window (floating, or folded to its title bar in the narrow layout): as
/// [`show_filling`], without a least size, which a folded window would keep around its title
/// bar.
pub fn show_pane<R>(
    window: egui::Window<'_>,
    ctx: &egui::Context,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<Option<R>>> {
    held(window, ctx).show(ctx, |ui| {
        visible_bars(ui);
        add(ui)
    })
}

/// As [`show_filling`], for a window with rows above its list (tabs, a header): in an app
/// window too small for `least` points of them and a few rows of the list, the window's
/// content keeps that size and scrolls.
pub fn show_filling_at_least<R>(
    window: egui::Window<'_>,
    ctx: &egui::Context,
    least: egui::Vec2,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<Option<R>>> {
    show_filling(window, ctx, |ui| page(ui, least, true, add))
}

/// A window that opens comes over what is on screen: the folded panes' title bars of the
/// narrow layout, which appear in the same first frames (drawn after it in the frame), would
/// otherwise lie over a dialog that opens with the app (Machine Setup, the Startup Wizard, the
/// update). Raised over its first few frames; after that its place among the windows is the
/// user's (a click on another window brings that one forward).
fn raise_when_opened(ui: &egui::Ui) {
    const FRAMES: u32 = 4;
    let ctx = ui.ctx();
    let layer = ui.layer_id();
    let key = ui.scope_id().with(("fit-opened", layer.id));
    let frame = ctx.cumulative_frame_nr();
    let (last, run): (u64, u32) = ctx.data(|d| d.get_temp(key)).unwrap_or((0, 0));
    let run = if run > 0 && frame == last + 1 { run + 1 } else { 1 };
    ctx.data_mut(|d| d.insert_temp(key, (frame, run)));
    if run <= FRAMES {
        ctx.move_to_top(layer);
    }
}

fn dialog<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    raise_when_opened(ui);
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
    let pin = last.buttons > 0.0
        && last.content + last.buttons > room + 0.5
        && room - last.buttons >= PIN_LEAST;
    let view = if pin { room - last.buttons } else { room };
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
        .show(ui, |ui| {
            let inner = add(ui);
            follow_focus(ui);
            inner
        });
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
    note(
        &ctx,
        Surface {
            kind: "dialog",
            rect: egui::Rect::from_min_size(
                out.inner_rect.min,
                egui::vec2(out.inner_rect.width(), out.inner_rect.height().min(out.content_size.y)),
            ),
            content: out.content_size.y,
            offset: out.state.clamped_offset().y,
            buttons: pinned.filter(|_| buttons > 0.0),
        },
    );
    let now = Sizes {
        content: out.content_size.y - if pin { 0.0 } else { buttons },
        buttons,
        width: out.inner_rect.width(),
        content_width: out.content_size.x,
    };
    let settled = same(now.content, last.content)
        && same(now.buttons, last.buttons)
        && same(now.width, last.width)
        && same(now.content_width, last.content_width);
    if !settled {
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
            // (A row wider than a narrow window scrolls sideways.)
            visible_bars(&mut row);
            let out = egui::ScrollArea::horizontal()
                .id_salt("fit-buttons-row")
                .auto_shrink([false, true])
                .show(&mut row, add);
            open.measured = row.min_rect().height() + gap;
            out.inner
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
#[derive(Clone, Copy, Default)]
struct PaneSizes {
    /// Its content was taller than the pane: it is drawn scrolling up and down.
    tall: bool,
    /// The height the content takes when it scrolls (its lists at their least height).
    needed: f32,
    /// The width its content took when that was more than the pane had, else 0.
    wide: f32,
    /// The width the pane had then.
    room: f32,
}

/// The content of a pane (docked or floating). While it fits it is laid out as it is; once the
/// pane is lower than its content, the whole content scrolls, so the rows below the edge can
/// be reached. Lists inside that fill the pane are made with [`list`].
pub fn pane<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    pane_in(ui, None, add)
}

/// As [`pane`], for a pane at the side that may be squeezed narrower than its content (panes
/// on both sides of a small window): the content is then laid out at the width it needs, at
/// least `width` points, and scrolls sideways. A pane that can grow to its content (the
/// toolkit widens it in the next frame) is left to.
pub fn pane_at_least<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    pane_in(ui, Some(width), add)
}

fn pane_in<R>(ui: &mut egui::Ui, width: Option<f32>, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let key = ui.scope_id().with("fit-pane");
    let last: PaneSizes = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    let room = ui.available_size();
    let down = last.tall && last.needed > room.y + 0.5;
    // Wider content than the pane, and the pane has not grown since it was measured.
    let stuck = last.wide > room.x + 0.5 && room.x <= last.room + 0.5;
    let lay = width.map(|w| if stuck { last.wide.max(w) } else { w });
    let sideways = lay.is_some_and(|w| w > room.x + 0.5);
    let (inner, now) = if down || sideways {
        visible_bars(ui);
        let out = egui::ScrollArea::new([sideways, down])
            .id_salt("fit-pane")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(w) = lay.filter(|_| sideways) {
                    ui.set_min_width(w);
                    ui.set_max_width(w);
                }
                let inner = add(ui);
                follow_focus(ui);
                inner
            });
        let tall = down || out.content_size.y > out.inner_rect.height() + 1.0;
        // The width the content took beyond what it was given (a row that does not wrap), or
        // the width it is kept at while it scrolls sideways: the pane leaves that when it
        // has the room.
        let given = if sideways { lay.unwrap_or(room.x) } else { room.x };
        let wide = if width.is_some() && out.content_size.x > given + 1.0 {
            out.content_size.x
        } else if sideways && stuck {
            last.wide
        } else {
            0.0
        };
        let now = PaneSizes {
            tall,
            needed: match (down, tall) {
                (true, _) => out.content_size.y,
                (false, true) => f32::INFINITY,
                (false, false) => 0.0,
            },
            wide,
            room: match (wide > 0.0, stuck) {
                (false, _) => 0.0,
                (true, true) => last.room.min(room.x),
                (true, false) => room.x,
            },
        };
        (out.inner, now)
    } else {
        let max = ui.max_rect();
        let inner = add(ui);
        let used = ui.min_rect();
        let tall = used.bottom() > max.bottom() + 1.0;
        let over = width.is_some() && used.right() > max.right() + 1.0;
        let now = PaneSizes {
            tall,
            needed: if tall { f32::INFINITY } else { 0.0 },
            wide: if over { used.right() - max.left() } else { 0.0 },
            room: if over { room.x } else { 0.0 },
        };
        (inner, now)
    };
    let settled = now.tall == last.tall
        && same(now.needed, last.needed)
        && same(now.wide, last.wide)
        && same(now.room, last.room);
    if !settled {
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

/// The whole content of a window or of a panel with bars of its own: laid out as it is while
/// there is `least` room for it (width, height; 0 for a side that takes any size), and at
/// that size in a page that scrolls once there is less, so the bars at its bottom and the
/// controls at its side can be reached. `wheel` is false while the pointer is over something
/// that takes the wheel and a drag for itself (the canvas): the page then scrolls by its bar
/// only.
pub fn page<R>(
    ui: &mut egui::Ui,
    least: egui::Vec2,
    wheel: bool,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let room = ui.available_rect_before_wrap();
    let scrolls = [room.width() + 0.5 < least.x, room.height() + 0.5 < least.y];
    if !scrolls[0] && !scrolls[1] {
        return add(ui);
    }
    visible_bars(ui);
    let source = egui::scroll_area::ScrollSource {
        mouse_wheel: wheel,
        drag: if wheel {
            egui::scroll_area::DragScroll::OnTouch
        } else {
            egui::scroll_area::DragScroll::Never
        },
        ..egui::scroll_area::ScrollSource::ALL
    };
    egui::ScrollArea::new(scrolls)
        .id_salt("fit-page")
        .auto_shrink([false, false])
        .scroll_source(source)
        .show(ui, |ui| {
            let rect = egui::Rect::from_min_size(ui.cursor().min, room.size().max(least));
            let mut body = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            let inner = add(&mut body);
            ui.allocate_rect(rect, egui::Sense::hover());
            inner
        })
        .inner
}

/// A row of controls that may be wider than its place (a status bar, a tab row, a toolbar):
/// drawn as it is while it fits, and scrolling sideways (wheel, touchpad, a thin bar under
/// it) once it does not, so the controls at its end can be reached.
pub fn row<R>(ui: &mut egui::Ui, salt: &'static str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let key = ui.scope_id().with(("fit-row", salt));
    let last: f32 = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0.0);
    let room = ui.available_width();
    let (inner, width) = if last > room + 1.0 {
        visible_bars(ui);
        let out = egui::ScrollArea::horizontal()
            .id_salt(("fit-row", salt))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                // Laid out for the room there is, as when it fits: what sizes itself by the
                // room left (a slider) keeps its size.
                ui.set_max_width(room);
                ui.horizontal(add).inner
            });
        (out.inner, out.content_size.x)
    } else {
        let r = ui.horizontal(add);
        (r.inner, r.response.rect.width())
    };
    if (width - last).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(key, width));
        ui.ctx().request_repaint();
    }
    inner
}

/// As [`row`], for a section laid out top to bottom (a form of labelled fields): drawn as it
/// is while it fits its place's width, and scrolling sideways once a row of it is wider.
pub fn block<R>(ui: &mut egui::Ui, salt: &'static str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let key = ui.scope_id().with(("fit-block", salt));
    let last: f32 = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0.0);
    let room = ui.available_width();
    let (inner, width) = if last > room + 1.0 {
        visible_bars(ui);
        let out = egui::ScrollArea::horizontal()
            .id_salt(("fit-block", salt))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                // Laid out for the room there is, as when it fits: a field that takes the
                // width left keeps that width; a row that needs more makes the section wider.
                ui.set_max_width(room);
                ui.vertical(add).inner
            });
        (out.inner, out.content_size.x)
    } else {
        let r = ui.vertical(add);
        (r.inner, r.response.rect.width())
    };
    if (width - last).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(key, width));
        ui.ctx().request_repaint();
    }
    inner
}

/// As [`row`], for a row laid out from the right edge (a status bar's controls).
pub fn row_from_right<R>(
    ui: &mut egui::Ui,
    salt: &'static str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let key = ui.scope_id().with(("fit-row-right", salt));
    let last: f32 = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0.0);
    let room = ui.available_width();
    let from_right = egui::Layout::right_to_left(egui::Align::Center);
    let (inner, width) = if last > room + 1.0 {
        visible_bars(ui);
        let height = ui.spacing().interact_size.y;
        let out = egui::ScrollArea::horizontal()
            .id_salt(("fit-row-right", salt))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(egui::vec2(last, height), from_right, add)
            });
        (out.inner.inner, out.inner.response.rect.width())
    } else {
        let r = ui.with_layout(from_right, add);
        (r.inner, r.response.rect.width())
    };
    if (width - last).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(key, width));
        ui.ctx().request_repaint();
    }
    inner
}
