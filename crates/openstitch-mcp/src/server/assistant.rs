//! `--assistant`: a handful of meta tools over the running editor, instead of one tool per
//! command. The tool list is fixed and sorted, so its bytes are the same on every request
//! (prompt caching depends on that). The command list comes from the editor's own
//! `commands.list`, which also holds its `ui.*` commands; it is fetched once per process.

use super::{Server, tool_success};
use crate::skills;
use crate::tools::{self, Meta, Tool, ToolKind};
use serde_json::{Map, Value, json};

pub(super) const INSTRUCTIONS: &str = "You are the assistant inside OpenStitch, an embroidery \
editor; the user's open design is what you change. Use these tools only. Read skill_reference \
(SKILL.md first) before building anything. Find commands with commands_list and \
command_describe; prefer run_batch to many execute calls. Check your work with view_render and \
design.info. Lengths are millimetres, angles degrees. Never claim a result you did not check. \
Pictures: an attached one is in the scratch folder; stitch it with digitize.autoPunch \
(compatMode openStitch). Else use generate_image if offered, or write an SVG with scratch_write \
and import.svg, and say why.";

/// Default `view_render` size, in pixels.
const RENDER_SIZE: i64 = 800;

/// The meta tools, sorted by name.
pub(super) fn tools(max_batch_steps: usize, max_render_pixels: u32) -> Vec<Tool> {
    let read = json!({ "readOnlyHint": true, "idempotentHint": true, "openWorldHint": false });
    let write = json!({ "readOnlyHint": false, "destructiveHint": true, "openWorldHint": false });
    let params = json!({ "type": "object",
        "description": "The command's parameters, as command_describe's inputSchema gives them" });
    let command = json!({ "type": "string", "minLength": 1,
        "description": "Command id, such as design.new or ui.state" });
    let mut list = vec![
        (
            "commands_list",
            Meta::CommandsList,
            "Commands",
            "List the editor's commands (engine commands and ui.* editor commands): id, summary \
             and effect (read, edit, history, settings, file). filter keeps those whose id or \
             summary contains it (case-insensitive).".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "filter": { "type": "string", "description": "Text to look for in ids and summaries, such as 'satin' or 'digitize.'" },
                },
                "additionalProperties": false,
            }),
            read.clone(),
        ),
        (
            "command_describe",
            Meta::CommandDescribe,
            "Describe command",
            "One command's full entry: summary, effect, required capabilities, parameters, and the \
             JSON Schema its params must match.".to_string(),
            json!({
                "type": "object",
                "properties": { "command": command.clone() },
                "required": ["command"],
                "additionalProperties": false,
            }),
            read.clone(),
        ),
        (
            "execute",
            Meta::Execute,
            "Execute",
            "Run one command in the editor and return its result. A failure comes back as an \
             error result naming the problem; correct the params and try again.".to_string(),
            json!({
                "type": "object",
                "properties": { "command": command.clone(), "params": params.clone() },
                "required": ["command"],
                "additionalProperties": false,
            }),
            write.clone(),
        ),
        (
            "run_batch",
            Meta::RunBatch,
            "Run batch",
            format!(
                "Run up to {max_batch_steps} commands in order and return every step's result. Stops at \
                 the first failure unless continueOnError is true; steps already done stay done. \
                 Every step is checked (shape and command id) before anything runs, so a malformed \
                 batch changes nothing."
            ),
            json!({
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array", "minItems": 1, "maxItems": max_batch_steps,
                        "items": {
                            "type": "object",
                            "properties": { "command": command, "params": params },
                            "required": ["command"],
                            "additionalProperties": false,
                        },
                    },
                    "continueOnError": { "type": "boolean", "description": "Keep going after a failed step (default false)" },
                },
                "required": ["steps"],
                "additionalProperties": false,
            }),
            write.clone(),
        ),
        (
            "view_render",
            Meta::ViewRender,
            "View render",
            "A PNG of the editor's design as it will sew (Design Page and stitches), plus stitch \
             statistics. Use it to check your work.".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "size": { "type": "integer", "minimum": 64, "maximum": max_render_pixels,
                              "description": format!("Longest image side in pixels (default {RENDER_SIZE})") },
                    "showTravel": { "type": "boolean", "description": "Draw jumps as dashed grey lines (default false)" },
                },
                "additionalProperties": false,
            }),
            read.clone(),
        ),
        (
            "scratch_write",
            Meta::ScratchWrite,
            "Write scratch file",
            "Write an SVG or other text (text), or a PNG/JPEG picture (base64), into this \
             conversation's scratch folder and return its absolute path, ready for import.svg or \
             digitize.*. Give exactly one of text and base64.".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "minLength": 1, "description": "File name with its extension, such as logo.svg" },
                    "text": { "type": "string", "description": "Text content (SVG, CSV, ...)" },
                    "base64": { "type": "string", "description": "Binary content, base64-encoded (PNG, JPEG)" },
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
            write,
        ),
        (
            "skill_reference",
            Meta::SkillReference,
            "Skill reference",
            "OpenStitch's embroidery guide. With no name, lists the files; with a name (SKILL.md, \
             reference/recipes.md, ...), returns that file. Read SKILL.md before building anything.".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "File name from the list, such as SKILL.md" },
                },
                "additionalProperties": false,
            }),
            read,
        ),
    ];
    list.sort_by_key(|t| t.0);
    list.into_iter()
        .map(
            |(name, kind, title, description, schema, annotations)| Tool {
                name: name.into(),
                kind: ToolKind::Meta(kind),
                definition: json!({
                    "name": name,
                    "title": title,
                    "description": description,
                    "inputSchema": schema,
                    "annotations": annotations,
                }),
            },
        )
        .collect()
}

/// Refuses keys a meta tool does not take (its schema has `additionalProperties: false`).
fn only(tool: &str, arguments: &Value, allowed: &[&str]) -> Result<(), String> {
    let Some(map) = arguments.as_object() else {
        return Err(format!("{tool}: arguments must be an object"));
    };
    match map.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(key) => Err(format!(
            "{tool}: unknown argument '{key}' (expected {})",
            allowed.join(", ")
        )),
        None => Ok(()),
    }
}

fn opt_str<'a>(tool: &str, arguments: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(format!("{tool}: '{key}' must be a string")),
    }
}

fn req_str<'a>(tool: &str, arguments: &'a Value, key: &str) -> Result<&'a str, String> {
    opt_str(tool, arguments, key)?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{tool}: needs a string '{key}'"))
}

fn opt_bool(tool: &str, arguments: &Value, key: &str) -> Result<Option<bool>, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(format!("{tool}: '{key}' must be true or false")),
    }
}

/// A step's or call's `params`: an object, `{}` when absent.
fn params_of(tool: &str, arguments: &Value) -> Result<Value, String> {
    match arguments.get("params") {
        None | Some(Value::Null) => Ok(Value::Object(Map::new())),
        Some(v @ Value::Object(_)) => Ok(v.clone()),
        Some(_) => Err(format!("{tool}: 'params' must be an object")),
    }
}

fn text_result(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": false })
}

impl Server {
    pub(super) fn call_meta(&mut self, meta: Meta, arguments: &Value) -> Result<Value, String> {
        match meta {
            Meta::CommandsList => self.commands_list(arguments),
            Meta::CommandDescribe => self.command_describe(arguments),
            Meta::Execute => {
                only("execute", arguments, &["command", "params"])?;
                let command = req_str("execute", arguments, "command")?.to_string();
                let params = params_of("execute", arguments)?;
                self.check_command(&command)?;
                self.run(&command, &params).map(|v| tool_success(&v))
            }
            Meta::RunBatch => self.meta_batch(arguments),
            Meta::ViewRender => self.view_render(arguments),
            Meta::ScratchWrite => {
                only("scratch_write", arguments, &["name", "text", "base64"])?;
                req_str("scratch_write", arguments, "name")?;
                let text = opt_str("scratch_write", arguments, "text")?;
                let data = opt_str("scratch_write", arguments, "base64")?;
                if text.is_some() == data.is_some() {
                    return Err("scratch_write: give exactly one of 'text' and 'base64'".into());
                }
                self.run("ui.scratchWrite", arguments)
                    .map(|v| tool_success(&v))
            }
            Meta::SkillReference => skill_reference(arguments),
        }
    }

    /// The editor's `commands.list`, fetched on first use and kept for the process.
    fn editor_commands(&mut self) -> Result<&[Value], String> {
        if self.editor_commands.is_none() {
            let list = self.run("commands.list", &json!({}))?;
            let Value::Array(entries) = list else {
                return Err("the editor's commands.list is not an array".into());
            };
            self.editor_commands = Some(entries);
        }
        Ok(self.editor_commands.as_deref().unwrap_or_default())
    }

    /// Whether this server's capabilities cover a `commands.list` entry's `requires`.
    fn permits(&self, entry: &Value) -> bool {
        let granted = self.config.capabilities.names();
        entry
            .get("requires")
            .and_then(Value::as_array)
            .is_none_or(|r| {
                r.iter()
                    .filter_map(Value::as_str)
                    .all(|c| granted.contains(&c))
            })
    }

    /// An editor command's entry by id, or why it cannot run.
    fn entry(&mut self, command: &str) -> Result<Value, String> {
        let entries = self.editor_commands()?;
        let found = entries
            .iter()
            .find(|e| e.get("id").and_then(Value::as_str) == Some(command))
            .or_else(|| {
                // A tool name (design_new) for a command id (design.new).
                entries.iter().find(|e| {
                    e.get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| tools::tool_name(id) == command)
                })
            })
            .cloned();
        let Some(entry) = found else {
            return Err(format!(
                "unknown command '{command}'; commands_list shows the commands"
            ));
        };
        if !self.permits(&entry) {
            return Err(format!(
                "command '{command}' is not available: this server allows only {:?}",
                self.config.capabilities.names()
            ));
        }
        Ok(entry)
    }

    /// The command's id when it exists and may run.
    fn check_command(&mut self, command: &str) -> Result<String, String> {
        let entry = self.entry(command)?;
        Ok(entry["id"].as_str().unwrap_or(command).to_string())
    }

    fn commands_list(&mut self, arguments: &Value) -> Result<Value, String> {
        only("commands_list", arguments, &["filter"])?;
        let filter = opt_str("commands_list", arguments, "filter")?
            .unwrap_or("")
            .to_lowercase();
        let entries = self.editor_commands()?.to_vec();
        let commands: Vec<Value> = entries
            .iter()
            .filter(|e| self.permits(e))
            .filter(|e| {
                let field = |k: &str| {
                    e.get(k)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_lowercase()
                };
                filter.is_empty()
                    || field("id").contains(&filter)
                    || field("summary").contains(&filter)
            })
            .map(|e| json!({ "id": e["id"], "summary": e["summary"], "effect": e["effect"] }))
            .collect();
        Ok(tool_success(
            &json!({ "count": commands.len(), "commands": commands }),
        ))
    }

    fn command_describe(&mut self, arguments: &Value) -> Result<Value, String> {
        only("command_describe", arguments, &["command"])?;
        let command = req_str("command_describe", arguments, "command")?;
        let mut entry = self.entry(command)?;
        let params = entry
            .get("params")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        entry["inputSchema"] = tools::input_schema_from_list(&params);
        Ok(tool_success(&entry))
    }

    fn meta_batch(&mut self, arguments: &Value) -> Result<Value, String> {
        only("run_batch", arguments, &["steps", "continueOnError"])?;
        let continue_on_error =
            opt_bool("run_batch", arguments, "continueOnError")?.unwrap_or(false);
        let Some(steps) = arguments.get("steps").and_then(Value::as_array) else {
            return Err("run_batch: needs an array 'steps'".into());
        };
        if steps.is_empty() {
            return Err("run_batch: steps must not be empty".into());
        }
        if steps.len() > self.config.max_batch_steps {
            return Err(format!(
                "run_batch: {} steps exceeds the limit of {}",
                steps.len(),
                self.config.max_batch_steps
            ));
        }
        // Check every step before running any.
        let mut plan: Vec<(String, Value)> = Vec::with_capacity(steps.len());
        for (i, step) in steps.iter().enumerate() {
            let at = format!("run_batch: step {i}");
            only(&at, step, &["command", "params"])?;
            let command = req_str(&at, step, "command")?;
            let params = params_of(&at, step)?;
            let id = self
                .check_command(command)
                .map_err(|e| format!("{at}: {e}"))?;
            plan.push((id, params));
        }
        let mut results = Vec::with_capacity(plan.len());
        let mut failed = 0;
        for (i, (id, params)) in plan.iter().enumerate() {
            match self.run(id, params) {
                Ok(value) => {
                    results.push(json!({ "step": i, "tool": id, "ok": true, "result": value }))
                }
                Err(e) => {
                    failed += 1;
                    results.push(json!({ "step": i, "tool": id, "ok": false, "error": e }));
                    if !continue_on_error {
                        break;
                    }
                }
            }
        }
        let summary = json!({ "steps": plan.len(), "ran": results.len(), "failed": failed, "results": results });
        let mut r = tool_success(&summary);
        if failed > 0 {
            r["isError"] = json!(true);
        }
        Ok(r)
    }

    fn view_render(&mut self, arguments: &Value) -> Result<Value, String> {
        only("view_render", arguments, &["size", "showTravel"])?;
        let max = i64::from(self.config.max_preview_pixels);
        let size = match arguments.get("size") {
            None | Some(Value::Null) => RENDER_SIZE.min(max),
            Some(v) => v
                .as_i64()
                .filter(|n| (64..=max).contains(n))
                .ok_or_else(|| format!("view_render: 'size' must be an integer 64-{max}"))?,
        };
        let show_travel = opt_bool("view_render", arguments, "showTravel")?.unwrap_or(false);
        let reply = self.run(
            "ui.render",
            &json!({ "size": size, "showTravel": show_travel }),
        )?;
        let png = reply
            .get("png")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or("view_render: the editor's ui.render reply has no png")?;
        let stats = reply.get("stats").cloned().unwrap_or(Value::Null);
        Ok(json!({
            "content": [
                { "type": "image", "data": png, "mimeType": "image/png" },
                { "type": "text", "text": stats.to_string() },
            ],
            "isError": false,
        }))
    }
}

fn skill_reference(arguments: &Value) -> Result<Value, String> {
    only("skill_reference", arguments, &["name"])?;
    match opt_str("skill_reference", arguments, "name")?.filter(|s| !s.is_empty()) {
        None => {
            let files: Vec<Value> = skills::FILES
                .iter()
                .map(|(name, text)| json!({ "name": name, "title": skills::title(text) }))
                .collect();
            Ok(tool_success(&json!({ "files": files })))
        }
        Some(name) => match skills::find(name) {
            Some((_, text)) => Ok(text_result(text)),
            None => {
                let names: Vec<&str> = skills::FILES.iter().map(|(n, _)| *n).collect();
                Err(format!(
                    "skill_reference: no file '{name}'; the files are {}",
                    names.join(", ")
                ))
            }
        },
    }
}
