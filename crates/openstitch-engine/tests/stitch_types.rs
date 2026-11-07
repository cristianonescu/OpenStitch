//! Engine commands for the line and region sew types beyond running/triple/zigzag/fill/satin.

use openstitch_core::Command;
use openstitch_design::{Arrange, ChainShape, Compiler, LineType};
use openstitch_engine::{Session, project};
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn temp_dir() -> TempDir {
    TempDir::new("stitch-types")
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// The session and the guard of its folder, which must outlive it.
fn session_with_circle() -> (Session, TempDir) {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 50, "rx": 30, "ry": 20, "outline": "Black"}),
    );
    (s, tmp)
}

#[test]
fn patterns_are_listed_by_kind() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    let list = run(&mut s, "patterns.list", json!({}));
    let count = |kind: &str| {
        list.as_array()
            .expect("array")
            .iter()
            .filter(|p| p["kind"] == kind)
            .count()
    };
    assert!(count("motif") >= 6);
    assert!(count("fill") >= 6);
    assert!(count("decorative") >= 6);
    assert_eq!(count("net"), 6);
}

#[test]
fn every_line_type_is_set_sewn_saved_and_undone() {
    let (mut s, _dir) = session_with_circle();
    for kind in LineType::ALL {
        run(
            &mut s,
            "object.setLine",
            json!({"id": 1, "type": kind.id()}),
        );
        assert_eq!(
            s.design().objects[0].line.as_ref().map(|l| l.kind),
            Some(kind)
        );
        let p = Compiler::default().compile(s.design());
        assert!(p.count(Command::Stitch) > 20, "{kind:?}");
    }
    run(
        &mut s,
        "object.setLine",
        json!({
            "id": 1, "type": "motif", "motif": "heart", "width": 5, "height": 4,
            "spacing": 1, "arrange": "alternate", "pitch": 1.5
        }),
    );
    let l = s.design().objects[0].line.clone().expect("line");
    assert_eq!(l.motif.as_deref(), Some("heart"));
    assert_eq!((l.width, l.height, l.spacing), (5.0, Some(4.0), Some(1.0)));
    assert_eq!(l.arrange, Arrange::Alternate);
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "chain", "chainShape": "diamond", "runTimes": 6}),
    );
    let l = s.design().objects[0].line.clone().expect("line");
    assert_eq!((l.chain_shape, l.run_times), (ChainShape::Diamond, 6));
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "stem", "angle": 120, "single": true, "spacing": 2}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "eStitch", "strokeTimes": 3}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "zigzag", "halfStitch": true}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "cutting", "leaveUncut": true}),
    );
    // Round trip through .osd keeps every attribute.
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(&mut s, "history.undo", json!({}));
    assert_eq!(
        s.design().objects[0].line.as_ref().map(|l| l.kind),
        Some(LineType::Zigzag)
    );
}

#[test]
fn bad_line_parameters_fail_cleanly() {
    let (mut s, _dir) = session_with_circle();
    let before = s.design().clone();
    for params in [
        json!({"id": 1, "type": "sparkle"}),
        json!({"id": 1, "type": "motif", "motif": "nope"}),
        json!({"id": 1, "motif": 3}),
        json!({"id": 1, "height": 0}),
        json!({"id": 1, "spacing": 50}),
        json!({"id": 1, "type": "stem", "spacing": 0}),
        json!({"id": 1, "angle": 10}),
        json!({"id": 1, "strokeTimes": 4}),
        json!({"id": 1, "arrange": "sideways"}),
        json!({"id": 1, "chainShape": "circle"}),
        json!({"id": 1, "leaveUncut": "yes"}),
        json!({"id": 1, "halfStitch": 1}),
        json!({"id": 1, "type": "running", "runTimes": 6}),
        json!({"id": 1, "type": "chain", "runTimes": 7}),
    ] {
        assert!(
            s.execute("object.setLine", Some(&params)).is_err(),
            "{params} should fail"
        );
        assert_eq!(s.design(), &before, "{params} changed the design");
    }
}

#[test]
fn satin_and_fill_options_are_set_and_undone() {
    use openstitch_design::{FeatherSide, RegionType, UnderlayStyle};
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 50, "rx": 30, "ry": 10, "fill": "Red"}),
    );
    let plain = Compiler::default().compile(s.design());
    // Gradation and Feathered edge exclude each other and Auto direction (the guide), so they
    // are set one after another.
    run(
        &mut s,
        "object.setRegion",
        json!({
            "id": 1, "type": "satin", "underlay": true, "underlayStyle": "double",
            "underlaySpacing": 3, "gradation": 40, "splitLength": 4, "travel": true,
            "stagger": "random", "staggerSeed": 42
        }),
    );
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!(r.kind, RegionType::Satin);
    assert_eq!(
        (r.stagger, r.stagger_seed),
        (openstitch_design::StaggerKind::Random, Some(42))
    );
    assert!(r.running_stitch_path.is_some() && r.underlay);
    assert_eq!(r.underlay_style, UnderlayStyle::Double);
    assert_eq!(
        (r.underlay_spacing, r.gradation, r.split_length),
        (
            Some(3.0),
            Some(openstitch_design::Gradation::continuous(40.0)),
            Some(4.0)
        )
    );
    let graded = Compiler::default().compile(s.design());
    assert_ne!(graded, plain);
    assert!(graded.count(Command::Stitch) > 50);
    // 100 % turns gradation off; a feathered edge can then be set.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "gradation": 100, "featherSide": "both", "featherWidth": 1.5}),
    );
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!(r.gradation, None);
    assert_eq!(
        r.feather.map(|f| (f.side, f.width)),
        Some((FeatherSide::Both, 1.5))
    );
    assert_ne!(Compiler::default().compile(s.design()), graded);
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "autoDirection": true}),
    );
    assert!(
        s.design().objects[0]
            .region
            .as_ref()
            .expect("r")
            .auto_direction
    );
    // Zero feather width and zero split length turn the options off.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "autoDirection": false, "featherWidth": 0, "splitLength": 0}),
    );
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!((r.gradation, r.feather, r.split_length), (None, None, None));
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    for _ in 0..4 {
        run(&mut s, "history.undo", json!({}));
    }
    assert_eq!(Compiler::default().compile(s.design()), plain);
}

#[test]
fn gradation_pattern_options() {
    use openstitch_design::Gradation;
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 30, "height": 30, "fill": "Red"}),
    );
    let gradation = |s: &Session| s.design().objects[0].region.as_ref().expect("r").gradation;
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "gradationPattern": true}),
    );
    assert_eq!(gradation(&s), Some(Gradation::PE_DEFAULT));
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "gradationMax": 80, "gradationSteps": 4}),
    );
    assert_eq!(
        gradation(&s),
        Some(Gradation {
            max: 80.0,
            min: 5.0,
            steps: Some(4)
        })
    );
    // Turning the pattern on again keeps it; off clears it.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "gradationPattern": true}),
    );
    assert_eq!(gradation(&s).and_then(|g| g.steps), Some(4));
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "gradationPattern": false}),
    );
    assert_eq!(gradation(&s), None);
    // A continuous (old) gradation becomes PE's default pattern when a pattern option is set.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "gradation": 40}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "gradationMin": 10}),
    );
    assert_eq!(
        gradation(&s),
        Some(Gradation {
            max: 53.0,
            min: 10.0,
            steps: Some(9)
        })
    );
    for params in [
        json!({"id": 1, "gradationMax": 4}),
        json!({"id": 1, "gradationMin": 101}),
        json!({"id": 1, "gradationSteps": 1}),
        json!({"id": 1, "gradationSteps": 21}),
    ] {
        let reply = s.handle(&json!({"command": "object.setRegion", "params": params}));
        assert_eq!(reply["ok"], json!(false), "{params}");
    }
}

#[test]
fn gradation_and_feathered_edge_follow_the_guide_exclusions() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 30, "height": 10, "fill": "Red"}),
    );
    run(
        &mut s,
        "text.add",
        json!({"text": "A", "x": 0, "y": 30, "size": 10}),
    );
    let refused = |s: &mut Session, params: Value, why: &str| {
        let before = s.design().clone();
        let reply = s.handle(&json!({"command": "object.setRegion", "params": params}));
        assert_eq!(reply["ok"], json!(false), "{params}");
        assert!(reply.to_string().contains(why), "{params}: {reply}");
        assert_eq!(s.design(), &before, "{params}");
    };
    // Gradation and Feathered edge exclude each other.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "gradationPattern": true}),
    );
    refused(
        &mut s,
        json!({"id": 1, "featherWidth": 2}),
        "with gradation",
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "gradationPattern": false}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "featherWidth": 2}),
    );
    refused(
        &mut s,
        json!({"id": 1, "gradationPattern": true}),
        "feathered edge",
    );
    // Both in one call: whichever comes second is refused.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "featherWidth": 0}),
    );
    refused(
        &mut s,
        json!({"id": 1, "gradation": 40, "featherWidth": 2}),
        "not available",
    );
    // Not with Auto direction, not on text, not for other sew types (yet).
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "autoDirection": true}),
    );
    refused(
        &mut s,
        json!({"id": 1, "gradationPattern": true}),
        "Auto direction",
    );
    refused(
        &mut s,
        json!({"id": 1, "featherWidth": 2}),
        "Auto direction",
    );
    refused(
        &mut s,
        json!({"id": 2, "gradationPattern": true}),
        "for text",
    );
    refused(&mut s, json!({"id": 2, "featherWidth": 2}), "for text");
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "programmableFill", "autoDirection": false}),
    );
    refused(
        &mut s,
        json!({"id": 1, "gradationPattern": true}),
        "fill and satin",
    );
}

#[test]
fn unavailable_gradation_and_feather_are_kept_but_not_sewn() {
    use openstitch_design::{Feather, FeatherSide, Gradation, RegionType};
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 30, "height": 10, "fill": "Red"}),
    );
    // A file may hold options that are unavailable now (an older version, a changed type).
    let mut d = s.design().clone();
    let r = d.objects[0].region.as_mut().expect("region");
    r.auto_direction = true;
    r.gradation = Some(Gradation::PE_DEFAULT);
    let auto_plain = {
        let mut a = s.design().clone();
        a.objects[0].region.as_mut().expect("r").auto_direction = true;
        Compiler::default().compile(&a)
    };
    assert_eq!(Compiler::default().compile(&d), auto_plain);
    let r = d.objects[0].region.as_mut().expect("region");
    r.auto_direction = false;
    r.feather = Some(Feather {
        side: FeatherSide::Both,
        width: 3.0,
    });
    // Gradation wins over a feathered edge stored with it.
    let mut graded = s.design().clone();
    graded.objects[0].region.as_mut().expect("r").gradation = Some(Gradation::PE_DEFAULT);
    assert_eq!(
        Compiler::default().compile(&d),
        Compiler::default().compile(&graded)
    );
    let r = d.objects[0].region.as_mut().expect("region");
    r.gradation = None;
    r.kind = RegionType::ProgrammableFill;
    assert_eq!(r.sewn_feather(), None);
    assert_ne!(
        Compiler::default().compile(s.design()),
        Compiler::default().compile(&graded)
    );
}

#[test]
fn fill_stitch_type_is_set_sewn_saved_and_undone() {
    use openstitch_design::FillStitchType;
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 40, "height": 40}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "fill", "angle": 0, "underlay": false}),
    );
    let flat = Compiler::default().compile(s.design());
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "stitchType": 2}),
    );
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!(r.stitch_type, FillStitchType::Two);
    // PE-DESIGN 11.50's 40 mm square at 0 degrees sews 2006 stitches as Type 1 and 2001 as
    // Type 2 (round 5): the sloped rows need no row-end steps.
    let sloped = Compiler::default().compile(s.design());
    assert_ne!(sloped, flat);
    assert!(sloped.count(Command::Stitch) < flat.count(Command::Stitch));
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "stitchType": 1}),
    );
    assert_eq!(Compiler::default().compile(s.design()), flat);
    // The inspector's choice sends the button as a string.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "stitchType": "2"}),
    );
    assert_eq!(Compiler::default().compile(s.design()), sloped);
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.undo", json!({}));
    assert_eq!(Compiler::default().compile(s.design()), flat);
}

#[test]
fn bad_region_options_fail_cleanly() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 10, "height": 10}),
    );
    let before = s.design().clone();
    for params in [
        json!({"id": 1, "underlayStyle": "zag"}),
        json!({"id": 1, "underlaySpacing": 0.5}),
        json!({"id": 1, "autoDirection": "on"}),
        json!({"id": 1, "gradation": 2}),
        json!({"id": 1, "gradation": 101}),
        json!({"id": 1, "featherSide": "top"}),
        json!({"id": 1, "featherWidth": -1}),
        json!({"id": 1, "splitLength": 1}),
        json!({"id": 1, "splitLength": 13}),
        json!({"id": 1, "travel": 1}),
        json!({"id": 1, "stitchType": 0}),
        json!({"id": 1, "stitchType": 1.5}),
        json!({"id": 1, "stitchType": 3}),
        json!({"id": 1, "stitchType": "3"}),
        json!({"id": 1, "stitchType": "two"}),
        json!({"id": 1, "stitchType": "3"}),
        json!({"id": 1, "stitchType": "two"}),
    ] {
        assert!(
            s.execute("object.setRegion", Some(&params)).is_err(),
            "{params} should fail"
        );
        assert_eq!(s.design(), &before, "{params} changed the design");
    }
}

#[test]
fn every_region_type_is_set_sewn_saved_and_undone() {
    use openstitch_design::RegionType;
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 50, "rx": 25, "ry": 18, "fill": "Red"}),
    );
    for kind in RegionType::ALL {
        run(
            &mut s,
            "object.setRegion",
            json!({"id": 1, "type": kind.id()}),
        );
        assert_eq!(
            s.design().objects[0].region.as_ref().map(|r| r.kind),
            Some(kind)
        );
        let p = Compiler::default().compile(s.design());
        assert!(p.count(Command::Stitch) > 50, "{kind:?}");
    }
    let cases = [
        json!({"type": "programmableFill", "pattern": "bricks", "patternWidth": 6,
               "patternHeight": 4, "rowOffset": 50, "columnOffset": 25, "frequency": 20,
               "baseSewing": true}),
        json!({"type": "motifFill", "pattern": "leaf", "spacing": 0, "rowOffset": 30}),
        json!({"type": "piping", "runPitch": 3, "frequency": 50}),
        json!({"type": "crossStitch", "spacing": 2, "triple": true}),
        json!({"type": "concentricCircle", "startInside": true, "density": 1}),
        json!({"type": "spiral", "counterclockwise": true}),
        json!({"type": "flexibleSpiral", "spacing": 1.5, "startInside": true}),
        json!({"type": "stippling", "spacing": 1.5, "runPitch": 1.5}),
        json!({"type": "netFill", "pattern": "honeycomb", "spacing": 5}),
        json!({"type": "zigzagNetFill", "pattern": "triangle", "patternWidth": 1.2,
               "underlay": true}),
        json!({"type": "decorativeFill", "pattern": "scales", "randomShift": 20,
               "travel": false}),
    ];
    for mut c in cases {
        c["id"] = json!(1);
        run(&mut s, "object.setRegion", c.clone());
        let p = Compiler::default().compile(s.design());
        assert!(p.count(Command::Stitch) > 50, "{c}");
    }
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!(r.pattern.as_deref(), Some("scales"));
    assert_eq!((r.random_shift, r.running_stitch_path), (Some(20.0), None));
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(&mut s, "history.undo", json!({}));
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!(r.kind, RegionType::ZigzagNetFill);
    assert!(r.underlay);
}

#[test]
fn bad_region_type_parameters_fail_cleanly() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 10, "height": 10}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "netFill"}),
    );
    let before = s.design().clone();
    for params in [
        json!({"id": 1, "type": "sparkle"}),
        json!({"id": 1, "pattern": "heart"}),
        json!({"id": 1, "type": "zigzagNetFill", "pattern": "waves"}),
        json!({"id": 1, "type": "fill", "pattern": "square"}),
        json!({"id": 1, "type": "programmableFill", "pattern": "nope"}),
        json!({"id": 1, "pattern": 5}),
        json!({"id": 1, "patternWidth": 0}),
        json!({"id": 1, "patternHeight": 60}),
        json!({"id": 1, "spacing": 0.2}),
        json!({"id": 1, "spacing": 25}),
        json!({"id": 1, "runPitch": 0.5}),
        json!({"id": 1, "frequency": 150}),
        json!({"id": 1, "rowOffset": -1}),
        json!({"id": 1, "columnOffset": 101}),
        json!({"id": 1, "startInside": "yes"}),
        json!({"id": 1, "counterclockwise": 0}),
        json!({"id": 1, "triple": "no"}),
        json!({"id": 1, "randomShift": 60}),
        json!({"id": 1, "baseSewing": null, "spacing": "wide"}),
    ] {
        assert!(
            s.execute("object.setRegion", Some(&params)).is_err(),
            "{params} should fail"
        );
        assert_eq!(s.design(), &before, "{params} changed the design");
    }
}

#[test]
fn auto_satin_half_stitch_is_set_sewn_saved_and_undone() {
    // A 5 mm band bent through 120 degrees (compat satin-auto-bend-120), Auto satin (#372).
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "shape.polygon",
        json!({"points": [[0, 0], [30, 0], [21.34, 15], [17.01, 12.5], [21.34, 5], [0, 5]]}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "satin", "autoDirection": true, "density": 3, "underlay": false}),
    );
    let region = |s: &Session| s.design().objects[0].region.clone().expect("region");
    assert!(
        region(&s).half_stitch,
        "on by default, as PE-DESIGN's drawing tools"
    );
    let on = Compiler::default().compile(s.design());
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "halfStitch": false}),
    );
    assert!(!region(&s).half_stitch);
    let off = Compiler::default().compile(s.design());
    assert_ne!(on, off, "Half Stitch moves the bend's inner needle points");
    assert_eq!(on.count(Command::Stitch), off.count(Command::Stitch));
    let saved = serde_json::to_string(s.design()).expect("json");
    assert!(saved.contains("\"halfStitch\":false"), "saved when off");
    run(&mut s, "history.undo", json!({}));
    assert!(region(&s).half_stitch);
    assert!(
        s.execute(
            "object.setRegion",
            Some(&json!({"id": 1, "halfStitch": "yes"}))
        )
        .is_err()
    );
}

/// The longest straight travel (consecutive stitches of 0.5-2.6 mm in one direction) that
/// runs more than 2 mm inside the rectangle (x0, y0)-(x1, y1), mm.
fn straight_travel_inside(s: &Session, rect: [f64; 4]) -> f64 {
    let p = Compiler::for_design(s.design()).compile(s.design());
    let pts: Vec<(f64, f64)> = p
        .stitches
        .iter()
        .filter(|q| q.command == Command::Stitch)
        .map(|q| (q.x / 10.0, q.y / 10.0))
        .collect();
    let inside = |q: (f64, f64)| {
        (q.0 - rect[0])
            .min(rect[2] - q.0)
            .min(q.1 - rect[1])
            .min(rect[3] - q.1)
            > 2.0
    };
    let (mut best, mut run, mut deep) = (0.0_f64, 0.0, false);
    for w in pts.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        let (u, v) = ((b.0 - a.0, b.1 - a.1), (c.0 - b.0, c.1 - b.1));
        let len = |d: (f64, f64)| d.0.hypot(d.1);
        let straight = (u.0 * v.1 - u.1 * v.0).abs() < 0.035 * len(u) * len(v)
            && u.0 * v.0 + u.1 * v.1 > 0.0
            && (0.5..=2.6).contains(&len(u))
            && (0.5..=2.6).contains(&len(v));
        if straight {
            run += len(v);
            deep |= inside(b);
        } else {
            run = len(v);
            deep = false;
        }
        if deep {
            best = best.max(run);
        }
    }
    best
}

#[test]
fn a_fill_entered_away_from_its_rows_travels_along_the_outline_when_asked() {
    // #628: a 12 mm swatch at 0.5 rows/mm entered at the middle of its bottom edge sewed a
    // 13 mm straight travel up to its first row. PE-DESIGN 11.50 travels straight at any
    // density (sweep-fill-density-min), so Inside keeps it; Along outline walks the edge.
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 9, "y": 9, "width": 12, "height": 12, "fill": "Black"}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"ids": [1], "type": "fill", "density": 0.5, "underlay": false, "angle": 0}),
    );
    run(
        &mut s,
        "object.setEntryExit",
        json!({"id": 1, "regionEntry": [15, 21]}),
    );
    let straight = straight_travel_inside(&s, [9.0, 9.0, 21.0, 21.0]);
    assert!(straight > 8.0, "{straight}");
    run(
        &mut s,
        "object.setRegion",
        json!({"ids": [1], "runningStitchPath": "alongOutline"}),
    );
    let along = straight_travel_inside(&s, [9.0, 9.0, 21.0, 21.0]);
    assert!(along < 3.0, "{along}");
}

#[test]
fn a_programmable_fills_under_sewing_does_not_cut_across_the_region() {
    // #628: in kit fills/region-sew-types-1 the programmable fill's under sewing went from
    // the end of its edge run straight across the region to its first row (26 mm, shown
    // through the open diamonds).
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 26, "height": 26, "fill": "Red"}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"ids": [1], "type": "programmableFill", "pattern": "diamonds"}),
    );
    let straight = straight_travel_inside(&s, [10.0, 10.0, 36.0, 36.0]);
    // The pattern's own diagonal lines are up to about 8.5 mm straight.
    assert!(straight < 9.0, "{straight}");
}

#[test]
fn a_moved_node_leaves_no_needle_drop_at_the_old_start_point() {
    // #570, #571 (random-design seeds 150, 398): `object.setPoint` moved an ellipse's bottom
    // node, where its start and end points stay; the fill travelled out to them, 3-4 mm past
    // the reshaped outline.
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 38.5, "cy": 38.7, "rx": 11.3, "ry": 17.2, "fill": "Red"}),
    );
    run(
        &mut s,
        "object.setPoint",
        json!({"id": 1, "index": 0, "x": 75.9, "y": 46.1}),
    );
    let b = s.design().objects[0].bounds().expect("bounds");
    let p = Compiler::for_design(s.design()).compile(s.design());
    let out: Vec<(f64, f64)> = p
        .stitches
        .iter()
        .filter(|q| q.command == Command::Stitch)
        .filter(|q| {
            q.x < b.min_x - 5.0 || q.x > b.max_x + 5.0 || q.y < b.min_y - 5.0 || q.y > b.max_y + 5.0
        })
        .map(|q| (q.x / 10.0, q.y / 10.0))
        .collect();
    assert!(out.is_empty(), "{out:?}");
}

/// #616: PE-DESIGN 11.50's zigzag net (`o-region-zigzag-net.pes`: a 20 mm square drawn with the
/// Rectangle tool, whose region Direction is 45°) sews its square net on the page axes, the
/// lines 8 mm apart from half a band (1.5 mm) inside the left and top edges. OpenStitch's
/// needle points lie on PE-DESIGN's: on average within 0.2 mm both ways, with the shapes'
/// corners aligned (PES stores stitches around the design's centre).
#[cfg(feature = "formats")]
#[test]
fn zigzag_net_sews_on_the_page_axes_as_pe_design_does() {
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 20, "height": 20}),
    );
    run(&mut s, "object.setLine", json!({"id": 1, "type": "none"}));
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "zigzagNetFill"}),
    );
    let angle = s.design().objects[0].region.as_ref().expect("region").angle;
    assert!(
        (angle - 45.0).abs() < 1e-9,
        "the Direction stays 45°: {angle}"
    );
    let points = |stitches: &[openstitch_core::Stitch]| -> Vec<(f64, f64)> {
        let pts: Vec<(f64, f64)> = stitches
            .iter()
            .filter(|st| st.command == Command::Stitch)
            .map(|st| (st.x, st.y))
            .collect();
        let (mx, my) = pts
            .iter()
            .fold((f64::MAX, f64::MAX), |(a, b), p| (a.min(p.0), b.min(p.1)));
        pts.into_iter().map(|(x, y)| (x - mx, y - my)).collect()
    };
    let ours = points(&Compiler::default().compile(s.design()).stitches);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../openstitch-formats/tests/fixtures/pe11/o-region-zigzag-net.pes");
    let pe = points(
        &openstitch_formats::pes::read(&std::fs::read(path).expect("reference"))
            .expect("pes")
            .stitches,
    );
    let mean_nearest = |a: &[(f64, f64)], b: &[(f64, f64)]| {
        a.iter()
            .map(|p| {
                b.iter()
                    .map(|q| (p.0 - q.0).hypot(p.1 - q.1))
                    .fold(f64::MAX, f64::min)
            })
            .sum::<f64>()
            / a.len() as f64
    };
    let (to_pe, from_pe) = (mean_nearest(&ours, &pe), mean_nearest(&pe, &ours));
    assert!(
        to_pe < 2.0 && from_pe < 2.0,
        "mean nearest distance (0.1 mm): ours to PE {to_pe:.2}, PE to ours {from_pe:.2}"
    );
}
