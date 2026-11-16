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
    // The shared notice, with the issue that tracks it (#766, #830).
    crate::limits_ui::browser_limit(app, openstitch_app::editor::browser_limits::Limit::Assistant);
}

pub fn ai_switched_off(_app: &mut EditorApp) {}

pub fn show_sample_notices(_app: &mut EditorApp) {}

pub fn panel(_app: &mut EditorApp, _ui: &mut egui::Ui) {}

/// A new document identity (#1169); the browser keeps no conversation, but tabs still count.
pub fn next_document() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// No conversation to keep in the browser (#1169): a save writes the chat the file had.
pub fn chat_to_session(_app: &mut EditorApp) {}

pub fn document_loaded(_app: &mut EditorApp) {}

pub fn document_activated(_app: &mut EditorApp) {}
pub fn show_sample_offers(_app: &mut EditorApp, _questions: bool) {}

/// `ui.askUser` and `ui.suggestReplies` (#1172): the web editor has no Assistant panel.
pub fn offer(
    _app: &mut EditorApp,
    _command: &str,
    _params: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    Err("the web editor has no Assistant panel".into())
}
