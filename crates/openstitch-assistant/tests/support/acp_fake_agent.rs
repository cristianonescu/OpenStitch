//! A scripted Agent Client Protocol v1 agent for the tests: no model, no network.
//!
//! `openstitch-acp-fake-agent <scenario> <log file>`. Every line it receives is appended to the
//! log file, so a test can check what the client sent. Scenarios:
//! - `normal`: a turn with a thought, text, one bridge tool and one foreign tool (each with a
//!   permission question) and an `fs/read_text_file` request, then `end_turn`.
//! - `auth`: `session/new` fails with -32000 (authentication required).
//! - `cancel`: streams one chunk, then waits; on `session/cancel` ends the turn `cancelled`.
//! - `hang`: streams one chunk, then ignores everything (also its end of input).
//! - `grandchild`: starts a helper process (`sleep 300`), reports both ids in a chunk, ends the
//!   turn; then keeps running after end of input like a stubborn agent.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

struct Agent {
    input: std::io::Lines<std::io::StdinLock<'static>>,
    log: std::fs::File,
}

impl Agent {
    fn send(&self, msg: Value) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{msg}");
        let _ = out.flush();
    }

    fn update(&self, session: &str, update: Value) {
        self.send(json!({"jsonrpc": "2.0", "method": "session/update",
            "params": {"sessionId": session, "update": update}}));
    }

    fn chunk(&self, session: &str, kind: &str, text: &str) {
        self.update(
            session,
            json!({"sessionUpdate": kind, "content": {"type": "text", "text": text}}),
        );
    }

    /// The next message, logged; `None` at end of input.
    fn next(&mut self) -> Option<Value> {
        loop {
            let line = self.input.next()?.ok()?;
            let _ = writeln!(self.log, "{line}");
            let _ = self.log.flush();
            if let Ok(v) = serde_json::from_str(&line) {
                return Some(v);
            }
        }
    }

    /// Wait for the response to our request `id`.
    fn wait_reply(&mut self, id: i64) -> Option<Value> {
        loop {
            let msg = self.next()?;
            if msg.get("method").is_none() && msg.get("id").and_then(Value::as_i64) == Some(id) {
                return Some(msg);
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario = args.get(1).cloned().unwrap_or_else(|| "normal".into());
    let log_path = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "acp_fake_agent.log".into());
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .expect("log file");
    let mut agent = Agent {
        input: std::io::stdin().lock().lines(),
        log,
    };
    let session = "session-1";
    while let Some(msg) = agent.next() {
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        match msg.get("method").and_then(Value::as_str) {
            Some("initialize") => agent.send(json!({"jsonrpc": "2.0", "id": id, "result": {
                "protocolVersion": 1,
                "agentCapabilities": {"loadSession": false},
                "agentInfo": {"name": "fake", "version": "0.0.1"},
                "authMethods": [
                    {"id": "fake-login", "name": "Log in", "description": "Run fake login", "type": "terminal", "args": ["--login"]},
                    {"id": "fake-oauth", "name": "Browser"},
                ],
            }})),
            Some("session/new") if scenario == "auth" => agent.send(json!({"jsonrpc": "2.0", "id": id,
                "error": {"code": -32000, "message": "Authentication required"}})),
            Some("session/new") => {
                agent.send(json!({"jsonrpc": "2.0", "id": id, "result": {"sessionId": session}}))
            }
            Some("session/prompt") => match scenario.as_str() {
                "normal" => normal_turn(&mut agent, session, id),
                "cancel" => {
                    agent.chunk(session, "agent_message_chunk", "Working");
                    while let Some(m) = agent.next() {
                        if m.get("method").and_then(Value::as_str) == Some("session/cancel") {
                            agent.send(json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": "cancelled"}}));
                            break;
                        }
                    }
                }
                "hang" => {
                    agent.chunk(
                        session,
                        "agent_message_chunk",
                        &format!("pid:{}", std::process::id()),
                    );
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
                "grandchild" => {
                    #[allow(
                        clippy::zombie_processes,
                        reason = "the helper must outlive this agent: the test checks that stop() ends it"
                    )]
                    let helper = std::process::Command::new("sleep")
                        .arg("300")
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                        .expect("helper");
                    agent.chunk(
                        session,
                        "agent_message_chunk",
                        &format!("pids:{} {}", std::process::id(), helper.id()),
                    );
                    agent.send(json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": "end_turn"}}));
                }
                _ => {}
            },
            _ => {}
        }
    }
    if scenario == "grandchild" {
        // A stubborn agent: it outlives its input.
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

fn normal_turn(agent: &mut Agent, session: &str, prompt_id: Value) {
    agent.chunk(session, "agent_thought_chunk", "Planning");
    agent.chunk(session, "agent_message_chunk", "Hello ");
    // A line that is not ACP: the client must skip it.
    {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "not json");
        let _ = out.flush();
    }
    agent.update(
        session,
        json!({"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "mcp__openstitch-app__batch",
               "kind": "other", "status": "pending", "rawInput": {"commands": []}}),
    );
    let options = json!([
        {"optionId": "yes", "name": "Allow", "kind": "allow_once"},
        {"optionId": "always", "name": "Always", "kind": "allow_always"},
        {"optionId": "no", "name": "Reject", "kind": "reject_once"},
    ]);
    // The tool call carries only its id: the client must know the title from the update.
    agent.send(
        json!({"jsonrpc": "2.0", "id": 100, "method": "session/request_permission",
        "params": {"sessionId": session, "toolCall": {"toolCallId": "t1"}, "options": options}}),
    );
    let answer = agent.wait_reply(100);
    let allowed = answer
        .as_ref()
        .and_then(|a| a.pointer("/result/outcome/optionId"))
        .and_then(Value::as_str)
        == Some("yes");
    agent.update(
        session,
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1",
               "status": if allowed { "completed" } else { "failed" },
               "content": [{"type": "content", "content": {"type": "text", "text": "3 objects"}}]}),
    );
    agent.update(
        session,
        json!({"sessionUpdate": "tool_call", "toolCallId": "t2", "title": "Bash",
               "kind": "execute", "status": "pending", "rawInput": {"command": "ls"}}),
    );
    agent.send(json!({"jsonrpc": "2.0", "id": 101, "method": "session/request_permission",
        "params": {"sessionId": session, "toolCall": {"toolCallId": "t2", "title": "Bash", "kind": "execute"},
                   "options": options}}));
    let answer = agent.wait_reply(101);
    let rejected = answer
        .as_ref()
        .and_then(|a| a.pointer("/result/outcome/optionId"))
        .and_then(Value::as_str)
        == Some("no");
    agent.update(
        session,
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "t2",
               "status": if rejected { "failed" } else { "completed" }}),
    );
    // A request the client declared it doesn't serve.
    agent.send(
        json!({"jsonrpc": "2.0", "id": "fs-1", "method": "fs/read_text_file",
        "params": {"sessionId": session, "path": "/etc/hosts"}}),
    );
    let _ = agent.next();
    agent.chunk(session, "agent_message_chunk", "done.");
    agent.send(json!({"jsonrpc": "2.0", "id": prompt_id, "result": {"stopReason": "end_turn"}}));
}
