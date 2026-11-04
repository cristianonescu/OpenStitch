//! Model Context Protocol server over the OpenStitch command engine: newline-delimited
//! JSON-RPC 2.0 on stdio. Tools are generated from the engine's command registry, plus
//! `run_batch` and `preview`; file access is confined by the engine's [`Sandbox`](openstitch_engine::Sandbox).
//!
//! With `--live` the commands run in a running editor instead. `--assistant` (which implies
//! `--live`) offers seven meta tools instead of one tool per command, for the editor's AI
//! assistant: `command_describe`, `commands_list`, `execute`, `run_batch`, `scratch_write`,
//! `skill_reference` and `view_render`, in that fixed order. The bundled embroidery skill
//! ([`skills`]) is served by `skill_reference`.

pub mod line_reader;
pub mod server;
pub mod skills;
pub mod tools;

/// The PNG renderer, shared with the thumbnailer (`openstitch-render`).
pub use openstitch_render::preview;

pub use line_reader::{Line, LineReader};
pub use server::{Config, PROTOCOL_VERSIONS, Server};
pub use tools::{Meta, Tool, ToolKind, tool_name};
