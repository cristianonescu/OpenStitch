//! Customisable keyboard shortcuts (#551, beyond PE-DESIGN). The default keymap is the
//! PE-style one the editor always had; the user's changes are kept in `ui.json` as
//! `commandKey → [shortcut…]`, only for the commands they changed. Shortcuts are written as on
//! Windows and Linux ("Ctrl+Shift+S"; Ctrl is Cmd on macOS).
//!
//! Single letters stay the tool keys (V, A, R, E, P, B, L, U, H, M, Z) and Copy, Cut, Paste and
//! the Command Palette keep their keys, so a user shortcut needs Ctrl or Alt, or is a function
//! or editing key (F1–F12, Delete, Insert, Home, …).
//!
//! Nothing here depends on egui.

use super::ribbon::Action;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// The editor's shortcuts before any change: the keys `app.rs` always handled. The first one of
/// each command is the one its menu entry and ScreenTip show.
pub const DEFAULTS: &[(Action, &[&str])] = &[
    (Action::New, &["Ctrl+N"]),
    (Action::Open, &["Ctrl+O"]),
    (Action::Save, &["Ctrl+S"]),
    (Action::SaveAs, &["Ctrl+Shift+S"]),
    (Action::Undo, &["Ctrl+Z"]),
    (Action::Redo, &["Ctrl+Y", "Ctrl+Shift+Z"]),
    (Action::ImportPatterns, &["Ctrl+Shift+I"]),
    (Action::OtherZoom, &["Ctrl+0"]),
    (Action::ZoomIn, &["Ctrl+=", "Ctrl+Plus"]),
    (Action::ZoomOut, &["Ctrl+-"]),
    (Action::PaneAssistant, &["Ctrl+Shift+A"]),
    (Action::SelectAll, &["Ctrl+A"]),
    (Action::Duplicate, &["Ctrl+D"]),
    (Action::Group, &["Ctrl+G"]),
    (Action::Ungroup, &["Ctrl+Shift+G"]),
    (Action::JoinLines, &["Ctrl+Shift+J"]),
    // PE-DESIGN's object menu (#1288): Flip Vertical Ctrl+H, Flip Horizontal Ctrl+J (Join
    // Lines, OpenStitch's own, moved to Ctrl+Shift+J), Center Ctrl+M.
    (Action::FlipVertical, &["Ctrl+H"]),
    (Action::FlipHorizontal, &["Ctrl+J"]),
    (Action::CenterObjects, &["Ctrl+M"]),
    (Action::BreakApart, &["Ctrl+Shift+K"]),
    (Action::Delete, &["Delete", "Backspace"]),
];

/// macOS keeps Cmd+H (Hide) and Cmd+M (Minimize) for the system (#1288), so there Flip Vertical
/// and Center, PE-DESIGN's Ctrl+H and Ctrl+M, have Cmd+Shift+H and Cmd+Shift+M instead.
pub const MAC_DEFAULTS: &[(Action, &[&str])] = &[
    (Action::FlipVertical, &["Ctrl+Shift+H"]),
    (Action::CenterObjects, &["Ctrl+Shift+M"]),
];

/// The system's keys on macOS a command cannot have (Cmd is written Ctrl here).
pub const MAC_SYSTEM_KEYS: &[(&str, &str)] = &[
    ("Ctrl+H", "the system's Hide"),
    ("Ctrl+M", "the system's Minimize"),
];

/// Keys that keep their meaning: the clipboard (the system sends them as clipboard events), the
/// Command Palette and closing the window.
pub const RESERVED: &[(&str, &str)] = &[
    ("Ctrl+C", "Copy"),
    ("Ctrl+X", "Cut"),
    ("Ctrl+V", "Paste"),
    ("Ctrl+K", "Command Palette"),
    ("Ctrl+Q", "Quit"),
    ("Ctrl+W", "Close the window"),
];

/// Keys a browser keeps for itself (#834): a web page never sees them, so in the browser
/// version they cannot be given to a command, and New (Ctrl+N) is in the File menu there.
pub const BROWSER_KEYS: &[(&str, &str)] = &[
    ("Ctrl+N", "the browser's New Window"),
    ("Ctrl+T", "the browser's New Tab"),
    ("Ctrl+W", "the browser's Close Tab"),
];

/// Commands that also run while a text field has the keyboard (as before #551).
pub const WHILE_TYPING: &[Action] = &[
    Action::New,
    Action::Open,
    Action::Save,
    Action::SaveAs,
    Action::Undo,
    Action::Redo,
    Action::ImportPatterns,
    Action::OtherZoom,
    Action::ZoomIn,
    Action::ZoomOut,
    Action::PaneAssistant,
];

/// Most shortcuts one command keeps.
pub const MAX_PER_COMMAND: usize = 4;

const NAMED_KEYS: &[&str] = &[
    "Delete",
    "Backspace",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
];
const PUNCTUATION: &[(&str, &str)] = &[
    ("=", "="),
    ("Equals", "="),
    ("-", "-"),
    ("Minus", "-"),
    ("Plus", "Plus"),
    ("[", "["),
    ("OpenBracket", "["),
    ("]", "]"),
    ("CloseBracket", "]"),
    (",", ","),
    ("Comma", ","),
    (".", "."),
    ("Period", "."),
    ("/", "/"),
    ("Slash", "/"),
    (";", ";"),
    ("Semicolon", ";"),
];

/// The default shortcuts of `a` on this platform.
pub fn defaults(a: Action) -> &'static [&'static str] {
    defaults_on(a, cfg!(target_os = "macos"))
}

/// The default shortcuts of `a` on macOS (`mac`) or on Windows, Linux and in the browser.
pub fn defaults_on(a: Action, mac: bool) -> &'static [&'static str] {
    mac.then(|| MAC_DEFAULTS.iter().find(|(d, _)| *d == a))
        .flatten()
        .or_else(|| DEFAULTS.iter().find(|(d, _)| *d == a))
        .map_or(&[], |(_, k)| k)
}

/// Whether the user can give `a` shortcuts: an available command that runs on its own (Copy, Cut
/// and Paste keep the system's keys; dropdown menus need a choice, except the Zoom fit).
pub fn editable(a: Action) -> bool {
    let info = a.info();
    info.available
        && (!a.is_menu() || a == Action::OtherZoom)
        && !matches!(a, Action::Cut | Action::Copy | Action::Paste | Action::Exit)
}

fn key_token(t: &str) -> Option<String> {
    let up = t.to_ascii_uppercase();
    if t.len() == 1 && t.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(up);
    }
    if let Some(n) = up.strip_prefix('F').and_then(|n| n.parse::<u8>().ok())
        && (1..=12).contains(&n)
    {
        return Some(format!("F{n}"));
    }
    if let Some(k) = NAMED_KEYS.iter().find(|k| k.eq_ignore_ascii_case(t)) {
        return Some((*k).to_string());
    }
    PUNCTUATION
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(t))
        .map(|(_, c)| (*c).to_string())
}

/// `chord` written the canonical way ("ctrl+shift+s" → "Ctrl+Shift+S"; Cmd counts as Ctrl).
pub fn normalize(chord: &str) -> Result<String, String> {
    let chord = chord.trim();
    // "Ctrl+-" and "Ctrl++" keep the last character as the key.
    let (mods, key) = match chord.rsplit_once('+') {
        Some((m, "")) => (m.strip_suffix('+').unwrap_or(m), "Plus"),
        Some((m, k)) => (m, k),
        None => ("", chord),
    };
    let key = key_token(key.trim()).ok_or_else(|| format!("'{chord}': unknown key"))?;
    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    for m in mods.split('+').map(str::trim).filter(|m| !m.is_empty()) {
        let flag = match m.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "command" | "control" => &mut ctrl,
            "alt" | "option" => &mut alt,
            "shift" => &mut shift,
            _ => return Err(format!("'{chord}': unknown modifier '{m}'")),
        };
        *flag = true;
    }
    let mut out = String::new();
    for (on, name) in [(ctrl, "Ctrl+"), (alt, "Alt+"), (shift, "Shift+")] {
        if on {
            out.push_str(name);
        }
    }
    out.push_str(&key);
    Ok(out)
}

/// Whether `chord` (canonical) may be a user shortcut: with Ctrl or Alt, or a function or editing
/// key, so typing and the single-letter tool keys keep working.
pub fn allowed(chord: &str) -> bool {
    let key = chord.rsplit('+').next().unwrap_or(chord);
    chord.starts_with("Ctrl+")
        || chord.starts_with("Alt+")
        || key.starts_with('F') && key.len() > 1
        || NAMED_KEYS.contains(&key)
}

/// The user's shortcuts: only the commands they changed are stored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keymap {
    custom: BTreeMap<Action, Vec<String>>,
}

impl Keymap {
    /// The shortcuts of `a` now.
    pub fn chords(&self, a: Action) -> Vec<String> {
        match self.custom.get(&a) {
            Some(k) => k.clone(),
            None => defaults(a).iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Whether `a` has shortcuts other than its defaults.
    pub fn is_custom(&self, a: Action) -> bool {
        self.custom.contains_key(&a)
    }

    pub fn is_default(&self) -> bool {
        self.custom.is_empty()
    }

    /// Every (command, shortcut) pair in effect.
    pub fn bindings(&self) -> Vec<(Action, String)> {
        Action::ALL
            .iter()
            .filter(|a| editable(**a))
            .flat_map(|&a| self.chords(a).into_iter().map(move |c| (a, c)))
            .collect()
    }

    /// What `chord` (canonical) does besides `except`: another command, or a reserved key.
    pub fn owner(&self, chord: &str, except: Action) -> Option<String> {
        if crate::platform::in_browser()
            && let Some((_, what)) = BROWSER_KEYS.iter().find(|(k, _)| *k == chord)
        {
            return Some((*what).to_string());
        }
        if let Some((_, what)) = RESERVED.iter().find(|(k, _)| *k == chord) {
            return Some((*what).to_string());
        }
        if cfg!(target_os = "macos")
            && let Some((_, what)) = MAC_SYSTEM_KEYS.iter().find(|(k, _)| *k == chord)
        {
            return Some((*what).to_string());
        }
        self.bindings()
            .into_iter()
            .find(|(a, c)| *a != except && c == chord)
            .map(|(a, _)| a.info().label.to_string())
    }

    /// Gives `a` exactly `chords`; refused (nothing changes) when one is not allowed or already
    /// does something else, so a clash is shown before anything is saved.
    pub fn set(&mut self, a: Action, chords: &[String]) -> Result<(), String> {
        if !editable(a) {
            return Err(format!("{} cannot have a shortcut", a.info().label));
        }
        let mut out: Vec<String> = Vec::new();
        for c in chords {
            let c = normalize(c)?;
            if !allowed(&c) {
                return Err(format!(
                    "{c}: a shortcut needs Ctrl or Alt, or a function key (single letters pick tools)"
                ));
            }
            if let Some(other) = self.owner(&c, a) {
                return Err(format!("{c} is already the shortcut of {other}"));
            }
            if !out.contains(&c) {
                out.push(c);
            }
        }
        if out.len() > MAX_PER_COMMAND {
            return Err(format!("at most {MAX_PER_COMMAND} shortcuts per command"));
        }
        if out
            .iter()
            .map(String::as_str)
            .eq(defaults(a).iter().copied())
        {
            self.custom.remove(&a);
        } else {
            self.custom.insert(a, out);
        }
        Ok(())
    }

    /// Adds one shortcut to `a`.
    pub fn add(&mut self, a: Action, chord: &str) -> Result<String, String> {
        let c = normalize(chord)?;
        let mut all = self.chords(a);
        all.push(c.clone());
        self.set(a, &all)?;
        Ok(c)
    }

    /// Removes one shortcut of `a`.
    pub fn remove(&mut self, a: Action, chord: &str) {
        let all: Vec<String> = self.chords(a).into_iter().filter(|c| c != chord).collect();
        // Removing never adds a clash.
        let _ = self.set(a, &all);
    }

    /// Back to the default shortcuts of `a`; another command that took one of them loses it.
    pub fn reset(&mut self, a: Action) {
        self.custom.remove(&a);
        for d in defaults(a) {
            for (b, list) in self.custom.iter_mut() {
                if *b != a {
                    list.retain(|c| c != d);
                }
            }
        }
    }

    /// The changed commands as `{ commandKey: [shortcut…] }` (what Export writes).
    pub fn to_json(&self) -> Value {
        Value::Object(
            self.custom
                .iter()
                .map(|(a, k)| (a.info().key.to_string(), json!(k)))
                .collect::<Map<_, _>>(),
        )
    }

    /// A keymap from `{ commandKey: [shortcut…] }` (Import, `ui.set shortcuts`): every entry is
    /// checked as [`Keymap::set`] does; the first problem refuses the whole map.
    pub fn from_json(v: &Value) -> Result<Keymap, String> {
        let obj = v
            .as_object()
            .ok_or("shortcuts take an object { commandKey: [shortcut, …] }")?;
        let mut k = Keymap::default();
        // Commands whose defaults move elsewhere are cleared first, so swaps work.
        for key in obj.keys() {
            let a = Action::from_key(key).ok_or_else(|| format!("no command '{key}'"))?;
            k.custom.insert(a, Vec::new());
        }
        for (key, list) in obj {
            let a = Action::from_key(key).expect("checked above");
            let chords: Vec<String> = serde_json::from_value(list.clone())
                .map_err(|_| format!("{key}: give a list of shortcuts"))?;
            k.set(a, &chords)?;
        }
        // Commands left with exactly their defaults are not kept as changes.
        k.custom.retain(|a, list| {
            !list
                .iter()
                .map(String::as_str)
                .eq(defaults(*a).iter().copied())
        });
        Ok(k)
    }

    /// Like [`Keymap::from_json`], but for `ui.json`: a bad file gives the defaults.
    pub fn load(v: &Value) -> Keymap {
        if v.is_null() {
            return Keymap::default();
        }
        Keymap::from_json(v).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #834: in the browser Ctrl+N, Ctrl+T and Ctrl+W are the browser's, so none can be given
    /// to a command there (the reason says whose they are); the desktop is unchanged.
    #[test]
    fn the_browsers_own_keys_are_refused_only_in_the_browser() {
        let mut k = Keymap::default();
        assert!(k.add(Action::Rectangle, "Ctrl+T").is_ok(), "desktop");
        crate::platform::pretend_browser(true);
        let mut k = Keymap::default();
        for chord in ["Ctrl+N", "Ctrl+T", "Ctrl+W"] {
            let e = k.add(Action::Rectangle, chord).expect_err(chord);
            assert!(e.contains("the browser's"), "{e}");
        }
        assert!(k.add(Action::Rectangle, "Ctrl+Alt+R").is_ok());
        crate::platform::pretend_browser(false);
    }

    #[test]
    fn defaults_are_the_editors_own_shortcuts() {
        // Every Ctrl shortcut the ribbon shows is a default, listed first, except the clipboard
        // keys the system handles.
        for &a in Action::ALL {
            let Some(s) = a.info().shortcut else { continue };
            if !s.starts_with("Ctrl+") || matches!(a, Action::Cut | Action::Copy | Action::Paste) {
                continue;
            }
            // On macOS Flip Vertical and Center differ (#1288); the ribbon shows that platform's.
            assert_eq!(defaults_on(a, false).first(), Some(&s), "{a:?}");
            assert_eq!(
                defaults_on(a, true)
                    .first()
                    .map(|c| c.replace("Ctrl+", "Cmd+")),
                a.info().shortcut_for(true),
                "{a:?}"
            );
        }
        assert_eq!(defaults(Action::Delete), ["Delete", "Backspace"]);
        let k = Keymap::default();
        let mut seen: Vec<String> = Vec::new();
        for (a, c) in k.bindings() {
            assert_eq!(normalize(&c).as_deref(), Ok(c.as_str()), "{a:?}");
            assert!(allowed(&c) && !seen.contains(&c), "{c}");
            seen.push(c);
        }
        assert_eq!(
            seen.len(),
            DEFAULTS.iter().map(|d| d.1.len()).sum::<usize>()
        );
    }

    /// #1288: on Windows, Linux and in the browser, and on macOS, no two commands share a
    /// default key, none takes a reserved key or one the browser or macOS keeps for itself, and
    /// PE-DESIGN's object keys are where the coordinator's decision put them.
    #[test]
    fn no_two_defaults_collide_on_any_platform() {
        for mac in [false, true] {
            let mut seen: Vec<(&str, Action)> = Vec::new();
            for &a in Action::ALL {
                for &c in defaults_on(a, mac) {
                    assert_eq!(normalize(c).as_deref(), Ok(c), "{a:?} {c}");
                    if let Some((_, other)) = seen.iter().find(|(s, _)| *s == c) {
                        panic!("mac {mac}: {c} is {other:?}'s and {a:?}'s");
                    }
                    seen.push((c, a));
                    assert!(!RESERVED.iter().any(|(k, _)| *k == c), "{c} is reserved");
                    if mac {
                        assert!(!MAC_SYSTEM_KEYS.iter().any(|(k, _)| *k == c), "{c}: macOS");
                    } else if a != Action::New {
                        // New keeps Ctrl+N on the desktop; in the browser it is in the File
                        // menu instead (#834). No other default is a key the browser keeps.
                        assert!(!BROWSER_KEYS.iter().any(|(k, _)| *k == c), "{c}: browser");
                    }
                }
            }
            let of = |a| defaults_on(a, mac);
            assert_eq!(of(Action::FlipHorizontal), ["Ctrl+J"]);
            assert_eq!(of(Action::JoinLines), ["Ctrl+Shift+J"]);
            assert_eq!(
                (of(Action::FlipVertical), of(Action::CenterObjects)),
                if mac {
                    (&["Ctrl+Shift+H"][..], &["Ctrl+Shift+M"][..])
                } else {
                    (&["Ctrl+H"][..], &["Ctrl+M"][..])
                }
            );
            // PE-DESIGN has no shortcut for Rotate Right and Left: none is made up.
            assert!(of(Action::RotateRight).is_empty() && of(Action::RotateLeft).is_empty());
        }
    }

    #[test]
    fn records_checks_clashes_and_round_trips() {
        assert_eq!(normalize("shift+ctrl+s").as_deref(), Ok("Ctrl+Shift+S"));
        assert_eq!(normalize("Cmd+-").as_deref(), Ok("Ctrl+-"));
        assert_eq!(normalize("Ctrl++").as_deref(), Ok("Ctrl+Plus"));
        assert_eq!(normalize("alt+f5").as_deref(), Ok("Alt+F5"));
        assert!(normalize("Hyper+X").is_err() && normalize("Ctrl+Tab").is_err());
        let mut k = Keymap::default();
        assert!(k.add(Action::Rectangle, "R").is_err(), "letters pick tools");
        assert!(k.add(Action::Rectangle, "Ctrl+V").is_err(), "reserved");
        let e = k.add(Action::Rectangle, "Ctrl+S").expect_err("clash");
        assert!(e.contains("Save"), "{e}");
        assert_eq!(k, Keymap::default(), "a clash changes nothing");
        assert_eq!(k.add(Action::Rectangle, "alt+r").as_deref(), Ok("Alt+R"));
        assert_eq!(k.add(Action::Save, "F2").as_deref(), Ok("F2"));
        assert_eq!(k.chords(Action::Save), ["Ctrl+S", "F2"]);
        k.remove(Action::Redo, "Ctrl+Y");
        assert_eq!(k.chords(Action::Redo), ["Ctrl+Shift+Z"]);
        // Ctrl+Y is free now.
        k.add(Action::Rectangle, "Ctrl+Y").expect("free");
        let exported = k.to_json();
        assert_eq!(Keymap::from_json(&exported), Ok(k.clone()));
        // Resetting Redo takes Ctrl+Y back from the Rectangle.
        k.reset(Action::Redo);
        assert_eq!(k.chords(Action::Redo), ["Ctrl+Y", "Ctrl+Shift+Z"]);
        assert_eq!(k.chords(Action::Rectangle), ["Alt+R"]);
        // A swap in one import works; giving a command its defaults is no change.
        let swap = json!({ "undo": ["Ctrl+Y"], "redo": ["Ctrl+Z"], "save": ["Ctrl+S"] });
        let s = Keymap::from_json(&swap).expect("swap");
        assert_eq!(s.chords(Action::Undo), ["Ctrl+Y"]);
        assert!(!s.is_custom(Action::Save));
        assert!(Keymap::from_json(&json!({ "nope": [] })).is_err());
        assert_eq!(Keymap::load(&json!({ "save": "x" })), Keymap::default());
        assert!(!editable(Action::Copy) && editable(Action::Rectangle));
    }
}
