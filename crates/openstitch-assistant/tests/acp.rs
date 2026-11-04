//! The ACP client against a scripted test agent (`openstitch-acp-fake-agent`): no model, no
//! network.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::time::{Duration, Instant};

use openstitch_assistant::{
    AcpConfig, AcpConversation, AcpError, AgentState, AuthMethod, BRIDGE_NAME, Event, McpServer,
    StopKind, claude_disallowed_tools, discover_in,
};
use openstitch_testutil::TempDir;
use serde_json::Value;

const FAKE: &str = env!("CARGO_BIN_EXE_openstitch-acp-fake-agent");

/// A fresh empty folder under the system temp folder.
fn temp_dir(tag: &str) -> TempDir {
    TempDir::new(&format!("acp-{tag}"))
}

fn config(scenario: &str, dir: &Path) -> AcpConfig {
    AcpConfig {
        program: PathBuf::from(FAKE),
        args: vec![
            scenario.to_string(),
            dir.join("log.jsonl").to_string_lossy().into_owned(),
        ],
        cwd: dir.to_path_buf(),
        mcp: McpServer {
            name: BRIDGE_NAME.to_string(),
            command: PathBuf::from(FAKE).with_file_name("openstitch-mcp"),
            args: vec!["--assistant".into()],
            env: vec![("OPENSTITCH_TOKEN_FILE".into(), "/tmp/token".into())],
        },
        system: "You are OpenStitch's assistant.".into(),
        extra_path: Vec::new(),
    }
}

/// Every message the fake agent received.
fn received(dir: &Path) -> Vec<Value> {
    std::fs::read_to_string(dir.join("log.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn method<'a>(msgs: &'a [Value], name: &str) -> &'a Value {
    msgs.iter()
        .find(|m| m.get("method").and_then(Value::as_str) == Some(name))
        .unwrap_or_else(|| panic!("no {name} in {msgs:?}"))
}

fn reply_to(msgs: &[Value], id: Value) -> &Value {
    msgs.iter()
        .find(|m| m.get("method").is_none() && m.get("id") == Some(&id))
        .unwrap_or_else(|| panic!("no reply to {id} in {msgs:?}"))
}

#[test]
fn a_turn_maps_updates_and_answers_permissions() {
    let dir = temp_dir("turn");
    let mut conv = AcpConversation::start(&config("normal", &dir)).expect("start");
    let mut events = Vec::new();
    let stop = conv.prompt("Make a star", &AtomicBool::new(false), &mut |e| {
        events.push(e)
    });
    assert_eq!(stop, StopKind::EndTurn, "{events:?}");
    assert_eq!(events[0], Event::Thinking("Planning".into()));
    assert_eq!(events[1], Event::Text("Hello ".into()));
    let bridge = "mcp__openstitch-app__batch".to_string();
    assert!(events.contains(&Event::ToolStart {
        id: "t1".into(),
        name: bridge.clone()
    }));
    assert!(events.contains(&Event::ToolCall {
        id: "t1".into(),
        name: bridge.clone(),
        input: serde_json::json!({"commands": []}),
    }));
    assert!(events.contains(&Event::ToolDone {
        id: "t1".into(),
        name: bridge,
        is_error: false,
        summary: "3 objects".into(),
    }));
    assert!(events.contains(&Event::ToolDone {
        id: "t2".into(),
        name: "Bash".into(),
        is_error: true,
        summary: "Failed".into(),
    }));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Text(t) if t.contains("refused") && t.contains("Bash"))),
        "{events:?}"
    );
    assert_eq!(events.last(), Some(&Event::Text("done.".into())));

    let msgs = received(&dir);
    // Bridge tool allowed once, the foreign one rejected.
    let allow = reply_to(&msgs, 100.into());
    assert_eq!(allow["result"]["outcome"]["outcome"], "selected");
    assert_eq!(allow["result"]["outcome"]["optionId"], "yes");
    let reject = reply_to(&msgs, 101.into());
    assert_eq!(reject["result"]["outcome"]["optionId"], "no");
    // fs/* answered "method not found".
    let fs = reply_to(&msgs, "fs-1".into());
    assert_eq!(fs["error"]["code"], -32601);
    // The prompt itself.
    let prompt = method(&msgs, "session/prompt");
    assert_eq!(prompt["params"]["sessionId"], "session-1");
    assert_eq!(prompt["params"]["prompt"][0]["type"], "text");
    assert_eq!(prompt["params"]["prompt"][0]["text"], "Make a star");
    conv.stop();
}

#[test]
fn initialize_declares_no_file_system_and_no_terminal() {
    let dir = temp_dir("init");
    let conv = AcpConversation::start(&config("normal", &dir)).expect("start");
    drop(conv);
    let msgs = received(&dir);
    let init = &method(&msgs, "initialize")["params"];
    assert_eq!(init["protocolVersion"], 1);
    let caps = &init["clientCapabilities"];
    assert_eq!(caps["fs"]["readTextFile"], false);
    assert_eq!(caps["fs"]["writeTextFile"], false);
    assert_eq!(caps["terminal"], false);
    assert_eq!(init["clientInfo"]["name"], "openstitch");
}

#[test]
fn session_new_carries_the_bridge_and_turns_claude_codes_tools_off() {
    let dir = temp_dir("session");
    let cfg = config("normal", &dir);
    let conv = AcpConversation::start(&cfg).expect("start");
    drop(conv);
    let msgs = received(&dir);
    let params = &method(&msgs, "session/new")["params"];
    assert!(Path::new(params["cwd"].as_str().expect("cwd")).is_absolute());
    let servers = params["mcpServers"].as_array().expect("mcpServers");
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["name"], BRIDGE_NAME);
    assert_eq!(
        servers[0]["command"],
        cfg.mcp.command.to_string_lossy().as_ref()
    );
    assert_eq!(servers[0]["args"], serde_json::json!(["--assistant"]));
    assert_eq!(
        servers[0]["env"],
        serde_json::json!([{"name": "OPENSTITCH_TOKEN_FILE", "value": "/tmp/token"}])
    );
    let disallowed: Vec<&str> = params["_meta"]["claudeCode"]["options"]["disallowedTools"]
        .as_array()
        .expect("disallowedTools")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(disallowed, claude_disallowed_tools());
    for tool in ["Bash", "Read", "Write", "Edit", "WebFetch", "WebSearch"] {
        assert!(disallowed.contains(&tool), "{tool}");
    }
    assert_eq!(
        params["_meta"]["systemPrompt"]["append"],
        "You are OpenStitch's assistant."
    );
}

#[test]
fn authentication_required_is_surfaced_with_the_methods() {
    let dir = temp_dir("auth");
    match AcpConversation::start(&config("auth", &dir)) {
        Err(AcpError::AuthRequired(methods)) => assert_eq!(
            methods,
            vec![
                AuthMethod::Command {
                    id: "fake-login".into(),
                    name: "Log in".into(),
                    description: Some("Run fake login".into()),
                },
                AuthMethod::Other {
                    id: "fake-oauth".into(),
                    name: "Browser".into()
                },
            ]
        ),
        other => panic!("expected AuthRequired, got {other:?}"),
    }
}

/// An agent that ends before answering `initialize` fails the start with its stderr tail.
#[test]
fn an_agent_that_ends_at_once_fails_the_start() {
    let dir = temp_dir("ends");
    let mut cfg = config("normal", &dir);
    // The fake agent cannot open a log file inside a missing folder: it panics on stderr.
    cfg.args[1] = dir
        .join("missing")
        .join("log.jsonl")
        .to_string_lossy()
        .into_owned();
    match AcpConversation::start(&cfg) {
        Err(AcpError::Start(m)) => assert!(m.contains("log file"), "{m}"),
        other => panic!("expected Start, got {other:?}"),
    }
}

#[test]
fn a_missing_program_is_not_found() {
    let dir = temp_dir("missing");
    let mut cfg = config("normal", &dir);
    cfg.program = dir.join("no-such-agent");
    assert!(matches!(
        AcpConversation::start(&cfg),
        Err(AcpError::NotFound(_))
    ));
}

#[test]
fn cancel_ends_the_turn_cancelled() {
    let dir = temp_dir("cancel");
    let mut conv = AcpConversation::start(&config("cancel", &dir)).expect("start");
    let cancel = AtomicBool::new(false);
    let mut events = Vec::new();
    let stop = conv.prompt("Go", &cancel, &mut |e| {
        if matches!(e, Event::Text(_)) {
            cancel.store(true, Ordering::SeqCst);
        }
        events.push(e);
    });
    assert_eq!(stop, StopKind::Cancelled);
    let msgs = received(&dir);
    assert_eq!(
        method(&msgs, "session/cancel")["params"]["sessionId"],
        "session-1"
    );
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    // A zombie still counts on Linux, so /proc's state is checked there too.
    #[cfg(target_os = "linux")]
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        return !stat
            .rsplit(')')
            .next()
            .is_some_and(|s| s.trim_start().starts_with('Z'));
    }
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(unix)]
fn gone_within(pids: &[u32], limit: Duration) -> bool {
    let start = Instant::now();
    while pids.iter().any(|&p| alive(p)) {
        if start.elapsed() > limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

/// An agent that ignores `session/cancel` is stopped 5 s later and the turn ends Cancelled.
/// Unix only: the check that the process is gone uses `kill -0`.
#[cfg(unix)]
#[test]
fn an_agent_ignoring_cancel_is_killed() {
    let dir = temp_dir("hang");
    let mut conv = AcpConversation::start(&config("hang", &dir)).expect("start");
    let cancel = AtomicBool::new(false);
    let mut pid = None;
    let started = Instant::now();
    let stop = conv.prompt("Go", &cancel, &mut |e| {
        if let Event::Text(t) = &e
            && let Some(p) = t.strip_prefix("pid:")
        {
            pid = p.trim().parse::<u32>().ok();
            cancel.store(true, Ordering::SeqCst);
        }
    });
    assert_eq!(stop, StopKind::Cancelled);
    assert!(started.elapsed() >= Duration::from_secs(5));
    let pid = pid.expect("the agent's pid");
    assert!(
        gone_within(&[pid], Duration::from_secs(5)),
        "agent {pid} still running"
    );
    // A later prompt says the agent has stopped instead of hanging.
    let again = conv.prompt("More", &AtomicBool::new(false), &mut |_| {});
    assert!(matches!(again, StopKind::Error(_)), "{again:?}");
}

/// stop() ends the agent and the helper it started (its process group). Unix only: the helper is
/// `sleep`, and Windows' `taskkill /T` path is not exercised here.
#[cfg(unix)]
#[test]
fn stop_kills_the_agent_and_its_helpers() {
    let dir = temp_dir("group");
    let mut conv = AcpConversation::start(&config("grandchild", &dir)).expect("start");
    let mut pids = Vec::new();
    let stop = conv.prompt("Go", &AtomicBool::new(false), &mut |e| {
        if let Event::Text(t) = &e
            && let Some(p) = t.strip_prefix("pids:")
        {
            pids = p
                .split_whitespace()
                .filter_map(|x| x.parse::<u32>().ok())
                .collect();
        }
    });
    assert_eq!(stop, StopKind::EndTurn);
    assert_eq!(pids.len(), 2, "{pids:?}");
    assert!(pids.iter().all(|&p| alive(p)), "running before stop");
    conv.stop();
    assert!(
        gone_within(&pids, Duration::from_secs(5)),
        "still running: {pids:?}"
    );
}

#[cfg(unix)]
fn script(dir: &Path, name: &str, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

#[cfg(windows)]
fn script(dir: &Path, name: &str, body: &str) {
    // `echo 2.1.0` / `exit 1` read the same in cmd.
    std::fs::write(dir.join(format!("{name}.cmd")), format!("@{body}\r\n")).expect("write");
}

fn state_of(found: &[openstitch_assistant::Discovered], id: &str) -> AgentState {
    found
        .iter()
        .find(|d| d.agent.id == id)
        .map(|d| d.state.clone())
        .expect("listed")
}

#[test]
fn discovery_reports_ready_missing_and_broken_agents() {
    let dir = temp_dir("discover");
    let home = temp_dir("home");
    std::fs::create_dir_all(home.join(".claude")).expect("mkdir");
    std::fs::write(home.join(".claude/.credentials.json"), "{}").expect("write");
    script(&dir, "claude-agent-acp", "echo 2.1.0");
    script(&dir, "codex-acp", "exit 1");
    let found = discover_in(&[dir.to_path_buf()], Some(&home));
    assert_eq!(found.len(), openstitch_assistant::KNOWN_AGENTS.len());
    assert_eq!(
        state_of(&found, "claude"),
        AgentState::Ready {
            version: "2.1.0".into(),
            logged_in: Some(true)
        }
    );
    assert!(
        matches!(state_of(&found, "codex"), AgentState::DoesNotStart(_)),
        "{:?}",
        state_of(&found, "codex")
    );
    assert_eq!(state_of(&found, "opencode"), AgentState::NotInstalled);
    let claude = found
        .iter()
        .find(|d| d.agent.id == "claude")
        .expect("claude");
    assert!(claude.path.as_ref().is_some_and(|p| p.starts_with(&dir)));
    let opencode = found
        .iter()
        .find(|d| d.agent.id == "opencode")
        .expect("opencode");
    assert_eq!(opencode.agent.args, ["acp"]);
    assert!(opencode.path.is_none());
}
