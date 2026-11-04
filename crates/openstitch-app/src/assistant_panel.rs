//! The Assistant panel (B17 A1, docs/research/beyond/ai-integration.md section 4): a chat
//! beside the design where the user asks Claude to make, change or explain embroidery, or to do
//! anything in the editor.
//!
//! - A worker thread runs the conversation (`openstitch-assistant`). The model's tools are the
//!   `openstitch-mcp` assistant server's seven meta tools, driven through an in-process
//!   [`EditorLink`] instead of the loopback socket.
//! - Every engine and `ui.*` command the server sends arrives here on the UI thread as a
//!   [`Job`]. It passes the permission gate (`editor::assistant_chat::verdict`), then runs
//!   through the same path as socket requests.
//! - All the changes of one message fold into one undo step.
//! - The API key lives only in the system keychain.

use crate::app::EditorApp;
use eframe::egui::{self, RichText};
use openstitch_app::editor::assistant_attach::{self, Attachment, MAX_FILES};
use openstitch_app::editor::assistant_chat::{
    APPROVAL_TIMEOUT_S, AiPrefs, GateInput, PRESETS, PermissionMode, Protocol, ScriptRecord,
    Verdict, is_this_computer, verdict,
};
use openstitch_assistant as claude;
use openstitch_engine::Effect;
use openstitch_mcp::EditorLink;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

/// The keychain service the API key is stored under, one entry per endpoint address.
const KEYCHAIN_SERVICE: &str = "OpenStitch assistant";

/// Where a command's answer goes: the in-process server, or a coding agent's MCP bridge on
/// the private socket (A3).
enum Reply {
    Channel(Sender<Value>),
    Socket(openstitch_remote::Job),
}

/// One command from the model, waiting for the UI thread.
struct Job {
    request: Value,
    reply: Reply,
}

impl Job {
    fn answer(self, value: Value) {
        match self.reply {
            Reply::Channel(tx) => {
                let _ = tx.send(value);
            }
            Reply::Socket(job) => job.respond(value),
        }
    }
}

/// Wakes the UI when the worker has something (a repaint request in the app, nothing in tests).
type Waker = Arc<dyn Fn() + Send + Sync>;

/// The server's way into the editor: a request goes to the UI thread and waits for its reply.
struct ChannelLink {
    jobs: Sender<Job>,
    wake: Waker,
}

impl EditorLink for ChannelLink {
    fn handle(&mut self, request: &Value) -> Value {
        let (reply, answer) = channel();
        if self
            .jobs
            .send(Job {
                request: request.clone(),
                reply: Reply::Channel(reply),
            })
            .is_err()
        {
            return json!({ "ok": false, "error": "the Assistant panel was closed" });
        }
        (self.wake)();
        answer
            .recv()
            .unwrap_or_else(|_| json!({ "ok": false, "error": "stopped by the user" }))
    }
}

/// The model's tools: the MCP assistant server's meta tools.
struct ServerTools(openstitch_mcp::Server);

impl claude::Tools for ServerTools {
    fn definitions(&self) -> Vec<Value> {
        self.0
            .tools()
            .iter()
            .map(|t| t.definition.clone())
            .collect()
    }

    fn call(&mut self, name: &str, input: &Value) -> claude::ToolResult {
        let Some(kind) = self
            .0
            .tools()
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.kind.clone())
        else {
            return claude::from_mcp_result(&openstitch_mcp::server::tool_failure(&format!(
                "unknown tool '{name}'"
            )));
        };
        claude::from_mcp_result(&self.0.call(&kind, input))
    }
}

enum ToWorker {
    /// A model turn: the request settings, the user's content blocks and the image generator.
    Turn(Box<claude::Config>, Vec<Value>, Option<claude::ImageConfig>),
    /// A coding agent's turn: the text (A3).
    Prompt(String),
}

enum FromWorker {
    Event(claude::Event),
    End(claude::StopKind),
}

struct Worker {
    to: Sender<ToWorker>,
    from: Receiver<FromWorker>,
    jobs: Receiver<Job>,
    cancel: Arc<AtomicBool>,
    /// A conversation keeps the protocol (and agent) it started with.
    kind: (Protocol, String),
    /// A coding agent reaches the editor through its MCP bridge on this private socket (A3).
    listener: Option<openstitch_remote::Server>,
}

impl Worker {
    fn start(transport: Arc<dyn claude::Transport>, wake: Waker) -> Worker {
        let (to, inbox) = channel::<ToWorker>();
        let (out, from) = channel();
        let (job_tx, jobs) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        std::thread::spawn(move || {
            let server = match openstitch_mcp::Server::new(openstitch_mcp::Config::default()) {
                Ok(s) => s.with_assistant(ChannelLink {
                    jobs: job_tx,
                    wake: wake.clone(),
                }),
                Err(e) => {
                    let _ = out.send(FromWorker::End(claude::StopKind::Error(e.to_string())));
                    return;
                }
            };
            let mut tools = claude::WithImages {
                inner: ServerTools(server),
                image: None,
                transport: transport.as_ref(),
            };
            let mut conversation = claude::Conversation::new();
            while let Ok(ToWorker::Turn(config, content, image)) = inbox.recv() {
                stop.store(false, Ordering::SeqCst);
                tools.image = image;
                let send = |m| {
                    let _ = out.send(m);
                    wake();
                };
                let end = conversation.run_turn(
                    &config,
                    transport.as_ref(),
                    &mut tools,
                    content,
                    &stop,
                    &mut |e| send(FromWorker::Event(e)),
                );
                send(FromWorker::End(end));
            }
        });
        Worker {
            to,
            from,
            jobs,
            cancel,
            kind: (Protocol::Anthropic, String::new()),
            listener: None,
        }
    }

    /// A coding agent (ACP) on the user's own account. It runs in its own process group in an
    /// empty folder; its only way into the editor is `openstitch-mcp --assistant` connected to
    /// `listener`, whose requests pass the same gate as the in-process ones (`serve`).
    fn start_agent(
        config: claude::AcpConfig,
        listener: openstitch_remote::Server,
        wake: Waker,
    ) -> Worker {
        let (to, inbox) = channel::<ToWorker>();
        let (out, from) = channel();
        let (_job_tx, jobs) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let w = wake.clone();
        listener.set_waker(move || w());
        std::thread::spawn(move || {
            let send = |m| {
                let _ = out.send(m);
                wake();
            };
            let mut agent = match claude::AcpConversation::start(&config) {
                Ok(a) => a,
                Err(e) => {
                    // The first message is waiting for an answer.
                    if let Ok(ToWorker::Prompt(_)) = inbox.recv() {
                        send(FromWorker::End(claude::StopKind::Error(agent_error(&e))));
                    }
                    return;
                }
            };
            while let Ok(m) = inbox.recv() {
                let ToWorker::Prompt(text) = m else { continue };
                stop.store(false, Ordering::SeqCst);
                let end = agent.prompt(&text, &stop, &mut |e| send(FromWorker::Event(e)));
                send(FromWorker::End(end));
            }
            agent.stop();
        });
        Worker {
            to,
            from,
            jobs,
            cancel,
            kind: (Protocol::Agent, String::new()),
            listener: Some(listener),
        }
    }
}

/// What the user is told when a coding agent cannot start.
fn agent_error(e: &claude::AcpError) -> String {
    match e {
        claude::AcpError::NotFound(m) => format!("The agent is not installed: {m}"),
        claude::AcpError::Start(m) => format!("The agent did not start: {m}"),
        claude::AcpError::Protocol(m) => format!("The agent answered unexpectedly: {m}"),
        claude::AcpError::AuthRequired(methods) => {
            let ways: Vec<String> = methods
                .iter()
                .map(|m| match m {
                    claude::AuthMethod::Command {
                        name, description, ..
                    } => match description {
                        Some(d) => format!("{name}: {d}"),
                        None => name.clone(),
                    },
                    claude::AuthMethod::Other { name, .. } => name.clone(),
                })
                .collect();
            format!(
                "Sign in to the agent first, in a terminal, with its own sign-in ({}); then send again.",
                if ways.is_empty() {
                    "see its documentation".to_string()
                } else {
                    ways.join("; ")
                }
            )
        }
    }
}

/// One line of the conversation.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    User(String),
    Assistant(String),
    Thinking(String),
    Tool {
        id: String,
        name: String,
        detail: String,
        done: bool,
        is_error: bool,
    },
    Notice(String),
}

/// A command waiting for Allow / Always allow / Deny.
struct Approval {
    job: Job,
    command: String,
    what: String,
    since: Instant,
}

/// The message being answered.
struct Turn {
    undo_start: usize,
    edits: u32,
    /// For the "answered without its tools" notice (local models, A2).
    text: String,
    used_tools: bool,
    reply: String,
}

#[derive(Default)]
pub struct Panel {
    pub open: bool,
    pub items: Vec<Item>,
    pub input: String,
    /// Files attached to the next message (A4), already copied into the scratch folder.
    pub attachments: Vec<Attachment>,
    /// Send a picture of the view, or the selection, with the next message (A4).
    pub attach_view: bool,
    pub attach_selection: bool,
    /// Messages typed while a turn runs; sent in order when it ends.
    pub queue: Vec<String>,
    worker: Option<Worker>,
    approvals: Vec<Approval>,
    turn: Option<Turn>,
    /// The undo group the last message folded into, for **Undo these changes**.
    last_undo: Option<usize>,
    pub script: ScriptRecord,
    pub tokens: (u64, u64),
    /// Settings popup: the key being typed (never shown again once saved).
    key_input: String,
    image_key_input: String,
    show_settings: bool,
    /// Tests put a scripted transport here; the app uses `ureq`.
    transport: Option<Arc<dyn claude::Transport>>,
    /// Tests put the key here instead of the keychain.
    test_key: Option<String>,
    /// The settings' model list and Ollama check, filled by background threads.
    lookups: Shared,
    /// Whether the keychain holds a key for this address (asked once per address).
    key_stored: Option<(String, bool)>,
}

fn keychain(base_url: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, base_url).ok()
}

/// The stored key for `base_url`, if any.
fn load_key(base_url: &str) -> Option<String> {
    keychain(base_url)?
        .get_password()
        .ok()
        .filter(|k| !k.is_empty())
}

fn store_key(base_url: &str, key: &str) -> Result<(), String> {
    let entry = keychain(base_url).ok_or("the system keychain is not available")?;
    if key.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    } else {
        entry.set_password(key).map_err(|e| e.to_string())
    }
}

impl Panel {
    pub fn busy(&self) -> bool {
        self.turn.is_some()
    }

    fn api_key(&self, base_url: &str) -> Option<String> {
        if cfg!(test) {
            return self.test_key.clone();
        }
        load_key(base_url)
    }
}

/// What the model is told about the editor with each message (not in the system prompt, so the
/// prompt cache holds).
fn context(app: &EditorApp) -> String {
    let file = app
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .map_or("an unsaved design".to_string(), |n| {
            n.to_string_lossy().into_owned()
        });
    format!(
        "[Editor: {file}; {} objects; selected ids {:?}; scratch folder {}]",
        app.session.design().objects.len(),
        app.selection,
        app.assistant
            .scratch
            .as_ref()
            .map_or("none".to_string(), |d| d.display().to_string()),
    )
}

/// Sends `text` (or queues it while a turn runs).
pub fn send(app: &mut EditorApp, text: String, wake: Waker) {
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    if app.assistant.panel.busy() {
        app.assistant.panel.queue.push(text);
        return;
    }
    let prefs = app.ui.prefs.ai.clone();
    scratch(app);
    let kind = (
        prefs.protocol,
        if prefs.protocol == Protocol::Agent {
            prefs.agent.clone()
        } else {
            String::new()
        },
    );
    let (message, chips) = if prefs.protocol == Protocol::Agent {
        let (extra, images, chips) = extras(app);
        if !images.is_empty() {
            app.assistant.panel.items.push(Item::Notice(
                "A coding agent gets the attached files' paths, not the pictures themselves."
                    .into(),
            ));
        }
        let prompt = format!("{text}{extra}\n\n{}", context(app));
        (ToWorker::Prompt(prompt), chips)
    } else {
        let Some(config) = model_config(app, &prefs) else {
            return;
        };
        let (extra, images, chips) = extras(app);
        let content = claude::user_content(&format!("{text}{extra}\n\n{}", context(app)), &images);
        let image = image_config(app, &prefs);
        (ToWorker::Turn(Box::new(config), content, image), chips)
    };
    let panel = &mut app.assistant.panel;
    if panel.worker.as_ref().is_some_and(|w| w.kind != kind) {
        // The settings changed the protocol or the agent: a new conversation.
        panel.worker = None;
        panel.items.push(Item::Notice(
            "The settings changed the model or agent, so this is a new conversation.".into(),
        ));
    }
    if panel.worker.is_none() {
        let worker = if prefs.protocol == Protocol::Agent {
            match start_agent(app, &prefs.agent, wake) {
                Ok(w) => w,
                Err(e) => {
                    app.assistant.panel.items.push(Item::Notice(e));
                    return;
                }
            }
        } else {
            let transport = panel
                .transport
                .clone()
                .unwrap_or_else(|| Arc::new(claude::UreqTransport));
            let mut w = Worker::start(transport, wake);
            w.kind = kind;
            w
        };
        app.assistant.panel.worker = Some(worker);
    }
    let panel = &mut app.assistant.panel;
    panel.items.push(Item::User(format!("{text}{chips}")));
    panel.turn = Some(Turn {
        undo_start: app.undo_groups.len(),
        edits: 0,
        text,
        used_tools: false,
        reply: String::new(),
    });
    let worker = panel.worker.as_ref().expect("worker");
    if worker.to.send(message).is_err() {
        panel.worker = None;
        panel.turn = None;
        panel
            .items
            .push(Item::Notice("The assistant stopped; send again.".into()));
    }
}

/// The conversation's scratch folder, made on first use: the model writes SVGs and pictures
/// here (ui.scratchWrite) and attached files are copied here; `--listen` made one already.
fn scratch(app: &mut EditorApp) -> Option<std::path::PathBuf> {
    if app.assistant.scratch.is_none() {
        match crate::assistant_ui::create_scratch() {
            Ok(dir) => app.assistant.scratch = Some(dir),
            Err(e) => app.assistant.panel.items.push(Item::Notice(format!(
                "No scratch folder for the assistant's files ({e}); it can still edit the design."
            ))),
        }
    }
    app.assistant.scratch.clone()
}

/// Attaches files to the next message (A4): copied into the scratch folder, at most
/// [`MAX_FILES`].
pub fn attach_files(app: &mut EditorApp, paths: Vec<std::path::PathBuf>) {
    let Some(dir) = scratch(app) else { return };
    for path in paths {
        let panel = &mut app.assistant.panel;
        if panel.attachments.len() >= MAX_FILES {
            panel.items.push(Item::Notice(format!(
                "At most {MAX_FILES} files go with one message."
            )));
            break;
        }
        match assistant_attach::attach(&path, &dir) {
            Ok(a) => panel.attachments.push(a),
            Err(e) => panel.items.push(Item::Notice(e)),
        }
    }
}

/// What goes with the message (A4): the attached files, a picture of the view and the
/// selection, as extra text, image blocks and a short line for the chat.
fn extras(app: &mut EditorApp) -> (String, Vec<(String, String)>, String) {
    let panel = &mut app.assistant.panel;
    let files = std::mem::take(&mut panel.attachments);
    let view = std::mem::take(&mut panel.attach_view);
    let selection = std::mem::take(&mut panel.attach_selection);
    let mut text = String::new();
    let mut images: Vec<(String, String)> = files.iter().filter_map(|a| a.image.clone()).collect();
    let mut chips: Vec<String> = files.iter().map(|a| a.name.clone()).collect();
    if !files.is_empty() {
        text = format!("\n\n{}", assistant_attach::describe(&files));
    }
    if view {
        match app.ui_command("ui.render", Some(&json!({ "size": 1024 }))) {
            Ok(r) => {
                if let Some(png) = r["png"].as_str() {
                    images.push(("image/png".into(), png.into()));
                    text.push_str("\n\n[The user attached a picture of the design view.]");
                    chips.push("view".into());
                }
            }
            Err(e) => app
                .assistant
                .panel
                .items
                .push(Item::Notice(format!("No picture of the view: {e}"))),
        }
    }
    if selection {
        let ids: Vec<u64> = app.selection.iter().copied().collect();
        let list = app.session.execute("object.list", None).unwrap_or_default();
        let s = assistant_attach::describe_selection(&list, &ids);
        if !s.is_empty() {
            text.push_str(&format!("\n\n{s}"));
            chips.push(format!("selection ({})", ids.len()));
        }
    }
    let chips = if chips.is_empty() {
        String::new()
    } else {
        format!("\n📎 {}", chips.join(" · "))
    };
    (text, images, chips)
}

/// The image generator (A4), when one is set up; its key is in the keychain under its address.
fn image_config(app: &EditorApp, prefs: &AiPrefs) -> Option<claude::ImageConfig> {
    let url = prefs.image_url.trim();
    (!url.is_empty()).then(|| claude::ImageConfig {
        base_url: url.to_string(),
        api_key: app.assistant.panel.api_key(url).unwrap_or_default(),
        model: prefs.image_model.clone(),
        request_timeout: Duration::from_secs(prefs.request_timeout_s),
    })
}

/// The model request settings, or `None` (with a notice) when the key may not be sent.
fn model_config(app: &mut EditorApp, prefs: &AiPrefs) -> Option<claude::Config> {
    if !claude::key_safe_for(&prefs.base_url) {
        app.assistant.panel.items.push(Item::Notice(format!(
            "The address {} is neither https nor this computer, so the key is not sent. Change it in the Assistant's settings.",
            prefs.base_url
        )));
        return None;
    }
    let local = prefs.protocol == Protocol::OpenAi && is_this_computer(&prefs.base_url);
    let key = match app.assistant.panel.api_key(&prefs.base_url) {
        Some(k) => k,
        // A model server on this computer needs no key.
        None if local => String::new(),
        None => {
            app.assistant.panel.show_settings = true;
            app.assistant.panel.items.push(Item::Notice(
                "Add the API key for this address in the Assistant's settings (⚙) first. It is kept in the system keychain and sent only to the address shown there.".into(),
            ));
            return None;
        }
    };
    Some(claude::Config {
        base_url: prefs.base_url.clone(),
        api_key: key,
        model: prefs.model.clone(),
        effort: prefs.effort.clone(),
        request_timeout: Duration::from_secs(prefs.request_timeout_s),
        system: openstitch_mcp::server::assistant_instructions().to_string(),
        protocol: match prefs.protocol {
            Protocol::OpenAi => claude::Protocol::OpenAi,
            Protocol::Anthropic | Protocol::Agent => claude::Protocol::Anthropic,
        },
        reasoning_effort: Some(prefs.reasoning_effort.clone()).filter(|e| !e.is_empty()),
        ..claude::Config::default()
    })
}

/// The agent's tool server is this editor's own executable in `--assistant-mcp` mode, so it
/// works from every package (the separate `openstitch-mcp` ships only in the CLI archive).
fn mcp_program() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok().filter(|p| p.is_file())
}

/// Finds a coding agent's program on PATH and in the usual install folders.
fn find_program(name: &str) -> Option<std::path::PathBuf> {
    claude::search_dirs().into_iter().find_map(|d| {
        [
            name.to_string(),
            format!("{name}.cmd"),
            format!("{name}{}", std::env::consts::EXE_SUFFIX),
        ]
        .into_iter()
        .map(|n| d.join(n))
        .find(|p| p.is_file())
    })
}

/// Starts a coding agent with a private socket for its MCP bridge.
fn start_agent(app: &mut EditorApp, id: &str, wake: Waker) -> Result<Worker, String> {
    let agent = claude::KNOWN_AGENTS
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| format!("Unknown agent '{id}'; pick one in the Assistant's settings."))?;
    let program = find_program(agent.program).ok_or_else(|| {
        format!(
            "{} is not installed. Install it with: {}  — then send again.",
            agent.name, agent.install
        )
    })?;
    let mcp = mcp_program()
        .ok_or("The editor cannot find its own program to start the agent's tool server.")?;
    let scratch = app
        .assistant
        .scratch
        .clone()
        .ok_or("No scratch folder for the agent.")?;
    // An empty working folder of its own; its endpoint file is readable by the user only (the
    // scratch folder is 0700 on Unix).
    let work = scratch.join("agent-work");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let endpoint = scratch.join("agent-endpoint.json");
    let _ = std::fs::remove_file(&endpoint);
    let listener = openstitch_remote::Server::start(openstitch_remote::ServerConfig {
        write_roots: vec![scratch.clone()],
        endpoint_file: Some(endpoint.clone()),
        reply_timeout: Duration::from_secs(APPROVAL_TIMEOUT_S + 30),
        ..openstitch_remote::ServerConfig::default()
    })
    .map_err(|e| format!("The agent's private socket did not open: {e}"))?;
    let config = claude::AcpConfig {
        program,
        args: agent.args.iter().map(|a| a.to_string()).collect(),
        cwd: work,
        mcp: claude::McpServer {
            name: claude::BRIDGE_NAME.into(),
            command: mcp,
            args: vec!["--assistant-mcp".into(), endpoint.display().to_string()],
            env: Vec::new(),
        },
        system: openstitch_mcp::server::assistant_instructions().to_string(),
        extra_path: claude::search_dirs(),
    };
    let mut w = Worker::start_agent(config, listener, wake);
    w.kind = (Protocol::Agent, id.to_string());
    Ok(w)
}

/// Stop: ends the turn; what it already changed stays, as one undo step.
pub fn stop(app: &mut EditorApp) {
    let panel = &mut app.assistant.panel;
    if let Some(w) = &panel.worker {
        w.cancel.store(true, Ordering::SeqCst);
    }
    for a in panel.approvals.drain(..) {
        a.job
            .answer(json!({ "ok": false, "error": "stopped by the user" }));
    }
    panel.queue.clear();
}

/// New chat: a fresh conversation (the design and its undo history stay).
pub fn new_chat(app: &mut EditorApp) {
    stop(app);
    let panel = &mut app.assistant.panel;
    panel.worker = None;
    panel.items.clear();
    panel.script = ScriptRecord::default();
    panel.last_undo = None;
    panel.tokens = (0, 0);
    panel.turn = None;
}

fn reply_err(job: Job, message: String) {
    job.answer(json!({ "ok": false, "error": message }));
}

/// The effect a command declares (engine or `ui.*`); `None` for unknown commands, which the
/// engine refuses anyway.
fn effect_of(app: &EditorApp, command: &str) -> Option<Effect> {
    if command == "commands.list" {
        return Some(Effect::Read);
    }
    if let Some(c) = openstitch_app::editor::assistant::ui_command(command) {
        return Some(c.effect);
    }
    app.session.permits(command).ok().map(|c| c.effect)
}

/// Runs a job the gate allowed, and counts and records it.
fn run_job(app: &mut EditorApp, job: Job) {
    let command = job.request["command"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let effect = effect_of(app, &command);
    let reply = app.remote_request(&job.request);
    if reply["ok"] == json!(true)
        && let Some(effect) = effect
    {
        let params = job.request.get("params").cloned().unwrap_or(json!({}));
        app.assistant.panel.script.record(&command, effect, &params);
        if effect != Effect::Read
            && let Some(t) = &mut app.assistant.panel.turn
        {
            t.edits += 1;
        }
    }
    job.answer(reply);
}

fn handle_job(app: &mut EditorApp, job: Job) {
    let command = job.request["command"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let Some(effect) = effect_of(app, &command) else {
        run_job(app, job); // the engine's own "unknown command" error
        return;
    };
    let empty = json!({});
    let params = job.request.get("params").unwrap_or(&empty).clone();
    let prefs = &app.ui.prefs.ai;
    let (edits, undo_start) = app
        .assistant
        .panel
        .turn
        .as_ref()
        .map_or((0, app.undo_groups.len()), |t| (t.edits, t.undo_start));
    let v = verdict(&GateInput {
        mode: prefs.mode,
        command: &command,
        effect,
        params: &params,
        scratch: app.assistant.scratch.as_deref(),
        always_allow: &prefs.always_allow,
        edits,
        edit_limit: prefs.edit_limit,
        undo_before_message: command == "history.undo" && app.undo_groups.len() <= undo_start,
    });
    match v {
        Verdict::Run => run_job(app, job),
        Verdict::Refuse(why) => reply_err(job, why),
        Verdict::Ask(what) => app.assistant.panel.approvals.push(Approval {
            job,
            command,
            what,
            since: Instant::now(),
        }),
    }
}

/// The user's answer to a question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Once,
    Always,
    Deny,
}

pub fn answer(app: &mut EditorApp, index: usize, a: Answer) {
    if index >= app.assistant.panel.approvals.len() {
        return;
    }
    let approval = app.assistant.panel.approvals.remove(index);
    match a {
        Answer::Deny => reply_err(
            approval.job,
            format!("the user declined: {}", approval.what),
        ),
        Answer::Always => {
            let list = &mut app.ui.prefs.ai.always_allow;
            if !list.contains(&approval.command) {
                list.push(approval.command.clone());
                app.ui.save_prefs();
            }
            run_job(app, approval.job);
        }
        Answer::Once => run_job(app, approval.job),
    }
}

/// Ends the running turn: folds its undo groups into one, and sends the next queued message.
fn finish_turn(app: &mut EditorApp, end: claude::StopKind, wake: Waker) {
    let Some(turn) = app.assistant.panel.turn.take() else {
        return;
    };
    for a in app.assistant.panel.approvals.drain(..) {
        a.job
            .answer(json!({ "ok": false, "error": "the turn ended" }));
    }
    let start = turn.undo_start.min(app.undo_groups.len());
    let steps: usize = app.undo_groups.drain(start..).sum();
    let panel = &mut app.assistant.panel;
    if claude::edit_request_without_tools(&turn.text, turn.used_tools, &turn.reply) {
        panel.items.push(Item::Notice(
            "The model answered without using its tools, so nothing changed. Choose a model that supports tool calling (for Ollama, one with \"tools\" in its capabilities).".into(),
        ));
    }
    if steps > 0 {
        app.undo_groups.push(steps);
        panel.last_undo = Some(app.undo_groups.len() - 1);
        panel.items.push(Item::Notice(format!(
            "{} change{}, one undo step.",
            turn.edits,
            if turn.edits == 1 { "" } else { "s" }
        )));
    }
    match end {
        claude::StopKind::EndTurn => {}
        claude::StopKind::MaxTokens => panel.items.push(Item::Notice(
            "The answer reached its length limit; a tool call it was writing was not run.".into(),
        )),
        claude::StopKind::Refusal { category } => panel.items.push(Item::Notice(format!(
            "Claude declined this request{}.",
            category.map(|c| format!(" ({c})")).unwrap_or_default()
        ))),
        claude::StopKind::Cancelled => panel.items.push(Item::Notice("Stopped.".into())),
        claude::StopKind::Error(e) => panel.items.push(Item::Notice(format!("Error: {e}"))),
    }
    if !panel.queue.is_empty() {
        let next = std::mem::take(&mut panel.queue).join("\n\n");
        send(app, next, wake);
    }
}

fn on_event(panel: &mut Panel, e: claude::Event) {
    if let Some(t) = &mut panel.turn {
        match &e {
            claude::Event::Text(s) => t.reply.push_str(s),
            claude::Event::ToolStart { .. } => t.used_tools = true,
            _ => {}
        }
    }
    match e {
        claude::Event::Text(t) => match panel.items.last_mut() {
            Some(Item::Assistant(s)) => s.push_str(&t),
            _ => panel.items.push(Item::Assistant(t)),
        },
        claude::Event::Thinking(t) => match panel.items.last_mut() {
            Some(Item::Thinking(s)) => s.push_str(&t),
            _ => panel.items.push(Item::Thinking(t)),
        },
        claude::Event::ToolStart { id, name } => panel.items.push(Item::Tool {
            id,
            name,
            detail: String::new(),
            done: false,
            is_error: false,
        }),
        claude::Event::ToolCall { id, input, .. } => {
            if let Some(Item::Tool { detail, .. }) = panel
                .items
                .iter_mut()
                .rev()
                .find(|i| matches!(i, Item::Tool { id: x, .. } if *x == id))
            {
                *detail = input.to_string().chars().take(300).collect();
            }
        }
        claude::Event::ToolDone {
            id,
            is_error,
            summary,
            ..
        } => {
            if let Some(Item::Tool {
                detail,
                done,
                is_error: err,
                ..
            }) = panel
                .items
                .iter_mut()
                .rev()
                .find(|i| matches!(i, Item::Tool { id: x, .. } if *x == id))
            {
                *done = true;
                *err = is_error;
                if !summary.is_empty() {
                    detail.push_str("\n→ ");
                    detail.push_str(&summary);
                }
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
            "{reason}; trying again in {} s.",
            after.as_secs().max(1)
        ))),
    }
}

/// Runs everything the worker sent; call once per frame (and in tests in a loop).
pub fn serve(app: &mut EditorApp, wake: Waker) {
    let mut events = Vec::new();
    let mut jobs = Vec::new();
    if let Some(w) = &app.assistant.panel.worker {
        events.extend(w.from.try_iter());
        jobs.extend(w.jobs.try_iter());
        // A coding agent's MCP bridge, on the private socket: the same gate, the same undo step.
        if let Some(listener) = &w.listener {
            while let Some(job) = listener.try_next() {
                let request = job.request().clone();
                jobs.push(Job {
                    request,
                    reply: Reply::Socket(job),
                });
            }
        }
    }
    for job in jobs {
        handle_job(app, job);
    }
    // Questions nobody answered are declined.
    let timeout = Duration::from_secs(APPROVAL_TIMEOUT_S);
    let panel = &mut app.assistant.panel;
    let (late, keep): (Vec<_>, Vec<_>) = panel
        .approvals
        .drain(..)
        .partition(|a| a.since.elapsed() >= timeout);
    panel.approvals = keep;
    for a in late {
        reply_err(
            a.job,
            "nobody answered in time; the user did not allow it".into(),
        );
    }
    for e in events {
        match e {
            FromWorker::Event(e) => on_event(&mut app.assistant.panel, e),
            FromWorker::End(end) => finish_turn(app, end, wake.clone()),
        }
    }
}

fn waker(ctx: &egui::Context) -> Waker {
    let ctx = ctx.clone();
    Arc::new(move || ctx.request_repaint())
}

/// Saves the conversation's design changes as a `stitchtool exec` script.
fn save_script(app: &mut EditorApp) {
    match app.assistant.panel.script.script() {
        Err(why) => app
            .assistant
            .panel
            .items
            .push(Item::Notice(format!("No script: {why}."))),
        Ok(script) => {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Command script", &["json"])
                .set_file_name("assistant-script.json")
                .save_file()
            {
                let text = serde_json::to_string_pretty(&script).unwrap_or_default();
                let note = match std::fs::write(&path, text) {
                    Ok(()) => format!(
                        "Saved {} commands to {}.",
                        app.assistant.panel.script.len(),
                        path.display()
                    ),
                    Err(e) => format!("Could not save the script: {e}"),
                };
                app.assistant.panel.items.push(Item::Notice(note));
            }
        }
    }
}

/// Results of the settings' background lookups (they reach the network or a local server).
#[derive(Default)]
pub struct Lookups {
    /// The endpoint's models, for the address they were asked from.
    models: Option<(String, Result<Vec<claude::ModelInfo>, String>)>,
    /// Ollama's answer to "does this model call tools", for the model it was asked about.
    ollama_tools: Option<(String, Option<bool>)>,
    /// Coding agents found on this computer (A3), once looked for.
    agents: Option<Vec<claude::Discovered>>,
    /// What is being looked up now.
    busy: Vec<&'static str>,
}

type Shared = Arc<std::sync::Mutex<Lookups>>;

fn lookup_config(app: &EditorApp) -> claude::Config {
    let prefs = &app.ui.prefs.ai;
    claude::Config {
        base_url: prefs.base_url.clone(),
        api_key: load_key(&prefs.base_url).unwrap_or_default(),
        model: prefs.model.clone(),
        request_timeout: Duration::from_secs(30),
        protocol: match prefs.protocol {
            Protocol::OpenAi => claude::Protocol::OpenAi,
            Protocol::Anthropic | Protocol::Agent => claude::Protocol::Anthropic,
        },
        ..claude::Config::default()
    }
}

fn list_models(app: &EditorApp, lookups: &Shared, ctx: &egui::Context) {
    let config = lookup_config(app);
    lookups
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .busy
        .push("models");
    let (out, ctx) = (lookups.clone(), ctx.clone());
    std::thread::spawn(move || {
        let r = claude::list_models(&config, &claude::UreqTransport);
        let mut l = out.lock().unwrap_or_else(|e| e.into_inner());
        l.models = Some((config.base_url.clone(), r));
        l.busy.retain(|b| *b != "models");
        ctx.request_repaint();
    });
}

fn check_ollama_tools(app: &EditorApp, lookups: &Shared, ctx: &egui::Context) {
    let (base, model) = (
        app.ui.prefs.ai.base_url.clone(),
        app.ui.prefs.ai.model.clone(),
    );
    if model.is_empty() || !is_this_computer(&base) {
        return;
    }
    let (out, ctx) = (lookups.clone(), ctx.clone());
    std::thread::spawn(move || {
        let r = claude::ollama_tools_capability(&base, &model, &claude::UreqTransport);
        out.lock().unwrap_or_else(|e| e.into_inner()).ollama_tools = Some((model, r));
        ctx.request_repaint();
    });
}

fn settings(app: &mut EditorApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let lookups = app.assistant.panel.lookups.clone();
    let mut changed = false;
    let mut model_changed = false;
    let mut want_models = false;
    let mut want_agents = false;
    let prefs = &mut app.ui.prefs.ai;
    egui::Grid::new("assistant-settings")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Endpoint");
            egui::ComboBox::from_id_salt("assistant-preset")
                .selected_text(
                    PRESETS
                        .iter()
                        .find(|p| p.base_url == prefs.base_url && p.protocol == prefs.protocol)
                        .map_or("Custom", |p| p.name),
                )
                .show_ui(ui, |ui| {
                    for p in PRESETS {
                        if ui.selectable_label(prefs.base_url == p.base_url, p.name).clicked() {
                            prefs.protocol = p.protocol;
                            prefs.base_url = p.base_url.to_string();
                            if !p.model.is_empty() || prefs.protocol == Protocol::Anthropic {
                                prefs.model = p.model.to_string();
                            }
                            changed = true;
                            model_changed = true;
                        }
                    }
                });
            ui.end_row();
            ui.label("Protocol");
            egui::ComboBox::from_id_salt("assistant-protocol")
                .selected_text(prefs.protocol.label())
                .show_ui(ui, |ui| {
                    for p in Protocol::ALL {
                        changed |= ui.selectable_value(&mut prefs.protocol, p, p.label()).changed();
                    }
                });
            ui.end_row();
            if prefs.protocol != Protocol::Agent {
                ui.label("Address");
                changed |= ui.text_edit_singleline(&mut prefs.base_url).lost_focus();
                ui.end_row();
                ui.label("Model");
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut prefs.model).desired_width(170.0));
                    if r.lost_focus() {
                        changed = true;
                        model_changed = true;
                    }
                    let l = lookups.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some((base, Ok(models))) = &l.models
                        && *base == prefs.base_url
                        && !models.is_empty()
                    {
                        egui::ComboBox::from_id_salt("assistant-models")
                            .selected_text("▾")
                            .width(30.0)
                            .show_ui(ui, |ui| {
                                for m in models {
                                    let text = m.name.as_deref().map_or(m.id.clone(), |n| format!("{n} ({})", m.id));
                                    if ui.selectable_label(prefs.model == m.id, text).clicked() {
                                        prefs.model = m.id.clone();
                                        changed = true;
                                        model_changed = true;
                                    }
                                }
                            });
                    }
                    let busy = l.busy.contains(&"models");
                    drop(l);
                    if busy {
                        ui.spinner();
                    } else if ui.small_button("List").on_hover_text("Ask the endpoint for its models").clicked() {
                        want_models = true;
                    }
                });
                ui.end_row();
                if prefs.protocol == Protocol::Anthropic {
                    ui.label("Effort");
                    egui::ComboBox::from_id_salt("assistant-effort")
                        .selected_text(prefs.effort.clone())
                        .show_ui(ui, |ui| {
                            for e in openstitch_app::editor::assistant_chat::EFFORTS {
                                changed |= ui.selectable_value(&mut prefs.effort, e.to_string(), e).changed();
                            }
                        });
                } else {
                    ui.label("Reasoning effort");
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut prefs.reasoning_effort).hint_text("provider default").desired_width(120.0))
                        .on_hover_text("Sent as reasoning_effort when set (for example low, medium, high); values differ per provider")
                        .lost_focus();
                }
                ui.end_row();
                ui.label("Request timeout (s)");
                changed |= ui
                    .add(egui::DragValue::new(&mut prefs.request_timeout_s).range(30..=3600))
                    .on_hover_text("How long one model request may take; a large local model may need more")
                    .changed();
                ui.end_row();
            } else {
                ui.label("Agent");
                let l = lookups.lock().unwrap_or_else(|e| e.into_inner());
                let found = l.agents.clone();
                drop(l);
                egui::ComboBox::from_id_salt("assistant-agent")
                    .selected_text(
                        claude::KNOWN_AGENTS
                            .iter()
                            .find(|a| a.id == prefs.agent)
                            .map_or(prefs.agent.as_str(), |a| a.name),
                    )
                    .show_ui(ui, |ui| {
                        for a in claude::KNOWN_AGENTS {
                            let state = found.as_ref().and_then(|f| {
                                f.iter().find(|d| d.agent.id == a.id).map(|d| match &d.state {
                                    claude::AgentState::Ready { version, .. } => format!(" · {version}"),
                                    claude::AgentState::NotInstalled => " · not installed".to_string(),
                                    claude::AgentState::DoesNotStart(_) => " · does not start".to_string(),
                                })
                            });
                            let text = format!("{}{}", a.name, state.unwrap_or_default());
                            if ui.selectable_label(prefs.agent == a.id, text).clicked() {
                                prefs.agent = a.id.to_string();
                                changed = true;
                            }
                        }
                    });
                ui.end_row();
                if let Some(a) = claude::KNOWN_AGENTS.iter().find(|a| a.id == prefs.agent) {
                    let missing = found.as_ref().is_some_and(|f| {
                        f.iter().any(|d| d.agent.id == a.id && !matches!(d.state, claude::AgentState::Ready { .. }))
                    });
                    if missing {
                        ui.label("Install");
                        ui.horizontal(|ui| {
                            ui.monospace(a.install);
                            if ui.small_button("Copy").clicked() {
                                ui.ctx().copy_text(a.install.to_string());
                            }
                        });
                        ui.end_row();
                    }
                }
                if found.is_none() {
                    want_agents = true;
                }
            }
            ui.label("Permissions");
            egui::ComboBox::from_id_salt("assistant-mode")
                .selected_text(prefs.mode.label())
                .show_ui(ui, |ui| {
                    for m in PermissionMode::ALL {
                        changed |= ui.selectable_value(&mut prefs.mode, m, m.label()).changed();
                    }
                });
            ui.end_row();
            ui.label("Changes per message");
            changed |= ui
                .add(egui::DragValue::new(&mut prefs.edit_limit).range(0..=10_000))
                .on_hover_text("0 = no limit")
                .changed();
            ui.end_row();
        });
    {
        let l = lookups.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((base, Err(e))) = &l.models
            && *base == prefs.base_url
        {
            ui.colored_label(ui.visuals().warn_fg_color, format!("No model list: {e}"));
        }
        if let Some((model, Some(false))) = &l.ollama_tools
            && *model == prefs.model
        {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "This Ollama model has no \"tools\" capability: it can only chat, not change the design. Pick one that lists tools.",
            );
        }
    }
    let local_openai = prefs.protocol == Protocol::OpenAi && is_this_computer(&prefs.base_url);
    if want_models {
        list_models(app, &lookups, &ctx);
    }
    if want_agents {
        let mut l = lookups.lock().unwrap_or_else(|e| e.into_inner());
        if !l.busy.contains(&"agents") {
            l.busy.push("agents");
            let (out, ctx) = (lookups.clone(), ctx.clone());
            std::thread::spawn(move || {
                let found = claude::discover();
                let mut l = out.lock().unwrap_or_else(|e| e.into_inner());
                l.agents = Some(found);
                l.busy.retain(|b| *b != "agents");
                ctx.request_repaint();
            });
        }
    }
    if model_changed && local_openai {
        check_ollama_tools(app, &lookups, &ctx);
    }
    let prefs = &mut app.ui.prefs.ai;
    if prefs.mode == PermissionMode::All {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "Allow everything: the assistant can save, overwrite and export files without asking.",
        );
    }
    if !prefs.always_allow.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Always allowed: {}", prefs.always_allow.join(", ")));
            if ui.small_button("Reset").clicked() {
                prefs.always_allow.clear();
                changed = true;
            }
        });
    }
    let base_url = prefs.base_url.clone();
    if !claude::key_safe_for(&base_url) {
        ui.colored_label(
            ui.visuals().error_fg_color,
            "This address is neither https nor this computer: the key will not be sent to it.",
        );
    }
    ui.separator();
    // The keychain is asked once per address, not every frame (it can be slow or prompt).
    let stored = match &app.assistant.panel.key_stored {
        Some((url, stored)) if *url == base_url => *stored,
        _ => {
            let stored = load_key(&base_url).is_some();
            app.assistant.panel.key_stored = Some((base_url.clone(), stored));
            stored
        }
    };
    ui.label(if stored {
        "API key: stored in the system keychain."
    } else if is_this_computer(&base_url) {
        "API key: none; a model server on this computer usually needs none."
    } else {
        "API key: none. Add the key for this address (for the Claude API: console.anthropic.com); it is kept in the system keychain."
    });
    ui.horizontal(|ui| {
        let panel = &mut app.assistant.panel;
        ui.add(
            egui::TextEdit::singleline(&mut panel.key_input)
                .password(true)
                .hint_text("Paste a new key")
                .desired_width(180.0),
        );
        if ui.button("Save key").clicked() {
            let note = match store_key(&base_url, panel.key_input.trim()) {
                Ok(()) if panel.key_input.trim().is_empty() => "Key removed.".to_string(),
                Ok(()) => "Key saved in the system keychain.".to_string(),
                Err(e) => format!("Could not use the keychain: {e}"),
            };
            panel.key_input.clear();
            panel.key_stored = None;
            panel.items.push(Item::Notice(note));
        }
    });
    ui.separator();
    ui.label("Image generator (optional): an OpenAI-compatible /images/generations address ending in /v1. Without one, the assistant draws pictures as vector shapes.");
    let prefs = &mut app.ui.prefs.ai;
    ui.horizontal(|ui| {
        changed |= ui
            .add(
                egui::TextEdit::singleline(&mut prefs.image_url)
                    .hint_text("none")
                    .desired_width(170.0),
            )
            .lost_focus();
        changed |= ui
            .add(
                egui::TextEdit::singleline(&mut prefs.image_model)
                    .hint_text("model")
                    .desired_width(90.0),
            )
            .lost_focus();
    });
    let image_url = prefs.image_url.trim().to_string();
    if !image_url.is_empty() {
        ui.horizontal(|ui| {
            let panel = &mut app.assistant.panel;
            ui.add(
                egui::TextEdit::singleline(&mut panel.image_key_input)
                    .password(true)
                    .hint_text("Its key, if it needs one")
                    .desired_width(180.0),
            );
            if ui.button("Save key").clicked() {
                let note = match store_key(&image_url, panel.image_key_input.trim()) {
                    Ok(()) => "Image generator key saved in the system keychain.".to_string(),
                    Err(e) => format!("Could not use the keychain: {e}"),
                };
                panel.image_key_input.clear();
                panel.items.push(Item::Notice(note));
            }
        });
    }
    if changed {
        app.ui.save_prefs();
    }
}

fn item(ui: &mut egui::Ui, it: &Item) {
    match it {
        Item::User(t) => {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.add(egui::Label::new(t).wrap());
                });
            });
        }
        Item::Assistant(t) => {
            ui.add(egui::Label::new(t).wrap().selectable(true));
        }
        Item::Thinking(t) => {
            ui.collapsing(RichText::new("Thinking").weak(), |ui| {
                ui.add(egui::Label::new(RichText::new(t).weak()).wrap());
            });
        }
        Item::Tool {
            name,
            detail,
            done,
            is_error,
            ..
        } => {
            let state = if !done {
                "…"
            } else if *is_error {
                "✗"
            } else {
                "✓"
            };
            ui.collapsing(RichText::new(format!("{state} {name}")).monospace(), |ui| {
                ui.add(egui::Label::new(RichText::new(detail).monospace().small()).wrap());
            });
        }
        Item::Notice(t) => {
            ui.label(RichText::new(t).italics().weak());
        }
    }
}

/// The docked panel (call before the central panel).
pub fn panel(app: &mut EditorApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    serve(app, waker(&ctx));
    if !(app.ui.prefs.ai.enabled && app.assistant.panel.open) {
        return;
    }
    // Files dropped onto the window while the panel is open go with the next message, as 📎
    // (#407).
    let dropped: Vec<_> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .collect()
    });
    if !dropped.is_empty() {
        attach_files(app, dropped);
    }
    egui::Panel::right("assistant")
        .default_size(380.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Assistant");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("×")
                        .on_hover_text("Close (Ctrl+Shift+A)")
                        .clicked()
                    {
                        app.assistant.panel.open = false;
                    }
                    if ui.small_button("⚙").on_hover_text("Settings").clicked() {
                        app.assistant.panel.show_settings ^= true;
                    }
                    if ui.small_button("New chat").clicked() {
                        new_chat(app);
                    }
                    if ui
                        .small_button("Save as script")
                        .on_hover_text(
                            "The design changes of this chat as a stitchtool command script",
                        )
                        .clicked()
                    {
                        save_script(app);
                    }
                });
            });
            ui.label(
                RichText::new(format!(
                    "{} · {} · {} in / {} out tokens",
                    app.ui.prefs.ai.model,
                    app.ui.prefs.ai.mode.label(),
                    app.assistant.panel.tokens.0,
                    app.assistant.panel.tokens.1
                ))
                .small()
                .weak(),
            );
            if app.assistant.panel.show_settings {
                ui.separator();
                settings(app, ui);
            }
            ui.separator();
            // Questions first: they hold the turn.
            let mut answered = None;
            for (i, a) in app.assistant.panel.approvals.iter().enumerate() {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(format!(
                        "The assistant wants to {} ({}).",
                        a.what, a.command
                    ));
                    let left = APPROVAL_TIMEOUT_S.saturating_sub(a.since.elapsed().as_secs());
                    ui.horizontal(|ui| {
                        if ui.button("Allow once").clicked() {
                            answered = Some((i, Answer::Once));
                        }
                        if ui.button("Always allow").clicked() {
                            answered = Some((i, Answer::Always));
                        }
                        if ui.button("Deny").clicked() {
                            answered = Some((i, Answer::Deny));
                        }
                        ui.label(RichText::new(format!("{left} s")).weak());
                    });
                });
            }
            if let Some((i, a)) = answered {
                answer(app, i, a);
            }
            if !app.assistant.panel.approvals.is_empty() {
                ctx.request_repaint_after(Duration::from_secs(1));
            }
            // The composer stays at the bottom; the conversation fills the space above it.
            egui::Panel::bottom("assistant-input").show(ui, |ui| {
                for q in &app.assistant.panel.queue {
                    ui.label(RichText::new(format!("Queued: {q}")).weak());
                }
                let panel = &mut app.assistant.panel;
                if !panel.attachments.is_empty() {
                    let mut remove = None;
                    ui.horizontal_wrapped(|ui| {
                        for (i, a) in panel.attachments.iter().enumerate() {
                            if ui
                                .small_button(format!("📎 {} ×", a.name))
                                .on_hover_text("Remove")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        }
                    });
                    if let Some(i) = remove {
                        let a = panel.attachments.remove(i);
                        let _ = std::fs::remove_file(a.path);
                    }
                }
                let edit = ui.add(
                    egui::TextEdit::multiline(&mut app.assistant.panel.input)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text("Ask for a design, a change, or anything in the editor…"),
                );
                let enter = edit.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                if edit.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    stop(app);
                }
                ui.horizontal(|ui| {
                    if ui
                        .small_button("📎")
                        .on_hover_text("Attach pictures, SVG, text or stitch files (up to 8)")
                        .clicked()
                        && let Some(files) = rfd::FileDialog::new()
                            .add_filter(
                                "Pictures, SVG, text, stitch files",
                                &[
                                    "png", "jpg", "jpeg", "bmp", "svg", "txt", "md", "csv", "json",
                                    "pes", "pec", "dst", "exp", "jef", "vp3", "xxx", "hus",
                                ],
                            )
                            .pick_files()
                    {
                        attach_files(app, files);
                    }
                    let panel = &mut app.assistant.panel;
                    ui.toggle_value(&mut panel.attach_view, "View")
                        .on_hover_text("Send a picture of the design with the message");
                    ui.add_enabled_ui(!app.selection.is_empty(), |ui| {
                        ui.toggle_value(&mut app.assistant.panel.attach_selection, "Selection")
                            .on_hover_text(
                                "Send the selected objects' ids, kinds and sizes with the message",
                            );
                    });
                    let busy = app.assistant.panel.busy();
                    if (ui.button(if busy { "Queue" } else { "Send" }).clicked() || enter)
                        && !app.assistant.panel.input.trim().is_empty()
                    {
                        let text = std::mem::take(&mut app.assistant.panel.input);
                        send(app, text, waker(&ctx));
                    }
                    if busy && ui.button("Stop").clicked() {
                        stop(app);
                    }
                    if busy {
                        ui.spinner();
                    }
                });
            });
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for it in &app.assistant.panel.items {
                        item(ui, it);
                        ui.add_space(4.0);
                    }
                    if let Some(last) = app.assistant.panel.last_undo
                        && last + 1 == app.undo_groups.len()
                        && ui.small_button("Undo these changes").clicked()
                    {
                        app.undo();
                        app.assistant.panel.last_undo = None;
                    }
                });
        });
}

/// Shared slot a scripted transport reads its replies from (tests only).
#[cfg(test)]
pub mod testing {
    use super::*;
    use std::sync::Mutex;

    /// Returns the next scripted SSE body for each request and keeps the request bodies.
    pub struct Scripted {
        pub replies: Mutex<std::collections::VecDeque<String>>,
        pub requests: Mutex<Vec<Value>>,
        pub headers: Mutex<Vec<Vec<(String, String)>>>,
    }

    impl claude::Transport for Scripted {
        fn post(
            &self,
            _url: &str,
            headers: &[(String, String)],
            body: &[u8],
            _timeout: Duration,
        ) -> Result<Box<dyn std::io::Read + Send>, claude::HttpError> {
            self.headers.lock().expect("lock").push(headers.to_vec());
            self.requests
                .lock()
                .expect("lock")
                .push(serde_json::from_slice(body).unwrap_or(Value::Null));
            let next = self
                .replies
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or_default();
            Ok(Box::new(std::io::Cursor::new(next.into_bytes())))
        }
    }

    fn sse(event: &str, data: Value) -> String {
        format!("event: {event}\ndata: {data}\n\n")
    }

    /// A streamed assistant message: tool calls `(command, params)` through `execute`, or
    /// plain text when there are none.
    pub fn reply(calls: &[(&str, Value)], text: &str, stop: &str) -> String {
        let mut s = sse(
            "message_start",
            json!({ "type": "message_start", "message": { "id": "msg", "type": "message", "role": "assistant",
                "content": [], "model": "claude-opus-5-5", "stop_reason": null,
                "usage": { "input_tokens": 10, "output_tokens": 1 } } }),
        );
        let mut index = 0;
        if !text.is_empty() {
            s += &sse(
                "content_block_start",
                json!({ "type": "content_block_start", "index": index, "content_block": { "type": "text", "text": "" } }),
            );
            s += &sse(
                "content_block_delta",
                json!({ "type": "content_block_delta", "index": index, "delta": { "type": "text_delta", "text": text } }),
            );
            s += &sse(
                "content_block_stop",
                json!({ "type": "content_block_stop", "index": index }),
            );
            index += 1;
        }
        for (i, (command, params)) in calls.iter().enumerate() {
            let input = json!({ "command": command, "params": params }).to_string();
            let (a, b) = input.split_at(input.len() / 2);
            s += &sse(
                "content_block_start",
                json!({ "type": "content_block_start", "index": index,
                "content_block": { "type": "tool_use", "id": format!("tool{i}"), "name": "execute", "input": {} } }),
            );
            for part in [a, b] {
                s += &sse(
                    "content_block_delta",
                    json!({ "type": "content_block_delta", "index": index,
                    "delta": { "type": "input_json_delta", "partial_json": part } }),
                );
            }
            s += &sse(
                "content_block_stop",
                json!({ "type": "content_block_stop", "index": index }),
            );
            index += 1;
        }
        s += &sse(
            "message_delta",
            json!({ "type": "message_delta", "delta": { "stop_reason": stop, "stop_sequence": null },
            "usage": { "output_tokens": 20 } }),
        );
        s += &sse("message_stop", json!({ "type": "message_stop" }));
        s
    }

    pub fn scripted(replies: Vec<String>) -> Arc<Scripted> {
        Arc::new(Scripted {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
            headers: Mutex::new(Vec::new()),
        })
    }

    fn chunk(choice: Value) -> String {
        format!(
            "data: {}\n\n",
            json!({ "id": "c", "object": "chat.completion.chunk", "model": "m", "choices": [choice] })
        )
    }

    /// A streamed `/chat/completions` reply (A2): tool calls through `execute`, split across
    /// chunks, or plain text.
    pub fn openai_reply(calls: &[(&str, Value)], text: &str, finish: &str) -> String {
        let mut s = String::new();
        if !text.is_empty() {
            s += &chunk(
                json!({ "index": 0, "delta": { "role": "assistant", "content": text }, "finish_reason": null }),
            );
        }
        for (i, (command, params)) in calls.iter().enumerate() {
            let args = json!({ "command": command, "params": params }).to_string();
            let (a, b) = args.split_at(args.len() / 2);
            s += &chunk(
                json!({ "index": 0, "delta": { "tool_calls": [{ "index": i, "id": format!("call{i}"),
                "type": "function", "function": { "name": "execute", "arguments": a } }] }, "finish_reason": null }),
            );
            s += &chunk(json!({ "index": 0, "delta": { "tool_calls": [{ "index": i,
                "function": { "arguments": b } }] }, "finish_reason": null }));
        }
        s += &chunk(json!({ "index": 0, "delta": {}, "finish_reason": finish }));
        s += &format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({ "id": "c", "object": "chat.completion.chunk", "choices": [],
                    "usage": { "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15 } })
        );
        s
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::app::Launch;

    fn no_wake() -> Waker {
        Arc::new(|| {})
    }

    fn app_with(replies: Vec<String>) -> (EditorApp, Arc<Scripted>) {
        let mut app = EditorApp::new(Launch {
            demo: true,
            ..Launch::default()
        });
        let t = scripted(replies);
        app.assistant.panel.transport = Some(t.clone());
        app.assistant.panel.test_key = Some("sk-test".into());
        (app, t)
    }

    /// Serves the worker until the turn ends (or `until` holds).
    fn run(app: &mut EditorApp, until: impl Fn(&EditorApp) -> bool) {
        let start = Instant::now();
        while !until(app) {
            serve(app, no_wake());
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "the turn did not end"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn idle(app: &EditorApp) -> bool {
        !app.assistant.panel.busy()
    }

    fn rect(x: f64) -> (&'static str, Value) {
        (
            "shape.rectangle",
            json!({ "x": x, "y": 10, "width": 10, "height": 10 }),
        )
    }

    #[test]
    fn one_message_is_one_undo_step() {
        let (mut app, transport) = app_with(vec![
            reply(&[rect(10.0), rect(30.0), rect(50.0)], "", "tool_use"),
            reply(&[], "Three boxes.", "end_turn"),
        ]);
        let before = app.session.design().objects.len();
        let groups = app.undo_groups.len();
        send(&mut app, "three boxes".into(), no_wake());
        run(&mut app, idle);
        assert_eq!(app.session.design().objects.len(), before + 3);
        assert_eq!(app.undo_groups.len(), groups + 1, "one undo step");
        assert!(
            app.assistant
                .panel
                .items
                .contains(&Item::Assistant("Three boxes.".into()))
        );
        app.undo();
        assert_eq!(
            app.session.design().objects.len(),
            before,
            "one undo removes all three"
        );
        // The second request carried the three tool results in one user message.
        let requests = transport.requests.lock().expect("lock");
        let last = requests[1]["messages"]
            .as_array()
            .expect("messages")
            .last()
            .cloned()
            .expect("last");
        assert_eq!(last["role"], "user");
        assert_eq!(last["content"].as_array().map(Vec::len), Some(3));
        assert_eq!(requests[0]["model"], "claude-opus-5-5");
    }

    #[test]
    fn files_wait_for_approval_and_a_denial_reaches_the_model() {
        let save = ("project.save", json!({ "path": "/nonexistent-dir/x.osd" }));
        let (mut app, transport) = app_with(vec![
            reply(std::slice::from_ref(&save), "", "tool_use"),
            reply(&[], "You declined.", "end_turn"),
        ]);
        send(&mut app, "save it".into(), no_wake());
        run(&mut app, |a| !a.assistant.panel.approvals.is_empty());
        assert!(
            app.assistant.panel.approvals[0]
                .what
                .contains("/nonexistent-dir/x.osd")
        );
        answer(&mut app, 0, Answer::Deny);
        run(&mut app, idle);
        let requests = transport.requests.lock().expect("lock");
        let result = requests[1]["messages"]
            .as_array()
            .expect("m")
            .last()
            .cloned()
            .expect("last");
        assert!(result.to_string().contains("declined"), "{result}");
    }

    #[test]
    fn an_unanswered_question_is_declined_after_the_timeout() {
        let save = ("design.export", json!({ "path": "/nonexistent-dir/x.pes" }));
        let (mut app, _) = app_with(vec![
            reply(&[save], "", "tool_use"),
            reply(&[], "ok", "end_turn"),
        ]);
        send(&mut app, "export".into(), no_wake());
        run(&mut app, |a| !a.assistant.panel.approvals.is_empty());
        app.assistant.panel.approvals[0].since =
            Instant::now() - Duration::from_secs(APPROVAL_TIMEOUT_S + 1);
        run(&mut app, idle);
        assert!(app.assistant.panel.approvals.is_empty());
    }

    #[test]
    fn stop_ends_the_turn_and_keeps_its_changes_undoable() {
        let save = ("project.save", json!({ "path": "/nonexistent-dir/x.osd" }));
        let (mut app, _) = app_with(vec![
            reply(&[rect(10.0), save], "", "tool_use"),
            reply(&[], "stopped", "end_turn"),
        ]);
        let before = app.session.design().objects.len();
        let groups = app.undo_groups.len();
        send(&mut app, "box then save".into(), no_wake());
        run(&mut app, |a| !a.assistant.panel.approvals.is_empty());
        stop(&mut app);
        run(&mut app, idle);
        assert_eq!(app.session.design().objects.len(), before + 1);
        assert_eq!(app.undo_groups.len(), groups + 1);
        app.undo();
        assert_eq!(app.session.design().objects.len(), before);
    }

    #[test]
    fn the_first_message_gives_the_assistant_a_scratch_folder() {
        let svg = (
            "ui.scratchWrite",
            json!({ "name": "t.svg", "text": "<svg/>" }),
        );
        let (mut app, _) = app_with(vec![
            reply(&[svg], "", "tool_use"),
            reply(&[], "written", "end_turn"),
        ]);
        assert!(app.assistant.scratch.is_none(), "no --listen in this test");
        send(&mut app, "write an svg".into(), no_wake());
        run(&mut app, idle);
        let dir = app.assistant.scratch.clone().expect("scratch folder");
        assert_eq!(
            std::fs::read(dir.join("t.svg")).expect("written"),
            b"<svg/>"
        );
    }

    /// A 120 × 120 PNG: a red disc on white.
    fn disc_png(path: &std::path::Path) {
        let img = image::RgbImage::from_fn(120, 120, |x, y| {
            let (dx, dy) = (x as f64 - 60.0, y as f64 - 60.0);
            if dx * dx + dy * dy < 40.0 * 40.0 {
                image::Rgb([200, 20, 20])
            } else {
                image::Rgb([255, 255, 255])
            }
        });
        img.save(path).expect("png");
    }

    #[test]
    fn files_dropped_on_the_window_attach_only_while_the_panel_is_open() {
        let (mut app, _) = app_with(Vec::new());
        let src =
            std::env::temp_dir().join(format!("openstitch-drop-disc-{}.png", std::process::id()));
        disc_png(&src);
        let ctx = egui::Context::default();
        let drop = |app: &mut EditorApp| {
            let raw = egui::RawInput {
                dropped_files: vec![crate::script::DroppedPath::handle(src.clone())],
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| panel(app, ui));
            out.textures_delta.clear();
        };
        app.assistant.panel.open = false;
        drop(&mut app);
        assert!(app.assistant.panel.attachments.is_empty());
        app.assistant.panel.open = true;
        drop(&mut app);
        let _ = std::fs::remove_file(&src);
        assert_eq!(app.assistant.panel.attachments.len(), 1);
        assert!(app.assistant.panel.attachments[0].path.exists());
    }

    #[test]
    fn an_attached_picture_becomes_an_auto_punch_design_in_one_turn() {
        let (mut app, transport) = app_with(Vec::new());
        let src =
            std::env::temp_dir().join(format!("openstitch-a4-disc-{}.png", std::process::id()));
        disc_png(&src);
        attach_files(&mut app, vec![src.clone()]);
        let _ = std::fs::remove_file(&src);
        let path = app.assistant.panel.attachments[0]
            .path
            .display()
            .to_string();
        transport.replies.lock().expect("lock").extend([
            reply(
                &[(
                    "digitize.autoPunch",
                    json!({ "path": path, "compatMode": "openStitch", "width": 50 }),
                )],
                "",
                "tool_use",
            ),
            reply(&[], "Stitched.", "end_turn"),
        ]);
        let before = app.session.design().objects.len();
        let groups = app.undo_groups.len();
        send(&mut app, "stitch this".into(), no_wake());
        run(&mut app, idle);
        let first = transport.requests.lock().expect("lock")[0].clone();
        let content = &first["messages"][0]["content"];
        assert_eq!(content[0]["type"], "image", "the picture goes to the model");
        assert!(
            content[1]["text"]
                .as_str()
                .is_some_and(|t| t.contains(&path) && t.contains("compatMode openStitch"))
        );
        let second = transport.requests.lock().expect("lock")[1].clone();
        let result = second["messages"][2]["content"][0].to_string();
        assert!(!result.contains("is_error\":true"), "{result}");
        assert!(
            app.session.design().objects.len() > before,
            "Auto Punch made objects"
        );
        assert_eq!(app.undo_groups.len(), groups + 1, "one undo step");
        assert!(
            app.assistant.panel.attachments.is_empty(),
            "sent with the message"
        );
        assert!(
            matches!(&app.assistant.panel.items[0], Item::User(t) if t.contains("📎 openstitch-a4-disc"))
        );
    }

    #[test]
    fn an_svg_the_model_writes_is_imported_as_shapes() {
        let (mut app, transport) = app_with(Vec::new());
        let dir = scratch(&mut app).expect("scratch");
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="40mm" viewBox="0 0 40 40"><path d="M5 5 L35 5 L20 35 Z" fill="#1f3a93"/></svg>"##;
        transport.replies.lock().expect("lock").extend([
            reply(
                &[
                    ("ui.scratchWrite", json!({ "name": "tri.svg", "text": svg })),
                    (
                        "import.svg",
                        json!({ "path": dir.join("tri.svg").display().to_string() }),
                    ),
                ],
                "",
                "tool_use",
            ),
            reply(&[], "A triangle.", "end_turn"),
        ]);
        let before = app.session.design().objects.len();
        send(&mut app, "draw a triangle".into(), no_wake());
        run(&mut app, idle);
        let after = app.session.design().objects.len();
        assert!(after > before, "imported as shapes ({before} -> {after})");
    }

    #[test]
    fn without_an_image_generator_the_model_is_pointed_at_the_vector_route() {
        let call =
            reply(&[("x", json!({}))], "", "tool_use").replace("\"execute\"", "\"generate_image\"");
        let (mut app, transport) =
            app_with(vec![call, reply(&[], "I drew it as shapes.", "end_turn")]);
        assert!(app.ui.prefs.ai.image_url.is_empty());
        send(&mut app, "a fox picture".into(), no_wake());
        run(&mut app, idle);
        let first = transport.requests.lock().expect("lock")[0].clone();
        assert!(
            first["tools"]
                .as_array()
                .is_some_and(|t| t.iter().any(|d| d["name"] == "generate_image"))
        );
        let second = transport.requests.lock().expect("lock")[1].clone();
        let result = &second["messages"][2]["content"][0];
        assert_eq!(result["is_error"], true);
        assert!(
            result.to_string().contains("vector route")
                && result.to_string().contains("import.svg")
        );
    }

    #[test]
    fn at_most_eight_files_go_with_a_message() {
        let (mut app, _) = app_with(Vec::new());
        let dir = std::env::temp_dir().join(format!("openstitch-a4-many-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let files: Vec<_> = (0..9)
            .map(|i| {
                let p = dir.join(format!("n{i}.txt"));
                std::fs::write(&p, "x").expect("w");
                p
            })
            .collect();
        attach_files(&mut app, files);
        assert_eq!(app.assistant.panel.attachments.len(), MAX_FILES);
        assert!(
            app.assistant
                .panel
                .items
                .iter()
                .any(|i| matches!(i, Item::Notice(t) if t.contains("At most 8")))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn local_openai(app: &mut EditorApp) {
        app.ui.prefs.ai.protocol = Protocol::OpenAi;
        app.ui.prefs.ai.base_url = "http://localhost:11434/v1".into();
        app.ui.prefs.ai.model = "qwen3".into();
        app.assistant.panel.test_key = None;
    }

    #[test]
    fn a_local_model_needs_no_key_and_its_changes_are_one_undo_step() {
        let (mut app, transport) = app_with(vec![
            openai_reply(&[rect(10.0), rect(30.0)], "", "tool_calls"),
            openai_reply(&[], "Two boxes.", "stop"),
        ]);
        local_openai(&mut app);
        let before = app.session.design().objects.len();
        let groups = app.undo_groups.len();
        send(&mut app, "make two boxes".into(), no_wake());
        run(&mut app, idle);
        assert_eq!(app.session.design().objects.len(), before + 2);
        assert_eq!(app.undo_groups.len(), groups + 1, "one undo step");
        assert!(
            app.assistant
                .panel
                .items
                .contains(&Item::Assistant("Two boxes.".into()))
        );
        let headers = transport.headers.lock().expect("lock");
        assert!(
            headers
                .iter()
                .flatten()
                .all(|(k, _)| !k.eq_ignore_ascii_case("authorization")),
            "no key, no Authorization header"
        );
        let requests = transport.requests.lock().expect("lock");
        assert_eq!(requests[0]["model"], "qwen3");
        assert!(requests[0].get("temperature").is_none());
    }

    #[test]
    fn changing_the_protocol_starts_a_new_conversation() {
        let (mut app, transport) = app_with(vec![
            reply(&[], "Claude here.", "end_turn"),
            openai_reply(&[], "Local here.", "stop"),
        ]);
        send(&mut app, "hello".into(), no_wake());
        run(&mut app, idle);
        local_openai(&mut app);
        send(&mut app, "hello again".into(), no_wake());
        run(&mut app, idle);
        let items = &app.assistant.panel.items;
        assert!(
            items
                .iter()
                .any(|i| matches!(i, Item::Notice(n) if n.contains("new conversation")))
        );
        assert!(items.contains(&Item::Assistant("Local here.".into())));
        let requests = transport.requests.lock().expect("lock");
        // The local model's request carries only the new conversation: system + one user turn.
        assert_eq!(
            requests[1]["messages"].as_array().map(Vec::len),
            Some(2),
            "{}",
            requests[1]
        );
    }

    #[test]
    fn a_model_that_only_talks_gets_a_notice() {
        let (mut app, _) = app_with(vec![openai_reply(
            &[],
            "Sure, here is a badge: ...",
            "stop",
        )]);
        local_openai(&mut app);
        send(&mut app, "make a badge".into(), no_wake());
        run(&mut app, idle);
        assert!(
            app.assistant
                .panel
                .items
                .iter()
                .any(|i| matches!(i, Item::Notice(n) if n.contains("without using its tools"))),
            "{:?}",
            app.assistant.panel.items
        );
    }

    #[test]
    fn a_remote_endpoint_without_a_key_is_not_called() {
        let (mut app, transport) = app_with(vec![openai_reply(&[], "hi", "stop")]);
        app.ui.prefs.ai.protocol = Protocol::OpenAi;
        app.ui.prefs.ai.base_url = "https://openrouter.ai/api/v1".into();
        app.assistant.panel.test_key = None;
        send(&mut app, "hello".into(), no_wake());
        assert!(!app.assistant.panel.busy());
        assert!(transport.requests.lock().expect("lock").is_empty());
    }

    /// A coding agent's MCP bridge reaches the editor only through the panel's private socket,
    /// and every request there passes the same gate (A3).
    #[test]
    fn the_agent_socket_goes_through_the_gate() {
        let mut app = EditorApp::new(Launch {
            demo: true,
            ..Launch::default()
        });
        let listener = openstitch_remote::Server::start(openstitch_remote::ServerConfig::default())
            .expect("listener");
        let endpoint = listener.endpoint().clone();
        let config = claude::AcpConfig {
            program: std::path::PathBuf::from("no-such-agent-program"),
            args: Vec::new(),
            cwd: std::env::temp_dir(),
            mcp: claude::McpServer {
                name: claude::BRIDGE_NAME.into(),
                command: std::path::PathBuf::from("no-such-mcp"),
                args: Vec::new(),
                env: Vec::new(),
            },
            system: String::new(),
            extra_path: Vec::new(),
        };
        app.assistant.panel.worker = Some(Worker::start_agent(config, listener, no_wake()));
        let before = app.session.design().objects.len();
        // The bridge's requests come from another thread, as from openstitch-mcp.
        let client = std::thread::spawn(move || {
            let mut c = openstitch_remote::Client::new(endpoint).expect("connect");
            let edit = c.handle(&json!({ "command": "shape.rectangle",
                "params": { "x": 5, "y": 5, "width": 10, "height": 10 } }));
            let save = c.handle(&json!({ "command": "project.save",
                "params": { "path": "/nonexistent-dir/x.osd" } }));
            (edit, save)
        });
        let start = Instant::now();
        while app.assistant.panel.approvals.is_empty() {
            serve(&mut app, no_wake());
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "no approval asked"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            app.session.design().objects.len(),
            before + 1,
            "the edit ran"
        );
        assert_eq!(app.assistant.panel.approvals[0].command, "project.save");
        answer(&mut app, 0, Answer::Deny);
        let (edit, save) = client.join().expect("client");
        assert_eq!(edit["ok"], json!(true), "{edit}");
        assert_eq!(save["ok"], json!(false));
        assert!(
            save["error"]
                .as_str()
                .is_some_and(|e| e.contains("declined")),
            "{save}"
        );
        // A wrong token is refused before anything reaches the gate.
        let w = app.assistant.panel.worker.as_ref().expect("worker");
        let mut bad = w.listener.as_ref().expect("listener").endpoint().clone();
        bad.token = "0".repeat(64);
        let r = std::thread::spawn(move || {
            openstitch_remote::Client::new(bad)
                .expect("connect")
                .handle(&json!({ "command": "design.info" }))
        });
        let start = Instant::now();
        while !r.is_finished() {
            serve(&mut app, no_wake());
            assert!(start.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(r.join().expect("join")["ok"], json!(false));
    }

    #[test]
    fn a_key_is_never_sent_to_a_plain_http_address_elsewhere() {
        let (mut app, transport) = app_with(vec![reply(&[], "hi", "end_turn")]);
        app.ui.prefs.ai.base_url = "http://example.com".into();
        send(&mut app, "hello".into(), no_wake());
        assert!(!app.assistant.panel.busy());
        assert!(transport.requests.lock().expect("lock").is_empty());
        assert!(
            matches!(app.assistant.panel.items.last(), Some(Item::Notice(n)) if n.contains("not sent"))
        );
    }

    #[test]
    fn save_as_script_rebuilds_the_design() {
        let (mut app, _) = app_with(vec![
            reply(
                &[
                    ("design.new", json!({ "width": 100, "height": 100 })),
                    rect(10.0),
                    rect(40.0),
                ],
                "",
                "tool_use",
            ),
            reply(&[], "done", "end_turn"),
        ]);
        send(&mut app, "two boxes on a new page".into(), no_wake());
        run(&mut app, idle);
        let script = app.assistant.panel.script.script().expect("script");
        let mut s = openstitch_engine::Session::new(std::env::temp_dir());
        for step in script.as_array().expect("steps") {
            s.execute(
                step["command"].as_str().expect("command"),
                Some(&step["params"]),
            )
            .expect("step runs");
        }
        assert_eq!(s.design().objects, app.session.design().objects);
    }

    #[test]
    fn messages_typed_during_a_turn_wait_in_the_queue() {
        let (mut app, transport) = app_with(vec![
            reply(&[rect(10.0)], "", "tool_use"),
            reply(&[], "one", "end_turn"),
            reply(&[], "two", "end_turn"),
        ]);
        send(&mut app, "first".into(), no_wake());
        send(&mut app, "second".into(), no_wake());
        assert_eq!(app.assistant.panel.queue, vec!["second".to_string()]);
        run(&mut app, |a| {
            idle(a)
                && a.assistant.panel.queue.is_empty()
                && a.assistant
                    .panel
                    .items
                    .contains(&Item::Assistant("two".into()))
        });
        assert_eq!(transport.requests.lock().expect("lock").len(), 3);
    }
}
