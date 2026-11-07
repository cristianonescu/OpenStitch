// The editor window's modules (#693): `main.rs` includes this list for the desktop binary, and
// on wasm32 `lib.rs` includes it too, so the web page (openstitch-web) runs the same EditorApp.
mod a11y;
mod app;
mod arc_ui;
mod arrange_ui;
mod art;
#[cfg(not(target_arch = "wasm32"))]
mod assistant_panel;
// The web build has no Assistant panel (#693): a stand-in with what the editor calls.
#[cfg(target_arch = "wasm32")]
#[path = "web/assistant_panel.rs"]
mod assistant_panel;
mod assistant_ui;
mod canvas;
mod close_ui;
mod colour_match_ui;
mod convert_ui;
mod creator_ui;
mod database_ui;
mod database_writing;
mod edit_ui;
mod files;
// The Assistant's notes (`openstitch-assistant/src/notes.rs`, std only) for the web build, which
// cannot link the Assistant's network client (#693).
#[cfg(target_arch = "wasm32")]
#[path = "../../openstitch-assistant/src/notes.rs"]
mod web_notes;
mod envelope_ui;
mod finish_ui;
mod icons;
mod inkstitch_ui;
mod jobsheet_ui;
mod keyboard_ui;
mod keymap_ui;
mod lettering_ui;
mod line_gap_ui;
mod open_job;
mod palette_ui;
mod panels;
mod panes;
mod pattern_wizard_ui;
mod pickers_ui;
mod print_ui;
mod quality_ui;
mod recovery_ui;
mod remote;
mod ribbon;
mod machine_setup_ui;
mod ripple_ui;
mod script;
mod sections_ui;
mod settings_ui;
mod sewing_ui;
mod shape_effects_ui;
mod shape_tools_ui;
mod splash;
mod start_ui;
mod stitch_table_ui;
mod suite_chrome;
mod swatch_ui;
mod theme;
mod transfer_ui;
mod units_ui;
#[cfg(not(target_arch = "wasm32"))]
mod update_ui;
// The web page is always the latest build: no updater (#693).
#[cfg(target_arch = "wasm32")]
#[path = "web/update_ui.rs"]
mod update_ui;
mod view_ui;
mod wizard_ui;
mod workspace_ui;
mod zoom_ui;
