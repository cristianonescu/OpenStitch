//! Ripple lines (#541) and motif line groups, free ends and motifs in turn (#544), through the
//! commands.

use openstitch_core::Command;
use openstitch_design::{Compiler, v2};
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn a_closed_ripple_spirals_in_without_trims_and_its_target_moves() {
    let dir = TempDir::new("ripple");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 50, "rx": 30, "ry": 20}),
    );
    run(&mut s, "object.setRegion", json!({"id": 1, "type": "none"}));
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "ripple", "strands": 6}),
    );
    let p = Compiler::for_design(s.design()).compile(s.design());
    let trims = p
        .stitches
        .iter()
        .filter(|st| st.command == Command::Trim)
        .count();
    let jumps = p
        .stitches
        .iter()
        .filter(|st| st.command == Command::Jump)
        .count();
    assert_eq!(trims, 1, "only the end of the design");
    assert_eq!(jumps, 1, "only the move to the start");
    let before = s.design().clone();
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "target": [40, 45]}),
    );
    assert_eq!(
        s.design().objects[0].line.as_ref().expect("line").target,
        Some(v2(400.0, 450.0))
    );
    let last = Compiler::for_design(s.design())
        .compile(s.design())
        .stitches
        .iter()
        .rev()
        .find(|st| st.command == Command::Stitch)
        .map(|st| v2(st.x, st.y))
        .expect("stitch");
    assert!(
        last.distance(v2(400.0, 450.0)) < 6.0,
        "ends at the target: {last:?}"
    );
    // The target follows a move of the object.
    run(&mut s, "object.move", json!({"ids": [1], "dx": 5, "dy": 0}));
    assert_eq!(
        s.design().objects[0].line.as_ref().expect("line").target,
        Some(v2(450.0, 450.0))
    );
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &before, "one undo step per change");
}

#[test]
fn motif_groups_free_ends_and_motifs_in_turn_through_the_command() {
    let dir = TempDir::new("motif-plan");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "shape.path",
        json!({"points": [[10, 30], [90, 30]], "closed": false}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "motif", "motif": "star", "width": 4, "height": 4}),
    );
    let plain = s.design().clone();
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "motifGroups": [[3, 6]], "freeStart": 5, "freeEnd": 5, "motifs": ["arrow"]}),
    );
    let l = s.design().objects[0].line.clone().expect("line");
    assert_eq!(l.motif_groups, vec![(3, 6.0)]);
    assert_eq!((l.free_start, l.free_end), (Some(5.0), Some(5.0)));
    assert_eq!(l.motifs, vec!["arrow".to_string()]);
    // The editor's forms: one group by size and gap, one motif in turn, "none".
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "groupSize": 2, "groupGap": 4}),
    );
    assert_eq!(
        s.design().objects[0]
            .line
            .as_ref()
            .expect("line")
            .motif_groups,
        vec![(2, 4.0)]
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "motifs": "none", "groupSize": 0}),
    );
    let l = s.design().objects[0].line.clone().expect("line");
    assert!(l.motifs.is_empty() && l.motif_groups.is_empty());
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "freeStart": null, "freeEnd": null}),
    );
    assert_eq!(s.design(), &plain, "back to PE-DESIGN's motif line");
    let before = s.design().clone();
    for params in [
        json!({"id": 1, "motifs": ["no-such-motif"]}),
        json!({"id": 1, "motifGroups": [[0, 5]]}),
        json!({"id": 1, "motifGroups": [[3, 60]]}),
        json!({"id": 1, "freeStart": 200}),
        json!({"id": 1, "target": [1, 2]}),
        json!({"id": 1, "type": "running", "motifGroups": [[2, 2]]}),
        json!({"id": 1, "type": "ripple", "strands": 1}),
        json!({"id": 1, "type": "ripple", "target": [1]}),
    ] {
        assert!(
            s.execute("object.setLine", Some(&params)).is_err(),
            "{params}"
        );
        assert_eq!(s.design(), &before, "{params}");
    }
}
