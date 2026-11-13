//! Seeded monkey test of the whole editor window (#880 part 2): random clicks, drags, keys,
//! typing, drops and ribbon actions fed through egui's raw input to `eframe::App::ui`, frame by
//! frame, as the window gets them (a child module of `app`, like the screen-reader audit). It
//! must never panic or hang, and Undo must bring the design back to where it started.
//!
//! Short in CI (`OPENSTITCH_MONKEY_RUNS` runs of `OPENSTITCH_MONKEY_STEPS` steps, default 2 x
//! 150); a long run sets those higher, and `OPENSTITCH_MONKEY_SEED` replays one seed. On a
//! failure it prints the seed and the last 20 actions.
//!
//! Only input that stays inside the window: pointer steps land on the canvas
//! (`workspace.canvas`), keys and actions come from allow-lists that leave out everything that
//! opens a native file dialog, the file manager or the network, or starts a new design (which
//! clears the undo history the test checks).

use super::*;
use crate::script::{Player, parse};
use openstitch_app::editor::Viewport;
use openstitch_design::v2;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Ribbon actions run through `ui.run` (the path a ribbon click takes): editing, arranging,
/// view and tool actions that open no file dialog and reach nothing outside the window.
const ACTIONS: &[&str] = &[
    "undo", "redo", "select", "point", "selectAll", "selectSame", "selectSameColor",
    "selectSameType", "selectSameBoth", "cut", "copy", "paste", "duplicate", "delete",
    "copyAttributes", "pasteAttributes", "rectangle", "ellipse", "polygon", "pen",
    "pencilClosed", "openStraight", "openCurve", "pencilOpen", "zoomIn", "zoomOut", "zoom", "pan",
    "measure", "alignLeft", "alignCenter", "alignRight", "alignTop", "alignMiddle", "alignBottom",
    "distributeHorizontal", "distributeVertical", "group", "ungroup", "solidView", "stitchView",
    "realisticView", "viewTrimming", "showGrid", "gridAxes", "snapToGrid", "snapToObjects",
    "precisionBar", "ruler", "toOutline", "toBlocks", "regionToLine", "editPoint", "deletePoint",
    "splitAtPoint", "reverseDirection", "smoothShape", "deriveSkeleton", "deriveBranches",
    "divideByColor", "smoothStitches", "connect", "entryExit", "sewingPoints", "punch",
    "satinSkeleton", "running", "curvedBlock", "knife", "toOutlines", "splitStitchData",
    "paneSewingOrder", "paneSimulator", "paneAttributes", "densityMap",
];

/// Keys and shortcuts typed on the canvas: the tool letters and editing keys; no Ctrl+N, O, S,
/// Shift+S, Shift+I (dialogs or a new design) and no Ctrl+Shift+A (the Assistant).
const KEYS: &[&str] = &[
    "key:A", "key:B", "key:E", "key:H", "key:L", "key:M", "key:P", "key:R", "key:U", "key:V",
    "key:Delete", "key:Escape", "key:Enter", "key:Backspace", "key:Tab", "key:ArrowUp",
    "key:ArrowDown", "key:ArrowLeft", "key:ArrowRight", "shift+key:ArrowRight", "cmd+key:Z",
    "cmd+key:Y", "cmd+key:A", "cmd+key:C", "cmd+key:X", "cmd+key:V", "cmd+key:D", "cmd+key:G",
    "shift+cmd+key:G",
];

/// A step slower than this is a hang.
const STEP_LIMIT: Duration = Duration::from_secs(30);

/// SplitMix64: the same sequence on every platform for a seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    /// 0..1.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn env_or(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// The window, headless: one egui context and an eframe frame for tests.
struct Window {
    ctx: egui::Context,
    frame: eframe::Frame,
}

impl Window {
    fn new() -> Self {
        Window {
            ctx: egui::Context::default(),
            frame: eframe::Frame::_new_kittest(),
        }
    }

    /// One frame with `events` (and `dropped`) as the window would receive them.
    fn frame(
        &mut self,
        app: &mut EditorApp,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
        dropped: Option<std::path::PathBuf>,
    ) {
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            ..Default::default()
        };
        raw.events.push(egui::Event::ModifiersChanged(modifiers));
        raw.events.extend(events);
        if let Some(p) = dropped {
            raw.dropped_files
                .push(crate::script::DroppedPath::handle(p));
        }
        let frame = &mut self.frame;
        let mut out = self
            .ctx
            .run_ui(raw, |ui| eframe::App::ui(app, ui, frame));
        out.textures_delta.clear();
    }

    /// Plays one `--script` step through the player, frame by frame.
    fn play(&mut self, app: &mut EditorApp, step: &str) {
        let steps = parse(step).unwrap_or_else(|e| panic!("{step}: {e}"));
        let mut player = Player::new(steps);
        let vp = app.viewport.unwrap_or(Viewport {
            scale: 1.0,
            offset: v2(0.0, 0.0),
        });
        while let Some((events, m)) = player.next_frame(&vp) {
            let dropped = player.take_dropped();
            self.frame(app, events, m, dropped);
        }
        self.frame(app, Vec::new(), egui::Modifiers::NONE, None);
    }
}

/// A point on the canvas as `--script` millimetres, inset from its edges.
fn canvas_point(app: &EditorApp, rng: &mut Rng) -> (f64, f64) {
    let (Some(c), Some(vp)) = (app.workspace.canvas, app.viewport) else {
        // Before the canvas has laid out: the page's middle.
        return (50.0, 50.0);
    };
    let (w, h) = (f64::from(c.width()), f64::from(c.height()));
    let inset = 20.0f64.min(w / 4.0).min(h / 4.0);
    let s = v2(
        f64::from(c.min.x) + inset + rng.unit() * (w - 2.0 * inset).max(1.0),
        f64::from(c.min.y) + inset + rng.unit() * (h - 2.0 * inset).max(1.0),
    );
    let m = vp.to_model(s);
    ((m.x / 10.0 * 100.0).round() / 100.0, (m.y / 10.0 * 100.0).round() / 100.0)
}

/// One random step: a `--script` step or a ribbon action (`run:KEY`).
fn random_step(app: &EditorApp, rng: &mut Rng, drop: &std::path::Path) -> String {
    match rng.below(100) {
        0..=21 => {
            let (x, y) = canvas_point(app, rng);
            let m = ["", "shift+"][rng.below(2)];
            format!("{m}click:{x},{y}")
        }
        22..=31 => {
            let (x1, y1) = canvas_point(app, rng);
            let (x2, y2) = canvas_point(app, rng);
            format!("drag:{x1},{y1},{x2},{y2}")
        }
        32..=36 => {
            let (x, y) = canvas_point(app, rng);
            format!("dclick:{x},{y}")
        }
        37..=41 => {
            let (x, y) = canvas_point(app, rng);
            format!("move:{x},{y}")
        }
        42..=63 => KEYS[rng.below(KEYS.len())].to_string(),
        64..=67 => {
            let text: String = (0..1 + rng.below(4))
                .map(|_| char::from(b'a' + rng.below(26) as u8))
                .collect();
            format!("type:{text}")
        }
        68 => format!("drop:{}", drop.display()),
        _ => format!("run:{}", ACTIONS[rng.below(ACTIONS.len())]),
    }
}

/// Runs one seeded session; Err holds the seed, the failure and the last actions.
fn run(seed: u64, steps: usize, drop: &std::path::Path) -> Result<(), String> {
    let mut rng = Rng(seed);
    let mut app = EditorApp::new(Launch {
        demo: true,
        ..Launch::default()
    });
    let _ = app.ui_command(
        "ui.set",
        Some(&serde_json::json!({ "name": "showStartCards", "value": false })),
    );
    let mut window = Window::new();
    for _ in 0..4 {
        window.frame(&mut app, Vec::new(), egui::Modifiers::NONE, None);
    }
    let start = app.session.design().clone();
    let mut last: VecDeque<String> = VecDeque::new();
    let report = |what: String, last: &VecDeque<String>| {
        let actions: Vec<&str> = last.iter().map(String::as_str).collect();
        format!(
            "monkey seed {seed}: {what}\nlast {} actions:\n  {}",
            actions.len(),
            actions.join("\n  ")
        )
    };
    for _ in 0..steps {
        let step = random_step(&app, &mut rng, drop);
        last.push_back(step.clone());
        if last.len() > 20 {
            last.pop_front();
        }
        let t = Instant::now();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(key) = step.strip_prefix("run:") {
                // Refusals (nothing selected, not available) are answers, not failures.
                let _ = app.ui_command("ui.run", Some(&serde_json::json!({ "action": key })));
                window.frame(&mut app, Vec::new(), egui::Modifiers::NONE, None);
            } else {
                window.play(&mut app, &step);
            }
        }));
        if let Err(panic) = outcome {
            let msg = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            return Err(report(format!("panicked: {msg}"), &last));
        }
        if t.elapsed() > STEP_LIMIT {
            return Err(report(format!("a step took {:?}", t.elapsed()), &last));
        }
    }
    // Undo back step by step: the design must pass through where it started. The demo's own
    // construction is on the history too, so the start is the first state equal to it, not an
    // empty history; a change made outside the history never gets back there.
    for _ in 0..10_000 {
        if *app.session.design() == start || !app.session.can_undo() {
            break;
        }
        app.undo();
    }
    if *app.session.design() != start {
        let (a, b) = (
            serde_json::to_value(&start).unwrap_or_default(),
            serde_json::to_value(app.session.design()).unwrap_or_default(),
        );
        let first = first_difference(&a, &b, String::new());
        return Err(report(
            format!("Undo did not return to the start (first difference at {first})"),
            &last,
        ));
    }
    Ok(())
}

/// The path of the first value that differs between two JSON trees.
fn first_difference(a: &Value, b: &Value, at: String) -> String {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    Some(w) if w == v => {}
                    Some(w) => return first_difference(v, w, format!("{at}.{k}")),
                    None => return format!("{at}.{k} (gone)"),
                }
            }
            y.keys()
                .find(|k| !x.contains_key(*k))
                .map_or(at.clone(), |k| format!("{at}.{k} (new)"))
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return format!("{at} ({} against {} entries)", x.len(), y.len());
            }
            x.iter()
                .zip(y)
                .enumerate()
                .find(|(_, (v, w))| v != w)
                .map_or(at.clone(), |(i, (v, w))| first_difference(v, w, format!("{at}[{i}]")))
        }
        _ => at,
    }
}

#[test]
fn the_editor_survives_seeded_random_input_and_undo_returns_to_the_start() {
    let first = env_or("OPENSTITCH_MONKEY_SEED", 880);
    let runs = env_or("OPENSTITCH_MONKEY_RUNS", 2);
    let steps = env_or("OPENSTITCH_MONKEY_STEPS", 150) as usize;
    let dir = openstitch_testutil::TempDir::new("monkey");
    let drop = dir.join("dropped.svg");
    std::fs::write(
        &drop,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm" viewBox="0 0 20 20"><rect width="10" height="10" fill="red"/></svg>"#,
    )
    .expect("drop file");
    let mut failures = Vec::new();
    for seed in first..first + runs {
        if let Err(e) = run(seed, steps, &drop) {
            eprintln!("{e}");
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The generator itself: a seed gives the same steps every time, and every step parses.
#[test]
fn the_monkey_is_reproducible_and_every_step_parses() {
    let app = EditorApp::new(Launch {
        demo: true,
        ..Launch::default()
    });
    let drop = std::path::Path::new("x.svg");
    let steps = |seed| {
        let mut rng = Rng(seed);
        (0..300)
            .map(|_| random_step(&app, &mut rng, drop))
            .collect::<Vec<_>>()
    };
    let a = steps(7);
    assert_eq!(a, steps(7));
    assert_ne!(a, steps(8));
    for s in &a {
        if let Some(key) = s.strip_prefix("run:") {
            assert!(
                openstitch_app::editor::ribbon::Action::from_key(key).is_some(),
                "{key}"
            );
        } else {
            parse(s).unwrap_or_else(|e| panic!("{s}: {e}"));
        }
    }
    for key in ACTIONS {
        assert!(
            openstitch_app::editor::ribbon::Action::from_key(key).is_some(),
            "{key} is not an action"
        );
    }
}

