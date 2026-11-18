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
    });
    hit || object
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
    let r = egui::Area::new(egui::Id::unique("point-context-menu"))
        .fixed_pos(m.at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(150.0);
                for (i, a) in PointAction::MENU.into_iter().enumerate() {
                    if a.separator_before() {
                        ui.separator();
                    }
                    if ui
                        .add_enabled(enabled[i], egui::Button::new(label(a)))
                        .clicked()
                    {
                        chosen = Some(a);
                    }
                }
            });
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
/// Close; a closed shape offers Open).
fn object_window(app: &mut EditorApp, ctx: &egui::Context, mut m: PointMenu) {
    use openstitch_app::editor::ribbon::POINT_OBJECT_MENU;
    let mut chosen = false;
    let r = egui::Area::new(egui::Id::unique("point-object-menu"))
        .fixed_pos(m.at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);
                for a in POINT_OBJECT_MENU {
                    if a == Action::OpenOutline {
                        ui.separator();
                    }
                    let enabled = crate::ribbon::enabled(app, a);
                    if ui
                        .add_enabled(enabled, egui::Button::new(a.info().label))
                        .clicked()
                    {
                        crate::ribbon::run(app, a, ui.ctx());
                        chosen = true;
                    }
                }
            });
        });
    let (pressed, esc) = ctx.input(|i| (i.pointer.any_pressed(), i.key_pressed(egui::Key::Escape)));
    let outside = pressed && !r.response.contains_pointer();
    if !chosen && !esc && (m.fresh || !outside) {
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
                "enabled": crate::ribbon::enabled(app, a),
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
            let at = *ctx.point_target()?.contours.get(p.contour)?.points.get(p.index)?;
            let screen = app.viewport.map(|vp| vp.to_screen(at));
            Some(json!({
                "contour": p.contour,
                "index": p.index,
                "x": at.x / 10.0,
                "y": at.y / 10.0,
                "screen": screen.map(|s| [s.x, s.y]),
            }))
        })
        .collect();
    let point_count = ctx
        .point_target()
        .and_then(|o| o.contours.first())
        .map(|c| c.points.len());
    json!({
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
        for a in [Action::EditPoint, Action::SplitAtPoint, Action::PointToCurve] {
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
