//! The OpenAI-compatible protocol: `POST {base_url}/chat/completions`, streamed, with the same
//! tool loop, safety rules and [`Event`]s as the Anthropic one.
//!
//! Format sources (read 2026-11-04):
//! - [OAI-CREATE] OpenAI API reference, "Create chat completion"
//!   (developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create;
//!   platform.openai.com/docs/api-reference/chat/create answered 403 to the fetch).
//! - [OAI-CHUNK] OpenAI API reference, the `chat.completion.chunk` streaming object
//!   (developers.openai.com/api/reference/resources/chat/subresources/completions/streaming-events).
//! - [OLLAMA-OAI] Ollama, "OpenAI compatibility" (docs.ollama.com/api/openai-compatibility, the
//!   source `docs/api/openai-compatibility.mdx` in ollama/ollama; the old `docs/openai.md` path
//!   is gone).
//!
//! Not verified against documentation, and handled defensively instead:
//! - the `data:` URL form of an `image_url` part ([OAI-CREATE] says only "Either a URL of the
//!   image or the base64 encoded image data");
//! - an `error` object inside the stream (not part of [OAI-CHUNK]; some compatible servers send
//!   one when a request fails after the stream began);
//! - tool call fragments without an `id` (every chunk example has one on the first fragment).

use std::io::BufReader;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Map, Value, json};

use crate::conversation::stream_error;
use crate::conversation::{
    BAD_JSON, CUT_OFF, DECLINED, End, NO_KEY, STOPPED, TOO_MANY, post_with_retry_status, str_field,
    summary, unsafe_address,
};
use crate::idle::{Progress, Watchdog};
use crate::sse::SseReader;
use crate::{
    Config, Conversation, Event, MAX_TOOL_CALLS_PER_MESSAGE, StopKind, ToolResult, Tools,
    Transport, is_loopback, key_safe_for,
};

/// What goes back in place of a picture: [OAI-CREATE] "For tool messages, only type `text` is
/// supported." The picture itself follows in a user message (A4).
pub(crate) const IMAGE_OMITTED: &str = "(picture: in the next user message)";
/// The text of that user message.
pub(crate) const TOOL_PICTURES: &str = "The pictures from the tool results above:";
/// What replaces a picture for a model that takes text only (#416).
pub(crate) const PICTURE_LEFT_OUT: &str =
    "[A picture was left out here: this model takes text only.]";
/// Ollama's text when it can't parse a tool call's arguments as JSON; it answers 500 then
/// (seen with 0.40.14 and gpt-oss:20b, #427).
const UNREADABLE_CALL: &str = "error parsing tool call";
/// How often one turn asks the model again after such a 500.
const MAX_UNREADABLE_CALLS: usize = 2;
/// What the model is told when Ollama ended its reply with a swallowed error (#429).
pub(crate) const SWALLOWED_NOTE: &str = "[Your last reply stopped early: the server could not \
read its tool call, probably because the arguments were not valid JSON. Send the call again with \
valid JSON.]";
/// The notice when the model is asked again.
pub(crate) const ASKED_AGAIN: &str =
    "The server could not read the model's tool call; asking it again";
/// The notice when the server refused the pictures.
/// The turn's end when the reply came back empty although the model wrote tokens (#425).
pub(crate) const EMPTY_REPLY: &str = "The reply arrived empty although the model wrote";
pub(crate) const TEXT_ONLY: &str =
    "The model refused the pictures (400); they go as text notes from now on";

/// One tool call as its fragments arrive ([OAI-CHUNK] `choices[].delta.tool_calls[]`: `index`,
/// `id`, `type`, `function.name`, `function.arguments`).
#[derive(Debug, Default)]
struct StreamCall {
    index: u64,
    id: String,
    name: String,
    arguments: String,
}

struct Reply {
    text: String,
    refusal: String,
    calls: Vec<StreamCall>,
    finish_reason: Option<String>,
    end: End,
    /// The last chunk had the shape Ollama gives an error it swallows mid-stream (#429).
    swallowed_error: bool,
    /// Reasoning text arrived (`delta.reasoning` or `delta.reasoning_content`).
    reasoned: bool,
    /// The usage's `completion_tokens`, when the server sent it.
    output_tokens: u64,
}

struct Call {
    id: String,
    name: String,
    input: Value,
    valid: bool,
}

impl Conversation {
    pub(crate) fn run_openai_turn(
        &mut self,
        cfg: &Config,
        transport: &dyn Transport,
        tools: &mut dyn Tools,
        user_content: Vec<Value>,
        cancel: &AtomicBool,
        on_event: &mut dyn FnMut(Event),
    ) -> StopKind {
        let key = cfg.api_key.trim();
        if key.is_empty() {
            // A local server (Ollama, LM Studio) needs no key; [OLLAMA-OAI]: "The client
            // requires an API key value, but Ollama ignores it."
            if !(key_safe_for(&cfg.base_url) && is_loopback(&cfg.base_url)) {
                return StopKind::Error(NO_KEY.to_string());
            }
        } else if !key_safe_for(&cfg.base_url) {
            return StopKind::Error(unsafe_address(&cfg.base_url));
        }
        let mut user = user_message(&user_content);
        if self.text_only
            && let Some(u) = &mut user
        {
            leave_out_pictures(u);
        }
        if user.is_none() && self.pending.is_empty() {
            return StopKind::Error("There is nothing to send.".to_string());
        }
        // Unanswered tool calls of the last assistant message are answered first.
        self.messages.append(&mut self.pending);
        if let Some(user) = user {
            self.messages.push(user);
        }

        let tool_defs = tool_definitions(&tools.definitions());
        let url = completions_url(&cfg.base_url);
        let headers = request_headers(cfg);
        let mut unreadable = 0;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return StopKind::Cancelled;
            }
            let body = request_body(cfg, &tool_defs, &self.messages);
            let reader = match post_with_retry_status(
                cfg, transport, &url, &headers, &body, cancel, on_event,
            ) {
                Ok(r) => r,
                // Ollama 0.40.14 answers 400 "Multimodal data provided, but model does not
                // support multimodal requests." for a model without vision (#416): the
                // conversation goes on without its pictures, sent once more.
                Err((StopKind::Error(_), Some(400)))
                    if !self.text_only && leave_out_all_pictures(&mut self.messages) =>
                {
                    self.text_only = true;
                    on_event(Event::Retrying {
                        after: std::time::Duration::ZERO,
                        reason: TEXT_ONLY.to_string(),
                    });
                    continue;
                }
                // The model's reply never arrives, so it is told, as for arguments that are not
                // JSON (`BAD_JSON`), and asked again (#427); resending the same request fails
                // the same way.
                Err((StopKind::Error(msg), Some(500)))
                    if unreadable < MAX_UNREADABLE_CALLS && msg.contains(UNREADABLE_CALL) =>
                {
                    unreadable += 1;
                    let said: String = msg.chars().take(400).collect();
                    self.messages
                        .push(json!({"role": "user", "content": format!(
                            "[Your last reply did not arrive: the server could not read its tool \
                             call arguments as JSON ({said}). Send the call again with valid JSON.]"
                        )}));
                    on_event(Event::Retrying {
                        after: std::time::Duration::ZERO,
                        reason: ASKED_AGAIN.to_string(),
                    });
                    continue;
                }
                Err((stop, _)) => return stop,
            };
            let generated = self.messages.len();
            let mut reply = read_stream(reader, generated, cfg.request_timeout, cancel, on_event);
            match reply.end {
                // Ollama lost an error mid-stream (#429): the same "error parsing tool call" as
                // the 500 above, raised after the reply's text had been sent. The text stays;
                // the unreadable call is asked for again.
                End::Error(_)
                    if reply.swallowed_error
                        && reply.finish_reason.is_none()
                        && unreadable < MAX_UNREADABLE_CALLS =>
                {
                    unreadable += 1;
                    self.commit_openai(reply.text, Vec::new());
                    self.messages
                        .push(json!({"role": "user", "content": SWALLOWED_NOTE}));
                    on_event(Event::Retrying {
                        after: std::time::Duration::ZERO,
                        reason: ASKED_AGAIN.to_string(),
                    });
                    continue;
                }
                End::Error(msg) => return StopKind::Error(msg),
                End::Cancelled => {
                    // Tool calls are known to be whole only once `finish_reason` arrived;
                    // before that they are dropped, and only the text shown is kept.
                    if reply.finish_reason.is_none() {
                        reply.calls.clear();
                    }
                    let calls = self.commit_openai(reply.text, reply.calls);
                    self.pending = calls.iter().map(|c| tool_message(&c.id, STOPPED)).collect();
                    return StopKind::Cancelled;
                }
                End::Complete => {}
            }
            let said_nothing = reply.text.is_empty();
            let calls = self.commit_openai(reply.text, reply.calls);
            // [OAI-CHUNK] finish_reason: "stop", "length", "tool_calls", "content_filter",
            // "function_call" (deprecated; never asked for here). [OAI-CREATE] `refusal`: "The
            // refusal message generated by the model."
            match reply.finish_reason.as_deref() {
                Some("length") => {
                    self.pending = calls.iter().map(|c| tool_message(&c.id, CUT_OFF)).collect();
                    return StopKind::MaxTokens;
                }
                Some("content_filter") => {
                    self.pending = calls
                        .iter()
                        .map(|c| tool_message(&c.id, DECLINED))
                        .collect();
                    return StopKind::Refusal {
                        category: Some("content_filter".to_string()),
                    };
                }
                _ if !reply.refusal.is_empty() => {
                    self.pending = calls
                        .iter()
                        .map(|c| tool_message(&c.id, DECLINED))
                        .collect();
                    return StopKind::Refusal { category: None };
                }
                // Nothing came back although the model wrote tokens and did not reason: a tool call
                // the server could not read. Ollama's generic tool parser drops a call whose
                // arguments hold a key `name` and answers an empty `stop` (#425).
                _ if calls.is_empty()
                    && said_nothing
                    && reply.refusal.is_empty()
                    && !reply.reasoned
                    && reply.output_tokens > 0 =>
                {
                    return StopKind::Error(format!(
                        "{EMPTY_REPLY} {} tokens: the server probably could not read a tool call. \
                         With Ollama, try another model (see the Assistant's notes on models).",
                        reply.output_tokens
                    ));
                }
                // `tool_calls`, and `stop` from servers that report it with calls.
                _ if calls.is_empty() => return StopKind::EndTurn,
                _ => {}
            }

            let mut results = Vec::with_capacity(calls.len());
            let mut pictures = Vec::new();
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
                results.push(json!({
                    "role": "tool",
                    "tool_call_id": call.id,
                    "content": result_text(&result),
                }));
                pictures.extend(
                    result
                        .content
                        .iter()
                        .filter(|b| b.get("type").and_then(Value::as_str) == Some("image"))
                        .cloned(),
                );
            }
            if stopped {
                self.pending = results;
                return StopKind::Cancelled;
            }
            // One `role: tool` message per call, in the order of the calls; tool messages take
            // text only, so their pictures follow as one user message with image parts.
            self.messages.extend(results);
            if !pictures.is_empty() {
                pictures.push(json!({"type": "text", "text": TOOL_PICTURES}));
                if let Some(mut m) = user_message(&pictures) {
                    if self.text_only {
                        leave_out_pictures(&mut m);
                    }
                    self.messages.push(m);
                }
            }
        }
    }

    /// Appends the assistant message (when it has text or tool calls) and returns its calls.
    fn commit_openai(&mut self, text: String, stream_calls: Vec<StreamCall>) -> Vec<Call> {
        if text.is_empty() && stream_calls.is_empty() {
            return Vec::new();
        }
        let mut calls = Vec::with_capacity(stream_calls.len());
        let mut wire = Vec::with_capacity(stream_calls.len());
        let generated = self.messages.len();
        for mut c in stream_calls {
            if c.id.is_empty() {
                c.id = format!("call_{generated}_{}", c.index);
            }
            let raw = c.arguments.trim();
            let parsed = if raw.is_empty() {
                Some(json!({}))
            } else {
                serde_json::from_str::<Value>(raw)
                    .ok()
                    .filter(Value::is_object)
            };
            let valid = parsed.is_some();
            // History needs valid JSON arguments; an unparsable input goes back as {} and is
            // not run.
            let arguments = if valid && !raw.is_empty() {
                raw.to_string()
            } else {
                "{}".to_string()
            };
            // [OAI-CREATE] assistant `tool_calls[]`: {id, type: "function", function: {name,
            // arguments (a JSON string)}}.
            wire.push(json!({
                "id": c.id,
                "type": "function",
                "function": {"name": c.name, "arguments": arguments},
            }));
            calls.push(Call {
                id: c.id,
                name: c.name,
                input: parsed.unwrap_or_else(|| json!({})),
                valid,
            });
        }
        // [OAI-CREATE] assistant `content` is "Required unless `tool_calls` or `function_call`
        // is specified"; it is null when there is no text.
        let mut message = json!({
            "role": "assistant",
            "content": if text.is_empty() { Value::Null } else { Value::String(text) },
        });
        if !wire.is_empty() {
            message["tool_calls"] = Value::Array(wire);
        }
        self.messages.push(message);
        calls
    }
}

/// [OAI-CREATE] tool message: `{role: "tool", tool_call_id, content}`.
fn tool_message(id: &str, text: &str) -> Value {
    json!({"role": "tool", "tool_call_id": id, "content": text})
}

/// A tool result as text: [OAI-CREATE] "For tool messages, only type `text` is supported", so a
/// picture becomes a short note, and an error is marked in the text (there is no `is_error`).
pub(crate) fn result_text(result: &ToolResult) -> String {
    let parts: Vec<String> = result
        .content
        .iter()
        .map(|b| match b.get("type").and_then(Value::as_str) {
            Some("image") => IMAGE_OMITTED.to_string(),
            _ => b
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| b.to_string()),
        })
        .collect();
    let text = parts.join("\n");
    match (result.is_error, text.is_empty()) {
        (true, true) => "Error.".to_string(),
        (true, false) => format!("Error: {text}"),
        (false, true) => "(no output)".to_string(),
        (false, false) => text,
    }
}

/// The user message from the crate's Anthropic-style content blocks ([`crate::user_content`]):
/// plain text as a string; with pictures, [OAI-CREATE] content parts (`{type: "text", text}`,
/// `{type: "image_url", image_url: {url}}`, the picture as a base64 `data:` URL).
fn user_message(blocks: &[Value]) -> Option<Value> {
    if blocks.is_empty() {
        return None;
    }
    let all_text = blocks
        .iter()
        .all(|b| b.get("type").and_then(Value::as_str) == Some("text"));
    if all_text {
        let text: Vec<&str> = blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect();
        return Some(json!({"role": "user", "content": text.join("\n\n")}));
    }
    let parts: Vec<Value> = blocks
        .iter()
        .map(|b| match b.get("type").and_then(Value::as_str) {
            Some("image") => {
                let media = b
                    .pointer("/source/media_type")
                    .and_then(Value::as_str)
                    .unwrap_or("image/png");
                let data = b.pointer("/source/data").and_then(Value::as_str).unwrap_or("");
                json!({"type": "image_url", "image_url": {"url": format!("data:{media};base64,{data}")}})
            }
            _ => json!({"type": "text", "text": str_field(b, "text")}),
        })
        .collect();
    Some(json!({"role": "user", "content": parts}))
}

/// Replaces the `image_url` parts of a message with [`PICTURE_LEFT_OUT`]; whether it had any.
fn leave_out_pictures(message: &mut Value) -> bool {
    let Some(parts) = message.get_mut("content").and_then(Value::as_array_mut) else {
        return false;
    };
    let mut any = false;
    for part in parts.iter_mut() {
        if part.get("type").and_then(Value::as_str) == Some("image_url") {
            *part = json!({"type": "text", "text": PICTURE_LEFT_OUT});
            any = true;
        }
    }
    any
}

/// [`leave_out_pictures`] on every message (all of them, not up to the first with a picture).
fn leave_out_all_pictures(messages: &mut [Value]) -> bool {
    let mut any = false;
    for m in messages {
        any |= leave_out_pictures(m);
    }
    any
}

/// MCP definitions as [OAI-CREATE] function tools: `{type: "function", function: {name,
/// description, parameters}}`, `parameters` being "a JSON Schema object".
pub(crate) fn tool_definitions(defs: &[Value]) -> Vec<Value> {
    defs.iter()
        .map(|d| {
            json!({
                "type": "function",
                "function": {
                    "name": d.get("name").cloned().unwrap_or(Value::Null),
                    "description": d.get("description").and_then(Value::as_str).unwrap_or(""),
                    "parameters": d
                        .get("inputSchema")
                        .or_else(|| d.get("input_schema"))
                        .cloned()
                        .unwrap_or_else(|| json!({"type": "object"})),
                },
            })
        })
        .collect()
}

/// The base URL already ends in `/v1` (`http://localhost:11434/v1`, `https://openrouter.ai/api/v1`).
fn completions_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim().trim_end_matches('/'))
}

/// `Authorization: Bearer` only with a key: a local server needs none.
pub(crate) fn auth_headers(cfg: &Config) -> Vec<(String, String)> {
    let key = cfg.api_key.trim();
    if key.is_empty() {
        Vec::new()
    } else {
        vec![("Authorization".to_string(), format!("Bearer {key}"))]
    }
}

fn request_headers(cfg: &Config) -> Vec<(String, String)> {
    let mut headers = auth_headers(cfg);
    headers.push(("content-type".to_string(), "application/json".to_string()));
    headers
}

/// The request ([OAI-CREATE]). Left out on purpose:
/// - `temperature` (the server's default; reasoning models refuse other values);
/// - an output limit: `max_tokens` is "deprecated in favor of `max_completion_tokens`, and is not
///   compatible with o-series models", while Ollama lists only `max_tokens` [OLLAMA-OAI], so no
///   one field works everywhere and the server's own limit applies;
/// - `tool_choice` (Ollama does not support it [OLLAMA-OAI]; the default is `auto`).
///
/// `stream_options: {include_usage: true}`: "an additional chunk will be streamed before the
/// `data: [DONE]` message" whose `usage` covers the whole request [OAI-CREATE]; Ollama supports
/// it [OLLAMA-OAI]. `reasoning_effort` only when set: OpenAI lists `none` … `max` "for reasoning
/// models" [OAI-CREATE], and Ollama maps the names per model [OLLAMA-OAI].
pub(crate) fn request_body(cfg: &Config, tools: &[Value], messages: &[Value]) -> Vec<u8> {
    let mut body = Map::new();
    body.insert("model".into(), json!(cfg.model));
    body.insert("stream".into(), json!(true));
    body.insert("stream_options".into(), json!({"include_usage": true}));
    let mut all = Vec::with_capacity(messages.len() + 1);
    if !cfg.system.is_empty() {
        all.push(json!({"role": "system", "content": cfg.system}));
    }
    all.extend(messages.iter().cloned());
    body.insert("messages".into(), Value::Array(all));
    if !tools.is_empty() {
        body.insert("tools".into(), Value::Array(tools.to_vec()));
    }
    if let Some(effort) = cfg.reasoning_effort.as_deref().map(str::trim)
        && !effort.is_empty()
    {
        body.insert("reasoning_effort".into(), json!(effort));
    }
    serde_json::to_vec(&Value::Object(body)).unwrap_or_default()
}

/// Whether a chunk is what Ollama's OpenAI layer makes of an error raised mid-stream (#429).
/// Ollama 0.40.14 sends `{"error": ...}` down an HTTP 200 stream when, for example, its gpt-oss
/// parser cannot read a tool call's JSON (`server/routes.go` ChatHandler, `streamResponse`);
/// `middleware/openai.go` `ChatWriter` reads that as an empty response, so the error text is lost
/// and what arrives is one chunk with no model name, empty content and no finish reason, then
/// the end of the stream without `[DONE]`. Ollama's own chunks always carry the model name.
fn is_swallowed_error(chunk: &Value) -> bool {
    let Some(choice) = chunk
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
    else {
        return false;
    };
    chunk.get("model").and_then(Value::as_str) == Some("")
        && choice.get("finish_reason").is_none_or(Value::is_null)
        && choice.get("delta") == Some(&json!({"content": ""}))
}

/// Reads the SSE stream: `data:` lines carrying `chat.completion.chunk` objects, ended by
/// `data: [DONE]` ([OAI-CREATE] `stream_options`, and its streaming example).
fn read_stream(
    reader: Box<dyn std::io::Read + Send>,
    generated: usize,
    timeout: std::time::Duration,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(Event),
) -> Reply {
    let mut sse = SseReader::with_idle(
        BufReader::new(Watchdog::new(reader, timeout, Some(cancel))),
        timeout,
    );
    // Reset by text, reasoning, tool-call pieces and the finish; not by keep-alive chunks with
    // an empty delta or a usage chunk alone (#580).
    let mut progress = Progress::new(timeout);
    let mut reply = Reply {
        text: String::new(),
        refusal: String::new(),
        calls: Vec::new(),
        finish_reason: None,
        end: End::Error("The connection closed before the reply finished.".to_string()),
        swallowed_error: false,
        reasoned: false,
        output_tokens: 0,
    };
    loop {
        if cancel.load(Ordering::SeqCst) {
            reply.end = End::Cancelled;
            return reply;
        }
        let ev = match sse.next_event() {
            Ok(Some(ev)) => ev,
            Ok(None) => {
                // A server that closes without `[DONE]` after the finish reason has still sent
                // the whole reply (only the usage chunk may be missing).
                if reply.finish_reason.is_some() {
                    reply.end = End::Complete;
                }
                return reply;
            }
            Err(e) => {
                reply.end = stream_error(&e, cancel, "endpoint");
                return reply;
            }
        };
        if let Some(msg) = progress.expired() {
            reply.end = End::Error(msg);
            return reply;
        }
        let data = ev.data.trim();
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            reply.end = if reply.finish_reason.is_some() {
                End::Complete
            } else {
                End::Error("The reply ended without a finish reason.".to_string())
            };
            return reply;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        reply.swallowed_error = is_swallowed_error(&chunk);
        if let Some(err) = chunk.get("error").filter(|e| !e.is_null()) {
            let msg = err
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| err.to_string());
            reply.end = End::Error(format!("The endpoint reported an error: {msg}"));
            return reply;
        }
        // [OAI-CHUNK] `usage`: null "except for the last chunk", whose `choices` may be empty.
        if let Some(u) = chunk.get("usage").filter(|u| u.is_object()) {
            let n = |p: &str| u.pointer(p).and_then(Value::as_u64).unwrap_or(0);
            // `prompt_tokens` already counts the cached ones ([OAI-CREATE] usage; Ollama 0.40.14
            // reports CacheN + PromptN), unlike Anthropic's `input_tokens`: count them once.
            let cached = n("/prompt_tokens_details/cached_tokens");
            reply.output_tokens = n("/completion_tokens");
            on_event(Event::Usage {
                input_tokens: n("/prompt_tokens").saturating_sub(cached),
                output_tokens: reply.output_tokens,
                cache_read_input_tokens: cached,
                cache_creation_input_tokens: 0,
            });
        }
        // Only one choice is asked for (`n` defaults to 1).
        let Some(choice) = chunk
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
        else {
            continue;
        };
        if let Some(delta) = choice.get("delta") {
            if let Some(t) = delta.get("content").and_then(Value::as_str)
                && !t.is_empty()
            {
                reply.text.push_str(t);
                progress.made();
                on_event(Event::Text(t.to_string()));
            }
            if let Some(r) = delta.get("refusal").and_then(Value::as_str) {
                reply.refusal.push_str(r);
                progress.made();
            }
            // Reasoning shows as the Thinking row, so a slow local model that reasons for
            // minutes shows that it is still working (#580). It is not sent back.
            if let Some(t) = ["reasoning", "reasoning_content"].iter().find_map(|k| {
                delta
                    .get(*k)
                    .and_then(Value::as_str)
                    .filter(|t| !t.is_empty())
            }) {
                reply.reasoned = true;
                progress.made();
                on_event(Event::Thinking(t.to_string()));
            }
            if let Some(list) = delta.get("tool_calls").and_then(Value::as_array) {
                if !list.is_empty() {
                    progress.made();
                }
                for (pos, frag) in list.iter().enumerate() {
                    let index = frag
                        .get("index")
                        .and_then(Value::as_u64)
                        .unwrap_or(pos as u64);
                    let at = match reply.calls.iter().position(|c| c.index == index) {
                        Some(at) => at,
                        None => {
                            reply.calls.push(StreamCall {
                                index,
                                ..StreamCall::default()
                            });
                            reply.calls.len() - 1
                        }
                    };
                    let call = &mut reply.calls[at];
                    let announce = call.name.is_empty();
                    // The id and name come whole on a call's first fragment; later fragments
                    // carry only `arguments` pieces.
                    if call.id.is_empty()
                        && let Some(id) = frag.get("id").and_then(Value::as_str)
                    {
                        call.id = id.to_string();
                    }
                    if call.name.is_empty()
                        && let Some(name) = frag.pointer("/function/name").and_then(Value::as_str)
                    {
                        call.name = name.to_string();
                    }
                    if let Some(a) = frag.pointer("/function/arguments").and_then(Value::as_str) {
                        call.arguments.push_str(a);
                    }
                    if announce && !call.name.is_empty() {
                        if call.id.is_empty() {
                            // A server that sends no id still needs one for the tool message.
                            call.id = format!("call_{generated}_{index}");
                        }
                        on_event(Event::ToolStart {
                            id: call.id.clone(),
                            name: call.name.clone(),
                        });
                    }
                }
            }
        }
        if let Some(r) = choice.get("finish_reason").and_then(Value::as_str) {
            reply.finish_reason = Some(r.to_string());
            progress.made();
        }
    }
}
