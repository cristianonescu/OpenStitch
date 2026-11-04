// Adapted from SolidCraft (github.com/tangency-labs/solidcraft, commit 6bca7d2), Copyright (c) 2026 SolidCraft contributors, MIT OR Apache-2.0.
//! The messages of the Agent Client Protocol, version 1, as plain data: JSON-RPC over the agent's
//! stdin/stdout, one message per line
//! (<https://agentclientprotocol.com/protocol/v1/transports>). Each builder names the spec page
//! its shape comes from.

use std::path::Path;

use serde_json::{Value, json};

use super::{AuthMethod, McpServer};

/// The protocol version we speak: an integer MAJOR version
/// (<https://agentclientprotocol.com/protocol/initialization>).
pub const PROTOCOL_VERSION: u64 = 1;

/// JSON-RPC "Method not found", our answer to every agent request we don't serve (the ErrorCode
/// list of <https://agentclientprotocol.com/protocol/v1/schema>).
pub const METHOD_NOT_FOUND: i64 = -32601;
/// "Authentication required" (`auth_required`, same ErrorCode list).
pub const AUTH_REQUIRED: i64 = -32000;

pub fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}
pub fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": method, "params": params})
}
pub fn reply(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}
pub fn reply_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// `initialize` params (<https://agentclientprotocol.com/protocol/initialization>; field
/// defaults in the schema's ClientCapabilities / FileSystemCapabilities). No file system and no
/// terminal: the agent reaches the design through the bridge only. `auth.terminal: true` lets an
/// agent advertise `type: "terminal"` sign-in methods, which the panel shows as a command for the
/// user to run (<https://agentclientprotocol.com/protocol/v1/authentication>); the client never
/// runs them itself.
pub fn initialize_params() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "clientCapabilities": {
            "fs": {"readTextFile": false, "writeTextFile": false},
            "terminal": false,
            "auth": {"terminal": true},
        },
        "clientInfo": {"name": "openstitch", "title": "OpenStitch", "version": env!("CARGO_PKG_VERSION")},
    })
}

/// The Claude Code adapter's own tools that reach the computer rather than the design: turned off
/// for the session (`_meta.claudeCode.options.disallowedTools`, an extension of that adapter, not
/// of ACP), so it works through OpenStitch's tools. SolidCraft's list.
pub const CLAUDE_DISALLOWED_TOOLS: &[&str] = &[
    "Bash",
    "BashOutput",
    "KillShell",
    "Read",
    "Write",
    "Edit",
    "MultiEdit",
    "NotebookEdit",
    "Glob",
    "Grep",
    "WebFetch",
    "WebSearch",
    "Task",
];

/// One `mcpServers` entry, the stdio form (McpServerStdio: `name`, `command` (absolute),
/// `args`, `env` as `[{name, value}]`, no `type`; <https://agentclientprotocol.com/protocol/session-setup>
/// and the schema, where `env` is required).
pub fn mcp_server_json(server: &McpServer) -> Value {
    let command = std::path::absolute(&server.command).unwrap_or_else(|_| server.command.clone());
    json!({
        "name": server.name,
        "command": command.to_string_lossy(),
        "args": server.args,
        "env": server.env.iter().map(|(name, value)| json!({"name": name, "value": value})).collect::<Vec<_>>(),
    })
}

/// `session/new` params (<https://agentclientprotocol.com/protocol/session-setup>: `cwd`, an
/// absolute path, and `mcpServers`). `_meta` is ACP's extension point (schema: "reserved by ACP
/// to allow clients and agents to attach additional metadata"); the keys inside are the Claude
/// Code adapter's, as SolidCraft sends them: `systemPrompt.append` and
/// `claudeCode.options.{allowDangerouslySkipPermissions, disallowedTools}`. `init` is the agent's
/// `initialize` result: the top-level `systemPrompt` field goes only to an agent that offers
/// `agentCapabilities.sessionCapabilities.systemPrompt` (SolidCraft's rule; that capability is not
/// on the spec pages checked).
pub fn session_new_params(cwd: &Path, bridge: &McpServer, system: &str, init: &Value) -> Value {
    let cwd = std::path::absolute(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let mut params = json!({
        "cwd": cwd.to_string_lossy(),
        "mcpServers": [mcp_server_json(bridge)],
        "_meta": {
            "systemPrompt": {"append": system},
            "claudeCode": {"options": {
                "allowDangerouslySkipPermissions": false,
                "disallowedTools": CLAUDE_DISALLOWED_TOOLS,
            }},
        },
    });
    let standard = init
        .pointer("/agentCapabilities/sessionCapabilities/systemPrompt")
        .is_some_and(|offer| !offer.is_null());
    if standard && let Some(p) = params.as_object_mut() {
        p.insert(
            "systemPrompt".into(),
            json!({"mode": "append", "content": [{"type": "text", "text": system}]}),
        );
    }
    params
}

/// `session/prompt` params: `sessionId` and `prompt`, ContentBlocks; a text block is
/// `{type: "text", text}` (<https://agentclientprotocol.com/protocol/prompt-turn>).
pub fn prompt_params(session: &str, text: &str) -> Value {
    json!({"sessionId": session, "prompt": [{"type": "text", "text": text}]})
}

/// `session/cancel` params, a notification (same page).
pub fn cancel_params(session: &str) -> Value {
    json!({"sessionId": session})
}

/// The sign-in methods in an `initialize` result's `authMethods`
/// (<https://agentclientprotocol.com/protocol/v1/authentication> and the schema's AuthMethod:
/// `id` and `name` required, `description` optional, discriminator `type`, absent = `agent`).
/// `terminal` becomes [`AuthMethod::Command`]; `agent` and anything else [`AuthMethod::Other`].
pub fn auth_methods(init: &Value) -> Vec<AuthMethod> {
    init.get("authMethods")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .take(32)
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    let name = m
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(&id)
                        .to_string();
                    Some(match m.get("type").and_then(Value::as_str) {
                        Some("terminal") => AuthMethod::Command {
                            id,
                            name,
                            description: m
                                .get("description")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        },
                        _ => AuthMethod::Other { id, name },
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a permission question is about a tool of our bridge, whose calls the panel's gate
/// checks. ACP gives no field naming a tool's MCP server, so the test is SolidCraft's strict one:
/// the whole title (or the tool call's optional `name`, schema ToolCall) is the tool's name as the
/// Claude adapter forms it (`mcp__openstitch-app__batch`), one word with no room for a command
/// line or a path, and the kind is `other` or not given (ToolKind values:
/// <https://agentclientprotocol.com/protocol/tool-calls>).
pub fn is_bridge_tool(
    bridge: &str,
    title: Option<&str>,
    name: Option<&str>,
    kind: Option<&str>,
) -> bool {
    let named = |t: &str| {
        let tool = t
            .strip_prefix("mcp__")
            .and_then(|t| t.strip_prefix(bridge))
            .and_then(|t| t.strip_prefix("__"));
        tool.is_some_and(|t| {
            !t.is_empty()
                && t.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
    };
    (title.is_some_and(named) || name.is_some_and(named)) && matches!(kind, None | Some("other"))
}

/// The longest text kept from one tool call's output.
const MAX_OUTPUT: usize = 2000;

/// The text of a ContentBlock, or of a ToolCallContent item (`{type: "content", content}`,
/// `diff`, `terminal`; <https://agentclientprotocol.com/protocol/tool-calls>).
pub fn content_text(c: &Value) -> Option<String> {
    let block = if c.get("type").and_then(Value::as_str) == Some("content") {
        c.get("content")?
    } else {
        c
    };
    match block.get("type").and_then(Value::as_str) {
        Some("text") => block
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string),
        Some("image") => Some("[image]".into()),
        Some("resource_link") => block
            .get("uri")
            .and_then(Value::as_str)
            .map(|u| format!("[{u}]")),
        Some("diff") => block
            .get("path")
            .and_then(Value::as_str)
            .map(|p| format!("[diff {p}]")),
        Some("terminal") => Some("[terminal]".into()),
        _ => None,
    }
}

/// A tool call's `content` as text, cut to [`MAX_OUTPUT`] bytes.
pub fn tool_output(u: &Value) -> Option<String> {
    let parts: Vec<String> = u
        .get("content")?
        .as_array()?
        .iter()
        .take(16)
        .filter_map(content_text)
        .collect();
    if parts.is_empty() {
        return None;
    }
    let mut s = parts.join("\n");
    if s.len() > MAX_OUTPUT {
        let mut cut = MAX_OUTPUT;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push('…');
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_tools_are_recognised_strictly() {
        let b = "openstitch-app";
        assert!(is_bridge_tool(
            b,
            Some("mcp__openstitch-app__batch"),
            None,
            Some("other")
        ));
        assert!(is_bridge_tool(
            b,
            None,
            Some("mcp__openstitch-app__execute"),
            None
        ));
        assert!(!is_bridge_tool(
            b,
            Some("mcp__openstitch-app__batch"),
            None,
            Some("execute")
        ));
        assert!(!is_bridge_tool(
            b,
            Some("mcp__openstitch-app__"),
            None,
            None
        ));
        assert!(!is_bridge_tool(
            b,
            Some("mcp__openstitch-app__x; rm -rf /"),
            None,
            None
        ));
        assert!(!is_bridge_tool(b, Some("mcp__other__batch"), None, None));
        assert!(!is_bridge_tool(b, Some("Bash"), None, None));
        assert!(!is_bridge_tool(b, None, None, None));
    }

    #[test]
    fn auth_methods_follow_the_type() {
        let init = json!({"authMethods": [
            {"id": "login", "name": "Log in", "description": "Run it", "type": "terminal", "args": ["login"]},
            {"id": "oauth", "name": "Browser"},
            {"name": "no id"},
        ]});
        assert_eq!(
            auth_methods(&init),
            vec![
                AuthMethod::Command {
                    id: "login".into(),
                    name: "Log in".into(),
                    description: Some("Run it".into())
                },
                AuthMethod::Other {
                    id: "oauth".into(),
                    name: "Browser".into()
                },
            ]
        );
    }
}
