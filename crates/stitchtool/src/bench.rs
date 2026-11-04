//! `stitchtool bench-assistant` (B17 A5, docs/research/beyond/ai-integration.md section 5): fixed
//! embroidery tasks sent to a model or coding agent through the assistant's own tools, each
//! scored on the design it leaves (`object.list`, `design.info`), never on the model's words.
//!
//! The editor is headless: an engine [`Session`] behind `openstitch-mcp --assistant`, with
//! `ui.render` and `ui.scratchWrite` answered here. A coding agent reaches the same session
//! through `openstitch-mcp --assistant --endpoint` on a private loopback socket.

use std::path::{Path, PathBuf};

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use openstitch_assistant as ai;
use openstitch_engine::Session;
use serde_json::{Value, json};

/// One task: the design it starts from, what is asked, and what the design must show after.
pub struct Task {
    pub id: &'static str,
    setup: &'static [(&'static str, &'static str)],
    pub prompt: &'static str,
    check: fn(&Design) -> Result<(), String>,
}

/// What a task is scored on.
pub struct Design {
    pub objects: Vec<Value>,
    pub info: Value,
    /// The reply text, for the one task that asks a question.
    pub reply: String,
}

impl Design {
    fn sizes(&self) -> Vec<(f64, f64)> {
        let f = |o: &Value, k: &str| o["bounds"][k].as_f64().unwrap_or(0.0);
        self.objects
            .iter()
            .map(|o| (f(o, "width"), f(o, "height")))
            .collect()
    }
    fn colors(&self) -> Vec<String> {
        let mut c: Vec<String> = self
            .objects
            .iter()
            .flat_map(|o| [o["regionColor"].as_str(), o["lineColor"].as_str()])
            .flatten()
            .map(str::to_string)
            .collect();
        c.sort();
        c.dedup();
        c
    }
    fn has(&self, key: &str, value: &str) -> bool {
        self.objects.iter().any(|o| {
            o[key]
                .as_str()
                .is_some_and(|v| v.eq_ignore_ascii_case(value))
        })
    }
    fn text(&self) -> String {
        self.objects
            .iter()
            .filter_map(|o| o["text"].as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn stitches(&self) -> u64 {
        self.info["stitchStats"]["stitches"].as_u64().unwrap_or(0)
    }
}

fn need(ok: bool, what: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what.to_string()) }
}

fn widest(d: &Design) -> f64 {
    d.sizes().iter().map(|s| s.0).fold(0.0, f64::max)
}

/// Red-ish: R well above G and B.
fn reddish(hex: &str) -> bool {
    let v = |i: usize| {
        u8::from_str_radix(
            hex.trim_start_matches('#').get(i..i + 2).unwrap_or("00"),
            16,
        )
        .unwrap_or(0) as i32
    };
    v(0) > 140 && v(0) > v(2) + 60 && v(0) > v(4) + 60
}

const RECT: &str = r##"{"x": 20, "y": 20, "width": 30, "height": 20, "fill": "#1f3a93"}"##;
const PICTURE: &str = "{PICTURE}";

pub const TASKS: &[Task] = &[
    Task {
        id: "badge",
        setup: &[],
        prompt: "Make a 70 mm round badge: a navy fill circle with a satin border.",
        check: |d| {
            need((60.0..=80.0).contains(&widest(d)), "about 70 mm wide")?;
            need(
                d.has("region", "satin") || d.has("line", "zigzag"),
                "a satin border",
            )?;
            need(d.stitches() > 500, "stitches")
        },
    },
    Task {
        id: "monogram",
        setup: &[],
        prompt: "Make a monogram of the letters ABC, about 60 mm wide.",
        check: |d| {
            need(
                d.text().to_uppercase().contains("ABC") || d.text().to_uppercase().contains('A'),
                "lettering with ABC",
            )?;
            need((40.0..=80.0).contains(&widest(d)), "about 60 mm wide")
        },
    },
    Task {
        id: "arc-text",
        setup: &[],
        prompt: "Write OPENSTITCH around a circle about 50 mm across.",
        check: |d| {
            need(
                d.text().to_uppercase().contains("OPENSTITCH"),
                "the word OPENSTITCH",
            )?;
            need(widest(d) >= 30.0, "at least 30 mm wide")
        },
    },
    Task {
        id: "applique",
        setup: &[],
        prompt: "Make an appliqué patch from a 60 mm heart.",
        check: |d| {
            need(d.objects.len() >= 2, "placement and cover objects")?;
            need((45.0..=75.0).contains(&widest(d)), "about 60 mm")
        },
    },
    Task {
        id: "star",
        setup: &[],
        prompt: "Draw a red five-pointed star 40 mm across, filled.",
        check: |d| {
            need((30.0..=50.0).contains(&widest(d)), "about 40 mm")?;
            need(d.colors().iter().any(|c| reddish(c)), "red")
        },
    },
    Task {
        id: "two-colours",
        setup: &[],
        prompt: "Make a 50 mm green filled square with a 20 mm yellow filled circle in its middle.",
        check: |d| {
            need(d.objects.len() >= 2, "two objects")?;
            need(d.colors().len() >= 2, "two colours")
        },
    },
    Task {
        id: "wider",
        setup: &[("shape.rectangle", RECT)],
        prompt: "Make the rectangle twice as wide.",
        check: |d| {
            need(
                d.sizes().iter().any(|s| (55.0..=65.0).contains(&s.0)),
                "a rectangle about 60 mm wide",
            )
        },
    },
    Task {
        id: "recolour",
        setup: &[
            ("shape.rectangle", RECT),
            (
                "shape.ellipse",
                r##"{"cx": 70, "cy": 30, "rx": 10, "fill": "#1f3a93"}"##,
            ),
        ],
        prompt: "Make every object red.",
        check: |d| {
            need(
                !d.objects.is_empty()
                    && d.objects
                        .iter()
                        .all(|o| o["regionColor"].as_str().is_none_or(reddish)),
                "every fill red",
            )
        },
    },
    Task {
        id: "delete",
        setup: &[
            ("shape.rectangle", RECT),
            ("shape.ellipse", r##"{"cx": 64, "cy": 24, "rx": 4}"##),
            ("shape.ellipse", r##"{"cx": 92, "cy": 32, "rx": 12.5}"##),
        ],
        prompt: "Delete the smallest object.",
        check: |d| {
            need(d.objects.len() == 2, "two objects left")?;
            need(d.sizes().iter().all(|s| s.0 > 9.0), "the 8 mm circle gone")
        },
    },
    Task {
        id: "picture",
        setup: &[],
        prompt: "Turn the picture at {PICTURE} into embroidery, about 50 mm wide.",
        check: |d| {
            need(!d.objects.is_empty(), "objects")?;
            need((35.0..=65.0).contains(&widest(d)), "about 50 mm")?;
            need(d.colors().iter().any(|c| reddish(c)), "the red disc")
        },
    },
    Task {
        id: "vector",
        setup: &[],
        prompt: "Draw a simple house (walls, roof, door) as an SVG and import it as shapes.",
        check: |d| need(d.objects.len() >= 2, "several shapes"),
    },
    Task {
        id: "question",
        setup: &[
            ("shape.rectangle", RECT),
            ("shape.ellipse", r##"{"cx": 70, "cy": 30, "rx": 10}"##),
        ],
        prompt: "How many objects does the design have? Do not change anything.",
        check: |d| {
            need(d.objects.len() == 2, "the design unchanged")?;
            need(
                d.reply.contains('2') || d.reply.to_lowercase().contains("two"),
                "the answer 2",
            )
        },
    },
];

/// The headless editor: engine commands, plus the two `ui.*` commands the meta tools use.
pub struct Headless {
    pub session: Session,
    pub scratch: PathBuf,
}

impl Headless {
    pub fn new(scratch: PathBuf) -> Headless {
        Headless {
            session: Session::new(&scratch),
            scratch,
        }
    }

    pub fn handle(&mut self, request: &Value) -> Value {
        let params = &request["params"];
        let result = match request["command"].as_str().unwrap_or("") {
            "ui.render" => {
                let d = self.session.design();
                let pattern = openstitch_design::Compiler::for_design(d).compile(d);
                let size = params["size"].as_u64().unwrap_or(800).clamp(64, 2048) as u32;
                openstitch_render::preview::png(d, &pattern, size, false).map(|png| {
                    let stats = self.session.execute("design.info", None).map(|i| i["stitchStats"].clone()).unwrap_or_default();
                    json!({ "png": base64::engine::general_purpose::STANDARD.encode(png), "stats": stats })
                })
            }
            "ui.scratchWrite" => self.scratch_write(params),
            _ => return self.session.handle(request),
        };
        match result {
            Ok(v) => json!({ "ok": true, "result": v }),
            Err(e) => json!({ "ok": false, "error": e }),
        }
    }

    fn scratch_write(&self, p: &Value) -> Result<Value, String> {
        let name = p["name"]
            .as_str()
            .filter(|n| {
                !n.is_empty()
                    && n.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_. ".contains(c))
                    && !n.starts_with('.')
            })
            .ok_or("name must be a plain file name")?;
        let bytes = match (p["text"].as_str(), p["base64"].as_str()) {
            (Some(t), None) => t.as_bytes().to_vec(),
            (None, Some(b)) => base64::engine::general_purpose::STANDARD
                .decode(b.trim())
                .map_err(|_| "base64 is not valid")?,
            _ => return Err("give either text or base64".into()),
        };
        let path = self.scratch.join(name);
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        Ok(json!({ "path": path.display().to_string(), "bytes": bytes.len() }))
    }
}

struct Link(Arc<Mutex<Headless>>);

impl openstitch_mcp::EditorLink for Link {
    fn handle(&mut self, request: &Value) -> Value {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .handle(request)
    }
}

/// The assistant server's meta tools, as the panel offers them.
struct McpTools(openstitch_mcp::Server);

impl ai::Tools for McpTools {
    fn definitions(&self) -> Vec<Value> {
        self.0
            .tools()
            .iter()
            .map(|t| t.definition.clone())
            .collect()
    }

    fn call(&mut self, name: &str, input: &Value) -> ai::ToolResult {
        match self
            .0
            .tools()
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.kind.clone())
        {
            Some(kind) => ai::from_mcp_result(&self.0.call(&kind, input)),
            None => ai::from_mcp_result(&openstitch_mcp::server::tool_failure(&format!(
                "unknown tool '{name}'"
            ))),
        }
    }
}

/// How the tasks reach a model.
pub enum Backend {
    Model {
        config: ai::Config,
        transport: Box<dyn ai::Transport>,
    },
    Agent {
        config: ai::AcpConfig,
    },
}

/// One task's outcome.
#[derive(Debug, Default)]
pub struct Outcome {
    pub task: String,
    pub passed: bool,
    pub category: String,
    pub detail: String,
    pub tool_calls: Vec<String>,
    pub tool_errors: Vec<String>,
    pub reply: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub seconds: f64,
}

/// Sorts a failure (SolidCraft's categories, plan section 2.2).
fn classify(o: &Outcome, end: &ai::StopKind, check: &str) -> String {
    let errors = o.tool_errors.join(" ").to_lowercase();
    let r = o.reply.to_lowercase();
    match end {
        ai::StopKind::Error(e) => format!("request failed: {e}"),
        _ if o.tool_calls.is_empty()
            && ["\"command\"", "execute(", "run_batch", "<tool_call>"]
                .iter()
                .any(|t| r.contains(t)) =>
        {
            "wrote the call as text".into()
        }
        _ if o.tool_calls.is_empty() => "only talked".into(),
        _ if errors.contains("unknown command") || errors.contains("unknown tool") => {
            "unknown command".into()
        }
        _ if !o.tool_errors.is_empty() && o.tool_errors.len() * 2 >= o.tool_calls.len() => {
            "gave up after errors".into()
        }
        _ if !o.tool_errors.is_empty()
            && (errors.contains("invalid")
                || errors.contains("must")
                || errors.contains("missing")) =>
        {
            "wrong arguments".into()
        }
        _ => format!("wrong result: {check}"),
    }
}

/// Runs the tasks, one fresh design and conversation each.
pub fn run(
    backend: &Backend,
    tasks: &[&Task],
    scratch: &Path,
    picture: &Path,
    log: &mut dyn FnMut(&Outcome),
) -> Vec<Outcome> {
    let mut out = Vec::new();
    for task in tasks {
        let ed = Arc::new(Mutex::new(Headless::new(scratch.to_path_buf())));
        let setup: Result<Vec<Value>, String> = task
            .setup
            .iter()
            .map(|(command, params)| {
                let params = serde_json::from_str::<Value>(params).map_err(|e| e.to_string())?;
                let mut ed = ed.lock().unwrap_or_else(|e| e.into_inner());
                ed.session
                    .execute(command, Some(&params))
                    .map_err(|e| e.to_string())
            })
            .collect();
        if let Err(e) = setup {
            let o = Outcome {
                task: task.id.into(),
                category: format!("setup failed: {e}"),
                ..Outcome::default()
            };
            log(&o);
            out.push(o);
            continue;
        }
        let prompt = task.prompt.replace(PICTURE, &picture.display().to_string());
        let mut o = Outcome {
            task: task.id.into(),
            ..Outcome::default()
        };
        let start = Instant::now();
        let stop = AtomicBool::new(false);
        let mut on_event = |e: ai::Event| match e {
            ai::Event::Text(t) => o.reply.push_str(&t),
            ai::Event::ToolCall { name, input, .. } => o
                .tool_calls
                .push(format!("{name} {}", cut(&input.to_string(), 300))),
            ai::Event::ToolDone {
                is_error: true,
                summary,
                ..
            } => o.tool_errors.push(summary),
            ai::Event::Usage {
                input_tokens,
                output_tokens,
                cache_read_input_tokens,
                cache_creation_input_tokens,
            } => {
                o.input_tokens +=
                    input_tokens + cache_read_input_tokens + cache_creation_input_tokens;
                o.output_tokens += output_tokens;
            }
            _ => {}
        };
        let end = match backend {
            Backend::Model { config, transport } => {
                let server = match openstitch_mcp::Server::new(openstitch_mcp::Config::default()) {
                    Ok(s) => s.with_assistant(Link(ed.clone())),
                    Err(e) => {
                        return vec![Outcome {
                            category: format!("no tool server: {e}"),
                            ..Outcome::default()
                        }];
                    }
                };
                let mut tools = ai::WithImages {
                    inner: McpTools(server),
                    image: None,
                    transport: transport.as_ref(),
                };
                ai::Conversation::new().run_turn(
                    config,
                    transport.as_ref(),
                    &mut tools,
                    ai::user_content(&prompt, &[]),
                    &stop,
                    &mut on_event,
                )
            }
            Backend::Agent { config } => agent_turn(
                config,
                &mut ed.lock().unwrap_or_else(|e| e.into_inner()),
                &prompt,
                &mut on_event,
            ),
        };
        let mut ed = ed.lock().unwrap_or_else(|e| e.into_inner());
        o.seconds = start.elapsed().as_secs_f64();
        let design = Design {
            objects: ed
                .session
                .execute("object.list", None)
                .ok()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default(),
            info: ed.session.execute("design.info", None).unwrap_or_default(),
            reply: o.reply.clone(),
        };
        match (task.check)(&design) {
            Ok(()) if !matches!(end, ai::StopKind::Error(_)) => o.passed = true,
            Ok(()) => o.category = classify(&o, &end, ""),
            Err(why) => {
                o.category = classify(&o, &end, &why);
                o.detail = why;
            }
        }
        log(&o);
        out.push(o);
    }
    out
}

/// A coding agent's turn: it runs on its own thread while this one serves its MCP bridge's
/// requests from the private socket.
fn agent_turn(
    config: &ai::AcpConfig,
    ed: &mut Headless,
    prompt: &str,
    on_event: &mut dyn FnMut(ai::Event),
) -> ai::StopKind {
    let endpoint = ed.scratch.join("bench-endpoint.json");
    let listener = match openstitch_remote::Server::start(openstitch_remote::ServerConfig {
        write_roots: vec![ed.scratch.clone()],
        endpoint_file: Some(endpoint.clone()),
        ..openstitch_remote::ServerConfig::default()
    }) {
        Ok(l) => l,
        Err(e) => return ai::StopKind::Error(format!("no private socket: {e}")),
    };
    let mut config = config.clone();
    config.mcp.args = vec![
        "--assistant".into(),
        "--endpoint".into(),
        endpoint.display().to_string(),
    ];
    let (tx, rx) = std::sync::mpsc::channel();
    let text = prompt.to_string();
    let worker = std::thread::spawn(move || {
        let never = AtomicBool::new(false);
        let mut agent = match ai::AcpConversation::start(&config) {
            Ok(a) => a,
            Err(e) => return ai::StopKind::Error(format!("{e:?}")),
        };
        let end = agent.prompt(&text, &never, &mut |e| {
            let _ = tx.send(e);
        });
        agent.stop();
        end
    });
    while !worker.is_finished() {
        while let Ok(e) = rx.try_recv() {
            on_event(e);
        }
        if let Some(job) = listener.wait_next(Duration::from_millis(20)) {
            let reply = ed.handle(job.request());
            job.respond(reply);
        }
    }
    rx.try_iter().for_each(&mut *on_event);
    worker
        .join()
        .unwrap_or(ai::StopKind::Error("the agent thread panicked".into()))
}

fn cut(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// The JSON and Markdown reports.
pub fn reports(label: &str, outcomes: &[Outcome]) -> (Value, String) {
    let passed = outcomes.iter().filter(|o| o.passed).count();
    let (tin, tout) = outcomes
        .iter()
        .fold((0, 0), |a, o| (a.0 + o.input_tokens, a.1 + o.output_tokens));
    let json = json!({
        "backend": label, "passed": passed, "tasks": outcomes.len(), "inputTokens": tin, "outputTokens": tout,
        "results": outcomes.iter().map(|o| json!({
            "task": o.task, "passed": o.passed, "category": o.category, "detail": o.detail,
            "toolCalls": o.tool_calls, "toolErrors": o.tool_errors, "reply": o.reply,
            "inputTokens": o.input_tokens, "outputTokens": o.output_tokens, "seconds": (o.seconds * 10.0).round() / 10.0,
        })).collect::<Vec<_>>(),
    });
    let mut md = format!(
        "# bench-assistant: {label}\n\n{passed} of {} tasks passed; {tin} input and {tout} output tokens.\n\n| Task | Result | Tool calls (errors) | Tokens in / out | Seconds |\n|---|---|---|---|---|\n",
        outcomes.len()
    );
    for o in outcomes {
        let result = if o.passed {
            "pass".to_string()
        } else {
            format!("fail: {}", o.category)
        };
        md.push_str(&format!(
            "| {} | {} | {} ({}) | {} / {} | {:.1} |\n",
            o.task,
            result.replace('|', "/"),
            o.tool_calls.len(),
            o.tool_errors.len(),
            o.input_tokens,
            o.output_tokens,
            o.seconds
        ));
    }
    (json, md)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Replies with scripted Claude Messages streams, one per request; no network.
    struct Scripted(Mutex<VecDeque<String>>);

    impl ai::Transport for Scripted {
        fn post(
            &self,
            _: &str,
            _: &[(String, String)],
            _: &[u8],
            _: Duration,
        ) -> Result<Box<dyn std::io::Read + Send>, ai::HttpError> {
            let body = self.0.lock().expect("lock").pop_front().unwrap_or_default();
            Ok(Box::new(std::io::Cursor::new(body.into_bytes())))
        }
    }

    fn sse(data: Value) -> String {
        format!(
            "event: {}\ndata: {data}\n\n",
            data["type"].as_str().unwrap_or("")
        )
    }

    /// One assistant message: text, then optional `execute` calls.
    fn message(text: &str, calls: &[Value]) -> String {
        let mut s = sse(
            json!({"type": "message_start", "message": {"id": "m", "type": "message", "role": "assistant", "content": [],
            "model": "scripted", "stop_reason": null, "usage": {"input_tokens": 100, "output_tokens": 1}}}),
        );
        let mut blocks: Vec<Value> = vec![json!({"type": "text", "text": text})];
        blocks.extend(calls.iter().enumerate().map(|(i, c)| json!({"type": "tool_use", "id": format!("t{i}"), "name": "execute", "input": c})));
        for (i, b) in blocks.iter().enumerate() {
            let (start, delta) = if b["type"] == "text" {
                (
                    json!({"type": "text", "text": ""}),
                    json!({"type": "text_delta", "text": b["text"]}),
                )
            } else {
                (
                    json!({"type": "tool_use", "id": b["id"], "name": "execute", "input": {}}),
                    json!({"type": "input_json_delta", "partial_json": b["input"].to_string()}),
                )
            };
            s += &sse(json!({"type": "content_block_start", "index": i, "content_block": start}));
            s += &sse(json!({"type": "content_block_delta", "index": i, "delta": delta}));
            s += &sse(json!({"type": "content_block_stop", "index": i}));
        }
        let stop = if calls.is_empty() {
            "end_turn"
        } else {
            "tool_use"
        };
        s += &sse(
            json!({"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": null}, "usage": {"output_tokens": 20}}),
        );
        s + &sse(json!({"type": "message_stop"}))
    }

    #[test]
    fn the_benchmark_scores_the_design_against_a_scripted_endpoint() {
        let replies = [
            // wider: the model draws the wider rectangle.
            message(
                "",
                &[
                    json!({"command": "shape.rectangle", "params": {"x": 20, "y": 50, "width": 60, "height": 20}}),
                ],
            ),
            message("Done.", &[]),
            // star: only talks.
            message("I would draw a red star.", &[]),
            // question: answers without changing anything.
            message("The design has 2 objects.", &[]),
        ];
        let backend = Backend::Model {
            config: ai::Config {
                api_key: "sk-test".into(),
                ..ai::Config::default()
            },
            transport: Box::new(Scripted(Mutex::new(replies.into_iter().collect()))),
        };
        let pick = |id| TASKS.iter().find(|t| t.id == id).expect("task");
        let dir =
            std::env::temp_dir().join(format!("openstitch-bench-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let out = run(
            &backend,
            &[pick("wider"), pick("star"), pick("question")],
            &dir,
            &dir.join("none.bmp"),
            &mut |_| {},
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(out[0].passed, "{:?}", out[0]);
        assert_eq!(out[0].tool_calls.len(), 1);
        assert_eq!(
            (out[1].passed, out[1].category.as_str()),
            (false, "only talked")
        );
        assert!(out[2].passed, "{:?}", out[2]);
        assert!(out[0].input_tokens >= 200, "usage is counted");
        let (json, md) = reports("scripted", &out);
        assert_eq!(json["passed"], 2);
        assert!(md.contains("| star | fail: only talked |"));
        assert_eq!(TASKS.len(), 12);
    }

    #[test]
    fn every_task_setup_runs() {
        for t in TASKS {
            let mut s = Session::new(std::env::temp_dir());
            for (command, params) in t.setup {
                let p: Value = serde_json::from_str(params).expect("json");
                s.execute(command, Some(&p))
                    .unwrap_or_else(|e| panic!("{}: {e}", t.id));
            }
        }
    }

    #[test]
    fn failures_are_sorted_into_categories() {
        let o = |calls: &[&str], errors: &[&str], reply: &str| Outcome {
            tool_calls: calls.iter().map(|s| s.to_string()).collect(),
            tool_errors: errors.iter().map(|s| s.to_string()).collect(),
            reply: reply.into(),
            ..Outcome::default()
        };
        let end = ai::StopKind::EndTurn;
        assert_eq!(
            classify(&o(&[], &[], r#"{"command": "shape.star"}"#), &end, ""),
            "wrote the call as text"
        );
        assert_eq!(
            classify(&o(&["a"], &["unknown command 'x'"], ""), &end, ""),
            "unknown command"
        );
        assert_eq!(
            classify(&o(&["a", "b", "c"], &["width: must be > 0"], ""), &end, ""),
            "wrong arguments"
        );
        assert_eq!(
            classify(&o(&["a"], &["failed"], ""), &end, ""),
            "gave up after errors"
        );
        assert_eq!(
            classify(&o(&["a"], &[], ""), &end, "red"),
            "wrong result: red"
        );
        assert!(
            classify(&o(&[], &[], ""), &ai::StopKind::Error("401".into()), "")
                .starts_with("request failed")
        );
        assert!(reddish("#c81414") && !reddish("#1f3a93"));
    }
}
