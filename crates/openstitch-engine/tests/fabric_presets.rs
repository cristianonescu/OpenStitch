//! Fabric presets (#508, part 1): thirteen fabrics, and the makers' backing, topping, needle and
//! smallest text height for each fabric and thread weight, through the commands.

use openstitch_engine::Session;
use openstitch_engine::fabric::{FABRICS, PE_FABRICS, WEIGHTS, advice};
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn thirteen_fabrics_and_pe_designs_eight_first() {
    let mut s = Session::new(std::env::temp_dir());
    let list = run(&mut s, "fabric.list", json!({}));
    let list = list.as_array().expect("list");
    assert_eq!(list.len(), 13);
    let ids: Vec<&str> = list.iter().map(|f| f["id"].as_str().expect("id")).collect();
    assert_eq!(
        &ids[..PE_FABRICS],
        [
            "standard", "thin", "heavy", "knit", "fleece", "towel", "felt", "leather"
        ]
    );
    for (k, f) in list.iter().enumerate() {
        assert_eq!(f["beyondPeDesign"], json!(k >= PE_FABRICS), "{}", f["id"]);
        for key in ["backing", "topping", "needle"] {
            assert!(
                f[key].as_str().is_some_and(|v| !v.is_empty()),
                "{} has no {key}",
                f["id"]
            );
        }
        assert_eq!(f["minTextMm"], json!(6.0));
    }
}

#[test]
fn the_advice_table_covers_every_fabric_and_cites_known_sources() {
    let a = advice();
    let ids: Vec<&str> = FABRICS.iter().map(|f| f.id).collect();
    let keys: Vec<&str> = a.fabrics.keys().map(String::as_str).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "one advice entry per fabric");
    let known: Vec<&str> = a.sources.iter().map(|s| s.id.as_str()).collect();
    for (id, f) in &a.fabrics {
        assert!(!f.sources.is_empty(), "{id}");
        for s in &f.sources {
            assert!(known.contains(&s.as_str()), "{id} cites {s}");
        }
    }
    for w in WEIGHTS {
        let t = a.threads.get(&w.to_string()).expect("each weight");
        for s in &t.sources {
            assert!(known.contains(&s.as_str()), "No. {w} cites {s}");
        }
    }
    for s in &a.sources {
        assert!(s.url.starts_with("https://"), "{}", s.id);
    }
}

#[test]
fn advise_gives_every_fabric_and_weight_with_its_sources() {
    let mut s = Session::new(std::env::temp_dir());
    for f in FABRICS {
        for w in WEIGHTS {
            let r = run(
                &mut s,
                "fabric.advise",
                json!({ "fabric": f.id, "weight": w }),
            );
            assert_eq!(r["fabric"], json!(f.id));
            assert!(
                r["needle"]["size"]
                    .as_str()
                    .is_some_and(|v| v.contains('/'))
            );
            assert!(r["minTextMm"].as_f64().is_some_and(|v| v > 0.0));
            assert!(!r["sources"].as_array().expect("sources").is_empty());
        }
    }
    // Published: No. 60 lettering down to 3 mm, No. 40 to 6 mm; No. 60 takes a 65/9 needle.
    let r = run(
        &mut s,
        "fabric.advise",
        json!({ "fabric": "standard", "weight": 60 }),
    );
    assert_eq!(
        (r["minTextMm"].clone(), r["needle"]["size"].clone()),
        (json!(3.0), json!("75/11"))
    );
    assert_eq!(r["minTextBasis"], json!("published"));
    let r = run(
        &mut s,
        "fabric.advise",
        json!({ "fabric": "thin", "weight": 60 }),
    );
    assert_eq!(r["needle"]["size"], json!("65/9"));
    // The default is No. 40; towels need at least a 90/14, whatever the thread.
    let r = run(&mut s, "fabric.advise", json!({ "fabric": "towel" }));
    assert_eq!(
        (r["weight"].clone(), r["needle"]["size"].clone()),
        (json!(40), json!("90/14"))
    );
    assert_eq!(r["needle"]["basis"], json!("the fabric's own minimum"));
    // No. 12 lettering is an estimate, said so.
    let r = run(
        &mut s,
        "fabric.advise",
        json!({ "fabric": "knit", "weight": 12 }),
    );
    assert!(
        r["minTextBasis"]
            .as_str()
            .expect("basis")
            .starts_with("estimate")
    );
    assert_eq!(r["needle"]["point"], json!("ballpoint"));
    // Makers disagree on leather needles: both views are given.
    let r = run(&mut s, "fabric.advise", json!({ "fabric": "leather" }));
    assert!(
        r["disagree"]
            .as_str()
            .expect("disagree")
            .contains("Groz-Beckert")
    );
    for bad in [
        json!({ "fabric": "standard", "weight": 50 }),
        json!({ "fabric": "nylon" }),
        json!({}),
    ] {
        assert!(s.execute("fabric.advise", Some(&bad)).is_err(), "{bad}");
    }
}

#[test]
fn a_custom_fabric_is_advised_as_its_base_and_new_fabrics_apply() {
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "fabric.calibrate",
        json!({ "fabric": "pique", "name": "Club polo", "fill": 2 }),
    );
    let r = run(
        &mut s,
        "fabric.advise",
        json!({ "fabric": "custom-club-polo" }),
    );
    assert_eq!(r["needle"]["point"], json!("ballpoint"));
    let id = run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 20, "height": 20 }),
    )["id"]
        .as_u64()
        .expect("id");
    run(
        &mut s,
        "fabric.apply",
        json!({ "fabric": "cap", "ids": [id] }),
    );
    let reg = s.design().objects[0].region.clone().expect("region");
    assert_eq!((reg.density, reg.pull_compensation), (2.8, 0.3));
}
