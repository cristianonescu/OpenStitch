//! The Select Point tool's right-click menu on a point (#1258): To Straight, To Curve,
//! To Smooth, To Symmetry, To Cusp, Split at Point and Delete, in PE-DESIGN's order with its
//! separators. The entries, what greys them out and what they run are the tool's
//! (`ToolController::point_actions` / `point_action`), shared with the ribbon's Edit Point
//! group.

use crate::app::EditorApp;
use eframe::egui::{self, Pos2};
use openstitch_app::editor::Viewport;
use openstitch_app::editor::points::PointAction;
use openstitch_app::editor::ribbon::Action;
use openstitch_app::editor::ui_texts::t;
use openstitch_design::v2;
use serde_json::{Value, json};

/// An open point menu, or the object's menu.
pub struct PointMenu {
    at: Pos2,
    /// Opened this frame: the click that opened it does not close it.
    fresh: bool,
    /// The object's menu (a right-click on the shape, not on a point).
    object: bool,
    /// The object's menu: the Delete Direction Line sub-menu is open.
    sub: bool,
    /// Where the menu's panel was painted, and in which colour; `None` until a frame has
    /// drawn it (a menu's first frame only measures it, and paints nothing).
    drawn: Option<(egui::Rect, egui::Color32)>,
    /// The needle point menu (a right-click on a stitch pattern's needle point, #1272):
    /// PE-DESIGN's two entries, Split at Point and Delete.
    needle: bool,
}

impl PointMenu {
    /// Whether this is the needle point menu.
    pub fn is_needle(&self) -> bool {
        self.needle
    }
}

/// What an arrow key moves the selected points by, 0.1 mm units: 0.5 mm, PE-DESIGN 11.50's
/// step (ten presses moved a point exactly 5.0 mm, #1260 capture e2).
pub const POINT_NUDGE: f64 = 5.0;

/// Whether the one selected shape's outline is closed; `None` without exactly one unlocked
/// shape selected.
pub fn outline_closed(app: &EditorApp) -> Option<bool> {
    let ctx = app.tool_context(app.session.design());
    ctx.point_target()
        .and_then(|o| o.contours.first())
        .map(|c| c.closed)
}

/// Open or Close on the selected shape (`object.openOutline` / `object.closeOutline`).
pub fn open_or_close(app: &mut EditorApp, close: bool) {
    if outline_closed(app) != Some(!close) {
        return;
    }
    let command = if close {
        "object.closeOutline"
    } else {
        "object.openOutline"
    };
    app.on_selection(|s| {
        openstitch_app::editor::CommandRequest::new(
            command,
            json!({ "id": s.iter().next().copied() }),
        )
    });
}

/// Opens the point menu (`object`: the object's menu) at `at`, as a right-click there does;
/// for tests of what closes it.
#[cfg(test)]
pub(crate) fn open_for_test(app: &mut EditorApp, at: Pos2, object: bool) {
    app.workspace.view.point_menu = Some(PointMenu {
        at,
        fresh: false,
        object,
        sub: false,
        drawn: None,
        needle: false,
    });
}

/// The entry's text, PE-DESIGN's words.
pub fn label(a: PointAction) -> &'static str {
    t(match a {
        PointAction::ToStraight => "menu-point-to-straight",
        PointAction::ToCurve => "menu-point-to-curve",
        PointAction::ToSmooth => "menu-point-to-smooth",
        PointAction::ToSymmetry => "menu-point-to-symmetry",
        PointAction::ToCusp => "menu-point-to-cusp",
        PointAction::Split => "menu-point-split",
        PointAction::Delete => "menu-point-delete",
    })
}

/// The entry's place in [`PointAction::MENU`].
pub fn slot(a: PointAction) -> usize {
    PointAction::MENU
        .iter()
        .position(|m| *m == a)
        .unwrap_or_default()
}

/// The menu entry a ribbon command stands for.
pub fn action_of(a: Action) -> Option<PointAction> {
    Some(match a {
        Action::PointToStraight => PointAction::ToStraight,
        Action::PointToCurve => PointAction::ToCurve,
        Action::PointToSmooth => PointAction::ToSmooth,
        Action::PointToSymmetry => PointAction::ToSymmetry,
        Action::PointToCusp => PointAction::ToCusp,
        Action::SplitAtPoint => PointAction::Split,
        Action::DeletePoint => PointAction::Delete,
        _ => return None,
    })
}

/// The Point tool's hint line for what is selected of the outline whose points show
/// (`editor::hints::point_hint`); `None` with another tool or target.
pub fn hint(app: &EditorApp) -> Option<&'static str> {
    let ctx = app.tool_context(app.session.design());
    // On a stitch pattern (#1272): what its needle points can do.
    if let Some(o) = ctx.needle_target().filter(|_| app.tools.needle_overlay(&ctx).is_some()) {
        let selected = app.tools.selected_needles(&ctx);
        let end = selected.len() == 1
            && selected
                .first()
                .and_then(|&i| openstitch_app::editor::tools::needles::end_of(o, i))
                .is_some();
        return Some(openstitch_app::editor::hints::needle_hint(selected.len(), end));
    }
    app.tools
        .point_selection(&ctx)
        .map(openstitch_app::editor::hints::point_hint)
}

/// Which entries are available for the selected points, in menu order; `None` when the Point
/// tool has no point selected.
pub fn states(app: &EditorApp) -> Option<[bool; 7]> {
    let ctx = app.tool_context(app.session.design());
    app.tools.point_actions(&ctx)
}

/// Runs one entry on the selected points (one Undo); nothing when it is greyed out.
pub fn run(app: &mut EditorApp, a: PointAction) {
    app.with_tools(|tools, ctx| tools.point_action(a, ctx));
}

/// A right-click on a point with the Point tool selects it (a point that is already selected
/// keeps the others selected too) and opens the menu beside it. Returns whether the click was
/// used.
pub fn pointer(app: &mut EditorApp, response: &egui::Response, vp: &Viewport) -> bool {
    if !response.secondary_clicked() {
        return false;
    }
    let Some(at) = response.interact_pointer_pos() else {
        return false;
    };
    let model = vp.to_model(v2(f64::from(at.x), f64::from(at.y)));
    let (mut hit, mut object, mut needle) = (false, false, false);
    app.with_tools(|tools, ctx| {
        // A stitch pattern's needle point: its menu, Split at Point and Delete (#1272).
        needle = tools.select_needle_at(model, ctx);
        hit = !needle && tools.select_point_at(model, ctx);
        // Not on a point: the object's menu, as PE-DESIGN opens it.
        object = !hit && !needle && tools.object_menu_at(model, ctx);
        vec![]
    });
    app.workspace.view.point_menu = (hit || object || needle).then_some(PointMenu {
        at,
        fresh: true,
        object,
        sub: false,
        drawn: None,
        needle,
    });
    hit || object || needle
}

/// The icon in front of a point-menu entry: PE-DESIGN's menu has one on Split at Point and on
/// Delete (ours are the ribbon's own icons for those commands).
pub fn entry_icon(a: PointAction) -> Option<Action> {
    match a {
        PointAction::Split => Some(Action::SplitAtPoint),
        PointAction::Delete => Some(Action::DeletePoint),
        _ => None,
    }
}

/// The icon in front of an object-menu entry: Add Direction Line and Reverse Direction Line
/// have one, as in PE-DESIGN's menu; Delete Direction Line, Open and Close have none.
pub fn object_entry_icon(a: Action) -> Option<Action> {
    matches!(a, Action::AddDirectionLine | Action::ReverseDirectionLine).then_some(a)
}

/// Whether an entry of the object's menu is available. As its ribbon command is, except
/// Delete Direction Line: in PE-DESIGN's menu it is always available (it only opens the
/// sub-menu, whose entries are greyed on their own).
pub fn object_entry_enabled(app: &EditorApp, a: Action) -> bool {
    a == Action::DeleteDirectionLine || crate::ribbon::enabled(app, a)
}

/// The selected stored direction line (the shape has two or more, #1273): (shape, index).
pub fn selected_stored_line(app: &EditorApp) -> Option<(u64, usize)> {
    let ctx = app.tool_context(app.session.design());
    app.tools
        .selected_direction_line(&ctx)
        .filter(|l| l.stored)
        .map(|l| (l.id, l.index))
}

/// Add Direction Line: PE-DESIGN's two clicks with the Point tool on the selected fill.
pub fn start_direction_line(app: &mut EditorApp) {
    app.tools.set_tool(openstitch_app::editor::Tool::Point);
    let mut started = false;
    app.with_tools(|tools, ctx| {
        started = tools.start_direction_line(ctx);
        vec![]
    });
    if started {
        app.status = openstitch_app::editor::hints::point_hint(
            openstitch_app::editor::hints::PointSelection::AddingDirectionLine,
        )
        .into();
    }
}

/// The Delete Direction Line sub-menu, PE-DESIGN's two entries: (key, text id, available).
/// As captured (capture g): with one of two or more direction lines selected both are
/// available: Delete Selected Line removes that line, Delete Lines Not Selected every other
/// one; with the shape's only line selected, or none, both are grey. Kept from before for
/// what PE-DESIGN's shapes do not have: on a shape with a curved direction or a Manual Punch
/// shape's lines (none of which can be selected) Delete Lines Not Selected removes them
/// ([`delete_direction_lines`]).
pub fn direction_submenu(app: &EditorApp) -> [(&'static str, &'static str, bool); 2] {
    let selected = selected_stored_line(app).is_some();
    let ctx = app.tool_context(app.session.design());
    let other = ctx
        .point_target()
        .and_then(|o| o.region.as_ref())
        .is_some_and(|r| r.direction_curve.is_some() || !r.direction_lines.is_empty());
    [
        (
            "deleteSelectedLine",
            "menu-point-delete-selected-line",
            selected,
        ),
        (
            "deleteLinesNotSelected",
            "menu-point-delete-lines-not-selected",
            selected || other,
        ),
    ]
}

/// Runs an entry of the Delete Direction Line sub-menu (one undo step); nothing when it is
/// grey.
pub fn run_direction_entry(app: &mut EditorApp, key: &str) {
    if !direction_submenu(app).iter().any(|e| e.0 == key && e.2) {
        return;
    }
    match (key, selected_stored_line(app)) {
        ("deleteSelectedLine", Some(_)) => {
            app.with_tools(|tools, ctx| tools.delete_point(ctx).unwrap_or_default());
        }
        ("deleteLinesNotSelected", Some((id, index))) => {
            app.execute(&openstitch_app::editor::CommandRequest::new(
                "object.deleteDirectionLine",
                json!({ "id": id, "keep": index }),
            ));
            // The line that is kept is no longer selected (capture g).
            app.with_tools(|tools, ctx| {
                tools.select_direction_line(None, ctx);
                vec![]
            });
        }
        ("deleteLinesNotSelected", None) => delete_direction_lines(app),
        _ => {}
    }
}

/// Width of the icon column at the left of a menu row, in points.
const GUTTER: f32 = 24.0;
/// Height of a menu row.
const ROW: f32 = 22.0;

/// One row of a context menu, drawn as a menu row and not as a button: an icon column, the
/// text left-aligned after it, the whole row highlighted under the pointer, grey text when
/// the entry is unavailable, and a marker at the right edge when it opens a sub-menu.
fn row(
    ui: &mut egui::Ui,
    icon: Option<Action>,
    text: &str,
    enabled: bool,
    submenu: bool,
) -> egui::Response {
    let width = ui.available_width();
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, ROW), sense);
    if ui.is_rect_visible(rect) {
        let v = ui.visuals();
        let lit = enabled && response.hovered();
        if lit {
            ui.painter().rect_filled(rect, 3.0, v.selection.bg_fill);
        }
        // An unavailable entry is plainly grey, well apart from the text colour.
        let fg = if !enabled {
            v.text_color().gamma_multiply(0.38)
        } else if lit {
            v.selection.stroke.color
        } else {
            v.text_color()
        };
        if let Some(a) = icon {
            let r = egui::Rect::from_center_size(
                egui::pos2(rect.left() + GUTTER / 2.0, rect.center().y),
                egui::vec2(16.0, 16.0),
            );
            crate::icons::paint(ui.painter(), r, a, fg, enabled);
        }
        let font = egui::TextStyle::Button.resolve(ui.style());
        ui.painter().text(
            egui::pos2(rect.left() + GUTTER, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            font.clone(),
            fg,
        );
        if submenu {
            ui.painter().text(
                egui::pos2(rect.right() - 6.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                "\u{25b8}",
                font,
                fg,
            );
        }
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, text));
    response
}

/// Delete Lines Not Selected: removes every direction line of the selected shape, straight
/// and curved, in one undo step: what the ribbon's Delete Direction Line does
/// ([`crate::edit_ui::delete_direction`]).
pub fn delete_direction_lines(app: &mut EditorApp) {
    crate::edit_ui::delete_direction(app);
}

/// A menu panel at `at` (the pointer: the menu opens to the right of and below it, as
/// PE-DESIGN's), opaque from its first frame.
fn panel<R>(
    ctx: &egui::Context,
    id: &'static str,
    at: Pos2,
    width: f32,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> (egui::InnerResponse<R>, Option<(egui::Rect, egui::Color32)>) {
    let mut painted = None;
    let r = egui::Area::new(egui::Id::unique(id))
        .fixed_pos(at)
        .fade_in(false)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            // An area's first frame is a sizing pass: laid out, not painted.
            let visible = !ui.is_sizing_pass();
            let frame = egui::Frame::menu(ui.style());
            let fill = frame.fill;
            let shown = frame.show(ui, |ui| {
                ui.set_min_width(width);
                ui.set_max_width(width);
                ui.spacing_mut().item_spacing.y = 0.0;
                // Taller than the window (a small window, a large Text size): it scrolls.
                crate::fit::scroll(ui, |ui| {
                    ui.set_min_width(width);
                    body(ui)
                })
            });
            if visible {
                painted = Some((shown.response.rect, fill));
            }
            shown.inner
        });
    if painted.is_none() {
        // The measuring frame painted nothing: draw the menu in the next one, at once.
        ctx.request_repaint();
    }
    (r, painted)
}

/// A separator between groups of rows, with a little air round it.
fn rule(ui: &mut egui::Ui) {
    ui.add_space(3.0);
    ui.separator();
    ui.add_space(3.0);
}

/// The menu, until an entry is chosen, Esc or a click elsewhere.
pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    let Some(mut m) = app.workspace.view.point_menu.take() else {
        return;
    };
    if m.object {
        object_window(app, ctx, m);
        return;
    }
    if m.needle {
        needle_window(app, ctx, m);
        return;
    }
    let Some(enabled) = states(app) else {
        return;
    };
    let mut chosen = None;
    let (r, painted) = panel(ctx, "point-context-menu", m.at, 170.0, |ui| {
        for (i, a) in PointAction::MENU.into_iter().enumerate() {
            if a.separator_before() {
                rule(ui);
            }
            if row(ui, entry_icon(a), label(a), enabled[i], false).clicked() {
                chosen = Some(a);
            }
        }
    });
    if let Some(a) = chosen {
        run(app, a);
        return;
    }
    m.drawn = painted.or(m.drawn);
    let (pressed, esc) = ctx.input(|i| (i.pointer.any_pressed(), i.key_pressed(egui::Key::Escape)));
    let outside = pressed && !r.response.contains_pointer();
    if !esc && (m.fresh || !outside) {
        m.fresh = false;
        app.workspace.view.point_menu = Some(m);
    }
}

/// The needle point menu (#1272): PE-DESIGN's two entries, Split at Point and Delete, with
/// their icons (capture k12: neither is grey on the entry or exit point).
fn needle_window(app: &mut EditorApp, ctx: &egui::Context, mut m: PointMenu) {
    let entries = crate::stitch_points_ui::needle_menu(app);
    let mut chosen = None;
    let (r, painted) = panel(ctx, "needle-point-menu", m.at, 170.0, |ui| {
        for (key, text, enabled) in entries {
            let icon = if key == "splitAtPoint" {
                entry_icon(PointAction::Split)
            } else {
                entry_icon(PointAction::Delete)
            };
            if row(ui, icon, text, enabled, false).clicked() {
                chosen = Some(key);
            }
        }
    });
    if let Some(key) = chosen {
        let _ = crate::stitch_points_ui::menu_entry(app, key);
        return;
    }
    m.drawn = painted.or(m.drawn);
    let (pressed, esc) = ctx.input(|i| (i.pointer.any_pressed(), i.key_pressed(egui::Key::Escape)));
    let outside = pressed && !r.response.contains_pointer();
    if !esc && (m.fresh || !outside) {
        m.fresh = false;
        app.workspace.view.point_menu = Some(m);
    }
}

/// The object's menu: Add, Delete and Reverse Direction Line, a separator, Open and Close,
/// each greyed as its ribbon command is (an open line has no direction lines and offers
/// Close; a closed shape offers Open). Delete Direction Line opens a sub-menu, as
/// PE-DESIGN's does.
fn object_window(app: &mut EditorApp, ctx: &egui::Context, mut m: PointMenu) {
    use openstitch_app::editor::ribbon::POINT_OBJECT_MENU;
    let mut chosen = None;
    let mut delete_lines: Option<&'static str> = None;
    let mut sub_at = None;
    let (r, painted) = panel(ctx, "point-object-menu", m.at, 210.0, |ui| {
        for a in POINT_OBJECT_MENU {
            if a == Action::OpenOutline {
                rule(ui);
            }
            let enabled = object_entry_enabled(app, a);
            let submenu = a == Action::DeleteDirectionLine;
            let response = row(ui, object_entry_icon(a), a.info().label, enabled, submenu);
            if submenu {
                if enabled && (response.hovered() || response.clicked()) {
                    m.sub = true;
                }
                if m.sub {
                    sub_at = Some(response.rect.right_top() + egui::vec2(2.0, -4.0));
                }
            } else {
                if response.hovered() {
                    m.sub = false;
                }
                if response.clicked() {
                    chosen = Some(a);
                }
            }
        }
    });
    // The sub-menu, PE-DESIGN's two entries (see [`direction_submenu`]).
    let mut over_sub = false;
    if let Some(at) = sub_at {
        let entries = direction_submenu(app);
        let (sub, _) = panel(ctx, "point-object-submenu", at, 200.0, |ui| {
            for (key, text, enabled) in entries {
                if row(ui, None, t(text), enabled, false).clicked() {
                    delete_lines = Some(key);
                }
            }
        });
        over_sub = sub.response.contains_pointer();
    }
    if let Some(key) = delete_lines {
        run_direction_entry(app, key);
        return;
    }
    if let Some(a) = chosen {
        crate::ribbon::run(app, a, ctx);
        return;
    }
    m.drawn = painted.or(m.drawn);
    let (pressed, esc) = ctx.input(|i| (i.pointer.any_pressed(), i.key_pressed(egui::Key::Escape)));
    let outside = pressed && !r.response.contains_pointer() && !over_sub;
    if !esc && (m.fresh || !outside) {
        m.fresh = false;
        app.workspace.view.point_menu = Some(m);
    }
}

/// The object's menu as `ui.pointMenu` reports it: each entry's key, text, whether a
/// separator sits above it and whether it is available for the selected shape.
fn object_state(app: &EditorApp) -> Value {
    use openstitch_app::editor::ribbon::POINT_OBJECT_MENU;
    json!(
        POINT_OBJECT_MENU
            .into_iter()
            .map(|a| json!({
                "key": a.info().key,
                "label": a.info().label,
                "separatorBefore": a == Action::OpenOutline,
                "enabled": object_entry_enabled(app, a),
                "icon": object_entry_icon(a).is_some(),
                "submenu": (a == Action::DeleteDirectionLine).then(|| {
                    direction_submenu(app)
                        .into_iter()
                        .map(|(key, text, enabled)| json!({ "key": key, "label": t(text), "enabled": enabled }))
                        .collect::<Vec<_>>()
                }),
            }))
            .collect::<Vec<_>>()
    )
}

/// The menu as `ui.state` reports it: whether it is open, and every entry with its text,
/// whether a separator sits above it and whether it is available for the selected points.
pub fn state(app: &EditorApp) -> Value {
    let enabled = states(app);
    // The selected points: index, position on the Design Page in mm, and where they are in
    // the window (points from its top-left corner), so a script can click one.
    let design = app.session.design();
    let ctx = app.tool_context(design);
    let selected: Vec<Value> = app
        .tools
        .selected_points(&ctx)
        .iter()
        .filter_map(|p| {
            let at = *ctx.point_contour(*p)?.points.get(p.index)?;
            let screen = app.viewport.map(|vp| vp.to_screen(at));
            Some(json!({
                "id": p.id,
                "contour": p.contour,
                "index": p.index,
                "x": at.x / 10.0,
                "y": at.y / 10.0,
                "screen": screen.map(|s| [s.x, s.y]),
            }))
        })
        .collect();
    // The points that show: of every outline of every shape the Point tool edits.
    let targets = ctx.point_targets();
    let point_count = (!targets.is_empty()).then(|| {
        targets
            .iter()
            .flat_map(|o| &o.contours)
            .map(|c| c.points.len())
            .sum::<usize>()
    });
    // Where the open menu (the point's or the object's) was painted, in window points, and
    // its panel colour; null while no menu is open or its first frame has not painted it.
    let drawn = app
        .workspace
        .view
        .point_menu
        .as_ref()
        .and_then(|m| m.drawn);
    // The end the Alt-dragged end point lies on now (the red square), mm and window.
    let overlap = app.tools.connect_overlap().map(|at| {
        let screen = app.viewport.map(|vp| vp.to_screen(at));
        json!({ "x": at.x / 10.0, "y": at.y / 10.0, "screen": screen.map(|s| [s.x, s.y]) })
    });
    // The ends the one selected end point can connect to (Alt + drag; the x marks).
    let connectable: Vec<Value> = match app.tools.selected_points(&ctx)[..] {
        [p] => app
            .tools
            .connect_ends(p, &ctx)
            .into_iter()
            .map(|(id, index, at)| {
                let screen = app.viewport.map(|vp| vp.to_screen(at));
                json!({
                    "id": id,
                    "index": index,
                    "x": at.x / 10.0,
                    "y": at.y / 10.0,
                    "screen": screen.map(|s| [s.x, s.y]),
                })
            })
            .collect(),
        _ => vec![],
    };
    // The shape's direction lines as the Point tool shows them: its stored lines, or the one
    // that is its fill angle.
    let lines: Vec<Value> = app
        .tools
        .direction_lines(&ctx)
        .iter()
        .map(|l| {
            let screen = |p| app.viewport.map(|vp| vp.to_screen(p)).map(|s| [s.x, s.y]);
            json!({
                "index": l.index,
                "stored": l.stored,
                "angle": l.angle,
                "selected": l.selected,
                "from": [l.from.x / 10.0, l.from.y / 10.0],
                "to": [l.to.x / 10.0, l.to.y / 10.0],
                "screenFrom": screen(l.from),
                "screenTo": screen(l.to),
            })
        })
        .collect();
    // The selected line, else the first.
    let direction = lines
        .iter()
        .find(|l| l["selected"] == true)
        .or(lines.first())
        .cloned();
    let adding = app.tools.adding_direction_line().is_some();
    json!({
        "directionLine": direction,
        "directionLines": lines,
        "addingDirectionLine": adding,
        "connectOverlap": overlap,
        "connectable": connectable,
        "menuRect": drawn.map(|(r, _)| [r.min.x, r.min.y, r.width(), r.height()]),
        "menuFill": drawn.map(|(_, c)| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())),
        "hint": hint(app),
        "selected": selected,
        "pointCount": point_count,
        "open": app.workspace.view.point_menu.as_ref().is_some_and(|m| !m.object),
        "objectMenuOpen": app.workspace.view.point_menu.as_ref().is_some_and(|m| m.object),
        "objectMenu": object_state(app),
        "entries": PointAction::MENU.into_iter().enumerate().map(|(i, a)| json!({
            "key": a.key(),
            "label": label(a),
            "separatorBefore": a.separator_before(),
            "enabled": enabled.is_some_and(|e| e[i]),
            "icon": entry_icon(a).is_some(),
        })).collect::<Vec<_>>(),
    })
}

/// `ui.pointMenu`: `{ points, contour, id }` selects vertices with the Point tool (a point is
/// an index, or `{ id, contour, index }` to name points of several shapes or outlines);
/// `{ action }` runs one entry on the selected points, as a click in the menu does; without
/// `action` it reports the menu ([`state`]).
pub fn command(app: &mut EditorApp, params: Option<&Value>) -> Result<Value, String> {
    if let Some(list) = params.and_then(|p| p.get("points")) {
        const SHAPE: &str = "points: must be a list of vertex indexes, or of {id, contour, index}";
        let contour = params
            .and_then(|p| p.get("contour"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let id = params.and_then(|p| p.get("id")).and_then(Value::as_u64);
        // (object id when named, contour, index) per point.
        let mut named: Vec<(Option<u64>, usize, usize)> = Vec::new();
        for v in list.as_array().ok_or(SHAPE)? {
            if let Some(index) = v.as_u64() {
                named.push((id, contour, index as usize));
            } else if let Some(index) = v.get("index").and_then(Value::as_u64) {
                named.push((
                    v.get("id").and_then(Value::as_u64).or(id),
                    v.get("contour")
                        .and_then(Value::as_u64)
                        .map_or(contour, |c| c as usize),
                    index as usize,
                ));
            } else {
                return Err(SHAPE.into());
            }
        }
        app.tools.set_tool(openstitch_app::editor::Tool::Point);
        let mut ok = false;
        app.with_tools(|tools, ctx| {
            // A point without an id is on the one selected shape.
            let only = ctx.point_target().map(|o| o.id);
            let refs: Option<Vec<openstitch_app::editor::points::PointRef>> = named
                .iter()
                .map(|&(id, contour, index)| {
                    Some(openstitch_app::editor::points::PointRef {
                        id: id.or(only)?,
                        contour,
                        index,
                    })
                })
                .collect();
            ok = refs.is_some_and(|r| tools.select_point_refs(&r, ctx));
            vec![]
        });
        if !ok {
            return Err(
                "points: select the shape first (or several plain shapes: then each point names its shape, {id, contour, index}), unlocked, and name vertices it has"
                    .into(),
            );
        }
    }
    // A click on an end of the shape's direction line, as a command.
    if let Some(which) = params.and_then(|p| p.get("selectDirectionLine")) {
        // true: the first line; a number: that line; false: none.
        let index = match (which.as_bool(), which.as_u64()) {
            (Some(true), _) => Some(0),
            (Some(false), _) => None,
            (None, Some(k)) => Some(k as usize),
            _ => return Err("selectDirectionLine: must be true, false or a line's index".into()),
        };
        app.tools.set_tool(openstitch_app::editor::Tool::Point);
        let mut ok = false;
        app.with_tools(|tools, ctx| {
            ok = tools.select_direction_line(index, ctx);
            vec![]
        });
        if !ok {
            return Err(
                "selectDirectionLine: the selected shape shows no such direction line (directionLines lists them; it needs one shape with a fill sewn in one straight direction or along direction lines)"
                    .into(),
            );
        }
    }
    // Alt + drag of the selected end point onto another end, as a command.
    if let Some(to) = params.and_then(|p| p.get("connectTo")) {
        // The connected point is gone afterwards, so an entry could not run on it.
        if params.is_some_and(|p| p.get("action").is_some() || p.get("step").is_some()) {
            return Err(
                "connectTo cannot be combined with action or step: send them in separate calls"
                    .into(),
            );
        }
        let (Some(id), Some(index)) = (
            to.get("id").and_then(Value::as_u64),
            to.get("index").and_then(Value::as_u64),
        ) else {
            return Err("connectTo: must be {id, index}, an entry of connectable".into());
        };
        let mut done = false;
        app.with_tools(|tools, ctx| {
            let effects = tools.connect_point((id, index as usize), ctx);
            done = effects.is_some();
            effects.unwrap_or_default()
        });
        if !done {
            return Err(
                "connectTo: select one end point of an open line first, and name an end listed in connectable"
                    .into(),
            );
        }
        return Ok(state(app));
    }
    // Ctrl+arrow (step) and Shift+arrow (step with extend): the selection moves or grows along
    // the outline; no point moves.
    if let Some(step) = params.and_then(|p| p.get("step")) {
        let forward = match step.as_str() {
            Some("next") => true,
            Some("previous") => false,
            _ => return Err("step: must be next or previous".into()),
        };
        let extend = params
            .and_then(|p| p.get("extend"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut ok = false;
        app.with_tools(|tools, ctx| {
            ok = tools.step_points(forward, extend, ctx);
            vec![]
        });
        if !ok {
            return Err("step: no point is selected with the Point tool".into());
        }
    }
    let Some(key) = params.and_then(|p| p.get("action")).and_then(Value::as_str) else {
        return Ok(state(app));
    };
    // An entry of the object menu's Delete Direction Line sub-menu (#1269).
    if let Some((_, _, enabled)) = direction_submenu(app).into_iter().find(|e| e.0 == key) {
        if !enabled {
            return Err(format!(
                "{key} is not available: {}",
                if key == "deleteSelectedLine" {
                    "select one of the shape's direction lines first (selectDirectionLine); a shape's only direction line cannot be deleted, and of a curved direction or a Manual Punch shape's lines none can be selected: deleteLinesNotSelected removes those"
                } else {
                    "select the direction line to keep first (selectDirectionLine): without a selected line the shape has no direction lines this entry can delete"
                }
            ));
        }
        run_direction_entry(app, key);
        app.workspace.view.point_menu = None;
        return Ok(state(app));
    }
    let a = PointAction::MENU
        .into_iter()
        .find(|a| a.key() == key)
        .ok_or_else(|| {
            format!(
                "action: must be one of {}, deleteSelectedLine, deleteLinesNotSelected",
                PointAction::MENU.map(PointAction::key).join(", ")
            )
        })?;
    let enabled = states(app).ok_or("no point is selected with the Point tool")?;
    if !enabled[slot(a)] {
        return Err(format!("{key} is not available for the selected points"));
    }
    run(app, a);
    app.workspace.view.point_menu = None;
    Ok(state(app))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;
    use crate::ribbon::enabled;
    use openstitch_app::editor::CommandRequest;
    use openstitch_design::PointKind;
    use std::collections::BTreeSet;

    fn with_rectangle() -> (EditorApp, u64) {
        let mut app = EditorApp::new(Launch::default());
        let id = app
            .execute(&CommandRequest::new(
                "shape.rectangle",
                json!({ "x": 10, "y": 10, "width": 20, "height": 10, "fill": "#2e8b57", "outline": "#203040" }),
            ))
            .expect("rectangle")["id"]
            .as_u64()
            .expect("id");
        app.selection = BTreeSet::from([id]);
        (app, id)
    }

    fn kind(app: &EditorApp, index: usize) -> PointKind {
        app.session.design().objects[0].contours[0].kind(index)
    }

    /// The Shapes tab's Edit Point group and `ui.pointMenu` follow the right-click menu:
    /// all grey with no point selected (as PE-DESIGN's), then greyed entry by entry.
    #[test]
    fn the_ribbon_and_ui_point_menu_follow_the_menu() {
        let (mut app, _) = with_rectangle();
        // No point selected, the points showing: all three of the group are grey.
        app.tools.set_tool(openstitch_app::editor::Tool::Point);
        for a in [
            Action::EditPoint,
            Action::DeletePoint,
            Action::SplitAtPoint,
            Action::PointToCurve,
        ] {
            assert!(!enabled(&app, a), "{a:?} with no point selected");
        }
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "points": [0] })))
            .expect("select a point");
        assert_eq!(s["pointCount"], 4);
        assert_eq!(s["selected"][0]["index"], 0);
        assert_eq!((&s["selected"][0]["x"], &s["selected"][0]["y"]), (&json!(10.0), &json!(10.0)));
        let entries = s["entries"].as_array().expect("entries");
        let row = |i: usize| {
            (
                entries[i]["label"].as_str().expect("label"),
                entries[i]["separatorBefore"] == true,
                entries[i]["enabled"] == true,
            )
        };
        assert_eq!(
            (0..7).map(row).collect::<Vec<_>>(),
            [
                ("To Straight", false, false),
                ("To Curve", false, true),
                ("To Smooth", true, false),
                ("To Symmetry", false, false),
                ("To Cusp", false, false),
                ("Split at Point", true, true),
                ("Delete", true, true),
            ]
        );
        assert_eq!(s["open"], false);
        for (a, on) in [
            (Action::EditPoint, true),
            (Action::PointToStraight, false),
            (Action::PointToCurve, true),
            (Action::PointToSmooth, false),
            (Action::PointToSymmetry, false),
            (Action::PointToCusp, false),
            (Action::SplitAtPoint, true),
            (Action::DeletePoint, true),
        ] {
            assert_eq!(enabled(&app, a), on, "{a:?} on a straight corner");
        }
        assert!(Action::EditPoint.is_menu(), "a drop-down, as PE-DESIGN's");
        // The drop-down's To Curve, through the ribbon command.
        app.ui_command("ui.run", Some(&json!({ "action": "pointToCurve" })))
            .expect("To Curve");
        assert_eq!(kind(&app, 0), PointKind::Smooth);
        assert!(!enabled(&app, Action::PointToCurve) && !enabled(&app, Action::PointToSmooth));
        assert!(enabled(&app, Action::PointToStraight) && enabled(&app, Action::PointToCusp));
        let e = app
            .ui_command("ui.pointMenu", Some(&json!({ "action": "toSmooth" })))
            .expect_err("greyed out");
        assert!(e.contains("not available"), "{e}");
        app.ui_command("ui.pointMenu", Some(&json!({ "action": "toCusp" })))
            .expect("To Cusp");
        assert_eq!(kind(&app, 0), PointKind::Cusp);
        app.undo();
        assert_eq!(kind(&app, 0), PointKind::Smooth, "one Undo per entry");
        app.undo();
        assert!(app.session.design().objects[0].contours[0].curves.is_empty());
        // Two points: Split at Point greys, the To entries act on both.
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "points": [0, 2], "action": "toCurve" })))
            .expect("two points");
        assert_eq!(s["entries"][5]["enabled"], false);
        assert_eq!((kind(&app, 0), kind(&app, 2)), (PointKind::Smooth, PointKind::Smooth));
        app.undo();
        app.ui_command(
            "ui.pointMenu",
            Some(&json!({ "points": [3], "action": "splitAtPoint" })),
        )
        .expect("Split at Point");
        assert!(!app.session.design().objects[0].contours[0].closed);
        assert!(
            app.ui_command("ui.pointMenu", Some(&json!({ "points": [99] })))
                .is_err()
        );
        assert!(
            app.ui_command("ui.pointMenu", Some(&json!({ "action": "bend" })))
                .is_err()
        );
    }

    /// Every row of PE-DESIGN 11.50's menu table (its menu read from the screen on
    /// 2026-11-18, 16 cases; docs/COMPATIBILITY_AUDIT.md "Select Point: the right-click menu
    /// on a point"), through `ui.pointMenu` as a script or the Assistant reads it, and through
    /// the ribbon's Edit Point drop-down, Delete Point and Split at Point, which must agree.
    /// Columns: To Straight, To Curve, To Smooth, To Symmetry, To Cusp, Split at Point, Delete.
    #[test]
    fn every_row_of_pe_designs_menu_table() {
        const ON: bool = true;
        const NO: bool = false;
        let rectangle = ("shape.rectangle", json!({ "x": 10, "y": 10, "width": 40, "height": 40, "fill": "#2e8b57", "outline": "#203040" }));
        let polygon = |points: Value, closed: bool| {
            ("shape.polygon", json!({ "points": points, "closed": closed, "outline": "#203040" }))
        };
        let circle = ("shape.ellipse", json!({ "cx": 50, "cy": 50, "rx": 20, "outline": "#203040" }));
        let kind = |indexes: Value, kind: &str| {
            ("object.setPointKind", json!({ "id": 1, "indexes": indexes, "kind": kind }))
        };
        let delete = |indexes: Value| ("object.deletePoint", json!({ "id": 1, "indexes": indexes }));
        let line = || polygon(json!([[10, 60], [40, 60], [40, 30]]), false);
        // (PE-DESIGN's row, the commands that make the case, the selected points, the row).
        type Row = (&'static str, Vec<(&'static str, Value)>, Value, [bool; 7]);
        let rows: Vec<Row> = vec![
            ("closed, straight corner", vec![rectangle.clone()], json!([0]), [NO, ON, NO, NO, NO, ON, ON]),
            (
                "closed, after To Curve (point is smooth)",
                vec![rectangle.clone(), kind(json!([0]), "curve")],
                json!([0]),
                [ON, NO, NO, ON, ON, ON, ON],
            ),
            (
                "closed, symmetric point",
                vec![rectangle.clone(), kind(json!([0]), "curve"), kind(json!([0]), "symmetric")],
                json!([0]),
                [ON, NO, ON, NO, ON, ON, ON],
            ),
            (
                "closed, cusp point",
                vec![rectangle.clone(), kind(json!([0]), "curve"), kind(json!([0]), "cusp")],
                json!([0]),
                [ON, NO, ON, ON, NO, ON, ON],
            ),
            (
                "closed, corner of the triangle",
                vec![rectangle.clone(), delete(json!([0]))],
                json!([0]),
                [NO, ON, NO, NO, NO, ON, ON],
            ),
            (
                "closed, corner of 3 collinear points",
                vec![rectangle.clone(), delete(json!([0])), ("object.setPoint", json!({ "id": 1, "index": 1, "x": 30, "y": 30 }))],
                json!([1]),
                [NO, ON, NO, NO, NO, ON, ON],
            ),
            (
                "closed, corner of the 2-point outline",
                vec![rectangle.clone(), delete(json!([2, 3]))],
                json!([0]),
                [NO, ON, NO, NO, NO, ON, ON],
            ),
            ("two straight points selected", vec![rectangle.clone()], json!([0, 2]), [NO, ON, NO, NO, NO, NO, ON]),
            (
                "two curve points selected",
                vec![rectangle.clone(), kind(json!([0, 2]), "curve")],
                json!([0, 2]),
                [ON, NO, NO, ON, ON, NO, ON],
            ),
            (
                "two curve + one straight selected",
                vec![rectangle.clone(), kind(json!([0, 2]), "curve")],
                json!([0, 2, 1]),
                [ON, ON, NO, ON, ON, NO, ON],
            ),
            ("open line, first point", vec![line()], json!([0]), [NO, ON, NO, NO, NO, NO, ON]),
            ("open line, middle point", vec![line()], json!([1]), [NO, ON, NO, NO, NO, ON, ON]),
            ("open line, last point", vec![line()], json!([2]), [NO, ON, NO, NO, NO, NO, ON]),
            (
                "open line, first point after To Curve",
                vec![line(), kind(json!([0]), "curve")],
                json!([0]),
                [ON, NO, NO, NO, NO, NO, ON],
            ),
            (
                "2-point open line, first point",
                vec![polygon(json!([[10, 60], [40, 60]]), false)],
                json!([0]),
                [NO, ON, NO, NO, NO, NO, ON],
            ),
            (
                "2-point open line, last point",
                vec![polygon(json!([[10, 60], [40, 60]]), false)],
                json!([1]),
                [NO, ON, NO, NO, NO, NO, ON],
            ),
            ("circle point (as drawn)", vec![circle.clone()], json!([0]), [ON, NO, NO, ON, ON, ON, ON]),
            (
                "circle point after To Cusp",
                vec![circle.clone(), kind(json!([0]), "cusp")],
                json!([0]),
                [ON, NO, ON, ON, NO, ON, ON],
            ),
            (
                "circle point after To Straight",
                vec![circle.clone(), kind(json!([0]), "straight")],
                json!([0]),
                [NO, ON, NO, NO, NO, ON, ON],
            ),
        ];
        // PE-DESIGN's 16 rows; three of them stand for more than one case here.
        assert_eq!(rows.len(), 19);
        let ribbon = [
            Action::PointToStraight,
            Action::PointToCurve,
            Action::PointToSmooth,
            Action::PointToSymmetry,
            Action::PointToCusp,
            Action::SplitAtPoint,
            Action::DeletePoint,
        ];
        for (name, setup, points, want) in rows {
            let mut app = EditorApp::new(Launch::default());
            for (command, params) in &setup {
                app.execute(&CommandRequest::new(command, params.clone()))
                    .unwrap_or_else(|| panic!("{name}: {command} {params}"));
            }
            assert_eq!(app.session.design().objects.len(), 1, "{name}");
            app.selection = BTreeSet::from([app.session.design().objects[0].id]);
            let s = app
                .ui_command("ui.pointMenu", Some(&json!({ "points": points })))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            let got: Vec<bool> = s["entries"]
                .as_array()
                .expect("entries")
                .iter()
                .map(|e| e["enabled"] == true)
                .collect();
            assert_eq!(got, want, "{name}: the menu");
            let keys: Vec<&str> = s["entries"]
                .as_array()
                .expect("entries")
                .iter()
                .map(|e| e["key"].as_str().expect("key"))
                .collect();
            assert_eq!(
                keys,
                ["toStraight", "toCurve", "toSmooth", "toSymmetry", "toCusp", "splitAtPoint", "deletePoint"],
                "{name}"
            );
            // The ribbon: the drop-down opens with a point selected, and each of its entries,
            // Delete Point and Split at Point follow the menu.
            assert!(enabled(&app, Action::EditPoint), "{name}: Edit Point");
            for (a, on) in ribbon.into_iter().zip(want) {
                assert_eq!(enabled(&app, a), on, "{name}: ribbon {a:?}");
            }
        }
        // The drop-down's entries in PE-DESIGN's order: Line Type, then Curve Type.
        use openstitch_app::editor::ribbon::EDIT_POINT_MENU;
        assert_eq!(EDIT_POINT_MENU, [ribbon[0], ribbon[1], ribbon[2], ribbon[3], ribbon[4]]);
        assert_eq!((t("menu-point-line-type"), t("menu-point-curve-type")), ("Line Type", "Curve Type"));
    }

    /// `ui.pointMenu` says where the open menu was painted: nothing while it is closed or
    /// only measured (a menu's first frame paints nothing), then its panel's rectangle at the
    /// pointer and its colour; so a script knows the menu is on screen, not just open.
    #[test]
    fn the_menu_reports_where_it_was_painted() {
        let (mut app, _) = with_rectangle();
        app.ui_command("ui.pointMenu", Some(&json!({ "points": [0] })))
            .expect("select a point");
        let ctx = egui::Context::default();
        let frame = |app: &mut EditorApp| {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 800.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| window(app, ui.ctx()));
            out.textures_delta.clear();
            app.ui_command("ui.pointMenu", None).expect("state")
        };
        for object in [false, true] {
            assert!(frame(&mut app)["menuRect"].is_null(), "no menu open");
            app.workspace.view.point_menu = Some(PointMenu {
                at: egui::pos2(300.0, 200.0),
                fresh: true,
                object,
                sub: false,
                drawn: None,
                needle: false,
            });
            let measured = frame(&mut app);
            assert_eq!(
                (measured["open"] == true, measured["objectMenuOpen"] == true),
                (!object, object)
            );
            assert!(measured["menuRect"].is_null(), "the first frame only measures: {measured}");
            let painted = frame(&mut app);
            let r: Vec<f64> = painted["menuRect"]
                .as_array()
                .expect("painted in the second frame")
                .iter()
                .filter_map(Value::as_f64)
                .collect();
            assert!((r[0] - 300.0).abs() < 1.0 && (r[1] - 200.0).abs() < 1.0, "at the pointer: {r:?}");
            assert!(r[2] >= 170.0 && r[3] >= 5.0 * 22.0, "the panel's size: {r:?}");
            let fill = painted["menuFill"].as_str().expect("fill");
            assert!(fill.starts_with('#') && fill.len() == 7, "{fill}");
            app.workspace.view.point_menu = None;
        }
        let summary = openstitch_app::editor::assistant::UI_COMMANDS
            .iter()
            .find(|c| c.id == "ui.pointMenu")
            .expect("ui.pointMenu")
            .summary;
        for words in ["menuRect", "menuFill", "null until a frame has painted"] {
            assert!(summary.contains(words), "ui.pointMenu's summary names {words:?}");
        }
    }

    /// The hint line follows the selection, as PE-DESIGN's status bar does (#1260): with an
    /// end point of an open line selected it tells of Alt and connecting the two ends.
    #[test]
    fn the_hint_line_follows_the_selected_point() {
        let hint = |app: &mut EditorApp, points: Value| {
            let s = app
                .ui_command("ui.pointMenu", Some(&json!({ "points": points })))
                .expect("points");
            s["hint"].as_str().expect("hint").to_string()
        };
        let (mut app, _) = with_rectangle();
        assert!(app.ui_command("ui.pointMenu", None).expect("state")["hint"].is_null());
        app.tools.set_tool(openstitch_app::editor::Tool::Point);
        let none = app.ui_command("ui.pointMenu", None).expect("state")["hint"]
            .as_str()
            .expect("hint")
            .to_string();
        assert!(none.contains("Click a point") && none.contains("Open and Close"), "{none}");
        let straight = hint(&mut app, json!([0]));
        assert!(straight.contains("To Curve") && !straight.contains("Alt"), "{straight}");
        let several = hint(&mut app, json!([0, 2]));
        assert!(several.contains("move them all"), "{several}");
        app.execute(&CommandRequest::new(
            "object.setPointKind",
            json!({ "id": 1, "index": 0, "kind": "curve" }),
        ))
        .expect("To Curve");
        let curve = hint(&mut app, json!([0]));
        assert!(curve.contains("handle lines") && curve.contains("To Symmetry"), "{curve}");
        // An open line: its ends tell of Alt, its middle point does not.
        let mut app = EditorApp::new(Launch::default());
        let id = app
            .execute(&CommandRequest::new(
                "shape.polygon",
                json!({ "points": [[10, 60], [40, 60], [40, 30]], "closed": false, "outline": "#203040" }),
            ))
            .expect("line")["id"]
            .as_u64()
            .expect("id");
        app.selection = BTreeSet::from([id]);
        for end in [0, 2] {
            let text = hint(&mut app, json!([end]));
            assert!(
                text.contains("Hold Alt") && text.contains("other end") && text.contains("closed shape"),
                "{text}"
            );
        }
        assert!(!hint(&mut app, json!([1])).contains("Alt"));
    }

    /// PE-DESIGN capture g, 2026-11-19 (#1273), the greying with real lines and the two
    /// sub-menu entries: with two or more lines and one selected, Delete Direction Line and
    /// both entries are available; Delete Lines Not Selected keeps the selected line,
    /// unselected afterwards; Delete Selected Line removes it; one undo step each.
    #[test]
    fn direction_lines_are_added_selected_and_deleted_through_the_ui_commands() {
        let mut app = EditorApp::new(Launch::default());
        let id = app
            .execute(&CommandRequest::new(
                "shape.rectangle",
                json!({ "x": 30, "y": 30, "width": 40, "height": 40, "fill": "#2e8b57" }),
            ))
            .expect("square")["id"]
            .as_u64()
            .expect("id");
        app.execute(&CommandRequest::new(
            "object.setRegion",
            json!({ "id": id, "angle": 45 }),
        ))
        .expect("angle");
        app.selection = BTreeSet::from([id]);
        // Add Direction Line from the ribbon starts the two clicks with the Point tool.
        app.ui_command("ui.run", Some(&json!({ "action": "addDirection" })))
            .expect("Add Direction Line");
        let s = app.ui_command("ui.pointMenu", None).expect("state");
        assert_eq!(s["addingDirectionLine"], true);
        assert!(s["hint"].as_str().expect("hint").contains("click outside the shape"));
        for at in [v2(252.0, 384.0), v2(501.7, 280.0)] {
            app.with_tools(|tools, ctx| {
                tools.handle(
                    openstitch_app::editor::PointerEvent::new(openstitch_app::editor::Phase::Down, at),
                    ctx,
                )
            });
        }
        let s = app.ui_command("ui.pointMenu", None).expect("state");
        assert_eq!(s["addingDirectionLine"], false);
        let lines = s["directionLines"].as_array().expect("lines");
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l["stored"] == true && l["selected"] == false));
        assert!(app.status.contains("object.addDirectionLine"), "{}", app.status);
        // Nothing selected: Delete Direction Line grey, both entries grey.
        assert!(!enabled(&app, Action::DeleteDirectionLine));
        let sub = direction_submenu(&app);
        assert_eq!((sub[0].2, sub[1].2), (false, false));
        assert!(enabled(&app, Action::AddDirectionLine) && enabled(&app, Action::ReverseDirectionLine));
        let two = app.session.design().clone();
        // One of two selected: all three available.
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "selectDirectionLine": 1 })))
            .expect("select the second line");
        assert_eq!(s["directionLine"]["index"], 1);
        assert_eq!(s["directionLines"][1]["selected"], true);
        assert!(enabled(&app, Action::DeleteDirectionLine));
        let sub = direction_submenu(&app);
        assert_eq!((sub[0].2, sub[1].2), (true, true));
        // Delete Lines Not Selected: the 45 degree line goes, the selected one stays as the
        // fill angle and is not selected any more; Delete is grey again.
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "action": "deleteLinesNotSelected" })))
            .expect("Delete Lines Not Selected");
        let lines = s["directionLines"].as_array().expect("lines");
        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0]["stored"].clone(), lines[0]["selected"].clone()), (json!(false), json!(false)));
        let angle = lines[0]["angle"].as_f64().expect("angle");
        assert!((angle - 22.6).abs() < 0.3, "{angle}");
        assert!(!enabled(&app, Action::DeleteDirectionLine));
        app.undo();
        assert_eq!(*app.session.design(), two, "one Undo");
        // Delete Selected Line: that line goes, the 45 degree one is left.
        app.ui_command("ui.pointMenu", Some(&json!({ "selectDirectionLine": 1 })))
            .expect("select");
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "action": "deleteSelectedLine" })))
            .expect("Delete Selected Line");
        assert_eq!(s["directionLines"].as_array().expect("lines").len(), 1);
        assert_eq!(s["directionLine"]["angle"], 45.0);
        app.undo();
        assert_eq!(*app.session.design(), two, "one Undo");
        // The ribbon's Delete Direction Line with a line selected deletes that line.
        app.ui_command("ui.pointMenu", Some(&json!({ "selectDirectionLine": 0 })))
            .expect("select");
        app.ui_command("ui.run", Some(&json!({ "action": "deleteDirection" })))
            .expect("Delete Direction Line");
        let s = app.ui_command("ui.pointMenu", None).expect("state");
        let angle = s["directionLine"]["angle"].as_f64().expect("angle");
        assert!((angle - 22.6).abs() < 0.3, "the 45 degree line went: {angle}");
        // Delete Lines Not Selected with the FIRST line selected (index 0 is also the index
        // of the one line that is left, so only the deselect leaves it unselected), and
        // which line is kept, by where it lies. Three lines, in order across the shape: the
        // bottom-right one, the 45 degree one, the top-left one.
        app.undo();
        assert_eq!(*app.session.design(), two);
        let third = app
            .execute(&CommandRequest::new(
                "object.addDirectionLine",
                json!({ "id": id, "line": [[57.05, 74.0], [74.0, 57.2]] }),
            ))
            .expect("a third line");
        assert_eq!(third["index"], 0, "before the 45 degree line");
        let three = app.session.design().clone();
        let stored = |app: &EditorApp| {
            app.session.design().objects[0]
                .region
                .as_ref()
                .expect("region")
                .angle_lines
                .clone()
        };
        let before = stored(&app);
        // (the line selected, the angle of the line that is left)
        for (keep, angle) in [(0, 44.7), (1, 45.0), (2, 22.6)] {
            let s = app
                .ui_command("ui.pointMenu", Some(&json!({ "selectDirectionLine": keep })))
                .expect("select");
            assert_eq!(s["directionLines"][keep]["selected"], true);
            let kept = before[keep];
            let s = app
                .ui_command("ui.pointMenu", Some(&json!({ "action": "deleteLinesNotSelected" })))
                .expect("Delete Lines Not Selected");
            let lines = s["directionLines"].as_array().expect("lines");
            assert_eq!(lines.len(), 1, "keep {keep}");
            assert_eq!(
                (lines[0]["stored"].clone(), lines[0]["selected"].clone()),
                (json!(false), json!(false)),
                "keep {keep}: the line that is left is not selected"
            );
            let left = lines[0]["angle"].as_f64().expect("angle");
            assert!((left - angle).abs() < 0.3, "keep {keep}: {left}");
            // The line that is left is the kept line itself, at its place (its two ends as
            // stored), and points the way it did.
            assert_eq!(stored(&app), vec![kept], "keep {keep}");
            assert!(
                (left - openstitch_design::angle_lines::line_angle(kept)).abs() < 1e-9,
                "keep {keep}"
            );
            assert!(!enabled(&app, Action::DeleteDirectionLine), "keep {keep}");
            let sub = direction_submenu(&app);
            assert_eq!((sub[0].2, sub[1].2), (false, false), "keep {keep}");
            app.undo();
            assert_eq!(*app.session.design(), three, "one Undo");
        }
        // The curved direction keeps its own command: Curved Direction draws it with the Pen.
        assert!(enabled(&app, Action::CurvedDirection));
        app.ui_command("ui.run", Some(&json!({ "action": "curvedDirection" })))
            .expect("Curved Direction");
        assert_eq!(app.tools.tool(), openstitch_app::editor::Tool::Pen);
        let info = Action::AddDirectionLine.info();
        assert_eq!(info.engine, Some("object.addDirectionLine"));
        assert!(info.tip.contains("click outside the shape"));
        assert_eq!(Action::CurvedDirection.info().engine, Some("object.setDirectionCurve"));
        let spec = openstitch_app::editor::assistant::UI_COMMANDS
            .iter()
            .find(|c| c.id == "ui.pointMenu")
            .expect("ui.pointMenu");
        for words in [
            "directionLines",
            "addingDirectionLine",
            "object.addDirectionLine",
            "object.deleteDirectionLine",
            "deleteSelectedLine removes the selected line",
            "deleteLinesNotSelected removes every line but the selected one",
            "object.moveDirectionLine",
            "The selection is the line itself, not its index",
        ] {
            assert!(spec.summary.contains(words), "summary: {words}");
        }
    }

    /// Adding a line to a shape with Auto direction switches Auto direction off, and the
    /// status bar says so after the command's name.
    #[test]
    fn adding_a_line_says_that_auto_direction_went_off() {
        let mut app = EditorApp::new(Launch::default());
        let id = app
            .execute(&CommandRequest::new(
                "shape.rectangle",
                json!({ "x": 30, "y": 30, "width": 40, "height": 40, "fill": "#2e8b57" }),
            ))
            .expect("square")["id"]
            .as_u64()
            .expect("id");
        app.execute(&CommandRequest::new(
            "object.setRegion",
            json!({ "id": id, "autoDirection": true }),
        ))
        .expect("auto");
        app.selection = BTreeSet::from([id]);
        app.ui_command("ui.run", Some(&json!({ "action": "addDirection" })))
            .expect("Add Direction Line");
        for at in [v2(252.0, 384.0), v2(501.7, 280.0)] {
            app.with_tools(|tools, ctx| {
                tools.handle(
                    openstitch_app::editor::PointerEvent::new(openstitch_app::editor::Phase::Down, at),
                    ctx,
                )
            });
        }
        assert!(
            app.status.contains("object.addDirectionLine")
                && app.status.contains("Auto direction is off"),
            "{}",
            app.status
        );
        let r = app.session.design().objects[0].region.clone().expect("region");
        assert!(!r.auto_direction && r.angle_lines.len() == 2);
    }

    /// An entry of the point menu or the object menu run through `ui.run` closes an open menu,
    /// as a click on the entry does (a script could leave it open, and the next click then
    /// landed on a row of it); a refused entry leaves the menu open.
    #[test]
    fn ui_run_of_a_menu_entry_closes_the_open_menu() {
        let open = |app: &mut EditorApp, object: bool| {
            app.workspace.view.point_menu = Some(PointMenu {
                at: egui::pos2(300.0, 200.0),
                fresh: false,
                object,
                sub: false,
                drawn: None,
                needle: false,
            });
        };
        let state = |app: &mut EditorApp| {
            let s = app.ui_command("ui.pointMenu", None).expect("state");
            (s["open"] == true, s["objectMenuOpen"] == true)
        };
        let (mut app, _) = with_rectangle();
        app.ui_command("ui.pointMenu", Some(&json!({ "points": [0] })))
            .expect("a point");
        // Refused (the outline is closed already): the object menu stays open.
        open(&mut app, true);
        assert!(
            app.ui_command("ui.run", Some(&json!({ "action": "closeOutline" })))
                .is_err()
        );
        assert_eq!(state(&mut app), (false, true));
        // Open runs: the menu closes.
        app.ui_command("ui.run", Some(&json!({ "action": "openOutline" })))
            .expect("Open");
        assert_eq!(state(&mut app), (false, false));
        assert!(!app.session.design().objects[0].contours[0].closed);
        // A point entry through ui.run closes the point menu.
        app.ui_command("ui.pointMenu", Some(&json!({ "points": [1] })))
            .expect("a point");
        open(&mut app, false);
        assert_eq!(state(&mut app), (true, false));
        app.ui_command("ui.run", Some(&json!({ "action": "pointToCurve" })))
            .expect("To Curve");
        assert_eq!(state(&mut app), (false, false));
        // Another command leaves a menu alone.
        open(&mut app, true);
        app.ui_command("ui.run", Some(&json!({ "action": "selectAll" })))
            .expect("Select All");
        assert_eq!(state(&mut app), (false, true));
    }

    /// PE-DESIGN capture g, 2026-11-19: a filled shape shows one direction line, its fill
    /// angle; a click on an end selects it; the only line cannot be deleted. The greying, row
    /// by row.
    #[test]
    fn the_direction_line_of_a_filled_shape_and_its_greying() {
        use Action as A;
        let (mut app, id) = with_rectangle();
        app.execute(&CommandRequest::new(
            "object.setRegion",
            json!({ "id": id, "angle": 45 }),
        ))
        .expect("a straight direction");
        // With another tool no line shows.
        assert!(app.ui_command("ui.pointMenu", None).expect("state")["directionLine"].is_null());
        app.tools.set_tool(openstitch_app::editor::Tool::Point);
        let s = app.ui_command("ui.pointMenu", None).expect("state");
        let angle = app.session.design().objects[0].region.as_ref().expect("region").angle;
        assert_eq!(s["directionLine"]["angle"], angle);
        assert_eq!(s["directionLine"]["selected"], false);
        // Shape selected, nothing else: Add available, Delete grey, Reverse available.
        assert!(enabled(&app, A::AddDirectionLine));
        assert!(!enabled(&app, A::DeleteDirectionLine));
        assert!(enabled(&app, A::ReverseDirectionLine));
        // The only line selected: Delete stays grey, its two entries too; the Delete key
        // removes nothing (not the line, not the shape).
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "selectDirectionLine": true })))
            .expect("select the line");
        assert_eq!(s["directionLine"]["selected"], true);
        assert!(s["hint"].as_str().expect("hint").contains("cannot be deleted"));
        assert!(!enabled(&app, A::DeleteDirectionLine));
        let sub = direction_submenu(&app);
        assert_eq!((sub[0].2, sub[1].2), (false, false));
        let before = app.session.design().clone();
        let mut used = false;
        app.with_tools(|tools, ctx| {
            let effects = tools.delete_point(ctx);
            used = effects.is_some();
            effects.unwrap_or_default()
        });
        assert!(used, "the key is taken: it does not delete the shape");
        assert_eq!(*app.session.design(), before);
        // Selecting a point drops the line's selection.
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "points": [0] })))
            .expect("points");
        assert_eq!(s["directionLine"]["selected"], false);
        // Reverse turns the line the other way (all of a shape's lines; here the one).
        app.ui_command("ui.run", Some(&json!({ "action": "reverseDirection" })))
            .expect("reverse");
        let s = app.ui_command("ui.pointMenu", None).expect("state");
        assert_eq!(s["directionLine"]["angle"], (angle + 180.0).rem_euclid(360.0));
        // A curved direction is OpenStitch's own: no straight line shows then, and Delete
        // Direction Line is available to remove it.
        app.execute(&CommandRequest::new(
            "object.setDirectionCurve",
            json!({ "id": id, "points": [[11, 15], [20, 13], [29, 15]] }),
        ))
        .expect("curve");
        assert!(app.ui_command("ui.pointMenu", None).expect("state")["directionLine"].is_null());
        assert!(enabled(&app, A::DeleteDirectionLine));
        assert!(
            app.ui_command("ui.pointMenu", Some(&json!({ "selectDirectionLine": true })))
                .is_err()
        );
        // An open line (no fill): Add and Reverse grey, both sub-menu entries grey, the
        // menu's Delete Direction Line row still opens its list.
        let mut app = EditorApp::new(Launch::default());
        let line = app
            .execute(&CommandRequest::new(
                "shape.polygon",
                json!({ "points": [[10, 60], [40, 60], [40, 30]], "closed": false, "outline": "#203040" }),
            ))
            .expect("line")["id"]
            .as_u64()
            .expect("id");
        app.selection = BTreeSet::from([line]);
        app.tools.set_tool(openstitch_app::editor::Tool::Point);
        assert!(app.ui_command("ui.pointMenu", None).expect("state")["directionLine"].is_null());
        assert!(!enabled(&app, A::AddDirectionLine) && !enabled(&app, A::ReverseDirectionLine));
        assert!(object_entry_enabled(&app, A::DeleteDirectionLine));
        let sub = direction_submenu(&app);
        assert_eq!((sub[0].2, sub[1].2), (false, false));
        let spec = openstitch_app::editor::assistant::UI_COMMANDS
            .iter()
            .find(|c| c.id == "ui.pointMenu")
            .expect("ui.pointMenu");
        // Why Delete Selected Line is grey, said truly: a line can be selected now.
        assert!(!spec.summary.contains("no direction line can be selected"));
        assert!(
            spec.summary.contains(
                "deleteSelectedLine (grey: a shape's only direction line cannot be deleted"
            )
        );
        let e = app
            .ui_command("ui.pointMenu", Some(&json!({ "action": "deleteSelectedLine" })))
            .expect_err("grey");
        assert!(e.contains("only direction line cannot be deleted"), "{e}");
        for words in ["directionLine", "selectDirectionLine", "object.setRegion", "cannot be deleted"] {
            assert!(spec.summary.contains(words), "summary: {words}");
        }
        assert!(spec.params.iter().any(|p| p.name == "selectDirectionLine"));
    }

    /// PE-DESIGN's manual (read 2026-11-19): with an end point selected and Alt held, the
    /// ends it can connect to are the same line's other end and the ends of other open
    /// lines; Alt + drag onto another line's end joins the two into one shape.
    #[test]
    fn an_end_point_connects_to_another_open_lines_end_in_one_undo_step() {
        let mut app = EditorApp::new(Launch::default());
        let line = |app: &mut EditorApp, points: Value| {
            app.execute(&CommandRequest::new(
                "shape.polygon",
                json!({ "points": points, "closed": false, "outline": "#203040" }),
            ))
            .expect("line")["id"]
                .as_u64()
                .expect("id")
        };
        let a = line(&mut app, json!([[10, 60], [40, 60], [40, 30]]));
        let b = line(&mut app, json!([[50, 30], [70, 30], [70, 60]]));
        app.selection = BTreeSet::from([a]);
        let before = app.session.design().clone();
        let ends = |s: &Value| {
            s["connectable"]
                .as_array()
                .expect("connectable")
                .iter()
                .map(|e| (e["id"].as_u64().expect("id"), e["index"].as_u64().expect("index")))
                .collect::<Vec<_>>()
        };
        // A middle point connects to nothing; an end to its own other end and both ends of
        // the other line (which is not selected).
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "points": [1] })))
            .expect("points");
        assert!(ends(&s).is_empty());
        assert!(
            app.ui_command("ui.pointMenu", Some(&json!({ "connectTo": { "id": b, "index": 0 } })))
                .is_err()
        );
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "points": [2] })))
            .expect("points");
        assert_eq!(ends(&s), [(a, 0), (b, 0), (b, 2)]);
        assert_eq!(s["connectable"][1]["x"], 50.0);
        assert!(s["hint"].as_str().expect("hint").contains("another open line"));
        // Connected to the other line's near end: one object, the two ends one point.
        app.ui_command("ui.pointMenu", Some(&json!({ "connectTo": { "id": b, "index": 0 } })))
            .expect("connect");
        assert!(
            app.status.contains("shape.joinLines"),
            "the status names the join: {}",
            app.status
        );
        let d = app.session.design();
        assert_eq!(d.objects.len(), 1, "the two lines are one object");
        assert_eq!(d.objects[0].id, a, "the dragged line's id is kept");
        let c = &d.objects[0].contours[0];
        assert_eq!(c.points.len(), 5);
        assert!(!c.closed);
        assert_eq!(
            (c.points[0].x, c.points[2].x, c.points[4].x),
            (100.0, 500.0, 700.0)
        );
        app.undo();
        assert_eq!(*app.session.design(), before, "one Undo");
        // A refused join changes nothing: the end is not left moved, Redo holds nothing,
        // the point is still selected and the status bar says why. (Here the join is made
        // to fail by naming an object that does not exist.)
        app.ui_command("ui.pointMenu", Some(&json!({ "points": [2] })))
            .expect("points");
        app.with_tools(|tools, ctx| {
            let mut effects = tools.connect_point((b, 0), ctx).expect("connect");
            let openstitch_app::editor::ToolEffect::Execute { requests, .. } = &mut effects[0]
            else {
                panic!("an Execute effect");
            };
            assert_eq!(requests[1].command, "shape.joinLines");
            requests[1].params = json!({ "ids": [a, 987_654], "tolerance": 0.1 });
            effects
        });
        assert_eq!(*app.session.design(), before, "the moved end is taken back");
        assert!(!app.session.can_redo(), "nothing is left for Redo");
        assert!(app.status.contains("987654"), "the refusal is shown: {}", app.status);
        let s = app.ui_command("ui.pointMenu", None).expect("state");
        assert_eq!(s["selected"][0]["index"], 2, "the point is still selected");
        app.undo();
        assert_eq!(
            app.session.design().objects.len(),
            1,
            "Undo goes on to the step before (the second line), not to a half-done join"
        );
        app.redo();
        assert_eq!(*app.session.design(), before);
        // connectTo with action or step in one call is refused, and nothing is connected.
        let e = app
            .ui_command(
                "ui.pointMenu",
                Some(&json!({ "connectTo": { "id": b, "index": 0 }, "action": "toCurve" })),
            )
            .expect_err("refused");
        assert!(e.contains("cannot be combined"), "{e}");
        assert_eq!(*app.session.design(), before);
        // Its own other end: the line is closed, as before.
        app.ui_command("ui.pointMenu", Some(&json!({ "points": [2] })))
            .expect("points");
        app.ui_command("ui.pointMenu", Some(&json!({ "connectTo": { "id": a, "index": 0 } })))
            .expect("close");
        assert!(app.session.design().objects[0].contours[0].closed);
        assert_eq!(app.session.design().objects.len(), 2);
        app.undo();
        assert_eq!(*app.session.design(), before, "one Undo");
        let spec = openstitch_app::editor::assistant::UI_COMMANDS
            .iter()
            .find(|c| c.id == "ui.pointMenu")
            .expect("ui.pointMenu");
        for words in [
            "connectable",
            "connectTo",
            "shape.joinLines",
            "keeps its id",
            "cannot be combined with action or step",
            "refused join changes nothing",
            "connectOverlap",
        ] {
            assert!(spec.summary.contains(words), "summary: {words}");
        }
        assert!(spec.params.iter().any(|p| p.name == "connectTo"));
    }

    /// PE-DESIGN capture f, 2026-11-19, on an open L line of 3 points (left end, corner, top
    /// end), row by row, through `ui.pointMenu` `step` as the keys run it.
    #[test]
    fn the_arrow_keys_with_ctrl_and_shift() {
        let mut app = EditorApp::new(Launch::default());
        let id = app
            .execute(&CommandRequest::new(
                "shape.polygon",
                json!({ "points": [[10, 60], [40, 60], [40, 30]], "closed": false, "outline": "#203040" }),
            ))
            .expect("line")["id"]
            .as_u64()
            .expect("id");
        app.selection = BTreeSet::from([id]);
        let before = app.session.design().clone();
        let picked = |s: &Value| {
            let mut out: Vec<u64> = s["selected"]
                .as_array()
                .expect("selected")
                .iter()
                .map(|p| p["index"].as_u64().expect("index"))
                .collect();
            out.sort_unstable();
            out
        };
        let select = |app: &mut EditorApp, points: Value| {
            picked(
                &app.ui_command("ui.pointMenu", Some(&json!({ "points": points })))
                    .expect("points"),
            )
        };
        let key = |app: &mut EditorApp, step: &str, shift: bool| {
            picked(
                &app.ui_command("ui.pointMenu", Some(&json!({ "step": step, "extend": shift })))
                    .expect("step"),
            )
        };
        // Corner selected. Ctrl+Right: only the top end; again: still the top end (no wrap
        // at the end of the line); Ctrl+Left: only the corner.
        assert_eq!(select(&mut app, json!([1])), [1]);
        assert_eq!(key(&mut app, "next", false), [2]);
        assert_eq!(key(&mut app, "next", false), [2]);
        assert_eq!(key(&mut app, "previous", false), [1]);
        // Corner selected. Shift+Right: corner and top end; Shift+Left: corner only;
        // Shift+Left again: corner and left end.
        assert_eq!(key(&mut app, "next", true), [1, 2]);
        assert_eq!(key(&mut app, "previous", true), [1]);
        assert_eq!(key(&mut app, "previous", true), [0, 1]);
        assert_eq!(key(&mut app, "previous", true), [0, 1], "it stops at the line's end");
        // Corner and left end selected, Ctrl+Down: only the top end (it steps from the
        // starting point, the corner); Ctrl+Up: the corner; Shift+Up: corner and left end.
        assert_eq!(key(&mut app, "next", false), [2]);
        assert_eq!(key(&mut app, "previous", false), [1]);
        assert_eq!(key(&mut app, "previous", true), [0, 1]);
        // The same from a selection made by clicks: the corner first, then the left end.
        assert_eq!(select(&mut app, json!([0, 1])), [0, 1]);
        assert_eq!(key(&mut app, "next", false), [2], "from the point selected first");
        assert_eq!(*app.session.design(), before, "no point moved");
        // Not captured, chosen: a closed outline's order goes round.
        let (mut app, _) = with_rectangle();
        assert_eq!(select(&mut app, json!([3])), [3]);
        assert_eq!(key(&mut app, "next", false), [0]);
        assert_eq!(key(&mut app, "previous", false), [3]);
        assert_eq!(key(&mut app, "next", true), [0, 3]);
        assert_eq!(key(&mut app, "next", true), [0, 1, 3]);
        assert_eq!(key(&mut app, "next", true), [0, 1, 2, 3]);
        assert_eq!(key(&mut app, "next", true), [0, 1, 2, 3], "every point: no further");
        assert_eq!(key(&mut app, "previous", true), [0, 1, 3]);
        // No point selected: refused.
        app.tools.set_tool(openstitch_app::editor::Tool::Point);
        assert!(
            app.ui_command("ui.pointMenu", Some(&json!({ "step": "next" })))
                .is_err()
        );
        let spec = openstitch_app::editor::assistant::UI_COMMANDS
            .iter()
            .find(|c| c.id == "ui.pointMenu")
            .expect("ui.pointMenu");
        for words in ["Ctrl+arrow", "Shift+arrow", "no point moves", "goes round"] {
            assert!(spec.summary.contains(words), "summary: {words}");
        }
        assert!(spec.params.iter().any(|p| p.name == "step"));
        assert!(spec.params.iter().any(|p| p.name == "extend"));
    }

    /// #1260, the owner's hand check of 2026-11-19: points of two shapes are selected
    /// together. `ui.pointMenu` names them by shape, runs an entry on all of them in one
    /// undo step, and its summary says how.
    #[test]
    fn ui_point_menu_selects_and_edits_points_of_two_shapes_in_one_undo_step() {
        let (mut app, a) = with_rectangle();
        let b = app
            .execute(&CommandRequest::new(
                "shape.rectangle",
                json!({ "x": 40, "y": 10, "width": 20, "height": 10, "outline": "#203040" }),
            ))
            .expect("rectangle")["id"]
            .as_u64()
            .expect("id");
        app.selection = BTreeSet::from([a, b]);
        // Indexes alone do not say which shape.
        let e = app
            .ui_command("ui.pointMenu", Some(&json!({ "points": [1] })))
            .expect_err("two shapes");
        assert!(e.contains("{id, contour, index}"), "{e}");
        let s = app
            .ui_command(
                "ui.pointMenu",
                Some(&json!({ "points": [{ "id": a, "index": 1 }, { "id": b, "contour": 0, "index": 0 }] })),
            )
            .expect("points of both");
        assert_eq!(s["pointCount"], 8, "the points that show: both shapes'");
        let selected: Vec<(u64, u64, u64)> = s["selected"]
            .as_array()
            .expect("selected")
            .iter()
            .map(|p| {
                (
                    p["id"].as_u64().expect("id"),
                    p["contour"].as_u64().expect("contour"),
                    p["index"].as_u64().expect("index"),
                )
            })
            .collect();
        assert_eq!(selected, [(a, 0, 1), (b, 0, 0)]);
        let enabled = |s: &Value, key: &str| {
            s["entries"]
                .as_array()
                .expect("entries")
                .iter()
                .find(|e| e["key"] == key)
                .expect("entry")["enabled"]
                == true
        };
        assert!(enabled(&s, "toCurve") && enabled(&s, "deletePoint"));
        assert!(!enabled(&s, "toStraight") && !enabled(&s, "splitAtPoint"));
        assert!(s["hint"].as_str().expect("hint").contains("move them all"));
        let curve = |app: &EditorApp| {
            let d = app.session.design();
            (
                d.objects[0].contours[0].is_curve_point(1),
                d.objects[1].contours[0].is_curve_point(0),
            )
        };
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "action": "toCurve" })))
            .expect("To Curve");
        assert_eq!(curve(&app), (true, true));
        assert!(enabled(&s, "toStraight") && !enabled(&s, "toCurve"));
        app.undo();
        assert_eq!(curve(&app), (false, false), "one Undo for both shapes");
        let count = |app: &EditorApp| {
            let d = app.session.design();
            (
                d.objects[0].contours[0].points.len(),
                d.objects[1].contours[0].points.len(),
            )
        };
        app.ui_command("ui.pointMenu", Some(&json!({ "action": "deletePoint" })))
            .expect("Delete");
        assert_eq!(count(&app), (3, 3));
        app.undo();
        assert_eq!(count(&app), (4, 4), "one Undo for both shapes");
        // A drag of one selected point moves the points of both shapes; ONE Undo brings
        // both back.
        app.ui_command(
            "ui.pointMenu",
            Some(&json!({ "points": [{ "id": a, "index": 1 }, { "id": b, "index": 0 }] })),
        )
        .expect("points of both");
        let corners = |app: &EditorApp| {
            let d = app.session.design();
            (d.objects[0].contours[0].points[1], d.objects[1].contours[0].points[0])
        };
        let start = corners(&app);
        for (phase, at) in [
            (openstitch_app::editor::Phase::Down, start.0),
            (openstitch_app::editor::Phase::Dragged, start.0 + v2(30.0, 40.0)),
            (openstitch_app::editor::Phase::Up, start.0 + v2(30.0, 40.0)),
        ] {
            app.with_tools(|tools, ctx| {
                tools.handle(openstitch_app::editor::PointerEvent::new(phase, at), ctx)
            });
        }
        assert_eq!(
            corners(&app),
            (start.0 + v2(30.0, 40.0), start.1 + v2(30.0, 40.0)),
            "both shapes' points moved"
        );
        app.undo();
        assert_eq!(corners(&app), start, "one Undo for both shapes");
        // The arrow keys and the Delete key, as the window runs them: one undo step each.
        app.ui_command(
            "ui.pointMenu",
            Some(&json!({ "points": [{ "id": a, "index": 1 }, { "id": b, "index": 0 }] })),
        )
        .expect("points of both");
        let x = |app: &EditorApp| {
            let d = app.session.design();
            (
                d.objects[0].contours[0].points[1].x,
                d.objects[1].contours[0].points[0].x,
            )
        };
        let before = x(&app);
        app.with_tools(|tools, ctx| tools.nudge_points(POINT_NUDGE, 0.0, ctx).unwrap_or_default());
        assert_eq!(x(&app), (before.0 + 5.0, before.1 + 5.0), "0.5 mm, both shapes");
        app.undo();
        assert_eq!(x(&app), before, "one Undo for both shapes");
        app.with_tools(|tools, ctx| tools.delete_point(ctx).unwrap_or_default());
        assert_eq!(count(&app), (3, 3));
        app.undo();
        assert_eq!(count(&app), (4, 4), "one Undo for both shapes");
        // Ctrl+arrow and Shift+arrow as commands: see `the_arrow_keys_with_ctrl_and_shift`.
        assert!(
            app.ui_command("ui.pointMenu", Some(&json!({ "step": "sideways" })))
                .is_err()
        );
        // What a model reads.
        let spec = openstitch_app::editor::assistant::UI_COMMANDS
            .iter()
            .find(|c| c.id == "ui.pointMenu")
            .expect("ui.pointMenu");
        for words in [
            "{id, contour, index}",
            "Shift+clicks",
            "one engine command per outline",
            "all in one undo step",
            "splitAtPoint only with exactly one point",
            "id of the shape",
        ] {
            assert!(spec.summary.contains(words), "ui.pointMenu's summary names {words:?}");
        }
        let points = spec.params.iter().find(|p| p.name == "points").expect("points");
        assert!(points.summary.contains("several shapes and outlines"));
    }

    /// The menus as PE-DESIGN draws them: an icon in front of Split at Point and of Delete,
    /// none on the To entries; in the object's menu an icon on the three direction-line
    /// entries and a sub-menu on Delete Direction Line.
    #[test]
    fn the_menus_have_pe_designs_icons_and_sub_menu() {
        let (mut app, _) = with_rectangle();
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "points": [0] })))
            .expect("state");
        let icons: Vec<bool> = s["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .map(|e| e["icon"] == true)
            .collect();
        assert_eq!(icons, [false, false, false, false, false, true, true]);
        let object: Vec<(bool, bool)> = s["objectMenu"]
            .as_array()
            .expect("objectMenu")
            .iter()
            .map(|e| (e["icon"] == true, e["submenu"].is_array()))
            .collect();
        assert_eq!(
            object,
            [(true, false), (false, true), (true, false), (false, false), (false, false)]
        );
        // The sub-menu: PE-DESIGN's two entries; both grey on a shape without direction lines.
        let sub: Vec<(&str, bool)> = s["objectMenu"][1]["submenu"]
            .as_array()
            .expect("submenu")
            .iter()
            .map(|e| (e["label"].as_str().expect("label"), e["enabled"] == true))
            .collect();
        assert_eq!(sub, [("Delete Selected Line", false), ("Delete Lines Not Selected", false)]);
        // With direction lines on the shape, Delete Lines Not Selected removes them all (none
        // of those can be selected yet), one undo step; Delete Selected Line stays grey.
        app.execute(&CommandRequest::new(
            "object.setDirectionLines",
            json!({ "id": 1, "lines": [[[12, 12], [12, 18]], [[28, 12], [28, 18]]] }),
        ))
        .expect("direction lines");
        let sub = direction_submenu(&app);
        assert_eq!((sub[0].2, sub[1].2), (false, true));
        // Through the command, as a script or the Assistant runs it (#1269); a curved
        // direction is set too, so that both kinds go in the one step.
        app.execute(&CommandRequest::new(
            "object.setDirectionCurve",
            json!({ "id": 1, "points": [[11, 15], [20, 13], [29, 15]] }),
        ));
        let had_curve = app.session.design().objects[0]
            .region
            .as_ref()
            .is_some_and(|r| r.direction_curve.is_some());
        let had_lines = app.session.design().objects[0]
            .region
            .as_ref()
            .map_or(0, |r| r.direction_lines.len());
        let e = app
            .ui_command("ui.pointMenu", Some(&json!({ "action": "deleteSelectedLine" })))
            .expect_err("grey");
        assert!(e.contains("not available") && e.contains("deleteLinesNotSelected"), "{e}");
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "action": "deleteLinesNotSelected" })))
            .expect("Delete Lines Not Selected");
        assert_eq!(s["objectMenu"][1]["submenu"][1]["enabled"], false, "nothing left to delete");
        let region = app.session.design().objects[0].region.clone().expect("region");
        assert!(region.direction_lines.is_empty() && region.direction_curve.is_none());
        assert!(!direction_submenu(&app)[1].2);
        let e = app
            .ui_command("ui.pointMenu", Some(&json!({ "action": "deleteLinesNotSelected" })))
            .expect_err("grey now");
        assert!(e.contains("no direction lines"), "{e}");
        app.undo();
        let region = app.session.design().objects[0].region.clone().expect("region");
        assert_eq!(
            (region.direction_lines.len(), region.direction_curve.is_some()),
            (had_lines, had_curve),
            "one Undo brings back what was there"
        );
        assert!(had_lines == 2 || had_curve, "the shape had direction lines to delete");
        // The ribbon's Delete Direction Line (`ui.run deleteDirection`) does the same, and
        // its ui.actions entry names the command it sends.
        app.ui_command("ui.run", Some(&json!({ "action": "deleteDirection" })))
            .expect("ui.run deleteDirection");
        let region = app.session.design().objects[0].region.clone().expect("region");
        assert!(region.direction_lines.is_empty() && region.direction_curve.is_none());
        let info = Action::DeleteDirectionLine.info();
        assert_eq!(info.engine, Some("object.setDirectionLines"));
        assert!(info.tip.contains("object.setDirectionCurve") && info.tip.contains("one undo step"));
        // The summary a model reads says what `submenu` is and how to run its entries.
        let summary = openstitch_app::editor::assistant::UI_COMMANDS
            .iter()
            .find(|c| c.id == "ui.pointMenu")
            .expect("ui.pointMenu")
            .summary;
        for words in ["submenu", "deleteSelectedLine", "deleteLinesNotSelected", "key, label and enabled"] {
            assert!(summary.contains(words), "ui.pointMenu's summary names {words:?}");
        }
    }

    /// The object's menu (#1260): PE-DESIGN's entries in its order, a separator before Open;
    /// a closed shape offers Open and the direction lines, an open line only Close. Close
    /// gives the line a region, Open takes it away again.
    #[test]
    fn the_object_menu_opens_and_closes_the_outline() {
        let (mut app, _) = with_rectangle();
        let entries = |app: &mut EditorApp| {
            let s = app.ui_command("ui.pointMenu", None).expect("state");
            s["objectMenu"]
                .as_array()
                .expect("objectMenu")
                .iter()
                .map(|e| {
                    (
                        e["label"].as_str().expect("label").to_string(),
                        e["separatorBefore"] == true,
                        e["enabled"] == true,
                    )
                })
                .collect::<Vec<_>>()
        };
        let closed = entries(&mut app);
        assert_eq!(
            closed.iter().map(|e| (e.0.as_str(), e.1)).collect::<Vec<_>>(),
            [
                ("Add Direction Line", false),
                ("Delete Direction Line", false),
                ("Reverse Direction Line", false),
                ("Open", true),
                ("Close", false),
            ]
        );
        assert!(closed[0].2 && closed[2].2 && closed[3].2 && !closed[4].2, "{closed:?}");
        app.ui_command("ui.run", Some(&json!({ "action": "openOutline" })))
            .expect("Open");
        let o = &app.session.design().objects[0];
        assert!(!o.contours[0].closed && o.region.is_none() && o.line.is_some());
        let open = entries(&mut app);
        assert!(!open[0].2 && !open[2].2 && !open[3].2 && open[4].2, "{open:?}");
        app.ui_command("ui.run", Some(&json!({ "action": "closeOutline" })))
            .expect("Close");
        let o = &app.session.design().objects[0];
        assert!(o.contours[0].closed && o.region.is_some());
        app.undo();
        app.undo();
        let o = &app.session.design().objects[0];
        assert!(o.contours[0].closed && o.region.is_some(), "one Undo each");
    }

    /// PE-DESIGN 11.50's object menu, read from its screen on 2026-11-18 (results of the
    /// second capture round, corrected the same evening): on a closed shape with a region Add,
    /// Delete and Reverse Direction Line and Open are available and Close is grey; on an open
    /// line Add and Reverse Direction Line and Open are grey, Delete Direction Line (which
    /// only opens its sub-menu) and Close are available. A shape sewn as a region only (what the
    /// Rectangle tool draws) has the same menu: Open and Split at Point make it a line.
    #[test]
    fn the_object_menu_is_greyed_as_pe_designs_also_on_a_region_only_shape() {
        let states = |app: &mut EditorApp| {
            let s = app.ui_command("ui.pointMenu", None).expect("state");
            s["objectMenu"]
                .as_array()
                .expect("objectMenu")
                .iter()
                .map(|e| (e["key"].as_str().expect("key").to_string(), e["enabled"] == true))
                .collect::<Vec<_>>()
        };
        let table = |on: [bool; 5]| {
            ["addDirection", "deleteDirection", "reverseDirection", "openOutline", "closeOutline"]
                .into_iter()
                .map(str::to_string)
                .zip(on)
                .collect::<Vec<_>>()
        };
        // Closed, region and line.
        let (mut app, _) = with_rectangle();
        assert_eq!(states(&mut app), table([true, true, true, true, false]));
        // The open line it becomes.
        app.ui_command("ui.run", Some(&json!({ "action": "openOutline" })))
            .expect("Open");
        assert_eq!(states(&mut app), table([false, true, false, false, true]));

        // Closed, sewn as a region only: the same menu, and nothing is refused.
        let region_only = || {
            let mut app = EditorApp::new(Launch::default());
            let id = app
                .execute(&CommandRequest::new(
                    "shape.rectangle",
                    json!({ "x": 10, "y": 10, "width": 20, "height": 10 }),
                ))
                .expect("rectangle")["id"]
                .as_u64()
                .expect("id");
            app.selection = BTreeSet::from([id]);
            assert!(app.session.design().objects[0].line.is_none());
            app
        };
        let mut app = region_only();
        assert_eq!(states(&mut app), table([true, true, true, true, false]));
        let thread = app.session.design().objects[0].region.as_ref().expect("region").thread.clone();
        app.ui_command("ui.run", Some(&json!({ "action": "openOutline" })))
            .expect("Open");
        let o = app.session.design().objects[0].clone();
        assert!(!o.contours[0].closed && o.region.is_none());
        assert_eq!(o.line.as_ref().map(|l| &l.thread), Some(&thread), "a line in the region's thread");
        app.undo();
        let o = &app.session.design().objects[0];
        assert!(o.contours[0].closed && o.region.is_some() && o.line.is_none(), "one Undo");
        // Split at Point is available on its corner and does the same.
        let mut app = region_only();
        let s = app
            .ui_command("ui.pointMenu", Some(&json!({ "points": [0] })))
            .expect("select a point");
        assert_eq!(s["entries"][5]["enabled"], true, "Split at Point");
        assert!(enabled(&app, Action::SplitAtPoint));
        app.ui_command("ui.pointMenu", Some(&json!({ "action": "splitAtPoint" })))
            .expect("Split at Point");
        let o = &app.session.design().objects[0];
        assert!(!o.contours[0].closed && o.region.is_none() && o.line.is_some());
        assert!(Action::OpenOutline.info().tip.contains("running-stitch line"));
        assert!(Action::SplitAtPoint.info().tip.contains("running-stitch line"));
    }

    /// Split at Point on a stitch pattern still splits it at the selected needle point
    /// (`stitches.splitAt`), as before the outline's Split at Point shared the button.
    #[test]
    fn split_at_point_still_splits_a_stitch_pattern_at_the_selected_needle() {
        let (mut app, id) = with_rectangle();
        app.execute(&CommandRequest::new(
            "object.toStitches",
            json!({ "ids": [id] }),
        ))
        .expect("stitches");
        app.select_all();
        let before = app.session.design().objects.len();
        assert!(!enabled(&app, Action::SplitAtPoint), "no needle point yet");
        app.ui_command(
            "ui.stitchTable",
            Some(&json!({ "open": true, "select": [4] })),
        )
        .expect("select a needle point");
        assert!(states(&app).is_none(), "no outline point is selected");
        assert!(enabled(&app, Action::SplitAtPoint));
        assert!(!enabled(&app, Action::EditPoint));
        app.ui_command("ui.run", Some(&json!({ "action": "splitAtPoint" })))
            .expect("split");
        assert_eq!(app.session.design().objects.len(), before + 1);
    }
}
