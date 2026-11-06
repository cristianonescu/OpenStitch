//! Keyboard shortcuts the user can change (#551): the keys pressed this frame as commands, and
//! Help ▸ Keyboard Shortcuts as an editor (record, remove, reset, export, import). The keymap
//! itself is `openstitch_app::editor::keymap`; the Assistant reads and sets it with
//! `ui.settings` / `ui.set shortcuts`.

use crate::app::EditorApp;
use eframe::egui::{self, KeyboardShortcut, Modifiers, RichText};
use openstitch_app::editor::keymap::{self, Keymap};
use openstitch_app::editor::ribbon::Action;
use serde_json::json;

/// Help ▸ Keyboard Shortcuts state.
#[derive(Default)]
pub struct KeysWindow {
    /// The command waiting for a key to be pressed.
    pub recording: Option<Action>,
    /// Commands whose label contains this.
    pub filter: String,
    /// List every command, not only those with a shortcut.
    pub all: bool,
    /// The last refusal (a clash) or confirmation.
    pub message: String,
}

/// "Ctrl+Shift+S" as an egui shortcut (Ctrl is Cmd on macOS).
pub fn parse(chord: &str) -> Option<KeyboardShortcut> {
    let (mods, key) = match chord.rsplit_once('+') {
        Some((m, k)) => (m, k),
        None => ("", chord),
    };
    let key = egui::Key::from_name(key)?;
    let mut m = Modifiers::NONE;
    for p in mods.split('+').filter(|p| !p.is_empty()) {
        m |= match p {
            "Ctrl" => Modifiers::COMMAND,
            "Shift" => Modifiers::SHIFT,
            "Alt" => Modifiers::ALT,
            _ => return None,
        };
    }
    Some(KeyboardShortcut::new(m, key))
}

/// A chord as this platform writes it.
pub fn shown(chord: &str) -> String {
    if cfg!(target_os = "macos") {
        chord.replace("Ctrl+", "Cmd+").replace("Alt+", "Option+")
    } else {
        chord.to_string()
    }
}

/// The commands whose shortcut was pressed this frame, consumed so nothing else sees them.
/// While a text field has the keyboard only [`keymap::WHILE_TYPING`] commands run; while a
/// shortcut is being recorded none do.
pub fn pressed(app: &EditorApp, ctx: &egui::Context) -> Vec<Action> {
    if app.ui.keys.recording.is_some() {
        return Vec::new();
    }
    let typing = ctx.egui_wants_keyboard_input();
    let mut keyed: Vec<(Action, KeyboardShortcut)> = app
        .ui
        .prefs
        .keymap
        .bindings()
        .into_iter()
        .filter(|(a, _)| !typing || keymap::WHILE_TYPING.contains(a))
        .filter_map(|(a, c)| parse(&c).map(|k| (a, k)))
        .collect();
    // egui matches Ctrl+Z on Ctrl+Shift+Z too: the shortcuts with more modifiers go first.
    keyed.sort_by_key(|(_, k)| {
        std::cmp::Reverse(u8::from(k.modifiers.shift) + u8::from(k.modifiers.alt))
    });
    let mut hits = Vec::new();
    for (a, k) in keyed {
        if !hits.contains(&a) && ctx.input_mut(|i| i.consume_shortcut(&k)) {
            hits.push(a);
        }
    }
    hits
}

/// The key pressed with its modifiers, as a chord ("Ctrl+Alt+R"); None for Escape.
fn recorded(ctx: &egui::Context) -> Option<Result<String, ()>> {
    ctx.input_mut(|i| {
        let at = i.events.iter().position(|e| {
            matches!(
                e,
                egui::Event::Key {
                    pressed: true,
                    repeat: false,
                    ..
                }
            )
        })?;
        let egui::Event::Key { key, modifiers, .. } = i.events.remove(at) else {
            return None;
        };
        if key == egui::Key::Escape {
            return Some(Err(()));
        }
        let mut chord = String::new();
        for (on, name) in [
            (modifiers.command, "Ctrl+"),
            (modifiers.alt, "Alt+"),
            (modifiers.shift, "Shift+"),
        ] {
            if on {
                chord.push_str(name);
            }
        }
        chord.push_str(key.name());
        Some(Ok(chord))
    })
}

fn set_keymap(app: &mut EditorApp, k: Keymap, message: String) {
    app.ui.prefs.keymap = k;
    app.ui.save_prefs();
    app.ui.keys.message = message;
}

/// Help ▸ Keyboard Shortcuts: every shortcut, editable.
pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.ui.shortcuts_open;
    if let Some(a) = app.ui.keys.recording
        && let Some(r) = recorded(ctx)
    {
        app.ui.keys.recording = None;
        match r {
            Err(()) => app.ui.keys.message.clear(),
            Ok(chord) => {
                let mut k = app.ui.prefs.keymap.clone();
                match k.add(a, &chord) {
                    Ok(c) => {
                        set_keymap(app, k, format!("{} now runs {}", shown(&c), a.info().label))
                    }
                    // A clash or a key that cannot be a shortcut: shown, nothing saved.
                    Err(e) => app.ui.keys.message = e,
                }
            }
        }
    }
    egui::Window::new("Keyboard Shortcuts")
        .open(&mut open)
        .default_size([480.0, 520.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Find");
                ui.add(
                    egui::TextEdit::singleline(&mut app.ui.keys.filter)
                        .hint_text("command name")
                        .desired_width(200.0),
                );
                ui.checkbox(&mut app.ui.keys.all, "All commands")
                    .on_hover_text("List commands without a shortcut too, to give them one");
            });
            ui.horizontal(|ui| {
                if ui
                    .button("Reset All")
                    .on_hover_text("Back to the PE-style default shortcuts")
                    .clicked()
                {
                    set_keymap(app, Keymap::default(), "Default shortcuts restored".into());
                }
                if ui
                    .button("Export…")
                    .on_hover_text("Save the shortcuts as a JSON file")
                    .clicked()
                {
                    export(app);
                }
                if ui
                    .button("Import…")
                    .on_hover_text("Load shortcuts saved with Export (replaces the changed ones)")
                    .clicked()
                {
                    import(app);
                }
            });
            if !app.ui.keys.message.is_empty() {
                ui.label(RichText::new(&app.ui.keys.message).strong());
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    rows(app, ui);
                });
            ui.separator();
            fixed_keys(ui);
        });
    app.ui.shortcuts_open = open;
    if !open {
        app.ui.keys.recording = None;
    }
}

fn rows(app: &mut EditorApp, ui: &mut egui::Ui) {
    let filter = app.ui.keys.filter.to_lowercase();
    egui::Grid::new("shortcuts").striped(true).show(ui, |ui| {
        for &a in Action::ALL {
            if !keymap::editable(a) {
                continue;
            }
            let info = a.info();
            let chords = app.ui.prefs.keymap.chords(a);
            if (chords.is_empty() && !app.ui.keys.all)
                || !filter.is_empty() && !info.label.to_lowercase().contains(&filter)
            {
                continue;
            }
            let mut label = RichText::new(info.label);
            if app.ui.prefs.keymap.is_custom(a) {
                label = label.strong();
            }
            ui.label(label).on_hover_text(info.tip);
            ui.horizontal(|ui| {
                for c in &chords {
                    if ui
                        .small_button(RichText::new(shown(c)).monospace())
                        .on_hover_text("Click to remove this shortcut")
                        .clicked()
                    {
                        let mut k = app.ui.prefs.keymap.clone();
                        k.remove(a, c);
                        set_keymap(app, k, format!("{} removed", shown(c)));
                    }
                }
            });
            ui.horizontal(|ui| {
                let recording = app.ui.keys.recording == Some(a);
                let text = if recording { "Press a key…" } else { "Add" };
                if ui
                    .selectable_label(recording, text)
                    .on_hover_text("Press the new shortcut; Esc cancels")
                    .clicked()
                {
                    app.ui.keys.recording = (!recording).then_some(a);
                    app.ui.keys.message.clear();
                }
                if app.ui.prefs.keymap.is_custom(a) && ui.small_button("Reset").clicked() {
                    let mut k = app.ui.prefs.keymap.clone();
                    k.reset(a);
                    set_keymap(app, k, format!("{} back to its default", info.label));
                }
            });
            ui.end_row();
        }
    });
}

/// Keys that cannot be changed.
fn fixed_keys(ui: &mut egui::Ui) {
    let mac = cfg!(target_os = "macos");
    egui::CollapsingHeader::new("Keys that cannot be changed").show(ui, |ui| {
        egui::Grid::new("fixed_keys").striped(true).show(ui, |ui| {
            let tools = "V A R E P B L U H M Z";
            let quit = if mac { "Cmd+Q, Cmd+W" } else { "Alt+F4" };
            for (what, keys) in [
                (
                    "Copy / Cut / Paste",
                    if mac {
                        "Cmd+C / Cmd+X / Cmd+V"
                    } else {
                        "Ctrl+C / Ctrl+X / Ctrl+V"
                    },
                ),
                (
                    "Find and run any command (Command Palette)",
                    crate::palette_ui::SHORTCUT,
                ),
                (
                    "Tools (Select, Point, shapes, Pan, Measure, Zoom Box)",
                    tools,
                ),
                ("Move the selection 0.1 mm / 1 mm", "Arrows / Shift+Arrows"),
                ("Finish a shape", "Return"),
                ("Cancel", "Esc"),
                ("Pan", "Space+drag, middle button"),
                ("Zoom at the pointer", "Pinch, Ctrl+scroll"),
                ("Quit / close the window (asks to save changes)", quit),
            ] {
                ui.label(what);
                ui.label(RichText::new(keys).monospace());
                ui.end_row();
            }
        });
    });
}

/// The file Export writes and Import reads.
pub fn file_json(k: &Keymap) -> serde_json::Value {
    json!({ "openstitchShortcuts": 1, "shortcuts": k.to_json() })
}

/// The keymap in a file Export wrote (or a bare `{ commandKey: [...] }` object).
pub fn from_file_json(v: &serde_json::Value) -> Result<Keymap, String> {
    Keymap::from_json(if v["shortcuts"].is_object() {
        &v["shortcuts"]
    } else {
        v
    })
}

fn export(app: &mut EditorApp) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Shortcuts", &["json"])
        .set_file_name("openstitch-shortcuts.json")
        .save_file()
    else {
        return;
    };
    let text = serde_json::to_string_pretty(&file_json(&app.ui.prefs.keymap)).unwrap_or_default();
    app.ui.keys.message = match std::fs::write(&path, text) {
        Ok(()) => format!("Saved {}", path.display()),
        Err(e) => format!("Cannot save {}: {e}", path.display()),
    };
}

fn import(app: &mut EditorApp) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Shortcuts", &["json"])
        .pick_file()
    else {
        return;
    };
    let read = std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        .and_then(|v| from_file_json(&v));
    match read {
        Ok(k) => set_keymap(app, k, format!("Loaded {}", path.display())),
        Err(e) => app.ui.keys.message = format!("Cannot load {}: {e}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shortcut_parses_and_export_import_round_trips() {
        for &(a, chords) in keymap::DEFAULTS {
            for c in chords {
                assert!(parse(c).is_some(), "{a:?} {c}");
            }
        }
        for c in [
            "Alt+F5",
            "Ctrl+Shift+[",
            "Ctrl+,",
            "Ctrl+.",
            "Ctrl+/",
            "Ctrl+;",
            "PageUp",
        ] {
            let n = keymap::normalize(c).expect(c);
            assert!(parse(&n).is_some(), "{n}");
        }
        // Recorded key names (egui's) are understood.
        for k in [
            egui::Key::Minus,
            egui::Key::Equals,
            egui::Key::OpenBracket,
            egui::Key::F3,
        ] {
            let n = keymap::normalize(&format!("Ctrl+{}", k.name()))
                .unwrap_or_else(|e| panic!("{}: {e}", k.name()));
            assert_eq!(parse(&n).map(|s| s.logical_key), Some(k));
        }
        let mut k = Keymap::default();
        k.add(Action::Rectangle, "Alt+R").expect("free");
        k.remove(Action::Save, "Ctrl+S");
        let dir = openstitch_testutil::TempDir::new("keys");
        let path = dir.join("keys.json");
        std::fs::write(&path, file_json(&k).to_string()).expect("write");
        let back: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(from_file_json(&back), Ok(k));
    }
}
