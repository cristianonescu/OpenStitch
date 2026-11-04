//! The Assistant panel's rules without egui (B17 A1, docs/research/beyond/ai-integration.md
//! section 4): the AI preferences kept in `ui.json`, the permission gate every command the
//! model sends passes through, and the record that becomes **Save as script**. The panel itself
//! is `assistant_panel.rs` in the binary; the Claude client is `openstitch-assistant`.

use openstitch_engine::Effect;
use serde_json::{Value, json};
use std::path::Path;

/// What the model may do without asking (Preferences ▸ AI ▸ Permissions).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PermissionMode {
    /// Every change and every file action waits for the user.
    Ask,
    /// Design and window changes run at once; files, machine transfer and quitting ask.
    #[default]
    Edits,
    /// Nothing asks.
    All,
}

impl PermissionMode {
    pub const ALL: [PermissionMode; 3] = [
        PermissionMode::Ask,
        PermissionMode::Edits,
        PermissionMode::All,
    ];

    pub fn key(self) -> &'static str {
        match self {
            PermissionMode::Ask => "ask",
            PermissionMode::Edits => "edits",
            PermissionMode::All => "all",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PermissionMode::Ask => "Ask every time",
            PermissionMode::Edits => "Auto-allow design edits",
            PermissionMode::All => "Allow everything",
        }
    }

    pub fn from_key(key: &str) -> Option<PermissionMode> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }
}

/// Effort levels the Claude API takes (`output_config.effort`).
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Preferences ▸ AI, kept in `ui.json` under `ai`. The API key is never here: it is in the
/// system keychain.
#[derive(Clone, Debug, PartialEq)]
pub struct AiPrefs {
    /// Master switch: off, no panel, button, shortcut or menu entry.
    pub enabled: bool,
    pub model: String,
    pub effort: String,
    /// The Claude API, or an Anthropic-compatible endpoint.
    pub base_url: String,
    pub mode: PermissionMode,
    /// Most design changes per message; 0 = no limit.
    pub edit_limit: u32,
    /// Commands the user chose **Always allow** for.
    pub always_allow: Vec<String>,
    /// Seconds one model request may take.
    pub request_timeout_s: u64,
}

pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

impl Default for AiPrefs {
    fn default() -> Self {
        AiPrefs {
            enabled: true,
            model: DEFAULT_MODEL.into(),
            effort: "high".into(),
            base_url: DEFAULT_BASE_URL.into(),
            mode: PermissionMode::default(),
            edit_limit: 0,
            always_allow: Vec::new(),
            request_timeout_s: 300,
        }
    }
}

impl AiPrefs {
    pub fn to_json(&self) -> Value {
        json!({
            "enabled": self.enabled, "model": self.model, "effort": self.effort,
            "baseUrl": self.base_url, "permissions": self.mode.key(),
            "editLimit": self.edit_limit, "alwaysAllow": self.always_allow,
            "requestTimeoutS": self.request_timeout_s,
        })
    }

    pub fn from_json(v: &Value) -> AiPrefs {
        let d = AiPrefs::default();
        let text = |k: &str, d: &str| {
            v[k].as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.len() <= 200)
                .unwrap_or(d)
                .to_string()
        };
        AiPrefs {
            enabled: v["enabled"].as_bool().unwrap_or(d.enabled),
            model: text("model", &d.model),
            effort: v["effort"]
                .as_str()
                .filter(|e| EFFORTS.contains(e))
                .unwrap_or(&d.effort)
                .to_string(),
            base_url: text("baseUrl", &d.base_url),
            mode: v["permissions"]
                .as_str()
                .and_then(PermissionMode::from_key)
                .unwrap_or(d.mode),
            edit_limit: v["editLimit"]
                .as_u64()
                .map_or(d.edit_limit, |n| n.min(10_000) as u32),
            always_allow: v["alwaysAllow"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .take(500)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            request_timeout_s: v["requestTimeoutS"]
                .as_u64()
                .map_or(d.request_timeout_s, |s| s.clamp(30, 3600)),
        }
    }
}

/// What the gate decides for one command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Run,
    /// Wait for Allow / Always allow / Deny; the text says what would happen.
    Ask(String),
    /// Refused, with the reason the model is told.
    Refuse(String),
}

/// Seconds a question waits before it is declined.
pub const APPROVAL_TIMEOUT_S: u64 = 240;

/// Parameters that hold a file or folder name.
const FILE_KEYS: [&str; 7] = [
    "path", "file", "dir", "folder", "output", "source", "target",
];

/// The real location of `p`: symlinks resolved on the file if it exists, else on its parent
/// folder (a file about to be written), as the engine's `Sandbox` does.
fn real_path(p: &Path) -> Option<std::path::PathBuf> {
    p.canonicalize().ok().or_else(|| {
        let parent = p.parent()?.canonicalize().ok()?;
        Some(parent.join(p.file_name()?))
    })
}

/// Whether `path` really lies inside `scratch`: absolute, no `..`, and inside after symlinks are
/// resolved, compared component by component (`Path::starts_with`, not a string prefix), so a
/// link in the scratch folder that points elsewhere is outside. Anything unresolvable is
/// outside, which only makes the gate ask.
pub fn inside(path: &str, scratch: Option<&Path>) -> bool {
    let Some(scratch) = scratch else {
        return false;
    };
    let p = Path::new(path);
    if !p.is_absolute()
        || p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return false;
    }
    match (real_path(p), scratch.canonicalize()) {
        (Some(real), Ok(root)) => real.starts_with(root),
        _ => false,
    }
}

/// The file names a command's parameters hold outside the scratch folder.
pub fn outside_files(params: &Value, scratch: Option<&Path>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(o) = params.as_object() {
        for k in FILE_KEYS {
            match o.get(k) {
                Some(Value::String(s)) if !inside(s, scratch) => out.push(s.clone()),
                Some(Value::Array(a)) => out.extend(
                    a.iter()
                        .filter_map(Value::as_str)
                        .filter(|s| !inside(s, scratch))
                        .map(str::to_string),
                ),
                _ => {}
            }
        }
    }
    out
}

/// Everything the gate looks at for one command.
pub struct GateInput<'a> {
    pub mode: PermissionMode,
    pub command: &'a str,
    pub effect: Effect,
    pub params: &'a Value,
    pub scratch: Option<&'a Path>,
    pub always_allow: &'a [String],
    /// Design changes already made in this message, and the limit (0 = none).
    pub edits: u32,
    pub edit_limit: u32,
    /// `history.undo` would undo something from before this message.
    pub undo_before_message: bool,
}

/// The permission gate (plan section 4.4). Commands are judged by their declared effect, with
/// files, machine transfer, quitting and undoing the user's own work behind a question.
pub fn verdict(g: &GateInput) -> Verdict {
    let changes = !matches!(g.effect, Effect::Read);
    if changes && g.edit_limit > 0 && g.edits >= g.edit_limit {
        return Verdict::Refuse(format!(
            "the limit of {} changes per message is reached; stop and tell the user what is left",
            g.edit_limit
        ));
    }
    if g.mode == PermissionMode::All || g.always_allow.iter().any(|c| c == g.command) {
        return Verdict::Run;
    }
    if g.command == "ui.scratchWrite" {
        return if g.mode == PermissionMode::Ask {
            Verdict::Ask("write a file into the assistant's scratch folder".into())
        } else {
            Verdict::Run
        };
    }
    let files = outside_files(g.params, g.scratch);
    let sensitive = if g.command.starts_with("transfer.") {
        Some("send the design to the machine or a USB drive".to_string())
    } else if g.effect == Effect::File {
        Some(if files.is_empty() {
            "read or write files".into()
        } else {
            format!("use {}", files.join(", "))
        })
    } else if !files.is_empty() {
        Some(format!("use {}", files.join(", ")))
    } else if g.command == "ui.run" && g.params["action"] == json!("exit") {
        Some("quit OpenStitch".into())
    } else if g.undo_before_message {
        Some("undo a change made before this message".into())
    } else {
        None
    };
    match sensitive {
        Some(what) => Verdict::Ask(what),
        None if changes && g.mode == PermissionMode::Ask => {
            Verdict::Ask("change the design".into())
        }
        None => Verdict::Run,
    }
}

/// The engine edits one message made, for **Save as script** (a `stitchtool exec` script).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScriptRecord {
    steps: Vec<Value>,
    /// A redo happened: the record no longer matches the design.
    broken: bool,
}

impl ScriptRecord {
    /// Records one successful command; `ui.*`, reads and files are not part of a script.
    pub fn record(&mut self, command: &str, effect: Effect, params: &Value) {
        match (command, effect) {
            ("history.undo", _) => {
                self.steps.pop();
            }
            ("history.redo", _) => self.broken = true,
            (c, Effect::Edit) if !c.starts_with("ui.") => self.steps.push(json!({
                "command": c,
                "params": if params.is_object() { params.clone() } else { json!({}) },
            })),
            _ => {}
        }
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// The script, or why there is none.
    pub fn script(&self) -> Result<Value, &'static str> {
        if self.broken {
            Err(
                "a redo happened in this conversation, so its commands no longer rebuild the design",
            )
        } else if self.steps.is_empty() {
            Err("the assistant has not changed the design yet")
        } else {
            Ok(Value::Array(self.steps.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An absolute scratch folder on every OS (`/tmp/...` is not absolute on Windows). It must
    /// exist: the gate resolves symlinks.
    fn scratch() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("openstitch-gate-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    fn in_scratch(name: &str) -> String {
        scratch().join(name).display().to_string()
    }

    fn gate(mode: PermissionMode, command: &str, effect: Effect, params: Value) -> Verdict {
        let scratch = scratch();
        verdict(&GateInput {
            mode,
            command,
            effect,
            params: &params,
            scratch: Some(&scratch),
            always_allow: &["design.export".to_string()],
            edits: 0,
            edit_limit: 0,
            undo_before_message: false,
        })
    }

    #[test]
    fn the_gate_follows_the_plan() {
        use PermissionMode::*;
        let none = json!({});
        // Reads always run.
        assert_eq!(
            gate(Ask, "design.info", Effect::Read, none.clone()),
            Verdict::Run
        );
        // Edits: run by default, ask in Ask mode.
        assert_eq!(
            gate(Edits, "shape.rectangle", Effect::Edit, none.clone()),
            Verdict::Run
        );
        assert!(matches!(
            gate(Ask, "shape.rectangle", Effect::Edit, none.clone()),
            Verdict::Ask(_)
        ));
        // Files ask, unless always allowed or All.
        assert!(matches!(
            gate(Edits, "project.save", Effect::File, json!({ "path": "/home/me/a.osd" })),
            Verdict::Ask(w) if w.contains("/home/me/a.osd")
        ));
        assert_eq!(
            gate(
                Edits,
                "design.export",
                Effect::File,
                json!({ "path": "/x.pes" })
            ),
            Verdict::Run
        );
        assert_eq!(
            gate(All, "project.save", Effect::File, none.clone()),
            Verdict::Run
        );
        // An edit that reads a file outside the scratch folder asks; inside it runs.
        assert!(matches!(
            gate(
                Edits,
                "import.svg",
                Effect::Edit,
                json!({ "path": "/home/me/logo.svg" })
            ),
            Verdict::Ask(_)
        ));
        assert_eq!(
            gate(
                Edits,
                "import.svg",
                Effect::Edit,
                json!({ "path": in_scratch("logo.svg") })
            ),
            Verdict::Run
        );
        assert!(matches!(
            gate(
                Edits,
                "import.svg",
                Effect::Edit,
                json!({ "path": in_scratch("../etc/x.svg") })
            ),
            Verdict::Ask(_)
        ));
        // Scratch writes, transfer and quitting.
        assert_eq!(
            gate(Edits, "ui.scratchWrite", Effect::File, none.clone()),
            Verdict::Run
        );
        assert!(matches!(
            gate(Edits, "transfer.send", Effect::File, none.clone()),
            Verdict::Ask(_)
        ));
        assert!(matches!(
            gate(Edits, "ui.run", Effect::Edit, json!({ "action": "exit" })),
            Verdict::Ask(w) if w.contains("quit")
        ));
        assert_eq!(
            gate(
                Edits,
                "ui.run",
                Effect::Edit,
                json!({ "action": "stitchView" })
            ),
            Verdict::Run
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_the_scratch_folder_that_points_elsewhere_is_outside() {
        let dir = scratch();
        let outside =
            std::env::temp_dir().join(format!("openstitch-gate-outside-{}", std::process::id()));
        std::fs::create_dir_all(&outside).expect("outside");
        std::fs::write(outside.join("secret.svg"), "<svg/>").expect("file");
        let link = dir.join("link");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&outside, &link).expect("symlink");
        let via_link = link.join("secret.svg").display().to_string();
        assert!(!inside(&via_link, Some(&dir)), "{via_link}");
        std::fs::write(dir.join("real.svg"), "<svg/>").expect("file");
        assert!(inside(
            &dir.join("real.svg").display().to_string(),
            Some(&dir)
        ));
        // Not written yet: resolved through its parent.
        assert!(inside(
            &dir.join("new.svg").display().to_string(),
            Some(&dir)
        ));
        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn the_limit_and_undoing_earlier_work() {
        let p = json!({});
        let mut g = GateInput {
            mode: PermissionMode::All,
            command: "shape.rectangle",
            effect: Effect::Edit,
            params: &p,
            scratch: None,
            always_allow: &[],
            edits: 3,
            edit_limit: 3,
            undo_before_message: false,
        };
        assert!(matches!(verdict(&g), Verdict::Refuse(_)));
        g.effect = Effect::Read;
        assert_eq!(verdict(&g), Verdict::Run);
        g.mode = PermissionMode::Edits;
        g.edit_limit = 0;
        g.command = "history.undo";
        g.effect = Effect::History;
        g.undo_before_message = true;
        assert!(matches!(verdict(&g), Verdict::Ask(_)));
    }

    #[test]
    fn prefs_round_trip_and_reject_junk() {
        let p = AiPrefs {
            mode: PermissionMode::Ask,
            edit_limit: 24,
            always_allow: vec!["project.save".into()],
            ..AiPrefs::default()
        };
        assert_eq!(AiPrefs::from_json(&p.to_json()), p);
        let junk = AiPrefs::from_json(&json!({
            "effort": "extreme", "permissions": "root", "requestTimeoutS": 1, "model": ""
        }));
        assert_eq!(junk.effort, "high");
        assert_eq!(junk.mode, PermissionMode::Edits);
        assert_eq!(junk.request_timeout_s, 30);
        assert_eq!(junk.model, DEFAULT_MODEL);
        assert!(
            AiPrefs::default().enabled,
            "on by default (owner, 2026-11-04)"
        );
    }

    #[test]
    fn the_script_keeps_engine_edits_only() {
        let mut r = ScriptRecord::default();
        assert!(r.script().is_err());
        r.record("design.new", Effect::Edit, &json!({ "width": 100 }));
        r.record("shape.rectangle", Effect::Edit, &json!({ "x": 1 }));
        r.record("ui.set", Effect::Edit, &json!({}));
        r.record("design.info", Effect::Read, &json!({}));
        r.record("history.undo", Effect::History, &json!({}));
        assert_eq!(
            r.script().expect("script"),
            json!([{ "command": "design.new", "params": { "width": 100 } }])
        );
        r.record("history.redo", Effect::History, &json!({}));
        assert!(r.script().is_err());
    }
}
