use super::hit::{bounds_of, object_at, objects_in, rect_from};
use super::inspector::{Common, InspectorModel, pec_index, pec_palette};
use super::render::{PatternStats, RenderModel, color_at};
use super::*;
use openstitch_core::{Command, Stitch, StitchPattern, Thread};
use openstitch_design::{Compiler, Design, RegionType, Vec2, v2};
use openstitch_engine::Session;
use serde_json::json;
use std::collections::BTreeSet;

fn session() -> Session {
    Session::new(std::env::temp_dir())
}

fn run(s: &mut Session, r: &CommandRequest) -> serde_json::Value {
    s.execute(r.command, Some(&r.params))
        .unwrap_or_else(|e| panic!("{} failed: {e}", r.command))
}

/// Applies tool effects the way the app does; returns the new selection.
fn apply(s: &mut Session, effects: Vec<ToolEffect>, mut selection: BTreeSet<u64>) -> BTreeSet<u64> {
    for e in effects {
        match e {
            ToolEffect::Select(sel) => selection = sel,
            ToolEffect::Execute {
                requests,
                select_result,
            } => {
                let mut last = json!(null);
                for r in &requests {
                    last = run(s, r);
                }
                if select_result {
                    selection = CommandRequest::created_ids(&last);
                }
            }
        }
    }
    selection
}

fn ctx<'a>(design: &'a Design, selection: &'a BTreeSet<u64>) -> ToolContext<'a> {
    ToolContext {
        design,
        selection,
        units_per_point: 1.0,
        scale_takes_origin: false,
    }
}

fn drag(
    tool: &mut ToolController,
    s: &mut Session,
    sel: BTreeSet<u64>,
    from: Vec2,
    to: Vec2,
    shift: bool,
) -> BTreeSet<u64> {
    let mut sel = sel;
    let mid = from.lerp(to, 0.5);
    for (phase, p) in [
        (Phase::Down, from),
        (Phase::Dragged, mid),
        (Phase::Dragged, to),
        (Phase::Up, to),
    ] {
        let mut e = PointerEvent::new(phase, p);
        e.shift = shift;
        let design = s.design().clone();
        let effects = tool.handle(e, &ctx(&design, &sel));
        sel = apply(s, effects, sel);
    }
    sel
}

fn rect_design(s: &mut Session) -> u64 {
    let r = run(
        s,
        &CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 10, "y": 10, "width": 20, "height": 10 }),
        ),
    );
    r["id"].as_u64().expect("id")
}

// MARK: viewport

#[test]
fn viewport_round_trips_and_zooms_about_anchor() {
    let mut v = Viewport {
        scale: 2.0,
        offset: v2(50.0, 20.0),
    };
    let p = v2(123.0, -45.0);
    let back = v.to_model(v.to_screen(p));
    assert!(back.distance(p) < 1e-9);
    let anchor = v2(300.0, 200.0);
    let under = v.to_model(anchor);
    v.zoom_about(anchor, 1.5);
    assert!((v.scale - 3.0).abs() < 1e-12);
    assert!(v.to_model(anchor).distance(under) < 1e-9);
    v.zoom_about(anchor, 1e9);
    assert_eq!(v.scale, Viewport::MAX_SCALE);
    v.zoom_about(anchor, f64::NAN);
    assert_eq!(v.scale, Viewport::MAX_SCALE);
    v.pan(10.0, -5.0);
    assert!(v.to_model(anchor + v2(10.0, -5.0)).distance(under) < 1e-9);
}

#[test]
fn viewport_fit_centres_page_with_margin() {
    let page = openstitch_design::Rect {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 1000.0,
        max_y: 500.0,
    };
    let screen = ScreenRect {
        x: 100.0,
        y: 50.0,
        width: 840.0,
        height: 640.0,
    };
    let v = Viewport::fit(page, screen, 20.0);
    assert!((v.scale - 0.8).abs() < 1e-12); // width-limited: 800 / 1000
    let c = v.to_screen(v2(500.0, 250.0));
    assert!(c.distance(screen.center()) < 1e-9);
    assert!((v.to_screen(v2(0.0, 0.0)).x - 120.0).abs() < 1e-9);
}

// MARK: hit testing

#[test]
fn hit_testing_prefers_topmost_and_outline_tolerance() {
    let mut s = session();
    let a = rect_design(&mut s);
    let b = run(
        &mut s,
        &CommandRequest::new("shape.ellipse", json!({ "cx": 20, "cy": 15, "rx": 3 })),
    )["id"]
        .as_u64()
        .expect("id");
    let d = s.design();
    assert_eq!(
        object_at(d, v2(200.0, 150.0), 1.0),
        Some(b),
        "ellipse is sewn later, so on top"
    );
    assert_eq!(object_at(d, v2(120.0, 120.0), 1.0), Some(a));
    assert_eq!(
        object_at(d, v2(97.0, 120.0), 4.0),
        Some(a),
        "near the outline"
    );
    assert_eq!(object_at(d, v2(90.0, 120.0), 4.0), None);
    assert_eq!(
        objects_in(d, &rect_from(v2(150.0, 100.0), v2(250.0, 200.0))),
        BTreeSet::from([b])
    );
    let all = BTreeSet::from([a, b]);
    let bb = bounds_of(d, &all).expect("bounds");
    assert_eq!((bb.min_x, bb.max_x), (100.0, 300.0));
    run(&mut s, &objects::set_visible(b, false));
    assert_eq!(
        object_at(s.design(), v2(200.0, 150.0), 1.0),
        Some(a),
        "hidden objects are not hit"
    );
}

// MARK: handles

#[test]
fn handles_scale_about_opposite_corner_and_rotate() {
    let h = SelectionHandles {
        bounds: rect_from(v2(0.0, 0.0), v2(100.0, 50.0)),
        units_per_point: 1.0,
    };
    assert_eq!(h.handle_at(v2(101.0, 51.0)), Some(Handle::BottomRight));
    assert_eq!(h.handle_at(v2(50.0, -22.0)), Some(Handle::Rotate));
    assert_eq!(h.handle_at(v2(50.0, 25.0)), None);
    let s = h.scale(Handle::BottomRight, v2(100.0, 50.0), v2(200.0, 75.0), false);
    assert_eq!((s.sx, s.sy, s.origin), (2.0, 1.5, v2(0.0, 0.0)));
    let p = h.scale(Handle::BottomRight, v2(100.0, 50.0), v2(200.0, 75.0), true);
    assert_eq!((p.sx, p.sy), (2.0, 2.0));
    let e = h.scale(Handle::Right, v2(100.0, 25.0), v2(-50.0, 90.0), false);
    assert_eq!(
        (e.sx, e.sy),
        (0.01, 1.0),
        "no mirroring past the fixed edge; edge handles keep height"
    );
    let deg = h.rotation(v2(100.0, 25.0), v2(50.0, 125.0), false);
    assert!((deg - 90.0).abs() < 1e-9, "clockwise on screen is positive");
    assert_eq!(h.rotation(v2(100.0, 25.0), v2(100.0, 28.0), true), 0.0);
    assert_eq!(h.rotation(v2(100.0, 25.0), v2(100.0, 32.0), true), 15.0);
}

// MARK: tools through the engine

#[test]
fn rectangle_and_ellipse_tools_create_and_select() {
    let mut s = session();
    let mut t = ToolController::default();
    t.set_tool(Tool::Rectangle);
    let sel = drag(
        &mut t,
        &mut s,
        BTreeSet::new(),
        v2(100.0, 100.0),
        v2(300.0, 150.0),
        false,
    );
    assert_eq!(sel.len(), 1);
    let o = &s.design().objects[0];
    let b = o.bounds().expect("bounds");
    assert_eq!(
        (b.min_x, b.min_y, b.width(), b.height()),
        (100.0, 100.0, 200.0, 50.0)
    );
    assert_eq!(o.region.as_ref().map(|r| r.kind), Some(RegionType::Fill));

    t.set_tool(Tool::Ellipse);
    let sel = drag(&mut t, &mut s, sel, v2(0.0, 0.0), v2(80.0, 30.0), true);
    let b = s.design().objects[1].bounds().expect("bounds");
    assert!(
        (b.width() - 80.0).abs() < 0.01 && (b.height() - 80.0).abs() < 0.01,
        "shift draws a circle"
    );
    assert_eq!(sel, BTreeSet::from([s.design().objects[1].id]));

    drag(&mut t, &mut s, sel, v2(0.0, 0.0), v2(3.0, 3.0), false);
    assert_eq!(s.design().objects.len(), 2, "tiny drags create nothing");
}

#[test]
fn polygon_tool_closes_on_first_vertex_double_click_or_return() {
    let mut s = session();
    let mut t = ToolController::default();
    t.set_tool(Tool::Polygon);
    let empty = BTreeSet::new();
    let d = Design::default();
    for p in [v2(0.0, 0.0), v2(100.0, 0.0), v2(100.0, 0.5), v2(50.0, 80.0)] {
        assert!(
            t.handle(PointerEvent::new(Phase::Down, p), &ctx(&d, &empty))
                .is_empty()
        );
    }
    assert_eq!(
        t.polygon().len(),
        3,
        "a click on the last vertex adds nothing"
    );
    t.handle(
        PointerEvent::new(Phase::Moved, v2(10.0, 60.0)),
        &ctx(&d, &empty),
    );
    assert!(matches!(t.preview(), Some(ToolPreview::Outline(c)) if c.points.len() == 4));
    let effects = t.handle(
        PointerEvent::new(Phase::Down, v2(1.0, 1.0)),
        &ctx(&d, &empty),
    );
    let sel = apply(&mut s, effects, BTreeSet::new());
    assert_eq!(sel.len(), 1);
    assert_eq!(s.design().objects[0].contours[0].points.len(), 3);
    assert!(!t.is_busy());

    t.handle(
        PointerEvent::new(Phase::Down, v2(0.0, 0.0)),
        &ctx(&d, &empty),
    );
    t.handle(
        PointerEvent::new(Phase::Down, v2(50.0, 0.0)),
        &ctx(&d, &empty),
    );
    assert!(t.finish_polygon().is_empty(), "two vertices are discarded");
    for p in [v2(0.0, 0.0), v2(50.0, 0.0), v2(50.0, 50.0)] {
        t.handle(PointerEvent::new(Phase::Down, p), &ctx(&d, &empty));
    }
    let mut dbl = PointerEvent::new(Phase::Down, v2(50.0, 50.0));
    dbl.clicks = 2;
    assert_eq!(t.handle(dbl, &ctx(&d, &empty)).len(), 1);
}

#[test]
fn select_tool_moves_marquees_and_toggles() {
    let mut s = session();
    let a = rect_design(&mut s);
    let b = run(
        &mut s,
        &CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 50, "y": 50, "width": 10, "height": 10 }),
        ),
    )["id"]
        .as_u64()
        .expect("id");
    let mut t = ToolController::default();
    // Click selects; drag moves.
    let sel = drag(
        &mut t,
        &mut s,
        BTreeSet::new(),
        v2(150.0, 150.0),
        v2(170.0, 140.0),
        false,
    );
    assert_eq!(sel, BTreeSet::from([a]));
    let bb = s.design().objects[0].bounds().expect("bounds");
    assert_eq!((bb.min_x, bb.min_y), (120.0, 90.0));
    // Shift-click toggles the other object in.
    let design = s.design().clone();
    let effects = t.handle(
        PointerEvent::new(Phase::Down, v2(550.0, 550.0)).shift(),
        &ctx(&design, &sel),
    );
    let sel = apply(&mut s, effects, sel);
    t.handle(
        PointerEvent::new(Phase::Up, v2(550.0, 550.0)),
        &ctx(&design, &sel),
    );
    assert_eq!(sel, BTreeSet::from([a, b]));
    // Marquee from empty space selects only fully enclosed objects.
    let sel = drag(
        &mut t,
        &mut s,
        sel,
        v2(400.0, 400.0),
        v2(700.0, 700.0),
        false,
    );
    assert_eq!(sel, BTreeSet::from([b]));
    assert!(t.preview().is_none());
    // Click on empty canvas clears.
    let sel = drag(&mut t, &mut s, sel, v2(900.0, 10.0), v2(900.0, 10.0), false);
    assert!(sel.is_empty());
    // Move is one undo step.
    assert!(s.can_undo());
}

#[test]
fn handle_scale_keeps_opposite_corner_fixed_through_engine() {
    for takes_origin in [false, true] {
        let mut s = session();
        let a = rect_design(&mut s); // 100,100 – 300,200
        let sel = BTreeSet::from([a]);
        let design = s.design().clone();
        let mut c = ctx(&design, &sel);
        c.scale_takes_origin = takes_origin;
        let mut t = ToolController::default();
        t.handle(PointerEvent::new(Phase::Down, v2(300.0, 200.0)), &c);
        t.handle(PointerEvent::new(Phase::Dragged, v2(500.0, 300.0)), &c);
        assert!(matches!(t.preview(), Some(ToolPreview::Transform(_))));
        let effects = t.handle(PointerEvent::new(Phase::Up, v2(500.0, 300.0)), &c);
        let ToolEffect::Execute { requests, .. } = &effects[0] else {
            panic!("expected commands")
        };
        assert_eq!(requests.len(), if takes_origin { 1 } else { 2 });
        if takes_origin {
            assert_eq!(requests[0].params["originX"], json!(10.0));
            continue; // the current engine scales about the centre; only the request shape is checked
        }
        apply(&mut s, effects, sel.clone());
        let b = s.design().objects[0].bounds().expect("bounds");
        assert!(
            (b.min_x - 100.0).abs() < 0.01 && (b.min_y - 100.0).abs() < 0.01,
            "{b:?}"
        );
        assert!(
            (b.max_x - 500.0).abs() < 0.01 && (b.max_y - 300.0).abs() < 0.01,
            "{b:?}"
        );
    }
}

#[test]
fn rotate_handle_issues_rotation() {
    let mut s = session();
    let a = rect_design(&mut s);
    let sel = BTreeSet::from([a]);
    let mut t = ToolController::default();
    let rotate = v2(200.0, 100.0 - SelectionHandles::ROTATE_OFFSET);
    let to = v2(200.0 + 200.0, 150.0); // quarter turn clockwise about the centre (200, 150)
    drag(&mut t, &mut s, sel, rotate, to, false);
    let b = s.design().objects[0].bounds().expect("bounds");
    assert!(
        (b.width() - 100.0).abs() < 0.01 && (b.height() - 200.0).abs() < 0.01,
        "{b:?}"
    );
}

#[test]
fn tool_shortcuts_and_cancel() {
    assert_eq!(Tool::from_shortcut('E'), Some(Tool::Ellipse));
    assert_eq!(Tool::from_shortcut('x'), None);
    let mut t = ToolController::default();
    t.set_tool(Tool::Polygon);
    let d = Design::default();
    let sel = BTreeSet::new();
    t.handle(PointerEvent::new(Phase::Down, v2(0.0, 0.0)), &ctx(&d, &sel));
    assert!(t.is_busy());
    t.cancel();
    assert!(!t.is_busy() && t.preview().is_none());
}

// MARK: render model

fn pattern() -> StitchPattern {
    let mut p = StitchPattern::new();
    p.threads = vec![Thread::rgb(0xff0000), Thread::rgb(0x0000ff)];
    p.stitches = vec![
        Stitch::new(0.0, 0.0, Command::Jump),
        Stitch::new(0.0, 0.0, Command::Stitch),
        Stitch::new(10.0, 0.0, Command::Stitch),
        Stitch::new(20.0, 0.0, Command::Stitch),
        Stitch::new(20.0, 0.0, Command::Trim),
        Stitch::new(50.0, 50.0, Command::Jump),
        Stitch::new(50.0, 50.0, Command::Stitch),
        Stitch::new(60.0, 50.0, Command::Stitch),
        Stitch::new(60.0, 50.0, Command::ColorChange),
        Stitch::new(60.0, 60.0, Command::Stitch),
        Stitch::new(70.0, 60.0, Command::Stitch),
        Stitch::new(70.0, 60.0, Command::End),
    ];
    p
}

#[test]
fn render_model_splits_colours_strokes_and_travel() {
    let m = RenderModel::build(&pattern(), usize::MAX);
    assert_eq!(m.layers.len(), 2);
    assert_eq!(m.layers[0].rgb, [255, 0, 0]);
    assert_eq!(
        m.layers[0].strokes.len(),
        2,
        "the trim + jump breaks the stroke"
    );
    assert_eq!(
        m.layers[0].strokes[0],
        vec![v2(0.0, 0.0), v2(0.0, 0.0), v2(10.0, 0.0), v2(20.0, 0.0)]
    );
    assert_eq!(
        m.layers[1].strokes,
        vec![vec![v2(60.0, 50.0), v2(60.0, 60.0), v2(70.0, 60.0)]]
    );
    assert_eq!(m.travel, vec![(v2(20.0, 0.0), v2(50.0, 50.0))]);
    assert_eq!((m.shown, m.needle), (7, Some(v2(70.0, 60.0))));
}

#[test]
fn trimmed_travel_is_marked_cut_and_a_plain_jump_is_not() {
    // The Realistic view leaves out travel the machine trims (#450); a jump without a trim or
    // colour change leaves its thread on the fabric and stays.
    assert_eq!(
        RenderModel::build(&pattern(), usize::MAX).travel_cut,
        vec![true]
    );
    let mut p = pattern();
    p.stitches.remove(4);
    let m = RenderModel::build(&p, usize::MAX);
    assert_eq!(m.travel, vec![(v2(20.0, 0.0), v2(50.0, 50.0))]);
    assert_eq!(m.travel_cut, vec![false]);
}

#[test]
fn render_model_stops_at_simulator_position() {
    let m = RenderModel::build(&pattern(), 4);
    assert_eq!(m.shown, 4);
    assert_eq!(m.layers.len(), 1);
    assert_eq!(m.needle, Some(v2(50.0, 50.0)));
    assert_eq!(RenderModel::build(&pattern(), 0), RenderModel::default());
    assert_eq!(color_at(&pattern(), 5), (0, 2));
    assert_eq!(color_at(&pattern(), 6), (1, 2));
    let mut few = pattern();
    few.threads.truncate(1);
    assert_eq!(
        RenderModel::build(&few, usize::MAX).layers[1].rgb,
        render::UNKNOWN_THREAD
    );
}

#[test]
fn stats_match_engine_and_compiled_design() {
    let mut s = session();
    rect_design(&mut s);
    run(
        &mut s,
        &CommandRequest::new(
            "shape.ellipse",
            json!({ "cx": 50, "cy": 50, "rx": 10, "outline": "Red", "fill": "#00ff00" }),
        ),
    );
    let p = Compiler::default().compile(s.design());
    let st = PatternStats::of(&p);
    let engine = run(&mut s, &CommandRequest::new("design.generate", json!({})));
    assert_eq!(st.stitches as u64, engine["stitches"].as_u64().expect("n"));
    assert_eq!(
        st.color_changes as u64,
        engine["colorChanges"].as_u64().expect("n")
    );
    assert_eq!(st.width_mm, engine["widthMM"].as_f64().expect("w"));
    assert_eq!(st.colors.len(), 3);
    let m = RenderModel::build(&p, usize::MAX);
    assert_eq!(m.shown, st.stitches);
}

// MARK: inspector and object list

#[test]
fn inspector_summarises_mixed_selection_and_edits_through_engine() {
    let mut s = session();
    let a = rect_design(&mut s);
    let b = run(
        &mut s,
        &CommandRequest::new(
            "shape.polygon",
            json!({ "points": [[0, 0], [10, 0]], "closed": false }),
        ),
    )["id"]
        .as_u64()
        .expect("id");
    let both = BTreeSet::from([a, b]);
    let m = InspectorModel::new(s.design(), &both);
    assert!(!m.closed, "the polyline has no region");
    assert_eq!(m.region_kind, Common::Mixed);
    assert_eq!(m.line_kind, Common::Mixed);

    let only = BTreeSet::from([a]);
    let m = InspectorModel::new(s.design(), &only);
    assert!(m.closed);
    assert_eq!(m.region_kind, Common::Same(Some(RegionType::Fill)));
    run(&mut s, &m.set_region(json!({ "angle": 45, "density": 4, "stitchLength": 3, "pullCompensation": 0.2, "underlay": true })));
    run(&mut s, &m.set_region(json!({ "color": 5 })));
    run(&mut s, &m.set_region_type(Some(RegionType::Satin)));
    run(
        &mut s,
        &m.set_line_type(Some(openstitch_design::LineType::Zigzag)),
    );
    // Hole sewing pairs the rectangle with a region inside it.
    run(
        &mut s,
        &CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 15, "y": 12, "width": 4, "height": 4 }),
        ),
    );
    let hole = m.set_hole_sewing(s.design(), true).expect("has region");
    run(&mut s, &hole);
    let o = &s.design().objects[0];
    let r = o.region.as_ref().expect("region");
    assert_eq!(
        (r.kind, r.angle),
        (RegionType::Satin, 45.0),
        "type change keeps the angle"
    );
    assert_eq!(pec_index(&r.thread), Some(5));
    assert!(o.hole_sewing);
    assert_eq!(
        o.line.as_ref().map(|l| l.kind),
        Some(openstitch_design::LineType::Zigzag)
    );
    let m = InspectorModel::new(s.design(), &both);
    assert!(
        m.set_hole_sewing(s.design(), false)
            .is_some_and(|r| r.params["ids"] == json!([a]))
    );
    assert_eq!(pec_palette().len(), 64);
}

#[test]
fn object_list_rows_and_reorder() {
    let mut s = session();
    let a = rect_design(&mut s);
    let b = run(
        &mut s,
        &CommandRequest::new(
            "shape.ellipse",
            json!({ "cx": 5, "cy": 5, "rx": 2, "name": "Dot" }),
        ),
    )["id"]
        .as_u64()
        .expect("id");
    let rows = objects::rows(s.design(), &BTreeSet::from([b]));
    assert_eq!(
        rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
        ["Shape 1", "Dot"]
    );
    assert!(rows[1].selected && !rows[0].selected);
    assert_eq!(rows[0].detail, "fill");
    assert_eq!(rows[0].swatches.len(), 1);
    assert!(objects::shift(s.design(), a, -1).is_none());
    let down = objects::shift(s.design(), a, 1).expect("can move down");
    run(&mut s, &down);
    assert_eq!(
        s.design().objects.iter().map(|o| o.id).collect::<Vec<_>>(),
        [b, a]
    );
    assert_eq!(objects::drop_index(0, 2), 1);
    assert_eq!(objects::drop_index(1, 0), 0);
    run(&mut s, &objects::reorder(a, objects::drop_index(1, 0)));
    assert_eq!(s.design().objects[0].id, a);
    s.execute("history.undo", None).expect("undo");
    assert_eq!(s.design().objects[0].id, b);
}

#[test]
fn transform_requests_run() {
    let mut s = session();
    let a = rect_design(&mut s);
    let sel = BTreeSet::from([a]);
    run(&mut s, &CommandRequest::flip(&sel, true));
    run(&mut s, &CommandRequest::rotate(&sel, 90.0));
    let created = CommandRequest::created_ids(&run(&mut s, &CommandRequest::duplicate(&sel)));
    assert_eq!(created.len(), 1);
    run(&mut s, &CommandRequest::delete(&created));
    assert_eq!(s.design().objects.len(), 1);
    assert_eq!(request::mm(123.456), 12.35);
}

// MARK: point editing and pen

use super::pen::{PenNode, path_request, pen_contour};
use super::points::{
    Control, PointHit, cubic_at, handle_lines, nearest_segment, point_at, segment_at, smooth_curve,
    with_vertex,
};
use openstitch_design::{Bezier, Contour};

/// Sends one pointer event and applies its effects.
fn send(
    tool: &mut ToolController,
    s: &mut Session,
    sel: BTreeSet<u64>,
    e: PointerEvent,
) -> BTreeSet<u64> {
    let design = s.design().clone();
    let effects = tool.handle(e, &ctx(&design, &sel));
    apply(s, effects, sel)
}

fn click_at(
    tool: &mut ToolController,
    s: &mut Session,
    sel: BTreeSet<u64>,
    p: Vec2,
    clicks: u32,
) -> BTreeSet<u64> {
    let mut e = PointerEvent::new(Phase::Down, p);
    e.clicks = clicks;
    let sel = send(tool, s, sel, e);
    send(tool, s, sel, PointerEvent::new(Phase::Up, p))
}

/// Rectangle (100,100)-(300,200) whose top segment is a curve bulging upwards.
fn curved_rect(s: &mut Session) -> u64 {
    let id = rect_design(s);
    run(
        s,
        &CommandRequest::new(
            "object.setSegment",
            json!({ "id": id, "segment": 0, "curve": [[15, 5], [25, 5]] }),
        ),
    );
    id
}

fn undo(s: &mut Session) {
    s.execute("history.undo", None).expect("undo");
}

#[test]
fn nearest_segment_finds_parameter_on_lines_and_curves() {
    let c = Contour::rectangle(0.0, 0.0, 100.0, 50.0);
    let (seg, t, d) = nearest_segment(&c, v2(25.0, -3.0)).expect("segment");
    assert_eq!(seg, 0);
    assert!((t - 0.25).abs() < 1e-9 && (d - 3.0).abs() < 1e-9);
    let (seg, t, _) = nearest_segment(&c, v2(-2.0, 40.0)).expect("closing segment");
    assert_eq!(seg, 3);
    assert!((t - 0.2).abs() < 1e-9);

    let mut curved = Contour::new(vec![v2(0.0, 0.0), v2(100.0, 0.0)], false);
    curved.set_curve(
        0,
        Some(Bezier {
            c1: v2(10.0, -80.0),
            c2: v2(90.0, -80.0),
        }),
    );
    for t in [0.1, 0.37, 0.5, 0.83] {
        let on = segment_at(&curved, 0, t);
        let (_, found, d) = nearest_segment(&curved, on).expect("segment");
        assert!(d < 1e-6, "point on the curve is at distance {d}");
        assert!((found - t).abs() < 1e-4, "t {found} vs {t}");
    }
    // Off the curve: the found point is the true nearest (no sample beats it).
    let p = v2(30.0, -20.0);
    let (_, t, d) = nearest_segment(&curved, p).expect("segment");
    assert!(
        (0..=1000).all(|k| p.distance(segment_at(&curved, 0, f64::from(k) / 1000.0)) >= d - 1e-6)
    );
    assert!(
        segment_at(&curved, 0, t).distance(cubic_at(
            v2(0.0, 0.0),
            v2(10.0, -80.0),
            v2(90.0, -80.0),
            v2(100.0, 0.0),
            t
        )) < 1e-12
    );
}

#[test]
fn point_hits_prefer_nearest_vertex_or_handle_then_segment() {
    let mut s = session();
    let id = curved_rect(&mut s);
    let o = s.design().objects[0].clone();
    assert_eq!(o.id, id);
    assert_eq!(handle_lines(&o.contours[0]).len(), 2);
    assert_eq!(
        point_at(&o, v2(302.0, 199.0), 7.0, 4.0),
        Some(PointHit::Vertex {
            contour: 0,
            index: 2
        })
    );
    assert_eq!(
        point_at(&o, v2(151.0, 52.0), 7.0, 4.0),
        Some(PointHit::Handle {
            contour: 0,
            segment: 0,
            control: Control::Start
        })
    );
    // On the right edge halfway down: a segment hit at t = 0.5.
    match point_at(&o, v2(302.0, 150.0), 7.0, 4.0) {
        Some(PointHit::Segment {
            contour: 0,
            segment: 1,
            t,
        }) => assert!((t - 0.5).abs() < 1e-9),
        other => panic!("expected segment 1, got {other:?}"),
    }
    assert_eq!(point_at(&o, v2(200.0, 150.0), 7.0, 4.0), None);
    // A vertex wins a tie with a handle on top of it.
    let mut c = o.clone();
    c.contours[0].set_curve(
        1,
        Some(Bezier {
            c1: v2(300.0, 200.0),
            c2: v2(300.0, 200.0),
        }),
    );
    assert_eq!(
        point_at(&c, v2(300.0, 200.0), 7.0, 4.0),
        Some(PointHit::Vertex {
            contour: 0,
            index: 2
        })
    );
}

#[test]
fn preview_edits_match_the_engine() {
    let mut s = session();
    let id = curved_rect(&mut s);
    for index in 0..4 {
        let c = s.design().objects[0].contours[0].clone();
        let expected = with_vertex(&c, index, c.points[index] + v2(30.0, -20.0));
        let v = expected.points[index];
        run(
            &mut s,
            &CommandRequest::new(
                "object.setPoint",
                json!({ "id": id, "index": index, "x": v.x / 10.0, "y": v.y / 10.0 }),
            ),
        );
        let got = &s.design().objects[0].contours[0];
        assert_eq!(got.curves.len(), expected.curves.len());
        for i in 0..got.segment_count() {
            let (a, b) = (got.curve(i), expected.curve(i));
            assert_eq!(a.is_some(), b.is_some());
            if let (Some(a), Some(b)) = (a, b) {
                assert!(a.c1.distance(b.c1) < 1e-9 && a.c2.distance(b.c2) < 1e-9);
            }
        }
    }
}

#[test]
fn point_tool_drags_vertex_as_one_undoable_command() {
    let mut s = session();
    let id = rect_design(&mut s);
    let before = s.design().clone();
    let mut t = ToolController::default();
    t.set_tool(Tool::from_shortcut('a').expect("point shortcut"));
    assert_eq!(t.tool(), Tool::Point);
    let sel = BTreeSet::from([id]);
    let sel = drag(
        &mut t,
        &mut s,
        sel,
        v2(300.0, 200.0),
        v2(350.0, 240.0),
        false,
    );
    assert_eq!(sel, BTreeSet::from([id]), "selection kept");
    assert_eq!(
        s.design().objects[0].contours[0].points[2],
        v2(350.0, 240.0)
    );
    let design = s.design().clone();
    let overlay = t.point_overlay(&ctx(&design, &sel)).expect("overlay");
    assert_eq!(overlay.selected, Some((0, 2)));
    undo(&mut s);
    assert_eq!(*s.design(), before, "one Undo reverts the drag");

    // The overlay follows the drag before release; nothing is committed yet.
    let design = s.design().clone();
    let c = ctx(&design, &sel);
    t.handle(PointerEvent::new(Phase::Down, v2(100.0, 100.0)), &c);
    t.handle(PointerEvent::new(Phase::Dragged, v2(80.0, 90.0)), &c);
    let o = t.point_overlay(&c).expect("overlay");
    assert_eq!(o.contours[0].points[0], v2(80.0, 90.0));
    t.cancel();
    assert_eq!(*s.design(), before);

    // Shift locks the move to an axis; a click without a drag changes nothing.
    let sel = drag(
        &mut t,
        &mut s,
        sel,
        v2(100.0, 200.0),
        v2(140.0, 210.0),
        true,
    );
    assert_eq!(
        s.design().objects[0].contours[0].points[3],
        v2(140.0, 200.0)
    );
    undo(&mut s);
    click_at(&mut t, &mut s, sel, v2(300.0, 100.0), 1);
    assert_eq!(*s.design(), before);
}

#[test]
fn point_tool_drags_handle_and_selects_shapes() {
    let mut s = session();
    let id = curved_rect(&mut s);
    let other = run(
        &mut s,
        &CommandRequest::new("shape.ellipse", json!({ "cx": 60, "cy": 60, "rx": 5 })),
    )["id"]
        .as_u64()
        .expect("id");
    let before = s.design().clone();
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    // No target yet: clicking a shape selects it.
    let sel = click_at(&mut t, &mut s, BTreeSet::new(), v2(200.0, 150.0), 1);
    assert_eq!(sel, BTreeSet::from([id]));
    // A handle shows, and can be dragged, once its point is selected (PE-DESIGN's, #1260):
    // before that a press there is a press on empty space.
    drag(&mut t, &mut s, sel, v2(250.0, 50.0), v2(260.0, 20.0), false);
    assert_eq!(
        *s.design(),
        before,
        "an unselected point's handle is not there"
    );
    let sel = click_at(&mut t, &mut s, BTreeSet::from([id]), v2(300.0, 100.0), 1);
    let sel = drag(&mut t, &mut s, sel, v2(250.0, 50.0), v2(260.0, 20.0), false);
    let z = s.design().objects[0].contours[0].curve(0).expect("curve");
    assert_eq!(z.c2, v2(260.0, 20.0));
    assert_eq!(z.c1, v2(150.0, 50.0), "the other handle stays");
    undo(&mut s);
    assert_eq!(*s.design(), before);
    let sel = click_at(&mut t, &mut s, sel, v2(600.0, 600.0), 1);
    assert_eq!(sel, BTreeSet::from([other]));
    let sel = click_at(&mut t, &mut s, sel, v2(900.0, 900.0), 1);
    assert!(sel.is_empty());
}

#[test]
fn point_tool_double_click_inserts_and_delete_removes() {
    let mut s = session();
    let id = curved_rect(&mut s);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([id]);
    // Double-click the curved top edge at its apex (t = 0.5).
    let apex = segment_at(&s.design().objects[0].contours[0], 0, 0.5);
    let sel = click_at(&mut t, &mut s, sel, apex + v2(0.0, 1.0), 1);
    assert_eq!(
        s.design().objects[0].contours[0].points.len(),
        5,
        "one click inserts a point, as PE-DESIGN's Select Point does"
    );
    // The second click of a double-click lands on the new point and adds nothing.
    let sel = click_at(&mut t, &mut s, sel, apex + v2(0.0, 1.0), 2);
    let c = s.design().objects[0].contours[0].clone();
    assert_eq!(c.points.len(), 5);
    assert!(
        c.points[1].distance(apex) < 0.5,
        "inserted at the clicked parameter"
    );
    assert!(
        c.curve(0).is_some() && c.curve(1).is_some(),
        "curve split in two"
    );
    let design = s.design().clone();
    let cx = ctx(&design, &sel);
    assert_eq!(t.selected_point(&cx).map(|p| p.index), Some(1));
    let effects = t.delete_point(&cx).expect("a vertex is selected");
    apply(&mut s, effects, sel.clone());
    assert_eq!(s.design().objects[0].contours[0].points.len(), 4);
    let design = s.design().clone();
    assert!(
        t.delete_point(&ctx(&design, &sel)).is_none(),
        "nothing selected now"
    );
    undo(&mut s);
    undo(&mut s);
    assert_eq!(s.design().objects[0].contours[0].points.len(), 4);
    assert!(s.design().objects[0].contours[0].curve(0).is_some());
    let mut select = ToolController::default();
    assert!(
        select.delete_point(&ctx(&design, &sel)).is_none(),
        "only the Point tool"
    );
}

#[test]
fn segment_shortcuts_curve_and_straighten_under_pointer() {
    let mut s = session();
    let id = rect_design(&mut s);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([id]);
    send(
        &mut t,
        &mut s,
        sel.clone(),
        PointerEvent::new(Phase::Moved, v2(301.0, 150.0)),
    );
    let design = s.design().clone();
    let effects = t.set_segment_under_pointer(true, &ctx(&design, &sel));
    apply(&mut s, effects, sel.clone());
    let c = s.design().objects[0].contours[0].clone();
    let z = c.curve(1).expect("right edge curved");
    // Catmull-Rom: the handle at (300,100) leans along (300,200) − (100,100).
    assert!(z.c1.x > 300.0 && z.c2.x > 300.0, "bulges outwards: {z:?}");
    assert_eq!(c.curves.iter().flatten().count(), 1);
    let design = s.design().clone();
    assert!(
        t.set_segment_under_pointer(true, &ctx(&design, &sel))
            .is_empty(),
        "already curved"
    );
    let on_curve = segment_at(&design.objects[0].contours[0], 1, 0.5);
    send(
        &mut t,
        &mut s,
        sel.clone(),
        PointerEvent::new(Phase::Moved, on_curve),
    );
    let design = s.design().clone();
    let effects = t.set_segment_under_pointer(false, &ctx(&design, &sel));
    apply(&mut s, effects, sel.clone());
    assert!(!s.design().objects[0].contours[0].has_curves());
    // Far from any segment: nothing.
    send(
        &mut t,
        &mut s,
        sel.clone(),
        PointerEvent::new(Phase::Moved, v2(200.0, 150.0)),
    );
    let design = s.design().clone();
    assert!(
        t.set_segment_under_pointer(true, &ctx(&design, &sel))
            .is_empty()
    );
}

#[test]
fn smooth_curve_uses_neighbour_tangents_and_chord_at_open_ends() {
    let open = Contour::new(vec![v2(0.0, 0.0), v2(90.0, 0.0), v2(90.0, 90.0)], false);
    let z = smooth_curve(&open, 0);
    assert_eq!(z.c1, v2(30.0, 0.0), "open start: along the chord");
    assert_eq!(
        z.c2,
        v2(90.0 - 15.0, -15.0),
        "tangent (90,90)−(0,0) halved, a third"
    );
}

#[test]
fn pen_builds_corner_and_smooth_anchors_and_closes_on_first() {
    let mut s = session();
    let mut t = ToolController::default();
    t.set_tool(Tool::from_shortcut('b').expect("pen shortcut"));
    assert_eq!(t.tool(), Tool::Pen);
    let sel = click_at(&mut t, &mut s, BTreeSet::new(), v2(100.0, 100.0), 1);
    // Click-drag: a smooth anchor with symmetric handles.
    let sel = drag(
        &mut t,
        &mut s,
        sel,
        v2(300.0, 100.0),
        v2(350.0, 150.0),
        false,
    );
    assert_eq!(t.pen()[1].out, Some(v2(350.0, 150.0)));
    assert_eq!(t.pen()[1].incoming(), Some(v2(250.0, 50.0)));
    assert_eq!(t.pen_handles().len(), 2);
    let sel = click_at(&mut t, &mut s, sel, v2(200.0, 300.0), 1);
    send(
        &mut t,
        &mut s,
        sel.clone(),
        PointerEvent::new(Phase::Moved, v2(120.0, 200.0)),
    );
    match t.preview() {
        Some(ToolPreview::Outline(c)) => {
            assert_eq!(c.points.len(), 4, "rubber band to the pointer")
        }
        other => panic!("expected outline preview, got {other:?}"),
    }
    assert!(
        s.design().objects.is_empty(),
        "nothing committed while drawing"
    );
    let sel = click_at(&mut t, &mut s, sel, v2(102.0, 101.0), 1);
    assert!(!t.is_busy());
    assert_eq!(s.design().objects.len(), 1);
    let o = &s.design().objects[0];
    assert_eq!(sel, BTreeSet::from([o.id]), "new path selected");
    let c = &o.contours[0];
    assert!(c.closed);
    assert_eq!(
        c.points,
        vec![v2(100.0, 100.0), v2(300.0, 100.0), v2(200.0, 300.0)]
    );
    assert_eq!(
        c.curve(0),
        Some(Bezier {
            c1: v2(100.0, 100.0),
            c2: v2(250.0, 50.0)
        })
    );
    assert_eq!(
        c.curve(1),
        Some(Bezier {
            c1: v2(350.0, 150.0),
            c2: v2(200.0, 300.0)
        })
    );
    assert_eq!(c.curve(2), None, "corner to corner stays straight");
    undo(&mut s);
    assert!(s.design().objects.is_empty(), "the whole path is one Undo");
}

#[test]
fn pen_return_finishes_open_and_rejects_too_few_points() {
    let mut s = session();
    let mut t = ToolController::default();
    t.set_tool(Tool::Pen);
    let sel = click_at(&mut t, &mut s, BTreeSet::new(), v2(0.0, 0.0), 1);
    assert!(t.finish().is_empty(), "one anchor is not a path");
    assert!(!t.is_busy());
    let sel = click_at(&mut t, &mut s, sel, v2(0.0, 0.0), 1);
    let sel = click_at(&mut t, &mut s, sel, v2(1.0, 0.0), 1);
    assert_eq!(t.pen().len(), 1, "a click on the last anchor adds nothing");
    let sel = click_at(&mut t, &mut s, sel, v2(100.0, 0.0), 2);
    let effects = t.finish();
    let sel = apply(&mut s, effects, sel);
    let c = &s.design().objects[0].contours[0];
    assert!(!c.closed && c.points.len() == 2 && !c.has_curves());
    assert_eq!(sel.len(), 1);
    // Escape abandons.
    click_at(&mut t, &mut s, sel, v2(0.0, 50.0), 1);
    t.cancel();
    assert!(!t.is_busy() && t.preview().is_none());
}

#[test]
fn pen_request_omits_curves_when_all_straight() {
    let corner = [
        PenNode::corner(v2(0.0, 0.0)),
        PenNode::corner(v2(100.0, 0.0)),
        PenNode::corner(v2(0.0, 100.0)),
    ];
    let r = path_request(&corner, true);
    assert_eq!(r.command, "shape.path");
    assert!(r.params.get("curves").is_none());
    assert_eq!(
        r.params["points"],
        json!([[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]])
    );
    let smooth = [
        PenNode::corner(v2(0.0, 0.0)),
        PenNode {
            at: v2(100.0, 0.0),
            out: Some(v2(100.0, 50.0)),
        },
    ];
    assert!(pen_contour(&smooth, true).has_curves());
    let r = path_request(&smooth, true);
    assert_eq!(
        r.params["curves"],
        json!([[[0.0, 0.0], [10.0, -5.0]], [[10.0, 5.0], [0.0, 0.0]]])
    );
    let mut s = session();
    run(&mut s, &r);
    assert_eq!(s.design().objects[0].contours[0].curves.len(), 2);
}

#[test]
fn click_counter_detects_double_clicks() {
    let mut c = ClickCounter::default();
    assert_eq!(c.press(1.0, v2(10.0, 10.0)), 1);
    assert_eq!(c.press(1.2, v2(12.0, 11.0)), 2);
    assert_eq!(c.press(1.3, v2(12.0, 11.0)), 3);
    assert_eq!(c.press(2.0, v2(12.0, 11.0)), 1, "too slow");
    assert_eq!(c.press(2.1, v2(40.0, 11.0)), 1, "too far");
}

#[test]
fn hand_made_line_controls_reach_the_engine() {
    // #540, #542: the profile's pass counts and the hand-stitch look's settings and variant
    // appear once they are on, and the engine takes each of them.
    use super::inspector::{Control, line_controls};
    use openstitch_design::line_looks::{HandStitch, RunProfile};
    use openstitch_design::{LineSewing, LineType};
    let mut s = Session::new(std::env::temp_dir());
    s.execute(
        "shape.path",
        Some(&json!({"points": [[10, 10], [60, 10]], "closed": false})),
    )
    .expect("line");
    for (setup, line) in [
        (
            json!({"id": 1, "type": "running", "profile": "arc"}),
            LineSewing {
                profile: Some(RunProfile::default()),
                ..LineSewing::new(LineType::Running, Thread::rgb(0))
            },
        ),
        (
            json!({"id": 1, "type": "running", "hand": true}),
            LineSewing {
                hand: Some(HandStitch::default()),
                ..LineSewing::new(LineType::Running, Thread::rgb(0))
            },
        ),
    ] {
        s.execute("object.setLine", Some(&setup)).expect("setup");
        let controls = line_controls(&line);
        let params: Vec<&str> = controls.iter().map(|c| c.param()).collect();
        for want in if line.profile.is_some() {
            vec!["profile", "minPasses", "maxPasses", "passSpacing"]
        } else {
            vec![
                "hand",
                "handLength",
                "handAngle",
                "handExtra",
                "handPasses",
                "seed",
            ]
        } {
            assert!(params.contains(&want), "{want} in {params:?}");
        }
        for c in &controls {
            let v = match c {
                Control::Number { value, .. } => json!(value),
                Control::Choice { value, .. } => json!(value),
                Control::Toggle { value, .. } => json!(value),
            };
            let mut p = c.params(v);
            p["id"] = json!(1);
            s.execute("object.setLine", Some(&p))
                .unwrap_or_else(|e| panic!("{p}: {e}"));
        }
    }
}

#[test]
fn every_line_type_has_inspector_controls_the_engine_accepts() {
    use super::inspector::{Control, line_controls, line_types};
    use openstitch_design::{LineSewing, LineType};
    let mut s = Session::new(std::env::temp_dir());
    s.execute(
        "shape.ellipse",
        Some(&json!({"cx": 50, "cy": 50, "rx": 20, "ry": 20, "outline": "Black"})),
    )
    .expect("ellipse");
    assert_eq!(line_types().len(), LineType::ALL.len() + 1);
    for kind in LineType::ALL {
        let controls = line_controls(&LineSewing::new(kind, Thread::rgb(0)));
        assert!(!controls.is_empty(), "{kind:?}");
        s.execute("object.setLine", Some(&json!({"id": 1, "type": kind.id()})))
            .expect("type");
        for c in &controls {
            let v = match c {
                Control::Number { value, .. } => json!(value),
                Control::Choice { options, .. } => json!(options.last().expect("options").0),
                Control::Toggle { value, .. } => json!(!value),
            };
            let mut params = c.params(v);
            params["id"] = json!(1);
            s.execute("object.setLine", Some(&params))
                .unwrap_or_else(|e| panic!("{kind:?} {params}: {e}"));
        }
    }
    let times = &line_controls(&LineSewing::new(LineType::Chain, Thread::rgb(0)))[2];
    assert_eq!(times.params(json!(2.6)), json!({"runTimes": 3}));
}

#[test]
fn auto_satin_offers_no_travel() {
    use super::inspector::region_controls;
    use openstitch_design::RegionSewing;
    let travel = |r: &RegionSewing| {
        region_controls(r)
            .iter()
            .any(|c| matches!(c.param(), "runningStitchPath" | "travel"))
    };
    let mut r = RegionSewing::new(RegionType::Satin, Thread::rgb(0));
    assert!(travel(&r));
    r.auto_direction = true;
    assert!(!travel(&r), "no Running Stitch Path with Auto satin");
    // The guide makes the attribute unavailable with Auto direction for fill too.
    r.kind = RegionType::Fill;
    assert!(!travel(&r));
    r.auto_direction = false;
    assert!(travel(&r));
}

#[test]
fn half_stitch_is_offered_for_auto_satin_only() {
    use super::inspector::region_controls;
    use openstitch_design::RegionSewing;
    let half = |r: &RegionSewing| region_controls(r).iter().any(|c| c.param() == "halfStitch");
    let mut r = RegionSewing::new(RegionType::Satin, Thread::rgb(0));
    assert!(
        !half(&r),
        "PE-DESIGN greys Half Stitch without Auto direction"
    );
    r.auto_direction = true;
    assert!(half(&r));
    r.kind = RegionType::Fill;
    assert!(!half(&r));
}

#[test]
fn region_option_controls_are_accepted_by_the_engine() {
    use super::inspector::{Control, region_controls};
    use openstitch_design::RegionSewing;
    let mut s = Session::new(std::env::temp_dir());
    s.execute(
        "shape.ellipse",
        Some(&json!({"cx": 50, "cy": 50, "rx": 20, "ry": 10, "fill": "Red"})),
    )
    .expect("ellipse");
    for kind in RegionType::ALL {
        s.execute(
            "object.setRegion",
            Some(&json!({"id": 1, "type": kind.id()})),
        )
        .expect("type");
        let mut r = RegionSewing::new(kind, Thread::rgb(0));
        r.underlay = true;
        r.underlay_style = openstitch_design::UnderlayStyle::Zigzag;
        r.feather = Some(openstitch_design::Feather {
            side: openstitch_design::FeatherSide::Start,
            width: 1.0,
            ..Default::default()
        });
        for c in region_controls(&r) {
            let v = match &c {
                Control::Number { value, range, .. } => {
                    json!(value.max(range.0 + 2.5).min(range.1))
                }
                Control::Choice { options, .. } => json!(options.last().expect("options").0),
                Control::Toggle { value, .. } => json!(!value),
            };
            let mut params = c.params(v);
            params["id"] = json!(1);
            // Each control on its own: from the type's defaults with the feathered edge of `r`
            // (some options exclude others, as in the guide).
            s.execute("object.setRegion", Some(&json!({"id": 1, "type": "none"})))
                .expect("clear");
            s.execute(
                "object.setRegion",
                Some(&json!({"id": 1, "type": kind.id()})),
            )
            .expect("type");
            if r.feather_unavailable().is_none() {
                s.execute(
                    "object.setRegion",
                    Some(&json!({"id": 1, "featherWidth": 1.0})),
                )
                .expect("feather");
            }
            s.execute("object.setRegion", Some(&params))
                .unwrap_or_else(|e| panic!("{kind:?} {params}: {e}"));
        }
    }
}

#[test]
fn fill_offers_stitch_type_with_the_region_s_value() {
    use super::inspector::{Control, region_controls};
    use openstitch_design::{FillStitchType, RegionSewing};
    let value = |r: &RegionSewing| {
        region_controls(r).into_iter().find_map(|c| match c {
            Control::Choice {
                param: "stitchType",
                value,
                options,
                ..
            } => Some((value, options.len())),
            _ => None,
        })
    };
    let mut r = RegionSewing::new(RegionType::Fill, Thread::rgb(0));
    assert_eq!(value(&r), Some(("1".to_string(), 2)));
    r.stitch_type = FillStitchType::Two;
    assert_eq!(value(&r), Some(("2".to_string(), 2)));
    // Only the Fill sew type has PE-DESIGN's Stitch Type buttons.
    let satin = RegionSewing::new(RegionType::Satin, Thread::rgb(0));
    assert_eq!(value(&satin), None);
}

// MARK: Sewing Order list (#502)

#[test]
fn locked_objects_stay_put_on_the_canvas() {
    let mut s = session();
    let a = rect_design(&mut s);
    run(&mut s, &objects::set_locked(&BTreeSet::from([a]), true));
    let before = s.design().objects[0].bounds().expect("bounds");
    let mut t = ToolController::default();
    // A click on it does not pick it, and a drag from it is a marquee that leaves it alone.
    let sel = drag(
        &mut t,
        &mut s,
        BTreeSet::new(),
        v2(150.0, 150.0),
        v2(170.0, 140.0),
        false,
    );
    assert!(sel.is_empty());
    assert!(objects_in(s.design(), &rect_from(v2(0.0, 0.0), v2(900.0, 900.0))).is_empty());
    // Selected from the list: no handles, no point editing, no moving.
    let sel = BTreeSet::from([a]);
    let design = s.design().clone();
    let c = ctx(&design, &sel);
    assert!(c.locked() && c.handles().is_none() && c.point_target().is_none());
    drag(
        &mut t,
        &mut s,
        sel.clone(),
        v2(150.0, 150.0),
        v2(250.0, 250.0),
        false,
    );
    assert_eq!(s.design().objects[0].bounds(), Some(before));
    assert!(objects::any_locked(s.design(), &sel));
    // Unlocked, it moves again.
    run(&mut s, &objects::set_locked(&sel, false));
    let sel = drag(
        &mut t,
        &mut s,
        BTreeSet::new(),
        v2(150.0, 150.0),
        v2(170.0, 140.0),
        false,
    );
    assert_eq!(sel, BTreeSet::from([a]));
    assert_ne!(s.design().objects[0].bounds(), Some(before));
}

#[test]
fn click_order_follows_the_clicks() {
    let mut order = Vec::new();
    objects::track_click_order(&mut order, &BTreeSet::from([5]));
    objects::track_click_order(&mut order, &BTreeSet::from([2, 5]));
    objects::track_click_order(&mut order, &BTreeSet::from([2, 4, 5]));
    assert_eq!(order, [5, 2, 4]);
    objects::track_click_order(&mut order, &BTreeSet::from([4, 5]));
    assert_eq!(order, [5, 4]);
    assert!(objects::click_order(&order[..1]).is_none());
    let mut s = session();
    for x in [0, 15, 30, 45, 60] {
        run(
            &mut s,
            &CommandRequest::new(
                "shape.rectangle",
                json!({ "x": x, "y": 0, "width": 10, "height": 10 }),
            ),
        );
    }
    run(&mut s, &objects::click_order(&[5, 2, 4]).expect("request"));
    let ids: Vec<u64> = s.design().objects.iter().map(|o| o.id).collect();
    assert_eq!(ids, [1, 5, 2, 4, 3]);
    // Rename, then back to the label.
    run(&mut s, &objects::set_name(1, "  Leaf "));
    let rows = objects::rows(s.design(), &BTreeSet::new());
    assert_eq!(
        (rows[0].label.as_str(), rows[0].name.as_deref()),
        ("Leaf", Some("Leaf"))
    );
    run(&mut s, &objects::set_name(1, ""));
    assert_eq!(
        objects::rows(s.design(), &BTreeSet::new())[0].label,
        "Shape 1"
    );
}

/// A 20 mm square of level fill rows `spacing` (0.1 mm) apart, stored on the machine's 0.1 mm
/// grid as a .pes stores them, sewn back and forth in 4 mm stitches.
fn level_rows(spacing: f64) -> StitchPattern {
    let mut p = StitchPattern::new();
    p.threads = vec![Thread::rgb(0x1a4f9c)];
    p.stitches.push(Stitch::new(0.0, 0.0, Command::Jump));
    let rows = (200.0 / spacing) as usize;
    for k in 0..=rows {
        let y = (k as f64 * spacing).round();
        let xs: Vec<f64> = (0..=5).map(|i| i as f64 * 40.0).collect();
        let xs: Vec<f64> = if k % 2 == 0 {
            xs
        } else {
            xs.into_iter().rev().collect()
        };
        for x in xs {
            p.stitches.push(Stitch::new(x, y, Command::Stitch));
        }
    }
    p.stitches.push(Stitch::new(0.0, 0.0, Command::End));
    p
}

/// Across the rows of `level_rows` at x = 10 mm, inside the square (3 mm from its edges): the
/// widest strip (mm) of bare fabric the Realistic view leaves, and the share it covers.
fn bare_fabric(m: &RenderModel) -> (f64, f64) {
    use openstitch_design::calibration::THREAD_MM;
    let l = &m.layers[0];
    let mut spans: Vec<(f64, f64)> = Vec::new();
    for (n, s) in l.strokes.iter().enumerate() {
        for (k, w) in s.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            if (a.y - b.y).abs() > 1e-9 || a.x.min(b.x) > 100.0 || a.x.max(b.x) < 100.0 {
                continue;
            }
            let len = a.distance(b);
            let normal_y = (b.x - a.x) / len;
            let y = a.y + f64::from(l.shift[n][k]) * normal_y;
            let half = f64::from(l.cover[n][k]) * THREAD_MM * 10.0 / 2.0;
            spans.push((y - half, y + half));
        }
    }
    spans.sort_by(|p, q| p.0.total_cmp(&q.0));
    let (mut reach, mut widest, mut bare) = (30.0f64, 0.0f64, 0.0);
    for (lo, hi) in spans {
        if hi < 30.0 || lo > 170.0 {
            continue;
        }
        let gap = (lo.min(170.0) - reach).max(0.0);
        widest = widest.max(gap);
        bare += gap;
        reach = reach.max(hi);
    }
    bare += (170.0 - reach).max(0.0);
    (widest / 10.0, 1.0 - bare / 140.0)
}

#[test]
fn realistic_view_shows_no_bare_stripes_in_rows_on_the_machine_grid() {
    // #893: 4.5 rows/mm (0.222 mm) stored 0.2, 0.2, 0.2 and 0.3 mm apart left a bare stripe in
    // every 0.3 mm gap (0.09 mm wide, about every 0.9 mm). Now no strip wider than 0.02 mm (a
    // fifth of a pixel at 10 px/mm) shows anywhere inside the fill.
    let (dense, _) = bare_fabric(&RenderModel::build(&level_rows(10.0 / 4.5), usize::MAX));
    assert!(dense < 0.02, "bare strip {dense} mm at 4.5 rows/mm");
    // Sparse rows still show their fabric: 1 row/mm leaves most of each 1 mm gap bare.
    let (sparse, _) = bare_fabric(&RenderModel::build(&level_rows(10.0), usize::MAX));
    assert!(sparse > 0.5, "bare strip {sparse} mm at 1 row/mm");
}

#[test]
fn realistic_view_covers_the_fabric_as_the_sewn_t1_ramp() {
    // The Black coverage ramp of both T1 sew-outs (pink cotton 2026-11-03, white cotton
    // 2026-11-04; examples/sew/calibration/results), rows on the 0.1 mm grid as sewn: the share
    // of fabric the drawn rows hide is within 4 % of both cards at every density (#291, #893).
    for (rows, pink, white) in [
        (0.5, 0.20, 0.16),
        (1.0, 0.40, 0.35),
        (1.5, 0.55, 0.53),
        (2.0, 0.68, 0.66),
        (3.0, 0.87, 0.84),
        (4.5, 0.96, 0.96),
    ] {
        let (_, covered) = bare_fabric(&RenderModel::build(&level_rows(10.0 / rows), usize::MAX));
        assert!(
            (covered - pink).abs() < 0.04 && (covered - white).abs() < 0.04,
            "{rows} rows/mm: the view covers {covered:.3}, the cards {pink} and {white}"
        );
    }
}

/// The share of the window `(x0, y0, x1, y1)` (design units) the Realistic view's threads hide,
/// sampled every 0.02 mm: each stitch drawn as wide as its cover, moved by its shift.
fn covered_share(m: &RenderModel, (x0, y0, x1, y1): (f64, f64, f64, f64)) -> f64 {
    use openstitch_design::calibration::THREAD_MM;
    let mut segs = Vec::new();
    for l in &m.layers {
        for (n, s) in l.strokes.iter().enumerate() {
            for (k, w) in s.windows(2).enumerate() {
                let (a, b) = (w[0], w[1]);
                let len = a.distance(b);
                let half = f64::from(l.cover[n][k]) * THREAD_MM * 10.0 / 2.0;
                if len < 1e-9
                    || a.x.min(b.x) - half > x1
                    || a.x.max(b.x) + half < x0
                    || a.y.min(b.y) - half > y1
                    || a.y.max(b.y) + half < y0
                {
                    continue;
                }
                let o = v2(-(b.y - a.y), b.x - a.x) * (f64::from(l.shift[n][k]) / len);
                segs.push((a + o, b - a, half));
            }
        }
    }
    let (mut hidden, mut all) = (0usize, 0usize);
    let mut y = y0;
    while y < y1 {
        let mut x = x0;
        while x < x1 {
            all += 1;
            let p = v2(x, y);
            if segs.iter().any(|&(a, d, half)| {
                let q = p - a;
                let t = ((q.x * d.x + q.y * d.y) / (d.x * d.x + d.y * d.y)).clamp(0.0, 1.0);
                (a + d * t).distance(p) <= half
            }) {
                hidden += 1;
            }
            x += 0.2;
        }
        y += 0.2;
    }
    hidden as f64 / all as f64
}

#[test]
fn crossing_rows_never_draw_lighter_than_the_rows_alone() {
    // #921: thread crossing a fill (a 45° underlay under 4.5 rows/mm, rows converging round a
    // bend) raised the row density the view reads, which narrowed every thread as if the rows
    // were that dense and parallel: the densest part of angle-lines drew lightest. More thread
    // never hides less fabric.
    let window = (50.0, 50.0, 150.0, 150.0);
    let rows = level_rows(10.0 / 4.5);
    let alone = covered_share(&RenderModel::build(&rows, usize::MAX), window);
    let mut crossed = rows.clone();
    let end = crossed.stitches.pop().expect("end");
    for k in 0..=40 {
        // 45° stitches about 0.7 mm apart across the square, sewn after the rows.
        let c = f64::from(k) * 10.0 - 200.0 + 100.0;
        let (a, b) = (
            v2(c.max(0.0), (-c).max(0.0)),
            v2((c + 200.0).min(200.0), (200.0 - c).min(200.0)),
        );
        let (a, b) = if k % 2 == 0 { (a, b) } else { (b, a) };
        crossed
            .stitches
            .push(Stitch::new(a.x, a.y, Command::Stitch));
        crossed
            .stitches
            .push(Stitch::new(b.x, b.y, Command::Stitch));
    }
    crossed.stitches.push(end);
    let both = covered_share(&RenderModel::build(&crossed, usize::MAX), window);
    assert!(alone > 0.9, "the rows alone hide {alone:.3}");
    assert!(
        both >= alone - 0.005,
        "with an underlay crossing them {both:.3}, the rows alone {alone:.3}"
    );
}

/// #1046: the thickness profile control's hover hint says that linear, smoothstep and stairs
/// need more than 3 passes, the same words as the command's description.
#[test]
fn the_thickness_profile_control_has_its_hint() {
    use super::inspector::{Control, hint, line_controls};
    use openstitch_design::{LineSewing, LineType};
    let mut line = LineSewing::new(LineType::Running, Thread::rgb(0));
    line.profile = Some(Default::default());
    let profile = line_controls(&line)
        .into_iter()
        .find(|c| {
            matches!(
                c,
                Control::Choice {
                    param: "profile",
                    ..
                }
            )
        })
        .expect("the profile control");
    assert_eq!(
        hint(profile.param()),
        Some(openstitch_design::line_looks::PROFILE_PASSES_NOTE)
    );
}

/// #1124: every inspector number field holds the values its setting can hold. For each region and
/// line type, from the object the command makes:
/// - the type's default is inside the field's range;
/// - both ends of the field are accepted by the command;
/// - a value just outside either end is refused by the command, or stored inside the range, so the
///   command cannot store what the field cannot show.
///
/// Values go to the command as the field sends them, and are read back from the field.
///
/// Fields narrower than the command on purpose are named in `NARROWER`, with why. Two fields offer
/// values inside their range that the command refuses, which a range cannot express: a satin's
/// Split stitches over (0, or 2-12) and the odd-only pass counts of a line's look (1, 3, ... 9).
#[test]
fn every_inspector_number_field_holds_its_settings_values() {
    use super::inspector::{Control, line_controls, region_controls};
    /// (type, param): narrower than the command, and why.
    const NARROWER: &[(&str, &str, &str)] = &[
        (
            "zigzagNetFill",
            "patternWidth",
            "PE-DESIGN's 1.0-10 (round 15); the command keeps 0.5-50 (#1109)",
        ),
        (
            "zigzagNetFill",
            "spacing",
            "PE-DESIGN's 2.0-10 (round 15); the command keeps 0.5-20 (#1109)",
        ),
        (
            "candlewick",
            "width",
            "PE-DESIGN's Size 3.0-10 (round 15); the command keeps 0.5-10 (#1124)",
        ),
        (
            "fill",
            "waveHeight",
            "capped by the wave length in the inspector, 0-20 by command (#1050)",
        ),
    ];
    let narrower = |kind: &str, param: &str| NARROWER.iter().any(|n| n.0 == kind && n.1 == param);
    let mut problems = Vec::new();
    let mut checked = 0;
    for (setter, kinds) in [
        (
            "object.setRegion",
            RegionType::ALL.iter().map(|k| k.id()).collect::<Vec<_>>(),
        ),
        (
            "object.setLine",
            openstitch_design::LineType::ALL
                .iter()
                .map(|k| k.id())
                .collect(),
        ),
    ] {
        let region = setter == "object.setRegion";
        for kind in kinds {
            // A fresh object of this type, with under sewing and a feathered edge where offered,
            // so their fields are shown too.
            let fresh = || {
                let mut s = Session::new(std::env::temp_dir());
                s.execute(
                    "shape.ellipse",
                    Some(&json!({"cx": 50, "cy": 50, "rx": 20, "ry": 10, "fill": "Red", "outline": "Black"})),
                )
                .expect("ellipse");
                s.execute(setter, Some(&json!({"id": 1, "type": kind})))
                    .expect("type");
                if region {
                    let _ = s.execute(
                        setter,
                        Some(&json!({"id": 1, "underlay": true, "underlayStyle": "zigzag"})),
                    );
                    let _ = s.execute(setter, Some(&json!({"id": 1, "featherWidth": 1.0})));
                }
                s
            };
            let controls = |s: &Session| {
                let o = &s.design().objects[0];
                if region {
                    region_controls(o.region.as_ref().expect("region"))
                } else {
                    line_controls(o.line.as_ref().expect("line"))
                }
            };
            let s = fresh();
            for c in controls(&s) {
                let Control::Number {
                    key,
                    param,
                    value,
                    range: (lo, hi),
                    integer,
                    ..
                } = c
                else {
                    continue;
                };
                checked += 1;
                if value < lo - 1e-9 || value > hi + 1e-9 {
                    problems.push(format!("{kind} {param}: default {value} outside {lo}-{hi}"));
                }
                // Sent as the field sends it (`Control::params`): a zigzag net's Density field
                // is in PE-DESIGN's lines per mm and sends the stored rungs per mm (#1163).
                let send = |v: f64| {
                    let mut s = fresh();
                    let mut p = c.params(json!(v));
                    p["id"] = json!(1);
                    s.execute(setter, Some(&p)).map(|_| {
                        controls(&s).into_iter().find_map(|c| match c {
                            Control::Number { key: k, value, .. } if k == key => Some(value),
                            _ => None,
                        })
                    })
                };
                for end in [lo, hi] {
                    if let Err(e) = send(end) {
                        problems.push(format!(
                            "{kind} {param}: the field's end {end} is refused: {e}"
                        ));
                    }
                }
                if narrower(kind, param) {
                    continue;
                }
                let step = if integer { 1.0 } else { 0.01 };
                for out in [lo - step, hi + step] {
                    if let Ok(Some(stored)) = send(out)
                        && (stored < lo - 1e-9 || stored > hi + 1e-9)
                    {
                        problems.push(format!(
                            "{kind} {param}: the command stores {stored} for {out}, outside the field's {lo}-{hi}"
                        ));
                    }
                }
            }
        }
    }
    assert!(checked > 100, "{checked} fields checked");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// #1239: the inspector's switch to Zigzag Net Fill gives a net with under sewing on, PE-DESIGN
/// 11.50's default for the type, also from a fill whose under sewing was off; its Under sewing
/// toggle shows on and turns it off.
#[test]
fn the_inspectors_switch_to_zigzag_net_has_under_sewing_on() {
    use super::inspector::{Control, region_controls};
    let mut s = Session::new(std::env::temp_dir());
    s.execute(
        "shape.rectangle",
        Some(&json!({"x": 10, "y": 10, "width": 30, "height": 20, "fill": "Red"})),
    )
    .expect("rectangle");
    let sel = BTreeSet::from([1]);
    let toggle = |s: &Session| {
        region_controls(s.design().objects[0].region.as_ref().expect("region"))
            .into_iter()
            .find(|c| c.param() == "underlay")
            .expect("underlay")
    };
    let m = InspectorModel::new(s.design(), &sel);
    run(&mut s, &m.set_region(json!({ "underlay": false })));
    let m = InspectorModel::new(s.design(), &sel);
    run(&mut s, &m.set_region_type(Some(RegionType::ZigzagNetFill)));
    let c = toggle(&s);
    assert!(matches!(c, Control::Toggle { value: true, .. }), "{c:?}");
    let m = InspectorModel::new(s.design(), &sel);
    run(&mut s, &m.set_region(c.params(json!(false))));
    assert!(matches!(toggle(&s), Control::Toggle { value: false, .. }));
    assert!(
        !s.design().objects[0]
            .region
            .as_ref()
            .expect("region")
            .underlay
    );
}

/// #1163: a zigzag net's Density field shows PE-DESIGN's lines per mm, 4.5 at the default, where
/// the net stores 2.25 rungs per mm; a value typed in the field is stored as half of it, and the
/// field of another type still shows the stored rows per mm.
#[test]
fn zigzag_net_density_field_shows_pe_designs_lines_per_mm() {
    use super::inspector::{Control, region_controls};
    let stored = |s: &Session| {
        s.design().objects[0]
            .region
            .as_ref()
            .expect("region")
            .density
    };
    let field = |s: &Session| {
        let c = region_controls(s.design().objects[0].region.as_ref().expect("region"))
            .into_iter()
            .find(|c| c.param() == "density")
            .expect("density");
        let Control::Number {
            value,
            range,
            suffix,
            ..
        } = c
        else {
            panic!("a number field");
        };
        (c, value, range, suffix)
    };
    let mut s = Session::new(std::env::temp_dir());
    s.execute(
        "shape.rectangle",
        Some(&json!({"x": 10, "y": 10, "width": 30, "height": 20, "fill": "Red"})),
    )
    .expect("rectangle");
    let (_, value, _, suffix) = field(&s);
    assert_eq!((value, suffix), (stored(&s), " rows/mm"), "a fill's rows");
    s.execute(
        "object.setRegion",
        Some(&json!({"id": 1, "type": "zigzagNetFill"})),
    )
    .expect("zigzag net");
    let (c, value, range, suffix) = field(&s);
    assert_eq!(stored(&s), 2.25);
    assert_eq!((value, suffix), (4.5, " lines/mm"));
    assert_eq!(range, (1.0, 20.0));
    // PE-DESIGN's 4.7 (zigzag-net-circle40-w3p0-density-4p7) is stored as 2.35 rungs per mm.
    let mut p = c.params(json!(4.7));
    p["id"] = json!(1);
    s.execute("object.setRegion", Some(&p)).expect("4.7");
    assert!((stored(&s) - 2.35).abs() < 1e-12, "{}", stored(&s));
    assert!((field(&s).1 - 4.7).abs() < 1e-12);
}

/// #1163: a Density typed in a zigzag net's field comes back the same from a PES version 11 file
/// and from a project. The PES holds the typed number itself at body +368, as PE-DESIGN's
/// lines per mm; opened again, the field shows it (to the file's 32-bit precision) and writes
/// the same number back. A project keeps the stored value exactly.
#[test]
fn zigzag_net_density_typed_in_the_field_round_trips_through_pes_11_and_osd() {
    use super::inspector::{Control, region_controls};
    let dir = openstitch_testutil::TempDir::new("app-net-density");
    let stored = |s: &Session| {
        s.design().objects[0]
            .region
            .as_ref()
            .expect("region")
            .density
    };
    let field = |s: &Session| {
        let c = region_controls(s.design().objects[0].region.as_ref().expect("region"))
            .into_iter()
            .find(|c| c.param() == "density")
            .expect("density");
        let Control::Number { value, .. } = c else {
            panic!("a number field");
        };
        (c, value)
    };
    let at_368 = |name: &str| {
        let bytes = std::fs::read(dir.join(name)).expect("read");
        let section = openstitch_formats::pes::read_objects(&bytes).expect("objects");
        section.shapes[0].net_density
    };
    let run = |s: &mut Session, id: &str, p: serde_json::Value| {
        s.execute(id, Some(&p))
            .unwrap_or_else(|e| panic!("{id}: {e}"))
    };
    for typed in [1.0, 4.5, 4.7, 6.3, 20.0] {
        let mut s = Session::new(&dir);
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": 10, "y": 10, "width": 30, "height": 20, "fill": "Red"}),
        );
        run(
            &mut s,
            "object.setRegion",
            json!({"id": 1, "type": "zigzagNetFill"}),
        );
        let mut p = field(&s).0.params(json!(typed));
        p["id"] = json!(1);
        run(&mut s, "object.setRegion", p);
        assert_eq!(field(&s).1, typed, "the field shows what was typed");
        run(
            &mut s,
            "design.export",
            json!({"path": "net.pes", "pesVersion": "11"}),
        );
        assert_eq!(at_368("net.pes"), typed as f32, "{typed}: +368");
        run(&mut s, "project.save", json!({"path": "net.osd"}));

        let mut pes = Session::new(&dir);
        run(&mut pes, "import.pes", json!({"path": "net.pes"}));
        assert!(
            (field(&pes).1 - typed).abs() < 1e-6,
            "{typed}: from the PES the field shows {}",
            field(&pes).1
        );
        run(
            &mut pes,
            "design.export",
            json!({"path": "again.pes", "pesVersion": "11"}),
        );
        assert_eq!(at_368("again.pes"), typed as f32, "{typed}: written back");

        let mut osd = Session::new(&dir);
        run(&mut osd, "project.open", json!({"path": "net.osd"}));
        assert_eq!(stored(&osd), stored(&s), "{typed}: the project's value");
        assert_eq!(field(&osd).1, typed, "{typed}: the project's field");
    }
}

/// #1124: a candlewick's Density is PE-DESIGN 11.50's drop-down (round 15): Dense, Medium and
/// Light, knots of 32, 24 and 20 points, stored as densities 16, 12 and 10. The default shows
/// Medium; each choice is accepted by the command and stored as its density; a density set by
/// command to another value is offered as it is.
#[test]
fn candlewick_density_offers_pe_designs_three_knots() {
    use super::inspector::{CANDLEWICK_DENSITIES, Control, line_controls};
    let density = |s: &Session| {
        line_controls(s.design().objects[0].line.as_ref().expect("line"))
            .into_iter()
            .find(|c| c.param() == "density")
            .expect("density")
    };
    let mut s = Session::new(std::env::temp_dir());
    s.execute(
        "shape.ellipse",
        Some(&json!({"cx": 50, "cy": 50, "rx": 20, "ry": 20, "outline": "Black"})),
    )
    .expect("ellipse");
    s.execute(
        "object.setLine",
        Some(&json!({"id": 1, "type": "candlewick"})),
    )
    .expect("candlewick");
    let Control::Choice { value, options, .. } = density(&s) else {
        panic!("a drop-down");
    };
    assert_eq!(value, "12");
    let names: Vec<(&str, &str)> = options
        .iter()
        .map(|(v, l)| (v.as_str(), l.as_str()))
        .collect();
    assert_eq!(names, [("16", "Dense"), ("12", "Medium"), ("10", "Light")]);
    for (d, name) in CANDLEWICK_DENSITIES {
        let mut p = density(&s).params(json!(format!("{d}")));
        p["id"] = json!(1);
        s.execute("object.setLine", Some(&p))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let l = s.design().objects[0].line.as_ref().expect("line");
        assert_eq!(l.density, d, "{name}");
    }
    s.execute("object.setLine", Some(&json!({"id": 1, "density": 11.0})))
        .expect("by command");
    let Control::Choice { value, options, .. } = density(&s) else {
        panic!("a drop-down");
    };
    assert_eq!(value, "11");
    assert_eq!(options[0], ("11".into(), "11 (22 points)".into()));
    assert_eq!(options.len(), 4);
}

/// #1124: a candlewick's Size field is PE-DESIGN's 3.0-10.0 mm, but the command keeps alpha 14's
/// 0.5-10, so a script's 2 mm knot still works. A `setLine` call that leaves `width` out, here one
/// that only changes the run pitch, keeps the stored 2 mm instead of refusing it.
#[test]
fn a_two_mm_candlewick_is_kept_by_a_set_line_without_width() {
    use super::inspector::{Control, line_controls};
    let mut s = Session::new(std::env::temp_dir());
    s.execute(
        "shape.ellipse",
        Some(&json!({"cx": 50, "cy": 50, "rx": 20, "ry": 20, "outline": "Black"})),
    )
    .expect("ellipse");
    s.execute(
        "object.setLine",
        Some(&json!({"id": 1, "type": "candlewick", "width": 2.0})),
    )
    .expect("a 2 mm candlewick, as alpha 14 took it");
    s.execute("object.setLine", Some(&json!({"id": 1, "pitch": 3.0})))
        .expect("pitch alone");
    let l = s.design().objects[0].line.as_ref().expect("line");
    assert_eq!((l.width, l.pitch), (2.0, 3.0));
    let size = line_controls(l)
        .into_iter()
        .find(|c| c.param() == "width")
        .expect("Size");
    let Control::Number { value, range, .. } = size else {
        panic!("a number");
    };
    assert_eq!((value, range), (2.0, (3.0, 10.0)), "shown as stored");
    assert!(
        s.execute("object.setLine", Some(&json!({"id": 1, "width": 0.4})))
            .is_err(),
        "below 0.5 is still refused"
    );
}

// MARK: the point menu (#1258)

/// The rectangle of [`rect_design`] with line sewing as well as its fill, as PE-DESIGN's
/// capture shapes had: a shape sewn as a region only greys Split at Point and Open.
fn outlined_rect(s: &mut Session) -> u64 {
    let r = run(
        s,
        &CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 10, "y": 10, "width": 20, "height": 10, "fill": "#2e8b57", "outline": "#203040" }),
        ),
    );
    r["id"].as_u64().expect("id")
}

/// The menu's entries for the selected points, as `(key, enabled)` in menu order.
fn menu(t: &ToolController, s: &Session, sel: &BTreeSet<u64>) -> Vec<(&'static str, bool)> {
    use super::points::PointAction;
    let design = s.design().clone();
    let enabled = t
        .point_actions(&ctx(&design, sel))
        .expect("a point is selected");
    PointAction::MENU
        .iter()
        .zip(enabled)
        .map(|(a, e)| (a.key(), e))
        .collect()
}

fn choose(
    t: &mut ToolController,
    s: &mut Session,
    sel: &BTreeSet<u64>,
    a: super::points::PointAction,
) -> usize {
    let design = s.design().clone();
    let effects = t.point_action(a, &ctx(&design, sel));
    let n = effects.len();
    apply(s, effects, sel.clone());
    n
}

#[test]
fn the_point_menu_has_pe_designs_entries_in_order_with_its_separators() {
    use super::points::PointAction;
    let keys: Vec<_> = PointAction::MENU.iter().map(|a| a.key()).collect();
    assert_eq!(
        keys,
        [
            "toStraight",
            "toCurve",
            "toSmooth",
            "toSymmetry",
            "toCusp",
            "splitAtPoint",
            "deletePoint"
        ]
    );
    let separators: Vec<_> = PointAction::MENU
        .iter()
        .filter(|a| a.separator_before())
        .map(|a| a.key())
        .collect();
    assert_eq!(separators, ["toSmooth", "splitAtPoint", "deletePoint"]);
}

/// The owner's screen (2026-11-18): on a corner between two straight sides To Straight,
/// To Smooth, To Symmetry and To Cusp are greyed out; To Curve, Split at Point and Delete are
/// available.
#[test]
fn a_right_click_on_a_straight_corner_offers_what_pe_design_offers() {
    use super::points::PointAction as P;
    let mut s = session();
    let id = outlined_rect(&mut s);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([id]);
    let design = s.design().clone();
    assert!(
        t.point_actions(&ctx(&design, &sel)).is_none(),
        "no point yet"
    );
    assert!(
        !t.select_point_at(v2(200.0, 150.0), &ctx(&design, &sel)),
        "not on a point"
    );
    assert!(t.select_point_at(v2(101.0, 99.0), &ctx(&design, &sel)));
    assert_eq!(
        t.selected_point(&ctx(&design, &sel)).map(|p| p.index),
        Some(0)
    );
    assert_eq!(
        menu(&t, &s, &sel),
        [
            ("toStraight", false),
            ("toCurve", true),
            ("toSmooth", false),
            ("toSymmetry", false),
            ("toCusp", false),
            ("splitAtPoint", true),
            ("deletePoint", true)
        ]
    );
    // A greyed-out entry does nothing.
    assert_eq!(choose(&mut t, &mut s, &sel, P::ToSmooth), 0);
    // To Curve: one command, one Undo. The point is then smooth, as PE-DESIGN's is: To Curve
    // and To Smooth are greyed out, the rest available.
    assert_eq!(choose(&mut t, &mut s, &sel, P::ToCurve), 1);
    let c = s.design().objects[0].contours[0].clone();
    assert!(c.curve(0).is_some() && c.curve(3).is_some());
    assert_eq!(c.kind(0), openstitch_design::PointKind::Smooth);
    assert_eq!(
        c.point_handles(0),
        (Some(v2(50.0, 125.0)), Some(v2(150.0, 75.0)))
    );
    assert_eq!(
        menu(&t, &s, &sel),
        [
            ("toStraight", true),
            ("toCurve", false),
            ("toSmooth", false),
            ("toSymmetry", true),
            ("toCusp", true),
            ("splitAtPoint", true),
            ("deletePoint", true)
        ]
    );
    // Only the curve point shows handles: its straight neighbours have none.
    let design = s.design().clone();
    let overlay = t.point_overlay(&ctx(&design, &sel)).expect("overlay");
    assert_eq!(super::points::handle_lines(&overlay.contours[0]).len(), 2);
    // Dragging a handle of the smooth point turns the other one, in the preview and after.
    let sel = drag(&mut t, &mut s, sel, v2(150.0, 75.0), v2(100.0, 60.0), false);
    let c = s.design().objects[0].contours[0].clone();
    let (a, l) = c.point_handles(0);
    let (a, l) = (a.expect("a") - c.points[0], l.expect("l") - c.points[0]);
    assert!(l.distance(v2(0.0, -40.0)) < 1e-6, "{l:?}");
    assert!(
        a.x.abs() < 1e-6 && (a.y - 3125.0_f64.sqrt()).abs() < 1e-6,
        "opposite: {a:?}"
    );
    assert!(
        !c.is_curve_point(1) && !c.is_curve_point(3),
        "neighbours stay straight points"
    );
    // To Cusp, then the menu greys To Cusp and offers To Smooth again.
    assert_eq!(choose(&mut t, &mut s, &sel, P::ToCusp), 1);
    let m = menu(&t, &s, &sel);
    assert_eq!(
        (m[2], m[3], m[4]),
        (("toSmooth", true), ("toSymmetry", true), ("toCusp", false))
    );
    // The neighbour is a straight point: its menu is a straight point's.
    let design = s.design().clone();
    assert!(t.select_point_at(v2(300.0, 100.0), &ctx(&design, &sel)));
    let m = menu(&t, &s, &sel);
    assert_eq!(
        (m[0], m[1], m[2]),
        (
            ("toStraight", false),
            ("toCurve", true),
            ("toSmooth", false)
        )
    );
    undo(&mut s);
    undo(&mut s);
    undo(&mut s);
    assert!(s.design().objects[0].contours[0].curves.is_empty());
}

/// PE-DESIGN greys To Smooth, To Symmetry and To Cusp on the end of an open line, also after
/// To Curve (the point has one side only), and Split at Point on both ends.
#[test]
fn the_end_of_an_open_line_offers_what_pe_design_offers() {
    use super::points::PointAction as P;
    let mut s = session();
    let line = run(
        &mut s,
        &CommandRequest::new(
            "shape.polygon",
            json!({ "points": [[50, 50], [80, 50], [80, 80]], "closed": false, "outline": "#203040" }),
        ),
    )["id"]
        .as_u64()
        .expect("id");
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([line]);
    let design = s.design().clone();
    assert!(t.select_point_at(v2(500.0, 500.0), &ctx(&design, &sel)));
    let straight_end = [
        ("toStraight", false),
        ("toCurve", true),
        ("toSmooth", false),
        ("toSymmetry", false),
        ("toCusp", false),
        ("splitAtPoint", false),
        ("deletePoint", true),
    ];
    assert_eq!(menu(&t, &s, &sel), straight_end);
    assert_eq!(choose(&mut t, &mut s, &sel, P::ToCurve), 1);
    assert_eq!(
        menu(&t, &s, &sel),
        [
            ("toStraight", true),
            ("toCurve", false),
            ("toSmooth", false),
            ("toSymmetry", false),
            ("toCusp", false),
            ("splitAtPoint", false),
            ("deletePoint", true)
        ]
    );
    let design = s.design().clone();
    assert!(t.select_point_at(v2(800.0, 800.0), &ctx(&design, &sel)));
    assert_eq!(menu(&t, &s, &sel), straight_end);
}

#[test]
fn ctrl_click_selects_several_points_and_the_menu_acts_on_all() {
    use super::points::PointAction as P;
    let mut s = session();
    let id = outlined_rect(&mut s);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([id]);
    let sel = click_at(&mut t, &mut s, sel, v2(100.0, 100.0), 1);
    // Ctrl+click adds a point (PE-DESIGN's gesture).
    let mut e = PointerEvent::new(Phase::Down, v2(300.0, 200.0));
    e.free = true;
    let sel = send(&mut t, &mut s, sel, e);
    let sel = send(
        &mut t,
        &mut s,
        sel,
        PointerEvent::new(Phase::Up, v2(300.0, 200.0)),
    );
    let design = s.design().clone();
    let picked: Vec<_> = t
        .selected_points(&ctx(&design, &sel))
        .iter()
        .map(|p| p.index)
        .collect();
    assert_eq!(picked, [2, 0]);
    let overlay = t.point_overlay(&ctx(&design, &sel)).expect("overlay");
    assert_eq!(
        (overlay.selected, overlay.also),
        (Some((0, 2)), vec![(0, 0)])
    );
    // Split at Point needs one point. Delete stays available, as PE-DESIGN's does, and
    // removes both selected points in one step (#1260).
    let m = menu(&t, &s, &sel);
    assert_eq!(
        (m[5], m[6]),
        (("splitAtPoint", false), ("deletePoint", true))
    );
    assert_eq!(choose(&mut t, &mut s, &sel, P::Delete), 1);
    assert_eq!(s.design().objects[0].contours[0].points.len(), 2);
    undo(&mut s);
    assert_eq!(
        s.design().objects[0].contours[0].points.len(),
        4,
        "one Undo"
    );
    let design = s.design().clone();
    assert!(t.select_points(0, &[2, 0], &ctx(&design, &sel)));
    assert_eq!(
        choose(&mut t, &mut s, &sel, P::ToCurve),
        1,
        "one command for both"
    );
    assert!(
        s.design().objects[0].contours[0]
            .curves
            .iter()
            .all(Option::is_some)
    );
    undo(&mut s);
    assert!(
        s.design().objects[0].contours[0].curves.is_empty(),
        "one Undo"
    );
    // A right-click on a selected point keeps both; on another point it selects that one.
    let design = s.design().clone();
    assert!(t.select_point_at(v2(100.0, 100.0), &ctx(&design, &sel)));
    assert_eq!(t.selected_points(&ctx(&design, &sel)).len(), 2);
    assert!(t.select_point_at(v2(300.0, 100.0), &ctx(&design, &sel)));
    let picked: Vec<_> = t
        .selected_points(&ctx(&design, &sel))
        .iter()
        .map(|p| p.index)
        .collect();
    assert_eq!(picked, [1]);
    // Ctrl+click on a selected point takes it out again; a Ctrl-drag moves the point.
    let mut e = PointerEvent::new(Phase::Down, v2(300.0, 100.0));
    e.free = true;
    let sel = send(&mut t, &mut s, sel, e);
    let sel = send(
        &mut t,
        &mut s,
        sel,
        PointerEvent::new(Phase::Up, v2(300.0, 100.0)),
    );
    let design = s.design().clone();
    assert!(t.selected_points(&ctx(&design, &sel)).is_empty());
}

#[test]
fn split_at_point_and_delete_from_the_menu() {
    use super::points::PointAction as P;
    let mut s = session();
    let id = outlined_rect(&mut s);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([id]);
    let design = s.design().clone();
    assert!(t.select_point_at(v2(100.0, 100.0), &ctx(&design, &sel)));
    assert_eq!(choose(&mut t, &mut s, &sel, P::Delete), 1);
    assert_eq!(s.design().objects[0].contours[0].points.len(), 3);
    let design = s.design().clone();
    assert!(
        t.point_actions(&ctx(&design, &sel)).is_none(),
        "selection cleared"
    );
    // Delete goes on down to two points, as PE-DESIGN's; there it is still shown as
    // available and does nothing.
    assert!(t.select_point_at(v2(300.0, 100.0), &ctx(&design, &sel)));
    assert_eq!(choose(&mut t, &mut s, &sel, P::Delete), 1);
    assert_eq!(s.design().objects[0].contours[0].points.len(), 2);
    let design = s.design().clone();
    assert!(t.select_point_at(v2(300.0, 200.0), &ctx(&design, &sel)));
    assert_eq!(menu(&t, &s, &sel)[6], ("deletePoint", true));
    assert_eq!(choose(&mut t, &mut s, &sel, P::Delete), 0);
    assert_eq!(s.design().objects[0].contours[0].points.len(), 2);
    undo(&mut s);
    undo(&mut s);

    // An open line: Split at Point is greyed out on its ends, available in between.
    let line = run(
        &mut s,
        &CommandRequest::new(
            "shape.polygon",
            json!({ "points": [[50, 50], [80, 50], [80, 80]], "closed": false, "outline": "#203040" }),
        ),
    )["id"]
        .as_u64()
        .expect("id");
    let sel = BTreeSet::from([line]);
    let design = s.design().clone();
    assert!(t.select_point_at(v2(500.0, 500.0), &ctx(&design, &sel)));
    assert_eq!(menu(&t, &s, &sel)[5], ("splitAtPoint", false));
    assert!(t.select_point_at(v2(800.0, 500.0), &ctx(&design, &sel)));
    assert_eq!(menu(&t, &s, &sel)[5], ("splitAtPoint", true));
    let before = s.design().objects.len();
    assert_eq!(choose(&mut t, &mut s, &sel, P::Split), 1);
    assert_eq!(s.design().objects.len(), before + 1);
    undo(&mut s);
    assert_eq!(s.design().objects.len(), before, "one Undo");
}

// MARK: #1260, the rest of the Select Point tool

/// A drag from empty space is a box over the points; a click on empty space clears the
/// selection; dragging one of several selected points moves them all (PE-DESIGN 11.50,
/// captures of 2026-11-18, round 2).
#[test]
fn a_drag_box_selects_points_and_they_move_together() {
    let mut s = session();
    let id = outlined_rect(&mut s);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([id]);
    // A box round the two top corners, from outside the shape.
    let sel = drag(&mut t, &mut s, sel, v2(80.0, 80.0), v2(320.0, 120.0), false);
    assert_eq!(sel, BTreeSet::from([id]), "the shape stays selected");
    let design = s.design().clone();
    let mut picked: Vec<_> = t
        .selected_points(&ctx(&design, &sel))
        .iter()
        .map(|p| p.index)
        .collect();
    picked.sort_unstable();
    assert_eq!(picked, [0, 1]);
    assert!(t.preview().is_none(), "the box is gone");
    // Dragging one of them moves both: one command, one Undo.
    let before = s.design().clone();
    let sel = drag(&mut t, &mut s, sel, v2(100.0, 100.0), v2(80.0, 60.0), false);
    let c = s.design().objects[0].contours[0].clone();
    assert_eq!(
        (c.points[0], c.points[1]),
        (v2(80.0, 60.0), v2(280.0, 60.0))
    );
    assert_eq!(
        (c.points[2], c.points[3]),
        (v2(300.0, 200.0), v2(100.0, 200.0))
    );
    // An arrow key moves the selected points (0.5 mm on PE-DESIGN).
    let design = s.design().clone();
    let effects = t
        .nudge_points(5.0, 0.0, &ctx(&design, &sel))
        .expect("points selected");
    let sel = apply(&mut s, effects, sel);
    assert_eq!(s.design().objects[0].contours[0].points[1], v2(285.0, 60.0));
    undo(&mut s);
    undo(&mut s);
    assert_eq!(*s.design(), before, "one Undo each");
    // A click on empty space clears the selection, points and shape.
    let sel = click_at(&mut t, &mut s, sel, v2(600.0, 600.0), 1);
    assert!(sel.is_empty());
    let design = s.design().clone();
    assert!(t.selected_points(&ctx(&design, &sel)).is_empty());
    assert!(t.nudge_points(5.0, 0.0, &ctx(&design, &sel)).is_none());
}

/// One click on a side inserts a point and selects it; pressing and dragging there inserts
/// only (PE-DESIGN has no segment drag).
#[test]
fn one_click_on_a_side_inserts_a_point_and_a_drag_there_does_not_move_it() {
    let mut s = session();
    let id = outlined_rect(&mut s);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([id]);
    let sel = drag(
        &mut t,
        &mut s,
        sel,
        v2(200.0, 100.0),
        v2(200.0, 60.0),
        false,
    );
    let c = s.design().objects[0].contours[0].clone();
    assert_eq!(c.points.len(), 5);
    assert_eq!(
        c.points[1],
        v2(200.0, 100.0),
        "inserted where pressed, not moved"
    );
    let design = s.design().clone();
    assert_eq!(
        t.selected_point(&ctx(&design, &sel)).map(|p| p.index),
        Some(1)
    );
    undo(&mut s);
    assert_eq!(
        s.design().objects[0].contours[0].points.len(),
        4,
        "one Undo"
    );
}

/// A right-click on the shape, not on a point, opens the object's menu; on a point, the
/// point's.
#[test]
fn a_right_click_beside_the_points_is_for_the_object_menu() {
    let mut s = session();
    let id = outlined_rect(&mut s);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([id]);
    let design = s.design().clone();
    let c = ctx(&design, &sel);
    assert!(t.object_menu_at(v2(200.0, 100.0), &c), "on a side");
    assert!(t.object_menu_at(v2(200.0, 150.0), &c), "inside the shape");
    assert!(
        !t.object_menu_at(v2(100.0, 100.0), &c),
        "a point has its own menu"
    );
    assert!(!t.object_menu_at(v2(600.0, 600.0), &c), "empty space");
    assert_eq!(
        s.design().objects[0].contours[0].points.len(),
        4,
        "no point inserted"
    );
}

/// Alt held while an end point of an open line is dropped on its other end connects the two
/// ends: the line is closed as Close closes it, with its fill, the two ends one point (the
/// owner's hand check of PE-DESIGN, 2026-11-18). Another line's end is not joined that way.
#[test]
fn alt_drops_an_end_point_on_another_end_point_and_connects_them() {
    let mut s = session();
    let line = |s: &mut Session, points: serde_json::Value| {
        run(
            s,
            &CommandRequest::new(
                "shape.polygon",
                json!({ "points": points, "closed": false, "outline": "#203040" }),
            ),
        )["id"]
            .as_u64()
            .expect("id")
    };
    let a = line(&mut s, json!([[50, 50], [80, 50], [80, 80]]));
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([a]);
    let alt_drag = |t: &mut ToolController, s: &mut Session, sel: BTreeSet<u64>, from, to, alt| {
        let mut sel = sel;
        for (phase, p) in [(Phase::Down, from), (Phase::Dragged, to), (Phase::Up, to)] {
            let mut e = PointerEvent::new(phase, p);
            e.alt = alt;
            let design = s.design().clone();
            let effects = t.handle(e, &ctx(&design, &sel));
            sel = apply(s, effects, sel);
        }
        sel
    };
    // Without Alt the end only lies on the other end (PE-DESIGN, capture e6).
    let sel = alt_drag(
        &mut t,
        &mut s,
        sel,
        v2(800.0, 800.0),
        v2(501.0, 501.0),
        false,
    );
    assert!(!s.design().objects[0].contours[0].closed);
    undo(&mut s);
    // With Alt: closed, the doubled end one point, filled as Close fills it, in one Undo.
    let sel = alt_drag(
        &mut t,
        &mut s,
        sel,
        v2(800.0, 800.0),
        v2(501.0, 501.0),
        true,
    );
    let o = s.design().objects[0].clone();
    assert!(
        o.contours[0].closed && o.contours[0].points.len() == 2,
        "{o:?}"
    );
    assert_eq!(
        o.region.as_ref().map(|r| (r.kind, r.angle)),
        Some((RegionType::Fill, 45.0))
    );
    // Two commands (the point onto the other end, then Close); the editor runs a gesture's
    // commands as one undo step (`EditorApp::execute_all`), this helper one by one.
    undo(&mut s);
    undo(&mut s);
    let o = s.design().objects[0].clone();
    assert!(!o.contours[0].closed && o.region.is_none());
    assert_eq!(o.contours[0].points[2], v2(800.0, 800.0));
    // Onto another line's end: the two lines become one object (PE-DESIGN's manual, read
    // 2026-11-19; see `alt_drag_onto_another_open_lines_end_joins_the_two_lines`).
    let b = line(&mut s, json!([[120, 80], [120, 50]]));
    let sel = alt_drag(
        &mut t,
        &mut s,
        sel,
        v2(800.0, 800.0),
        v2(1199.0, 801.0),
        true,
    );
    assert_eq!(s.design().objects.len(), 1);
    assert!(s.design().index_of(b).is_none());
    let c = &s.design().objects[0].contours[0];
    assert_eq!((c.points.len(), c.points[2]), (4, v2(1200.0, 800.0)));
    let _ = sel;
}

/// Split at Point on a closed shape sewn as a region only (what the Rectangle tool draws) is
/// available like on any corner: the outline opens and the region becomes a line.
#[test]
fn split_at_point_makes_a_region_only_shape_a_line() {
    use super::points::PointAction as P;
    let mut s = session();
    let id = rect_design(&mut s);
    assert!(s.design().objects[0].line.is_none());
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([id]);
    let design = s.design().clone();
    assert!(t.select_point_at(v2(100.0, 100.0), &ctx(&design, &sel)));
    let m = menu(&t, &s, &sel);
    assert_eq!(
        (m[1], m[5], m[6]),
        (
            ("toCurve", true),
            ("splitAtPoint", true),
            ("deletePoint", true)
        )
    );
    assert_eq!(choose(&mut t, &mut s, &sel, P::Split), 1);
    let o = &s.design().objects[0];
    assert!(!o.contours[0].closed && o.region.is_none() && o.line.is_some());
    undo(&mut s);
    assert!(s.design().objects[0].contours[0].closed, "one Undo");
}

/// A press, two moves and a release, with the keys `keys` sets on every event.
fn drag_with(
    tool: &mut ToolController,
    s: &mut Session,
    mut sel: BTreeSet<u64>,
    from: Vec2,
    to: Vec2,
    keys: impl Fn(&mut PointerEvent),
) -> BTreeSet<u64> {
    for (phase, p) in [
        (Phase::Down, from),
        (Phase::Dragged, from.lerp(to, 0.5)),
        (Phase::Dragged, to),
        (Phase::Up, to),
    ] {
        let mut e = PointerEvent::new(phase, p);
        keys(&mut e);
        let design = s.design().clone();
        let effects = tool.handle(e, &ctx(&design, &sel));
        sel = apply(s, effects, sel);
    }
    sel
}

/// PE-DESIGN, the owner's hand check of 2026-11-19 (#1260): "dragging with shift held drags it
/// only horizontally / vertically"; "dragging with CTRL held, doesn't seem to do anything
/// special it is draged normally"; "no diff when a curve point is draged from one handle end
/// with shift or ctrl".
#[test]
fn shift_locks_a_point_drag_to_an_axis_and_ctrl_and_handles_are_as_without_a_key() {
    let mut s = session();
    let id = outlined_rect(&mut s);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([id]);
    let point = |s: &Session, i: usize| s.design().objects[0].contours[0].points[i];
    // Shift: along the axis the drag mostly runs on from its start; the other coordinate
    // stays the point's own.
    let sel = drag(
        &mut t,
        &mut s,
        sel,
        v2(100.0, 100.0),
        v2(160.0, 130.0),
        true,
    );
    assert_eq!(
        point(&s, 0),
        v2(160.0, 100.0),
        "mostly sideways: horizontal"
    );
    let sel = drag(
        &mut t,
        &mut s,
        sel,
        v2(160.0, 100.0),
        v2(175.0, 113.0),
        false,
    );
    assert_eq!(point(&s, 0), v2(175.0, 113.0), "no key: free");
    let sel = drag(&mut t, &mut s, sel, v2(175.0, 113.0), v2(180.0, 70.0), true);
    assert_eq!(point(&s, 0), v2(175.0, 70.0), "mostly up: vertical");
    // With Snap to Grid the moving coordinate lands on the grid and the locked one is not
    // pulled onto it.
    let sel = drag(&mut t, &mut s, sel, v2(175.0, 70.0), v2(163.0, 87.0), false);
    assert_eq!(point(&s, 0), v2(163.0, 87.0));
    t.set_snap(Some(50.0));
    let sel = drag(&mut t, &mut s, sel, v2(163.0, 87.0), v2(196.0, 95.0), true);
    assert_eq!(point(&s, 0), v2(200.0, 87.0), "x on the grid, y as it was");
    // Ctrl: as without it. The grid still applies (Ctrl does not switch snapping off), and
    // there is no axis lock.
    let sel = drag_with(
        &mut t,
        &mut s,
        sel,
        v2(300.0, 100.0),
        v2(333.0, 128.0),
        |e| e.free = true,
    );
    assert_eq!(
        point(&s, 1),
        v2(350.0, 150.0),
        "Ctrl-drag: both axes, on the grid"
    );
    undo(&mut s);
    let sel = drag(
        &mut t,
        &mut s,
        sel,
        v2(300.0, 100.0),
        v2(333.0, 128.0),
        false,
    );
    assert_eq!(point(&s, 1), v2(350.0, 150.0), "the same without Ctrl");
    t.set_snap(None);
    // A handle end: Shift or Ctrl changes nothing; the point's kind still rules (a smooth
    // point's other handle turns with the dragged one).
    run(
        &mut s,
        &CommandRequest::new(
            "object.setPointKind",
            json!({ "id": id, "index": 2, "kind": "curve" }),
        ),
    );
    let mut sel = click_at(&mut t, &mut s, sel, v2(300.0, 200.0), 1);
    for (step, shift, free) in [(v2(31.0, 17.0), true, false), (v2(-9.0, 23.0), false, true)] {
        let c = s.design().objects[0].contours[0].clone();
        let from = c.curve(2).expect("the side after the curve point").c1;
        let mut plain = c.clone();
        plain.move_handle(2, true, from + step);
        sel = drag_with(&mut t, &mut s, sel, from, from + step, |e| {
            e.shift = shift;
            e.free = free;
        });
        let now = s.design().objects[0].contours[0].clone();
        let (z, other) = (now.curve(2).expect("curve"), now.curve(1).expect("curve"));
        assert!(
            z.c1.distance(from + step) < 0.2,
            "the handle is where it was dropped, both axes: {:?}",
            z.c1
        );
        let want = plain.curve(1).expect("curve").c2;
        assert!(
            other.c2.distance(want) < 0.2,
            "the smooth point's other handle follows as without a key: {:?} / {want:?}",
            other.c2
        );
        assert!(
            other.c2.distance(c.curve(1).expect("curve").c2) > 1.0,
            "it turned"
        );
    }
}

/// PE-DESIGN, the owner's hand check of 2026-11-19 (#1260): "shift + click allows the user to
/// select more points".
#[test]
fn shift_click_adds_a_point_to_the_selection_and_ctrl_click_adds_or_takes_out() {
    let mut s = session();
    let id = outlined_rect(&mut s);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([id]);
    let mut sel = click_at(&mut t, &mut s, sel, v2(100.0, 100.0), 1);
    let shift_click = |t: &mut ToolController, s: &mut Session, sel: BTreeSet<u64>, at: Vec2| {
        let sel = send(t, s, sel, PointerEvent::new(Phase::Down, at).shift());
        send(t, s, sel, PointerEvent::new(Phase::Up, at).shift())
    };
    let picked = |t: &ToolController, s: &Session, sel: &BTreeSet<u64>| {
        let design = s.design().clone();
        t.selected_points(&ctx(&design, sel))
            .iter()
            .map(|p| p.index)
            .collect::<Vec<_>>()
    };
    sel = shift_click(&mut t, &mut s, sel, v2(300.0, 200.0));
    assert_eq!(picked(&t, &s, &sel), [2, 0]);
    sel = shift_click(&mut t, &mut s, sel, v2(100.0, 200.0));
    assert_eq!(picked(&t, &s, &sel), [3, 2, 0]);
    // On a selected point it changes nothing: Shift+click only adds (Ctrl+click is what
    // takes a selected point out, PE-DESIGN's manual).
    sel = shift_click(&mut t, &mut s, sel, v2(300.0, 200.0));
    assert_eq!(picked(&t, &s, &sel), [3, 2, 0]);
    let mut e = PointerEvent::new(Phase::Down, v2(300.0, 200.0));
    e.free = true;
    sel = send(&mut t, &mut s, sel, e);
    sel = send(
        &mut t,
        &mut s,
        sel,
        PointerEvent::new(Phase::Up, v2(300.0, 200.0)),
    );
    assert_eq!(picked(&t, &s, &sel), [3, 0], "Ctrl+click takes it out");
    assert_eq!(
        s.design().objects[0].contours[0].points[3],
        v2(100.0, 200.0),
        "a click moves nothing"
    );
}

/// PE-DESIGN, the owner's hand check of 2026-11-19 (#1260): "dragging a box over two shapes
/// selects points of both shapes". The selection then spans the shapes: a drag, the arrow
/// keys, the menu and Delete act on every selected point, one command per outline.
#[test]
fn a_drag_box_over_two_shapes_selects_points_of_both_and_they_are_edited_together() {
    use super::points::{PointAction as P, PointRef};
    let mut s = session();
    let a = outlined_rect(&mut s);
    let b = run(
        &mut s,
        &CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 40, "y": 10, "width": 20, "height": 10, "outline": "#203040" }),
        ),
    )["id"]
        .as_u64()
        .expect("id");
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let both = BTreeSet::from([a, b]);
    let design = s.design().clone();
    assert_eq!(
        t.point_overlays(&ctx(&design, &both)).len(),
        2,
        "both show their points"
    );
    // A box over the right side of the first and the left side of the second.
    let sel = drag(
        &mut t,
        &mut s,
        both.clone(),
        v2(280.0, 80.0),
        v2(420.0, 220.0),
        false,
    );
    assert_eq!(sel, both, "both stay selected");
    let design = s.design().clone();
    let mut picked: Vec<(u64, usize)> = t
        .selected_points(&ctx(&design, &sel))
        .iter()
        .map(|p| (p.id, p.index))
        .collect();
    picked.sort_unstable();
    assert_eq!(picked, [(a, 1), (a, 2), (b, 0), (b, 3)]);
    let overlays = t.point_overlays(&ctx(&design, &sel));
    assert!(
        overlays
            .iter()
            .all(|o| o.selected.is_some() && o.also.len() == 1),
        "two selected points drawn on each shape"
    );
    // The menu's greying is over all of them.
    let m = menu(&t, &s, &sel);
    assert_eq!(
        m,
        [
            ("toStraight", false),
            ("toCurve", true),
            ("toSmooth", false),
            ("toSymmetry", false),
            ("toCusp", false),
            ("splitAtPoint", false),
            ("deletePoint", true),
        ]
    );
    let points = |s: &Session, k: usize| s.design().objects[k].contours[0].points.clone();
    // Dragging one selected point moves all four: one object.movePoints per outline, in one
    // effect (the app runs an effect's commands as one undo step).
    let design = s.design().clone();
    let c = ctx(&design, &sel);
    t.handle(PointerEvent::new(Phase::Down, v2(300.0, 100.0)), &c);
    t.handle(PointerEvent::new(Phase::Dragged, v2(310.0, 120.0)), &c);
    let effects = t.handle(PointerEvent::new(Phase::Up, v2(310.0, 120.0)), &c);
    let [ToolEffect::Execute { requests, .. }] = effects.as_slice() else {
        panic!("one effect: {effects:?}");
    };
    assert_eq!(
        requests.iter().map(|r| r.command).collect::<Vec<_>>(),
        ["object.movePoints", "object.movePoints"]
    );
    let sel = apply(&mut s, effects, sel);
    assert_eq!(
        (points(&s, 0)[1], points(&s, 0)[2], points(&s, 0)[0]),
        (v2(310.0, 120.0), v2(310.0, 220.0), v2(100.0, 100.0))
    );
    assert_eq!(
        (points(&s, 1)[0], points(&s, 1)[3], points(&s, 1)[1]),
        (v2(410.0, 120.0), v2(410.0, 220.0), v2(600.0, 100.0))
    );
    // The arrow keys.
    let design = s.design().clone();
    let effects = t
        .nudge_points(5.0, 0.0, &ctx(&design, &sel))
        .expect("points selected");
    assert!(
        matches!(effects.as_slice(), [ToolEffect::Execute { requests, .. }] if requests.len() == 2)
    );
    let sel = apply(&mut s, effects, sel);
    assert_eq!(
        (points(&s, 0)[1], points(&s, 1)[0]),
        (v2(315.0, 120.0), v2(415.0, 120.0))
    );
    // To Curve on all four; To Straight is then available, To Curve no longer.
    assert_eq!(choose(&mut t, &mut s, &sel, P::ToCurve), 1, "one effect");
    for (k, i) in [(0, 1), (0, 2), (1, 0), (1, 3)] {
        assert!(
            s.design().objects[k].contours[0].is_curve_point(i),
            "{k} {i}"
        );
    }
    let m = menu(&t, &s, &sel);
    assert_eq!((m[0], m[1]), (("toStraight", true), ("toCurve", false)));
    // Shift+click adds a point of the other shape; a mixed selection offers both.
    let sel = send(
        &mut t,
        &mut s,
        sel,
        PointerEvent::new(Phase::Down, v2(600.0, 100.0)).shift(),
    );
    let sel = send(
        &mut t,
        &mut s,
        sel,
        PointerEvent::new(Phase::Up, v2(600.0, 100.0)).shift(),
    );
    let design = s.design().clone();
    assert_eq!(t.selected_points(&ctx(&design, &sel)).len(), 5);
    let m = menu(&t, &s, &sel);
    assert_eq!((m[0], m[1]), (("toStraight", true), ("toCurve", true)));
    // Delete removes the selected points of both shapes: the first keeps 2, the second 1 of
    // its 4 would be too few (3 selected), so it is left as it is.
    let design = s.design().clone();
    let effects = t
        .delete_point(&ctx(&design, &sel))
        .expect("points selected");
    let [ToolEffect::Execute { requests, .. }] = effects.as_slice() else {
        panic!("one effect: {effects:?}");
    };
    assert_eq!(
        requests.len(),
        1,
        "only the outline that can lose its points"
    );
    let sel = apply(&mut s, effects, sel);
    assert_eq!((points(&s, 0).len(), points(&s, 1).len()), (2, 4));
    // Points named across shapes, as ui.pointMenu selects them; one that does not exist
    // selects nothing.
    let design = s.design().clone();
    let c = ctx(&design, &sel);
    let at = |id, index| PointRef {
        id,
        contour: 0,
        index,
    };
    assert!(!t.select_point_refs(&[at(a, 0), at(b, 9)], &c));
    assert!(t.select_point_refs(&[at(b, 1), at(a, 0)], &c));
    assert_eq!(
        t.selected_by_outline(&c),
        [(b, 0, vec![1]), (a, 0, vec![0])]
    );
    let effects = t.delete_point(&c).expect("points selected");
    assert!(
        matches!(effects.as_slice(), [ToolEffect::Execute { requests, .. }] if requests.len() == 1),
        "the first shape is at 2 points and keeps them"
    );
}

/// PE-DESIGN's manual (read 2026-11-19, #1260): Alt + drag of an end point onto another open
/// line's end joins the two lines into one shape; a red square shows while the two points
/// overlap, and the connectable ends are marked while Alt is held.
#[test]
fn alt_drag_onto_another_open_lines_end_joins_the_two_lines() {
    let mut s = session();
    let mut line = |points: serde_json::Value| {
        run(
            &mut s,
            &CommandRequest::new(
                "shape.polygon",
                json!({ "points": points, "closed": false, "outline": "#203040" }),
            ),
        )["id"]
            .as_u64()
            .expect("id")
    };
    let a = line(json!([[10, 60], [40, 60], [40, 30]]));
    let b = line(json!([[50, 30], [70, 30], [70, 60]]));
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([a]);
    let event = |phase, at: Vec2, alt: bool| {
        let mut e = PointerEvent::new(phase, at);
        e.alt = alt;
        e
    };
    // Without Alt the end only lies on the other line's end.
    let sel = drag(
        &mut t,
        &mut s,
        sel,
        v2(400.0, 300.0),
        v2(500.0, 300.0),
        false,
    );
    assert_eq!(s.design().objects.len(), 2);
    undo(&mut s);
    // The marks: with Alt held and the end selected, its own other end and both ends of the
    // other line; none without Alt, none for a middle point.
    let design = s.design().clone();
    let c = ctx(&design, &sel);
    assert_eq!(
        t.connect_marks(true, &c),
        [v2(100.0, 600.0), v2(500.0, 300.0), v2(700.0, 600.0)]
    );
    assert!(t.connect_marks(false, &c).is_empty());
    // Alt + drag: the red square while the dragged end lies on the other end.
    t.handle(event(Phase::Down, v2(400.0, 300.0), true), &c);
    t.handle(event(Phase::Dragged, v2(450.0, 320.0), true), &c);
    assert_eq!(t.connect_overlap(), None, "not on an end yet");
    assert_eq!(
        t.connect_marks(false, &c).len(),
        3,
        "marked while the Alt drag runs"
    );
    t.handle(event(Phase::Dragged, v2(498.0, 301.0), true), &c);
    assert_eq!(t.connect_overlap(), Some(v2(500.0, 300.0)));
    let effects = t.handle(event(Phase::Up, v2(498.0, 301.0), true), &c);
    assert_eq!(t.connect_overlap(), None);
    let [ToolEffect::Execute { requests, .. }] = effects.as_slice() else {
        panic!("one effect: {effects:?}");
    };
    assert_eq!(
        requests.iter().map(|r| r.command).collect::<Vec<_>>(),
        ["object.setPoint", "shape.joinLines"]
    );
    let sel = apply(&mut s, effects, sel);
    let d = s.design();
    assert_eq!((d.objects.len(), d.objects[0].id), (1, a));
    assert_eq!(
        d.objects[0].contours[0].points.len(),
        5,
        "the two ends are one point"
    );
    assert!(d.objects.iter().all(|o| o.id != b));
    let design = s.design().clone();
    assert!(t.selected_points(&ctx(&design, &sel)).is_empty());
}

/// PE-DESIGN capture f, 2026-11-19: a plain click on one of several selected points keeps the
/// whole selection.
#[test]
fn a_plain_click_on_one_of_several_selected_points_keeps_the_selection() {
    let mut s = session();
    let id = outlined_rect(&mut s);
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([id]);
    let design = s.design().clone();
    assert!(t.select_points(0, &[2, 0, 3], &ctx(&design, &sel)));
    let before = s.design().clone();
    // Press and release on point 0, no drag.
    let sel = click_at(&mut t, &mut s, sel, v2(100.0, 100.0), 1);
    let design = s.design().clone();
    let mut picked: Vec<usize> = t
        .selected_points(&ctx(&design, &sel))
        .iter()
        .map(|p| p.index)
        .collect();
    picked.sort_unstable();
    assert_eq!(picked, [0, 2, 3], "all three stay selected");
    assert_eq!(*s.design(), before, "nothing moved");
    // On a point that is not selected it selects that one alone.
    let sel = click_at(&mut t, &mut s, sel, v2(300.0, 100.0), 1);
    let design = s.design().clone();
    let picked: Vec<usize> = t
        .selected_points(&ctx(&design, &sel))
        .iter()
        .map(|p| p.index)
        .collect();
    assert_eq!(picked, [1]);
}

/// Delete with nothing that can be deleted (every outline with selected points would keep
/// fewer than 2) removes nothing and keeps the selection, as the menu's Delete does.
#[test]
fn delete_with_nothing_deletable_keeps_the_selection() {
    let mut s = session();
    let mut line = |points: serde_json::Value| {
        run(
            &mut s,
            &CommandRequest::new(
                "shape.polygon",
                json!({ "points": points, "closed": false, "outline": "#203040" }),
            ),
        )["id"]
            .as_u64()
            .expect("id")
    };
    let a = line(json!([[10, 10], [30, 10]]));
    let b = line(json!([[10, 30], [30, 30]]));
    let mut t = ToolController::default();
    t.set_tool(Tool::Point);
    let sel = BTreeSet::from([a, b]);
    let design = s.design().clone();
    let c = ctx(&design, &sel);
    let at = |id, index| super::points::PointRef {
        id,
        contour: 0,
        index,
    };
    // One outline, then two outlines.
    for points in [vec![at(a, 0)], vec![at(a, 0), at(b, 1)]] {
        assert!(t.select_point_refs(&points, &c));
        assert_eq!(
            t.delete_point(&c),
            Some(vec![]),
            "the key is taken, nothing is sent"
        );
        assert_eq!(t.selected_points(&c), points, "the selection stays");
        assert!(
            t.point_action(super::points::PointAction::Delete, &c)
                .is_empty()
        );
        assert_eq!(t.selected_points(&c), points);
    }
}
