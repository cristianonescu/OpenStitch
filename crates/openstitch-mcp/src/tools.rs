//! The tool list, built from the engine's command registry, so a command added there becomes a
//! tool with no MCP-specific code.

use openstitch_engine::{CommandSpec, Effect, ParamDoc};
use serde_json::{Map, Value, json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolKind {
    /// An engine command, by id.
    Command(&'static str),
    Batch,
    Preview,
    /// An `--assistant` meta tool.
    Meta(Meta),
}

/// The `--assistant` meta tools (see [`crate::assistant`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Meta {
    CommandsList,
    CommandDescribe,
    Execute,
    RunBatch,
    ViewRender,
    ScratchWrite,
    SkillReference,
    NotesRead,
    NotesWrite,
}

#[derive(Clone, Debug)]
pub struct Tool {
    pub name: String,
    pub kind: ToolKind,
    /// The `tools/list` entry.
    pub definition: Value,
}

/// MCP clients (and the Claude API) accept `[A-Za-z0-9_-]{1,64}`; command ids use dots.
pub fn tool_name(command_id: &str) -> String {
    command_id.replace('.', "_")
}

pub fn tools(
    commands: &[&CommandSpec],
    max_batch_steps: usize,
    max_preview_pixels: u32,
) -> Vec<Tool> {
    let mut specs = commands.to_vec();
    specs.sort_by_key(|c| c.id);
    let mut tools: Vec<Tool> = specs
        .iter()
        .map(|spec| {
            let name = tool_name(spec.id);
            let definition = json!({
                "name": name,
                "title": spec.id,
                "description": format!(
                    "{}. Engine command '{}' ({}). Lengths are millimetres, angles degrees.",
                    spec.summary, spec.id, spec.effect.as_str()
                ),
                "inputSchema": input_schema(&spec.params),
                "annotations": {
                    "readOnlyHint": spec.effect == Effect::Read,
                    // Writes files, which may overwrite existing ones. Edits are undoable.
                    "destructiveHint": spec.effect == Effect::File,
                    "idempotentHint": spec.effect == Effect::Read,
                    "openWorldHint": false,
                },
            });
            Tool {
                name,
                kind: ToolKind::Command(spec.id),
                definition,
            }
        })
        .collect();
    tools.push(Tool {
        name: "run_batch".into(),
        kind: ToolKind::Batch,
        definition: json!({
            "name": "run_batch",
            "title": "Run batch",
            "description": format!(
                "Run up to {max_batch_steps} engine commands in order and return every step's result. Stops at the first \
                 failure unless continueOnError is true; steps already done stay done (each edit is its own undo step). \
                 Every step's parameters are checked before anything runs: a batch with mistakes runs nothing and \
                 returns every one at once as errors [{{step, command, param, message}}]; fix them all before resending. \
                 validateOnly checks without running."
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array", "minItems": 1, "maxItems": max_batch_steps,
                        "items": {
                            "type": "object",
                            "properties": {
                                "tool": { "type": "string", "description": "Tool name (design_new) or command id (design.new)" },
                                "arguments": { "type": "object", "description": "That tool's arguments" },
                            },
                            "required": ["tool"],
                            "additionalProperties": false,
                        },
                    },
                    "continueOnError": { "type": "boolean", "description": "Keep going after a failed step (default false)" },
                    "validateOnly": { "type": "boolean", "description": "Only check every step's parameters; run nothing (default false)" },
                },
                "required": ["steps"],
                "additionalProperties": false,
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "openWorldHint": false },
        }),
    });
    tools.push(Tool {
        name: "preview".into(),
        kind: ToolKind::Preview,
        definition: json!({
            "name": "preview",
            "title": "Preview",
            "description": "Generate the design's stitches and return a PNG of them on the Design Page, plus stitch statistics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "size": { "type": "integer", "minimum": 64, "maximum": max_preview_pixels,
                              "description": "Longest image side in pixels (default 512)" },
                    "showTravel": { "type": "boolean", "description": "Draw jumps as dashed grey lines (default false)" },
                    "layers": { "type": "string", "enum": ["both", "underlay", "top"], "description": "Underlay only or top stitching only (default both)" },
                    "colorBy": { "type": "string", "enum": ["thread", "order", "mono"], "description": "Colour by thread (default), by sewing order (a gradient from first to last) or one colour" },
                    "only": { "type": "array", "items": { "type": "integer" }, "description": "Only these objects' stitches" },
                    "solo": { "type": "integer", "minimum": 0, "description": "Fade every colour block but this one (0-based, sewing order)" },
                    "needlePoints": { "type": "boolean", "description": "A dot at every needle drop" },
                    "trimMarks": { "type": "boolean", "description": "A red cut mark at every trim" },
                },
                "additionalProperties": false,
            },
            "annotations": { "readOnlyHint": true, "idempotentHint": true, "openWorldHint": false },
        }),
    });
    tools
}

/// JSON Schema for a command's parameters, from the `ParamDoc` kind strings. No top-level
/// `anyOf` (the Claude API rejects it); the engine still validates every value.
pub fn input_schema(params: &[ParamDoc]) -> Value {
    schema_from(
        params
            .iter()
            .map(|d| (d.name, d.kind, d.required, d.summary)),
    )
}

/// [`input_schema`] for the JSON parameter entries of `commands.list`
/// (`{name, type, required, summary}`), as a live editor reports them.
pub fn input_schema_from_list(params: &[Value]) -> Value {
    let field = |p: &Value, key: &str| p.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
    let owned: Vec<(String, String, bool, String)> = params
        .iter()
        .map(|p| {
            (
                field(p, "name"),
                field(p, "type"),
                p.get("required").and_then(Value::as_bool).unwrap_or(false),
                field(p, "summary"),
            )
        })
        .filter(|(name, ..)| !name.is_empty())
        .collect();
    schema_from(
        owned
            .iter()
            .map(|(n, k, r, s)| (n.as_str(), k.as_str(), *r, s.as_str())),
    )
}

/// `(name, kind, required, summary)` per parameter.
fn schema_from<'a>(params: impl Iterator<Item = (&'a str, &'a str, bool, &'a str)>) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for (name, kind, is_required, summary) in params {
        if name == "ids" && kind.contains("id: int") {
            // `ids` or a single `id`: neither alone is required.
            properties.insert(
                "ids".into(),
                json!({ "type": "array", "items": { "type": "integer" }, "minItems": 1,
                        "description": format!("{summary} (give ids or id)") }),
            );
            properties.insert(
                "id".into(),
                json!({ "type": "integer", "description": "Single object id, instead of ids" }),
            );
            continue;
        }
        properties.insert(name.into(), kind_schema(kind, summary));
        if is_required {
            required.push(Value::from(name));
        }
    }
    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), Value::Object(properties));
    schema.insert("additionalProperties".into(), json!(false));
    if !required.is_empty() {
        schema.insert("required".into(), Value::Array(required));
    }
    Value::Object(schema)
}

fn point_pair_schema() -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 })
}

pub fn property_schema(doc: &ParamDoc) -> Value {
    kind_schema(doc.kind, doc.summary)
}

/// One property's schema from its kind string. An unrecognised kind, such as `any`, gives an
/// untyped property (a description only), which accepts any JSON value.
fn kind_schema(t: &str, summary: &str) -> Value {
    let mut s = Map::new();
    let mut unit: Option<&str> = None;
    match t {
        "string" => {
            s.insert("type".into(), json!("string"));
        }
        "bool" => {
            s.insert("type".into(), json!("boolean"));
        }
        "int" => {
            s.insert("type".into(), json!("integer"));
        }
        "[int]" => {
            s.insert("type".into(), json!("array"));
            s.insert("items".into(), json!({ "type": "integer", "minimum": 0 }));
        }
        "color" => {
            s.insert("type".into(), json!(["string", "integer", "object"]));
            unit = Some(
                "#rrggbb, a Brother thread name, chart number 1-64, or a chart's thread {builtin: \"newBrothread\", code: \"001\"}",
            );
        }
        "mm" | "degrees" | "factor" | "percent" => {
            s.insert("type".into(), json!("number"));
            unit = Some(t);
        }
        _ if t.ends_with("/mm") => {
            s.insert("type".into(), json!("number"));
            unit = Some(t);
        }
        _ if t.starts_with("int ") => {
            s.insert("type".into(), json!("integer"));
            let bounds: Vec<i64> = t[4..].split('-').filter_map(|b| b.parse().ok()).collect();
            if let [lo, hi] = bounds[..] {
                s.insert("minimum".into(), json!(lo));
                s.insert("maximum".into(), json!(hi));
            }
        }
        _ if t.starts_with("[[c1x, c1y], [c2x, c2y]]") => {
            s.insert("type".into(), json!("array"));
            s.insert("items".into(), point_pair_schema());
            s.insert("minItems".into(), json!(2));
            s.insert("maxItems".into(), json!(2));
            unit = Some(t);
        }
        _ if t.starts_with("[null | [[c1x, c1y], [c2x, c2y]]]") => {
            s.insert("type".into(), json!("array"));
            s.insert(
                "items".into(),
                json!({ "anyOf": [ { "type": "null" }, {
                    "type": "array", "items": point_pair_schema(), "minItems": 2, "maxItems": 2
                } ] }),
            );
            unit = Some(t);
        }
        "[mm]" => {
            s.insert("type".into(), json!("array"));
            s.insert("items".into(), json!({ "type": "number" }));
            unit = Some(t);
        }
        _ if t.starts_with("[x, y]") => {
            s.insert("type".into(), json!("array"));
            s.insert("items".into(), json!({ "type": "number" }));
            s.insert("minItems".into(), json!(2));
            s.insert("maxItems".into(), json!(2));
            unit = Some(t);
        }
        _ if t.starts_with("[[x, y]]") => {
            s.insert("type".into(), json!("array"));
            s.insert("items".into(), json!({ "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 }));
            unit = Some(t);
        }
        _ if t.contains('|') && !t.contains(' ') && !t.contains("...") => {
            s.insert("type".into(), json!("string"));
            s.insert("enum".into(), t.split('|').collect::<Vec<_>>().into());
        }
        "any" | "" => {}
        _ => unit = Some(t),
    }
    let description = match unit {
        Some(u) => format!("{summary} ({u})"),
        None => summary.to_string(),
    };
    s.insert("description".into(), json!(description));
    Value::Object(s)
}
