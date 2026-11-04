//! Coverage for `EditorApp` behaviour FEATURE_MATRIX claims that no other test asserts (#332):
//! File ▸ Import into design, the undo/redo and arrow-nudge keys, the simulator's play, and the
//! window close request going through the unsaved-changes prompt. A child module of `app` so it
//! can drive the private frame steps (`shortcuts`, `tick`, `close_request`) as `update` does.

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
