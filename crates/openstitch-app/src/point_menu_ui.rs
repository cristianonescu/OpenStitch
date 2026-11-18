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
    let (mut hit, mut object) = (false, false);
    app.with_tools(|tools, ctx| {
        hit = tools.select_point_at(model, ctx);
        // Not on a point: the object's menu, as PE-DESIGN opens it.
        object = !hit && tools.object_menu_at(model, ctx);
        vec![]
    });
    app.workspace.view.point_menu = (hit || object).then_some(PointMenu {
        at,
        fresh: true,
        object,
        sub: false,
    });
    hit || object
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

/// The Delete Direction Line sub-menu, PE-DESIGN's two entries: (key, text id, available).
/// Built from their names (what they do with lines present is not captured): Delete Selected
/// Line removes the selected direction lines, and no direction line can be selected on the
/// canvas here, so it has nothing to act on; Delete Lines Not Selected removes the others,
/// which is all of them: the shape goes back to one straight sewing direction
/// (`object.setDirectionLines` with none). Both are grey on a shape without direction lines.
pub fn direction_submenu(app: &EditorApp) -> [(&'static str, &'static str, bool); 2] {
    let ctx = app.tool_context(app.session.design());
    let has_lines = ctx
        .point_target()
        .and_then(|o| o.region.as_ref())
        .is_some_and(|r| r.direction_curve.is_some() || !r.direction_lines.is_empty());
    [
        ("deleteSelectedLine", "menu-point-delete-selected-line", false),
        (
            "deleteLinesNotSelected",
            "menu-point-delete-lines-not-selected",
            has_lines && crate::ribbon::enabled(app, Action::DeleteDirectionLine),
        ),
    ]
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

/// A menu panel at `at` (the pointer: the menu opens to the right of and below it, as
/// PE-DESIGN's), opaque from its first frame.
fn panel<R>(
    ctx: &egui::Context,
    id: &'static str,
    at: Pos2,
    width: f32,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Area::new(egui::Id::unique(id))
        .fixed_pos(at)
        .fade_in(false)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style())
                .show(ui, |ui| {
                    ui.set_min_width(width);
                    ui.set_max_width(width);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    body(ui)
                })
                .inner
        })
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
    let Some(enabled) = states(app) else {
        return;
    };
    let mut chosen = None;
    let r = panel(ctx, "point-context-menu", m.at, 170.0, |ui| {
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
    let mut sub_at = None;
    let r = panel(ctx, "point-object-menu", m.at, 210.0, |ui| {
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
        let sub = panel(ctx, "point-object-submenu", at, 200.0, |ui| {
            for (_, text, enabled) in entries {
                if row(ui, None, t(text), enabled, false).clicked() {
                    chosen = Some(Action::DeleteDirectionLine);
                }
            }
        });
        over_sub = sub.response.contains_pointer();
    }
    if let Some(a) = chosen {
        crate::ribbon::run(app, a, ctx);
        return;
    }
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
    json!({
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

/// `ui.pointMenu`: `{ points, contour }` selects vertices with the Point tool; `{ action }` runs one entry on the selected points, as a click in the menu
/// does; without `action` it reports the menu ([`state`]).
pub fn command(app: &mut EditorApp, params: Option<&Value>) -> Result<Value, String> {
    if let Some(list) = params.and_then(|p| p.get("points")) {
        let indexes: Vec<usize> = list
            .as_array()
            .and_then(|l| {
                l.iter()
                    .map(|v| v.as_u64().map(|i| i as usize))
                    .collect::<Option<_>>()
            })
            .ok_or("points: must be a list of vertex indexes")?;
        let contour = params
            .and_then(|p| p.get("contour"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        app.tools.set_tool(openstitch_app::editor::Tool::Point);
        let mut ok = false;
        app.with_tools(|tools, ctx| {
            ok = tools.select_points(contour, &indexes, ctx);
            vec![]
        });
        if !ok {
            return Err(
                "points: select exactly one unlocked shape first, and name vertices it has"
                    .into(),
            );
        }
    }
    let Some(key) = params.and_then(|p| p.get("action")).and_then(Value::as_str) else {
        return Ok(state(app));
    };
    let a = PointAction::MENU
        .into_iter()
        .find(|a| a.key() == key)
        .ok_or_else(|| {
            format!(
                "action: must be one of {}",
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
        // can be selected here), one undo step; Delete Selected Line has nothing to act on.
        app.execute(&CommandRequest::new(
            "object.setDirectionLines",
            json!({ "id": 1, "lines": [[[12, 12], [12, 18]], [[28, 12], [28, 18]]] }),
        ))
        .expect("direction lines");
        let sub = direction_submenu(&app);
        assert_eq!((sub[0].2, sub[1].2), (false, true));
        crate::ribbon::run(&mut app, Action::DeleteDirectionLine, &egui::Context::default());
        let region = app.session.design().objects[0].region.clone().expect("region");
        assert!(region.direction_lines.is_empty() && region.direction_curve.is_none());
        assert!(!direction_submenu(&app)[1].2);
        app.undo();
        assert_eq!(
            app.session.design().objects[0].region.as_ref().map(|r| r.direction_lines.len()),
            Some(2),
            "one Undo"
        );
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
