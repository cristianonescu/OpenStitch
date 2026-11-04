//! `stitchtool`: run OpenStitch commands from scripts or a JSON-lines control channel.
//!
//! ```text
//! stitchtool commands              # command catalogue (JSON)
//! stitchtool exec script.json      # JSON array of {"command", "params"}; stops at first failure
//! stitchtool serve [--read-only] [--no-files]
//!                                  # one JSON request per stdin line, one reply per stdout line;
//!                                  # --read-only refuses edits, undo/redo and file writes,
//!                                  # --no-files refuses every file read and write
//! stitchtool connect [--endpoint FILE]
//!                                  # same JSON lines, sent to the running editor (started with
//!                                  # `openstitch --listen PORT`); FILE defaults to automation.json
//!                                  # in the OpenStitch config directory
//! stitchtool compat [options]      # run the compatibility specs (compat/specs) and write the
//!                                  # report; see docs/COMPATIBILITY_TESTS.md
//! stitchtool bench-assistant [options]
//!                                  # the AI assistant benchmark (B17 A5); see bench_args
//! ```

mod bench;

use openstitch_engine::{Capabilities, Session};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::Path;
use std::process::ExitCode;

const MAX_LINE: usize = 1 << 20;
const USAGE: &str = "usage: stitchtool commands | exec <script.json> | serve [--read-only] [--no-files] | connect [--endpoint FILE] | compat [options] | bench-assistant [options]";
const BENCH_USAGE: &str =
    "usage: stitchtool bench-assistant [--protocol anthropic|openai] [--base-url URL] [--model M]
       [--key-env VAR] [--effort E] [--timeout S] [--agent ID [--agent-program PATH] [--mcp PATH]]
       [--tasks ID,ID] [--out DIR]
  The key is read from the environment variable VAR (default ANTHROPIC_API_KEY for anthropic,
  none for openai) and never printed. --agent runs a coding agent over ACP instead (claude,
  codex, opencode, ...). Writes DIR/report.json and DIR/report.md (DIR: bench-assistant).";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["commands"] => {
            let mut s = Session::new(cwd);
            match s.execute("commands.list", None) {
                Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default()),
                Err(e) => return fail(&e.to_string()),
            }
        }
        ["exec", script] => return exec(Path::new(script)),
        ["serve", flags @ ..] => {
            let mut caps = Capabilities::FULL;
            for f in flags {
                caps = match *f {
                    "--read-only" => caps.read_only(),
                    "--no-files" => caps.without_files(),
                    _ => return fail(USAGE),
                };
            }
            let mut session = Session::new(cwd);
            session.set_capabilities(caps);
            serve(session)
        }
        ["connect"] => return connect(None),
        ["connect", "--endpoint", file] => return connect(Some(Path::new(file))),
        ["compat", rest @ ..] => return compat(rest),
        ["bench-assistant", rest @ ..] => return bench_assistant(rest),
        _ => return fail(USAGE),
    }
    ExitCode::SUCCESS
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
}

fn exec(script: &Path) -> ExitCode {
    let text = match std::fs::read_to_string(script) {
        Ok(t) => t,
        Err(e) => return fail(&format!("cannot read {}: {e}", script.display())),
    };
    let Ok(Value::Array(steps)) = serde_json::from_str::<Value>(&text) else {
        return fail("script must be a JSON array of requests");
    };
    let base = script
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut session = Session::new(base);
    for step in &steps {
        let reply = session.handle(step);
        println!("{reply}");
        if reply["ok"] != json!(true) {
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

/// A 120 × 120 BMP of a red disc on white: the picture task's input.
fn disc_bmp() -> Vec<u8> {
    let (w, h) = (120u32, 120u32);
    let size = 54 + w * h * 3;
    let mut b = Vec::with_capacity(size as usize);
    b.extend(b"BM");
    for v in [size, 0, 54, 40, w, h] {
        b.extend(v.to_le_bytes());
    }
    b.extend(1u16.to_le_bytes());
    b.extend(24u16.to_le_bytes());
    b.extend([0u8; 24]);
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f64 - 60.0, y as f64 - 60.0);
            b.extend(if dx * dx + dy * dy < 1600.0 {
                [20, 20, 200]
            } else {
                [255, 255, 255]
            });
        }
    }
    b
}

fn bench_assistant(args: &[&str]) -> ExitCode {
    use openstitch_assistant as ai;
    let mut flags = std::collections::HashMap::new();
    let mut it = args.iter();
    while let Some(f) = it.next() {
        match (f.strip_prefix("--"), it.next()) {
            (Some(k), Some(v)) => flags.insert(k.to_string(), v.to_string()),
            _ => return fail(BENCH_USAGE),
        };
    }
    let get = |k: &str| flags.get(k).cloned();
    let selected: Vec<&bench::Task> = match get("tasks") {
        Some(list) => list
            .split(',')
            .filter_map(|id| bench::TASKS.iter().find(|t| t.id == id))
            .collect(),
        None => bench::TASKS.iter().collect(),
    };
    let out = std::path::PathBuf::from(get("out").unwrap_or_else(|| "bench-assistant".into()));
    let scratch = out.join("scratch");
    if let Err(e) = std::fs::create_dir_all(&scratch) {
        return fail(&format!("cannot make {}: {e}", scratch.display()));
    }
    let scratch = scratch.canonicalize().unwrap_or(scratch);
    let picture = scratch.join("disc.bmp");
    if let Err(e) = std::fs::write(&picture, disc_bmp()) {
        return fail(&e.to_string());
    }
    let timeout =
        std::time::Duration::from_secs(get("timeout").and_then(|t| t.parse().ok()).unwrap_or(300));
    let (backend, label) = if let Some(id) = get("agent") {
        let Some(agent) = ai::KNOWN_AGENTS.iter().find(|a| a.id == id) else {
            return fail(&format!("unknown agent {id}"));
        };
        let program = get("agent-program")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                ai::search_dirs()
                    .into_iter()
                    .map(|d| d.join(agent.program))
                    .find(|p| p.is_file())
            });
        let Some(program) = program else {
            return fail(&format!(
                "{} is not installed: {}",
                agent.name, agent.install
            ));
        };
        let mcp = get("mcp").map(std::path::PathBuf::from).or_else(|| {
            std::env::current_exe().ok().map(|e| {
                e.with_file_name(format!("openstitch-mcp{}", std::env::consts::EXE_SUFFIX))
            })
        });
        let work = scratch.join("agent-work");
        let _ = std::fs::create_dir_all(&work);
        let config = ai::AcpConfig {
            program,
            args: agent.args.iter().map(|a| a.to_string()).collect(),
            cwd: work,
            mcp: ai::McpServer {
                name: ai::BRIDGE_NAME.into(),
                command: mcp.unwrap_or_default(),
                args: Vec::new(),
                env: Vec::new(),
            },
            system: openstitch_mcp::server::assistant_instructions().to_string(),
            extra_path: ai::search_dirs(),
        };
        (
            bench::Backend::Agent { config },
            format!("agent {}", agent.name),
        )
    } else {
        let openai = get("protocol").as_deref() == Some("openai");
        let key_env = get("key-env").or_else(|| (!openai).then(|| "ANTHROPIC_API_KEY".into()));
        let api_key = key_env
            .as_deref()
            .and_then(|v| std::env::var(v).ok())
            .unwrap_or_default();
        if !openai && api_key.is_empty() {
            return fail("no key: set ANTHROPIC_API_KEY (or --key-env VAR)");
        }
        let config = ai::Config {
            protocol: if openai {
                ai::Protocol::OpenAi
            } else {
                ai::Protocol::Anthropic
            },
            base_url: get("base-url").unwrap_or_else(|| ai::DEFAULT_BASE_URL.into()),
            api_key,
            model: get("model").unwrap_or_else(|| ai::DEFAULT_MODEL.into()),
            effort: get("effort").unwrap_or_else(|| "high".into()),
            request_timeout: timeout,
            system: openstitch_mcp::server::assistant_instructions().to_string(),
            ..ai::Config::default()
        };
        let label = format!("{} at {}", config.model, config.base_url);
        (
            bench::Backend::Model {
                config,
                transport: Box::new(ai::UreqTransport),
            },
            label,
        )
    };
    let outcomes = bench::run(&backend, &selected, &scratch, &picture, &mut |o| {
        eprintln!(
            "{}: {}",
            o.task,
            if o.passed {
                "pass".to_string()
            } else {
                format!("fail ({})", o.category)
            }
        );
    });
    let (json, md) = bench::reports(&label, &outcomes);
    let written = std::fs::write(
        out.join("report.json"),
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    )
    .and_then(|()| std::fs::write(out.join("report.md"), &md));
    if let Err(e) = written {
        return fail(&e.to_string());
    }
    println!("{md}");
    ExitCode::SUCCESS
}

fn compat(args: &[&str]) -> ExitCode {
    let options = match openstitch_compat::cli::parse(args) {
        Ok(o) => o,
        Err(e) => return fail(&e),
    };
    match openstitch_compat::cli::run(&options) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => fail(&e),
    }
}

/// Forwards stdin request lines to the running editor and prints its replies.
fn connect(endpoint: Option<&Path>) -> ExitCode {
    let Some(file) = endpoint
        .map(Path::to_path_buf)
        .or_else(openstitch_remote::default_endpoint_file)
    else {
        return fail("no OpenStitch config directory; pass --endpoint FILE");
    };
    let mut client = match openstitch_remote::Client::from_file(&file) {
        Ok(c) => c,
        Err(e) => {
            return fail(&format!(
                "cannot read {} ({e}); is the editor running with --listen?",
                file.display()
            ));
        }
    };
    lines(|req| client.handle(req));
    ExitCode::SUCCESS
}

fn serve(mut session: Session) {
    lines(|req| session.handle(req));
}

/// One JSON request per stdin line, one reply per stdout line.
fn lines(mut handle: impl FnMut(&Value) -> Value) {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(req) if line.len() <= MAX_LINE => handle(&req),
            _ => json!({ "ok": false, "error": "invalid JSON request" }),
        };
        if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
            break;
        }
    }
}
