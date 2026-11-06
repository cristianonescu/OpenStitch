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

/// The wire protocol of the address in the settings (A2, #317).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Protocol {
    /// The Claude API's Messages API, or a compatible endpoint.
    #[default]
    Anthropic,
    /// `/chat/completions` (Ollama, LM Studio, OpenRouter, OpenAI and others).
    OpenAi,
    /// A coding agent on the user's own account, over the Agent Client Protocol (A3, #338).
    Agent,
}

impl Protocol {
    pub const ALL: [Protocol; 3] = [Protocol::Anthropic, Protocol::OpenAi, Protocol::Agent];

    pub fn key(self) -> &'static str {
        match self {
            Protocol::Anthropic => "anthropic",
            Protocol::OpenAi => "openai",
            Protocol::Agent => "agent",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Protocol::Anthropic => "Anthropic (Claude API)",
            Protocol::OpenAi => "OpenAI-compatible",
            Protocol::Agent => "Coding agent (Claude Code, Codex, …)",
        }
    }

    pub fn from_key(key: &str) -> Option<Protocol> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }
}

/// A ready-made endpoint in the settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    pub name: &'static str,
    pub protocol: Protocol,
    pub base_url: &'static str,
    /// The model it starts with; empty: pick one from the endpoint's list.
    pub model: &'static str,
}

/// The presets of plan section 4.3 and issue #317 (addresses from SolidCraft's
/// `docs/agent-chat.md` and each server's documentation).
pub const PRESETS: [Preset; 4] = [
    Preset {
        name: "Claude API",
        protocol: Protocol::Anthropic,
        base_url: DEFAULT_BASE_URL,
        model: DEFAULT_MODEL,
    },
    Preset {
        name: "Ollama (this computer)",
        protocol: Protocol::OpenAi,
        base_url: "http://localhost:11434/v1",
        model: "",
    },
    Preset {
        name: "LM Studio (this computer)",
        protocol: Protocol::OpenAi,
        base_url: "http://localhost:1234/v1",
        model: "",
    },
    Preset {
        name: "OpenRouter",
        protocol: Protocol::OpenAi,
        base_url: "https://openrouter.ai/api/v1",
        model: "",
    },
];

/// What choosing `preset` in Endpoint does to the settings (#432). A preset with its own model
/// takes it. One without (Ollama, LM Studio, OpenRouter) clears a model that came from another
/// endpoint, such as the Claude model, which no local server has: a message sent with it would
/// fail with a 404. Returns whether the model is now empty, so the panel picks one (a local
/// endpoint) or points at **List**.
pub fn apply_preset(prefs: &mut AiPrefs, preset: &Preset) -> bool {
    let moved = prefs.base_url != preset.base_url || prefs.protocol != preset.protocol;
    prefs.protocol = preset.protocol;
    prefs.base_url = preset.base_url.to_string();
    if !preset.model.is_empty() {
        prefs.model = preset.model.to_string();
        return false;
    }
    if moved || prefs.model == DEFAULT_MODEL {
        prefs.model.clear();
    }
    prefs.model.trim().is_empty()
}

/// Whether `url` is plain http or https to exactly this computer (localhost, 127.0.0.1, [::1]),
/// where a local model server needs no key.
pub fn is_this_computer(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    let Some(rest) = u
        .strip_prefix("http://")
        .or_else(|| u.strip_prefix("https://"))
    else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return false;
    }
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']')
            .next()
            .map(|h| format!("[{h}]"))
            .unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or_default().to_string()
    };
    matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]")
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
    pub protocol: Protocol,
    /// OpenAI-compatible endpoints only: sent as `reasoning_effort` when not empty.
    pub reasoning_effort: String,
    /// The coding agent (`openstitch_assistant::KNOWN_AGENTS` id) for [`Protocol::Agent`].
    pub agent: String,
    /// The optional image generator (A4): an OpenAI-compatible `/images/generations` address
    /// ending in `/v1`; empty = none. Its key, if any, is in the keychain under this address.
    pub image_url: String,
    pub image_model: String,
    /// The Assistant panel is open (#422): open on first start, then as the user left it. An
    /// old `ui.json` without the field reads as open.
    pub panel_open: bool,
    /// Closing the panel says how to reopen it, until "Don't show this again".
    pub close_hint: bool,
    /// "Let the Assistant remember things" (#491): the notes go into each conversation's system
    /// prompt and `notes_write` may change them. Off: neither.
    pub memory: bool,
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
            protocol: Protocol::Anthropic,
            reasoning_effort: String::new(),
            agent: "claude".into(),
            image_url: String::new(),
            image_model: String::new(),
            panel_open: true,
            close_hint: true,
            memory: true,
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
            "protocol": self.protocol.key(), "reasoningEffort": self.reasoning_effort,
            "agent": self.agent,
            "imageUrl": self.image_url, "imageModel": self.image_model,
            "panelOpen": self.panel_open, "closeHint": self.close_hint,
            "memory": self.memory,
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
        let protocol = v["protocol"]
            .as_str()
            .and_then(Protocol::from_key)
            .unwrap_or(d.protocol);
        AiPrefs {
            enabled: v["enabled"].as_bool().unwrap_or(d.enabled),
            // #432: an OpenAI-compatible endpoint keeps an empty model (pick one from its list)
            // instead of getting the Claude model back, which no local server has.
            model: match v["model"].as_str().map(str::trim) {
                Some("") if protocol == Protocol::OpenAi => String::new(),
                _ => text("model", &d.model),
            },
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
            protocol,
            reasoning_effort: v["reasoningEffort"]
                .as_str()
                .map(str::trim)
                .filter(|s| {
                    s.len() <= 40 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                })
                .unwrap_or_default()
                .to_string(),
            // "claude-agent-acp" was the default before #386: the program, not the id.
            agent: match text("agent", &d.agent).as_str() {
                "claude-agent-acp" => d.agent.clone(),
                a => a.to_string(),
            },
            image_url: text("imageUrl", ""),
            image_model: text("imageModel", ""),
            panel_open: v["panelOpen"].as_bool().unwrap_or(d.panel_open),
            close_hint: v["closeHint"].as_bool().unwrap_or(d.close_hint),
            memory: v["memory"].as_bool().unwrap_or(d.memory),
        }
    }
}

/// What the closing dialog says (#422): where the Assistant comes back, with the shortcut as
/// this platform shows it, and the Options switch when AI features are off.
pub fn reopen_hint(shortcut: Option<&str>, ai_enabled: bool) -> Vec<String> {
    let mut lines = vec!["View ▸ Show/Hide ▸ Assistant".to_string()];
    if let Some(s) = shortcut {
        lines.push(format!("or press {s}"));
    }
    if !ai_enabled {
        lines.insert(
            0,
            "Options ▸ Show AI features (AI features are off), then".to_string(),
        );
    }
    lines
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

/// A file name with no folder part, such as `heart.svg`: one normal path component, no
/// separator of either platform, not hidden (the scratch folder's own `.lock`).
fn bare_name(s: &str) -> bool {
    let mut parts = Path::new(s).components();
    matches!(parts.next(), Some(std::path::Component::Normal(_)))
        && parts.next().is_none()
        && !s.contains(['/', '\\', ':'])
        && !s.starts_with('.')
}

/// #431: a bare file name the model gave (`import.svg {"path": "heart.svg"}` right after
/// `scratch_write heart.svg`) becomes that file's absolute path in the scratch folder, when the
/// file exists there and really lies inside it ([`inside`]: a symlink pointing out stays a bare
/// name, which the gate then asks about). Run before the gate and before the command, so both see
/// the same path and the engine does not look in the editor's working folder. Returns whether
/// anything changed.
pub fn resolve_scratch_names(params: &mut Value, scratch: Option<&Path>) -> bool {
    let Some(dir) = scratch else {
        return false;
    };
    let resolve = |v: &mut Value| -> bool {
        let Some(s) = v.as_str() else {
            return false;
        };
        if !bare_name(s) {
            return false;
        }
        let full = dir.join(s);
        let full_text = full.display().to_string();
        if full.is_file() && inside(&full_text, Some(dir)) {
            *v = Value::String(full_text);
            return true;
        }
        false
    };
    let mut changed = false;
    if let Some(o) = params.as_object_mut() {
        for k in FILE_KEYS {
            match o.get_mut(k) {
                Some(Value::Array(a)) => {
                    for v in a {
                        changed |= resolve(v);
                    }
                }
                Some(v) => changed |= resolve(v),
                None => {}
            }
        }
    }
    changed
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
    /// `ui.layout` names a layout the user already saved (#551).
    pub existing_layout: bool,
}

/// What the gate asks before preferences the user set by hand are thrown away (#551): they have
/// no undo. None for other commands.
pub fn prefs_question(command: &str, params: &Value, existing_layout: bool) -> Option<String> {
    let text = |k: &str| params[k].as_str().unwrap_or_default().trim().to_string();
    match command {
        "ui.resetPrefs" => {
            let area = text("area");
            let what = super::prefs::RESET_AREAS
                .iter()
                .find(|a| a.0 == area)
                .map_or(String::new(), |a| format!(" ({})", a.2));
            Some(format!(
                "reset your {area} preferences to their defaults{what}"
            ))
        }
        "ui.layout" => match params["op"].as_str() {
            Some("delete") => Some(format!("delete your saved layout \"{}\"", text("name"))),
            Some("save") if existing_layout => Some(format!(
                "replace your saved layout \"{}\" with the current one",
                text("name")
            )),
            _ => None,
        },
        "ui.set"
            if params["name"] == json!("shortcuts")
                && params["value"].as_object().is_some_and(|m| m.is_empty()) =>
        {
            Some("reset every keyboard shortcut you changed to its default".into())
        }
        _ => None,
    }
}

/// What the gate asks before the Assistant's memory changes (#491), or None for other commands.
pub fn memory_question(command: &str, params: &Value) -> Option<String> {
    let text = |k: &str| params[k].as_str().unwrap_or_default().trim().to_string();
    match command {
        "ui.notesWrite" => {
            let replace = params["mode"] == json!("replace");
            let body = text("text");
            let shown: String = body.chars().take(300).collect();
            let more = if shown.len() < body.len() { "…" } else { "" };
            Some(match (replace, body.is_empty()) {
                (true, true) => format!("forget its {} notes", text("section")),
                (true, false) => format!("rewrite its {} notes as: {shown}{more}", text("section")),
                _ => format!("remember ({}): {shown}{more}", text("section")),
            })
        }
        "ui.notesClear" => Some("forget everything it remembers about you".into()),
        // A folder the editor reads fonts from is the user's to choose (#529).
        "ui.set" if params["name"] == json!("inkStitchFolder") => Some(
            match params["value"].as_str().map(str::trim).unwrap_or_default() {
                "" => "forget the Ink/Stitch fonts folder".into(),
                p => format!(
                    "use {} as the Ink/Stitch fonts folder",
                    p.chars().take(200).collect::<String>()
                ),
            },
        ),
        "ui.set" if params["name"] == json!("assistantMemory") => {
            Some(if params["value"] == json!(true) {
                "turn its memory on".into()
            } else {
                "turn its memory off".into()
            })
        }
        _ => None,
    }
}

/// What the gate asks before the user's custom fabrics change (#453): they live in the user's
/// settings, with no undo. None for other commands.
pub fn fabric_question(command: &str, params: &Value) -> Option<String> {
    let text = |k: &str| params[k].as_str().unwrap_or_default().trim().to_string();
    match command {
        "fabric.calibrate" => Some(format!(
            "save the custom fabric \"{}\" from the {} calibration card (fill F{}, satin S{}, pull P{}; one of the same name is replaced)",
            text("name"),
            text("fabric"),
            params["fill"].as_u64().unwrap_or(3),
            params["satin"].as_u64().unwrap_or(3),
            params["pull"].as_u64().unwrap_or(3),
        )),
        "fabric.deleteCustom" => Some(format!("delete the custom fabric {}", text("fabric"))),
        _ => None,
    }
}

/// What the gate asks before the user's own thread charts change (#495), or None for other
/// commands: a chart built from a photo of spools or a list is the user's, so it is made only
/// with their yes.
pub fn chart_question(command: &str, effect: Effect, params: &Value) -> Option<String> {
    if effect != Effect::Settings || !command.starts_with("threads.") {
        return None;
    }
    let text = |k: &str| params[k].as_str().unwrap_or_default().trim().to_string();
    let chart = if params["chart"].is_string() {
        text("chart")
    } else {
        text("name")
    };
    Some(match command {
        "threads.createChart" => format!("create the thread chart \"{chart}\""),
        "threads.addColor" => {
            let what = [text("code"), text("description"), text("color")]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            format!("add {what} to your thread chart \"{chart}\"")
        }
        _ if chart.is_empty() => format!("change your thread charts ({command})"),
        _ => format!("change your thread chart \"{chart}\" ({command})"),
    })
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
    // #491: the memory changes only with the user's yes, in every mode but Allow everything.
    if let Some(what) = memory_question(g.command, g.params) {
        return Verdict::Ask(what);
    }
    // #551: so do preferences that cannot be undone.
    if let Some(what) = prefs_question(g.command, g.params, g.existing_layout) {
        return Verdict::Ask(what);
    }
    // #495: so do the user's thread charts.
    if let Some(what) = chart_question(g.command, g.effect, g.params) {
        return Verdict::Ask(what);
    }
    // #453: so do the user's custom fabrics.
    if let Some(what) = fabric_question(g.command, g.params) {
        return Verdict::Ask(what);
    }
    // #549: deleting autosave snapshots asks, naming what goes.
    if g.command == "ui.recoveryDiscard" {
        return Verdict::Ask(match g.params.get("id").and_then(Value::as_str) {
            Some(id) => format!(
                "delete the recovery copy {id} (a design a crashed window left; it cannot be restored afterwards)"
            ),
            None => "delete every recovery copy crashed windows left (Clear Recovery Files; they cannot be restored afterwards)".into(),
        });
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
        thread_local! {
            // One folder per test (each test runs on its own thread), removed when it ends.
            static SCRATCH: openstitch_testutil::TempDir =
                openstitch_testutil::TempDir::new("gate-test");
        }
        SCRATCH.with(|d| d.to_path_buf())
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
            existing_layout: false,
        })
    }

    #[test]
    fn resetting_or_deleting_preferences_asks() {
        use PermissionMode::*;
        let ask = |mode, command, params: Value| match gate(mode, command, Effect::Edit, params) {
            Verdict::Ask(w) => w,
            v => panic!("{command}: {v:?}"),
        };
        for mode in [Ask, Edits] {
            let w = ask(mode, "ui.resetPrefs", json!({ "area": "shortcuts" }));
            assert!(
                w.contains("shortcuts") && w.contains("changed shortcut"),
                "{w}"
            );
            let w = ask(
                mode,
                "ui.layout",
                json!({ "op": "delete", "name": "Lettering" }),
            );
            assert!(w.contains("delete") && w.contains("Lettering"), "{w}");
            let w = ask(mode, "ui.set", json!({ "name": "shortcuts", "value": {} }));
            assert!(w.contains("every keyboard shortcut"), "{w}");
        }
        // A save over an existing name asks; a new name, list and apply run.
        let scratch = scratch();
        let save = json!({ "op": "save", "name": "Lettering" });
        let g = |existing_layout| GateInput {
            mode: Edits,
            command: "ui.layout",
            effect: Effect::Edit,
            params: &save,
            scratch: Some(&scratch),
            always_allow: &[],
            edits: 0,
            edit_limit: 0,
            undo_before_message: false,
            existing_layout,
        };
        assert!(matches!(verdict(&g(true)), Verdict::Ask(w) if w.contains("replace")));
        assert_eq!(verdict(&g(false)), Verdict::Run);
        for params in [
            json!({ "op": "list" }),
            json!({ "op": "apply", "name": "x" }),
        ] {
            assert_eq!(gate(Edits, "ui.layout", Effect::Edit, params), Verdict::Run);
        }
        assert_eq!(
            gate(
                Edits,
                "ui.set",
                Effect::Edit,
                json!({ "name": "shortcuts", "value": { "rectangle": ["Alt+R"] } })
            ),
            Verdict::Run
        );
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
        // #549: deleting recovery copies always asks and names what goes, whatever the
        // command's declared effect.
        for mode in [Ask, Edits] {
            for effect in [Effect::File, Effect::Edit, Effect::Read] {
                assert!(matches!(
                    gate(mode, "ui.recoveryDiscard", effect, json!({ "all": true })),
                    Verdict::Ask(w) if w.contains("delete every recovery copy")
                ));
                assert!(matches!(
                    gate(mode, "ui.recoveryDiscard", effect, json!({ "id": "4242-0" })),
                    Verdict::Ask(w) if w.contains("delete the recovery copy 4242-0")
                ));
            }
        }
        assert_eq!(
            gate(
                All,
                "ui.recoveryDiscard",
                Effect::File,
                json!({ "all": true })
            ),
            Verdict::Run
        );
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
        let outside = openstitch_testutil::TempDir::new("gate-outside");
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

    /// #453: custom fabrics are saved or deleted only after the user's yes (no undo), and the
    /// swatch files are written only after it (File effect), in Ask and Edits mode.
    #[test]
    fn custom_fabric_changes_and_swatch_files_ask_first() {
        use PermissionMode::*;
        for mode in [Ask, Edits] {
            assert!(matches!(
                gate(
                    mode,
                    "fabric.calibrate",
                    Effect::Settings,
                    json!({ "fabric": "knit", "name": "My jersey", "fill": 2, "pull": 5 })
                ),
                Verdict::Ask(w) if w.contains("My jersey") && w.contains("F2") && w.contains("P5")
            ));
            assert!(matches!(
                gate(mode, "fabric.deleteCustom", Effect::Settings, json!({ "fabric": "custom-my-jersey" })),
                Verdict::Ask(w) if w.contains("custom-my-jersey")
            ));
            assert!(matches!(
                gate(
                    mode,
                    "design.testSwatches",
                    Effect::File,
                    json!({ "id": 1, "path": "/home/me/s.pes" })
                ),
                Verdict::Ask(_)
            ));
        }
        assert_eq!(
            gate(
                All,
                "fabric.calibrate",
                Effect::Settings,
                json!({ "name": "x" })
            ),
            Verdict::Run
        );
    }

    /// #495: a thread chart (from a photo of the user's spools, say) is created or changed only
    /// after the user's yes, in Ask and Edits mode; reading charts and remapping the design to
    /// one are not asked about here.
    #[test]
    fn thread_chart_changes_ask_first() {
        use PermissionMode::*;
        for mode in [Ask, Edits] {
            assert_eq!(
                gate(
                    mode,
                    "threads.createChart",
                    Effect::Settings,
                    json!({ "name": "My shelf" })
                ),
                Verdict::Ask("create the thread chart \"My shelf\"".into())
            );
            assert_eq!(
                gate(
                    mode,
                    "threads.addColor",
                    Effect::Settings,
                    json!({ "chart": "My shelf", "color": "#c8102e", "code": "7101", "description": "Holly Red" })
                ),
                Verdict::Ask("add 7101 Holly Red #c8102e to your thread chart \"My shelf\"".into())
            );
            assert!(matches!(
                gate(mode, "threads.removeColor", Effect::Settings, json!({ "chart": "My shelf", "index": 1 })),
                Verdict::Ask(w) if w.contains("My shelf") && w.contains("threads.removeColor")
            ));
        }
        assert_eq!(
            gate(
                All,
                "threads.createChart",
                Effect::Settings,
                json!({ "name": "x" })
            ),
            Verdict::Run
        );
        assert_eq!(
            gate(Edits, "threads.charts", Effect::Read, json!({})),
            Verdict::Run
        );
        assert_eq!(
            gate(
                Edits,
                "threads.remap",
                Effect::Edit,
                json!({ "chart": "My shelf" })
            ),
            Verdict::Run
        );
    }

    /// #491: writing or clearing the notes, and the memory switch, ask in Ask and Edits mode
    /// and say what would be remembered; Always allow and Allow everything run them.
    #[test]
    fn memory_changes_always_ask() {
        use PermissionMode::*;
        let write = json!({ "section": "Threads", "text": "Madeira Polyneon 40" });
        for mode in [Ask, Edits] {
            assert_eq!(
                gate(mode, "ui.notesWrite", Effect::Settings, write.clone()),
                Verdict::Ask("remember (Threads): Madeira Polyneon 40".into())
            );
            assert!(matches!(
                gate(mode, "ui.notesClear", Effect::Settings, json!({})),
                Verdict::Ask(w) if w.contains("forget everything")
            ));
            assert!(matches!(
                gate(mode, "ui.set", Effect::Edit, json!({ "name": "assistantMemory", "value": true })),
                Verdict::Ask(w) if w == "turn its memory on"
            ));
            // #529: the Ink/Stitch fonts folder is a path the editor reads from: asked.
            assert!(matches!(
                gate(mode, "ui.set", Effect::Edit, json!({ "name": "inkStitchFolder", "value": "fonts" })),
                Verdict::Ask(w) if w == "use fonts as the Ink/Stitch fonts folder"
            ));
        }
        assert!(matches!(
            gate(Edits, "ui.notesWrite", Effect::Settings,
                json!({ "section": "Lessons", "text": "", "mode": "replace" })),
            Verdict::Ask(w) if w == "forget its Lessons notes"
        ));
        assert_eq!(
            gate(All, "ui.notesWrite", Effect::Settings, write.clone()),
            Verdict::Run
        );
        // Reading the notes and other settings do not ask.
        assert_eq!(
            gate(Edits, "ui.notesRead", Effect::Read, json!({})),
            Verdict::Run
        );
        assert_eq!(
            gate(
                Edits,
                "ui.set",
                Effect::Edit,
                json!({ "name": "grid", "value": true })
            ),
            Verdict::Run
        );
        let scratch = scratch();
        let allowed = ["ui.notesWrite".to_string()];
        assert_eq!(
            verdict(&GateInput {
                mode: Edits,
                command: "ui.notesWrite",
                effect: Effect::Settings,
                params: &write,
                scratch: Some(&scratch),
                always_allow: &allowed,
                edits: 0,
                edit_limit: 0,
                undo_before_message: false,
                existing_layout: false,
            }),
            Verdict::Run
        );
    }

    /// #431: `heart.svg` right after scratch_write is the scratch folder's file, so the gate
    /// runs it without a question; names with folders, missing files and hidden files are left
    /// as they are, and the gate asks about them as before.
    #[test]
    fn a_bare_name_of_a_scratch_file_becomes_its_path() {
        let dir = scratch();
        std::fs::write(dir.join("heart.svg"), "<svg/>").expect("file");
        std::fs::write(dir.join(".lock"), "").expect("file");
        let mut p = json!({ "path": "heart.svg", "x": 1 });
        assert!(resolve_scratch_names(&mut p, Some(&dir)));
        assert_eq!(p["path"], json!(in_scratch("heart.svg")));
        assert_eq!(p["x"], json!(1));
        assert_eq!(
            gate(PermissionMode::Edits, "import.svg", Effect::Edit, p),
            Verdict::Run
        );
        // Lists too.
        let mut l = json!({ "files": ["heart.svg"], "source": ["heart.svg", "missing.svg"] });
        assert!(resolve_scratch_names(&mut l, Some(&dir)));
        assert_eq!(l["files"], json!(["heart.svg"]), "not a file key");
        assert_eq!(l["source"], json!([in_scratch("heart.svg"), "missing.svg"]));
        for left in [
            "missing.svg",
            "sub/heart.svg",
            "..\\heart.svg",
            "../heart.svg",
            ".lock",
            "..",
            "C:heart.svg",
        ] {
            let mut p = json!({ "path": left });
            assert!(!resolve_scratch_names(&mut p, Some(&dir)), "{left}");
            assert_eq!(p["path"], json!(left));
        }
        let mut p = json!({ "path": "heart.svg" });
        assert!(!resolve_scratch_names(&mut p, None), "no scratch folder");
        assert!(matches!(
            gate(PermissionMode::Edits, "import.svg", Effect::Edit, p),
            Verdict::Ask(_)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_bare_name_that_links_out_of_the_scratch_folder_still_asks() {
        let dir = scratch();
        let outside = openstitch_testutil::TempDir::new("gate-outside-431");
        std::fs::write(outside.join("secret.svg"), "<svg/>").expect("file");
        let link = dir.join("away.svg");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(outside.join("secret.svg"), &link).expect("symlink");
        let mut p = json!({ "path": "away.svg" });
        assert!(!resolve_scratch_names(&mut p, Some(&dir)));
        assert!(matches!(
            gate(PermissionMode::Edits, "import.svg", Effect::Edit, p),
            Verdict::Ask(_)
        ));
        let _ = std::fs::remove_file(&link);
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
            existing_layout: false,
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

    /// #432: choosing Ollama or LM Studio drops the Claude model; the Claude API preset brings
    /// it back; staying on the same endpoint keeps the model the user picked.
    #[test]
    fn a_local_preset_does_not_keep_the_claude_model() {
        let mut p = AiPrefs::default();
        let [claude, ollama, lm_studio, openrouter] = PRESETS;
        assert!(apply_preset(&mut p, &ollama), "pick a model");
        assert_eq!(
            (p.protocol, p.base_url.as_str(), p.model.as_str()),
            (Protocol::OpenAi, "http://localhost:11434/v1", "")
        );
        // An empty OpenAI-compatible model survives a restart (it used to read back as Claude).
        assert_eq!(AiPrefs::from_json(&p.to_json()).model, "");
        p.model = "qwen3:8b".into();
        assert!(!apply_preset(&mut p, &ollama), "same endpoint: kept");
        assert_eq!(p.model, "qwen3:8b");
        assert!(apply_preset(&mut p, &lm_studio), "another server: cleared");
        assert!(apply_preset(&mut p, &openrouter));
        assert!(!apply_preset(&mut p, &claude));
        assert_eq!(p.model, DEFAULT_MODEL);
        assert_eq!(p.protocol, Protocol::Anthropic);
        // The Claude API never keeps an empty model.
        let back = AiPrefs::from_json(&json!({ "protocol": "anthropic", "model": "" }));
        assert_eq!(back.model, DEFAULT_MODEL);
        // Memory (#491) is on by default and its switch is kept.
        assert!(AiPrefs::default().memory);
        let off = AiPrefs {
            memory: false,
            ..AiPrefs::default()
        };
        assert_eq!(AiPrefs::from_json(&off.to_json()), off);
    }

    #[test]
    fn protocols_presets_and_this_computer() {
        let p = AiPrefs {
            protocol: Protocol::OpenAi,
            reasoning_effort: "high".into(),
            ..AiPrefs::default()
        };
        assert_eq!(AiPrefs::from_json(&p.to_json()), p);
        assert_eq!(AiPrefs::default().protocol, Protocol::Anthropic);
        let images = AiPrefs {
            image_url: "http://localhost:8080/v1".into(),
            image_model: "flux".into(),
            ..AiPrefs::default()
        };
        assert_eq!(AiPrefs::from_json(&images.to_json()), images);
        assert!(
            AiPrefs::default().image_url.is_empty(),
            "no generator unless chosen"
        );
        // The panel opens by default, and an old ui.json without the fields reads as open
        // with the closing hint on (#422).
        let old = AiPrefs::from_json(&json!({ "model": "claude-opus-5-5" }));
        assert!(old.panel_open && old.close_hint);
        let closed = AiPrefs {
            panel_open: false,
            close_hint: false,
            ..AiPrefs::default()
        };
        assert_eq!(AiPrefs::from_json(&closed.to_json()), closed);
        let junk = AiPrefs::from_json(&json!({ "protocol": "smtp", "reasoningEffort": "x y" }));
        assert_eq!(
            (junk.protocol, junk.reasoning_effort.as_str()),
            (Protocol::Anthropic, "")
        );
        for ok in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:1234/v1",
            "http://[::1]:8080/v1",
            "https://localhost/v1",
        ] {
            assert!(is_this_computer(ok), "{ok}");
        }
        for no in [
            "https://openrouter.ai/api/v1",
            "http://localhost.evil.com/v1",
            "http://user@evil.com@localhost/v1",
            "http://127.0.0.2/v1",
            "ftp://localhost/",
        ] {
            assert!(!is_this_computer(no), "{no}");
        }
        assert!(PRESETS.iter().all(|p| !p.base_url.is_empty()));
        assert_eq!(PRESETS[0].model, DEFAULT_MODEL);
    }

    #[test]
    fn the_reopen_hint_names_the_menu_the_shortcut_and_the_ai_switch() {
        assert_eq!(
            reopen_hint(Some("Cmd+Shift+A"), true),
            vec!["View ▸ Show/Hide ▸ Assistant", "or press Cmd+Shift+A"]
        );
        let off = reopen_hint(Some("Ctrl+Shift+A"), false);
        assert!(off[0].starts_with("Options ▸ Show AI features"));
        assert!(off.iter().any(|l| l.contains("Ctrl+Shift+A")));
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
