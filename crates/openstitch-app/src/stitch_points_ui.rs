//! The Point tool on a stitch pattern from the keyboard and from `ui.stitchPoints` (#1272):
//! PE-DESIGN's Select Point keys (Tab, Shift+Tab, Home, End, Ctrl+Home, Ctrl+End; the arrows
//! are in `app.rs` with the shape's), and the same as a command for the Assistant and scripts.

/// What an arrow key moves the selected needle points by, 0.1 mm units: 0.1 mm, PE-DESIGN
/// 11.50's (capture k5: one Right press moved a point from 8.0 to 8.1 mm).
pub const NEEDLE_NUDGE: f64 = 1.0;

use crate::app::EditorApp;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use openstitch_app::editor::{CommandRequest, Tool};
use serde_json::{Value, json};

/// Whether the Point tool has needle points of a stitch pattern selected.
pub fn has_selection(app: &EditorApp) -> bool {
    let ctx = app.tool_context(app.session.design());
    app.tools.tool() == Tool::Point && !app.tools.selected_needles(&ctx).is_empty()
}

/// Whether the Point tool shows a stitch pattern's needle points.
pub fn shows(app: &EditorApp) -> bool {
    let ctx = app.tool_context(app.session.design());
    app.tools.needle_overlay(&ctx).is_some()
}

/// Tab, Shift+Tab, Home, End, Ctrl+Home and Ctrl+End with the Point tool on a stitch pattern.
/// The Stitch Simulator, while open, has taken Home and End before this runs; Tab is left to
/// a text field that has focus.
pub fn keys(app: &mut EditorApp, ctx: &egui::Context, take: &dyn Fn(KeyboardShortcut) -> bool) {
    if !shows(app) {
        return;
    }
    if !ctx.egui_wants_keyboard_input() {
        for mods in [Modifiers::SHIFT, Modifiers::NONE] {
            if take(KeyboardShortcut::new(mods, Key::Tab)) {
                app.with_tools(|tools, ctx| {
                    tools.tab_needles(ctx);
                    vec![]
                });
            }
        }
    }
    // The Stitch Simulator keeps Home and End (and Ctrl+Home, Ctrl+End) while it is open.
    if app.simulate.is_some() {
        return;
    }
    for (mods, key, exit, whole) in [
        (Modifiers::COMMAND, Key::Home, false, true),
        (Modifiers::COMMAND, Key::End, true, true),
        (Modifiers::NONE, Key::Home, false, false),
        (Modifiers::NONE, Key::End, true, false),
    ] {
        if take(KeyboardShortcut::new(mods, key)) {
            app.with_tools(|tools, ctx| {
                tools.select_needle_end(exit, whole, ctx);
                vec![]
            });
        }
    }
}

/// The Point tool's state on the selected stitch pattern, for `ui.stitchPoints`.
fn state(app: &EditorApp) -> Value {
    let ctx = app.tool_context(app.session.design());
    let Some(o) = ctx.needle_target() else {
        return json!({ "id": null, "note": "select one stitch pattern (ui.select) and the Point tool" });
    };
    let selected = app.tools.selected_needles(&ctx);
    let groups = openstitch_app::editor::tools::needles::color_groups(o);
    let screen = |p: openstitch_design::Vec2| {
        app.viewport
            .map(|vp| vp.to_screen(p))
            .map(|s| [s.x, s.y])
    };
    let connectable: Vec<Value> = app
        .tools
        .needle_connect_ends(&ctx)
        .into_iter()
        .map(|(id, p)| json!({ "id": id, "x": p.x / 10.0, "y": p.y / 10.0, "screen": screen(p) }))
        .collect();
    let all = openstitch_app::editor::tools::needles::needle_points(o);
    let at = |i: usize| all.iter().find(|q| q.0 == i).map(|q| q.1);
    let selected_at: Vec<Value> = selected
        .iter()
        .filter_map(|&i| at(i).map(|p| json!({ "index": i, "x": p.x / 10.0, "y": p.y / 10.0, "screen": screen(p) })))
        .collect();
    let overlap = app.tools.needle_connect_overlap().map(|p| {
        json!({ "x": p.x / 10.0, "y": p.y / 10.0, "screen": screen(p) })
    });
    json!({
        "id": o.id,
        "points": openstitch_app::editor::tools::needles::needle_points(o).len(),
        "selected": selected,
        "colors": groups.iter().map(|g| json!({
            "entry": g.first(), "exit": g.last(), "points": g.len(),
        })).collect::<Vec<_>>(),
        "selectedAt": selected_at,
        "connectable": connectable,
        "connectOverlap": overlap,
        "menu": needle_menu(app).iter().map(|(key, label, enabled)| json!({
            "key": key, "label": label, "enabled": enabled,
        })).collect::<Vec<_>>(),
        "menuOpen": app.workspace.view.point_menu.as_ref().is_some_and(|m| m.is_needle()),
        "hint": crate::point_menu_ui::hint(app),
    })
}

/// `ui.stitchPoints`: drives the Point tool on the one selected stitch pattern as its keys
/// and clicks do. Each key is one call; several in one call run in the order listed below.
pub fn command(app: &mut EditorApp, params: Option<&Value>) -> Result<Value, String> {
    let p = params.cloned().unwrap_or(Value::Null);
    if p.as_object().is_some_and(|m| !m.is_empty()) {
        app.tools.set_tool(Tool::Point);
    }
    if !shows(app) {
        if p.as_object().is_some_and(|m| !m.is_empty()) {
            return Err("ui.stitchPoints: select one stitch pattern first (ui.select)".into());
        }
        return Ok(state(app));
    }
    let id = {
        let ctx = app.tool_context(app.session.design());
        ctx.needle_target().map(|o| o.id).expect("shows")
    };
    if let Some(sel) = p.get("select") {
        let indexes: std::collections::BTreeSet<usize> = serde_json::from_value(sel.clone())
            .map_err(|_| "select is a list of needle point indexes".to_string())?;
        let all: Vec<usize> = {
            let ctx = app.tool_context(app.session.design());
            ctx.needle_target()
                .map(|o| {
                    openstitch_app::editor::tools::needles::needle_points(o)
                        .into_iter()
                        .map(|(i, ..)| i)
                        .collect()
                })
                .unwrap_or_default()
        };
        if let Some(bad) = indexes.iter().find(|i| !all.contains(i)) {
            return Err(format!("select: {bad} is not a needle point (stitches.points lists them)"));
        }
        app.tools.set_needle_selection(id, indexes);
    }
    if p.get("tab").and_then(Value::as_bool) == Some(true) {
        app.with_tools(|tools, ctx| {
            tools.tab_needles(ctx);
            vec![]
        });
    }
    if let Some(e) = p.get("end").and_then(Value::as_str) {
        let (exit, whole) = match e {
            "entry" => (false, false),
            "exit" => (true, false),
            "first" => (false, true),
            "last" => (true, true),
            _ => return Err("end: entry (Home), exit (End), first (Ctrl+Home) or last (Ctrl+End)".into()),
        };
        app.with_tools(|tools, ctx| {
            tools.select_needle_end(exit, whole, ctx);
            vec![]
        });
    }
    if let Some(st) = p.get("step").and_then(Value::as_str) {
        let forward = match st {
            "next" => true,
            "previous" => false,
            _ => return Err("step: next or previous".into()),
        };
        let extend = p.get("extend").and_then(Value::as_bool).unwrap_or(false);
        let mut done = false;
        app.with_tools(|tools, ctx| {
            done = tools.step_needles(forward, extend, ctx);
            vec![]
        });
        if !done {
            return Err("step: no needle point is selected".into());
        }
    }
    if let Some(to) = p.get("connectTo").and_then(Value::as_u64) {
        let ctx = app.tool_context(app.session.design());
        let ends = app.tools.needle_connect_ends(&ctx);
        let o = ctx.needle_target().expect("shows");
        let selected = app.tools.selected_needles(&ctx);
        let end = selected
            .first()
            .filter(|_| selected.len() == 1)
            .and_then(|&i| openstitch_app::editor::tools::needles::end_of(o, i));
        let Some(end) = end else {
            return Err("connectTo: select the pattern's entry or exit point alone first".into());
        };
        if !ends.iter().any(|(k, _)| *k == to) {
            return Err(format!("connectTo: object {to} is not a stitch pattern that end can connect to (connectable lists them)"));
        }
        let first = if end == openstitch_app::editor::tools::needles::End::Exit {
            id
        } else {
            to
        };
        if app
            .execute(&CommandRequest::new(
                "stitches.connect",
                json!({ "ids": [id, to], "first": first }),
            ))
            .is_none()
        {
            return Err(app.status.clone());
        }
    }
    if let Some(a) = p.get("action").and_then(Value::as_str) {
        menu_entry(app, a)?;
    }
    Ok(state(app))
}

/// Runs an entry of the needle point menu (PE-DESIGN's: Split at Point, Delete) on the
/// selected needle points, as a click on it does; refused when it is greyed out.
pub fn menu_entry(app: &mut EditorApp, key: &str) -> Result<(), String> {
    let enabled = needle_menu(app);
    let Some((_, _, on)) = enabled.iter().find(|e| e.0 == key) else {
        return Err("action: splitAtPoint or deletePoint".into());
    };
    if !on {
        return Err(format!("{key} is not available: select a needle point first"));
    }
    app.workspace.view.point_menu = None;
    match key {
        "splitAtPoint" => crate::convert_ui::split_at_point(app),
        _ => app.with_tools(|tools, ctx| tools.delete_point(ctx).unwrap_or_default()),
    }
    Ok(())
}

/// The needle point menu's entries (key, text, available): Split at Point with one needle
/// point selected (the entry and exit point too, as PE-DESIGN shows it, capture k12),
/// Delete with any.
pub fn needle_menu(app: &EditorApp) -> [(&'static str, &'static str, bool); 2] {
    let ctx = app.tool_context(app.session.design());
    let n = app.tools.selected_needles(&ctx).len();
    [
        (
            "splitAtPoint",
            crate::point_menu_ui::label(openstitch_app::editor::points::PointAction::Split),
            n == 1,
        ),
        (
            "deletePoint",
            crate::point_menu_ui::label(openstitch_app::editor::points::PointAction::Delete),
            n >= 1,
        ),
    ]
}
