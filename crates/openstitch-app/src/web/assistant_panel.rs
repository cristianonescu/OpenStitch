//! The web build's stand-in for the Assistant panel (#693). The Assistant needs a model
//! connection, the system keychain and a local socket for coding agents, none of which a web
//! page has; the editor's own `ui.*` commands still work. These are the few things the rest of
//! the editor calls on the panel.

use crate::app::EditorApp;
use eframe::egui;

/// The panel's state the editor reads: it never opens in the browser.
#[derive(Default)]
pub struct Panel {
    pub open: bool,
}

pub fn attach_files(_app: &mut EditorApp, _paths: Vec<std::path::PathBuf>) {}

pub fn set_open(app: &mut EditorApp, _open: bool) {
    app.assistant.panel.open = false;
    app.status = "The Assistant is not available in the web editor".into();
}

pub fn ai_switched_off(_app: &mut EditorApp) {}

pub fn show_sample_notices(_app: &mut EditorApp) {}

pub fn panel(_app: &mut EditorApp, _ui: &mut egui::Ui) {}
