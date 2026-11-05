//! `background.setLayer` (#518): where the background image is drawn is stored in `.osd`, is
//! one undo step, defaults to behind the design, and never changes a machine file.

use openstitch_digitize::Raster;
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn the_layer_is_saved_undone_and_leaves_exports_alone() {
    let dir = TempDir::new("bg-layer");
    let img = Raster::from_fn(20, 10, |x, _| [x as u8, 0, 0, 255]).expect("raster");
    std::fs::write(dir.join("ref.png"), img.encode_png().expect("png")).expect("write");
    let mut s = Session::new(dir.path());
    for x in [10, 50] {
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": x, "y": 10, "width": 20, "height": 20, "fill": "Red"}),
        );
    }
    run(
        &mut s,
        "background.set",
        json!({"path": "ref.png", "width": 50}),
    );
    std::fs::create_dir_all(dir.join("before")).expect("dir");
    std::fs::create_dir_all(dir.join("after")).expect("dir");
    run(&mut s, "design.export", json!({"path": "before/x.dst"}));
    run(&mut s, "project.save", json!({"path": "before.osd"}));
    let before_osd = std::fs::read_to_string(dir.join("before.osd")).expect("osd");
    assert!(
        !before_osd.contains("\"layer\""),
        "the default is left out of .osd"
    );
    let first = s.design().objects[0].id;
    let r = run(
        &mut s,
        "background.setLayer",
        json!({"position": "afterObject", "afterId": first, "opacity": 0.8}),
    );
    assert_eq!(
        r,
        json!({"position": "afterObject", "afterId": first, "opacity": 0.8})
    );
    run(&mut s, "design.export", json!({"path": "after/x.dst"}));
    assert_eq!(
        std::fs::read(dir.join("before/x.dst")).expect("dst"),
        std::fs::read(dir.join("after/x.dst")).expect("dst"),
        "the image never sews"
    );
    run(&mut s, "project.save", json!({"path": "after.osd"}));
    let mut o = Session::new(dir.path());
    run(&mut o, "project.open", json!({"path": "after.osd"}));
    assert_eq!(o.design().background, s.design().background, "round trip");
    run(&mut s, "history.undo", json!({}));
    assert_eq!(
        s.design().background.as_ref().map(|b| b.layer),
        Some(openstitch_design::BackgroundLayer::Bottom)
    );
    run(&mut s, "background.setLayer", json!({"position": "top"}));
    for bad in [
        json!({"position": "afterObject"}),
        json!({"position": "afterObject", "afterId": 999}),
        json!({"position": "top", "afterId": first}),
        json!({"position": "middle"}),
    ] {
        assert!(
            s.execute("background.setLayer", Some(&bad)).is_err(),
            "{bad}"
        );
    }
}
