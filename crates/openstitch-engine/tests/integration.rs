//! Every integration test of this crate in one test binary (#403): one compile and one link
//! instead of one per file, which was most of the warm CI build. Cargo.toml sets
//! `autotests = false`, so a new file under tests/ needs its own line here;
//! `every_test_file_is_listed` fails when one is missing. The files keep their own `mod` items
//! (`mod common;`), which `#[path]` resolves from this folder as before.

#[path = "arrange_modes.rs"]
mod arrange_modes;
#[path = "background_layer.rs"]
mod background_layer;
#[path = "branching.rs"]
mod branching;
#[path = "builder.rs"]
mod builder;
#[path = "capabilities.rs"]
mod capabilities;
#[path = "continuing.rs"]
mod continuing;
#[path = "contour_fill.rs"]
mod contour_fill;
#[path = "convert.rs"]
mod convert;
#[path = "coverage_digitize.rs"]
mod coverage_digitize;
#[path = "coverage_editing.rs"]
mod coverage_editing;
#[path = "coverage_fabric.rs"]
mod coverage_fabric;
#[path = "coverage_formats.rs"]
mod coverage_formats;
#[path = "coverage_lettering.rs"]
mod coverage_lettering;
#[path = "coverage_library.rs"]
mod coverage_library;
#[path = "coverage_pattern_points.rs"]
mod coverage_pattern_points;
#[path = "coverage_shape_ops.rs"]
mod coverage_shape_ops;
#[path = "coverage_stitch_types.rs"]
mod coverage_stitch_types;
#[path = "decorative_lines.rs"]
mod decorative_lines;
#[path = "determinism.rs"]
mod determinism;
#[path = "digitize.rs"]
mod digitize;
#[path = "direction.rs"]
mod direction;
#[path = "drawing.rs"]
mod drawing;
#[path = "edit_tools.rs"]
mod edit_tools;
#[path = "engine.rs"]
mod engine;
#[path = "extra_exports.rs"]
mod extra_exports;
#[path = "fabric_presets.rs"]
mod fabric_presets;
#[path = "finish.rs"]
mod finish;
#[path = "font_creator.rs"]
mod font_creator;
#[path = "font_from_image.rs"]
mod font_from_image;
#[path = "fonts.rs"]
mod fonts;
#[path = "free_hoopings.rs"]
mod free_hoopings;
#[path = "hoop_lists.rs"]
mod hoop_lists;
#[path = "imports.rs"]
mod imports;
#[path = "job_sheets.rs"]
mod job_sheets;
#[path = "knife.rs"]
mod knife;
#[path = "knockdown.rs"]
mod knockdown;
#[path = "layout.rs"]
mod layout;
#[path = "lettering.rs"]
mod lettering;
#[path = "lettering_arc.rs"]
mod lettering_arc;
#[path = "lettering_envelopes.rs"]
mod lettering_envelopes;
#[path = "lettering_library.rs"]
mod lettering_library;
#[path = "library.rs"]
mod library;
#[path = "line_looks.rs"]
mod line_looks;
#[path = "machine.rs"]
mod machine;
#[path = "mapped_text.rs"]
mod mapped_text;
#[path = "media_exports.rs"]
mod media_exports;
#[path = "monogram.rs"]
mod monogram;
#[path = "name_drop.rs"]
mod name_drop;
#[path = "order.rs"]
mod order;
#[path = "order_holes.rs"]
mod order_holes;
#[path = "order_min_jump.rs"]
mod order_min_jump;
#[path = "ornament.rs"]
mod ornament;
#[path = "palette.rs"]
mod palette;
#[path = "pes_compat.rs"]
mod pes_compat;
#[path = "pes_resave.rs"]
mod pes_resave;
#[path = "print.rs"]
mod print;
#[path = "production.rs"]
mod production;
#[path = "punch.rs"]
mod punch;
#[path = "quality.rs"]
mod quality;
#[path = "random_designs.rs"]
mod random_designs;
#[path = "region_compat.rs"]
mod region_compat;
#[path = "routing.rs"]
mod routing;
#[path = "satin_compat.rs"]
mod satin_compat;
#[path = "satin_shaping.rs"]
mod satin_shaping;
#[path = "sections.rs"]
mod sections;
#[path = "session_fork.rs"]
mod session_fork;
#[path = "session_sibling.rs"]
mod session_sibling;
#[path = "sewing.rs"]
mod sewing;
#[path = "shape_cut_attributes.rs"]
mod shape_cut_attributes;
#[path = "shape_tools.rs"]
mod shape_tools;
#[path = "shaping.rs"]
mod shaping;
#[path = "smoothing.rs"]
mod smoothing;
#[path = "stamps.rs"]
mod stamps;
#[path = "stitch_edit.rs"]
mod stitch_edit;
#[path = "stitch_engine_4.rs"]
mod stitch_engine_4;
#[path = "stitch_types.rs"]
mod stitch_types;
#[path = "svg_import.rs"]
mod svg_import;
#[path = "svg_settings.rs"]
mod svg_settings;
#[path = "swatch.rs"]
mod swatch;
#[path = "templates.rs"]
mod templates;
#[path = "thread_lists.rs"]
mod thread_lists;
#[path = "thread_match.rs"]
mod thread_match;
#[path = "timeline.rs"]
mod timeline;
#[path = "transfer.rs"]
mod transfer;
#[path = "user.rs"]
mod user;
#[path = "user_patterns.rs"]
mod user_patterns;
#[path = "validate.rs"]
mod validate;
#[path = "view.rs"]
mod view;
#[path = "wizards.rs"]
mod wizards;

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
