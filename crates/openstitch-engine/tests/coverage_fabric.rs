//! Coverage for the Fabric Selector claims `fabric_selector_sets_recommended_attributes` leaves
//! out (#332): eight fabrics, and programmable fill and stitch length set as well.

use openstitch_engine::Session;
use openstitch_engine::fabric::FABRICS;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn eight_fabrics_are_offered_with_their_values() {
    let mut s = Session::new(std::env::temp_dir());
    let list = run(&mut s, "fabric.list", json!({}));
    let fabrics = list.as_array().expect("list");
    assert_eq!(fabrics.len(), 8, "the eight types FEATURE_MATRIX states");
    assert_eq!(fabrics.len(), FABRICS.len());
    let mut ids: Vec<&str> = fabrics.iter().filter_map(|f| f["id"].as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 8, "distinct ids");
    for (f, def) in fabrics.iter().zip(FABRICS) {
        assert_eq!(f["stitchLength"], json!(def.stitch_length), "{}", def.id);
        assert!(
            def.fill_density > 0.0 && def.satin_density > 0.0,
            "{}",
            def.id
        );
    }
}

#[test]
fn every_fabric_sets_programmable_fill_density_and_fill_stitch_length() {
    for def in FABRICS {
        let mut s = Session::new(std::env::temp_dir());
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": 10, "y": 10, "width": 30, "height": 20}),
        );
        run(
            &mut s,
            "object.setRegion",
            json!({"id": 1, "type": "programmableFill"}),
        );
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": 50, "y": 10, "width": 30, "height": 20}),
        );
        run(&mut s, "object.setRegion", json!({"id": 2, "type": "fill"}));
        run(&mut s, "fabric.apply", json!({"fabric": def.id}));
        let d = s.design();
        let prog = d.objects[0].region.as_ref().expect("programmable fill");
        assert_eq!(prog.density, def.fill_density, "{}", def.id);
        let fill = d.objects[1].region.as_ref().expect("fill");
        assert_eq!(fill.density, def.fill_density, "{}", def.id);
        assert_eq!(fill.stitch_length, def.stitch_length, "{}", def.id);
        assert_eq!(fill.pull_compensation, def.pull_compensation, "{}", def.id);
        assert_eq!(fill.underlay, def.underlay.is_some(), "{}", def.id);
    }
}
