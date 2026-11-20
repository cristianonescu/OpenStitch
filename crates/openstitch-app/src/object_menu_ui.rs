//! The object menu (#1288): a right-click with the Select tool on an object opens PE-DESIGN
//! 11.50's menu for the selection: Cut, Copy, Duplicate, Paste ▸, Delete; Flip ▸, Center; Open,
//! Close; Create Offset Lines, Convert to Stitches, Create Floral Pattern; Cut and Sew ▸, in its
//! order, with its separators and shortcuts ([`OBJECT_MENU`]). A right-click on an object that is
//! not selected selects it first; on empty canvas no menu opens. Each entry runs the ribbon's own
//! command, so what greys it out and what it does are the ribbon's.
//!
//! The Point tool keeps its own menus (`point_menu_ui`); this menu is drawn with the same panel
//! and rows.

use crate::app::EditorApp;
use crate::point_menu_ui::{menu_row, panel, rule};
use eframe::egui::{self, Pos2};
use openstitch_app::editor::Viewport;
use openstitch_app::editor::ribbon::{Action, MenuRow, object_menu};
use openstitch_app::editor::ui_texts::t;
use openstitch_design::v2;
use serde_json::{Value, json};

/// The open object menu.
pub struct ObjectMenu {
    at: Pos2,
    /// Opened this frame: the click that opened it does not close it.
    fresh: bool,
    /// The sub-menu that is open: the key of its row.
    sub: Option<&'static str>,
    /// Where the menu's panel was painted, and in which colour; `None` until a frame has drawn
    /// it (a menu's first frame only measures it).
    drawn: Option<(egui::Rect, egui::Color32)>,
    /// Where each row was painted last (window points), by key: the entries, the sub-menus'
    /// rows and, while one is open, a sub-menu's entries.
    rects: Vec<(&'static str, egui::Rect)>,
    /// Where the sew-type bar was painted last.
    bar: Option<egui::Rect>,
}

/// The shortcut shown beside an entry, as this platform writes it.
fn shortcut(a: Action) -> Option<String> {
    a.info().shortcut_for(cfg!(target_os = "macos"))
}

/// Whether PE-DESIGN's menu shows an icon in front of the entry: not on Paste and its
/// entries, nor on Open and Close.
pub fn has_icon(a: Action) -> bool {
    !matches!(
        a,
        Action::Paste | Action::PasteAttributes | Action::OpenOutline | Action::CloseOutline
    )
}

/// The entry's text: the command's label, PE-DESIGN's words.
fn label(a: Action) -> &'static str {
    a.info().label
}

/// A right-click on the canvas with the Select tool: on an object, it is selected when it is
/// not already (a selected one keeps the others selected too) and the menu opens at the
/// pointer. Returns whether the click was used.
pub fn pointer(app: &mut EditorApp, response: &egui::Response, vp: &Viewport) -> bool {
    if !response.secondary_clicked() {
        return false;
    }
    let Some(at) = response.interact_pointer_pos() else {
        return false;
    };
    open_at(app, at, vp)
}

/// Opens the menu as a right-click at `at` (window points) does; `false` on empty canvas or
/// with another tool than Select.
pub fn open_at(app: &mut EditorApp, at: Pos2, vp: &Viewport) -> bool {
    let model = vp.to_model(v2(f64::from(at.x), f64::from(at.y)));
    let ctx = app.tool_context(app.session.design());
    let Some(selection) = app.tools.object_menu_selection(model, &ctx) else {
        return false;
    };
    if selection != app.selection {
        app.apply_effects(vec![openstitch_app::editor::ToolEffect::Select(selection)]);
    }
    app.workspace.view.object_menu = Some(ObjectMenu {
        at,
        fresh: true,
        sub: None,
        drawn: None,
        rects: Vec::new(),
        bar: None,
    });
    true
}

/// Runs one entry on the selection, as a click on it does; nothing when it is greyed out.
pub fn run(app: &mut EditorApp, a: Action, ctx: &egui::Context) {
    if crate::ribbon::enabled(app, a) {
        crate::ribbon::run(app, a, ctx);
    }
}

/// PE-DESIGN's small bar beside the object menu (#1288): the line sew type and its colour, the
/// region sew type and its colour, each opening the ribbon's own list (Shapes ▸ Line Sew Type,
/// Line Color, Region Sew Type, Region Color). Above the menu for one object, below it for
/// several (the capture of two objects). A list empty when the selection has no such sewing or
/// mixed ones. Returns the bar's rectangle.
fn sew_bar(app: &mut EditorApp, ctx: &egui::Context, menu: egui::Rect, below: bool) -> egui::Rect {
    use openstitch_app::editor::inspector::{InspectorModel, line_types, region_types};
    let m = InspectorModel::new(app.session.design(), &app.selection);
    let line = m
        .line_kind
        .same()
        .and_then(|k| line_types().into_iter().find(|(t, _)| t == k))
        .map_or("", |(_, label)| openstitch_app::editor::ui_texts::tr(label));
    let region = m
        .region_kind
        .same()
        .and_then(|k| region_types().into_iter().find(|(t, _)| t == k))
        .map_or("", |(_, label)| openstitch_app::editor::ui_texts::tr(label));
    let colour = |t: Option<&openstitch_core::Thread>| {
        t.map(|t| egui::Color32::from_rgb(t.red, t.green, t.blue))
    };
    // A colour shows only when every selected object has it; with several colours the swatch
    // is the pane's Mixed one (grey), and the button still opens the list to set one for all.
    let line_colour = colour(
        m.line
            .as_ref()
            .filter(|_| crate::panels::all_same_line_thread(app, &m))
            .map(|l| &l.thread),
    );
    let region_colour = colour(
        m.region
            .as_ref()
            .filter(|_| crate::panels::all_same_region_thread(app, &m))
            .map(|r| &r.thread),
    );
    let (has_line, has_region) = (m.line.is_some(), m.region.is_some());
    let height = 30.0;
    let at = if below {
        menu.left_bottom() + egui::vec2(0.0, 4.0)
    } else {
        menu.left_top() - egui::vec2(0.0, height + 4.0)
    };
    let r = egui::Area::new(egui::Id::unique("object-menu-sew-bar"))
        .fixed_pos(at)
        .fade_in(false)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (a, text, swatch, ca, has) in [
                        (Action::LineSewType, line, line_colour, Action::LineColor, has_line),
                        (
                            Action::RegionSewType,
                            region,
                            region_colour,
                            Action::RegionColor,
                            has_region,
                        ),
                    ] {
                        let on = crate::ribbon::enabled(app, a);
                        ui.add_enabled_ui(on, |ui| {
                            ui.menu_button(format!("{text} \u{25be}"), |ui| {
                                crate::ribbon::menu(app, a, ui)
                            })
                            .response
                            .on_hover_text(a.info().label);
                        });
                        let fill = swatch.unwrap_or(ui.visuals().extreme_bg_color);
                        let button = egui::Button::new("    ").fill(fill);
                        ui.add_enabled_ui(on && has, |ui| {
                            egui::containers::menu::MenuButton::from_button(button)
                                .ui(ui, |ui| crate::ribbon::menu(app, ca, ui))
                                .0
                                .on_hover_text(ca.info().label);
                        });
                    }
                });
            });
        });
    r.response.rect
}

/// Width of the menu and of its sub-menus.
const WIDTH: f32 = 230.0;
const SUB_WIDTH: f32 = 210.0;

/// The menu, until an entry is chosen, Esc or a click elsewhere. A sub-menu opens beside its
/// row while the pointer is on the row.
pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    let Some(mut m) = app.workspace.view.object_menu.take() else {
        return;
    };
    let mut chosen = None;
    let mut sub_at = None;
    let mut rects = Vec::new();
    let rows = object_menu(app.selection.len());
    let (r, painted) = panel(ctx, "object-menu", m.at, WIDTH, |ui| {
        for row in rows {
            match *row {
                MenuRow::Separator => rule(ui),
                MenuRow::Entry(a) => {
                    let keys = shortcut(a);
                    let response = menu_row(
                        ui,
                        has_icon(a).then_some(a),
                        label(a),
                        keys.as_deref(),
                        crate::ribbon::enabled(app, a),
                        false,
                    );
                    rects.push((a.info().key, response.rect));
                    if response.hovered() {
                        m.sub = None;
                    }
                    if response.clicked() {
                        chosen = Some(a);
                    }
                }
                MenuRow::Sub(key, text, entries) => {
                    let enabled = entries.iter().any(|a| crate::ribbon::enabled(app, *a));
                    let response = menu_row(ui, None, t(text), None, enabled, true);
                    rects.push((key, response.rect));
                    if enabled && (response.hovered() || response.clicked()) {
                        m.sub = Some(key);
                    }
                    if m.sub == Some(key) {
                        sub_at = Some((
                            entries,
                            response.rect.right_top() + egui::vec2(2.0, -4.0),
                        ));
                    }
                }
            }
        }
    });
    // The sew-type bar, once the menu has been measured.
    let bar = m
        .drawn
        .or(painted)
        .map(|(menu, _)| sew_bar(app, ctx, menu, app.selection.len() >= 2));
    let mut over_sub = false;
    if let Some((entries, at)) = sub_at {
        let (sub, _) = panel(ctx, "object-submenu", at, SUB_WIDTH, |ui| {
            for a in entries {
                let keys = shortcut(*a);
                let icon = has_icon(*a).then_some(*a);
                let enabled = crate::ribbon::enabled(app, *a);
                let response = menu_row(ui, icon, label(*a), keys.as_deref(), enabled, false);
                rects.push((a.info().key, response.rect));
                if response.clicked() {
                    chosen = Some(*a);
                }
            }
        });
        over_sub = sub.response.contains_pointer();
    }
    if let Some(a) = chosen {
        run(app, a, ctx);
        return;
    }
    m.drawn = painted.or(m.drawn);
    if painted.is_some() {
        m.rects = rects;
    }
    m.bar = bar.or(m.bar);
    let (pressed, esc) =
        ctx.input(|i| (i.pointer.any_pressed(), i.key_pressed(egui::Key::Escape)));
    let over_bar = bar.is_some_and(|b| {
        ctx.input(|i| i.pointer.interact_pos())
            .is_some_and(|p| b.contains(p))
    });
    // A list opened from the bar is a pop-up of its own: a click in it is not outside.
    let outside = pressed
        && !r.response.contains_pointer()
        && !over_sub
        && !over_bar
        && !egui::Popup::is_any_open(ctx);
    if !esc && (m.fresh || !outside) {
        m.fresh = false;
        app.workspace.view.object_menu = Some(m);
    }
}

/// Where the row `key` was painted last, [x, y, width, height] in window points.
fn rect_of(app: &EditorApp, key: &str) -> Option<[f32; 4]> {
    let m = app.workspace.view.object_menu.as_ref()?;
    m.rects
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, r)| [r.min.x, r.min.y, r.width(), r.height()])
}

/// One entry as `ui.objectMenu` reports it.
fn entry(app: &EditorApp, a: Action) -> Value {
    json!({
        "rect": rect_of(app, a.info().key),
        "key": a.info().key,
        "label": label(a),
        "shortcut": shortcut(a),
        "icon": has_icon(a),
        "enabled": crate::ribbon::enabled(app, a),
    })
}

/// The menu as `ui.objectMenu` reports it: whether it is open, where it was painted and in
/// which colour (null until a frame has painted it), the open sub-menu, and every row in
/// order: an entry (key, text, shortcut, icon, whether it is available for the selection), a
/// sub-menu (key, text, its entries) or a separator.
pub fn state(app: &EditorApp) -> Value {
    let open = app.workspace.view.object_menu.as_ref();
    let drawn = open.and_then(|m| m.drawn);
    let rows: Vec<Value> = object_menu(app.selection.len())
        .iter()
        .map(|row| match *row {
            MenuRow::Separator => json!({ "separator": true }),
            MenuRow::Entry(a) => entry(app, a),
            MenuRow::Sub(key, text, entries) => json!({
                "rect": rect_of(app, key),
                "key": key,
                "label": t(text),
                "submenu": entries.iter().map(|a| entry(app, *a)).collect::<Vec<_>>(),
            }),
        })
        .collect();
    json!({
        "open": open.is_some(),
        "at": open.map(|m| [m.at.x, m.at.y]),
        "submenuOpen": open.and_then(|m| m.sub),
        "menuRect": drawn.map(|(r, _)| [r.min.x, r.min.y, r.width(), r.height()]),
        "menuFill": drawn.map(|(_, c)| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())),
        "selection": app.selection,
        "selectionRect": selection_rect(app),
        "dialog": crate::numeric_ui::state(app),
        "bar": bar_state(app),
        "rows": rows,
    })
}

/// The sew-type bar as `ui.objectMenu` reports it: the line and region sew types and colours of
/// the selection (null when it has none or mixed ones), whether it sits below the menu, and where
/// it was painted (null until then).
fn bar_state(app: &EditorApp) -> Value {
    use openstitch_app::editor::inspector::{InspectorModel, line_name, region_name};
    let m = InspectorModel::new(app.session.design(), &app.selection);
    let hex = |t: &openstitch_core::Thread| format!("#{:02x}{:02x}{:02x}", t.red, t.green, t.blue);
    let line_same = crate::panels::all_same_line_thread(app, &m);
    let region_same = crate::panels::all_same_region_thread(app, &m);
    json!({
        "line": m.line_kind.same().map(|k| line_name(*k)),
        "lineColor": m.line.as_ref().filter(|_| line_same).map(|l| hex(&l.thread)),
        "lineColorMixed": m.line.is_some() && !line_same,
        "region": m.region_kind.same().map(|k| region_name(*k)),
        "regionColor": m.region.as_ref().filter(|_| region_same).map(|r| hex(&r.thread)),
        "regionColorMixed": m.region.is_some() && !region_same,
        "below": app.selection.len() >= 2,
        "rect": app.workspace.view.object_menu.as_ref().and_then(|m| m.bar).map(|r| [r.min.x, r.min.y, r.width(), r.height()]),
    })
}

/// Where the selected objects are in the window, [x, y, width, height] in points from its
/// top-left corner (null with nothing selected or before the canvas is drawn): where a
/// right-click opens the menu for them.
fn selection_rect(app: &EditorApp) -> Option<[f64; 4]> {
    let vp = app.viewport?;
    let d = app.session.design();
    let b = openstitch_design::Rect::enclosing(
        d.objects
            .iter()
            .filter(|o| app.selection.contains(&o.id))
            .filter_map(|o| o.bounds())
            .flat_map(|b| [v2(b.min_x, b.min_y), v2(b.max_x, b.max_y)]),
    )?;
    let a = vp.to_screen(v2(b.min_x, b.min_y));
    let c = vp.to_screen(v2(b.max_x, b.max_y));
    Some([a.x.min(c.x), a.y.min(c.y), (c.x - a.x).abs(), (c.y - a.y).abs()])
}

/// `ui.objectMenu`: without params, the menu ([`state`]); `{at: [x, y]}` opens it as a
/// right-click there (window points) does, selecting the object under the pointer; `{action}`
/// runs an entry (a key of `rows` or of a sub-menu) on the selection as a click on it does,
/// and is refused while the entry is greyed out; `{close: true}` closes it.
pub fn command(app: &mut EditorApp, params: Option<&Value>) -> Result<Value, String> {
    if let Some(at) = params.and_then(|p| p.get("at")) {
        let xy = at
            .as_array()
            .filter(|a| a.len() == 2)
            .and_then(|a| Some((a[0].as_f64()?, a[1].as_f64()?)))
            .ok_or("at: must be [x, y] in window points")?;
        let vp = app.viewport.ok_or("the canvas has not been drawn yet")?;
        if !open_at(app, egui::pos2(xy.0 as f32, xy.1 as f32), &vp) {
            return Err(
                "at: no object there (the menu opens on an object, with the Select tool)".into(),
            );
        }
    }
    if let Some(n) = params.and_then(|p| p.get("numeric")) {
        crate::numeric_ui::command(app, n)?;
        return Ok(state(app));
    }
    if params
        .and_then(|p| p.get("close"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        app.workspace.view.object_menu = None;
    }
    let Some(key) = params.and_then(|p| p.get("action")).and_then(Value::as_str) else {
        return Ok(state(app));
    };
    let menu = openstitch_app::editor::ribbon::object_menu_actions();
    let a = menu
        .iter()
        .copied()
        .find(|a| a.info().key == key)
        .ok_or_else(|| {
            format!(
                "action: must be one of {}",
                menu.iter().map(|a| a.info().key).collect::<Vec<_>>().join(", ")
            )
        })?;
    if !crate::ribbon::enabled(app, a) {
        return Err(format!("{key} is not available for the selection"));
    }
    app.workspace.view.object_menu = None;
    let ctx = egui::Context::default();
    crate::ribbon::run(app, a, &ctx);
    Ok(state(app))
}
