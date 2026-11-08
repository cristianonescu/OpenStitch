//! `--assistant`: nine meta tools over a live editor. The editor here is a fake host: a real
//! engine `Session` behind the loopback transport, plus canned replies for the editor's `ui.*`
//! commands (which the real editor implements in `openstitch-app`).

use openstitch_engine::Session;
use openstitch_mcp::{Config, Server};
use openstitch_remote::{Client, ServerConfig};
use serde_json::{Value, json};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const PNG_BASE64: &str = "iVBORw0KGgo=";

/// The editor's own commands, as its `commands.list` adds them to the engine's.
fn ui_entries() -> Vec<Value> {
    let entry = |id: &str, summary: &str, effect: &str, params: Value| json!({ "id": id, "summary": summary, "effect": effect, "requires": [], "params": params });
    vec![
        entry(
            "ui.render",
            "Render the design as a PNG",
            "read",
            json!([
                { "name": "size", "type": "int 64-2048", "required": false, "summary": "Longest side" },
                { "name": "showTravel", "type": "bool", "required": false, "summary": "Draw jumps" },
            ]),
        ),
        entry(
            "ui.scratchWrite",
            "Write a scratch file",
            "read",
            json!([
                { "name": "name", "type": "string", "required": true, "summary": "File name" },
                { "name": "text", "type": "string", "required": false, "summary": "Text" },
                { "name": "base64", "type": "string", "required": false, "summary": "Bytes" },
            ]),
        ),
        entry("ui.notesRead", "The Assistant's notes", "read", json!([])),
        entry(
            "ui.notesWrite",
            "Remember something",
            "settings",
            json!([
                { "name": "section", "type": "string", "required": true, "summary": "Section" },
                { "name": "text", "type": "string", "required": true, "summary": "Text" },
                { "name": "mode", "type": "append|replace", "required": false, "summary": "Mode" },
            ]),
        ),
        entry(
            "ui.set",
            "Change a view setting",
            "settings",
            json!([
                { "name": "key", "type": "string", "required": true, "summary": "Setting" },
                { "name": "value", "type": "any", "required": true, "summary": "New value" },
            ]),
        ),
        entry(
            "ui.state",
            "The editor's state and selection",
            "read",
            json!([]),
        ),
    ]
}

struct Editor {
    stop: mpsc::Sender<()>,
    host: JoinHandle<Session>,
    /// Every command the editor received, in order.
    log: Arc<Mutex<Vec<String>>>,
}

impl Editor {
    fn start() -> (Editor, Client) {
        let editor = openstitch_remote::Server::start(ServerConfig::default()).expect("listen");
        let client = Client::new(editor.endpoint().clone()).expect("client");
        let (stop, stopped) = mpsc::channel::<()>();
        let log = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&log);
        let host = std::thread::spawn(move || {
            let mut session = Session::new(".");
            let notes = Mutex::new(String::new());
            while stopped.try_recv().is_err() {
                let Some(job) = editor.wait_next(Duration::from_millis(20)) else {
                    continue;
                };
                let request = job.request().clone();
                let command = request["command"].as_str().unwrap_or("").to_string();
                seen.lock().expect("log").push(command.clone());
                let params = &request["params"];
                let canned = match command.as_str() {
                    "ui.render" => Some(json!({
                        "png": PNG_BASE64, "width": params["size"], "height": params["size"],
                        "stats": { "stitches": 1234, "showTravel": params["showTravel"], "layers": params["layers"] },
                    })),
                    "ui.scratchWrite" => Some(json!({
                        "path": format!("/scratch/{}", params["name"].as_str().unwrap_or("")),
                        "bytes": params["text"].as_str().map_or(0, str::len),
                    })),
                    "ui.state" => Some(json!({ "selection": [] })),
                    "ui.notesRead" => Some(json!({
                        "text": notes.lock().expect("notes").clone(), "bytes": 0, "limit": 8192,
                    })),
                    "ui.notesWrite" if params["section"] == json!("Diary") => None,
                    "ui.notesWrite" => {
                        let mut n = notes.lock().expect("notes");
                        n.push_str(&format!(
                            "## {}\n- {} ({})\n",
                            params["section"].as_str().unwrap_or(""),
                            params["text"].as_str().unwrap_or(""),
                            params["mode"].as_str().unwrap_or(""),
                        ));
                        Some(json!({ "text": n.clone(), "bytes": n.len(), "limit": 8192 }))
                    }
                    _ => None,
                };
                let mut reply = match canned {
                    Some(result) => json!({ "ok": true, "result": result }),
                    None => editor.policy().handle(&mut session, &request),
                };
                if command == "commands.list"
                    && let Some(list) = reply["result"].as_array_mut()
                {
                    list.extend(ui_entries());
                }
                job.respond(reply);
            }
            session
        });
        (Editor { stop, host, log }, client)
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().expect("log").clone()
    }

    fn finish(self) -> Session {
        self.stop.send(()).expect("stop");
        self.host.join().expect("join")
    }
}

struct Mcp {
    server: Server,
    next: i64,
}

impl Mcp {
    fn new(client: Client) -> Mcp {
        let mut mcp = Mcp {
            server: Server::new(Config::default())
                .expect("server")
                .with_assistant(client),
            next: 0,
        };
        let init = mcp.request("initialize", json!({"protocolVersion": "2025-11-25"}));
        assert!(init["result"]["instructions"].is_string(), "{init}");
        mcp
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let m = json!({"jsonrpc": "2.0", "id": self.next, "method": method, "params": params});
        let line = self.server.handle_line(&m.to_string()).expect("reply");
        serde_json::from_str(&line).expect("json")
    }

    fn list_line(&mut self) -> String {
        self.next += 1;
        let m = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
        self.server.handle_line(&m.to_string()).expect("reply")
    }

    /// The `CallToolResult`.
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let reply = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        reply["result"].clone()
    }
}

fn text(result: &Value) -> Value {
    let t = result["content"][0]["text"].as_str().expect("text");
    serde_json::from_str(t).unwrap_or_else(|_| Value::String(t.to_string()))
}

/// Whether a JSON schema has a property called `name` at any depth (#425).
fn has_name_property(schema: &Value) -> bool {
    match schema {
        Value::Object(map) => {
            map.get("properties")
                .and_then(Value::as_object)
                .is_some_and(|p| p.contains_key("name"))
                || map.values().any(has_name_property)
        }
        Value::Array(list) => list.iter().any(has_name_property),
        _ => false,
    }
}

#[test]
fn assistant_lists_exactly_the_nine_meta_tools_sorted_and_stable() {
    let (editor, client) = Editor::start();
    let mut mcp = Mcp::new(client);
    assert!(mcp.server.is_assistant() && mcp.server.is_live());
    let first = mcp.list_line();
    let second = mcp.list_line();
    assert_eq!(first, second, "tools/list must be byte-identical");
    let list: Value = serde_json::from_str(&first).expect("json");
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        [
            "command_describe",
            "commands_list",
            "execute",
            "notes_read",
            "notes_write",
            "run_batch",
            "scratch_write",
            "skill_reference",
            "view_render"
        ]
    );
    for tool in list["result"]["tools"].as_array().expect("tools") {
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], json!("object"), "{tool}");
        assert_eq!(schema["additionalProperties"], json!(false), "{tool}");
        assert!(schema.get("anyOf").is_none(), "{tool}");
        assert!(
            !has_name_property(schema),
            "#425: Ollama's generic parser drops calls whose arguments have a 'name': {tool}"
        );
    }
    // Listing needs no editor round trip.
    assert!(editor.log().is_empty(), "{:?}", editor.log());
    editor.finish();
}

#[test]
fn assistant_instructions_are_short_and_point_at_the_skill() {
    let (editor, client) = Editor::start();
    let mut server = Server::new(Config::default())
        .expect("server")
        .with_assistant(client);
    let init = server
        .handle_message(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}))
        .expect("reply");
    let instructions = init["result"]["instructions"].as_str().expect("text");
    assert!(instructions.contains("skill_reference"), "{instructions}");
    assert!(instructions.contains("view_render"), "{instructions}");
    assert!(instructions.contains("run_batch"), "{instructions}");
    assert!(instructions.len() < 700, "{}", instructions.len());
    editor.finish();
}

#[test]
fn commands_list_filters_the_editors_list_and_is_fetched_once() {
    let (editor, client) = Editor::start();
    let mut mcp = Mcp::new(client);
    let all = text(&mcp.call("commands_list", json!({})));
    let ids: Vec<&str> = all["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .map(|c| c["id"].as_str().expect("id"))
        .collect();
    assert!(
        ids.contains(&"design.new") && ids.contains(&"ui.render"),
        "{ids:?}"
    );
    assert_eq!(all["count"], json!(ids.len()));
    let entry = &all["commands"][0];
    assert_eq!(
        entry.as_object().expect("entry").len(),
        3,
        "id, summary, effect: {entry}"
    );

    let ui = text(&mcp.call("commands_list", json!({"filter": "UI."})));
    let ui_ids: Vec<&str> = ui["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .map(|c| c["id"].as_str().expect("id"))
        .collect();
    assert_eq!(
        ui_ids,
        [
            "ui.render",
            "ui.scratchWrite",
            "ui.notesRead",
            "ui.notesWrite",
            "ui.set",
            "ui.state"
        ]
    );
    // Matches summaries too.
    let lettering = text(&mcp.call("commands_list", json!({"filter": "lettering"})));
    assert!(
        lettering["commands"]
            .as_array()
            .expect("commands")
            .iter()
            .any(|c| c["id"] == json!("text.add")),
        "{lettering}"
    );
    let bad = mcp.call("commands_list", json!({"filter": 3}));
    assert_eq!(bad["isError"], json!(true));
    let extra = mcp.call("commands_list", json!({"prefix": "x"}));
    assert_eq!(extra["isError"], json!(true));

    let fetched = editor
        .log()
        .iter()
        .filter(|c| *c == "commands.list")
        .count();
    assert_eq!(fetched, 1, "commands.list is cached per process");
    editor.finish();
}

#[test]
fn command_describe_gives_the_entry_and_its_schema() {
    let (editor, client) = Editor::start();
    let mut mcp = Mcp::new(client);
    let rect = mcp.call("command_describe", json!({"command": "shape.rectangle"}));
    assert_eq!(rect["isError"], json!(false), "{rect}");
    let rect = text(&rect);
    assert_eq!(rect["id"], json!("shape.rectangle"));
    assert_eq!(rect["effect"], json!("edit"));
    let schema = &rect["inputSchema"];
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(schema["properties"]["width"]["type"], json!("number"));
    assert_eq!(schema["required"], json!(["x", "y", "width", "height"]));

    // The schema is the one the per-command tools use.
    let session = Session::new(".");
    let spec = session
        .commands()
        .into_iter()
        .find(|c| c.id == "object.setRegion")
        .expect("spec");
    let region = text(&mcp.call("command_describe", json!({"command": "object.setRegion"})));
    assert_eq!(
        region["inputSchema"],
        openstitch_mcp::tools::input_schema(&spec.params)
    );

    // An editor command whose value is untyped (`any`): a description only.
    let set = text(&mcp.call("command_describe", json!({"command": "ui.set"})));
    assert_eq!(
        set["inputSchema"]["properties"]["value"],
        json!({"description": "New value"})
    );
    assert_eq!(set["inputSchema"]["required"], json!(["key", "value"]));

    let unknown = mcp.call("command_describe", json!({"command": "shape.hexagon"}));
    assert_eq!(unknown["isError"], json!(true));
    let missing = mcp.call("command_describe", json!({}));
    assert_eq!(missing["isError"], json!(true));
    editor.finish();
}

#[test]
fn execute_forwards_to_the_editor_and_reports_errors() {
    let (editor, client) = Editor::start();
    let mut mcp = Mcp::new(client);
    let added = mcp.call(
        "execute",
        json!({"command": "shape.rectangle", "params": {"x": 0, "y": 0, "width": 10, "height": 10}}),
    );
    assert_eq!(added["isError"], json!(false), "{added}");
    assert_eq!(text(&added)["id"], json!(1));
    let state = mcp.call("execute", json!({"command": "ui.state"}));
    assert_eq!(text(&state), json!({"selection": []}));
    let failed = mcp.call(
        "execute",
        json!({"command": "object.get", "params": {"id": 999}}),
    );
    assert_eq!(failed["isError"], json!(true), "{failed}");
    let unknown = mcp.call("execute", json!({"command": "shape.hexagon"}));
    assert_eq!(unknown["isError"], json!(true));
    assert!(
        text(&unknown)
            .as_str()
            .expect("message")
            .contains("commands_list")
    );
    let bad_params = mcp.call("execute", json!({"command": "design.info", "params": [1]}));
    assert_eq!(bad_params["isError"], json!(true));
    // A per-command tool name is not a tool in this mode.
    let direct = mcp.request(
        "tools/call",
        json!({"name": "shape_rectangle", "arguments": {}}),
    );
    assert!(
        direct["error"]["message"]
            .as_str()
            .expect("message")
            .contains("execute")
    );

    let log = editor.log();
    assert!(!log.contains(&"shape.hexagon".to_string()), "{log:?}");
    let session = editor.finish();
    assert_eq!(session.design().objects.len(), 1);
}

#[test]
fn run_batch_forwards_every_step_and_stops_at_a_failure() {
    let (editor, client) = Editor::start();
    let mut mcp = Mcp::new(client);
    let batch = mcp.call(
        "run_batch",
        json!({"steps": [
            {"command": "design.new", "params": {"name": "Badge"}},
            {"command": "shape.ellipse", "params": {"cx": 50, "cy": 50, "rx": 30}},
            {"command": "object.setRegion", "params": {"id": 1, "type": "satin"}},
        ]}),
    );
    assert_eq!(batch["isError"], json!(false), "{batch}");
    let summary = text(&batch);
    assert_eq!(summary["ran"], json!(3));
    assert_eq!(summary["results"][1]["result"]["id"], json!(1));
    assert_eq!(summary["results"][2]["tool"], json!("object.setRegion"));

    let failing = mcp.call(
        "run_batch",
        json!({"steps": [
            {"command": "object.get", "params": {"id": 99}},
            {"command": "shape.rectangle", "params": {"x": 0, "y": 0, "width": 5, "height": 5}},
        ]}),
    );
    assert_eq!(failing["isError"], json!(true));
    let summary = text(&failing);
    assert_eq!(
        (summary["ran"].clone(), summary["failed"].clone()),
        (json!(1), json!(1))
    );

    let onward = mcp.call(
        "run_batch",
        json!({"continueOnError": true, "steps": [
            {"command": "object.get", "params": {"id": 99}},
            {"command": "shape.rectangle", "params": {"x": 0, "y": 0, "width": 5, "height": 5}},
        ]}),
    );
    assert_eq!(text(&onward)["ran"], json!(2));
    let session = editor.finish();
    assert_eq!(session.design().objects.len(), 2);
}

/// #494: every parameter mistake in the batch comes back at once, with its step, and nothing
/// reaches the editor; `validateOnly` checks a good batch without running it.
#[test]
fn run_batch_reports_every_parameter_error_before_running() {
    let (editor, client) = Editor::start();
    let mut mcp = Mcp::new(client);
    mcp.call("commands_list", json!({}));
    let before = editor.log().len();
    let bad = mcp.call(
        "run_batch",
        json!({"steps": [
            {"command": "design.new", "params": {"name": "Badge"}},
            {"command": "shape.rectangle", "params": {"x": 0, "y": 0, "width": 5, "height": 5, "colour": "red"}},
            {"command": "object.setRegion", "params": {"id": 1, "type": "satin", "angle": "steep"}},
            {"command": "ui.render", "params": {"size": 9}},
        ]}),
    );
    assert_eq!(bad["isError"], json!(true), "{bad}");
    let summary = text(&bad);
    assert_eq!(
        (summary["ok"].clone(), summary["ran"].clone()),
        (json!(false), json!(0))
    );
    let errors = summary["errors"].as_array().expect("errors");
    let found: Vec<(u64, &str)> = errors
        .iter()
        .map(|e| {
            (
                e["step"].as_u64().unwrap_or(99),
                e["param"].as_str().unwrap_or(""),
            )
        })
        .collect();
    assert_eq!(
        found,
        [(1, "colour"), (2, "angle"), (3, "size")],
        "{summary}"
    );
    assert_eq!(errors[1]["command"], json!("object.setRegion"));
    assert_eq!(
        editor.log().len(),
        before,
        "nothing ran: {:?}",
        editor.log()
    );

    let check = mcp.call(
        "run_batch",
        json!({"validateOnly": true, "steps": [
            {"command": "design.new", "params": {"name": "Badge"}},
            {"command": "shape.ellipse", "params": {"cx": 50, "cy": 50, "rx": 30}},
            {"command": "object.setRegion", "params": {"id": 1, "type": "satin"}},
        ]}),
    );
    assert_eq!(check["isError"], json!(false), "{check}");
    assert_eq!(text(&check)["ok"], json!(true));
    assert_eq!(text(&check)["ran"], json!(0));
    assert_eq!(editor.log().len(), before);
    let session = editor.finish();
    assert!(session.design().objects.is_empty());
}

#[test]
fn run_batch_with_an_unknown_command_or_bad_step_runs_nothing() {
    let (editor, client) = Editor::start();
    let mut mcp = Mcp::new(client);
    // Fetch the list first, so the log below holds only what the batches send.
    mcp.call("commands_list", json!({"filter": "design."}));
    let before = editor.log().len();
    let rect =
        json!({"command": "shape.rectangle", "params": {"x": 0, "y": 0, "width": 5, "height": 5}});
    for steps in [
        json!([rect, {"command": "shape.hexagon"}]),
        json!([rect, {"command": "design.info", "params": "x"}]),
        json!([rect, {"command": "design.info", "arguments": {}}]),
        json!([rect, {"params": {}}]),
        json!([]),
    ] {
        let r = mcp.call("run_batch", json!({ "steps": steps }));
        assert_eq!(r["isError"], json!(true), "{steps}: {r}");
    }
    let too_many: Vec<Value> = (0..257).map(|_| rect.clone()).collect();
    let r = mcp.call("run_batch", json!({ "steps": too_many }));
    assert_eq!(r["isError"], json!(true));
    assert_eq!(editor.log().len(), before, "{:?}", editor.log());
    let session = editor.finish();
    assert!(session.design().objects.is_empty());
}

#[test]
fn view_render_returns_an_image_block_and_the_stats() {
    let (editor, client) = Editor::start();
    let mut mcp = Mcp::new(client);
    let render = mcp.call("view_render", json!({"size": 256, "showTravel": true}));
    assert_eq!(render["isError"], json!(false), "{render}");
    let content = render["content"].as_array().expect("content");
    assert_eq!(content[0]["type"], json!("image"));
    assert_eq!(content[0]["mimeType"], json!("image/png"));
    assert_eq!(content[0]["data"], json!(PNG_BASE64));
    assert_eq!(content[1]["type"], json!("text"));
    let stats: Value =
        serde_json::from_str(content[1]["text"].as_str().expect("text")).expect("json");
    assert_eq!(
        stats,
        json!({"stitches": 1234, "showTravel": true, "layers": null})
    );

    // Default size, and range checks before anything is sent.
    let default = mcp.call("view_render", json!({}));
    assert_eq!(default["isError"], json!(false));
    for bad in [
        json!({"size": 10}),
        json!({"size": 4096}),
        json!({"size": "big"}),
    ] {
        assert_eq!(
            mcp.call("view_render", bad.clone())["isError"],
            json!(true),
            "{bad}"
        );
    }
    // View ▸ Show overrides (#517) reach the editor; unknown keys are refused.
    let under = mcp.call(
        "view_render",
        json!({"layers": "underlay", "needlePoints": true}),
    );
    assert!(
        under["content"][1]["text"]
            .as_str()
            .is_some_and(|t| t.contains("\"layers\":\"underlay\"")),
        "{under}"
    );
    assert_eq!(
        mcp.call("view_render", json!({"sparkle": true}))["isError"],
        json!(true)
    );
    let renders: Vec<String> = editor
        .log()
        .into_iter()
        .filter(|c| c == "ui.render")
        .collect();
    assert_eq!(renders.len(), 3);
    editor.finish();
}

#[test]
fn scratch_write_forwards_and_needs_exactly_one_content() {
    let (editor, client) = Editor::start();
    let mut mcp = Mcp::new(client);
    let written = mcp.call(
        "scratch_write",
        json!({"file": "logo.svg", "text": "<svg xmlns=\"http://www.w3.org/2000/svg\"/>"}),
    );
    assert_eq!(written["isError"], json!(false), "{written}");
    assert_eq!(text(&written)["path"], json!("/scratch/logo.svg"));
    // The old argument name still works (#425 renamed it to `file`).
    let old = mcp.call(
        "scratch_write",
        json!({"name": "old.svg", "text": "<svg/>"}),
    );
    assert_eq!(text(&old)["path"], json!("/scratch/old.svg"), "{old}");
    for bad in [
        json!({"file": "a.svg"}),
        json!({"file": "a.svg", "text": "x", "base64": "eA=="}),
        json!({"text": "x"}),
        json!({"file": "a.svg", "text": "x", "path": "/etc"}),
        json!({"file": "a.svg", "name": "b.svg", "text": "x"}),
    ] {
        assert_eq!(
            mcp.call("scratch_write", bad.clone())["isError"],
            json!(true),
            "{bad}"
        );
    }
    let writes = editor
        .log()
        .iter()
        .filter(|c| *c == "ui.scratchWrite")
        .count();
    assert_eq!(writes, 2);
    editor.finish();
}

/// #491: the notes tools forward to the editor's `ui.notesRead` / `ui.notesWrite` (where the
/// permission gate and the checks are) and refuse malformed calls before reaching it.
#[test]
fn notes_tools_forward_to_the_editor() {
    let (editor, client) = Editor::start();
    let mut mcp = Mcp::new(client);
    let empty = mcp.call("notes_read", json!({}));
    assert_eq!(empty["isError"], json!(false), "{empty}");
    assert_eq!(empty["content"][0]["text"], json!("(no notes yet)"));
    let w = mcp.call(
        "notes_write",
        json!({"section": "Threads", "text": "Madeira Polyneon"}),
    );
    assert_eq!(w["isError"], json!(false), "{w}");
    assert!(
        w["content"][0]["text"]
            .as_str()
            .expect("text")
            .starts_with("Remembered."),
        "{w}"
    );
    let r = mcp.call("notes_read", json!({}));
    assert_eq!(
        r["content"][0]["text"],
        json!("## Threads\n- Madeira Polyneon (append)\n")
    );
    for bad in [
        json!({"text": "x"}),
        json!({"section": "Threads"}),
        json!({"section": "Threads", "text": "x", "mode": "prepend"}),
        json!({"section": "Threads", "text": "x", "name": "y"}),
    ] {
        assert_eq!(
            mcp.call("notes_write", bad.clone())["isError"],
            json!(true),
            "{bad}"
        );
    }
    assert_eq!(
        mcp.call("notes_read", json!({"x": 1}))["isError"],
        json!(true)
    );
    let writes = editor
        .log()
        .iter()
        .filter(|c| *c == "ui.notesWrite")
        .count();
    assert_eq!(writes, 1, "malformed calls never reach the editor");
    editor.finish();
}

#[test]
fn skill_reference_lists_and_returns_the_bundled_files() {
    let (editor, client) = Editor::start();
    let mut mcp = Mcp::new(client);
    let list = text(&mcp.call("skill_reference", json!({})));
    let names: Vec<&str> = list["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| f["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        &[
            "SKILL.md",
            "reference/answers.md",
            "reference/composition.md",
            "reference/examples.md",
            "reference/recipes.md",
            "reference/sewability.md"
        ][..],
        &names[..6]
    );
    // The illustrated examples (#492) follow the guides, each returned by its bare name.
    assert!(names.len() >= 14, "{names:?}");
    let example = mcp.call("skill_reference", json!({"file": "strawberry"}));
    assert_eq!(example["isError"], json!(false), "{example}");
    let steps: Value =
        serde_json::from_str(example["content"][0]["text"].as_str().expect("text")).expect("json");
    assert_eq!(steps[0]["command"], "design.new");
    for file in list["files"].as_array().expect("files") {
        assert!(!file["title"].as_str().expect("title").is_empty(), "{file}");
    }
    let skill = mcp.call("skill_reference", json!({"file": "SKILL.md"}));
    assert_eq!(skill["isError"], json!(false));
    let body = skill["content"][0]["text"].as_str().expect("text");
    assert!(body.contains("# OpenStitch embroidery"), "{body}");
    let recipes = mcp.call("skill_reference", json!({"file": "recipes"}));
    assert!(
        recipes["content"][0]["text"]
            .as_str()
            .expect("text")
            .contains("run_batch")
    );
    let missing = mcp.call("skill_reference", json!({"file": "secrets.md"}));
    assert_eq!(missing["isError"], json!(true));
    // The old argument name still works.
    let old = mcp.call("skill_reference", json!({"name": "SKILL.md"}));
    assert_eq!(old["isError"], json!(false), "{old}");
    // No editor round trip for the skill.
    assert!(editor.log().is_empty());
    editor.finish();
}

#[test]
fn every_command_the_skill_names_exists() {
    let session = Session::new(".");
    let mut ids: Vec<&str> = session.commands().iter().map(|c| c.id).collect();
    let ui = ui_entries();
    let all_ui = [
        "ui.state",
        "ui.actions",
        "ui.run",
        "ui.select",
        "ui.zoom",
        "ui.settings",
        "ui.threadLabels",
        "ui.browserLimits",
        "ui.set",
        "ui.render",
        "ui.simulate",
        "ui.help",
        "ui.update",
        "ui.scratchWrite",
        "ui.notesRead",
        "ui.notesWrite",
        "ui.notesClear",
        "ui.searchCommands",
        "ui.recoveryList",
        "ui.recoveryRestore",
        "ui.recoveryDiscard",
        "ui.layout",
        "ui.resetPrefs",
        "ui.gettingStarted",
        "ui.startupWizard",
        "ui.stitchTable",
        "ui.cursor",
        "ui.preview",
        "ui.documents",
        "ui.loading",
        "ui.guides",
    ];
    // The web editor's own (crates/openstitch-web, #556; SKILL.md "Web editor").
    let web_ui = openstitch_web::session::UI_COMMANDS;
    assert!(
        ui.iter()
            .all(|e| all_ui.contains(&e["id"].as_str().expect("id")))
    );
    ids.extend(all_ui);
    ids.extend(web_ui);
    for (name, body) in openstitch_mcp::skills::FILES {
        // `word.word` in backticks or as a "command" value: a command id.
        for quoted in body.split('`').skip(1).step_by(2).chain(
            body.split("\"command\": \"")
                .skip(1)
                .filter_map(|s| s.split('"').next()),
        ) {
            let candidate = quoted.split_whitespace().next().unwrap_or("");
            let is_id = candidate.contains('.')
                && candidate
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.')
                && candidate.starts_with(|c: char| c.is_ascii_lowercase())
                && !candidate.ends_with(".md")
                && !candidate.contains("..")
                && candidate.split('.').count() == 2
                // File names in the examples, not commands.
                && ![
                    "osd", "pes", "dst", "jef", "exp", "svg", "png", "zip", "gcode", "csv", "txt",
                    "inf", "edr", "col",
                ]
                .contains(&candidate.split('.').nth(1).unwrap_or(""));
            if is_id {
                assert!(
                    ids.contains(&candidate),
                    "{name} names unknown command {candidate}"
                );
            }
        }
    }
}

/// #595, #596: every machine format OpenStitch reads (`formats.list`) is named in the skill's
/// formats table, so the Assistant never tells a user a readable file can't be opened.
#[test]
fn the_skill_names_every_file_format() {
    let mut session = Session::new(".");
    let list = session.execute("formats.list", None).expect("formats.list");
    let answers = openstitch_mcp::skills::FILES
        .iter()
        .find(|(n, _)| *n == "reference/answers.md")
        .expect("answers.md")
        .1;
    let table = &answers[answers.find("## File formats").expect("formats section")..];
    let machine = list["machine"].as_array().expect("machine formats");
    assert!(machine.len() >= 22, "{list}");
    for f in machine {
        let ext = f["extension"].as_str().expect("extension").to_uppercase();
        assert!(table.contains(&ext), "answers.md File formats lacks {ext}");
    }
    for extra in [
        "G-code",
        "design.exportPicture",
        "design.exportVideo",
        "design.exportBundle",
    ] {
        assert!(
            table.contains(extra),
            "answers.md File formats lacks {extra}"
        );
    }
}

/// #600: the skill teaches the meta tools' `file` key; a `name` key is dropped by Ollama's
/// generic tool parser (#425), so no example may use it.
#[test]
fn skill_examples_of_scratch_write_and_skill_reference_use_the_file_key() {
    for (name, body) in openstitch_mcp::skills::FILES {
        for tool in ["scratch_write", "skill_reference"] {
            for (i, _) in body.match_indices(tool) {
                let after = &body[i + tool.len()..];
                let near = &after[..after.len().min(40)];
                if let Some(brace) = near.find('{') {
                    assert!(
                        !near[brace..]
                            .trim_start_matches(['{', ' '])
                            .starts_with("\"name\""),
                        "{name}: {tool} example uses the old \"name\" key"
                    );
                }
            }
        }
    }
}

#[test]
fn without_assistant_the_tool_list_is_per_command() {
    let (editor, client) = Editor::start();
    let mut server = Server::new(Config::default())
        .expect("server")
        .with_live(client);
    assert!(!server.is_assistant());
    server
        .handle_message(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}));
    let list = server
        .handle_message(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
        .expect("reply");
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    assert!(names.contains(&"design_new") && names.contains(&"preview"));
    // (`commands_list` is also the per-command tool for `commands.list`.)
    assert!(!names.contains(&"command_describe") && !names.contains(&"view_render"));
    let plain = Server::new(Config::default()).expect("server");
    assert_eq!(plain.tools().len(), server.tools().len());
    editor.finish();
}
