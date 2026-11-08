//! The browser version's notice for what only the desktop can do (#830): one message, in the
//! status line and in a small window, with what to do instead and the issue that tracks it.

use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::browser_limits::Limit;
use openstitch_app::editor::ui_texts::t;
use serde_json::{Value, json};

/// In the browser, shows `limit`'s notice and returns true (the caller then does nothing
/// else); on the desktop returns false.
pub fn browser_limit(app: &mut EditorApp, limit: Limit) -> bool {
    if !openstitch_app::platform::in_browser() {
        return false;
    }
    app.status = limit.message();
    app.ui.limit_notice = Some(limit);
    true
}

/// Each frame: a folder chooser that the browser could not answer (the page's file dialog
/// stub notes it) shows the Folders notice.
pub fn poll(app: &mut EditorApp) {
    if crate::files::take_folder_refused() {
        browser_limit(app, Limit::Folders);
    }
}

/// The notice window.
pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    let Some(limit) = app.ui.limit_notice else {
        return;
    };
    let mut open = true;
    let mut ok = false;
    egui::Window::new(t("ui-window-not-in-the-browser-version"))
        .id(egui::Id::unique("window-browser-limit"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_max_width(360.0);
            ui.label(egui::RichText::new(limit.what()).strong());
            ui.label(limit.instead());
            let url = format!(
                "https://github.com/cristianonescu/OpenStitch/issues/{}",
                limit.issue()
            );
            ui.hyperlink_to(format!("OpenStitch issue #{}", limit.issue()), url);
            ui.add_space(4.0);
            ok = ui.button(t("ui-ok")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter));
        });
    if !open || ok {
        app.ui.limit_notice = None;
    }
}

/// `ui.browserLimits`: whether this is the browser version, and each limit with its message.
pub fn command(app: &EditorApp) -> Value {
    json!({
        "inBrowser": openstitch_app::platform::in_browser(),
        "shown": app.ui.limit_notice.map(Limit::key),
        "limits": Limit::ALL.iter().map(|l| json!({
            "key": l.key(),
            "what": l.what(),
            "instead": l.instead(),
            "issue": l.issue(),
            "message": l.message(),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;

    /// #830: in the browser (pretended here) each desktop-only entry point shows its notice
    /// and does nothing else; on the desktop the same calls go ahead as before.
    #[test]
    fn desktop_only_features_say_so_in_the_browser() {
        let mut app = EditorApp::new(Launch::default());
        // The desktop: no notice.
        assert!(!browser_limit(&mut app, Limit::Print));
        assert!(app.ui.limit_notice.is_none());
        crate::files::note_folder_refused();
        poll(&mut app);
        assert!(app.ui.limit_notice.is_none(), "no browser, no notice");
        openstitch_app::platform::pretend_browser(true);
        // Send to USB Media opens nothing and says why.
        crate::transfer_ui::open(&mut app);
        assert!(!app.workspace.usb.open);
        assert_eq!(app.ui.limit_notice, Some(Limit::Transfer));
        assert!(app.status.contains("USB"), "{}", app.status);
        // A folder chooser the page could not answer.
        app.ui.limit_notice = None;
        crate::files::note_folder_refused();
        poll(&mut app);
        assert_eq!(app.ui.limit_notice, Some(Limit::Folders));
        // Through ui.browserLimits.
        let v = app.ui_command("ui.browserLimits", None).expect("limits");
        assert_eq!(v["inBrowser"], json!(true));
        assert_eq!(v["shown"], json!("folders"));
        assert_eq!(v["limits"].as_array().map(Vec::len), Some(Limit::ALL.len()));
        assert!(v["limits"][0]["message"].as_str().unwrap_or_default().contains("#766"));
        openstitch_app::platform::pretend_browser(false);
    }
}
