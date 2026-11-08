//! The editor side of `batch:editor-tools`: Align and Distribute from the ribbon and `ui.run`
//! (#472), Snap to Objects through a `--script` drag on the real canvas (#473), the precision
//! bar and the nudge step (#474), Select Same, Alt-click and Click Inside Outlines (#475). A
//! child module of `app` so it drives the private frame steps as `update` does.

use super::*;
use openstitch_app::editor::precision::Field;
use openstitch_app::editor::{Phase, PointerEvent};
use openstitch_design::{Rect, v2};

fn rect(app: &mut EditorApp, x: f64, y: f64, w: f64, h: f64, color: &str) -> u64 {
    let r = app
        .execute(&CommandRequest::new(
            "shape.rectangle",
            json!({ "x": x, "y": y, "width": w, "height": h, "fill": color }),
        ))
        .expect("rectangle");
    r["id"].as_u64().expect("id")
}

fn bounds(app: &EditorApp, id: u64) -> Rect {
    let d = app.session.design();
    d.objects[d.index_of(id).expect("object")]
        .bounds()
        .expect("bounds")
}

fn ui_run(app: &mut EditorApp, key: &str) -> Result<Value, String> {
    app.ui_command("ui.run", Some(&json!({ "action": key })))
}

/// Plays a `--script` on the canvas as the window does: the canvas lays out and fits the
/// page, then each step's events arrive frame by frame through egui's raw input.
fn play(app: &mut EditorApp, script: &str) {
    let ctx = egui::Context::default();
    let frame = |app: &mut EditorApp, events: Vec<egui::Event>, m: Modifiers| {
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 800.0),
            )),
            ..Default::default()
        };
        raw.events.push(egui::Event::ModifiersChanged(m));
        raw.events.extend(events);
        let mut out = ctx.run_ui(raw, |ui| crate::canvas::canvas(app, ui));
        out.textures_delta.clear();
    };
    for _ in 0..4 {
        frame(app, Vec::new(), Modifiers::NONE);
    }
    let mut player = crate::script::Player::new(crate::script::parse(script).expect("script"));
    while let Some((events, m)) = player.next_frame(&app.viewport.expect("viewport")) {
        frame(app, events, m);
    }
    frame(app, Vec::new(), Modifiers::NONE);
}

#[test]
fn align_and_distribute_run_from_the_ribbon_and_ui_run() {
    let mut app = EditorApp::new(Launch::default());
    let a = rect(&mut app, 10.0, 10.0, 20.0, 10.0, "Red");
    let b = rect(&mut app, 40.0, 30.0, 10.0, 20.0, "Red");
    let c = rect(&mut app, 70.0, 5.0, 15.0, 15.0, "Blue");
    app.selection = BTreeSet::from([a]);
    assert!(
        ui_run(&mut app, "alignLeft").is_err(),
        "one object: disabled"
    );
    assert!(ui_run(&mut app, "distributeHorizontal").is_err());
    app.selection = BTreeSet::from([a, b, c]);
    ui_run(&mut app, "alignTop").expect("align top");
    for id in [a, b, c] {
        assert_eq!(bounds(&app, id).min_y, 50.0, "tops on the selection's top");
    }
    ui_run(&mut app, "distributeHorizontal").expect("distribute");
    let (ba, bb, bc) = (bounds(&app, a), bounds(&app, b), bounds(&app, c));
    assert!(((bb.min_x - ba.max_x) - (bc.min_x - bb.max_x)).abs() < 0.1);
    // One undo per command.
    app.undo();
    assert_eq!(bounds(&app, b).min_x, 400.0);
    app.undo();
    assert_eq!(bounds(&app, b).min_y, 300.0);
    // Relative to the Design Page, through the ui.set setting.
    app.ui_command(
        "ui.set",
        Some(&json!({ "name": "alignTo", "value": "page" })),
    )
    .expect("alignTo");
    app.selection = BTreeSet::from([c]);
    ui_run(&mut app, "alignRight").expect("one object to the page");
    assert_eq!(bounds(&app, c).max_x, app.session.design().page.width);
    // The menu itself is a list of choices.
    assert!(ui_run(&mut app, "align").is_err());
}

#[test]
fn align_to_last_selected_keeps_the_object_clicked_last() {
    let mut app = EditorApp::new(Launch::default());
    let a = rect(&mut app, 10.0, 10.0, 20.0, 10.0, "Red");
    let b = rect(&mut app, 50.0, 40.0, 10.0, 20.0, "Red");
    let click = |app: &mut EditorApp, at, shift| {
        let mut e = PointerEvent::new(Phase::Down, at);
        e.shift = shift;
        app.with_tools(|t, c| t.handle(e, c));
        app.with_tools(|t, c| t.handle(PointerEvent::new(Phase::Up, at), c));
    };
    click(&mut app, v2(550.0, 500.0), false);
    click(&mut app, v2(200.0, 150.0), true);
    assert_eq!(app.selection, BTreeSet::from([a, b]));
    assert_eq!(app.key_object, Some(a), "a was selected last");
    app.align_to = "key";
    let key = bounds(&app, a);
    crate::ribbon::run(
        &mut app,
        openstitch_app::editor::ribbon::Action::AlignBottom,
        &egui::Context::default(),
    );
    assert_eq!(bounds(&app, a), key, "the key object stays put");
    assert_eq!(bounds(&app, b).max_y, key.max_y);
}

#[test]
fn snap_to_objects_snaps_a_scripted_drag_exactly_and_ctrl_drags_freely() {
    for (snap, script, expect) in [
        (true, "drag:55,40,35.1,40", Some(300.0)),
        (false, "drag:55,40,35.1,40", None),
        (true, "cmd+drag:55,40,35.1,40", None),
    ] {
        let mut app = EditorApp::new(Launch::default());
        rect(&mut app, 10.0, 10.0, 20.0, 10.0, "Red");
        let b = rect(&mut app, 50.0, 30.0, 10.0, 20.0, "Blue");
        app.ui.prefs.editing.snap_objects = snap;
        play(&mut app, script);
        let left = bounds(&app, b).min_x;
        match expect {
            // B's left edge lands exactly on A's right edge (0.0 mm difference).
            Some(x) => assert_eq!(left, x, "{script}"),
            None => assert!(
                (left - 301.0).abs() < 1.0 && left != 300.0,
                "{script}: {left}"
            ),
        }
        assert!(
            app.tools.guides().is_empty(),
            "guides go when the drag ends"
        );
        // One undo step per drag.
        app.undo();
        assert_eq!(bounds(&app, b).min_x, 500.0);
    }
}

/// #799: with Snap to Objects off, a dragged selection still snaps to a guideline while they
/// show and snap to guides is on (the default); off, or with guidelines hidden, it does not.
#[test]
fn a_drag_snaps_to_a_guideline_with_snap_to_guides() {
    for (snap, shown, expect) in [(true, true, true), (false, true, false), (true, false, false)] {
        let mut app = EditorApp::new(Launch::default());
        let b = rect(&mut app, 50.0, 30.0, 10.0, 20.0, "Blue");
        app.execute(&openstitch_app::editor::CommandRequest::new(
            "guides.set",
            serde_json::json!({ "vertical": [30] }),
        ))
        .expect("guide");
        app.ui.prefs.editing.snap_objects = false;
        app.ui.prefs.editing.snap_guides = snap;
        app.workspace.guides = shown;
        play(&mut app, "drag:55,40,35.1,40");
        let left = bounds(&app, b).min_x;
        if expect {
            assert_eq!(left, 300.0, "on the guideline");
        } else {
            assert!((left - 301.0).abs() < 1.0 && left != 300.0, "{snap} {shown}: {left}");
        }
    }
}

#[test]
fn snap_guides_show_while_dragging_and_object_snap_beats_the_grid() {
    let mut app = EditorApp::new(Launch::default());
    rect(&mut app, 10.0, 10.0, 20.0, 10.0, "Red");
    let b = rect(&mut app, 50.0, 32.0, 10.0, 20.0, "Blue");
    app.ui.prefs.editing.snap_objects = true;
    app.workspace.grid.snap = true;
    app.workspace.grid.spacing_mm = 7.0;
    app.selection = BTreeSet::from([b]);
    let drag = |app: &mut EditorApp, phase, at| {
        app.with_tools(|t, c| t.handle(PointerEvent::new(phase, at), c));
    };
    drag(&mut app, Phase::Down, v2(550.0, 400.0));
    drag(&mut app, Phase::Dragged, v2(351.0, 400.0));
    use openstitch_app::editor::snap::SnapGuide;
    assert!(app.tools.guides().contains(&SnapGuide::Vertical(300.0)));
    drag(&mut app, Phase::Up, v2(351.0, 400.0));
    assert_eq!(
        bounds(&app, b).min_x,
        300.0,
        "the object, not the 7 mm grid"
    );
    assert_eq!(
        bounds(&app, b).min_y % 70.0,
        0.0,
        "y had no object in reach: grid"
    );
}

#[test]
fn nudge_step_and_grid_step() {
    let ctx = egui::Context::default();
    let mut app = EditorApp::new(Launch::default());
    let a = rect(&mut app, 10.0, 10.0, 20.0, 10.0, "Red");
    app.selection = BTreeSet::from([a]);
    let press = |app: &mut EditorApp, k, m| {
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::ModifiersChanged(m));
        raw.events.push(egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: m,
        });
        let mut out = ctx.run_ui(raw, |ui| app.shortcuts(ui.ctx()));
        out.textures_delta.clear();
    };
    press(&mut app, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(bounds(&app, a).min_x, 101.0, "0.1 mm by default, as today");
    app.ui.prefs.editing.nudge_mm = 0.5;
    press(&mut app, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(bounds(&app, a).min_x, 106.0);
    press(&mut app, Key::ArrowLeft, Modifiers::SHIFT);
    assert_eq!(bounds(&app, a).min_x, 56.0, "Shift: ten times");
    // Grid-step nudge needs both the option and Snap to Grid.
    app.ui.prefs.editing.nudge_grid = true;
    press(&mut app, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(bounds(&app, a).min_y, 105.0, "snap off: the nudge step");
    app.workspace.grid.snap = true;
    app.workspace.grid.spacing_mm = 2.5;
    press(&mut app, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(bounds(&app, a).min_y, 130.0, "one grid step");
}

#[test]
fn precision_bar_commits_one_undo_step_per_value() {
    let mut app = EditorApp::new(Launch::default());
    let a = rect(&mut app, 10.0, 10.0, 20.0, 10.0, "Red");
    app.selection = BTreeSet::from([a]);
    crate::arrange_ui::commit(&mut app, Field::Width, 80.0);
    let b = bounds(&app, a);
    assert!((b.width() - 800.0).abs() < 0.05 && (b.height() - 400.0).abs() < 0.05);
    // X counts from the ruler origin: the page's centre by default (#799).
    crate::arrange_ui::commit(&mut app, Field::X, 5.0);
    assert_eq!(bounds(&app, a).min_x, app.ruler_zero().x + 50.0);
    crate::arrange_ui::commit(&mut app, Field::Skew, 15.0);
    app.undo();
    app.undo();
    assert_eq!(bounds(&app, a), b);
    app.undo();
    assert_eq!(bounds(&app, a).width(), 200.0);
    // The bar is a View toggle kept in ui.json, reachable through ui.run.
    ui_run(&mut app, "precisionBar").expect("toggle");
    assert!(app.ui.prefs.editing.precision_bar);
    ui_run(&mut app, "snapToObjects").expect("toggle");
    assert!(app.ui.prefs.editing.snap_objects);
}

#[test]
fn select_same_alt_click_and_click_inside() {
    let mut app = EditorApp::new(Launch::default());
    let a = rect(&mut app, 10.0, 10.0, 10.0, 10.0, "Red");
    let b = rect(&mut app, 30.0, 10.0, 10.0, 10.0, "Red");
    let c = rect(&mut app, 50.0, 10.0, 10.0, 10.0, "Blue");
    app.execute(&CommandRequest::new(
        "object.setRegion",
        json!({ "id": b, "type": "satin" }),
    ));
    app.selection = BTreeSet::from([a]);
    ui_run(&mut app, "selectSameColor").expect("same colour");
    assert_eq!(app.selection, BTreeSet::from([a, b]));
    app.selection = BTreeSet::from([a]);
    ui_run(&mut app, "selectSameType").expect("same type");
    assert_eq!(app.selection, BTreeSet::from([a, c]));
    // The menu and object.list agree.
    let listed = app
        .execute(&CommandRequest::new(
            "object.list",
            json!({ "sameAs": [a], "match": "type", "visible": true }),
        ))
        .expect("list");
    assert_eq!(listed.as_array().map(Vec::len), Some(2));

    // Alt-click on a stitch pattern selects every pattern sewing the colour clicked.
    for id in [a, b, c] {
        app.execute(&CommandRequest::new(
            "object.toStitches",
            json!({ "ids": [id] }),
        ))
        .expect("to stitches");
    }
    let d = app.session.design();
    assert_eq!(d.objects.len(), 3);
    let blue = d
        .objects
        .iter()
        .find(|o| o.bounds().is_some_and(|b| b.min_x >= 490.0))
        .expect("blue")
        .id;
    app.selection.clear();
    let mut e = PointerEvent::new(Phase::Down, v2(150.0, 150.0));
    e.alt = true;
    app.with_tools(|t, c| t.handle(e, c));
    app.with_tools(|t, c| t.handle(PointerEvent::new(Phase::Up, v2(150.0, 150.0)), c));
    assert_eq!(app.selection.len(), 2, "the red ones");
    assert!(!app.selection.contains(&blue));

    // Click inside an outline-only circle: only with the preference on.
    let mut app = EditorApp::new(Launch::default());
    app.execute(&CommandRequest::new(
        "shape.ellipse",
        json!({ "cx": 50, "cy": 50, "rx": 10, "outline": "Red" }),
    ))
    .expect("circle");
    let click = |app: &mut EditorApp| {
        app.selection.clear();
        for phase in [Phase::Down, Phase::Up] {
            app.with_tools(|t, c| t.handle(PointerEvent::new(phase, v2(500.0, 500.0)), c));
        }
        app.selection.len()
    };
    assert_eq!(
        click(&mut app),
        0,
        "off by default: click the line, as in PE-DESIGN"
    );
    app.ui.prefs.editing.click_inside = true;
    assert_eq!(click(&mut app), 1);
}

/// Text Attributes ▸ Font list (#527): browsing fonts with the arrow keys is one Undo.
#[test]
fn a_font_browse_is_undone_in_one_step() {
    let mut app = EditorApp::new(Launch::default());
    let r = app
        .execute(&CommandRequest::new("text.add", json!({ "text": "Abc" })))
        .expect("text");
    let id = r["id"].as_u64().expect("id");
    app.selection = BTreeSet::from([id]);
    let font = |app: &EditorApp| {
        let d = app.session.design();
        d.objects[d.index_of(id).expect("object")]
            .text
            .as_ref()
            .expect("text")
            .font
            .clone()
    };
    for f in ["pacifico", "openstitch-line", "openstitch-pixel"] {
        crate::lettering_ui::browse_to(&mut app, f);
    }
    assert_eq!(font(&app), "openstitch-pixel");
    app.undo();
    assert_eq!(
        font(&app),
        "lato-bold",
        "one Undo takes the whole browse back"
    );
}
