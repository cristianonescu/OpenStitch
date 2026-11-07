//! Coverage for editor claims no other test asserts (#332, and #330's character moves): every
//! inspector number control of every line type and of the main region types reaches the engine
//! and reads back; the object list's visibility toggle; per-character moves on mirrored and on
//! on-path text; the Select tool's Shift axis lock.

use openstitch_app::editor::inspector::{self, Control, InspectorModel};
use openstitch_app::editor::tools::characters::{char_edit, character_frame};
use openstitch_app::editor::{
    CommandRequest, Phase, PointerEvent, Tool, ToolContext, ToolController, ToolEffect, objects,
};
use openstitch_design::{Compiler, Design, RegionType, Vec2, v2};
use openstitch_engine::Session;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn run(s: &mut Session, r: &CommandRequest) -> Value {
    s.execute(r.command, Some(&r.params))
        .unwrap_or_else(|e| panic!("{} {}: {e}", r.command, r.params))
}

fn exec(s: &mut Session, command: &'static str, params: Value) -> Value {
    run(s, &CommandRequest::new(command, params))
}

/// Sends one pointer event with `sel` selected and runs its effects.
fn send(t: &mut ToolController, s: &mut Session, sel: &mut BTreeSet<u64>, e: PointerEvent) {
    let design: Design = s.design().clone();
    let ctx = ToolContext {
        design: &design,
        selection: sel,
        units_per_point: 1.0,
        scale_takes_origin: true,
    };
    let effects = t.handle(e, &ctx);
    for effect in effects {
        match effect {
            ToolEffect::Select(new) => *sel = new,
            ToolEffect::Execute {
                requests,
                select_result,
            } => {
                let mut last = json!(null);
                for r in &requests {
                    last = run(s, r);
                }
                if select_result {
                    *sel = CommandRequest::created_ids(&last);
                }
            }
        }
    }
}

/// Press at `from`, drag to `to` in four steps, release; `shift` is held after the press (a
/// Shift-press toggles the selection instead).
fn drag(
    t: &mut ToolController,
    s: &mut Session,
    sel: &mut BTreeSet<u64>,
    from: Vec2,
    to: Vec2,
    shift: bool,
) {
    let ev = |phase, p| {
        let e = PointerEvent::new(phase, p);
        if shift { e.shift() } else { e }
    };
    send(t, s, sel, PointerEvent::new(Phase::Down, from));
    for k in 1..=4 {
        send(
            t,
            s,
            sel,
            ev(Phase::Dragged, from.lerp(to, f64::from(k) / 4.0)),
        );
    }
    send(t, s, sel, ev(Phase::Up, to));
}

/// A value inside `range`, away from `current`, on the control's grid.
fn new_value(current: f64, range: (f64, f64), integer: bool) -> f64 {
    let mut v = range.0 + (range.1 - range.0) * 0.37;
    if integer {
        v = v.round();
    } else {
        v = (v * 10.0).round() / 10.0;
    }
    if (v - current).abs() < 1e-9 {
        v = if integer { range.0 } else { range.1 };
    }
    v
}

/// Sets every number control in turn and checks the next model shows the value.
fn round_trip(
    s: &mut Session,
    id: u64,
    controls: impl Fn(&Design) -> Vec<Control>,
    set: impl Fn(&InspectorModel, Value) -> CommandRequest,
    what: &str,
) -> usize {
    let sel = BTreeSet::from([id]);
    let mut checked = 0;
    for c in controls(s.design()) {
        let Control::Number {
            key,
            value,
            range,
            integer,
            ..
        } = c
        else {
            continue;
        };
        let v = new_value(value, range, integer);
        let m = InspectorModel::new(s.design(), &sel);
        run(s, &set(&m, c.params(json!(v))));
        let back = controls(s.design())
            .into_iter()
            .find_map(|c| match c {
                Control::Number { key: k, value, .. } if k == key => Some(value),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{what}: control {key} gone after setting it"));
        assert!(
            (back - v).abs() < 1e-6,
            "{what}: {key} set to {v}, reads back {back}"
        );
        checked += 1;
    }
    checked
}

#[test]
fn every_line_control_reaches_the_engine_and_reads_back() {
    let mut s = Session::new(std::env::temp_dir());
    let id = exec(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 40, "height": 20}),
    )["id"]
        .as_u64()
        .expect("id");
    let sel = BTreeSet::from([id]);
    let mut total = 0;
    for (kind, name) in inspector::line_types() {
        let Some(kind) = kind else { continue };
        let m = InspectorModel::new(s.design(), &sel);
        run(&mut s, &m.set_line_type(Some(kind)));
        let line = |d: &Design| d.objects[0].line.clone().expect("line");
        assert_eq!(line(s.design()).kind, kind, "{name}");
        total += round_trip(
            &mut s,
            id,
            |d| inspector::line_controls(&line(d)),
            |m, p| m.set_line(p),
            name,
        );
    }
    assert!(total >= 10, "number controls checked: {total}");
}

#[test]
fn region_controls_of_fill_satin_and_programmable_fill_reach_the_engine() {
    let mut s = Session::new(std::env::temp_dir());
    let id = exec(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 40, "height": 20}),
    )["id"]
        .as_u64()
        .expect("id");
    let sel = BTreeSet::from([id]);
    for kind in [
        RegionType::Fill,
        RegionType::Satin,
        RegionType::ProgrammableFill,
    ] {
        let m = InspectorModel::new(s.design(), &sel);
        run(&mut s, &m.set_region_type(Some(kind)));
        let region = |d: &Design| d.objects[0].region.clone().expect("region");
        assert_eq!(region(s.design()).kind, kind);
        let n = round_trip(
            &mut s,
            id,
            |d| inspector::region_controls(&region(d)),
            |m, p| m.set_region(p),
            inspector::region_name(Some(kind)),
        );
        assert!(n >= 2, "{kind:?}: {n} number controls");
    }
    // Region type "none" removes the region and leaves the line as it was.
    let line = s.design().objects[0].line.clone();
    let m = InspectorModel::new(s.design(), &sel);
    run(&mut s, &m.set_region_type(None));
    let o = &s.design().objects[0];
    assert!(o.region.is_none());
    assert_eq!(o.line, line);
}

/// Cross stitch shows PE-DESIGN's Size and Times and OpenStitch's cross kinds, but no Direction
/// (PE-DESIGN 11.50 has none for it, #614); every choice reaches the engine and reads back.
#[test]
fn cross_stitch_choices_reach_the_engine_and_it_has_no_direction() {
    let mut s = Session::new(std::env::temp_dir());
    let id = exec(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 20, "height": 20}),
    )["id"]
        .as_u64()
        .expect("id");
    let sel = BTreeSet::from([id]);
    let m = InspectorModel::new(s.design(), &sel);
    run(&mut s, &m.set_region_type(Some(RegionType::CrossStitch)));
    let region = |d: &Design| d.objects[0].region.clone().expect("region");
    let controls = |d: &Design| inspector::region_controls(&region(d));
    let params: Vec<&str> = controls(s.design()).iter().map(Control::param).collect();
    assert!(!params.contains(&"angle"), "{params:?}");
    assert!(params.contains(&"spacing"), "Size: {params:?}");
    let mut checked = 0;
    for c in controls(s.design()) {
        let Control::Choice { param, options, .. } = c.clone() else {
            continue;
        };
        for (value, _) in options {
            let m = InspectorModel::new(s.design(), &sel);
            run(&mut s, &m.set_region(c.params(json!(value))));
            let back = controls(s.design())
                .into_iter()
                .find_map(|c| match c {
                    Control::Choice {
                        param: p, value, ..
                    } if p == param => Some(value),
                    _ => None,
                })
                .expect("the choice stays");
            assert_eq!(back, value, "{param}");
            checked += 1;
        }
    }
    assert_eq!(checked, 3 + 4, "Times and Cross options");
}

#[test]
fn hiding_an_object_in_the_list_stops_it_sewing_until_undone() {
    let mut s = Session::new(std::env::temp_dir());
    for x in [0, 40] {
        exec(
            &mut s,
            "shape.rectangle",
            json!({"x": x, "y": 0, "width": 30, "height": 30}),
        );
    }
    let stitches = |s: &Session| Compiler::default().compile(s.design()).stitches.len();
    let both = stitches(&s);
    let id = s.design().objects[1].id;
    run(&mut s, &objects::set_visible(id, false));
    let rows = objects::rows(s.design(), &BTreeSet::new());
    let row = rows.iter().find(|r| r.id == id).expect("row");
    assert!(!row.visible, "the list shows it hidden");
    let one = stitches(&s);
    assert!(one < both && one > 0, "{one} of {both}");
    exec(&mut s, "history.undo", json!({}));
    assert_eq!(stitches(&s), both);
    assert!(
        objects::rows(s.design(), &BTreeSet::new())
            .iter()
            .all(|r| r.visible)
    );
}

#[test]
fn character_moves_follow_a_mirrored_text() {
    let mut s = Session::new(std::env::temp_dir());
    let id = exec(
        &mut s,
        "text.add",
        json!({"text": "AB", "x": 40, "y": 30, "size": 10}),
    )["id"]
        .as_u64()
        .expect("id");
    exec(
        &mut s,
        "object.flip",
        json!({"ids": [id], "axis": "horizontal"}),
    );
    let o = s.design().objects[0].clone();
    let frame = character_frame(s.design(), &o, 0).expect("A");
    // Mirrored: the text's x runs right to left on the page.
    assert!(
        (frame.per_mm[0] - v2(-10.0, 0.0)).length() < 0.2,
        "{:?}",
        frame.per_mm
    );
    let mut sel = BTreeSet::from([id]);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let c = frame.bounds.center();
    drag(&mut t, &mut s, &mut sel, c, c + v2(30.0, 0.0), false);
    let e = char_edit(s.design().objects[0].text.as_ref().expect("text"), 0);
    assert!(
        (e.x + 3.0).abs() < 0.1 && e.y.abs() < 0.1,
        "3 mm right on the page is 3 mm back along the mirrored text: {e:?}"
    );
}

#[test]
fn character_moves_follow_the_texts_path() {
    let mut s = Session::new(std::env::temp_dir());
    let path = exec(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 50, "rx": 30, "ry": 30}),
    )["id"]
        .as_u64()
        .expect("id");
    let id = exec(
        &mut s,
        "text.add",
        json!({"text": "ABCD", "size": 8, "path": path}),
    )["id"]
        .as_u64()
        .expect("id");
    let d = s.design().clone();
    let o = d.objects[d.index_of(id).expect("text")].clone();
    let frame = character_frame(&d, &o, 1).expect("B");
    let along = frame.per_mm[0];
    assert!(
        (along.length() - 10.0).abs() < 1.5,
        "1 mm along the path: {along:?}"
    );
    let mut sel = BTreeSet::from([id]);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let c = frame.bounds.center();
    // Drag 2 mm along the path's direction at the character.
    let to = c + along * (20.0 / along.length());
    drag(&mut t, &mut s, &mut sel, c, to, false);
    let d = s.design();
    let e = char_edit(
        d.objects[d.index_of(id).expect("text")]
            .text
            .as_ref()
            .expect("text"),
        1,
    );
    assert!(
        (e.x - 2.0).abs() < 0.3 && e.y.abs() < 0.3,
        "2 mm along the path is 2 mm along the text: {e:?}"
    );
}

#[test]
fn shift_locks_a_select_move_to_one_axis() {
    let mut s = Session::new(std::env::temp_dir());
    let id = exec(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 20, "height": 20}),
    )["id"]
        .as_u64()
        .expect("id");
    let before = s.design().objects[0].bounds().expect("bounds");
    let mut sel = BTreeSet::from([id]);
    let mut t = ToolController::default();
    t.set_tool(Tool::Select);
    let c = before.center();
    // Mostly right, a little down: Shift keeps only the horizontal part.
    drag(&mut t, &mut s, &mut sel, c, c + v2(50.0, 12.0), true);
    let after = s.design().objects[0].bounds().expect("bounds");
    assert!(
        (after.min_x - before.min_x - 50.0).abs() < 1e-6,
        "{before:?} → {after:?}"
    );
    assert!(
        (after.min_y - before.min_y).abs() < 1e-6,
        "no vertical move"
    );

    // Without Shift the move keeps both parts.
    let c = after.center();
    drag(&mut t, &mut s, &mut sel, c, c + v2(12.0, 50.0), false);
    let free = s.design().objects[0].bounds().expect("bounds");
    assert!((free.min_x - after.min_x - 12.0).abs() < 1e-6);
    assert!((free.min_y - after.min_y - 50.0).abs() < 1e-6);
}

/// The Fill's Texture and Stagger choices (#537) all reach the engine and read back; the
/// steps and curve settings appear with their stagger.
#[test]
fn fill_texture_choices_reach_the_engine() {
    let mut s = Session::new(std::env::temp_dir());
    let id = exec(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 20, "height": 20, "fill": "Red"}),
    )["id"]
        .as_u64()
        .expect("id");
    let sel = BTreeSet::from([id]);
    let region = |d: &Design| d.objects[0].region.clone().expect("region");
    let controls = |d: &Design| inspector::region_controls(&region(d));
    let choice = |d: &Design, param: &str| {
        controls(d).into_iter().find_map(|c| match c {
            Control::Choice {
                param: p, value, ..
            } if p == param => Some(value),
            _ => None,
        })
    };
    assert_eq!(choice(s.design(), "texture").as_deref(), Some("smooth"));
    for t in &openstitch_design::FILL_TEXTURES {
        let m = InspectorModel::new(s.design(), &sel);
        let c = controls(s.design())
            .into_iter()
            .find(|c| c.param() == "texture")
            .expect("texture");
        run(&mut s, &m.set_region(c.params(json!(t.id))));
        assert_eq!(choice(s.design(), "texture").as_deref(), Some(t.id));
    }
    for (stagger, extra) in [
        ("steps", ["staggerSteps", "staggerRepeat"]),
        ("wave", ["staggerAmplitude", "staggerPeriod"]),
        ("arc", ["staggerAmplitude", "staggerPeriod"]),
    ] {
        let m = InspectorModel::new(s.design(), &sel);
        let c = controls(s.design())
            .into_iter()
            .find(|c| c.param() == "stagger")
            .expect("stagger");
        run(&mut s, &m.set_region(c.params(json!(stagger))));
        assert_eq!(choice(s.design(), "stagger").as_deref(), Some(stagger));
        let params: Vec<String> = controls(s.design())
            .iter()
            .map(|c| c.param().to_string())
            .collect();
        for p in extra {
            assert!(params.iter().any(|x| x == p), "{stagger}: {params:?}");
        }
        // A setting matching no texture reads as custom, and choosing custom keeps it.
        let m = InspectorModel::new(s.design(), &sel);
        run(&mut s, &m.set_region(json!({ extra[0]: 7 })));
        assert_eq!(choice(s.design(), "texture").as_deref(), Some("custom"));
        let before = region(s.design());
        let m = InspectorModel::new(s.design(), &sel);
        run(&mut s, &m.set_region(json!({"texture": "custom"})));
        assert_eq!(region(s.design()), before);
    }
}

/// Maze fill (#533, beyond PE-DESIGN): listed among the region sew types; its Spacing, Run
/// pitch, Straightness and Seed reach the engine and read back.
#[test]
fn maze_fill_controls_reach_the_engine() {
    let mut s = Session::new(std::env::temp_dir());
    let id = exec(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 30, "height": 30}),
    )["id"]
        .as_u64()
        .expect("id");
    let sel = BTreeSet::from([id]);
    assert!(
        inspector::region_types()
            .iter()
            .any(|(k, _)| *k == Some(RegionType::Maze))
    );
    let m = InspectorModel::new(s.design(), &sel);
    run(&mut s, &m.set_region_type(Some(RegionType::Maze)));
    let region = |d: &Design| d.objects[0].region.clone().expect("region");
    assert_eq!(region(s.design()).kind, RegionType::Maze);
    let n = round_trip(
        &mut s,
        id,
        |d| inspector::region_controls(&region(d)),
        |m, p| m.set_region(p),
        "maze fill",
    );
    assert!(n >= 5, "{n} number controls");
    let r = region(s.design());
    assert!(r.straightness.is_some() && r.maze_seed.is_some(), "{r:?}");
}
