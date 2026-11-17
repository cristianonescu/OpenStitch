//! The Assistant panel in the browser (#766): a chat beside the design, as on the desktop, with
//! a model the page reaches itself.
//!
//! - The model: the Claude API, or an OpenAI-compatible endpoint that allows the page's origin
//!   (a local Ollama with `OLLAMA_ORIGINS`). Requests go from the page straight to that address
//!   (`openstitch_assistant::browser`); coding agents (ACP) are desktop-only.
//! - The key: kept in this panel's memory for the session. **Remember on this device** keeps it
//!   in this site's browser storage instead, with a warning ([`super::assistant_key`]). It is
//!   sent only to the address it was entered for, never written to preferences, files or logs.
//! - The tools: the same `openstitch-mcp` assistant meta tools, gate and `ui.*` commands as the
//!   desktop. A turn runs as a task between frames (`run`), with the editor itself; a tool call
//!   that needs the user's approval waits for the card's answer before anything runs.
//! - All the changes of one message fold into one undo step.

use crate::app::EditorApp;
use eframe::egui::{self, RichText};
use openstitch_app::editor::assistant_chat::{
    self, APPROVAL_TIMEOUT_S, GateInput, PRESETS, Protocol, Verdict, declined_reply,
    is_this_computer, resolve_scratch_names, unanswered_reply, verdict,
};
use openstitch_app::editor::ui_texts::t;
use openstitch_assistant::browser::BrowserFetch;
use openstitch_assistant::fetch::{AsyncTools, Fetch, LocalFuture};
use openstitch_assistant::{self as claude, StopKind};
use openstitch_engine::Effect;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

// The remembered key, beside this file (#766).
#[path = "assistant_key.rs"]
mod assistant_key;

thread_local! {
    /// The page's editor (openstitch-web registers it): a turn runs between frames with it.
    static EDITOR: RefCell<Weak<RefCell<EditorApp>>> = const { RefCell::new(Weak::new()) };
}

/// The page calls this once the editor exists, so the Assistant's turns can reach it.
pub fn set_editor(editor: &Rc<RefCell<EditorApp>>) {
    EDITOR.with(|e| *e.borrow_mut() = Rc::downgrade(editor));
}

fn editor() -> Option<Rc<RefCell<EditorApp>>> {
    EDITOR.with(|e| e.borrow().upgrade())
}

/// One line of the conversation.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    User(String),
    Assistant(String),
    Thinking(String),
    Tool { id: String, label: String, done: bool, is_error: bool },
    Notice(String),
}

/// Commands of one tool call waiting for Allow or Deny.
struct Approval {
    what: Vec<String>,
    answer: Option<bool>,
}

/// The message being answered.
struct Turn {
    undo_start: usize,
    edits: u32,
}

#[derive(Default)]
pub struct Panel {
    pub open: bool,
    pub items: Vec<Item>,
    pub input: String,
    /// The key as typed, for this session; see [`assistant_key`] for the remembered one.
    key: String,
    /// The address `key` belongs to: a key never follows the user to another address.
    key_for: String,
    /// "Remember on this device" is ticked.
    remember: bool,
    show_settings: bool,
    /// Away while a turn holds them.
    conversation: Option<claude::Conversation>,
    server: Option<openstitch_mcp::Server>,
    turn: Option<Turn>,
    cancel: Rc<AtomicBool>,
    approval: Option<Approval>,
    /// What the user allowed for the tool call now running: `(command, params)`; `None` params
    /// match any (the meta tools whose own params the panel does not build).
    approved: Vec<(String, Option<Value>)>,
    tokens: (u64, u64),
    chat: u64,
}

impl Panel {
    fn busy(&self) -> bool {
        self.turn.is_some()
    }
}

pub fn attach_files(app: &mut EditorApp, _paths: Vec<std::path::PathBuf>) {
    app.assistant.panel.items.push(Item::Notice(BROWSER_LIMITS.into()));
}

/// What the browser's Assistant cannot do yet, shown in the panel at all times (#766).
pub const BROWSER_LIMITS: &str = "In the browser the Assistant cannot yet see a picture you give \
it or import an SVG it draws (#1224), and outside AI clients cannot control this page (#1223). \
The desktop editor does both.";

pub fn set_open(app: &mut EditorApp, open: bool) {
    if open && !app.ui.prefs.ai.enabled {
        return;
    }
    app.assistant.panel.open = open;
    if app.ui.prefs.ai.panel_open != open {
        app.ui.prefs.ai.panel_open = open;
        app.ui.save_prefs();
    }
}

pub fn ai_switched_off(app: &mut EditorApp) {
    app.assistant.panel.open = false;
}

pub fn show_sample_notices(_app: &mut EditorApp) {}

/// A new document identity (#1169).
pub fn next_document() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// The browser keeps no conversation with a document (#1169): a save writes the chat the file had.
pub fn chat_to_session(_app: &mut EditorApp) {}

pub fn document_loaded(_app: &mut EditorApp) {}

pub fn document_activated(_app: &mut EditorApp) {}

pub fn show_sample_offers(_app: &mut EditorApp, _questions: bool) {}

/// `ui.askUser` and `ui.suggestReplies` (#1172): the browser panel shows no cards for them yet,
/// so the model is told to ask in its reply instead.
pub fn offer(
    _app: &mut EditorApp,
    command: &str,
    _params: Option<&Value>,
) -> Result<Value, String> {
    Ok(match command {
        "ui.askUser" => json!({
            "shown": 0,
            "next": "This panel cannot show question cards. Ask your questions in your reply \
                     instead, each with its short answer choices (your recommendation first), \
                     then end your turn; the answers come as the next message.",
        }),
        _ => json!({ "shown": 0 }),
    })
}

pub fn turn_running(app: &EditorApp) -> bool {
    app.assistant.panel.busy()
}

/// Stop: the turn ends soon; what it already changed stays, as one undo step.
pub fn stop(app: &mut EditorApp) {
    let panel = &mut app.assistant.panel;
    panel.cancel.store(true, Ordering::SeqCst);
    if let Some(a) = &mut panel.approval {
        a.answer = Some(false);
    }
}

fn new_chat(app: &mut EditorApp) {
    let panel = &mut app.assistant.panel;
    if panel.busy() {
        return;
    }
    panel.conversation = None;
    panel.items.clear();
    panel.tokens = (0, 0);
    panel.chat += 1;
}

/// What the model is told about the editor with each message.
fn context(app: &EditorApp) -> String {
    let file = app
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .map_or("an unsaved design".to_string(), |n| {
            n.to_string_lossy().into_owned()
        });
    let units = match app.ui.prefs.units {
        openstitch_app::editor::units::Units::Mm => "mm",
        openstitch_app::editor::units::Units::Inch => "inches (commands still take mm)",
    };
    format!(
        "[Editor in a web browser: {file}; {} objects; selected ids {:?}; units {units}; \
         scratch folder none]",
        app.session.design().objects.len(),
        app.selection,
    )
}

/// The key for the address in the settings: the one typed this session, else the remembered one.
fn api_key(app: &mut EditorApp, base_url: &str) -> Option<String> {
    let panel = &mut app.assistant.panel;
    if panel.key_for != base_url {
        panel.key = assistant_key::load(base_url).unwrap_or_default();
        panel.remember = !panel.key.is_empty();
        panel.key_for = base_url.to_string();
    }
    Some(panel.key.trim().to_string()).filter(|k| !k.is_empty())
}

fn model_config(app: &mut EditorApp) -> Option<claude::Config> {
    let prefs = app.ui.prefs.ai.clone();
    let say = |app: &mut EditorApp, text: &str| {
        app.assistant.panel.show_settings = true;
        app.assistant.panel.items.push(Item::Notice(text.into()));
    };
    if prefs.protocol == Protocol::Agent {
        say(
            app,
            "Coding agents run only in the desktop editor. Choose the Claude API or an OpenAI-compatible endpoint in the settings.",
        );
        return None;
    }
    if prefs.model.trim().is_empty() {
        say(app, "Type the model's name in the settings first.");
        return None;
    }
    if !claude::key_safe_for(&prefs.base_url) {
        say(
            app,
            &format!(
                "The address {} is neither https nor this computer, so no key is sent there.",
                prefs.base_url
            ),
        );
        return None;
    }
    let local = prefs.protocol == Protocol::OpenAi && is_this_computer(&prefs.base_url);
    let key = match api_key(app, &prefs.base_url) {
        Some(k) => k,
        None if local => String::new(),
        None => {
            say(app, "Enter the API key for this address in the settings first.");
            return None;
        }
    };
    Some(claude::Config {
        base_url: prefs.base_url.clone(),
        api_key: key,
        model: prefs.model.clone(),
        effort: prefs.effort.clone(),
        request_timeout: Duration::from_secs(prefs.request_timeout_s),
        system: format!(
            "{}{}",
            openstitch_mcp::server::assistant_instructions(),
            app.memory_prompt().unwrap_or_default()
        ),
        protocol: match prefs.protocol {
            Protocol::OpenAi => claude::Protocol::OpenAi,
            Protocol::Anthropic | Protocol::Agent => claude::Protocol::Anthropic,
        },
        reasoning_effort: Some(prefs.reasoning_effort.clone()).filter(|e| !e.is_empty()),
        ..claude::Config::default()
    })
}

/// Sends `text` as the next message.
pub fn send(app: &mut EditorApp, text: String, ctx: &egui::Context) {
    let text = text.trim().to_string();
    if text.is_empty() || app.assistant.panel.busy() {
        return;
    }
    let Some(editor) = editor() else {
        app.assistant.panel.items.push(Item::Notice(
            "The page has not connected the Assistant to the editor.".into(),
        ));
        return;
    };
    let Some(cfg) = model_config(app) else {
        return;
    };
    let content = claude::user_content(&format!("{text}\n\n{}", context(app)), &[]);
    let panel = &mut app.assistant.panel;
    let mut chat = panel.conversation.take().unwrap_or_default();
    if !chat.messages().is_empty() && chat.protocol() != cfg.protocol {
        chat = claude::Conversation::new();
        panel.items.push(Item::Notice(
            "The settings changed the kind of endpoint, so this is a new conversation.".into(),
        ));
    }
    let server = match panel.server.take() {
        Some(s) => s,
        None => match openstitch_mcp::Server::new(openstitch_mcp::Config::default()) {
            Ok(s) => s.with_assistant(Link(Rc::downgrade(&editor))),
            Err(e) => {
                panel.conversation = Some(chat);
                panel.items.push(Item::Notice(format!("Error: {e}")));
                return;
            }
        },
    };
    panel.items.push(Item::User(text));
    panel.cancel.store(false, Ordering::SeqCst);
    panel.turn = Some(Turn {
        undo_start: app.undo_groups.len(),
        edits: 0,
    });
    let cancel = app.assistant.panel.cancel.clone();
    wasm_bindgen_futures::spawn_local(run(editor, cfg, content, chat, server, cancel, ctx.clone()));
}

/// One turn, between frames: the model's requests wait on the page's `fetch`, the tool calls
/// run on the editor, and every event lands in the panel.
async fn run(
    editor: Rc<RefCell<EditorApp>>,
    cfg: claude::Config,
    content: Vec<Value>,
    mut chat: claude::Conversation,
    server: openstitch_mcp::Server,
    cancel: Rc<AtomicBool>,
    ctx: egui::Context,
) {
    let mut tools = WebTools {
        server,
        editor: editor.clone(),
        cancel: cancel.clone(),
        ctx: ctx.clone(),
    };
    let end = chat
        .run_turn_async(
            &cfg,
            &BrowserFetch,
            &mut tools,
            content,
            &cancel,
            &mut |e| {
                if let Ok(mut app) = editor.try_borrow_mut() {
                    on_event(&mut app.assistant.panel, e);
                }
                ctx.request_repaint();
            },
        )
        .await;
    if let Ok(mut app) = editor.try_borrow_mut() {
        app.assistant.panel.conversation = Some(chat);
        app.assistant.panel.server = Some(tools.server);
        finish_turn(&mut app, end);
    }
    ctx.request_repaint();
}

fn on_event(panel: &mut Panel, e: claude::Event) {
    match e {
        claude::Event::Text(s) => match panel.items.last_mut() {
            Some(Item::Assistant(t)) => t.push_str(&s),
            _ => panel.items.push(Item::Assistant(s)),
        },
        claude::Event::Thinking(s) => match panel.items.last_mut() {
            Some(Item::Thinking(t)) => t.push_str(&s),
            _ => panel.items.push(Item::Thinking(s)),
        },
        claude::Event::ToolStart { id, name } => panel.items.push(Item::Tool {
            id,
            label: name,
            done: false,
            is_error: false,
        }),
        claude::Event::ToolCall { id, name, input } => {
            let text = tool_label(&name, &input);
            // The newest row of that call: ids are unique per reply, not per conversation.
            if let Some(Item::Tool { label, .. }) = panel
                .items
                .iter_mut()
                .rev()
                .find(|it| matches!(it, Item::Tool { id: i, .. } if *i == id))
            {
                *label = text;
            }
        }
        claude::Event::ToolDone { id, is_error, .. } => {
            if let Some(Item::Tool {
                done, is_error: e, ..
            }) = panel
                .items
                .iter_mut()
                .rev()
                .find(|it| matches!(it, Item::Tool { id: i, .. } if *i == id))
            {
                *done = true;
                *e = is_error;
            }
        }
        claude::Event::Usage {
            input_tokens,
            output_tokens,
            ..
        } => {
            panel.tokens.0 += input_tokens;
            panel.tokens.1 += output_tokens;
        }
        claude::Event::Retrying { after, reason } => panel.items.push(Item::Notice(format!(
            "{reason} Trying again in {} s.",
            after.as_secs()
        ))),
    }
}

/// A tool row's label, in words.
fn tool_label(name: &str, input: &Value) -> String {
    let arg = |k: &str| input.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
    match name {
        "execute" => format!("Ran {}", arg("command").unwrap_or("a command")),
        "run_batch" => match input.get("steps").and_then(Value::as_array) {
            Some(s) if s.len() == 1 => "Ran 1 command".into(),
            Some(s) => format!("Ran {} commands", s.len()),
            None => "Ran commands".into(),
        },
        "view_render" => "Looked at the design".into(),
        "commands_list" => "Looked up the commands".into(),
        "command_describe" => format!("Read the help for {}", arg("command").unwrap_or("a command")),
        "skill_reference" => "Read the embroidery guide".into(),
        "notes_read" => "Read its notes".into(),
        "notes_write" => "Remembered something".into(),
        other => other.to_string(),
    }
}

fn finish_turn(app: &mut EditorApp, end: StopKind) {
    let Some(turn) = app.assistant.panel.turn.take() else {
        return;
    };
    app.assistant.panel.approval = None;
    app.assistant.panel.approved.clear();
    let start = turn.undo_start.min(app.undo_groups.len());
    let steps: usize = app.undo_groups.drain(start..).sum();
    let panel = &mut app.assistant.panel;
    if steps > 0 {
        app.undo_groups.push(steps);
        panel.items.push(Item::Notice(format!(
            "{} change{}, one undo step.",
            turn.edits,
            if turn.edits == 1 { "" } else { "s" }
        )));
    }
    match end {
        StopKind::EndTurn => {}
        StopKind::MaxTokens => panel.items.push(Item::Notice(
            "The answer reached its length limit; a tool call it was writing was not run.".into(),
        )),
        StopKind::Refusal { category } => panel.items.push(Item::Notice(format!(
            "The model declined this request{}.",
            category.map(|c| format!(" ({c})")).unwrap_or_default()
        ))),
        StopKind::Cancelled => panel.items.push(Item::Notice("Stopped.".into())),
        StopKind::Error(e) => panel.items.push(Item::Notice(format!("Error: {e}"))),
    }
}

/// The editor commands a meta tool call sends that the gate may ask about.
fn planned(name: &str, input: &Value) -> Vec<(String, Option<Value>)> {
    let step = |s: &Value| {
        s["command"].as_str().map(|c| {
            (
                c.to_string(),
                Some(s.get("params").cloned().unwrap_or_else(|| json!({}))),
            )
        })
    };
    match name {
        "execute" => step(input).into_iter().collect(),
        "run_batch" if input["validateOnly"] != json!(true) => input["steps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(step)
            .collect(),
        "scratch_write" => vec![("ui.scratchWrite".into(), None)],
        "notes_write" => vec![("ui.notesWrite".into(), None)],
        _ => Vec::new(),
    }
}

/// The effect a command declares (engine or `ui.*`); `None` for unknown commands.
fn effect_of(app: &EditorApp, command: &str) -> Option<Effect> {
    if command == "commands.list" {
        return Some(Effect::Read);
    }
    if let Some(c) = openstitch_app::editor::assistant::ui_command(command) {
        return Some(c.effect);
    }
    app.session.permits(command).ok().map(|c| c.effect)
}

/// The gate's answer for one command, as on the desktop.
fn gate(app: &EditorApp, command: &str, effect: Effect, params: &Value) -> Verdict {
    let prefs = &app.ui.prefs.ai;
    let (edits, undo_start) = app
        .assistant
        .panel
        .turn
        .as_ref()
        .map_or((0, app.undo_groups.len()), |t| (t.edits, t.undo_start));
    verdict(&GateInput {
        mode: prefs.mode,
        command,
        effect,
        params,
        scratch: app.assistant.scratch.as_deref(),
        always_allow: &prefs.always_allow,
        edits,
        edit_limit: prefs.edit_limit,
        undo_before_message: command == "history.undo" && app.undo_groups.len() <= undo_start,
        existing_layout: command == "ui.layout"
            && params["name"]
                .as_str()
                .is_some_and(|n| app.ui.prefs.layouts.iter().any(|(l, _)| l == n.trim())),
        unsaved: app.is_dirty(),
        unsaved_chat_only: app.is_dirty() && !app.design_dirty(),
    })
}

/// The model's tools: the assistant MCP server's meta tools, with the approval asked first.
struct WebTools {
    server: openstitch_mcp::Server,
    editor: Rc<RefCell<EditorApp>>,
    cancel: Rc<AtomicBool>,
    ctx: egui::Context,
}

impl WebTools {
    /// What the tool call would do that needs the user's yes.
    fn questions(&self, name: &str, input: &Value) -> Vec<((String, Option<Value>), String)> {
        let Ok(app) = self.editor.try_borrow() else {
            return Vec::new();
        };
        planned(name, input)
            .into_iter()
            .filter_map(|(command, params)| {
                let effect = effect_of(&app, &command)?;
                let p = params.clone().unwrap_or_else(|| json!({}));
                match gate(&app, &command, effect, &p) {
                    Verdict::Ask(what) => Some(((command, params), what)),
                    _ => None,
                }
            })
            .collect()
    }

    /// Shows the card and waits for its answer (Deny when Stop is pressed).
    /// Shows the card and waits for its answer: `Some(allow)`, or `None` when nobody answered
    /// within the desktop's approval window (APPROVAL_TIMEOUT_S). Stop answers Deny.
    async fn ask(&self, what: Vec<String>) -> Option<bool> {
        if let Ok(mut app) = self.editor.try_borrow_mut() {
            app.assistant.panel.approval = Some(Approval { what, answer: None });
        }
        self.ctx.request_repaint();
        let until = web_time::Instant::now() + Duration::from_secs(APPROVAL_TIMEOUT_S);
        loop {
            BrowserFetch
                .sleep(Duration::from_millis(100), &self.cancel)
                .await;
            let Ok(mut app) = self.editor.try_borrow_mut() else {
                continue;
            };
            let panel = &mut app.assistant.panel;
            if self.cancel.load(Ordering::SeqCst) {
                panel.approval = None;
                return Some(false);
            }
            if let Some(answer) = panel.approval.as_ref().and_then(|a| a.answer) {
                panel.approval = None;
                return Some(answer);
            }
            if web_time::Instant::now() >= until {
                panel.approval = None;
                return None;
            }
        }
    }
}

impl AsyncTools for WebTools {
    fn definitions(&self) -> Vec<Value> {
        self.server
            .tools()
            .iter()
            .map(|t| t.definition.clone())
            .collect()
    }

    fn call<'a>(&'a mut self, name: &'a str, input: &'a Value) -> LocalFuture<'a, claude::ToolResult> {
        Box::pin(async move {
            let Some(kind) = self
                .server
                .tools()
                .iter()
                .find(|t| t.name == name)
                .map(|t| t.kind.clone())
            else {
                return claude::from_mcp_result(&openstitch_mcp::server::tool_failure(&format!(
                    "unknown tool '{name}'"
                )));
            };
            let questions = self.questions(name, input);
            if !questions.is_empty() {
                let what: Vec<String> = questions.iter().map(|(_, w)| w.clone()).collect();
                let refused = match self.ask(what.clone()).await {
                    Some(true) => None,
                    Some(false) => Some(declined_reply(&what.join("; "))),
                    None => Some(unanswered_reply()),
                };
                if let Some(why) = refused {
                    return claude::from_mcp_result(&openstitch_mcp::server::tool_failure(&why));
                }
                if let Ok(mut app) = self.editor.try_borrow_mut() {
                    app.assistant.panel.approved = questions.into_iter().map(|(c, _)| c).collect();
                }
            }
            let result = self.server.call(&kind, input);
            if let Ok(mut app) = self.editor.try_borrow_mut() {
                app.assistant.panel.approved.clear();
            }
            claude::from_mcp_result(&result)
        })
    }
}

/// The server's way into the editor: each request passes the gate, then runs as the desktop's.
struct Link(Weak<RefCell<EditorApp>>);

impl openstitch_mcp::EditorLink for Link {
    fn handle(&mut self, request: &Value) -> Value {
        let Some(editor) = self.0.upgrade() else {
            return json!({ "ok": false, "error": "the editor has closed" });
        };
        let Ok(mut app) = editor.try_borrow_mut() else {
            return json!({ "ok": false, "error": "the editor is busy; try again" });
        };
        handle(&mut app, request.clone())
    }
}

/// The note an approved command's result carries (#1198).
const APPROVED_NOTE: &str = "the user approved this step on the approval card";

fn handle(app: &mut EditorApp, mut request: Value) -> Value {
    let err = |e: String| json!({ "ok": false, "error": e });
    let command = request["command"].as_str().unwrap_or_default().to_string();
    let Some(effect) = effect_of(app, &command) else {
        return app.remote_request(&request); // the engine's own "unknown command" error
    };
    let mut params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    if resolve_scratch_names(&mut params, app.assistant.scratch.as_deref()) {
        request["params"] = params.clone();
    }
    match app.beside_the_design(&command, &request) {
        Ok(Some(r)) => {
            request = r;
            params = request.get("params").cloned().unwrap_or_else(|| json!({}));
        }
        Ok(None) => {}
        Err(e) => return err(e),
    }
    if !app.ui.prefs.ai.memory && matches!(command.as_str(), "ui.notesRead" | "ui.notesWrite") {
        return err(
            "the user turned the Assistant's memory off; do not try to remember things".into(),
        );
    }
    let approved = match gate(app, &command, effect, &params) {
        Verdict::Run => false,
        Verdict::Refuse(why) => return err(why),
        Verdict::Ask(what) => {
            let allowed = &mut app.assistant.panel.approved;
            match allowed
                .iter()
                .position(|(c, p)| *c == command && p.as_ref().is_none_or(|p| *p == params))
            {
                Some(i) => {
                    allowed.remove(i);
                    true
                }
                None => {
                    return err(format!(
                        "this needs the user's approval ({what}), which this call did not ask \
                         for; send it again as its own execute call"
                    ));
                }
            }
        }
    };
    let revision = app.revision;
    let mut reply = app.remote_request(&request);
    if reply["ok"] == json!(true) {
        if let Some(n) =
            assistant_chat::consequence_notice(&command, &params, app.revision != revision)
        {
            app.assistant.panel.items.push(Item::Notice(n));
        }
        if effect != Effect::Read
            && let Some(t) = &mut app.assistant.panel.turn
        {
            t.edits += 1;
        }
        if approved {
            match reply.get_mut("result") {
                Some(Value::Object(r)) => {
                    r.insert("approval".into(), json!(APPROVED_NOTE));
                }
                Some(other) => {
                    let value = other.take();
                    *other = json!({ "value": value, "approval": APPROVED_NOTE });
                }
                None => reply["result"] = json!({ "approval": APPROVED_NOTE }),
            }
        }
    }
    reply
}

pub fn panel(app: &mut EditorApp, ui: &mut egui::Ui) {
    if !(app.ui.prefs.ai.enabled && app.assistant.panel.open) {
        return;
    }
    let ctx = ui.ctx().clone();
    egui::Panel::right("assistant")
        .default_size(380.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(t("ui-assistant"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if close_button(ui).clicked() {
                        set_open(app, false);
                    }
                    let busy = app.assistant.panel.busy();
                    if ui
                        .selectable_label(app.assistant.panel.show_settings, "Settings")
                        .clicked()
                    {
                        app.assistant.panel.show_settings ^= true;
                    }
                    if ui.add_enabled(!busy, egui::Button::new(t("ui-new-chat"))).clicked() {
                        new_chat(app);
                    }
                });
            });
            let (i, o) = app.assistant.panel.tokens;
            ui.label(
                RichText::new(format!(
                    "{} · {} · {i} in / {o} out tokens",
                    match app.ui.prefs.ai.protocol {
                        Protocol::OpenAi => "OpenAI-compatible",
                        _ => "Claude API",
                    },
                    app.ui.prefs.ai.model
                ))
                .small()
                .weak(),
            );
            if app.assistant.panel.show_settings {
                ui.separator();
                settings(app, ui);
            }
            ui.separator();
            approval_card(app, ui);
            input_row(app, ui, &ctx);
            transcript(app, ui);
        });
}

/// The panel's close button: the editor's own close icon, as on the desktop.
fn close_button(ui: &mut egui::Ui) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(28.0, 26.0), egui::Sense::click());
    let v = ui.style().interact_selectable(&resp, false);
    ui.painter().rect(
        r,
        v.corner_radius,
        v.weak_bg_fill,
        v.bg_stroke,
        egui::StrokeKind::Inside,
    );
    crate::icons::paint_ui(ui.painter(), r.shrink(5.0), "close", v.fg_stroke.color);
    resp.on_hover_text(t("ui-close"))
}

fn settings(app: &mut EditorApp, ui: &mut egui::Ui) {
    let mut changed = false;
    let prefs = &mut app.ui.prefs.ai;
    ui.horizontal(|ui| {
        ui.label(t("ui-endpoint"));
        egui::ComboBox::from_id_salt("web-ai-preset")
            .selected_text(
                PRESETS
                    .iter()
                    .find(|p| p.base_url == prefs.base_url && p.protocol == prefs.protocol)
                    .map_or("Custom", |p| p.name),
            )
            .show_ui(ui, |ui| {
                for p in PRESETS.iter().filter(|p| p.name != "LM Studio (this computer)") {
                    if ui.selectable_label(false, p.name).clicked() {
                        assistant_chat::apply_preset(prefs, p);
                        changed = true;
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Address");
        changed |= ui.text_edit_singleline(&mut prefs.base_url).changed();
    });
    ui.horizontal(|ui| {
        ui.label("Kind");
        changed |= ui
            .radio_value(&mut prefs.protocol, Protocol::Anthropic, "Claude API")
            .changed();
        changed |= ui
            .radio_value(&mut prefs.protocol, Protocol::OpenAi, "OpenAI-compatible")
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label(t("ui-model"));
        changed |= ui.text_edit_singleline(&mut prefs.model).changed();
    });
    let base_url = prefs.base_url.trim().to_string();
    if changed {
        app.ui.save_prefs();
    }
    // The key: masked, for this address only.
    let _ = api_key(app, &base_url);
    let panel = &mut app.assistant.panel;
    ui.horizontal(|ui| {
        ui.label("API key");
        let r = ui.add(
            egui::TextEdit::singleline(&mut panel.key)
                .password(true)
                .hint_text("kept for this session only"),
        );
        if r.changed() && panel.remember {
            assistant_key::store(&base_url, panel.key.trim());
        }
    });
    let was = panel.remember;
    ui.checkbox(&mut panel.remember, "Remember on this device");
    if panel.remember != was {
        if panel.remember {
            assistant_key::store(&base_url, panel.key.trim());
        } else {
            assistant_key::forget(&base_url);
        }
    }
    ui.label(
        RichText::new(if panel.remember {
            "The key is kept in this site's browser storage on this device, unencrypted: anyone \
             using this browser profile, and any script this site serves, can read it. Untick to \
             remove it."
        } else {
            "The key stays in this page's memory and is gone when the tab closes. It is sent \
             only to the address above."
        })
        .small()
        .weak(),
    );
    ui.label(
        RichText::new(
            "The page talks to the address directly, so it must allow this page (CORS). A local \
             Ollama: start it with OLLAMA_ORIGINS set to this page's address.",
        )
        .small()
        .weak(),
    );
}

fn approval_card(app: &mut EditorApp, ui: &mut egui::Ui) {
    let Some(a) = &mut app.assistant.panel.approval else {
        return;
    };
    if a.answer.is_some() {
        return;
    }
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("The Assistant asks to:").strong());
        for w in &a.what {
            ui.label(format!("•  {w}"));
        }
        ui.horizontal(|ui| {
            if ui.button("Allow").clicked() {
                a.answer = Some(true);
            }
            if ui.button("Deny").clicked() {
                a.answer = Some(false);
            }
        });
    });
}

/// Where a message goes (#766): said before the first one is sent, and always in view.
pub fn recipient_notice(base_url: &str) -> String {
    let host = claude_host(base_url);
    format!(
        "Your messages, the design's details and the pictures the Assistant looks at are sent \
         from this page to {host}, and to nothing else of ours."
    )
}

/// The host part of an address, for the notice; the whole address when it has none.
fn claude_host(url: &str) -> String {
    let rest = url.trim().split_once("://").map_or(url.trim(), |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or(rest).to_string()
}

fn input_row(app: &mut EditorApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.label(
        RichText::new(recipient_notice(&app.ui.prefs.ai.base_url))
            .small()
            .weak(),
    );
    ui.label(RichText::new(BROWSER_LIMITS).small().weak());
    let busy = app.assistant.panel.busy();
    let r = ui.add(
        egui::TextEdit::multiline(&mut app.assistant.panel.input)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .hint_text("Ask the Assistant…"),
    );
    let enter = r.has_focus()
        && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
    ui.horizontal(|ui| {
        if busy {
            if ui.button("Stop").clicked() {
                stop(app);
            }
        } else if ui.button("Send").clicked() || enter {
            let text = std::mem::take(&mut app.assistant.panel.input);
            send(app, text, ctx);
        }
    });
}

fn transcript(app: &mut EditorApp, ui: &mut egui::Ui) {
    let chat = app.assistant.panel.chat;
    egui::ScrollArea::vertical()
        .id_salt(("web-assistant", chat))
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for it in &app.assistant.panel.items {
                match it {
                    Item::User(s) => {
                        ui.label(RichText::new(s).strong());
                    }
                    Item::Assistant(s) => {
                        ui.label(s);
                    }
                    Item::Thinking(s) => {
                        ui.label(RichText::new(s).italics().weak());
                    }
                    Item::Tool {
                        label,
                        done,
                        is_error,
                        ..
                    } => {
                        let text = match (done, is_error) {
                            (false, _) => format!("{label} (running)"),
                            (true, false) => label.clone(),
                            (true, true) => format!("{label} (failed)"),
                        };
                        ui.label(RichText::new(text).small());
                    }
                    Item::Notice(s) => {
                        ui.label(RichText::new(s).small().weak());
                    }
                }
                ui.add_space(4.0);
            }
        });
}

/// The page's own commands for the Assistant (`openstitch.run`, #766): what a script in the page
/// may do that the model may not, since its tools reach only the editor's commands. `None` for
/// every other command.
///
/// - `ui.assistantSetup {kind, address, model, key?, remember?}`: the endpoint (`kind`
///   `anthropic` or `openai`, its base `address`, the `model`) is stored in the preferences, as
///   the settings do; `key` is kept in the panel's memory for this session, and with
///   `remember: true` in this site's browser storage on this device. The reply never holds a key.
/// - `ui.assistantSend {text}`: opens the panel and sends the message (`busy` is true while the
///   turn runs).
/// - `ui.assistantState`: `{open, busy, sendsTo, limits, approval, items: [{kind, text}],
///   tokens}`, where `sendsTo` is the panel's notice of the host messages go to, `limits` what it
///   cannot do yet in the browser, and `approval` the waiting card's lines or null.
/// - `ui.assistantAnswer {allow}`: answers the waiting approval card.
/// - `ui.assistantStop`: Stop.
pub fn page_command(
    app: &mut EditorApp,
    command: &str,
    params: &Value,
    ctx: &egui::Context,
) -> Option<Result<Value, String>> {
    Some(match command {
        "ui.assistantSetup" => page_setup(app, params),
        "ui.assistantSend" => match params["text"].as_str().filter(|t| !t.trim().is_empty()) {
            None => Err("text is required".into()),
            Some(_) if app.assistant.panel.busy() => Err("a turn is running".into()),
            Some(text) => {
                set_open(app, true);
                let before = app.assistant.panel.items.len();
                send(app, text.to_string(), ctx);
                let busy = app.assistant.panel.busy();
                match app.assistant.panel.items.get(before..).unwrap_or_default() {
                    // Not sent: the reason is the notice it left.
                    [.., Item::Notice(n)] if !busy => Err(n.clone()),
                    _ => Ok(json!({ "busy": busy })),
                }
            }
        },
        "ui.assistantState" => Ok(page_state(app)),
        "ui.assistantAnswer" => match (&mut app.assistant.panel.approval, params["allow"].as_bool()) {
            (_, None) => Err("allow (true or false) is required".into()),
            (Some(a), Some(allow)) if a.answer.is_none() => {
                a.answer = Some(allow);
                Ok(json!({ "answered": allow }))
            }
            _ => Err("no approval card is waiting".into()),
        },
        "ui.assistantStop" => {
            stop(app);
            Ok(json!({ "stopping": app.assistant.panel.busy() }))
        }
        _ => return None,
    })
}

fn page_setup(app: &mut EditorApp, params: &Value) -> Result<Value, String> {
    let protocol = match params["kind"].as_str() {
        Some("anthropic") => Protocol::Anthropic,
        Some("openai") => Protocol::OpenAi,
        _ => return Err("kind is anthropic or openai".into()),
    };
    let address = params["address"]
        .as_str()
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .ok_or("address is required")?;
    if !claude::key_safe_for(address) {
        return Err(format!(
            "{address} is neither https nor this computer (http://localhost)"
        ));
    }
    let model = params["model"].as_str().unwrap_or_default().trim();
    if model.is_empty() {
        return Err("model is required".into());
    }
    let prefs = &mut app.ui.prefs.ai;
    prefs.protocol = protocol;
    prefs.base_url = address.to_string();
    prefs.model = model.to_string();
    app.ui.save_prefs();
    let remember = params["remember"].as_bool().unwrap_or(false);
    let _ = api_key(app, address);
    let panel = &mut app.assistant.panel;
    if let Some(key) = params["key"].as_str() {
        panel.key = key.trim().to_string();
    }
    panel.remember = remember;
    if remember {
        assistant_key::store(address, panel.key.trim());
    } else {
        assistant_key::forget(address);
    }
    Ok(json!({ "kind": params["kind"], "address": address, "model": model, "key": !panel.key.is_empty(), "remembered": remember }))
}

fn page_state(app: &EditorApp) -> Value {
    let panel = &app.assistant.panel;
    let items: Vec<Value> = panel
        .items
        .iter()
        .map(|it| match it {
            Item::User(t) => json!({ "kind": "user", "text": t }),
            Item::Assistant(t) => json!({ "kind": "assistant", "text": t }),
            Item::Thinking(t) => json!({ "kind": "thinking", "text": t }),
            Item::Tool { label, done, is_error, .. } => {
                json!({ "kind": "tool", "text": label, "done": done, "error": is_error })
            }
            Item::Notice(t) => json!({ "kind": "notice", "text": t }),
        })
        .collect();
    json!({
        "open": panel.open,
        "busy": panel.busy(),
        // What the panel shows before a message is sent: where it goes.
        "sendsTo": recipient_notice(&app.ui.prefs.ai.base_url),
        // What it cannot do yet, as the panel shows it.
        "limits": BROWSER_LIMITS,
        "approval": panel.approval.as_ref().filter(|a| a.answer.is_none()).map(|a| a.what.clone()),
        "items": items,
        "tokens": { "input": panel.tokens.0, "output": panel.tokens.1 },
    })
}
