//! Model Context Protocol server over the OpenStitch command engine: newline-delimited
//! JSON-RPC 2.0 on stdio. Tools are generated from the engine's command registry, plus
//! `run_batch` and `preview`; file access is confined by the engine's [`Sandbox`](openstitch_engine::Sandbox).
//!
//! With `--live` the commands run in a running editor instead. `--assistant` (which implies
//! `--live`) offers nine meta tools instead of one tool per command, for the editor's AI
//! assistant: `command_describe`, `commands_list`, `execute`, `notes_read`, `notes_write`,
//! `run_batch`, `scratch_write`, `skill_reference` and `view_render`, in that fixed order. The bundled embroidery skill
//! ([`skills`]) is served by `skill_reference`.

pub mod line_reader;
pub mod server;
pub mod skills;
pub mod tools;

/// The PNG renderer, shared with the thumbnailer (`openstitch-render`).
pub use openstitch_render::preview;

pub use line_reader::{Line, LineReader};
pub use server::{Config, EditorLink, PROTOCOL_VERSIONS, Server};
pub use tools::{Meta, Tool, ToolKind, tool_name};

/// Serves MCP over this process's stdin and stdout until the client goes away (the stdio
/// transport). Used by the `openstitch-mcp` binary and by the editor's own
/// `openstitch --assistant-mcp`, which a coding agent starts as its OpenStitch tool server.
pub fn serve_stdio(server: &mut Server) {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    let max = server.config().max_request_bytes;
    for line in LineReader::new(std::io::stdin().lock(), max) {
        let reply = match line {
            Line::Text(text) => server.handle_line(&text),
            Line::Oversized => Some(server.oversized_request_reply()),
        };
        if let Some(reply) = reply
            && writeln!(stdout, "{reply}")
                .and_then(|()| stdout.flush())
                .is_err()
        {
            break; // client went away
        }
    }
}
