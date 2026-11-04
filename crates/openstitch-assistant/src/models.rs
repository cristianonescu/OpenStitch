//! Model lists, the Ollama `tools` capability check, and the "answered without its tools" check.
//!
//! Format sources (read 2026-11-04):
//! - [ANT-MODELS] Claude API reference, "List Models" (platform.claude.com/docs/en/api/models/list):
//!   `GET /v1/models`, `limit` "Defaults to `20`. Ranges from `1` to `1000`", response
//!   `{data: [{id, display_name, ...}], has_more, first_id, last_id}`.
//! - [OAI-MODELS] OpenAI API reference, "List models"
//!   (developers.openai.com/api/reference/resources/models/methods/list): `{object: "list",
//!   data: [{id, object, created, owned_by}]}`.
//! - [OPENROUTER] OpenRouter API reference, "List all models"
//!   (openrouter.ai/docs/api/api-reference/models/get-models): `data[]` items carry `id` and
//!   `name` ("Display name of the model").
//! - [OLLAMA-API] Ollama `docs/api.md` and `docs/openapi.yaml` (ollama/ollama, main):
//!   `GET /api/version` answers `{"version": "0.5.1"}`; `POST /api/show` takes `model` and its
//!   response has `capabilities`, "List of supported features". The documented examples list
//!   `completion`, `vision` and `thinking`; the value `tools` is `CapabilityTools` in the
//!   server source `types/model/capability.go`.

use std::io::Read;
use std::time::Duration;

use serde_json::{Value, json};

use crate::{
    ANTHROPIC_VERSION, Config, HttpError, Protocol, Transport, describe, is_loopback, key_safe_for,
    scheme_and_host,
};

/// The most read of a model list (OpenRouter's is a few MB).
const MAX_LIST_BYTES: u64 = 32 * 1024 * 1024;
/// The wait for each Ollama probe.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// One entry of an endpoint's model list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    /// A display name, when the endpoint gives one (Claude API `display_name`, OpenRouter `name`).
    pub name: Option<String>,
}

/// The models the endpoint lists, in its order.
///
/// Anthropic: `GET {base}/v1/models?limit=1000` with `x-api-key` and `anthropic-version`
/// [ANT-MODELS] (one page of the documented maximum; a further page is not fetched).
/// OpenAI-compatible: `GET {base}/models`, with `Authorization: Bearer` only when a key is set
/// [OAI-MODELS]. The key rules of [`crate::Conversation::run_turn`] apply.
pub fn list_models(cfg: &Config, transport: &dyn Transport) -> Result<Vec<ModelInfo>, String> {
    let base = cfg.base_url.trim().trim_end_matches('/');
    let key = cfg.api_key.trim();
    let (url, headers, name_field) = match cfg.protocol {
        Protocol::Anthropic => {
            if key.is_empty() {
                return Err(crate::conversation::NO_KEY.to_string());
            }
            if !key_safe_for(base) {
                return Err(crate::conversation::unsafe_address(base));
            }
            (
                format!("{base}/v1/models?limit=1000"),
                vec![
                    ("x-api-key".to_string(), key.to_string()),
                    (
                        "anthropic-version".to_string(),
                        ANTHROPIC_VERSION.to_string(),
                    ),
                ],
                "display_name",
            )
        }
        Protocol::OpenAi => {
            if key.is_empty() {
                if !(key_safe_for(base) && is_loopback(base)) {
                    return Err(crate::conversation::NO_KEY.to_string());
                }
            } else if !key_safe_for(base) {
                return Err(crate::conversation::unsafe_address(base));
            }
            (
                format!("{base}/models"),
                crate::openai::auth_headers(cfg),
                "name",
            )
        }
    };
    let body = get_json(transport, &url, &headers, cfg.request_timeout)
        .map_err(|e| e.unwrap_or_else(|| "The model list was not understood.".to_string()))?;
    let Some(items) = body.get("data").and_then(Value::as_array) else {
        return Err("The model list was not understood.".to_string());
    };
    Ok(items
        .iter()
        .filter_map(|m| {
            let id = m.get("id").and_then(Value::as_str)?.trim();
            if id.is_empty() {
                return None;
            }
            let name = m
                .get(name_field)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|n| !n.is_empty() && *n != id)
                .map(str::to_string);
            Some(ModelInfo {
                id: id.to_string(),
                name,
            })
        })
        .collect())
}

/// GET and parse JSON. Err(Some) = a readable HTTP failure, Err(None) = not JSON.
fn get_json(
    transport: &dyn Transport,
    url: &str,
    headers: &[(String, String)],
    timeout: Duration,
) -> Result<Value, Option<String>> {
    let reader = transport
        .get(url, headers, timeout)
        .map_err(|e: HttpError| Some(describe(&e)))?;
    read_json(reader).ok_or(None)
}

fn read_json(reader: Box<dyn Read + Send>) -> Option<Value> {
    let mut bytes = Vec::new();
    reader.take(MAX_LIST_BYTES).read_to_end(&mut bytes).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// `scheme://host[:port]` of a URL.
fn origin(url: &str) -> Option<String> {
    let url = url.trim();
    scheme_and_host(url)?;
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    Some(format!("{}://{authority}", scheme.to_ascii_lowercase()))
}

/// Whether an Ollama server says `model` can call tools.
///
/// Only when the base URL's origin answers `GET /api/version` with a `version` (an Ollama
/// server) is `POST /api/show {"model": model}` sent; the answer is whether its `capabilities`
/// contain `"tools"` [OLLAMA-API]. None when the server is not Ollama, on any error, or when the
/// response has no `capabilities` (older servers). No key is sent.
pub fn ollama_tools_capability(
    base_url: &str,
    model: &str,
    transport: &dyn Transport,
) -> Option<bool> {
    let origin = origin(base_url)?;
    let version = get_json(
        transport,
        &format!("{origin}/api/version"),
        &[],
        PROBE_TIMEOUT,
    )
    .ok()?;
    version.get("version").and_then(Value::as_str)?;
    let body = serde_json::to_vec(&json!({"model": model})).ok()?;
    let headers = [("content-type".to_string(), "application/json".to_string())];
    let reader = transport
        .post(
            &format!("{origin}/api/show"),
            &headers,
            &body,
            PROBE_TIMEOUT,
        )
        .ok()?;
    let show = read_json(reader)?;
    let caps = show.get("capabilities")?.as_array()?;
    Some(caps.iter().any(|c| c.as_str() == Some("tools")))
}

/// Verbs that open a request to change the design.
const EDIT_VERBS: &[&str] = &[
    "make",
    "add",
    "create",
    "change",
    "move",
    "delete",
    "remove",
    "rename",
    "set",
    "convert",
    "export",
    "save",
    "draw",
    "resize",
    "rotate",
    "scale",
    "recolor",
    "recolour",
    "fill",
    "replace",
    "insert",
    "duplicate",
    "mirror",
    "flip",
    "digitize",
    "digitise",
];

/// Whether a reply looks like a model that ignored its tools: the user asked to change the design
/// (an English request opening with an edit verb, optionally after "please" or "can / could /
/// would / will you"), the model used no tools, and its reply is not a question back.
///
/// Conservative: anything else (questions, other languages, other openings) is false, so the
/// panel's notice shows only when the case is clear.
pub fn edit_request_without_tools(user_text: &str, used_tools: bool, reply_text: &str) -> bool {
    if used_tools {
        return false;
    }
    // A clarifying question is a fair answer without tools.
    if reply_text.trim_end().ends_with('?') {
        return false;
    }
    let lower = user_text.trim().to_lowercase();
    let mut words = lower
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .peekable();
    if words.peek() == Some(&"please") {
        words.next();
    }
    if matches!(words.peek(), Some(&("can" | "could" | "would" | "will"))) {
        words.next();
        if words.next() != Some("you") {
            return false;
        }
        if words.peek() == Some(&"please") {
            words.next();
        }
    }
    words.next().is_some_and(|w| EDIT_VERBS.contains(&w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins() {
        assert_eq!(
            origin("http://localhost:11434/v1").as_deref(),
            Some("http://localhost:11434")
        );
        assert_eq!(
            origin("HTTPS://openrouter.ai/api/v1").as_deref(),
            Some("https://openrouter.ai")
        );
        assert_eq!(origin("localhost:11434"), None);
    }
}
