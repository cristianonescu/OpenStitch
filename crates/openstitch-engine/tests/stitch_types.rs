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
    // Beyond PE-DESIGN (#685): the mode and a seed, kept when the width changes; null restores
    // PE-DESIGN's pattern, and the default mode is not saved.
    let pe_feather = Compiler::default().compile(s.design());
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "featherMode": "out", "featherSeed": 5}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "featherWidth": 2}),
    );
    let f = s.design().objects[0]
        .region
        .clone()
        .expect("r")
        .feather
        .expect("f");
    assert_eq!(
        (f.side, f.width, f.mode, f.seed),
        (
            FeatherSide::Both,
            2.0,
            openstitch_design::FeatherMode::Out,
            Some(5)
        )
    );
    // Saved and read back with the project.
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    assert!(
        s.execute(
            "object.setRegion",
            Some(&json!({"id": 1, "featherMode": "sideways"}))
        )
        .is_err()
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "featherMode": "in", "featherSeed": null, "featherWidth": 1.5}),
    );
    assert_eq!(Compiler::default().compile(s.design()), pe_feather);
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
    // The four edits above and the three feather-mode ones (#685), one undo step each.
    for _ in 0..7 {
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
            steps: Some(4),
            shape: Default::default(),
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
            steps: Some(9),
            shape: Default::default(),
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
        ..Default::default()
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
        json!({"id": 1, "texture": "plaid"}),
        json!({"id": 1, "stagger": "spiral"}),
        json!({"id": 1, "staggerSteps": 0}),
        json!({"id": 1, "staggerSteps": 2.5}),
        json!({"id": 1, "staggerRepeat": 17}),
        json!({"id": 1, "staggerPeriod": 1}),
        json!({"id": 1, "staggerAmplitude": 101}),
        json!({"id": 1, "gradient": true}),
        json!({"id": 1, "gradationShape": "spiky"}),
        json!({"id": 1, "gradientDither": "rows"}),
        json!({"id": 1, "type": "meander", "pattern": "waves"}),
        json!({"id": 1, "type": "meander", "pattern": "no-such-tile"}),
        json!({"id": 1, "straightness": -1}),
        json!({"id": 1, "straightness": 101}),
        json!({"id": 1, "mazeSeed": -1}),
        json!({"id": 1, "mazeSeed": 2.5}),
        json!({"id": 1, "mazeSeed": 1000000}),
        json!({"id": 1, "waveHeight": -1}),
        json!({"id": 1, "waveHeight": 21}),
        json!({"id": 1, "waveLength": 1}),
        json!({"id": 1, "waveLength": 101}),
        json!({"id": 1, "waveLength": "long"}),
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

/// Cross stitch Times (PE-DESIGN's Single, Double, Triple) and the cross kinds beyond PE-DESIGN
/// (#614) through `object.setRegion`: each sews, Times adds 4 stitches per cell per step, the
/// old `triple` switch is Times Double, and the settings are saved and undone.
#[test]
fn cross_stitch_times_and_kinds_go_through_the_command() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 40, "y": 40, "width": 20, "height": 20}),
    );
    let stitches = |s: &Session| {
        Compiler::default()
            .compile(s.design())
            .count(Command::Stitch)
    };
    let region = |s: &Session| s.design().objects[0].region.clone().expect("region");
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "crossStitch"}),
    );
    let single = stitches(&s);
    for (times, step) in [("double", 1), ("triple", 2), ("single", 0)] {
        run(&mut s, "object.setRegion", json!({"id": 1, "times": times}));
        assert_eq!(region(&s).cross_times().id(), times);
        // 64 cells on a 20 mm square at the default 2.5 mm, as PE-DESIGN's references.
        assert_eq!(stitches(&s), single + step * 4 * 64, "{times}");
    }
    run(&mut s, "object.setRegion", json!({"id": 1, "triple": true}));
    assert_eq!(region(&s).cross_times().id(), "double");
    assert_eq!(stitches(&s), single + 4 * 64);
    for kind in ["half", "upright", "smyrna", "full"] {
        run(
            &mut s,
            "object.setRegion",
            json!({"id": 1, "crossKind": kind, "times": "triple"}),
        );
        assert_eq!(region(&s).cross_kind.id(), kind);
        assert!(stitches(&s) > 200, "{kind}");
    }
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "crossKind": "smyrna"}),
    );
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(&mut s, "history.undo", json!({}));
    assert_eq!(region(&s).cross_kind.id(), "full");
    // The help names the options and says cross stitch ignores the direction.
    let help = run(&mut s, "commands.list", json!({}))
        .as_array()
        .expect("array")
        .iter()
        .find(|c| c["id"] == "object.setRegion")
        .expect("object.setRegion")
        .to_string();
    for word in [
        "times",
        "single|double|triple",
        "crossKind",
        "smyrna",
        "ignored by cross stitch",
    ] {
        assert!(help.contains(word), "{word}");
    }
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
        json!({"id": 1, "times": "quadruple"}),
        json!({"id": 1, "times": 2}),
        json!({"id": 1, "crossKind": "star"}),
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

#[test]
fn tatami_textures_go_through_the_command() {
    use openstitch_design::{FILL_TEXTURES, StaggerKind};
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 50, "height": 50, "fill": "Red"}),
    );
    let plain = Compiler::default().compile(s.design());
    let count = plain.count(Command::Stitch) as f64;
    for t in &FILL_TEXTURES {
        run(
            &mut s,
            "object.setRegion",
            json!({"id": 1, "texture": t.id}),
        );
        let r = s.design().objects[0].region.clone().expect("region");
        assert_eq!(r.texture().map(|x| x.id), Some(t.id));
        let n = Compiler::default()
            .compile(s.design())
            .count(Command::Stitch) as f64;
        assert!(
            (n - count).abs() / count <= 0.03,
            "{}: {n} vs {count}",
            t.id
        );
    }
    // A texture, then its settings changed in the same call: a custom texture.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "texture": "waves", "staggerPeriod": 20, "staggerAmplitude": 40}),
    );
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!(
        (r.stagger, r.stagger_period, r.stagger_amplitude),
        (StaggerKind::Wave, Some(20), Some(40.0))
    );
    assert!(r.texture().is_none());
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "stagger": "steps", "staggerSteps": 5, "staggerRepeat": 2}),
    );
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!(
        (r.stagger, r.stagger_steps, r.stagger_repeat),
        (StaggerKind::Steps, Some(5), Some(2))
    );
    // Saved and opened again, and one undo step back.
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(&mut s, "history.undo", json!({}));
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!(r.stagger, StaggerKind::Wave);
    // The help lists the textures.
    let list = run(&mut s, "commands.list", json!({}));
    let text = list.to_string();
    for t in &FILL_TEXTURES {
        assert!(text.contains(t.id), "help names {}", t.id);
    }
}

/// Wave fill (#538, beyond PE-DESIGN): set by `object.setRegion`, sewn as rippling rows, saved,
/// undone in one step, turned off by height 0, and listed in the command's help.
#[test]
fn wave_fill_goes_through_the_command() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 40, "height": 30}),
    );
    run(&mut s, "object.setRegion", json!({"id": 1, "type": "fill"}));
    let plain = Compiler::default().compile(s.design());
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "waveHeight": 4, "waveLength": 24}),
    );
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!((r.wave_height, r.wave_length), (Some(4.0), Some(24.0)));
    assert_eq!(r.wave(), Some((2.0, 24.0)));
    let wavy = Compiler::default().compile(s.design());
    assert_ne!(plain, wavy, "the rows ripple");
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(&mut s, "history.undo", json!({}));
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!((r.wave_height, r.wave_length), (None, None));
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "waveHeight": 4}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "waveHeight": 0}),
    );
    assert!(
        s.design().objects[0]
            .region
            .as_ref()
            .expect("r")
            .wave()
            .is_none()
    );
    assert_eq!(Compiler::default().compile(s.design()), plain);
    let help = run(&mut s, "commands.list", json!({})).to_string();
    assert!(help.contains("waveHeight") && help.contains("waveLength"));
}

/// #373: a satin line round the same V is cut as the zigzag line of its width, through the
/// same `sharpCorners` and `miterLimit`; with a corner style the style decides.
#[test]
fn satin_line_sharp_corners_and_miter_limit_go_through_object_set_line() {
    let sewn = |kind: &str, extra: serde_json::Value| {
        let mut s = Session::new(std::env::temp_dir());
        run(
            &mut s,
            "shape.polygon",
            json!({"points": [[10, 20], [50, 20], [15, 40]], "closed": false}),
        );
        run(
            &mut s,
            "object.setLine",
            json!({"id": 1, "type": kind, "width": 2}),
        );
        let mut p = extra;
        p["id"] = json!(1);
        run(&mut s, "object.setLine", p);
        Compiler::for_design(s.design())
            .compile(s.design())
            .stitches
            .iter()
            .filter(|q| q.command == Command::Stitch)
            .map(|q| (q.x, q.y))
            .collect::<Vec<_>>()
    };
    for extra in [
        json!({}),
        json!({"miterLimit": 3.0}),
        json!({"sharpCorners": 2, "miterLimit": 1.0}),
    ] {
        let z = sewn("zigzag", {
            let mut e = extra.clone();
            e["underSewing"] = json!(false);
            e
        });
        let satin = sewn("satin", {
            let mut e = extra.clone();
            e["underSewing"] = json!(false);
            e
        });
        assert_eq!(satin, z, "{extra}");
    }
    let mitre = sewn(
        "satin",
        json!({"underSewing": false, "cornerStyle": "mitre"}),
    );
    let cut = sewn("satin", json!({"underSewing": false}));
    assert_ne!(mitre, cut, "a corner style decides");
}

#[test]
fn zigzag_sharp_corners_and_miter_limit_go_through_object_set_line() {
    // compat line-zigzag-miter-v-*: the 29.7-degree V of PE-DESIGN's #373 captures (#373).
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "shape.polygon",
        json!({"points": [[10, 20], [50, 20], [15, 40]], "closed": false}),
    );
    run(&mut s, "object.setLine", json!({"id": 1, "type": "zigzag"}));
    let corners = |s: &Session| {
        s.design().objects[0]
            .line
            .as_ref()
            .expect("line")
            .sharp_corners
    };
    // How far right the corner's point reaches, mm (the corner is at x = 50).
    let reach = |s: &Session| {
        Compiler::for_design(s.design())
            .compile(s.design())
            .stitches
            .iter()
            .filter(|q| q.command == Command::Stitch)
            .map(|q| q.x / 10.0)
            .fold(f64::MIN, f64::max)
    };
    // PE-DESIGN's defaults: button 1 with 0.0 mm cuts the point at the corner (40.2 mm wide).
    assert_eq!((corners(&s).button, corners(&s).limit()), (1, 0.0));
    let cut = reach(&s);
    assert!(cut < 50.4, "cut at the corner: {cut}");
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "miterLimit": 3.0}),
    );
    assert!((reach(&s) - 53.0).abs() < 0.15, "{}", reach(&s));
    // Button 2 recalls its own value (7.0 mm: past the full point, 43.8 mm wide).
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "sharpCorners": 2}),
    );
    assert_eq!(corners(&s).limit(), 7.0);
    assert!((reach(&s) - 53.8).abs() < 0.15, "{}", reach(&s));
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "sharpCorners": "1"}),
    );
    assert_eq!(corners(&s).limit(), 3.0, "button 1 kept its 3.0 mm");
    // Saved, read back and undone.
    let saved = serde_json::to_string(s.design()).expect("json");
    assert!(saved.contains("\"sharpCorners\""), "{saved}");
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(&mut s, "history.undo", json!({}));
    assert_eq!(corners(&s).button, 2);
    // Out of range or unknown: refused, the design unchanged.
    let before = s.design().clone();
    for bad in [
        json!({"id": 1, "sharpCorners": 3}),
        json!({"id": 1, "sharpCorners": "wedge"}),
        json!({"id": 1, "miterLimit": 51}),
        json!({"id": 1, "miterLimit": -1}),
    ] {
        assert!(s.execute("object.setLine", Some(&bad)).is_err(), "{bad}");
        assert_eq!(s.design(), &before, "{bad} changed the design");
    }
}

/// Maze fill (#533, beyond PE-DESIGN): set by `object.setRegion`, sewn as one path, reseeded,
/// saved, undone in one step, and listed in the command's help.
#[test]
fn maze_fill_goes_through_the_command() {
    use openstitch_design::RegionType;
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 30, "height": 30}),
    );
    run(&mut s, "object.setRegion", json!({"id": 1, "type": "maze"}));
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!(r.kind, RegionType::Maze);
    let first = Compiler::default().compile(s.design());
    assert!(first.count(Command::Stitch) > 100);
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "straightness": 40, "mazeSeed": 9, "spacing": 2.5}),
    );
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!(
        (r.straightness, r.maze_seed, r.spacing),
        (Some(40.0), Some(9), Some(2.5))
    );
    assert_ne!(Compiler::default().compile(s.design()), first);
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(&mut s, "history.undo", json!({}));
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!((r.straightness, r.maze_seed), (None, None));
    assert_eq!(Compiler::default().compile(s.design()), first);
    let help = run(&mut s, "commands.list", json!({})).to_string();
    assert!(help.contains("mazeSeed") && help.contains("straightness") && help.contains("maze|"));
}

/// Meander fill (#533, beyond PE-DESIGN): its 12 own tiles are listed, each can be set and
/// sews; a user tile of kind meander is checked for crossings; saved and undone.
#[test]
fn meander_fill_goes_through_the_command() {
    use openstitch_design::RegionType;
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 30, "height": 30}),
    );
    let list = run(&mut s, "patterns.list", json!({}));
    let tiles: Vec<String> = list
        .as_array()
        .or_else(|| list["patterns"].as_array())
        .expect("list")
        .iter()
        .filter(|p| p["kind"] == "meander")
        .map(|p| p["id"].as_str().expect("id").to_string())
        .collect();
    assert!(tiles.len() >= 12, "{tiles:?}");
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "meander"}),
    );
    assert_eq!(
        s.design().objects[0].region.as_ref().expect("r").kind,
        RegionType::Meander
    );
    let first = Compiler::default().compile(s.design());
    assert!(first.count(Command::Stitch) > 100);
    for id in &tiles {
        run(&mut s, "object.setRegion", json!({"id": 1, "pattern": id}));
        let p = Compiler::default().compile(s.design());
        assert!(p.count(Command::Stitch) > 50, "{id}");
    }
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(&mut s, "history.undo", json!({}));
    // A user meander tile: refused when it crosses itself, kept when it does not.
    let tile = |paths: Value| {
        json!({"pattern": {"format": "openstitch.pattern", "version": 1, "kind": "meander",
            "id": "my-meander", "name": "Mine", "paths": paths}})
    };
    assert!(
        s.execute(
            "pattern.define",
            Some(&tile(json!([[
                [0, 0.5],
                [0.5, 0.2],
                [0.5, 0.8],
                [0.2, 0.4],
                [1, 0.5]
            ]])))
        )
        .is_err()
    );
    run(
        &mut s,
        "pattern.define",
        tile(json!([[[0, 0.3], [0.5, 0.7], [1, 0.3]]])),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "pattern": "my-meander"}),
    );
    let help = run(&mut s, "commands.list", json!({})).to_string();
    assert!(help.contains("meander|"));
}

/// Gradient fill and colour blend (#532, beyond PE-DESIGN): one fill object per colour,
/// grouped and next to each other, without under sewing; undone in one step; the dither set
/// on the group; bad input refused.
#[test]
fn gradient_and_blend_go_through_the_commands() {
    use openstitch_design::{GradientCurve, GradientDither, GradientKind};
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 40, "height": 30, "fill": "Red"}),
    );
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 50, "y": 0, "width": 20, "height": 20, "fill": "Blue"}),
    );
    let before = s.design().clone();
    let out = run(
        &mut s,
        "shape.gradient",
        json!({"id": 1, "threads": ["Red", "#ffcc00", 5], "angle": 90, "dither": "ordered"}),
    );
    let ids: Vec<u64> = out["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .map(|v| v.as_u64().expect("id"))
        .collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(ids[0], 1);
    // Next to each other in the sewing order, before the second rectangle, one group.
    let order: Vec<u64> = s.design().objects.iter().map(|o| o.id).collect();
    assert_eq!(&order[..3], &ids[..]);
    let group = s.design().objects[0].group;
    assert!(group.is_some());
    for (k, o) in s.design().objects[..3].iter().enumerate() {
        let r = o.region.as_ref().expect("region");
        assert_eq!(o.group, group);
        assert!(!r.underlay);
        let g = r.gradient.as_ref().expect("gradient");
        assert_eq!((g.layer, g.stops.clone()), (k, vec![0.0, 0.5, 1.0]));
        assert_eq!(g.dither, GradientDither::Ordered);
    }
    let p = Compiler::default().compile(s.design());
    assert!(p.count(Command::Stitch) > 300);
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    // The dither and curve are set on the gradient's objects together.
    run(
        &mut s,
        "object.setRegion",
        json!({"ids": ids, "gradientDither": "noise", "gradientCurve": "sCurve"}),
    );
    assert!(s.design().objects[..3].iter().all(|o| {
        let g = o
            .region
            .as_ref()
            .and_then(|r| r.gradient.as_ref())
            .expect("g");
        g.dither == GradientDither::Noise && g.curve == GradientCurve::SCurve
    }));
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &before, "one undo step each");
    // A blend: the shape's thread into thread2, with the mix handles.
    let out = run(
        &mut s,
        "shape.blend",
        json!({"id": 2, "thread2": "White", "mix": [0.2, 0.8], "kind": "radial"}),
    );
    assert_eq!(out["ids"].as_array().map(Vec::len), Some(2));
    let blend = |s: &Session| {
        s.design()
            .objects
            .iter()
            .find(|o| o.id == 2)
            .and_then(|o| o.region.clone())
            .expect("region")
    };
    let g = blend(&s).gradient.expect("g");
    assert_eq!(
        (g.stops.clone(), g.kind),
        (vec![0.2, 0.8], GradientKind::Radial)
    );
    // Back to a plain fill.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 2, "gradient": false}),
    );
    assert!(blend(&s).gradient.is_none());
    let mid = s.design().clone();
    for (cmd, bad) in [
        ("shape.gradient", json!({"id": 1, "threads": ["Red"]})),
        (
            "shape.gradient",
            json!({"id": 1, "threads": ["Red", "nope"]}),
        ),
        (
            "shape.gradient",
            json!({"id": 1, "threads": ["Red", "Blue"], "stops": [0.6, 0.2]}),
        ),
        (
            "shape.gradient",
            json!({"id": 1, "threads": ["Red", "Blue"], "dither": "fuzzy"}),
        ),
        (
            "shape.gradient",
            json!({"id": 1, "threads": ["Red", "Blue"], "from": [0, 0]}),
        ),
        (
            "shape.gradient",
            json!({"id": 99, "threads": ["Red", "Blue"]}),
        ),
        ("shape.blend", json!({"id": 1})),
        (
            "shape.blend",
            json!({"id": 1, "thread2": "Blue", "mix": [0.5]}),
        ),
    ] {
        assert!(
            s.execute(cmd, Some(&bad)).is_err(),
            "{cmd} {bad} should fail"
        );
        assert_eq!(s.design(), &mid, "{cmd} {bad} changed the design");
    }
    let help = run(&mut s, "commands.list", json!({})).to_string();
    assert!(help.contains("shape.gradient") && help.contains("shape.blend"));
    assert!(help.contains("gradientDither"));
}

/// Named gradation shapes (#532, beyond PE-DESIGN): set by `object.setRegion`, sewn, saved,
/// undone; PES 0110 refuses a shape other than PE-DESIGN's linear one.
#[test]
fn gradation_shapes_go_through_the_command() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 40, "height": 30, "fill": "Red"}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "gradationPattern": true}),
    );
    let linear = Compiler::default().compile(s.design());
    let shape = |s: &Session| {
        s.design().objects[0]
            .region
            .as_ref()
            .and_then(|r| r.gradation)
            .map(|g| g.shape)
    };
    for id in ["convex", "concave", "centre", "edges", "wave"] {
        run(
            &mut s,
            "object.setRegion",
            json!({"id": 1, "gradationShape": id}),
        );
        assert_eq!(shape(&s).map(|g| g.id()), Some(id));
        assert_ne!(Compiler::default().compile(s.design()), linear, "{id}");
    }
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(&mut s, "history.undo", json!({}));
    assert_eq!(shape(&s).map(|g| g.id()), Some("edges"));
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "gradationShape": "linear"}),
    );
    assert_eq!(Compiler::default().compile(s.design()), linear);
    let help = run(&mut s, "commands.list", json!({})).to_string();
    assert!(help.contains("gradationShape"));
}

/// The Gradient tool's command (#532): `gradientFrom` / `gradientTo` move a gradient's start
/// and end on every object given, in one undo step; one without the other is refused.
#[test]
fn gradient_start_and_end_move_with_the_command() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 40, "height": 20, "fill": "Blue"}),
    );
    let out = run(
        &mut s,
        "shape.gradient",
        json!({"id": 1, "threads": ["Blue", "White"]}),
    );
    let ids = out["ids"].clone();
    let before = s.design().clone();
    let first = Compiler::default().compile(s.design());
    run(
        &mut s,
        "object.setRegion",
        json!({"ids": ids, "gradientFrom": [20, 0], "gradientTo": [20, 20]}),
    );
    for o in &s.design().objects {
        let g = o
            .region
            .as_ref()
            .and_then(|r| r.gradient.as_ref())
            .expect("g");
        assert_eq!(
            (g.from.x, g.from.y, g.to.x, g.to.y),
            (200.0, 0.0, 200.0, 200.0)
        );
    }
    assert_ne!(
        Compiler::default().compile(s.design()),
        first,
        "the colours turn"
    );
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &before);
    let mid = s.design().clone();
    for bad in [
        json!({"ids": ids, "gradientFrom": [20, 0]}),
        json!({"ids": ids, "gradientFrom": [20, 0], "gradientTo": [20, 0]}),
        json!({"ids": ids, "gradientFrom": "top", "gradientTo": [20, 20]}),
    ] {
        assert!(s.execute("object.setRegion", Some(&bad)).is_err(), "{bad}");
        assert_eq!(s.design(), &mid);
    }
}

/// Stitch angle lines (#534, beyond PE-DESIGN): set by `object.setAngleLines`, sewn, saved,
/// undone; crossing lines refused; `[]` and `object.setRegion angleLines: false` clear them.
#[test]
fn angle_lines_go_through_the_command() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    // An L: a horizontal bar and a vertical bar (x, y in mm).
    run(
        &mut s,
        "shape.polygon",
        json!({"points": [[0, 0], [60, 0], [60, 20], [20, 20], [20, 60], [0, 60]], "closed": true, "fill": "Red"}),
    );
    let plain = Compiler::default().compile(s.design());
    let lines = json!([
        [[40, -2], [40, 22]],
        [[22, 22], [-2, -2]],
        [[-2, 40], [22, 40]]
    ]);
    let out = run(
        &mut s,
        "object.setAngleLines",
        json!({"id": 1, "lines": lines}),
    );
    assert_eq!(out["lines"], 3);
    let r = s.design().objects[0].region.clone().expect("r");
    assert_eq!(r.angle_lines.len(), 3);
    assert_eq!(r.angle_lines[0][0], openstitch_design::v2(400.0, -20.0));
    let turned = Compiler::default().compile(s.design());
    assert_ne!(turned, plain);
    assert!(turned.count(Command::Stitch) > 200);
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(&mut s, "history.undo", json!({}));
    assert!(
        s.design().objects[0]
            .region
            .as_ref()
            .expect("r")
            .angle_lines
            .is_empty()
    );
    let mid = s.design().clone();
    for bad in [
        json!({"id": 1, "lines": [[[40, -2], [40, 22]]]}),
        json!({"id": 1, "lines": [[[40, -2], [40, 22]], [[30, -2], [50, 22]]]}),
        json!({"id": 1, "lines": [[[40, 30], [50, 50]], [[40, -2], [40, 22]]]}),
        json!({"id": 1, "lines": [[[40, -2]]]}),
        json!({"id": 99, "lines": []}),
    ] {
        assert!(
            s.execute("object.setAngleLines", Some(&bad)).is_err(),
            "{bad}"
        );
        assert_eq!(s.design(), &mid, "{bad}");
    }
    run(
        &mut s,
        "object.setAngleLines",
        json!({"id": 1, "lines": lines}),
    );
    run(
        &mut s,
        "object.setAngleLines",
        json!({"id": 1, "lines": []}),
    );
    assert!(
        s.design().objects[0]
            .region
            .as_ref()
            .expect("r")
            .angle_lines
            .is_empty()
    );
    run(
        &mut s,
        "object.setAngleLines",
        json!({"id": 1, "lines": lines}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "angleLines": false}),
    );
    assert_eq!(Compiler::default().compile(s.design()), plain);
    let help = run(&mut s, "commands.list", json!({})).to_string();
    assert!(help.contains("object.setAngleLines") && help.contains("angleLines"));
}

/// #826: `object.setRegion {angle}` is a straight direction, so it turns Auto direction off,
/// unless `autoDirection` is given in the same call.
#[test]
fn setting_the_angle_turns_auto_direction_off() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 40, "height": 20, "fill": "Red"}),
    );
    let region = |s: &Session| s.design().objects[0].region.clone().expect("r");
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "autoDirection": true}),
    );
    assert!(region(&s).auto_direction);
    let auto = Compiler::default().compile(s.design());
    run(&mut s, "object.setRegion", json!({"id": 1, "angle": 30}));
    let r = region(&s);
    assert!(!r.auto_direction && r.angle == 30.0, "{r:?}");
    assert_ne!(
        Compiler::default().compile(s.design()),
        auto,
        "the rows turn to 30°"
    );
    // Both in one call: as given.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "angle": 60, "autoDirection": true}),
    );
    let r = region(&s);
    assert!(r.auto_direction && r.angle == 60.0);
    // Other options leave Auto direction as it is.
    run(&mut s, "object.setRegion", json!({"id": 1, "density": 4.0}));
    assert!(region(&s).auto_direction);
}

/// #677: push compensation and a second under sewing layer on a fill go through
/// `object.setRegion`, are saved and undone; bad values are refused.
#[test]
fn push_compensation_and_a_second_underlay_go_through_the_command() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 40, "height": 20, "fill": "Red"}),
    );
    let plain = Compiler::default().compile(s.design());
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "pushCompensation": 0.8, "underlay2": {"style": "zigzag", "angle": 30, "spacing": 3}}),
    );
    let r = s.design().objects[0].region.clone().expect("r");
    assert_eq!(r.push_compensation, Some(0.8));
    let u = r.underlay2.expect("u2");
    assert_eq!((u.angle, u.spacing), (30.0, 3.0));
    assert_ne!(Compiler::default().compile(s.design()), plain);
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(&mut s, "history.undo", json!({}));
    assert_eq!(Compiler::default().compile(s.design()), plain);
    let mid = s.design().clone();
    for bad in [
        json!({"id": 1, "pushCompensation": 3}),
        json!({"id": 1, "pushCompensation": -1}),
        json!({"id": 1, "underlay2": {"angle": 200}}),
    ] {
        assert!(s.execute("object.setRegion", Some(&bad)).is_err(), "{bad}");
        assert_eq!(s.design(), &mid);
    }
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "pushCompensation": 0.5}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "pushCompensation": 0}),
    );
    assert_eq!(
        s.design().objects[0]
            .region
            .as_ref()
            .expect("r")
            .push_compensation,
        None
    );
    let help = run(&mut s, "commands.list", json!({})).to_string();
    assert!(help.contains("pushCompensation") && help.contains("crossed tatami"));
}

/// Tartan fill (#539, beyond PE-DESIGN): `shape.tartan` with the usual notation or a list
/// makes one grouped fill per colour, one colour block each; saved, undone, refused cleanly.
#[test]
fn tartan_goes_through_the_command() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 60, "height": 60, "fill": "Red"}),
    );
    let before = s.design().clone();
    let out = run(
        &mut s,
        "shape.tartan",
        json!({"id": 1, "sett": "R8 G24 W4"}),
    );
    assert_eq!(out["colors"], 3);
    let ids: Vec<u64> = out["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .map(|v| v.as_u64().expect("id"))
        .collect();
    assert_eq!(ids.len(), 3);
    let d = s.design();
    assert!(
        d.objects[..3]
            .iter()
            .all(|o| o.group == d.objects[0].group && o.group.is_some())
    );
    for (k, o) in d.objects[..3].iter().enumerate() {
        let r = o.region.as_ref().expect("r");
        let t = r.tartan.as_ref().expect("tartan");
        assert_eq!(
            (t.layer, t.sett.len(), t.symmetric, r.underlay),
            (k, 3, true, false)
        );
    }
    let p = Compiler::default().compile(s.design());
    assert!(p.count(Command::Stitch) > 1000);
    assert_eq!(
        p.count(Command::ColorChange),
        2,
        "one colour block per thread"
    );
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &before);
    // The list form; a colour used twice is one colour.
    let out = run(
        &mut s,
        "shape.tartan",
        json!({"id": 1, "symmetric": false, "sett": [
            {"color": "Black", "width": 6}, {"color": "#ffcc00", "width": 2},
            {"color": "Black", "width": 6}, {"color": 5, "width": 10}]}),
    );
    assert_eq!(out["colors"], 3);
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "tartan": false}),
    );
    assert!(
        s.design().objects[0]
            .region
            .as_ref()
            .expect("r")
            .tartan
            .is_none()
    );
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.undo", json!({}));
    let mid = s.design().clone();
    for bad in [
        json!({"id": 1, "sett": "R8"}),
        json!({"id": 1, "sett": "R8 Q4"}),
        json!({"id": 1, "sett": "R8 G0.1"}),
        json!({"id": 1, "sett": "R8 R4"}),
        json!({"id": 1, "sett": [{"color": "Red"}]}),
        json!({"id": 1, "sett": 7}),
        json!({"id": 99, "sett": "R8 G4"}),
    ] {
        assert!(s.execute("shape.tartan", Some(&bad)).is_err(), "{bad}");
        assert_eq!(s.design(), &mid, "{bad}");
    }
    let help = run(&mut s, "commands.list", json!({})).to_string();
    assert!(help.contains("shape.tartan"));
}

/// #538: a flow fill through `object.setRegion {flowField}`: radial or swirl round the object's
/// own centre, or a given field; it is kept in the project, undone in one step, cleared back to
/// the plain fill, refused on non-fill regions and by PES 0110.
#[test]
fn flow_fill_goes_through_the_command() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 30, "cy": 30, "rx": 20, "ry": 20}),
    );
    run(&mut s, "object.setRegion", json!({"id": 1, "type": "fill"}));
    let plain = Compiler::default().compile(s.design());
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "flowField": "swirl"}),
    );
    let r = s.design().objects[0].region.clone().expect("region");
    match r.flow_field {
        Some(openstitch_design::flow::FlowField::Swirl { center }) => {
            assert!(
                (center.x - 300.0).abs() < 1.0 && (center.y - 300.0).abs() < 1.0,
                "{center:?}"
            );
        }
        other => panic!("{other:?}"),
    }
    let swirl = Compiler::default().compile(s.design());
    assert_ne!(plain, swirl, "the rows circle");
    assert!(swirl.count(Command::Stitch) > 500);
    let got = run(&mut s, "object.get", json!({"id": 1}));
    assert_eq!(
        got["region"]["flowField"]["kind"], "swirl",
        "{}",
        got["region"]
    );
    let bytes = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&bytes).expect("decode"), s.design());
    // PES 0110 keeps editable objects: PE-DESIGN would sew straight rows, so it is refused.
    let e = s
        .execute(
            "design.export",
            Some(&json!({"path": "flow.pes", "pesVersion": "11"})),
        )
        .expect_err("not mapped");
    assert!(e.to_string().contains("flow fill"), "{e}");
    // One undo step back to the plain fill; straight clears it.
    run(&mut s, "history.undo", json!({}));
    assert!(
        s.design().objects[0]
            .region
            .as_ref()
            .expect("r")
            .flow_field
            .is_none()
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "flowField": {"kind": "radial", "center": [30, 30]}}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "flowField": "straight"}),
    );
    assert_eq!(Compiler::default().compile(s.design()), plain);
    // A grid field, and what is refused.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "flowField": {"kind": "grid", "origin": [10, 10], "cell": 10, "cols": 4,
                                     "angles": [0, 30, 60, 90, 0, 30, 60, 90, 0, 30, 60, 90, 0, 30, 60, 90]}}),
    );
    assert!(matches!(
        s.design().objects[0].region.as_ref().expect("r").flow_field,
        Some(openstitch_design::flow::FlowField::Grid { cols: 4, .. })
    ));
    for bad in [
        json!("spiral"),
        json!({"kind": "radial"}),
        json!({"kind": "grid", "origin": [0, 0], "cell": 10, "cols": 3, "angles": [0, 1]}),
    ] {
        assert!(
            s.execute(
                "object.setRegion",
                Some(&json!({"id": 1, "flowField": bad.clone()}))
            )
            .is_err(),
            "{bad}"
        );
    }
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "satin"}),
    );
    assert!(
        s.execute(
            "object.setRegion",
            Some(&json!({"id": 1, "flowField": "radial"}))
        )
        .is_err(),
        "only fills"
    );
}
