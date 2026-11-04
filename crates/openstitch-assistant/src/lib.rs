//! The OpenStitch assistant's model client (design: `docs/research/beyond/ai-integration.md`,
//! section 4.3).
//!
//! [`Conversation::run_turn`] sends one user turn to `POST {base_url}/v1/messages` (the Claude
//! API, [`Protocol::Anthropic`]) or `POST {base_url}/chat/completions` (an OpenAI-compatible
//! endpoint such as Ollama, LM Studio or OpenRouter, [`Protocol::OpenAi`]) as a streamed
//! request, shows the reply as it arrives through [`Event`]s, runs the tool calls the model asks
//! for through a [`Tools`] host, sends their results back, and repeats until the model ends its
//! turn, the reply is cut off or declined, the user stops it, or an error ends it.
//!
//! There is no official Anthropic SDK for Rust, so this is raw HTTP over a [`Transport`]
//! ([`UreqTransport`] in the editor, a scripted fake in the tests). Nothing here is async and
//! nothing touches global state.
//!
//! [`acp`] is the other way to reach a model: an Agent Client Protocol coding agent (Claude Code,
//! Codex, OpenCode, ...) started as a child process on the user's own account.
//!
//! The history is append-only: the assistant's content blocks (thinking blocks included, with
//! their signatures) go back exactly as they arrived, and earlier messages are never edited, so
//! preserved thinking and the prompt cache both hold across turns.

pub mod acp;
mod conversation;
mod images;
mod models;
mod openai;
mod sse;
mod transport;

use std::time::Duration;

use serde_json::{Value, json};

pub use acp::{
    AcpConfig, AcpConversation, AcpError, AgentState, AuthMethod, BRIDGE_NAME, Discovered,
    KNOWN_AGENTS, KnownAgent, McpServer, claude_disallowed_tools, discover, discover_in,
    search_dirs,
};
pub use conversation::Conversation;
pub use images::{GENERATE_IMAGE, ImageConfig, NO_GENERATOR, WithImages, generate_image};
pub use models::{ModelInfo, edit_request_without_tools, list_models, ollama_tools_capability};
pub use transport::UreqTransport;

/// The model the assistant uses unless the user picks another.
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
/// The Claude API.
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
/// The `anthropic-version` header (the Claude API version current in 2026-10).
pub const ANTHROPIC_VERSION: &str = "2026-10-21";
/// The beta that turns on server-side refusal fallbacks with `fallbacks: "default"`.
pub const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// The most tool calls run from one assistant message; extra ones get error results.
pub const MAX_TOOL_CALLS_PER_MESSAGE: usize = 32;

/// The wire protocol of the endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Protocol {
    /// The Claude Messages API: `POST {base_url}/v1/messages`.
    #[default]
    Anthropic,
    /// OpenAI-compatible chat completions: `POST {base_url}/chat/completions`, where the base
    /// URL already ends in `/v1` (`http://localhost:11434/v1`, `https://openrouter.ai/api/v1`).
    OpenAi,
}

/// How to reach the model and what to ask of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub protocol: Protocol,
    /// Scheme and host, optionally with a port and a path prefix. Anthropic: `/v1/messages` is
    /// appended. OpenAI-compatible: `/chat/completions` is appended (the URL ends in `/v1`).
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// Anthropic only: `output_config.effort`: `low`, `medium`, `high`, `xhigh` or `max`.
    pub effort: String,
    /// OpenAI-compatible only: sent as `reasoning_effort` when set (the values a model takes
    /// differ by model and server).
    pub reasoning_effort: Option<String>,
    /// Anthropic only. OpenAI-compatible requests send no output limit (the server's default).
    pub max_tokens: u32,
    /// The longest wait for the response headers, and for each read of the stream after that.
    pub request_timeout: Duration,
    /// The system prompt. Keep it stable: volatile context belongs in the user turn.
    pub system: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            protocol: Protocol::Anthropic,
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key: String::new(),
            model: DEFAULT_MODEL.to_string(),
            effort: "high".to_string(),
            reasoning_effort: None,
            max_tokens: 64_000,
            request_timeout: Duration::from_secs(300),
            system: String::new(),
        }
    }
}

/// A request that failed before the stream began: an HTTP status with the server's message, or
/// a connection failure or timeout (`status: None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpError {
    pub status: Option<u16>,
    pub message: String,
    /// The `retry-after` header, when the server sent one.
    pub retry_after: Option<Duration>,
}

/// Sends the HTTP request. The editor uses [`UreqTransport`]; tests script the responses.
pub trait Transport: Send + Sync {
    /// POST; Ok = a reader over the response body for a 2xx status (the SSE stream).
    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &[u8],
        timeout: Duration,
    ) -> Result<Box<dyn std::io::Read + Send>, HttpError>;

    /// GET; Ok = a reader over the response body for a 2xx status. Used for the model lists.
    /// The default refuses, so a transport that only posts keeps compiling.
    fn get(
        &self,
        url: &str,
        headers: &[(String, String)],
        timeout: Duration,
    ) -> Result<Box<dyn std::io::Read + Send>, HttpError> {
        let _ = (url, headers, timeout);
        Err(HttpError {
            status: None,
            message: "This transport cannot send GET requests.".to_string(),
            retry_after: None,
        })
    }
}

/// What a tool call returned, ready to go back to the model.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    /// Anthropic `tool_result` content blocks: `{type: "text", text}` or
    /// `{type: "image", source: {type: "base64", media_type, data}}`.
    pub content: Vec<Value>,
    pub is_error: bool,
}

/// The tools the model may call (the editor's `openstitch-mcp --assistant` meta tools).
pub trait Tools {
    /// MCP-style definitions: {name, description, inputSchema, ...}, in a fixed order.
    fn definitions(&self) -> Vec<Value>;
    fn call(&mut self, name: &str, input: &Value) -> ToolResult;
}

/// Why a turn ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopKind {
    /// The model finished its reply.
    EndTurn,
    /// The reply reached `max_tokens` (or the context window); its tool calls were not run.
    MaxTokens,
    /// The request was declined (`stop_reason: "refusal"`); `category` is from `stop_details`.
    Refusal { category: Option<String> },
    /// The user pressed Stop.
    Cancelled,
    /// A readable message for the chat.
    Error(String),
}

/// What happens during a turn, in order, for the panel to show.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Text delta.
    Text(String),
    /// Summarized thinking delta.
    Thinking(String),
    /// The model began a tool call (its input is still streaming).
    ToolStart { id: String, name: String },
    /// Just before the tool runs.
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    /// The tool ran (or was refused); `summary` is the start of its text result.
    ToolDone {
        id: String,
        name: String,
        is_error: bool,
        summary: String,
    },
    /// Token use of one request.
    Usage {
        input_tokens: u64,
        output_tokens: u64,
        cache_read_input_tokens: u64,
        cache_creation_input_tokens: u64,
    },
    /// The request failed and is sent again after `after`.
    Retrying { after: Duration, reason: String },
}

/// Converts an MCP tools/call result ({content:[{type:text,text}|{type:image,data,mimeType}],
/// isError}) into a ToolResult.
pub fn from_mcp_result(result: &Value) -> ToolResult {
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(items) = result.get("content").and_then(Value::as_array) else {
        return ToolResult {
            content: vec![json!({"type": "text", "text": result.to_string()})],
            is_error,
        };
    };
    let content = items
        .iter()
        .map(|item| match item.get("type").and_then(Value::as_str) {
            Some("text") => json!({
                "type": "text",
                "text": item.get("text").and_then(Value::as_str).unwrap_or(""),
            }),
            Some("image") => json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": item.get("mimeType").and_then(Value::as_str).unwrap_or("image/png"),
                    "data": item.get("data").and_then(Value::as_str).unwrap_or(""),
                },
            }),
            // An embedded text resource reads as its text; anything else as its JSON.
            _ => match item.pointer("/resource/text").and_then(Value::as_str) {
                Some(text) => json!({"type": "text", "text": text}),
                None => json!({"type": "text", "text": item.to_string()}),
            },
        })
        .collect();
    ToolResult { content, is_error }
}

/// The parts of a URL the key check needs: lower-case scheme and host (IPv6 in brackets).
pub(crate) fn scheme_and_host(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // Userinfo (`user@host`) is refused outright: it only ever serves to disguise the host.
    if authority.contains('@') {
        return None;
    }
    let (host, port) = if let Some(after) = authority.strip_prefix('[') {
        let end = after.find(']')?;
        let host = &authority[..end + 2];
        (host, &authority[end + 2..])
    } else {
        match authority.find(':') {
            Some(i) => (&authority[..i], &authority[i..]),
            None => (authority, ""),
        }
    };
    if !port.is_empty() {
        let digits = port.strip_prefix(':')?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    }
    if host.is_empty() {
        return None;
    }
    Some((scheme.to_ascii_lowercase(), host.to_ascii_lowercase()))
}

/// Whether the URL's host is exactly localhost / 127.0.0.1 / [::1].
pub(crate) fn is_loopback(url: &str) -> bool {
    matches!(
        scheme_and_host(url.trim()),
        Some((_, host)) if matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]")
    )
}

/// A key may only go to https, or to plain http on exactly localhost / 127.0.0.1 / [::1] (any port).
pub fn key_safe_for(url: &str) -> bool {
    match scheme_and_host(url.trim()) {
        Some((scheme, _)) if scheme == "https" => true,
        Some((scheme, host)) if scheme == "http" => {
            matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]")
        }
        _ => false,
    }
}

/// A user message's content blocks: the text, plus optional PNG/JPEG pictures as base64 image blocks.
///
/// The pictures come first, then the text, as the Claude API recommends.
pub fn user_content(text: &str, images: &[(String, String)]) -> Vec<Value> {
    let mut blocks: Vec<Value> = images
        .iter()
        .map(|(media_type, data)| {
            json!({
                "type": "image",
                "source": {"type": "base64", "media_type": media_type, "data": data},
            })
        })
        .collect();
    if !text.is_empty() {
        blocks.push(json!({"type": "text", "text": text}));
    }
    blocks
}

/// A readable message for a failed request.
pub(crate) fn describe(e: &HttpError) -> String {
    let msg = e.message.trim();
    let detail = if msg.is_empty() {
        String::new()
    } else {
        format!(": {msg}")
    };
    match e.status {
        Some(401) => format!(
            "The API key was not accepted (401){detail}. Check the key in the assistant settings."
        ),
        Some(403) => format!("The API key may not use this model or endpoint (403){detail}."),
        Some(404) => format!("Not found (404){detail}. Check the base URL and the model name."),
        Some(413) => format!("The request is too large (413){detail}."),
        Some(429) => match e.retry_after {
            Some(after) => format!(
                "Rate limited (429){detail}. Try again in {} s.",
                after.as_secs().max(1)
            ),
            None => format!("Rate limited (429){detail}. Try again shortly."),
        },
        Some(529) => format!("The Claude API is overloaded (529){detail}. Try again shortly."),
        Some(s) if s >= 500 => format!("The Claude API had a server error ({s}){detail}."),
        Some(400) => format!("The request was rejected (400){detail}."),
        Some(s) => format!("The request failed (HTTP {s}){detail}."),
        None if msg.is_empty() => "The request failed.".to_string(),
        None => msg.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert_eq!(
            scheme_and_host("HTTPS://Api.Anthropic.com/v1"),
            Some(("https".into(), "api.anthropic.com".into()))
        );
        assert_eq!(
            scheme_and_host("http://[::1]:8080/x"),
            Some(("http".into(), "[::1]".into()))
        );
        assert_eq!(scheme_and_host("http://localhost:abc"), None);
        assert_eq!(scheme_and_host("http://localhost@evil.com"), None);
        assert_eq!(scheme_and_host("localhost"), None);
    }

    #[test]
    fn readable_errors() {
        let e = |status, message: &str| HttpError {
            status,
            message: message.into(),
            retry_after: None,
        };
        assert!(describe(&e(Some(401), "invalid x-api-key")).contains("API key"));
        assert!(describe(&e(Some(529), "")).contains("overloaded"));
        assert!(describe(&e(Some(503), "")).contains("server error"));
        assert_eq!(describe(&e(None, "timed out")), "timed out");
        let mut r = e(Some(429), "slow down");
        r.retry_after = Some(Duration::from_secs(7));
        assert!(describe(&r).contains("7 s"));
    }
}
