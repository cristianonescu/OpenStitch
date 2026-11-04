//! The request body, the stream reader and the tool loop.

use std::io::BufReader;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::sse::SseReader;
use crate::{
    ANTHROPIC_VERSION, Config, Event, FALLBACK_BETA, HttpError, MAX_TOOL_CALLS_PER_MESSAGE,
    Protocol, StopKind, ToolResult, Tools, Transport, describe, key_safe_for, scheme_and_host,
};

pub(crate) const CUT_OFF: &str = "not run: the reply was cut off";
pub(crate) const DECLINED: &str = "not run: the request was declined";
pub(crate) const STOPPED: &str = "stopped by the user";
pub(crate) const BAD_JSON: &str = "tool input was not valid JSON; send it again";
pub(crate) const TOO_MANY: &str =
    "not run: more than 32 tool calls in one message; send the rest again";

/// The longest wait before the one automatic retry.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(20);
/// The wait before the retry when the server named none.
const DEFAULT_RETRY_WAIT: Duration = Duration::from_secs(2);

/// One conversation with the model: the append-only history plus the `tool_result` blocks that
/// still have to open the next user message.
///
/// The history is kept in the format of the protocol that started it (Anthropic content blocks,
/// or OpenAI-compatible chat messages). A conversation does not change protocol: a turn whose
/// [`Config::protocol`] differs from the one that started a non-empty history ends at once with
/// `StopKind::Error("Start a new chat to change the protocol.")`, and nothing is sent.
#[derive(Debug, Clone, Default)]
pub struct Conversation {
    pub(crate) messages: Vec<Value>,
    /// Results for tool calls that were not run (cut off, declined, stopped): the API requires
    /// every `tool_use` to be answered at the start of the next user message. For the
    /// OpenAI-compatible protocol these are `role: tool` messages, sent before the next user
    /// message.
    pub(crate) pending: Vec<Value>,
    /// The protocol of the history.
    pub(crate) protocol: Protocol,
    /// OpenAI-compatible only: the server refused pictures for this model (#416), so they go
    /// as text notes from then on.
    pub(crate) text_only: bool,
}

impl Conversation {
    pub fn new() -> Self {
        Self::default()
    }

    /// The history as sent: `{role, content}` messages, oldest first.
    pub fn messages(&self) -> &[Value] {
        &self.messages
    }

    /// The protocol of the history (the default, Anthropic, while it is empty).
    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// Runs one user turn to its end: streams, runs tools, loops until end_turn / max_tokens /
    /// refusal / cancel / error.
    ///
    /// The user message is appended before the first request and stays in the history whatever
    /// happens, so a failed turn's text is not sent again by itself. An error reply leaves no
    /// assistant message; a cancelled, cut-off or declined one is kept, and its tool calls
    /// are answered with error results at the start of the next turn.
    pub fn run_turn(
        &mut self,
        cfg: &Config,
        transport: &dyn Transport,
        tools: &mut dyn Tools,
        user_content: Vec<Value>,
        cancel: &AtomicBool,
        on_event: &mut dyn FnMut(Event),
    ) -> StopKind {
        if self.messages.is_empty() && self.pending.is_empty() {
            self.protocol = cfg.protocol;
        } else if self.protocol != cfg.protocol {
            return StopKind::Error("Start a new chat to change the protocol.".to_string());
        }
        match cfg.protocol {
            Protocol::Anthropic => {
                self.run_anthropic_turn(cfg, transport, tools, user_content, cancel, on_event)
            }
            Protocol::OpenAi => {
                self.run_openai_turn(cfg, transport, tools, user_content, cancel, on_event)
            }
        }
    }

    fn run_anthropic_turn(
        &mut self,
        cfg: &Config,
        transport: &dyn Transport,
        tools: &mut dyn Tools,
        user_content: Vec<Value>,
        cancel: &AtomicBool,
        on_event: &mut dyn FnMut(Event),
    ) -> StopKind {
        if cfg.api_key.trim().is_empty() {
            return StopKind::Error(NO_KEY.to_string());
        }
        if !key_safe_for(&cfg.base_url) {
            return StopKind::Error(unsafe_address(&cfg.base_url));
        }
        let mut content = std::mem::take(&mut self.pending);
        content.extend(user_content);
        if content.is_empty() {
            return StopKind::Error("There is nothing to send.".to_string());
        }
        self.messages
            .push(json!({"role": "user", "content": content}));

        let tool_defs = tool_definitions(&tools.definitions());
        loop {
            if cancel.load(Ordering::SeqCst) {
                return StopKind::Cancelled;
            }
            let body = request_body(cfg, &tool_defs, &self.messages);
            let url = messages_url(&cfg.base_url);
            let headers = request_headers(cfg);
            let reader =
                match post_with_retry(cfg, transport, &url, &headers, &body, cancel, on_event) {
                    Ok(r) => r,
                    Err(stop) => return stop,
                };
            let reply = read_stream(reader, cancel, on_event);
            let blocks = match reply.end {
                End::Error(msg) => return StopKind::Error(msg),
                End::Cancelled => {
                    let calls = self.commit(reply.blocks);
                    self.pending = calls.iter().map(|c| error_result(&c.id, STOPPED)).collect();
                    return StopKind::Cancelled;
                }
                End::Complete => reply.blocks,
            };
            let calls = self.commit(blocks);
            match reply.stop_reason.as_deref() {
                Some("max_tokens") | Some("model_context_window_exceeded") => {
                    self.pending = calls.iter().map(|c| error_result(&c.id, CUT_OFF)).collect();
                    return StopKind::MaxTokens;
                }
                Some("refusal") => {
                    self.pending = calls
                        .iter()
                        .map(|c| error_result(&c.id, DECLINED))
                        .collect();
                    let category = reply
                        .stop_details
                        .as_ref()
                        .and_then(|d| d.get("category"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    return StopKind::Refusal { category };
                }
                // Server tools paused the turn: send the history back as it is to resume.
                Some("pause_turn") if calls.is_empty() => continue,
                _ if calls.is_empty() => return StopKind::EndTurn,
                _ => {}
            }

            let mut results = Vec::with_capacity(calls.len());
            let mut stopped = false;
            for (i, call) in calls.iter().enumerate() {
                if !stopped && cancel.load(Ordering::SeqCst) {
                    stopped = true;
                }
                let refused = if stopped {
                    Some(STOPPED)
                } else if i >= MAX_TOOL_CALLS_PER_MESSAGE {
                    Some(TOO_MANY)
                } else if !call.valid {
                    Some(BAD_JSON)
                } else {
                    None
                };
                let result = match refused {
                    Some(text) => ToolResult {
                        content: vec![json!({"type": "text", "text": text})],
                        is_error: true,
                    },
                    None => {
                        on_event(Event::ToolCall {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            input: call.input.clone(),
                        });
                        tools.call(&call.name, &call.input)
                    }
                };
                on_event(Event::ToolDone {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    is_error: result.is_error,
                    summary: summary(&result),
                });
                results.push(tool_result_block(&call.id, result));
            }
            if stopped {
                self.pending = results;
                return StopKind::Cancelled;
            }
            self.messages
                .push(json!({"role": "user", "content": results}));
        }
    }

    /// Appends the assistant's blocks as one message (when anything valid is left) and returns
    /// its tool calls.
    fn commit(&mut self, blocks: Vec<Block>) -> Vec<Call> {
        let blocks = history_blocks(blocks);
        // A message of thinking alone (a stream stopped before any answer) is not sent back.
        let answered = blocks.iter().any(|b| {
            !matches!(
                b.value.get("type").and_then(Value::as_str),
                Some("thinking" | "redacted_thinking")
            )
        });
        if !answered {
            return Vec::new();
        }
        let mut calls = Vec::new();
        let mut content = Vec::with_capacity(blocks.len());
        for b in blocks {
            if b.value.get("type").and_then(Value::as_str) == Some("tool_use") {
                calls.push(Call {
                    id: str_field(&b.value, "id"),
                    name: str_field(&b.value, "name"),
                    input: b.value.get("input").cloned().unwrap_or_else(|| json!({})),
                    valid: b.input_valid,
                });
            }
            content.push(b.value);
        }
        self.messages
            .push(json!({"role": "assistant", "content": content}));
        calls
    }
}

struct Call {
    id: String,
    name: String,
    input: Value,
    valid: bool,
}

pub(crate) const NO_KEY: &str = "No API key is set for the assistant.";

/// Why a key is not sent to `url`.
pub(crate) fn unsafe_address(url: &str) -> String {
    format!(
        "The API key is only sent over https, or over http to this computer \
         (localhost); {url} is neither."
    )
}

pub(crate) fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn error_result(id: &str, text: &str) -> Value {
    json!({
        "type": "tool_result",
        "tool_use_id": id,
        "content": [{"type": "text", "text": text}],
        "is_error": true,
    })
}

fn tool_result_block(id: &str, result: ToolResult) -> Value {
    let mut block = json!({"type": "tool_result", "tool_use_id": id});
    if !result.content.is_empty() {
        block["content"] = Value::Array(result.content);
    }
    if result.is_error {
        block["is_error"] = Value::Bool(true);
    }
    block
}

/// The first ~200 characters of a result's text.
pub(crate) fn summary(result: &ToolResult) -> String {
    let text: Vec<&str> = result
        .content
        .iter()
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect();
    let joined = text.join(" ");
    let mut out: String = joined.chars().take(200).collect();
    if joined.chars().count() > 200 {
        out.push('…');
    }
    out
}

/// MCP definitions as Anthropic client tools, in the order given; the last one carries the
/// cache breakpoint.
fn tool_definitions(defs: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = defs
        .iter()
        .map(|d| {
            json!({
                "name": d.get("name").cloned().unwrap_or(Value::Null),
                "description": d.get("description").and_then(Value::as_str).unwrap_or(""),
                "input_schema": d
                    .get("inputSchema")
                    .or_else(|| d.get("input_schema"))
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object"})),
                "eager_input_streaming": true,
            })
        })
        .collect();
    if let Some(last) = out.last_mut() {
        last["cache_control"] = json!({"type": "ephemeral"});
    }
    out
}

/// Whether the base URL is the Claude API itself (server-side fallbacks exist only there).
fn is_claude_api(base_url: &str) -> bool {
    matches!(scheme_and_host(base_url.trim()), Some((_, host)) if host == "api.anthropic.com")
}

fn messages_url(base_url: &str) -> String {
    format!("{}/v1/messages", base_url.trim().trim_end_matches('/'))
}

pub(crate) fn request_body(cfg: &Config, tools: &[Value], messages: &[Value]) -> Vec<u8> {
    let mut body = Map::new();
    body.insert("model".into(), json!(cfg.model));
    body.insert("max_tokens".into(), json!(cfg.max_tokens));
    body.insert("stream".into(), json!(true));
    body.insert(
        "thinking".into(),
        json!({"type": "adaptive", "display": "summarized"}),
    );
    body.insert("output_config".into(), json!({"effort": cfg.effort}));
    if !cfg.system.is_empty() {
        body.insert(
            "system".into(),
            json!([{"type": "text", "text": cfg.system, "cache_control": {"type": "ephemeral"}}]),
        );
    }
    if !tools.is_empty() {
        body.insert("tools".into(), Value::Array(tools.to_vec()));
    }
    // Automatic caching of the growing conversation tail (the tools and system prompt have
    // their own breakpoints above).
    body.insert("cache_control".into(), json!({"type": "ephemeral"}));
    body.insert("messages".into(), Value::Array(messages.to_vec()));
    if is_claude_api(&cfg.base_url) {
        body.insert("fallbacks".into(), json!("default"));
    }
    serde_json::to_vec(&Value::Object(body)).unwrap_or_default()
}

fn request_headers(cfg: &Config) -> Vec<(String, String)> {
    let mut headers = vec![
        ("x-api-key".to_string(), cfg.api_key.trim().to_string()),
        (
            "anthropic-version".to_string(),
            ANTHROPIC_VERSION.to_string(),
        ),
        ("content-type".to_string(), "application/json".to_string()),
    ];
    if is_claude_api(&cfg.base_url) {
        headers.push(("anthropic-beta".to_string(), FALLBACK_BETA.to_string()));
    }
    headers
}

pub(crate) fn retryable(e: &HttpError) -> bool {
    matches!(e.status, Some(429) | Some(500..=599))
}

/// Sends the request; one automatic retry for 429 / 5xx / 529, after the server's
/// `retry-after` (at most 20 s), unless the user stops it meanwhile.
pub(crate) fn post_with_retry(
    cfg: &Config,
    transport: &dyn Transport,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(Event),
) -> Result<Box<dyn std::io::Read + Send>, StopKind> {
    post_with_retry_status(cfg, transport, url, headers, body, cancel, on_event).map_err(|e| e.0)
}

/// [`post_with_retry`], with the HTTP status of a failed request.
pub(crate) fn post_with_retry_status(
    cfg: &Config,
    transport: &dyn Transport,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(Event),
) -> Result<Box<dyn std::io::Read + Send>, (StopKind, Option<u16>)> {
    let mut retried = false;
    loop {
        match transport.post(url, headers, body, cfg.request_timeout) {
            Ok(r) => return Ok(r),
            Err(e) if !retried && retryable(&e) => {
                retried = true;
                let after = e
                    .retry_after
                    .unwrap_or(DEFAULT_RETRY_WAIT)
                    .min(MAX_RETRY_WAIT);
                on_event(Event::Retrying {
                    after,
                    reason: describe(&e),
                });
                let until = Instant::now() + after;
                loop {
                    if cancel.load(Ordering::SeqCst) {
                        return Err((StopKind::Cancelled, None));
                    }
                    let now = Instant::now();
                    if now >= until {
                        break;
                    }
                    std::thread::sleep((until - now).min(Duration::from_millis(50)));
                }
            }
            Err(e) => return Err((StopKind::Error(describe(&e)), e.status)),
        }
    }
}

/// One content block as it streams in.
#[derive(Debug)]
struct Block {
    value: Value,
    /// `input_json_delta` fragments of a tool_use block.
    partial_json: String,
    /// The block received its `content_block_stop`.
    done: bool,
    /// For tool_use: the input parsed as a JSON object.
    input_valid: bool,
}

pub(crate) enum End {
    Complete,
    Cancelled,
    Error(String),
}

struct Reply {
    blocks: Vec<Block>,
    stop_reason: Option<String>,
    stop_details: Option<Value>,
    end: End,
}

fn read_stream(
    reader: Box<dyn std::io::Read + Send>,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(Event),
) -> Reply {
    let mut sse = SseReader::new(BufReader::new(reader));
    let mut reply = Reply {
        blocks: Vec::new(),
        stop_reason: None,
        stop_details: None,
        end: End::Error("The connection closed before the reply finished.".to_string()),
    };
    // Index in the stream → position in `blocks`.
    let mut index: Vec<(u64, usize)> = Vec::new();
    let mut usage = [0u64; 4]; // input, output, cache read, cache creation
    loop {
        if cancel.load(Ordering::SeqCst) {
            reply.end = End::Cancelled;
            return reply;
        }
        let ev = match sse.next_event() {
            Ok(Some(ev)) => ev,
            Ok(None) => return reply,
            Err(e) => {
                reply.end = End::Error(format!("The connection to the Claude API broke: {e}"));
                return reply;
            }
        };
        if ev.data.trim().is_empty() {
            continue;
        }
        let Ok(data) = serde_json::from_str::<Value>(&ev.data) else {
            continue;
        };
        let kind = data
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or(ev.event);
        let at = |index: &[(u64, usize)], data: &Value| -> Option<usize> {
            let i = data.get("index").and_then(Value::as_u64)?;
            index.iter().rev().find(|(k, _)| *k == i).map(|(_, p)| *p)
        };
        match kind.as_str() {
            "message_start" => {
                if let Some(u) = data.pointer("/message/usage") {
                    read_usage(u, &mut usage);
                }
            }
            "content_block_start" => {
                let Some(i) = data.get("index").and_then(Value::as_u64) else {
                    continue;
                };
                let mut value = data
                    .get("content_block")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if value.get("type").and_then(Value::as_str) == Some("tool_use") {
                    on_event(Event::ToolStart {
                        id: str_field(&value, "id"),
                        name: str_field(&value, "name"),
                    });
                    // The input arrives as input_json_delta fragments.
                    value["input"] = json!({});
                }
                index.push((i, reply.blocks.len()));
                reply.blocks.push(Block {
                    value,
                    partial_json: String::new(),
                    done: false,
                    input_valid: false,
                });
            }
            "content_block_delta" => {
                let Some(p) = at(&index, &data) else { continue };
                let block = &mut reply.blocks[p];
                let delta = data.get("delta").cloned().unwrap_or(Value::Null);
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        let t = str_field(&delta, "text");
                        append(&mut block.value, "text", &t);
                        on_event(Event::Text(t));
                    }
                    Some("thinking_delta") => {
                        let t = str_field(&delta, "thinking");
                        append(&mut block.value, "thinking", &t);
                        on_event(Event::Thinking(t));
                    }
                    Some("signature_delta") => {
                        append(
                            &mut block.value,
                            "signature",
                            &str_field(&delta, "signature"),
                        );
                    }
                    Some("input_json_delta") => {
                        block.partial_json.push_str(
                            delta
                                .get("partial_json")
                                .and_then(Value::as_str)
                                .unwrap_or(""),
                        );
                    }
                    Some("citations_delta") => {
                        if let Some(c) = delta.get("citation") {
                            match block.value.get_mut("citations") {
                                Some(Value::Array(list)) => list.push(c.clone()),
                                _ => block.value["citations"] = json!([c]),
                            }
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let Some(p) = at(&index, &data) else { continue };
                let block = &mut reply.blocks[p];
                block.done = true;
                if block.value.get("type").and_then(Value::as_str) == Some("tool_use") {
                    let raw = block.partial_json.trim();
                    let parsed = if raw.is_empty() {
                        Some(json!({}))
                    } else {
                        serde_json::from_str::<Value>(raw)
                            .ok()
                            .filter(Value::is_object)
                    };
                    block.input_valid = parsed.is_some();
                    // History needs an object; an unparsable input goes back as {} and is not run.
                    block.value["input"] = parsed.unwrap_or_else(|| json!({}));
                }
            }
            "message_delta" => {
                if let Some(r) = data.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    reply.stop_reason = Some(r.to_string());
                }
                if let Some(d) = data.pointer("/delta/stop_details")
                    && !d.is_null()
                {
                    reply.stop_details = Some(d.clone());
                }
                if let Some(u) = data.get("usage") {
                    read_usage(u, &mut usage);
                }
                on_event(Event::Usage {
                    input_tokens: usage[0],
                    output_tokens: usage[1],
                    cache_read_input_tokens: usage[2],
                    cache_creation_input_tokens: usage[3],
                });
            }
            "message_stop" => {
                reply.end = End::Complete;
                return reply;
            }
            "error" => {
                let msg = data
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                let kind = data
                    .pointer("/error/type")
                    .and_then(Value::as_str)
                    .unwrap_or("error");
                reply.end = End::Error(match kind {
                    "overloaded_error" => {
                        format!("The Claude API is overloaded: {msg}. Try again shortly.")
                    }
                    _ => format!("The Claude API reported an error ({kind}): {msg}"),
                });
                return reply;
            }
            // `ping` and event types this client does not know.
            _ => {}
        }
    }
}

fn append(value: &mut Value, key: &str, text: &str) {
    match value.get_mut(key) {
        Some(Value::String(s)) => s.push_str(text),
        _ => value[key] = Value::String(text.to_string()),
    }
}

fn read_usage(u: &Value, usage: &mut [u64; 4]) {
    for (slot, key) in [
        "input_tokens",
        "output_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ]
    .iter()
    .enumerate()
    {
        if let Some(n) = u.get(*key).and_then(Value::as_u64) {
            usage[slot] = n;
        }
    }
}

/// The blocks that go into the history:
/// - unfinished blocks (a stopped stream) are dropped, except text already shown;
/// - after a server-side fallback, the blocks before the last `fallback` marker are kept only
///   when they are text or paired server-tool blocks, and the markers themselves are dropped
///   (the fallback model's reply is what continues).
fn history_blocks(blocks: Vec<Block>) -> Vec<Block> {
    let ty = |b: &Block| {
        b.value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let boundary = blocks.iter().rposition(|b| ty(b) == "fallback");
    let result_ids: Vec<String> = blocks
        .iter()
        .filter(|b| ty(b).ends_with("_tool_result"))
        .map(|b| str_field(&b.value, "tool_use_id"))
        .collect();
    blocks
        .into_iter()
        .enumerate()
        .filter(|(i, b)| {
            let t = ty(b);
            if t == "fallback" {
                return false;
            }
            if !b.done {
                return t == "text" && !str_field(&b.value, "text").is_empty();
            }
            match boundary {
                Some(at) if *i < at => {
                    t == "text"
                        || t.ends_with("_tool_result")
                        || (t == "server_tool_use"
                            && result_ids.contains(&str_field(&b.value, "id")))
                }
                _ => true,
            }
        })
        .map(|(_, b)| b)
        .collect()
}
