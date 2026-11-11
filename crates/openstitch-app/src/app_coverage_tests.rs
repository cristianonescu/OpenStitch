//! Coverage for `EditorApp` behaviour FEATURE_MATRIX claims that no other test asserts (#332):
//! File ▸ Import into design, the undo/redo and arrow-nudge keys, the simulator's play, and the
//! window close request going through the unsaved-changes prompt; the Fit Text dialog
//! remembering its last choices (#351); Solid View's drawing (#378) and Snap to Grid for curve
//! handles and the Arrange Copies handle (#379). A child module of `app` so it can drive the
//! private frame steps (`shortcuts`, `tick`, `close_request`) as `update` does.

use super::*;

/// One frame with `events` and `modifiers` held, `dt` seconds after the last one.
fn frame(
    ctx: &egui::Context,
    app: &mut EditorApp,
    events: Vec<egui::Event>,
    modifiers: Modifiers,
    dt: f32,
    mut step: impl FnMut(&mut EditorApp, &egui::Context),
) {
    let mut raw = egui::RawInput {
        predicted_dt: dt,
        ..Default::default()
    };
    raw.events.push(egui::Event::ModifiersChanged(modifiers));
    raw.events.extend(events);
    let mut out = ctx.run_ui(raw, |ui| step(app, ui.ctx()));
    out.textures_delta.clear();
}

fn key(key: Key, modifiers: Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Presses `k` with `modifiers` and runs the editor's shortcut handling for that frame.
fn press(ctx: &egui::Context, app: &mut EditorApp, k: Key, modifiers: Modifiers) {
    frame(
        ctx,
        app,
        vec![key(k, modifiers)],
        modifiers,
        1.0 / 60.0,
        |app, ctx| app.shortcuts(ctx),
    );
}

fn rectangle(x: f64) -> CommandRequest {
    CommandRequest::new(
        "shape.rectangle",
        json!({ "x": x, "y": 10, "width": 20, "height": 20 }),
    )
}

#[test]
fn import_into_design_adds_the_file_centred_and_selected_beside_what_is_there() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../openstitch-engine/tests/fixtures/badge-golden.pes");
    let mut app = EditorApp::new(Launch::default());
    app.execute(&rectangle(5.0)).expect("rectangle");
    let kept = app.session.design().objects[0].clone();
    assert!(app.import(&fixture), "{}", app.status);
    let d = app.session.design();
    assert_eq!(d.objects.len(), 2, "added, not replacing");
    assert_eq!(d.objects[0], kept, "the existing object is untouched");
    let added = d.objects[1].clone();
    assert_eq!(added.kind, openstitch_design::ObjectKind::Stitches);
    assert_eq!(
        app.selection,
        BTreeSet::from([added.id]),
        "the import is selected"
    );
    let b = added.bounds().expect("bounds");
    let page = d.page.center();
    assert!(
        (b.center().x - page.x).abs() < 1.0 && (b.center().y - page.y).abs() < 1.0,
        "centred on the Design Page: {b:?} vs {page:?}"
    );
    assert!(app.is_dirty());
    assert!(!app.import(Path::new("/nonexistent/design.pes")));
    assert_eq!(
        app.session.design().objects.len(),
        2,
        "a bad file adds nothing"
    );
}

#[test]
fn undo_and_redo_keys() {
    let ctx = egui::Context::default();
    let mut app = EditorApp::new(Launch::default());
    app.execute(&rectangle(10.0)).expect("rectangle");
    let count = |app: &EditorApp| app.session.design().objects.len();
    assert_eq!(count(&app), 1);

    press(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
    assert_eq!(count(&app), 0, "Cmd/Ctrl+Z undoes");
    press(
        &ctx,
        &mut app,
        Key::Z,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert_eq!(count(&app), 1, "Shift+Cmd/Ctrl+Z redoes");
    press(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
    assert_eq!(count(&app), 0);
    press(&ctx, &mut app, Key::Y, Modifiers::COMMAND);
    assert_eq!(count(&app), 1, "Cmd/Ctrl+Y redoes");
    press(&ctx, &mut app, Key::Z, Modifiers::NONE);
    assert_eq!(count(&app), 1, "Z alone does not undo");
}

#[test]
fn changed_shortcuts_run_their_commands() {
    // #551: Undo moved to Alt+U; Ctrl+Z is given to nothing; Alt+R picks the Rectangle tool.
    use openstitch_app::editor::ribbon::Action as A;
    let ctx = egui::Context::default();
    let mut app = EditorApp::new(Launch::default());
    let keys = &mut app.ui.prefs.keymap;
    keys.set(A::Undo, &["Alt+U".into()]).expect("free");
    keys.add(A::Rectangle, "Alt+R").expect("free");
    app.execute(&rectangle(10.0)).expect("rectangle");
    let count = |app: &EditorApp| app.session.design().objects.len();
    press(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
    assert_eq!(count(&app), 1, "Ctrl+Z no longer undoes");
    press(&ctx, &mut app, Key::U, Modifiers::ALT);
    assert_eq!(count(&app), 0, "Alt+U undoes");
    press(&ctx, &mut app, Key::R, Modifiers::ALT);
    assert_eq!(app.tools.tool(), openstitch_app::editor::Tool::Rectangle);
    // While a shortcut is being recorded nothing runs.
    app.ui.keys.recording = Some(A::Save);
    press(&ctx, &mut app, Key::Y, Modifiers::COMMAND);
    assert_eq!(count(&app), 0, "Ctrl+Y waits for the recording");
}

#[test]
fn arrow_keys_nudge_the_selection_a_tenth_or_one_millimetre() {
    let ctx = egui::Context::default();
    let mut app = EditorApp::new(Launch::default());
    let r = app.execute(&rectangle(10.0)).expect("rectangle");
    app.selection = CommandRequest::created_ids(&r);
    let at = |app: &EditorApp| {
        let b = app.session.design().objects[0].bounds().expect("bounds");
        (b.min_x, b.min_y)
    };
    assert_eq!(at(&app), (100.0, 100.0));
    press(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(at(&app), (101.0, 100.0), "0.1 mm right");
    press(&ctx, &mut app, Key::ArrowDown, Modifiers::SHIFT);
    assert_eq!(at(&app), (101.0, 110.0), "Shift: 1 mm down");
    press(&ctx, &mut app, Key::ArrowLeft, Modifiers::NONE);
    press(&ctx, &mut app, Key::ArrowUp, Modifiers::SHIFT);
    assert_eq!(at(&app), (100.0, 100.0), "back");

    // Nothing selected: nothing moves.
    app.selection.clear();
    press(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(at(&app), (100.0, 100.0));
}

#[test]
fn play_advances_the_simulator_at_its_speed_and_stops_at_the_end() {
    let ctx = egui::Context::default();
    let mut app = EditorApp::new(Launch::default());
    app.execute(&rectangle(10.0)).expect("rectangle");
    app.refresh();
    let total = app.stats.stitches;
    assert!(total > 100, "{total}");
    app.playing = true;
    app.play_speed = 400.0;
    app.simulate = None;
    let mut seen = Vec::new();
    for _ in 0..10_000 {
        frame(
            &ctx,
            &mut app,
            Vec::new(),
            Modifiers::NONE,
            0.05,
            |app, ctx| app.tick(ctx),
        );
        if !app.playing {
            break;
        }
        seen.push(app.simulate.expect("playing shows a position"));
    }
    assert!(!app.playing, "play stops by itself");
    assert_eq!(app.simulate, None, "and shows the whole design again");
    assert!(seen.len() > 2, "{seen:?}");
    assert!(seen.windows(2).all(|w| w[1] > w[0]), "always forward");
    assert!(*seen.last().expect("last") < total);
    // Faster play covers more stitches per frame.
    let per_frame = |speed: f64| {
        let mut app = EditorApp::new(Launch::default());
        app.execute(&rectangle(10.0)).expect("rectangle");
        app.refresh();
        app.playing = true;
        app.play_speed = speed;
        for _ in 0..5 {
            frame(
                &ctx,
                &mut app,
                Vec::new(),
                Modifiers::NONE,
                0.05,
                |app, ctx| app.tick(ctx),
            );
        }
        app.simulate.unwrap_or(0)
    };
    assert!(per_frame(800.0) > per_frame(200.0));
}

#[test]
fn closing_the_window_with_unsaved_changes_opens_the_prompt() {
    let ctx = egui::Context::default();
    let close = |app: &mut EditorApp| {
        let mut raw = egui::RawInput::default();
        raw.viewports.insert(
            egui::ViewportId::ROOT,
            egui::ViewportInfo {
                events: vec![egui::ViewportEvent::Close],
                ..Default::default()
            },
        );
        let mut out = ctx.run_ui(raw, |ui| app.close_request(ui.ctx()));
        out.textures_delta.clear();
        out.viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|v| {
                v.commands
                    .iter()
                    .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
            })
    };
    // Clean: the window just closes.
    let mut app = EditorApp::new(Launch::default());
    assert!(!close(&mut app));
    assert!(!app.close_prompt.is_open());

    // Unsaved changes: the close is cancelled and the prompt opens.
    app.execute(&rectangle(10.0)).expect("rectangle");
    assert!(close(&mut app), "the close is cancelled");
    assert!(app.close_prompt.is_open());
    assert!(!app.exit_approved());

    // Approved (Save / Don't Save, the updater, a screenshot run): no prompt.
    let mut app = EditorApp::new(Launch::default());
    app.execute(&rectangle(10.0)).expect("rectangle");
    app.approve_exit();
    assert!(!close(&mut app));
    assert!(!app.close_prompt.is_open());
}

#[test]
fn fit_text_dialog_remembers_its_last_choices_and_shows_a_fitted_texts_own() {
    use crate::lettering_ui::{fit_request, open_fit};
    use openstitch_design::{Spread, TextAlign, VAlign};
    let mut app = EditorApp::new(Launch::default());
    app.execute(&rectangle(10.0)).expect("outline");
    let outline = app.session.design().objects[0].id;
    let add_text = |app: &mut EditorApp, word: &str| {
        app.execute(&CommandRequest::new(
            "text.add",
            json!({ "text": word, "x": 60, "y": 20 }),
        ))
        .expect("text");
        app.session.design().objects.last().expect("text").id
    };
    let first = add_text(&mut app, "Hi");
    let second = add_text(&mut app, "Yo");

    // An unfitted text opens with the defaults; the choices made are sent on OK.
    open_fit(&mut app, first, outline);
    assert_eq!(app.lettering.fit.open, Some((first, outline)));
    let f = &mut app.lettering.fit;
    (f.align, f.spread, f.valign) = (TextAlign::Right, Spread::None, VAlign::Top);
    (f.upright, f.other_side) = (true, true);
    let r = fit_request(&app.lettering.fit, first, outline);
    app.execute(&r).expect("fit");
    app.lettering.fit.open = None;

    // Another unfitted text: the dialog shows the last choices, not the defaults.
    open_fit(&mut app, second, outline);
    let f = &app.lettering.fit;
    assert_eq!(f.open, Some((second, outline)));
    assert_eq!(
        (f.align, f.spread, f.valign, f.upright, f.other_side),
        (TextAlign::Right, Spread::None, VAlign::Top, true, true),
        "the last choices are remembered"
    );

    // A fitted text: the dialog shows that text's own settings.
    app.lettering.fit.open = None;
    let f = &mut app.lettering.fit;
    (f.align, f.valign, f.upright, f.other_side) = (TextAlign::Left, VAlign::Bottom, false, false);
    open_fit(&mut app, first, outline);
    let f = &app.lettering.fit;
    assert_eq!(
        (f.align, f.spread, f.valign, f.upright, f.other_side),
        (TextAlign::Right, Spread::None, VAlign::Top, true, true),
        "a fitted text shows its own settings"
    );
}

/// The shapes `draw_solid` paints for `app` at `vp`, as egui returns them from the frame.
fn solid_shapes(app: &EditorApp, vp: &Viewport) -> Vec<egui::Shape> {
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter().clone();
        crate::view_ui::draw_solid(app, &painter, vp);
    });
    out.textures_delta.clear();
    out.shapes
        .into_iter()
        .map(|c| c.shape)
        .filter(|s| matches!(s, egui::Shape::LineSegment { .. } | egui::Shape::Circle(_)))
        .collect()
}

#[test]
fn solid_view_draws_each_colour_block_as_flat_thread_0_7_mm_wide() {
    use openstitch_app::editor::render::ColorLayer;
    use openstitch_design::v2;
    let mut app = EditorApp::new(Launch::default());
    let (red, blue) = ([200, 20, 30], [10, 40, 220]);
    app.render = RenderModel {
        layers: vec![
            ColorLayer {
                rgb: red,
                strokes: vec![vec![v2(0.0, 0.0), v2(10.0, 0.0), v2(10.0, 10.0)]],
                cover: Vec::new(),
                shift: Vec::new(),
            },
            ColorLayer {
                rgb: blue,
                strokes: vec![vec![v2(20.0, 0.0), v2(30.0, 0.0)]],
                cover: Vec::new(),
                shift: Vec::new(),
            },
        ],
        ..Default::default()
    };
    let color = |rgb: [u8; 3]| egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);

    // 2 points per 0.1 mm: 0.7 mm is 14 points; each stitch is one segment of that width, and
    // every needle point a round join of half of it, all in the block's own colour.
    let vp = Viewport {
        scale: 2.0,
        offset: openstitch_design::v2(10.0, 20.0),
    };
    let shapes = solid_shapes(&app, &vp);
    let mut segments = Vec::new();
    let mut joins = Vec::new();
    for s in &shapes {
        match s {
            egui::Shape::LineSegment { points, stroke } => {
                segments.push((points[0], points[1], stroke.width, stroke.color));
            }
            egui::Shape::Circle(c) => {
                assert_eq!(c.stroke, egui::Stroke::NONE, "unshaded: no outline");
                joins.push((c.center, c.radius, c.fill));
            }
            _ => unreachable!(),
        }
    }
    let p = |x: f32, y: f32| egui::Pos2::new(x, y);
    assert_eq!(
        segments,
        vec![
            (p(10.0, 20.0), p(30.0, 20.0), 14.0, color(red)),
            (p(30.0, 20.0), p(30.0, 40.0), 14.0, color(red)),
            (p(50.0, 20.0), p(70.0, 20.0), 14.0, color(blue)),
        ],
        "one flat-coloured segment per stitch, in sewing order"
    );
    assert_eq!(
        joins,
        vec![
            (p(10.0, 20.0), 7.0, color(red)),
            (p(30.0, 20.0), 7.0, color(red)),
            (p(30.0, 40.0), 7.0, color(red)),
            (p(50.0, 20.0), 7.0, color(blue)),
            (p(70.0, 20.0), 7.0, color(blue)),
        ]
    );

    // Zoomed far out the thread stays at least a point wide, and the joins (under 2 points)
    // are left out.
    let vp = Viewport {
        scale: 0.1,
        offset: openstitch_design::v2(0.0, 0.0),
    };
    let shapes = solid_shapes(&app, &vp);
    assert_eq!(shapes.len(), 3, "segments only: {shapes:?}");
    assert!(
        shapes
            .iter()
            .all(|s| matches!(s, egui::Shape::LineSegment { stroke, .. } if stroke.width == 1.0))
    );
}

/// A Down, Dragged, Up pointer gesture through the tools, as the canvas sends it.
fn tool_gesture(app: &mut EditorApp, from: openstitch_design::Vec2, to: openstitch_design::Vec2) {
    use openstitch_app::editor::{Phase, PointerEvent};
    for (phase, at) in [(Phase::Down, from), (Phase::Dragged, to), (Phase::Up, to)] {
        app.with_tools(|t, c| t.handle(PointerEvent::new(phase, at), c));
    }
}

#[test]
fn snap_to_grid_places_a_dragged_curve_handle_on_the_grid() {
    use openstitch_design::v2;
    // A 20 mm square whose top edge is a curve with handles at (15, 10) and (25, 10) mm.
    let curved = |snap: bool| {
        let mut app = EditorApp::new(Launch::default());
        app.execute(&rectangle(10.0)).expect("square");
        app.execute(&CommandRequest::new(
            "object.setSegment",
            json!({ "id": 1, "segment": 0, "curve": [[15, 10], [25, 10]] }),
        ))
        .expect("curve");
        if snap {
            app.toggle_snap_to_grid();
            app.workspace.grid.spacing_mm = 1.0;
        }
        app.tools.set_tool(Tool::Point);
        app.selection = BTreeSet::from([1]);
        app
    };
    let first_handle = |app: &EditorApp| {
        app.session.design().objects[0].contours[0]
            .curve(0)
            .expect("curve")
            .c1
    };

    let mut app = curved(true);
    assert_eq!(first_handle(&app), v2(150.0, 100.0));
    tool_gesture(&mut app, v2(151.0, 101.0), v2(163.0, 128.0));
    assert_eq!(first_handle(&app), v2(160.0, 130.0), "on the 1 mm grid");
    app.undo();
    assert_eq!(first_handle(&app), v2(150.0, 100.0), "one undo step");

    // Off: the handle goes where it is dropped.
    let mut app = curved(false);
    tool_gesture(&mut app, v2(151.0, 101.0), v2(163.0, 128.0));
    assert_eq!(first_handle(&app), v2(163.0, 128.0));
}

#[test]
fn snap_to_grid_places_the_arrange_copies_handle_on_the_grid() {
    use crate::edit_ui::drag;
    use openstitch_app::editor::Phase;
    use openstitch_app::editor::edit_ops::ArrangeMode;
    use openstitch_design::v2;
    // Mirror copies of a 20 mm square: the handle is the mirror axis on its right edge, and
    // the gap between the square and its copy is twice the handle's distance from the edge.
    let spacing_after_drag = |snap: bool, to: f64| {
        let mut app = EditorApp::new(Launch::default());
        app.execute(&rectangle(10.0)).expect("square");
        if snap {
            app.toggle_snap_to_grid();
            app.workspace.grid.spacing_mm = 10.0;
        }
        app.selection = BTreeSet::from([1]);
        app.edit.arrange_open = true;
        app.edit.arrange.mode = ArrangeMode::MirrorHorizontal;
        assert!(drag(&mut app, Phase::Down, v2(301.0, 199.0)), "handle");
        assert!(drag(&mut app, Phase::Dragged, v2(to, 230.0)));
        assert!(drag(&mut app, Phase::Up, v2(to, 230.0)));
        app.edit.arrange.spacing
    };
    // On: dropped at 37.2 mm, the axis lands on the 40 mm grid line, 10 mm right of the edge.
    assert_eq!(spacing_after_drag(true, 372.0), 20.0);
    // A drag the grid rounds back to the edge leaves no gap.
    assert_eq!(spacing_after_drag(true, 340.0), 0.0);
    // Off: the axis is where it is dropped, 7.2 mm right of the edge.
    let off = spacing_after_drag(false, 372.0);
    assert!((off - 14.4).abs() < 1e-9, "{off}");
}

/// #487: Import SVG's status line says what the import left out or approximated.
#[test]
fn svg_import_status_lists_what_was_left_out() {
    let dir = openstitch_testutil::TempDir::new("app-svg-report");
    let svg = dir.join("art.svg");
    std::fs::write(
        &svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20">
          <rect x="0" y="0" width="20" height="20" fill="#e01010"/>
          <text id="t" x="0" y="10">Hi</text>
        </svg>"##,
    )
    .expect("svg");
    let mut app = EditorApp::new(Launch::default());
    let r = app
        .execute(&CommandRequest::new(
            "import.svg",
            json!({ "path": svg.display().to_string() }),
        ))
        .expect("import");
    assert_eq!(import_report(&r), "; left out: 1 text");
    assert_eq!(import_report(&json!({"ids": [1]})), "");
}

#[test]
fn backspace_while_drawing_takes_back_a_point_and_keeps_the_selection() {
    // #499: Backspace is a Delete key, but while a drawing tool is placing points it removes
    // the last point instead of deleting the selected objects.
    let ctx = egui::Context::default();
    let mut app = EditorApp::new(Launch::default());
    app.execute(&rectangle(5.0)).expect("rectangle");
    app.select_all();
    app.tools.set_tool(Tool::Polygon);
    for (x, y) in [(40.0, 40.0), (60.0, 40.0)] {
        app.cursor_command(Some(&json!({ "x": x, "y": y, "action": "place" })))
            .expect("place");
    }
    press(&ctx, &mut app, Key::Backspace, Modifiers::NONE);
    assert_eq!(app.session.design().objects.len(), 1, "the rectangle stays");
    assert_eq!(app.tools.polygon().len(), 1, "one point taken back");
    app.tools.set_tool(Tool::Select);
    press(&ctx, &mut app, Key::Backspace, Modifiers::NONE);
    assert!(
        app.session.design().objects.is_empty(),
        "not drawing: Backspace deletes"
    );
}

#[test]
fn hand_edits_on_a_live_fill_are_kept_until_a_setting_changes_and_the_user_is_told() {
    // #501: a needle point moved on a live fill; changing its density discards the edit with a
    // message; Regenerate drops edits on purpose.
    let mut app = EditorApp::new(Launch::default());
    let id = app.execute(&rectangle(5.0)).expect("rectangle")["id"]
        .as_u64()
        .expect("id");
    app.execute(&CommandRequest::new(
        "stitches.setPoint",
        json!({ "id": id, "index": 3, "x": 12.3, "y": 21.0 }),
    ))
    .expect("edit a live fill");
    let sewn = |app: &EditorApp| {
        app.pattern
            .stitches
            .iter()
            .any(|s| (s.x - 123.0).abs() < 1e-6 && (s.y - 210.0).abs() < 1e-6)
    };
    assert!(sewn(&app), "the canvas sews the edited point");
    app.execute(&CommandRequest::new(
        "object.setRegion",
        json!({ "ids": [id], "density": 2.0 }),
    ))
    .expect("density");
    assert!(app.status.contains("discarded"), "{}", app.status);
    assert!(!sewn(&app));
    app.undo();
    assert!(sewn(&app), "Undo brings them back");
    app.execute(&CommandRequest::new(
        "object.regenerate",
        json!({ "ids": [id] }),
    ))
    .expect("regenerate");
    assert!(!sewn(&app));
    let steps = app.undo_groups.len();
    app.undo();
    assert!(sewn(&app), "Undo after Regenerate brings the edits back");
    assert_eq!(
        app.undo_groups.len(),
        steps - 1,
        "Regenerate was one undo step"
    );
    app.redo();
    assert!(!sewn(&app), "Redo regenerates again");
}

/// #799: Delete removes the selected guideline and leaves the selected objects.
#[test]
fn delete_removes_a_selected_guideline_first() {
    let ctx = egui::Context::default();
    let mut app = EditorApp::new(Launch::default());
    app.execute(&rectangle(5.0)).expect("rectangle");
    app.select_all();
    app.execute(&CommandRequest::new("view.newGuide", json!({ "orientation": "horizontal", "position": 20 })))
        .expect("guide");
    app.ui_command("ui.guides", Some(&json!({ "orientation": "horizontal", "index": 0 })))
        .expect("select");
    press(&ctx, &mut app, Key::Delete, Modifiers::NONE);
    assert!(app.session.design().guides.horizontal.is_empty());
    assert_eq!(app.session.design().objects.len(), 1, "the rectangle stays");
    press(&ctx, &mut app, Key::Delete, Modifiers::NONE);
    assert!(app.session.design().objects.is_empty(), "then Delete deletes objects");
}

/// #797: the start cards close for the session with Escape or a click on the canvas beside them,
/// stay closed over a new empty design, and the preference is untouched; "Don't show this again"
/// turns the preference off; `ui.gettingStarted {cards}` closes and shows them.
#[test]
fn start_cards_close_for_the_session_and_dont_show_again_turns_them_off() {
    let ctx = egui::Context::default();
    let canvas = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 700.0));
    let fresh = || {
        let mut app = EditorApp::new(Launch::default());
        app.workspace.canvas = Some(canvas);
        app
    };
    let cards = |app: &mut EditorApp, events: Vec<egui::Event>| {
        frame(&ctx, app, events, Modifiers::NONE, 1.0 / 60.0, |app, ctx| {
            crate::start_ui::cards(app, ctx)
        });
    };
    // Escape.
    let mut app = fresh();
    assert!(crate::start_ui::cards_visible(&app));
    cards(&mut app, vec![]);
    cards(&mut app, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(!crate::start_ui::cards_visible(&app), "Escape closes them");
    assert!(app.ui.prefs.start_cards, "only for the session");
    app.execute(&CommandRequest::new("design.new", json!({})));
    assert!(!crate::start_ui::cards_visible(&app), "a new empty design keeps them closed");
    // A click on the canvas beside the cards (its corner).
    let mut app = fresh();
    cards(&mut app, vec![]);
    let at = egui::pos2(10.0, 10.0);
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    cards(&mut app, vec![egui::Event::PointerMoved(at), button(true)]);
    cards(&mut app, vec![button(false)]);
    assert!(!crate::start_ui::cards_visible(&app), "a click beside them closes them");
    assert!(app.ui.prefs.start_cards);
    // Through the Assistant: show them again, close them, read the state.
    let r = app
        .ui_command("ui.gettingStarted", Some(&json!({ "cards": "show" })))
        .expect("show");
    assert_eq!(r["cardsShown"], json!(true));
    app.ui_command("ui.gettingStarted", Some(&json!({ "cards": "dismiss" })))
        .expect("dismiss");
    let state = app.ui_command("ui.gettingStarted", None).expect("state");
    assert_eq!(
        (&state["cardsShown"], &state["dismissed"], &state["enabled"]),
        (&json!(false), &json!(true), &json!(true))
    );
    assert!(app
        .ui_command("ui.gettingStarted", Some(&json!({ "cards": "nope" })))
        .is_err());
    // "Don't show this again": the preference is off, as Options ▸ Show start cards sets it.
    let mut app = fresh();
    crate::start_ui::never_show_cards(&mut app);
    assert!(!app.ui.prefs.start_cards);
    assert!(!crate::start_ui::cards_visible(&app));
    app.ui_command("ui.set", Some(&json!({ "name": "showStartCards", "value": true })))
        .expect("back on");
    assert!(crate::start_ui::cards_visible(&app));
}

thread_local! {
    static FAKE_STORAGE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// #797: an editor with no ui.json (the browser page) reads its preferences from the store the
/// page registers and writes every change back; a store that holds nothing or garbage gives the
/// defaults.
#[test]
fn preferences_last_in_the_pages_storage() {
    use openstitch_app::platform::{PrefsStore, set_prefs_store};
    set_prefs_store(PrefsStore {
        read: || FAKE_STORAGE.with(|s| s.borrow().clone()),
        write: |t| FAKE_STORAGE.with(|s| *s.borrow_mut() = Some(t.to_string())),
    });
    // Nothing stored: the defaults.
    let app = EditorApp::new(Launch::default());
    assert!(app.ui.prefs.start_cards);
    // Garbage stored (cleared or damaged site data): the defaults, no panic.
    FAKE_STORAGE.with(|s| *s.borrow_mut() = Some("{not json".into()));
    let mut app = EditorApp::new(Launch::default());
    assert!(app.ui.prefs.start_cards);
    // "Don't show this again" is written to the store and read back by the next page load.
    crate::start_ui::never_show_cards(&mut app);
    let stored = FAKE_STORAGE.with(|s| s.borrow().clone()).expect("written");
    assert!(stored.contains("\"showStartCards\":false"), "{stored}");
    let again = EditorApp::new(Launch::default());
    assert!(!again.ui.prefs.start_cards, "kept across a reload");
    assert!(!crate::start_ui::cards_visible(&again));
    // Options ▸ Show start cards turns them back on, and that lasts too.
    let mut again = again;
    again
        .ui_command("ui.set", Some(&json!({ "name": "showStartCards", "value": true })))
        .expect("on");
    assert!(EditorApp::new(Launch::default()).ui.prefs.start_cards);
}

thread_local! {
    static FAKE_USER_STORE: std::cell::RefCell<Option<Vec<u8>>> = const { std::cell::RefCell::new(None) };
}

/// #829: an editor with no user.json (the browser page) takes its user settings from the store
/// the page registers, and a thread chart added in the window is there at the next start.
#[test]
fn thread_charts_last_in_the_pages_storage() {
    openstitch_app::platform::set_user_store(openstitch_engine::session::UserStore {
        read: || FAKE_USER_STORE.with(|s| s.borrow().clone()),
        write: |b| FAKE_USER_STORE.with(|s| *s.borrow_mut() = Some(b.to_vec())),
    });
    let mut app = EditorApp::new(Launch::default());
    assert_eq!(app.session.user_settings().thread_charts[0].name, "My Threads");
    app.execute(&CommandRequest::new("threads.createChart", json!({ "name": "Shelf" })));
    let again = EditorApp::new(Launch::default());
    let names: Vec<&str> = again
        .session
        .user_settings()
        .thread_charts
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["My Threads", "Shelf"], "kept across a reload");
}
