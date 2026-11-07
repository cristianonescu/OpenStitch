//! The command palette window (#550, beyond PE-DESIGN): Ctrl+K (Cmd+K), type a few words,
//! Up/Down and Return run the command as a click on its button would. The search is
//! `openstitch_app::editor::palette`; the Assistant uses the same index with
//! `ui.searchCommands`.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui::{self, RichText};
use openstitch_app::editor::palette;
use openstitch_app::editor::ribbon::{Action, TabId};

/// The palette's shortcut as this platform writes it.
pub const SHORTCUT: &str = if cfg!(target_os = "macos") {
    "Cmd+K"
} else {
    "Ctrl+K"
};

#[derive(Default)]
pub struct PaletteState {
    pub open: bool,
    pub query: String,
    /// The highlighted row.
    pub selected: usize,
    focus: bool,
}

impl PaletteState {
    pub fn show(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
        self.focus = true;
    }
}

/// Why `a` cannot run now, if it cannot.
pub fn blocked(app: &EditorApp, a: Action) -> Option<&'static str> {
    if !a.info().available {
        Some("not available in OpenStitch yet")
    } else if !crate::ribbon::enabled(app, a) {
        Some("cannot run now (check the selection)")
    } else {
        None
    }
}

/// Runs `a` from the palette: the same as clicking its button; a menu button brings its ribbon
/// tab forward instead, since its list is chosen there. Remembers it as recent.
pub fn run(app: &mut EditorApp, a: Action, ctx: &egui::Context) {
    palette::remember(&mut app.ui.prefs.recent_commands, a.info().key);
    app.ui.save_prefs();
    if a.is_menu() {
        if let Some((tab, _)) = palette::place(a)
            && TabId::FIXED.contains(&tab)
        {
            app.ui.tab = tab;
        }
        app.status = format!(
            "{} is a list: choose on the ribbon, {}",
            a.info().label,
            palette::path(a)
        );
        return;
    }
    crate::ribbon::run(app, a, ctx);
}

pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    if !app.palette.open {
        return;
    }
    let hits = palette::search(&app.palette.query, &app.ui.prefs.recent_commands);
    let (down, up, enter, esc) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
        )
    });
    if down {
        app.palette.selected = (app.palette.selected + 1).min(hits.len().saturating_sub(1));
    }
    if up {
        app.palette.selected = app.palette.selected.saturating_sub(1);
    }
    app.palette.selected = app.palette.selected.min(hits.len().saturating_sub(1));
    let mut chosen: Option<Action> = None;
    let mut open = true;
    egui::Window::new("Command Palette")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 80.0))
        .show(ctx, |ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut app.palette.query)
                    .hint_text("Type a command: arrange, satin, export…")
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut app.palette.focus) {
                edit.request_focus();
            }
            if edit.changed() {
                app.palette.selected = 0;
            }
            ui.separator();
            if hits.is_empty() {
                ui.label(RichText::new(t("ui-no-command-matches")).weak());
            }
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    for (k, &a) in hits.iter().enumerate() {
                        let info = a.info();
                        let why = blocked(app, a);
                        let mut text = format!("{}    {}", info.label, palette::path(a));
                        if let Some(s) = info.shortcut_for(cfg!(target_os = "macos")) {
                            text.push_str(&format!("    {s}"));
                        }
                        let selected = k == app.palette.selected;
                        let r = ui
                            .add_enabled(why.is_none(), egui::Button::selectable(selected, text))
                            .on_hover_text(info.tip)
                            .on_disabled_hover_text(why.unwrap_or_default());
                        if selected && (down || up) {
                            r.scroll_to_me(None);
                        }
                        if r.clicked() {
                            chosen = Some(a);
                        }
                    }
                });
        });
    if enter
        && let Some(&a) = hits.get(app.palette.selected)
        && blocked(app, a).is_none()
    {
        chosen = Some(a);
    }
    if esc || !open || chosen.is_some() {
        app.palette.open = false;
    }
    if let Some(a) = chosen {
        run(app, a, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;
    use serde_json::json;

    fn demo() -> EditorApp {
        EditorApp::new(Launch {
            demo: true,
            ..Launch::default()
        })
    }

    #[test]
    fn running_from_the_palette_equals_clicking() {
        let ctx = egui::Context::default();
        let (mut clicked, mut palette) = (demo(), demo());
        for app in [&mut clicked, &mut palette] {
            app.selection = [1].into();
        }
        crate::ribbon::run(&mut clicked, Action::Duplicate, &ctx);
        run(&mut palette, Action::Duplicate, &ctx);
        assert_eq!(clicked.session.design(), palette.session.design());
        assert_eq!(clicked.selection, palette.selection);
        // One undo step each.
        palette.undo();
        clicked.undo();
        assert_eq!(clicked.session.design(), palette.session.design());
        assert_eq!(palette.ui.prefs.recent_commands, ["duplicate"]);
        // A disabled command says why; a menu brings its tab forward instead of running.
        palette.selection.clear();
        assert!(blocked(&palette, Action::Duplicate).is_some());
        run(&mut palette, Action::Export, &ctx);
        assert!(palette.status.contains("list"));
    }

    #[test]
    fn the_assistant_searches_the_same_index() {
        let mut app = demo();
        let r = app
            .ui_command(
                "ui.searchCommands",
                Some(&json!({ "query": "arrange copy", "limit": 3 })),
            )
            .expect("search");
        assert_eq!(r[0]["key"], "arrangeCopy");
        assert!(r[0]["path"].as_str().is_some_and(|p| p.starts_with("Home")));
        assert!(r.as_array().is_some_and(|l| l.len() <= 3));
        // Recovery commands answer even without a recovery folder.
        assert!(app.ui_command("ui.recoveryList", None).is_ok());
        assert!(
            app.ui_command("ui.recoveryRestore", Some(&json!({ "id": "1-0" })))
                .is_err()
        );
        assert!(
            app.ui_command("ui.recoveryDiscard", Some(&json!({})))
                .is_err()
        );
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "propertySearch", "value": "pull" })),
        )
        .expect("search");
        assert_eq!(app.ui.property_search, "pull");
    }
}
