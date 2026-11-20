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

    // #1186: while the Assistant's turn runs, a close asks first even with nothing unsaved.
    let mut app = EditorApp::new(Launch::default());
    crate::assistant_panel::set_test_turn(&mut app, true);
    assert!(close(&mut app), "the close is cancelled during a turn");
    assert!(app.turn_close.is_some() && !app.close_prompt.is_open());
    // A screenshot run's own exit is approved: it closes, so docs runs do not hang.
    let mut app = EditorApp::new(Launch::default());
    crate::assistant_panel::set_test_turn(&mut app, true);
    app.approve_exit();
    assert!(!close(&mut app));
    assert!(app.turn_close.is_none());

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
        // The handle belongs to the first point, which has to be selected for it to show.
        app.with_tools(|tools, ctx| {
            assert!(tools.select_points(0, &[0], ctx));
            vec![]
        });
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

/// PE-DESIGN capture f, 2026-11-19 (#1260), through the key events as the window gets them:
/// with points selected Ctrl+Right / Ctrl+Down select the next point and Ctrl+Left / Ctrl+Up
/// the previous one, Shift+arrow grows and shrinks the selection, and with Shift or Ctrl held
/// no point moves (the plain arrows' 0.5 mm move does not fire).
#[test]
fn ctrl_and_shift_arrows_change_the_point_selection_and_move_no_point() {
    use openstitch_design::v2;
    let ctx = egui::Context::default();
    let mut app = EditorApp::new(Launch::default());
    let r = app
        .execute(&CommandRequest::new(
            "shape.polygon",
            json!({ "points": [[10, 60], [40, 60], [40, 30]], "closed": false, "outline": "#203040" }),
        ))
        .expect("line");
    app.selection = CommandRequest::created_ids(&r);
    let picked = |app: &mut EditorApp| {
        let s = app.ui_command("ui.pointMenu", None).expect("state");
        let mut out: Vec<u64> = s["selected"]
            .as_array()
            .expect("selected")
            .iter()
            .map(|p| p["index"].as_u64().expect("index"))
            .collect();
        out.sort_unstable();
        out
    };
    app.ui_command("ui.pointMenu", Some(&json!({ "points": [1] })))
        .expect("the corner");
    let before = app.session.design().clone();
    let ctrl = Modifiers::COMMAND;
    for (key, modifiers, want, what) in [
        (Key::ArrowRight, ctrl, vec![2], "Ctrl+Right: the next point"),
        (Key::ArrowDown, ctrl, vec![2], "Ctrl+Down at the line's end: it stays"),
        (Key::ArrowLeft, ctrl, vec![1], "Ctrl+Left: the previous point"),
        (Key::ArrowUp, ctrl, vec![0], "Ctrl+Up: the previous point"),
        (Key::ArrowDown, ctrl, vec![1], "Ctrl+Down: the next point"),
        (Key::ArrowRight, Modifiers::SHIFT, vec![1, 2], "Shift+Right grows"),
        (Key::ArrowLeft, Modifiers::SHIFT, vec![1], "Shift+Left shrinks"),
        (Key::ArrowUp, Modifiers::SHIFT, vec![0, 1], "Shift+Up grows the other way"),
        (Key::ArrowDown, Modifiers::SHIFT, vec![1], "Shift+Down shrinks"),
        (Key::ArrowDown, Modifiers::SHIFT, vec![1, 2], "Shift+Down grows"),
        (Key::ArrowUp, ctrl, vec![0], "Ctrl+Up from the point it started at"),
    ] {
        press(&ctx, &mut app, key, modifiers);
        assert_eq!(picked(&mut app), want, "{what}");
        assert_eq!(*app.session.design(), before, "{what}: no point moves");
    }
    // The plain arrows still move the selected point, 0.5 mm, in all four directions.
    let at = |app: &EditorApp| app.session.design().objects[0].contours[0].points[0];
    let start = at(&app);
    for (key, by) in [
        (Key::ArrowRight, v2(5.0, 0.0)),
        (Key::ArrowDown, v2(5.0, 5.0)),
        (Key::ArrowLeft, v2(0.0, 5.0)),
        (Key::ArrowUp, v2(0.0, 0.0)),
    ] {
        press(&ctx, &mut app, key, Modifiers::NONE);
        assert_eq!(at(&app), start + by, "{key:?}");
    }
    // Without a selected point Ctrl+arrow is not taken by the Point tool.
    app.tools.set_tool(Tool::Point);
    press(&ctx, &mut app, Key::ArrowRight, ctrl);
    assert!(picked(&mut app).is_empty());
}

/// The marks of an Alt-connect are drawn (#1260): an x (two strokes) on every end the selected
/// end point can connect to, and a red square on the end the dragged point lies on. The whole
/// frame is run as the window runs it and its painted shapes are read.
#[test]
fn the_alt_connect_marks_and_the_red_square_are_painted() {
    use openstitch_app::editor::Phase;
    use openstitch_design::v2;
    fn flat(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(list) => list.iter().for_each(|s| flat(s, out)),
            other => out.push(other.clone()),
        }
    }
    let ctx = egui::Context::default();
    let mut frame = eframe::Frame::_new_kittest();
    let mut painted = |app: &mut EditorApp| {
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| eframe::App::ui(app, ui, &mut frame));
            out.textures_delta.clear();
            shapes.clear();
            for clipped in &out.shapes {
                flat(&clipped.shape, &mut shapes);
            }
        }
        let ink = egui::Color32::from_gray(16);
        let red = egui::Color32::from_rgb(0xe0, 0x20, 0x20);
        let strokes = shapes
            .iter()
            .filter(|s| {
                matches!(s, egui::Shape::LineSegment { stroke, .. } if stroke.width == 1.5 && stroke.color == ink)
            })
            .count();
        let squares = shapes
            .iter()
            .filter(|s| matches!(s, egui::Shape::Rect(r) if r.stroke.color == red && r.stroke.width == 2.0))
            .count();
        (strokes, squares)
    };
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
    line(&mut app, json!([[50, 30], [70, 30], [70, 60]]));
    app.selection = BTreeSet::from([a]);
    app.ui_command("ui.pointMenu", Some(&json!({ "points": [2] })))
        .expect("the end point");
    assert_eq!(painted(&mut app), (0, 0), "no marks without Alt");
    // An Alt + drag of the end, not yet on another end: three ends marked, no square.
    let event = |phase, at| {
        let mut e = openstitch_app::editor::PointerEvent::new(phase, at);
        e.alt = true;
        e
    };
    app.with_tools(|t, c| t.handle(event(Phase::Down, v2(400.0, 300.0)), c));
    app.with_tools(|t, c| t.handle(event(Phase::Dragged, v2(450.0, 330.0)), c));
    assert_eq!(painted(&mut app), (6, 0), "an x of two strokes on each of the three ends");
    // On the other line's end: the red square as well.
    app.with_tools(|t, c| t.handle(event(Phase::Dragged, v2(500.0, 300.0)), c));
    assert_eq!(painted(&mut app), (6, 1));
    let s = app.ui_command("ui.pointMenu", None).expect("state");
    assert_eq!(s["connectOverlap"]["x"], 50.0);
    // Released: joined, and nothing is marked any more.
    app.with_tools(|t, c| t.handle(event(Phase::Up, v2(500.0, 300.0)), c));
    assert_eq!(app.session.design().objects.len(), 1);
    assert_eq!(painted(&mut app), (0, 0));
}

/// The direction line as painted (#1260, PE-DESIGN capture g): a red line, and a square and
/// an arrow head that are white inside and outlined in the same red; the selected line's
/// square is black. The whole frame is run and its painted shapes are read.
#[test]
fn the_direction_line_is_painted_red_and_its_selected_square_black() {
    use egui::epaint::ColorMode;
    fn flat(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(list) => list.iter().for_each(|s| flat(s, out)),
            other => out.push(other.clone()),
        }
    }
    let ctx = egui::Context::default();
    let mut frame = eframe::Frame::_new_kittest();
    let red = egui::Color32::from_rgb(0xd3, 0x2f, 0x2f);
    let dark = egui::Color32::from_gray(16);
    // (red lines of width 1, white polygons outlined red, dark polygons outlined dark)
    let mut painted = |app: &mut EditorApp| {
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| eframe::App::ui(app, ui, &mut frame));
            out.textures_delta.clear();
            shapes.clear();
            for clipped in &out.shapes {
                flat(&clipped.shape, &mut shapes);
            }
        }
        let lines = shapes
            .iter()
            .filter(|s| {
                matches!(s, egui::Shape::LineSegment { stroke, .. } if stroke.width == 1.0 && stroke.color == red)
            })
            .count();
        let polygons = |fill: egui::Color32, outline: egui::Color32| {
            shapes
                .iter()
                .filter(|s| {
                    matches!(s, egui::Shape::Path(p) if p.closed
                        && p.fill == fill
                        && matches!(p.stroke.color, ColorMode::Solid(c) if c == outline))
                })
                .count()
        };
        (
            lines,
            polygons(egui::Color32::WHITE, red),
            polygons(dark, dark),
        )
    };
    let mut app = EditorApp::new(Launch::default());
    let r = app
        .execute(&CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 20, "y": 20, "width": 40, "height": 30, "fill": "#2e8b57", "outline": "#203040" }),
        ))
        .expect("rectangle");
    app.selection = CommandRequest::created_ids(&r);
    let id = *app.selection.first().expect("id");
    app.execute(&CommandRequest::new(
        "object.setRegion",
        json!({ "id": id, "angle": 45 }),
    ))
    .expect("a straight direction");
    assert_eq!(painted(&mut app), (0, 0, 0), "not with the Select tool");
    app.tools.set_tool(Tool::Point);
    assert_eq!(
        painted(&mut app),
        (1, 2, 0),
        "the line, and the square and the arrow head white with a red outline"
    );
    app.ui_command("ui.pointMenu", Some(&json!({ "selectDirectionLine": true })))
        .expect("select the line");
    assert_eq!(
        painted(&mut app),
        (1, 1, 1),
        "selected: the square black, the arrow head as before"
    );
    // Several direction lines (#1273): every line is painted, each with its square and its
    // arrow head; only the selected line's square is black.
    app.ui_command("ui.pointMenu", Some(&json!({ "selectDirectionLine": false })))
        .expect("select none");
    for line in [[[15, 30], [35, 15]], [[45, 55], [65, 40]]] {
        app.execute(&CommandRequest::new(
            "object.addDirectionLine",
            json!({ "id": id, "line": line }),
        ))
        .expect("a direction line");
    }
    assert_eq!(
        painted(&mut app),
        (3, 6, 0),
        "three lines, three squares and three arrow heads"
    );
    for index in 0..3 {
        app.ui_command("ui.pointMenu", Some(&json!({ "selectDirectionLine": index })))
            .expect("select a line");
        assert_eq!(
            painted(&mut app),
            (3, 5, 1),
            "line {index} selected: one black square"
        );
    }
}

/// Esc with a point menu or an object menu open closes the menu and nothing else (PE-DESIGN's:
/// Esc closes a menu): the shape stays selected, the selected point too, and the Point tool
/// stays. It used to drop the selection as well, which the browser test showed (#1260).
#[test]
fn esc_closes_an_open_point_menu_and_keeps_the_selection_and_the_tool() {
    let ctx = egui::Context::default();
    for object in [false, true] {
        let mut app = EditorApp::new(Launch::default());
        let r = app.execute(&rectangle(10.0)).expect("rectangle");
        app.selection = CommandRequest::created_ids(&r);
        let selection = app.selection.clone();
        app.ui_command("ui.pointMenu", Some(&json!({ "points": [0] })))
            .expect("a point");
        crate::point_menu_ui::open_for_test(&mut app, egui::pos2(300.0, 200.0), object);
        let open = |app: &mut EditorApp| {
            let s = app.ui_command("ui.pointMenu", None).expect("state");
            (
                s["open"] == true,
                s["objectMenuOpen"] == true,
                s["selected"].as_array().map_or(0, Vec::len),
            )
        };
        assert_eq!(open(&mut app), (!object, object, 1));
        press(&ctx, &mut app, Key::Escape, Modifiers::NONE);
        assert_eq!(open(&mut app), (false, false, 1), "the menu is closed, the point selected");
        assert_eq!(app.selection, selection, "the shape is still selected");
        assert_eq!(app.tools.tool(), Tool::Point);
        // With no menu open Esc deselects, as before.
        press(&ctx, &mut app, Key::Escape, Modifiers::NONE);
        assert!(app.selection.is_empty());
    }
}

/// #1272: an editor with one stitch pattern in two colours (red lines at y = 20 and 30 mm,
/// blue at 50 and 60, from x = 20 to 40, a jump between each two lines), read from a DST file
/// as a machine file is imported, selected, with the Point tool. Returns the editor and the
/// pattern's id.
fn two_colour_pattern() -> (EditorApp, u64) {
    let dir = std::env::temp_dir().join(format!(
        "openstitch-app-two-colours-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch folder");
    let mut s = openstitch_engine::Session::new(dir.clone());
    for (k, (y, colour)) in [(20, "#d32f2f"), (30, "#d32f2f"), (50, "#1f3a93"), (60, "#1f3a93")]
        .into_iter()
        .enumerate()
    {
        s.execute(
            "shape.path",
            Some(&json!({ "points": [[20, y], [40, y]], "closed": false, "outline": colour })),
        )
        .expect("line");
        s.execute(
            "object.setLine",
            Some(&json!({ "id": k + 1, "type": "running", "pitch": 2.0 })),
        )
        .expect("running");
    }
    s.execute("design.export", Some(&json!({ "path": "k0.dst" })))
        .expect("export");
    let mut app = EditorApp::new(Launch::default());
    let r = app
        .execute(&CommandRequest::new(
            "stitches.import",
            json!({ "path": dir.join("k0.dst").display().to_string() }),
        ))
        .expect("import");
    let _ = std::fs::remove_dir_all(&dir);
    let id = r["id"]
        .as_u64()
        .or_else(|| r["ids"][0].as_u64())
        .expect("id");
    app.selection = BTreeSet::from([id]);
    app.tools.set_tool(Tool::Point);
    (app, id)
}

/// The needle points selected with the Point tool, and each colour's points.
fn needles(app: &EditorApp) -> (BTreeSet<usize>, Vec<Vec<usize>>) {
    let ctx = app.tool_context(app.session.design());
    let o = ctx.needle_target().expect("the stitch pattern");
    (
        app.tools.selected_needles(&ctx),
        openstitch_app::editor::tools::needles::color_groups(o),
    )
}

/// Tab, Shift+Tab, Home, End, Ctrl+Home, Ctrl+End, Ctrl+arrow, Shift+arrow and the arrows on
/// a stitch pattern with the Point tool, as real key events (#1272, PE-DESIGN captures k4 to
/// k7: the arrows move the points 0.1 mm).
#[test]
fn the_point_tools_keys_on_a_stitch_pattern() {
    let ctx = egui::Context::default();
    let (mut app, id) = two_colour_pattern();
    let (none, groups) = needles(&app);
    assert!(none.is_empty());
    let set = |g: &Vec<usize>| g.iter().copied().collect::<BTreeSet<usize>>();
    let (red, blue) = (groups[0].clone(), groups[1].clone());
    let all: BTreeSet<usize> = groups.iter().flatten().copied().collect();
    let (first, last) = (*all.first().expect("entry"), *all.last().expect("exit"));
    // Nothing selected: Tab and Home do nothing (PE-DESIGN capture k6, k7).
    press(&ctx, &mut app, Key::Tab, Modifiers::NONE);
    press(&ctx, &mut app, Key::Home, Modifiers::NONE);
    assert!(needles(&app).0.is_empty());
    // One red point: Tab selects every point of its colour block (both red lines; in
    // PE-DESIGN each colour of a machine file is a stitch pattern of its own), again none,
    // Shift+Tab all again; one blue point: every blue point.
    app.tools.set_needle_selection(id, BTreeSet::from([red[3]]));
    press(&ctx, &mut app, Key::Tab, Modifiers::NONE);
    assert_eq!(needles(&app).0, set(&red), "Tab: every red point");
    press(&ctx, &mut app, Key::Tab, Modifiers::NONE);
    assert!(needles(&app).0.is_empty(), "Tab: none");
    press(&ctx, &mut app, Key::Tab, Modifiers::SHIFT);
    assert_eq!(needles(&app).0, set(&red), "Shift+Tab: every red point again");
    app.tools.set_needle_selection(id, BTreeSet::from([blue[3]]));
    press(&ctx, &mut app, Key::Tab, Modifiers::NONE);
    assert_eq!(needles(&app).0, set(&blue), "Tab on blue: every blue point");
    // Home and End: the colour block's first and last point.
    app.tools.set_needle_selection(id, BTreeSet::from([red[3]]));
    press(&ctx, &mut app, Key::End, Modifiers::NONE);
    let last_red = *red.last().expect("red");
    assert_eq!(needles(&app).0, BTreeSet::from([last_red]), "End: the last red point");
    press(&ctx, &mut app, Key::Home, Modifiers::NONE);
    assert_eq!(needles(&app).0, BTreeSet::from([first]), "Home: the first red point");
    app.tools.set_needle_selection(id, BTreeSet::from([blue[3]]));
    press(&ctx, &mut app, Key::Home, Modifiers::NONE);
    assert_eq!(needles(&app).0, BTreeSet::from([blue[0]]), "Home: the first blue point");
    press(&ctx, &mut app, Key::End, Modifiers::NONE);
    assert_eq!(needles(&app).0, BTreeSet::from([last]), "End: the last blue point");
    // Ctrl+Home and Ctrl+End: the pattern's entry and exit point.
    app.tools.set_needle_selection(id, BTreeSet::from([red[3]]));
    press(&ctx, &mut app, Key::End, Modifiers::COMMAND);
    assert_eq!(needles(&app).0, BTreeSet::from([last]), "Ctrl+End");
    press(&ctx, &mut app, Key::Home, Modifiers::COMMAND);
    assert_eq!(needles(&app).0, BTreeSet::from([first]), "Ctrl+Home");
    // Ctrl+Right stops at the colour block's end (PE-DESIGN capture k4 never reached blue).
    app.tools.set_needle_selection(id, BTreeSet::from([last_red]));
    press(&ctx, &mut app, Key::ArrowRight, Modifiers::COMMAND);
    assert_eq!(needles(&app).0, BTreeSet::from([last_red]), "Ctrl+Right: not into blue");
    app.tools.set_needle_selection(id, BTreeSet::from([first]));
    // Ctrl+arrow steps, Shift+arrow grows; nothing moves.
    let before = app.session.design().clone();
    press(&ctx, &mut app, Key::ArrowRight, Modifiers::COMMAND);
    assert_eq!(needles(&app).0, BTreeSet::from([red[1]]), "Ctrl+Right: the next point");
    press(&ctx, &mut app, Key::ArrowRight, Modifiers::SHIFT);
    press(&ctx, &mut app, Key::ArrowRight, Modifiers::SHIFT);
    assert_eq!(
        needles(&app).0,
        BTreeSet::from([red[1], red[2], red[3]]),
        "Shift+Right twice: two more"
    );
    assert_eq!(*app.session.design(), before, "stepping moves nothing");
    // The arrows move the selected points: one undo step.
    press(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    let moved = app.session.design().clone();
    assert_ne!(moved, before);
    let block = |d: &openstitch_design::Design| {
        d.objects[d.index_of(id).expect("pattern")]
            .stitch_block
            .clone()
            .expect("block")
    };
    let (b0, b1) = (block(&before), block(&moved));
    let step = crate::stitch_points_ui::NEEDLE_NUDGE;
    for k in [red[1], red[2], red[3]] {
        assert!((b1.stitches[k].y - b0.stitches[k].y - step).abs() < 1e-9, "{k}");
    }
    assert_eq!(b1.stitches[red[0]], b0.stitches[red[0]], "an unselected point stays");
    app.undo();
    assert_eq!(*app.session.design(), before, "one Undo");
}

/// Home and End belong to the Stitch Simulator while it is open (it steps to the start and
/// the end), to the Point tool otherwise; Tab is left to a text field that has the focus.
#[test]
fn the_simulator_keeps_home_and_end_and_a_text_field_keeps_tab() {
    let ctx = egui::Context::default();
    let (mut app, _) = two_colour_pattern();
    app.refresh();
    app.simulate = Some(5);
    press(&ctx, &mut app, Key::End, Modifiers::NONE);
    assert!(needles(&app).0.is_empty(), "the Point tool did not take End");
    assert_ne!(app.simulate, Some(5), "the simulator stepped");
    // End took the simulator to the end, which closes it: open it again.
    app.simulate = Some(5);
    press(&ctx, &mut app, Key::End, Modifiers::COMMAND);
    assert!(needles(&app).0.is_empty(), "the Point tool did not take Ctrl+End");
    app.simulate = None;
    press(&ctx, &mut app, Key::End, Modifiers::COMMAND);
    assert_eq!(needles(&app).0.len(), 1, "with the simulator closed Ctrl+End selects");
    // A text field with the focus keeps Tab.
    let (mut app, id) = two_colour_pattern();
    let one = needles(&app).1[0][3];
    app.tools.set_needle_selection(id, BTreeSet::from([one]));
    let field = egui::Id::unique("a text field");
    let mut text = String::new();
    for k in 0..2 {
        let events = if k == 1 {
            vec![key(Key::Tab, Modifiers::NONE)]
        } else {
            vec![]
        };
        frame(&ctx, &mut app, events, Modifiers::NONE, 1.0 / 60.0, |app, ctx| {
            egui::Area::new(egui::Id::unique("area")).show(ctx, |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut text).id(field));
                r.request_focus();
            });
            app.shortcuts(ctx);
        });
    }
    assert_eq!(
        needles(&app).0,
        BTreeSet::from([one]),
        "Tab went to the text field"
    );
}

/// Split at Point on a stitch pattern is available on every point, the entry and exit point
/// too, where it makes a group of the point alone and the pattern that sews exactly as before
/// (PE-DESIGN capture k12); `ui.stitchPoints` drives the keys and reports the colours, and
/// the hint line follows the selection.
#[test]
fn split_at_point_ui_stitch_points_and_the_hint_on_a_stitch_pattern() {
    use openstitch_app::editor::ribbon::Action;
    let (mut app, id) = two_colour_pattern();
    let s = app.ui_command("ui.stitchPoints", None).expect("state");
    assert_eq!(s["id"], json!(id));
    let colors = s["colors"].as_array().expect("colors").clone();
    assert_eq!(colors.len(), 2);
    let entry = colors[0]["entry"].as_u64().expect("entry") as usize;
    let exit = colors[1]["exit"].as_u64().expect("exit") as usize;
    assert!(
        s["hint"].as_str().expect("hint").contains("Split at Point"),
        "{s}"
    );
    // Split at Point is not grey on the entry and exit point, as PE-DESIGN shows it (capture
    // k12): it makes a group of the point alone and the pattern, and the stitches the design
    // sews stay the same.
    let sewn = |app: &EditorApp| {
        let d = app.session.design();
        openstitch_design::Compiler::for_design(d).compile(d).stitches
    };
    let k0 = sewn(&app);
    for (end, name) in [(entry, "entry"), (exit, "exit")] {
        let s = app
            .ui_command("ui.stitchPoints", Some(&json!({ "select": [end] })))
            .expect("select");
        assert!(
            crate::ribbon::enabled(&app, Action::SplitAtPoint),
            "Split at Point on the {name} point"
        );
        assert_eq!(
            s["menu"],
            json!([
                { "key": "splitAtPoint", "label": "Split at Point", "enabled": true },
                { "key": "deletePoint", "label": "Delete", "enabled": true },
            ]),
            "the needle point menu"
        );
        let before = app.session.design().clone();
        app.ui_command("ui.stitchPoints", Some(&json!({ "action": "splitAtPoint" })))
            .expect("Split at Point");
        let d = app.session.design();
        assert_eq!(d.objects.len(), 2, "{name}: two pieces");
        assert!(d.objects[0].group.is_some(), "{name}: a group");
        assert_eq!(d.objects[0].group, d.objects[1].group, "{name}");
        let lone = usize::from(end == exit);
        assert_eq!(
            d.objects[lone].stitch_block.as_ref().map(|b| b.stitches.len()),
            Some(1),
            "{name}: the point alone"
        );
        assert_eq!(sewn(&app), k0, "{name}: the same stitches as before the split");
        app.undo();
        assert_eq!(*app.session.design(), before, "{name}: one undo step");
        app.selection = BTreeSet::from([id]);
    }
    let s = app
        .ui_command("ui.stitchPoints", Some(&json!({ "select": [entry] })))
        .expect("select");
    assert!(s["hint"].as_str().expect("hint").contains("Alt"), "{s}");
    let s = app
        .ui_command("ui.stitchPoints", Some(&json!({ "step": "next" })))
        .expect("step");
    let middle = s["selected"][0].as_u64().expect("one") as usize;
    assert_ne!(middle, entry);
    assert!(crate::ribbon::enabled(&app, Action::SplitAtPoint), "available between");
    let s = app
        .ui_command("ui.stitchPoints", Some(&json!({ "tab": true })))
        .expect("Tab");
    assert_eq!(
        s["selected"].as_array().expect("selected").len() as u64,
        colors[0]["points"].as_u64().expect("count"),
        "Tab: every point of the selected point's colour block"
    );
    // The needle point menu's Delete, as a click on it: one point fewer, the neighbour
    // selected.
    let one = app
        .ui_command("ui.stitchPoints", Some(&json!({ "select": [middle] })))
        .expect("select");
    let n = one["points"].as_u64().expect("points");
    let s = app
        .ui_command("ui.stitchPoints", Some(&json!({ "action": "deletePoint" })))
        .expect("Delete");
    assert_eq!(s["points"].as_u64(), Some(n - 1));
    assert_eq!(s["selected"].as_array().map(Vec::len), Some(1));
    app.undo();
    let s = app
        .ui_command("ui.stitchPoints", Some(&json!({ "end": "first" })))
        .expect("Ctrl+Home");
    assert_eq!(s["selected"], json!([entry]));
    assert!(
        app.ui_command("ui.stitchPoints", Some(&json!({ "end": "middle" })))
            .is_err()
    );
    assert!(
        app.ui_command("ui.stitchPoints", Some(&json!({ "select": [99999] })))
            .is_err()
    );
    let spec = openstitch_app::editor::assistant::UI_COMMANDS
        .iter()
        .find(|c| c.id == "ui.stitchPoints")
        .expect("ui.stitchPoints");
    for words in ["Tab", "Ctrl+Home", "Shift+arrow", "stitches.connect", "jump: true", "Split at Point"] {
        assert!(spec.summary.contains(words), "{words}");
    }
}

/// With the pattern's exit point selected and Alt held, the other pattern's entry point is
/// marked with an x; Alt+drag onto it shows the red square, and releasing connects the two
/// into one pattern (#1272).
#[test]
fn the_alt_marks_of_a_stitch_pattern_are_painted_and_connect() {
    use openstitch_app::editor::Phase;
    fn flat(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(list) => list.iter().for_each(|s| flat(s, out)),
            other => out.push(other.clone()),
        }
    }
    let ctx = egui::Context::default();
    let mut frame = eframe::Frame::_new_kittest();
    let mut painted = |app: &mut EditorApp| {
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| eframe::App::ui(app, ui, &mut frame));
            out.textures_delta.clear();
            shapes.clear();
            for clipped in &out.shapes {
                flat(&clipped.shape, &mut shapes);
            }
        }
        let ink = egui::Color32::from_gray(16);
        let red = egui::Color32::from_rgb(0xe0, 0x20, 0x20);
        let strokes = shapes
            .iter()
            .filter(|s| {
                matches!(s, egui::Shape::LineSegment { stroke, .. } if stroke.width == 1.5 && stroke.color == ink)
            })
            .count();
        let squares = shapes
            .iter()
            .filter(|s| matches!(s, egui::Shape::Rect(r) if r.stroke.color == red && r.stroke.width == 2.0))
            .count();
        (strokes, squares)
    };
    let (mut app, id) = two_colour_pattern();
    // A second pattern: the pattern split at a point of its second colour.
    let (_, groups) = needles(&app);
    let cut = groups[1][4];
    let r = app
        .execute(&CommandRequest::new(
            "stitches.splitAt",
            json!({ "id": id, "index": cut }),
        ))
        .expect("split");
    let second = r["ids"][1].as_u64().expect("second part");
    app.selection = BTreeSet::from([id]);
    app.tools.set_tool(Tool::Point);
    let s = app
        .ui_command("ui.stitchPoints", Some(&json!({ "end": "last" })))
        .expect("the exit point");
    let connectable = s["connectable"].as_array().expect("connectable").clone();
    assert_eq!(connectable.len(), 1, "{s}");
    assert_eq!(connectable[0]["id"], json!(second));
    assert_eq!(painted(&mut app), (0, 0), "no marks without Alt");
    let exit_at = {
        let c = app.tool_context(app.session.design());
        let o = c.needle_target().expect("pattern");
        openstitch_app::editor::tools::needles::needle_points(o)
            .last()
            .expect("exit")
            .1
    };
    let to = openstitch_design::v2(
        connectable[0]["x"].as_f64().expect("x") * 10.0,
        connectable[0]["y"].as_f64().expect("y") * 10.0,
    );
    let event = |phase, at| {
        let mut e = openstitch_app::editor::PointerEvent::new(phase, at);
        e.alt = true;
        e
    };
    app.with_tools(|t, c| t.handle(event(Phase::Down, exit_at), c));
    {
        let c = app.tool_context(app.session.design());
        assert!(
            !app.tools.needle_connect_marks(false, &c).is_empty(),
            "the Alt-drag started: selected {:?}, exit at {exit_at:?}",
            app.tools.selected_needles(&c)
        );
    }
    app.with_tools(|t, c| t.handle(event(Phase::Dragged, exit_at + openstitch_design::v2(30.0, 0.0)), c));
    assert_eq!(painted(&mut app), (2, 0), "an x of two strokes on the one end");
    app.with_tools(|t, c| t.handle(event(Phase::Dragged, to), c));
    assert_eq!(painted(&mut app), (2, 1), "and the red square on it");
    app.with_tools(|t, c| t.handle(event(Phase::Up, to), c));
    let patterns = app
        .session
        .design()
        .objects
        .iter()
        .filter(|o| o.kind == openstitch_design::ObjectKind::Stitches)
        .count();
    assert_eq!(patterns, 1, "connected into one");
    assert_eq!(painted(&mut app), (0, 0));
}
