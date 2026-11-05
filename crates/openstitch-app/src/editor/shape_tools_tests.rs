//! The editor side of `batch:editor-shapes`: continuing a finished line, path, satin skeleton
//! or Manual Punch column from either end with its drawing tool (#479), driven as the canvas
//! drives the tools and run through the engine.

use super::*;
use openstitch_design::{ObjectKind, Vec2, v2};
use openstitch_engine::Session;
use serde_json::json;
use std::collections::BTreeSet;

fn run(s: &mut Session, command: &'static str, params: serde_json::Value) -> u64 {
    s.execute(command, Some(&params))
        .unwrap_or_else(|e| panic!("{command}: {e}"))["id"]
        .as_u64()
        .expect("id")
}

/// Sends one event with `selection` and runs its effects; returns how many commands ran.
fn send(t: &mut ToolController, s: &mut Session, sel: &BTreeSet<u64>, e: PointerEvent) -> usize {
    let design = s.design().clone();
    let effects = t.handle(
        e,
        &ToolContext {
            design: &design,
            selection: sel,
            units_per_point: 1.0,
            scale_takes_origin: false,
        },
    );
    apply(s, effects)
}

fn apply(s: &mut Session, effects: Vec<ToolEffect>) -> usize {
    let mut n = 0;
    for e in effects {
        if let ToolEffect::Execute { requests, .. } = e {
            for r in &requests {
                s.execute(r.command, Some(&r.params))
                    .unwrap_or_else(|e| panic!("{}: {e}", r.command));
                n += 1;
            }
        }
    }
    n
}

fn click(t: &mut ToolController, s: &mut Session, sel: &BTreeSet<u64>, p: Vec2) {
    send(t, s, sel, PointerEvent::new(Phase::Down, p));
    send(t, s, sel, PointerEvent::new(Phase::Up, p));
}

fn ctx<'a>(design: &'a openstitch_design::Design, sel: &'a BTreeSet<u64>) -> ToolContext<'a> {
    ToolContext {
        design,
        selection: sel,
        units_per_point: 1.0,
        scale_takes_origin: false,
    }
}

fn points(s: &Session, id: u64) -> Vec<Vec2> {
    let d = s.design();
    d.objects[d.index_of(id).expect("object")].contours[0]
        .points
        .clone()
}

#[test]
fn the_line_tool_continues_a_line_from_its_end_in_one_undo_step() {
    let mut s = Session::new(std::env::temp_dir());
    let l = run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[10, 10], [20, 10]], "closed": false }),
    );
    let sel = BTreeSet::from([l]);
    let mut t = ToolController::default();
    t.set_tool(Tool::Line);
    let d = s.design().clone();
    // Within 6 screen points of the end; not in the middle of the line.
    let hit = t
        .continue_target(&ctx(&d, &sel), v2(203.0, 102.0))
        .expect("end");
    assert_eq!(hit.id, l);
    assert!(
        t.continue_target(&ctx(&d, &sel), v2(150.0, 100.0))
            .is_none()
    );
    click(&mut t, &mut s, &sel, v2(203.0, 102.0));
    assert!(t.continuation().is_some() && t.is_busy());
    click(&mut t, &mut s, &sel, v2(300.0, 100.0));
    click(&mut t, &mut s, &sel, v2(300.0, 200.0));
    let effects = t.finish();
    assert_eq!(apply(&mut s, effects), 1, "one command, one undo step");
    assert_eq!(
        points(&s, l),
        vec![
            v2(100.0, 100.0),
            v2(200.0, 100.0),
            v2(300.0, 100.0),
            v2(300.0, 200.0)
        ]
    );
    assert_eq!(s.design().objects.len(), 1);
    assert!(!t.is_busy());
    s.execute("history.undo", None).expect("undo");
    assert_eq!(points(&s, l).len(), 2);
}

#[test]
fn the_pen_prepends_at_the_start_and_escape_changes_nothing() {
    let mut s = Session::new(std::env::temp_dir());
    let l = run(
        &mut s,
        "shape.path",
        json!({ "points": [[10, 10], [20, 10]], "closed": false }),
    );
    let sel = BTreeSet::from([l]);
    let mut t = ToolController::default();
    t.set_tool(Tool::Pen);
    click(&mut t, &mut s, &sel, v2(100.0, 100.0));
    click(&mut t, &mut s, &sel, v2(0.0, 100.0));
    // Esc: nothing changes, the tool is free again.
    t.cancel();
    assert!(!t.is_busy());
    assert_eq!(points(&s, l).len(), 2);
    click(&mut t, &mut s, &sel, v2(101.0, 99.0));
    // A drag pulls out handles: the new segment is curved.
    send(
        &mut t,
        &mut s,
        &sel,
        PointerEvent::new(Phase::Down, v2(0.0, 100.0)),
    );
    send(
        &mut t,
        &mut s,
        &sel,
        PointerEvent::new(Phase::Dragged, v2(-10.0, 120.0)),
    );
    send(
        &mut t,
        &mut s,
        &sel,
        PointerEvent::new(Phase::Up, v2(-10.0, 120.0)),
    );
    let effects = t.finish();
    apply(&mut s, effects);
    let p = points(&s, l);
    assert_eq!(p, vec![v2(0.0, 100.0), v2(100.0, 100.0), v2(200.0, 100.0)]);
    let d = s.design();
    assert!(d.objects[0].contours[0].curve(0).is_some());
}

#[test]
fn closed_shapes_and_other_tools_are_not_offered() {
    let mut s = Session::new(std::env::temp_dir());
    let r = run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[10, 10], [20, 10], [20, 20]] }),
    );
    let l = run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[30, 10], [40, 10]], "closed": false }),
    );
    let d = s.design().clone();
    let mut t = ToolController::default();
    for tool in [Tool::Line, Tool::Pen, Tool::Running] {
        t.set_tool(tool);
        let sel = BTreeSet::from([r]);
        assert!(
            t.continue_target(&ctx(&d, &sel), v2(100.0, 100.0))
                .is_none()
        );
    }
    let sel = BTreeSet::from([l]);
    for tool in [Tool::Polygon, Tool::Rectangle, Tool::Punch, Tool::Skeleton] {
        t.set_tool(tool);
        assert!(
            t.continue_target(&ctx(&d, &sel), v2(300.0, 100.0))
                .is_none(),
            "{tool:?}"
        );
    }
    // Two objects selected: nothing to continue.
    t.set_tool(Tool::Line);
    let both = BTreeSet::from([r, l]);
    assert!(
        t.continue_target(&ctx(&d, &both), v2(300.0, 100.0))
            .is_none()
    );
}

#[test]
fn straight_block_adds_direction_lines_to_a_column() {
    let mut s = Session::new(std::env::temp_dir());
    let c = run(
        &mut s,
        "shape.column",
        json!({ "lines": [[[10, 10], [10, 14]], [[14, 10], [14, 14]]] }),
    );
    let sel = BTreeSet::from([c]);
    let mut t = ToolController::default();
    t.set_tool(Tool::Punch);
    // A click on the last direction line continues from the end.
    click(&mut t, &mut s, &sel, v2(141.0, 125.0));
    assert!(t.continuation().is_some());
    click(&mut t, &mut s, &sel, v2(180.0, 100.0));
    click(&mut t, &mut s, &sel, v2(180.0, 140.0));
    let effects = t.finish();
    assert_eq!(apply(&mut s, effects), 1);
    let d = s.design();
    let o = &d.objects[0];
    assert_eq!(o.kind, ObjectKind::Shape);
    assert_eq!(o.region.as_ref().expect("region").direction_lines.len(), 3);
}

#[test]
fn running_and_skeleton_continue_too() {
    let mut s = Session::new(std::env::temp_dir());
    let l = run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[10, 10], [20, 10]], "closed": false }),
    );
    let sel = BTreeSet::from([l]);
    let mut t = ToolController::default();
    t.set_tool(Tool::Running);
    click(&mut t, &mut s, &sel, v2(100.0, 100.0));
    click(&mut t, &mut s, &sel, v2(50.0, 50.0));
    let effects = t.finish();
    apply(&mut s, effects);
    assert_eq!(points(&s, l)[0], v2(50.0, 50.0));

    let k = run(
        &mut s,
        "shape.skeletonSatin",
        json!({ "points": [[30, 30], [50, 30]], "width": 4 }),
    );
    let sel = BTreeSet::from([k]);
    t.set_tool(Tool::Skeleton);
    // A click without a drag on the end point continues; a drag would move it.
    click(&mut t, &mut s, &sel, v2(500.0, 300.0));
    assert!(t.continuation().is_some());
    click(&mut t, &mut s, &sel, v2(600.0, 350.0));
    let effects = t.finish();
    apply(&mut s, effects);
    let d = s.design();
    let sk = d.objects[d.index_of(k).expect("value")]
        .region
        .as_ref()
        .expect("value")
        .skeleton
        .clone()
        .expect("value");
    assert_eq!(sk.points.last(), Some(&v2(600.0, 350.0)));
    assert_eq!(sk.widths.last(), Some(&4.0));
}
