//! Coverage for the per-design font limits (#330): at most `MAX_DESIGN_FONTS` fonts and
//! `MAX_TOTAL_FONT_BYTES` of font data in one design; a refused load leaves the design as it was.

use openstitch_design::fonts::{MAX_DESIGN_FONTS, MAX_FONT_BYTES, MAX_TOTAL_FONT_BYTES};
use openstitch_engine::Session;
use serde_json::{Value, json};
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "openstitch-coverage-lettering-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn lato() -> &'static [u8] {
    include_bytes!("../../openstitch-design/fonts/Lato-Bold.ttf")
}

fn load(s: &mut Session, path: &str, id: &str) -> Result<Value, String> {
    s.execute("fonts.loadTrueType", Some(&json!({"path": path, "id": id})))
        .map_err(|e| e.to_string())
}

#[test]
fn a_design_holds_at_most_sixteen_fonts() {
    assert_eq!(MAX_DESIGN_FONTS, 16, "the limit FEATURE_MATRIX states");
    let dir = temp_dir("count");
    std::fs::write(dir.join("user.ttf"), lato()).expect("font");
    let mut s = Session::new(&dir);
    for n in 1..=MAX_DESIGN_FONTS {
        load(&mut s, "user.ttf", &format!("font-{n}")).expect("within the limit");
    }
    assert_eq!(s.design().fonts.len(), MAX_DESIGN_FONTS);
    let before = s.design().clone();
    let e = load(&mut s, "user.ttf", "one-too-many").expect_err("refused");
    assert!(e.contains("16"), "{e}");
    assert_eq!(s.design(), &before, "a refused load changes nothing");

    // Removing one makes room again.
    s.execute("fonts.remove", Some(&json!({"id": "font-1"})))
        .expect("remove");
    load(&mut s, "user.ttf", "one-more").expect("room again");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_design_holds_at_most_forty_mib_of_fonts() {
    assert_eq!(
        MAX_TOTAL_FONT_BYTES,
        40 << 20,
        "the limit FEATURE_MATRIX states"
    );
    let dir = temp_dir("bytes");
    // Lato Bold padded to 15 MiB: a valid font (the padding follows its tables), under the
    // 24 MiB per-file limit; two fit in 40 MiB, a third does not.
    let mut big = lato().to_vec();
    big.resize(15 << 20, 0);
    assert!(big.len() <= MAX_FONT_BYTES);
    std::fs::write(dir.join("big.ttf"), &big).expect("font");
    std::fs::write(dir.join("small.ttf"), lato()).expect("font");
    let mut s = Session::new(&dir);
    load(&mut s, "big.ttf", "big-1").expect("15 MiB");
    load(&mut s, "big.ttf", "big-2").expect("30 MiB");
    let before = s.design().clone();
    let e = load(&mut s, "big.ttf", "big-3").expect_err("45 MiB refused");
    assert!(e.contains("40 MiB"), "{e}");
    assert_eq!(s.design(), &before, "a refused load changes nothing");
    load(&mut s, "small.ttf", "small").expect("a small font still fits");
    let _ = std::fs::remove_dir_all(&dir);
}
