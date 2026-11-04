// Adapted from SolidCraft (github.com/tangency-labs/solidcraft, commit 6bca7d2), Copyright (c) 2026 SolidCraft contributors, MIT OR Apache-2.0.
//! The assistant through an Agent Client Protocol coding agent (Claude Code via
//! `claude-agent-acp`, Codex via `codex-acp`, OpenCode via `opencode acp`, ...) on the user's own
//! account (design: `docs/research/beyond/ai-integration.md`, sections 2.2 and 4.1; issue #338).
//!
//! The client side of ACP version 1 (<https://agentclientprotocol.com>): [`discover`] finds the
//! agents installed here, [`AcpConversation::start`] spawns one in a process group of its own,
//! says `initialize` and opens a session with OpenStitch's MCP bridge, and
//! [`AcpConversation::prompt`] runs one turn, mapping the agent's `session/update` notifications
//! to the crate's [`Event`]s. The agent gets no file system and no terminal; its permission
//! questions are answered here: our bridge's tools once, everything else refused.
//!
//! Threads: one writes the agent's stdin, one reads its stdout and answers the agent's requests
//! by itself (it never waits on the caller), one keeps the stderr tail. The caller's thread only
//! waits on a channel.

mod discover;
mod process;
mod protocol;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::{Event, StopKind};
pub use discover::{
    AgentState, Discovered, KNOWN_AGENTS, KnownAgent, discover, discover_in, search_dirs,
};
use process::{AgentProcess, Writer};
pub use protocol::PROTOCOL_VERSION;
use protocol::{AUTH_REQUIRED, METHOD_NOT_FOUND};

/// The name the session gives OpenStitch's MCP server. Not `openstitch`, the name a user's own
/// MCP setup would take, so an agent that merges its own MCP servers with the session's can't
/// shadow the bridge with a headless one.
pub const BRIDGE_NAME: &str = "openstitch-app";

/// The Claude Code adapter's own shell, file and web tools, turned off for the session.
pub fn claude_disallowed_tools() -> &'static [&'static str] {
    protocol::CLAUDE_DISALLOWED_TOOLS
}

/// The MCP server the agent starts to reach the editor (`openstitch-mcp --assistant`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServer {
    /// Its name in the session; the bridge is [`BRIDGE_NAME`].
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// How to start the agent and its session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// The session's working folder (an empty one); made absolute.
    pub cwd: PathBuf,
    /// The bridge. Permission questions about tools named `mcp__{mcp.name}__*` are allowed.
    pub mcp: McpServer,
    /// Appended to the agent's system prompt.
    pub system: String,
    /// Folders put first on the agent's PATH (before [`search_dirs`]).
    pub extra_path: Vec<PathBuf>,
}

/// A sign-in method the agent offers (`authMethods` in its `initialize` result,
/// <https://agentclientprotocol.com/protocol/v1/authentication>).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthMethod {
    /// `type: "terminal"`: the user runs the agent's own login in a terminal.
    Command {
        id: String,
        name: String,
        description: Option<String>,
    },
    /// `type: "agent"` (or absent, or unknown): the agent signs in by itself through
    /// `authenticate`.
    Other { id: String, name: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcpError {
    /// The program is not there.
    NotFound(String),
    /// It did not start, or ended or stalled before the session opened (with its stderr tail).
    Start(String),
    /// `session/new` answered "authentication required" (-32000); the agent's sign-in methods.
    AuthRequired(Vec<AuthMethod>),
    /// It answered something we can't use.
    Protocol(String),
}

impl std::fmt::Display for AcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcpError::NotFound(m) | AcpError::Start(m) | AcpError::Protocol(m) => f.write_str(m),
            AcpError::AuthRequired(_) => f.write_str("The agent needs you to sign in first."),
        }
    }
}

impl std::error::Error for AcpError {}

/// How long `initialize` and `session/new` may each take (Node adapters start slowly).
const START_LIMIT: Duration = Duration::from_secs(60);
/// How long a cancelled turn gets to end before the agent is stopped.
const CANCEL_GRACE: Duration = Duration::from_secs(5);
/// The longest tool summary.
const SUMMARY_CHARS: usize = 200;

/// What the reader thread passes on.
enum FromAgent {
    Response {
        id: u64,
        result: Result<Value, (i64, String)>,
    },
    /// A `session/update`'s `update` object.
    Update(Value),
    /// A line for the chat (a refused tool).
    Note(String),
    Exited,
}

/// What the reader thread knows about each tool call, to judge permission questions whose
/// `toolCall` carries only the id (it is a ToolCallUpdate: only changed fields).
#[derive(Default, Clone)]
struct ToolInfo {
    title: Option<String>,
    name: Option<String>,
    kind: Option<String>,
}

struct Reader {
    writer: Writer,
    tx: Sender<FromAgent>,
    bridge: String,
    cancelling: Arc<AtomicBool>,
    tools: HashMap<String, ToolInfo>,
}

impl Reader {
    fn line(&mut self, line: &str) {
        // A line that is not JSON (a stray log) is ignored.
        let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else {
            return;
        };
        let method = msg.get("method").and_then(Value::as_str);
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        match (method, msg.get("id")) {
            (None, Some(id)) => {
                let Some(id) = id.as_u64() else { return };
                let result = match msg.get("error") {
                    Some(e) => Err((
                        e.get("code").and_then(Value::as_i64).unwrap_or(0),
                        e.get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("error")
                            .to_string(),
                    )),
                    None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = self.tx.send(FromAgent::Response { id, result });
            }
            (Some("session/request_permission"), Some(id)) => self.permission(id.clone(), &params),
            // fs/* and terminal/* (we declared neither), and anything else: "method not found",
            // so every request id gets an answer.
            (Some(m), Some(id)) => self.writer.send(&protocol::reply_error(
                id.clone(),
                METHOD_NOT_FOUND,
                &format!("Method not found: {m}"),
            )),
            (Some("session/update"), None) => {
                if let Some(u) = params.get("update") {
                    self.remember(u);
                    let _ = self.tx.send(FromAgent::Update(u.clone()));
                }
            }
            _ => {}
        }
    }

    fn remember(&mut self, u: &Value) {
        if !matches!(
            u.get("sessionUpdate").and_then(Value::as_str),
            Some("tool_call" | "tool_call_update")
        ) {
            return;
        }
        let Some(id) = u.get("toolCallId").and_then(Value::as_str) else {
            return;
        };
        if self.tools.len() > 4096 {
            self.tools.clear();
        }
        let info = self.tools.entry(id.to_string()).or_default();
        merge(info, u);
    }

    /// `session/request_permission` (<https://agentclientprotocol.com/protocol/tool-calls>):
    /// answered `{outcome: {outcome: "selected", optionId}}`, or `{outcome: {outcome:
    /// "cancelled"}}` while the turn is being cancelled (prompt-turn page: the client MUST).
    fn permission(&mut self, id: Value, params: &Value) {
        let call = params.get("toolCall").cloned().unwrap_or(Value::Null);
        let mut info = call
            .get("toolCallId")
            .and_then(Value::as_str)
            .and_then(|t| self.tools.get(t))
            .cloned()
            .unwrap_or_default();
        merge(&mut info, &call);
        let options: Vec<(String, String)> = params
            .get("options")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .take(16)
                    .filter_map(|o| {
                        Some((
                            o.get("optionId")?.as_str()?.to_string(),
                            o.get("kind")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let pick = |kinds: &[&str]| {
            kinds.iter().find_map(|k| {
                options
                    .iter()
                    .find(|(_, kind)| kind == k)
                    .map(|(id, _)| id.clone())
            })
        };
        let cancelled = json!({"outcome": {"outcome": "cancelled"}});
        let outcome = if self.cancelling.load(Ordering::SeqCst) {
            cancelled
        } else if protocol::is_bridge_tool(
            &self.bridge,
            info.title.as_deref(),
            info.name.as_deref(),
            info.kind.as_deref(),
        ) {
            match pick(&["allow_once"]) {
                Some(option) => json!({"outcome": {"outcome": "selected", "optionId": option}}),
                None => cancelled,
            }
        } else {
            let label = info
                .title
                .or(info.name)
                .unwrap_or_else(|| "a tool".to_string());
            let label: String = label.chars().take(80).collect();
            let _ = self.tx.send(FromAgent::Note(format!(
                "\n[OpenStitch refused the agent's tool \u{201c}{label}\u{201d}: only OpenStitch's own tools may run.]\n"
            )));
            match pick(&["reject_once", "reject_always"]) {
                Some(option) => json!({"outcome": {"outcome": "selected", "optionId": option}}),
                None => cancelled,
            }
        };
        self.writer.send(&protocol::reply(id, outcome));
    }
}

/// Copy the fields a tool call update sets (only changed fields are present).
fn merge(info: &mut ToolInfo, u: &Value) {
    let s = |k: &str| u.get(k).and_then(Value::as_str).map(str::to_string);
    if let Some(t) = s("title") {
        info.title = Some(t);
    }
    if let Some(n) = s("name") {
        info.name = Some(n);
    }
    if let Some(k) = s("kind") {
        info.kind = Some(k);
    }
}

/// One conversation with an ACP agent: its process, the reader thread and the session.
pub struct AcpConversation {
    process: AgentProcess,
    writer: Writer,
    rx: Receiver<FromAgent>,
    session: String,
    next_id: u64,
    cancelling: Arc<AtomicBool>,
    exited: bool,
}

impl std::fmt::Debug for AcpConversation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpConversation")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl AcpConversation {
    /// Spawns the agent (own process group, stdin/stdout piped, stderr kept as a short tail),
    /// sends `initialize` and `session/new` with the bridge, Claude Code's `disallowedTools` and
    /// the system prompt. Blocks until the session is open (up to a minute per step).
    pub fn start(cfg: &AcpConfig) -> Result<AcpConversation, AcpError> {
        let mut cmd = discover::command_for(&cfg.program, &cfg.extra_path);
        cmd.args(&cfg.args).current_dir(&cfg.cwd);
        let (tx, rx) = channel::<FromAgent>();
        let cancelling = Arc::new(AtomicBool::new(false));
        // The reader needs the writer, which exists only once the process does.
        let slot: Arc<Mutex<Option<Reader>>> = Arc::new(Mutex::new(None));
        let (slot_r, tx_exit) = (slot.clone(), tx.clone());
        let process = AgentProcess::spawn(
            cmd,
            move |line| {
                if let Ok(mut r) = slot_r.lock()
                    && let Some(r) = r.as_mut()
                {
                    r.line(line);
                }
            },
            move || {
                let _ = tx_exit.send(FromAgent::Exited);
            },
        )
        .map_err(|e| {
            let what = cfg.program.display();
            if e.kind() == std::io::ErrorKind::NotFound {
                AcpError::NotFound(format!("{what} was not found: {e}"))
            } else {
                AcpError::Start(format!("{what} did not start: {e}"))
            }
        })?;
        let writer = process.writer();
        if let Ok(mut r) = slot.lock() {
            *r = Some(Reader {
                writer: writer.clone(),
                tx,
                bridge: cfg.mcp.name.clone(),
                cancelling: cancelling.clone(),
                tools: HashMap::new(),
            });
        }
        let mut conv = AcpConversation {
            process,
            writer,
            rx,
            session: String::new(),
            next_id: 0,
            cancelling,
            exited: false,
        };
        // On any error below, dropping `conv` stops the agent.
        let init = conv.call("initialize", protocol::initialize_params())?;
        if init.get("protocolVersion").and_then(Value::as_u64) != Some(PROTOCOL_VERSION) {
            return Err(AcpError::Protocol(format!(
                "The agent speaks ACP version {}, OpenStitch speaks version {PROTOCOL_VERSION}.",
                init.get("protocolVersion").unwrap_or(&Value::Null)
            )));
        }
        let params = protocol::session_new_params(&cfg.cwd, &cfg.mcp, &cfg.system, &init);
        let session = match conv.call("session/new", params) {
            Err(AcpError::Protocol(m)) if m.starts_with(&format!("[{AUTH_REQUIRED}]")) => {
                return Err(AcpError::AuthRequired(protocol::auth_methods(&init)));
            }
            other => other?,
        };
        conv.session = session
            .get("sessionId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AcpError::Protocol("The agent opened no session.".into()))?
            .to_string();
        Ok(conv)
    }

    /// The agent's stderr tail as a suffix for a message.
    fn stderr_note(&self) -> String {
        let tail = self.process.stderr_tail();
        if tail.trim().is_empty() {
            String::new()
        } else {
            format!("\n{tail}")
        }
    }

    fn send_request(&mut self, method: &str, params: Value) -> u64 {
        self.next_id += 1;
        self.writer
            .send(&protocol::request(self.next_id, method, params));
        self.next_id
    }

    /// One request during start; an error reply is `Protocol("[code] message")`.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, AcpError> {
        let id = self.send_request(method, params);
        let deadline = Instant::now() + START_LIMIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(FromAgent::Response { id: got, result }) if got == id => {
                    return result.map_err(|(code, message)| {
                        AcpError::Protocol(format!("[{code}] {method} failed: {message}"))
                    });
                }
                Ok(FromAgent::Exited) | Err(RecvTimeoutError::Disconnected) => {
                    self.exited = true;
                    return Err(AcpError::Start(format!(
                        "The agent ended before answering {method}.{}",
                        self.stderr_note()
                    )));
                }
                // Updates and notes before the session exists have no turn to go to.
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {
                    return Err(AcpError::Start(format!(
                        "The agent did not answer {method} within {} s.{}",
                        START_LIMIT.as_secs(),
                        self.stderr_note()
                    )));
                }
            }
        }
    }

    /// Sends `session/prompt` with the user's text and runs the turn: `session/update`s become
    /// [`Event`]s (`agent_message_chunk` → Text, `agent_thought_chunk` → Thinking, `tool_call` /
    /// `tool_call_update` → ToolStart / ToolCall / ToolDone), and the reply's `stopReason` the
    /// [`StopKind`] (<https://agentclientprotocol.com/protocol/prompt-turn>). When `cancel` is
    /// set, `session/cancel` is sent; if the turn has not ended 5 s later, the agent and
    /// everything it started are stopped and the turn ends Cancelled.
    pub fn prompt(
        &mut self,
        text: &str,
        cancel: &AtomicBool,
        on_event: &mut dyn FnMut(Event),
    ) -> StopKind {
        if self.exited {
            return StopKind::Error(format!("The agent has stopped.{}", self.stderr_note()));
        }
        // Updates left over from an earlier turn are not this turn's.
        while let Ok(msg) = self.rx.try_recv() {
            if matches!(msg, FromAgent::Exited) {
                self.exited = true;
                return StopKind::Error(format!("The agent has stopped.{}", self.stderr_note()));
            }
        }
        self.cancelling.store(false, Ordering::SeqCst);
        let session = self.session.clone();
        let id = self.send_request("session/prompt", protocol::prompt_params(&session, text));
        let mut turn = Turn::default();
        let mut cancel_deadline: Option<Instant> = None;
        loop {
            if cancel_deadline.is_none() && cancel.load(Ordering::SeqCst) {
                self.cancelling.store(true, Ordering::SeqCst);
                self.writer.send(&protocol::notification(
                    "session/cancel",
                    protocol::cancel_params(&session),
                ));
                cancel_deadline = Some(Instant::now() + CANCEL_GRACE);
            }
            if cancel_deadline.is_some_and(|d| Instant::now() >= d) {
                self.stop();
                turn.finish_cancelled(on_event);
                return StopKind::Cancelled;
            }
            match self.rx.recv_timeout(Duration::from_millis(50)) {
                Ok(FromAgent::Update(u)) => turn.update(&u, on_event),
                Ok(FromAgent::Note(n)) => on_event(Event::Text(n)),
                Ok(FromAgent::Response { id: got, result }) if got == id => {
                    return match result {
                        Ok(v) => {
                            let stop = stop_kind(&v);
                            if stop == StopKind::Cancelled {
                                turn.finish_cancelled(on_event);
                            }
                            stop
                        }
                        Err(_) if cancel_deadline.is_some() => {
                            turn.finish_cancelled(on_event);
                            StopKind::Cancelled
                        }
                        Err((code, message)) => {
                            StopKind::Error(format!("The agent failed ({code}): {message}"))
                        }
                    };
                }
                Ok(FromAgent::Response { .. }) | Err(RecvTimeoutError::Timeout) => {}
                Ok(FromAgent::Exited) | Err(RecvTimeoutError::Disconnected) => {
                    self.exited = true;
                    if cancel_deadline.is_some() {
                        self.stop();
                        turn.finish_cancelled(on_event);
                        return StopKind::Cancelled;
                    }
                    return StopKind::Error(format!(
                        "The agent stopped during the reply.{}",
                        self.stderr_note()
                    ));
                }
            }
        }
    }

    /// Kills the agent and everything it started (blocks for up to about three seconds).
    pub fn stop(&mut self) {
        self.exited = true;
        self.process.shutdown();
    }
}

impl Drop for AcpConversation {
    fn drop(&mut self) {
        self.stop();
    }
}

/// `stopReason` (StopReason: `end_turn`, `max_tokens`, `max_turn_requests`, `refusal`,
/// `cancelled`; <https://agentclientprotocol.com/protocol/prompt-turn>). `max_turn_requests`
/// (the agent's own limit of model requests in one turn) is reported as MaxTokens: the reply
/// was cut off by a limit.
fn stop_kind(result: &Value) -> StopKind {
    match result.get("stopReason").and_then(Value::as_str) {
        Some("end_turn") => StopKind::EndTurn,
        Some("max_tokens" | "max_turn_requests") => StopKind::MaxTokens,
        Some("refusal") => StopKind::Refusal { category: None },
        Some("cancelled") => StopKind::Cancelled,
        Some(other) => StopKind::Error(format!(
            "The agent ended the turn with \u{201c}{other}\u{201d}."
        )),
        None => StopKind::Error("The agent's reply had no stop reason.".into()),
    }
}

/// The tool calls of one turn, in order.
#[derive(Default)]
struct Turn {
    order: Vec<String>,
    tools: HashMap<String, TurnTool>,
}

#[derive(Default)]
struct TurnTool {
    name: String,
    called: bool,
    done: bool,
}

impl Turn {
    /// One `session/update` (variants: <https://agentclientprotocol.com/protocol/prompt-turn>;
    /// chunk and tool call fields: the schema's ContentChunk and ToolCall / ToolCallUpdate).
    fn update(&mut self, u: &Value, on_event: &mut dyn FnMut(Event)) {
        match u.get("sessionUpdate").and_then(Value::as_str) {
            Some("agent_message_chunk") => {
                if let Some(t) = u.get("content").and_then(protocol::content_text) {
                    on_event(Event::Text(t));
                }
            }
            Some("agent_thought_chunk") => {
                if let Some(t) = u.get("content").and_then(protocol::content_text) {
                    on_event(Event::Thinking(t));
                }
            }
            Some("tool_call" | "tool_call_update") => self.tool(u, on_event),
            // Plans, modes, commands, usage and the rest: nothing for the chat.
            _ => {}
        }
    }

    fn tool(&mut self, u: &Value, on_event: &mut dyn FnMut(Event)) {
        let Some(id) = u.get("toolCallId").and_then(Value::as_str) else {
            return;
        };
        let label = u
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| u.get("title").and_then(Value::as_str))
            .filter(|s| !s.is_empty());
        if !self.tools.contains_key(id) {
            let name = label.unwrap_or("tool").to_string();
            self.order.push(id.to_string());
            on_event(Event::ToolStart {
                id: id.to_string(),
                name: name.clone(),
            });
            self.tools.insert(
                id.to_string(),
                TurnTool {
                    name,
                    ..TurnTool::default()
                },
            );
        }
        let Some(tool) = self.tools.get_mut(id) else {
            return;
        };
        if let Some(l) = label {
            tool.name = l.to_string();
        }
        if !tool.called
            && let Some(input) = u.get("rawInput").filter(|v| !v.is_null())
        {
            tool.called = true;
            on_event(Event::ToolCall {
                id: id.to_string(),
                name: tool.name.clone(),
                input: input.clone(),
            });
        }
        let status = u.get("status").and_then(Value::as_str);
        if !tool.done && matches!(status, Some("completed" | "failed")) {
            tool.done = true;
            let is_error = status == Some("failed");
            let summary = protocol::tool_output(u)
                .map(|s| s.chars().take(SUMMARY_CHARS).collect())
                .unwrap_or_else(|| if is_error { "Failed" } else { "Done" }.to_string());
            on_event(Event::ToolDone {
                id: id.to_string(),
                name: tool.name.clone(),
                is_error,
                summary,
            });
        }
    }

    /// The prompt-turn page: on cancelling, the client marks unfinished tool calls cancelled.
    fn finish_cancelled(&mut self, on_event: &mut dyn FnMut(Event)) {
        for id in &self.order {
            if let Some(tool) = self.tools.get_mut(id)
                && !tool.done
            {
                tool.done = true;
                on_event(Event::ToolDone {
                    id: id.clone(),
                    name: tool.name.clone(),
                    is_error: true,
                    summary: "Cancelled".into(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_reasons() {
        let s = |r: &str| stop_kind(&json!({"stopReason": r}));
        assert_eq!(s("end_turn"), StopKind::EndTurn);
        assert_eq!(s("max_tokens"), StopKind::MaxTokens);
        assert_eq!(s("max_turn_requests"), StopKind::MaxTokens);
        assert_eq!(s("refusal"), StopKind::Refusal { category: None });
        assert_eq!(s("cancelled"), StopKind::Cancelled);
        assert!(matches!(s("other"), StopKind::Error(_)));
    }

    #[test]
    fn tool_updates_become_events() {
        let mut turn = Turn::default();
        let mut events = Vec::new();
        let mut push = |e| events.push(e);
        turn.update(
            &json!({"sessionUpdate": "tool_call", "toolCallId": "t", "title": "mcp__openstitch-app__batch", "status": "pending"}),
            &mut push,
        );
        turn.update(
            &json!({"sessionUpdate": "tool_call_update", "toolCallId": "t", "rawInput": {"a": 1}, "status": "in_progress"}),
            &mut push,
        );
        turn.update(
            &json!({"sessionUpdate": "tool_call_update", "toolCallId": "t", "status": "completed",
                    "content": [{"type": "content", "content": {"type": "text", "text": "ok"}}]}),
            &mut push,
        );
        turn.update(
            &json!({"sessionUpdate": "tool_call_update", "toolCallId": "t", "status": "completed"}),
            &mut push,
        );
        let name = "mcp__openstitch-app__batch".to_string();
        assert_eq!(
            events,
            vec![
                Event::ToolStart {
                    id: "t".into(),
                    name: name.clone()
                },
                Event::ToolCall {
                    id: "t".into(),
                    name: name.clone(),
                    input: json!({"a": 1})
                },
                Event::ToolDone {
                    id: "t".into(),
                    name,
                    is_error: false,
                    summary: "ok".into()
                },
            ]
        );
    }
}
