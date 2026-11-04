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
use openstitch_app::editor::assistant_chat::{
    APPROVAL_TIMEOUT_S, GateInput, PermissionMode, ScriptRecord, Verdict, verdict,
};
use openstitch_assistant as claude;
use openstitch_engine::Effect;
use openstitch_mcp::EditorLink;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The keychain service the API key is stored under, one entry per endpoint address.
const KEYCHAIN_SERVICE: &str = "OpenStitch assistant";

/// One command from the model, waiting for the UI thread.
struct Job {
    request: Value,
    reply: Sender<Value>,
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
                reply,
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
    Turn(Box<claude::Config>, Vec<Value>),
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
            let mut tools = ServerTools(server);
            let mut conversation = claude::Conversation::new();
            while let Ok(ToWorker::Turn(config, content)) = inbox.recv() {
                stop.store(false, Ordering::SeqCst);
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
}

#[derive(Default)]
pub struct Panel {
    pub open: bool,
    pub items: Vec<Item>,
    pub input: String,
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
    show_settings: bool,
    /// Tests put a scripted transport here; the app uses `ureq`.
    transport: Option<Arc<dyn claude::Transport>>,
    /// Tests put the key here instead of the keychain.
    test_key: Option<String>,
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
        .map_or("an unsaved design".to_string(), |n| n.to_string_lossy().into_owned());
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
    if !claude::key_safe_for(&prefs.base_url) {
        app.assistant.panel.items.push(Item::Notice(format!(
            "The address {} is neither https nor this computer, so the key is not sent. Change it in the Assistant's settings.",
            prefs.base_url
        )));
        return;
    }
    let Some(key) = app.assistant.panel.api_key(&prefs.base_url) else {
        app.assistant.panel.show_settings = true;
        app.assistant.panel.items.push(Item::Notice(
            "Add your Claude API key in the Assistant's settings (⚙) first. It is kept in the system keychain and sent only to the address shown there.".into(),
        ));
        return;
    };
    let config = claude::Config {
        base_url: prefs.base_url.clone(),
        api_key: key,
        model: prefs.model.clone(),
        effort: prefs.effort.clone(),
        request_timeout: Duration::from_secs(prefs.request_timeout_s),
        system: openstitch_mcp::server::assistant_instructions().to_string(),
        ..claude::Config::default()
    };
    let content = claude::user_content(&format!("{text}\n\n{}", context(app)), &[]);
    let panel = &mut app.assistant.panel;
    if panel.worker.is_none() {
        let transport = panel
            .transport
            .clone()
            .unwrap_or_else(|| Arc::new(claude::UreqTransport));
        panel.worker = Some(Worker::start(transport, wake));
    }
    panel.items.push(Item::User(text));
    panel.turn = Some(Turn {
        undo_start: app.undo_groups.len(),
        edits: 0,
    });
    let worker = panel.worker.as_ref().expect("worker");
    if worker
        .to
        .send(ToWorker::Turn(Box::new(config), content))
        .is_err()
    {
        panel.worker = None;
        panel.turn = None;
        panel
            .items
            .push(Item::Notice("The assistant stopped; send again.".into()));
    }
}

/// Stop: ends the turn; what it already changed stays, as one undo step.
pub fn stop(app: &mut EditorApp) {
    let panel = &mut app.assistant.panel;
    if let Some(w) = &panel.worker {
        w.cancel.store(true, Ordering::SeqCst);
    }
    for a in panel.approvals.drain(..) {
        let _ = a
            .job
            .reply
            .send(json!({ "ok": false, "error": "stopped by the user" }));
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
    let _ = job.reply.send(json!({ "ok": false, "error": message }));
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
    let command = job.request["command"].as_str().unwrap_or_default().to_string();
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
    let _ = job.reply.send(reply);
}

fn handle_job(app: &mut EditorApp, job: Job) {
    let command = job.request["command"].as_str().unwrap_or_default().to_string();
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
        let _ = a
            .job
            .reply
            .send(json!({ "ok": false, "error": "the turn ended" }));
    }
    let start = turn.undo_start.min(app.undo_groups.len());
    let steps: usize = app.undo_groups.drain(start..).sum();
    let panel = &mut app.assistant.panel;
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
        reply_err(a.job, "nobody answered in time; the user did not allow it".into());
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
        Err(why) => app.assistant.panel.items.push(Item::Notice(format!("No script: {why}."))),
        Ok(script) => {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Command script", &["json"])
                .set_file_name("assistant-script.json")
                .save_file()
            {
                let text = serde_json::to_string_pretty(&script).unwrap_or_default();
                let note = match std::fs::write(&path, text) {
                    Ok(()) => format!("Saved {} commands to {}.", app.assistant.panel.script.len(), path.display()),
                    Err(e) => format!("Could not save the script: {e}"),
                };
                app.assistant.panel.items.push(Item::Notice(note));
            }
        }
    }
}

fn settings(app: &mut EditorApp, ui: &mut egui::Ui) {
    let mut changed = false;
    let prefs = &mut app.ui.prefs.ai;
    egui::Grid::new("assistant-settings").num_columns(2).show(ui, |ui| {
        ui.label("Address");
        changed |= ui.text_edit_singleline(&mut prefs.base_url).lost_focus();
        ui.end_row();
        ui.label("Model");
        changed |= ui.text_edit_singleline(&mut prefs.model).lost_focus();
        ui.end_row();
        ui.label("Effort");
        egui::ComboBox::from_id_salt("assistant-effort")
            .selected_text(prefs.effort.clone())
            .show_ui(ui, |ui| {
                for e in openstitch_app::editor::assistant_chat::EFFORTS {
                    changed |= ui.selectable_value(&mut prefs.effort, e.to_string(), e).changed();
                }
            });
        ui.end_row();
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
    let stored = load_key(&base_url).is_some();
    ui.label(if stored {
        "API key: stored in the system keychain."
    } else {
        "API key: none. Get one at console.anthropic.com; it is kept in the system keychain."
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
            panel.items.push(Item::Notice(note));
        }
    });
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
    egui::Panel::right("assistant")
        .default_size(380.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Assistant");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text("Close (Ctrl+Shift+A)").clicked() {
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
                        .on_hover_text("The design changes of this chat as a stitchtool command script")
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
                    ui.label(format!("The assistant wants to {} ({}).", a.what, a.command));
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
            let input_height = 110.0;
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height((ui.available_height() - input_height).max(80.0))
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
            ui.separator();
            for q in &app.assistant.panel.queue {
                ui.label(RichText::new(format!("Queued: {q}")).weak());
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
}

/// Shared slot a scripted transport reads its replies from (tests only).
#[cfg(test)]
pub mod testing {
    use super::*;

    /// Returns the next scripted SSE body for each request and keeps the request bodies.
    pub struct Scripted {
        pub replies: Mutex<std::collections::VecDeque<String>>,
        pub requests: Mutex<Vec<Value>>,
    }

    impl claude::Transport for Scripted {
        fn post(
            &self,
            _url: &str,
            _headers: &[(String, String)],
            body: &[u8],
            _timeout: Duration,
        ) -> Result<Box<dyn std::io::Read + Send>, claude::HttpError> {
            self.requests
                .lock()
                .expect("lock")
                .push(serde_json::from_slice(body).unwrap_or(Value::Null));
            let next = self.replies.lock().expect("lock").pop_front().unwrap_or_default();
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
            s += &sse("content_block_start", json!({ "type": "content_block_start", "index": index, "content_block": { "type": "text", "text": "" } }));
            s += &sse("content_block_delta", json!({ "type": "content_block_delta", "index": index, "delta": { "type": "text_delta", "text": text } }));
            s += &sse("content_block_stop", json!({ "type": "content_block_stop", "index": index }));
            index += 1;
        }
        for (i, (command, params)) in calls.iter().enumerate() {
            let input = json!({ "command": command, "params": params }).to_string();
            let (a, b) = input.split_at(input.len() / 2);
            s += &sse("content_block_start", json!({ "type": "content_block_start", "index": index,
                "content_block": { "type": "tool_use", "id": format!("tool{i}"), "name": "execute", "input": {} } }));
            for part in [a, b] {
                s += &sse("content_block_delta", json!({ "type": "content_block_delta", "index": index,
                    "delta": { "type": "input_json_delta", "partial_json": part } }));
            }
            s += &sse("content_block_stop", json!({ "type": "content_block_stop", "index": index }));
            index += 1;
        }
        s += &sse("message_delta", json!({ "type": "message_delta", "delta": { "stop_reason": stop, "stop_sequence": null },
            "usage": { "output_tokens": 20 } }));
        s += &sse("message_stop", json!({ "type": "message_stop" }));
        s
    }

    pub fn scripted(replies: Vec<String>) -> Arc<Scripted> {
        Arc::new(Scripted {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        })
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
            assert!(start.elapsed() < Duration::from_secs(30), "the turn did not end");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn idle(app: &EditorApp) -> bool {
        !app.assistant.panel.busy()
    }

    fn rect(x: f64) -> (&'static str, Value) {
        ("shape.rectangle", json!({ "x": x, "y": 10, "width": 10, "height": 10 }))
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
        assert!(app.assistant.panel.items.contains(&Item::Assistant("Three boxes.".into())));
        app.undo();
        assert_eq!(app.session.design().objects.len(), before, "one undo removes all three");
        // The second request carried the three tool results in one user message.
        let requests = transport.requests.lock().expect("lock");
        let last = requests[1]["messages"].as_array().expect("messages").last().cloned().expect("last");
        assert_eq!(last["role"], "user");
        assert_eq!(last["content"].as_array().map(Vec::len), Some(3));
        assert_eq!(requests[0]["model"], "claude-opus-5-5");
    }

    #[test]
    fn files_wait_for_approval_and_a_denial_reaches_the_model() {
        let save = ("project.save", json!({ "path": "/nonexistent-dir/x.osd" }));
        let (mut app, transport) = app_with(vec![
            reply(&[save.clone()], "", "tool_use"),
            reply(&[], "You declined.", "end_turn"),
        ]);
        send(&mut app, "save it".into(), no_wake());
        run(&mut app, |a| !a.assistant.panel.approvals.is_empty());
        assert!(app.assistant.panel.approvals[0].what.contains("/nonexistent-dir/x.osd"));
        answer(&mut app, 0, Answer::Deny);
        run(&mut app, idle);
        let requests = transport.requests.lock().expect("lock");
        let result = requests[1]["messages"].as_array().expect("m").last().cloned().expect("last");
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
    fn a_key_is_never_sent_to_a_plain_http_address_elsewhere() {
        let (mut app, transport) = app_with(vec![reply(&[], "hi", "end_turn")]);
        app.ui.prefs.ai.base_url = "http://example.com".into();
        send(&mut app, "hello".into(), no_wake());
        assert!(!app.assistant.panel.busy());
        assert!(transport.requests.lock().expect("lock").is_empty());
        assert!(matches!(app.assistant.panel.items.last(), Some(Item::Notice(n)) if n.contains("not sent")));
    }

    #[test]
    fn save_as_script_rebuilds_the_design() {
        let (mut app, _) = app_with(vec![
            reply(&[("design.new", json!({ "width": 100, "height": 100 })), rect(10.0), rect(40.0)], "", "tool_use"),
            reply(&[], "done", "end_turn"),
        ]);
        send(&mut app, "two boxes on a new page".into(), no_wake());
        run(&mut app, idle);
        let script = app.assistant.panel.script.script().expect("script");
        let mut s = openstitch_engine::Session::new(std::env::temp_dir());
        for step in script.as_array().expect("steps") {
            s.execute(step["command"].as_str().expect("command"), Some(&step["params"]))
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
        run(&mut app, |a| idle(a) && a.assistant.panel.queue.is_empty() && a.assistant.panel.items.contains(&Item::Assistant("two".into())));
        assert_eq!(transport.requests.lock().expect("lock").len(), 3);
    }
}
