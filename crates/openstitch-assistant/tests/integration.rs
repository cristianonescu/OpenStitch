//! Every integration test of this crate in one test binary (#403): one compile and one link
//! instead of one per file, which was most of the warm CI build. Cargo.toml sets
//! `autotests = false`, so a new file under tests/ needs its own line here;
//! `every_test_file_is_listed` fails when one is missing. The files keep their own `mod` items
//! (`mod common;`), which `#[path]` resolves from this folder as before.

#[path = "acp.rs"]
mod acp;
#[path = "local_preset.rs"]
mod local_preset;
#[path = "openai.rs"]
mod openai;
#[path = "stuck.rs"]
mod stuck;
#[path = "turns.rs"]
mod turns;

/// A test file left out of this list would compile and run nowhere.
#[test]
fn every_test_file_is_listed() {
    let listed = include_str!("integration.rs");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("tests folder") {
        let name = entry
            .expect("tests folder entry")
            .file_name()
            .into_string()
            .expect("UTF-8 file name");
        if name.ends_with(".rs") && name != "integration.rs" {
            let line = format!("#[path = \"{name}\"]");
            if !listed.contains(&line) {
                missing.push(name);
            }
        }
    }
    assert!(
        missing.is_empty(),
        "add these to tests/integration.rs: {missing:?}"
    );
}
