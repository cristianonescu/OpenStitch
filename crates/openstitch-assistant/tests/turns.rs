//! The tool loop against a scripted transport: no network.

use std::collections::VecDeque;
use std::io::Read;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use openstitch_assistant::{
    Config, Conversation, Event, HttpError, StopKind, ToolResult, Tools, Transport,
    from_mcp_result, key_safe_for, user_content,
};
use serde_json::{Value, json};

struct Request {
    url: String,
    headers: Vec<(String, String)>,
    body: Value,
}

/// Returns the scripted responses in order and records every request.
#[derive(Default)]
struct FakeTransport {
    responses: Mutex<VecDeque<Result<Vec<u8>, HttpError>>>,
    requests: Mutex<Vec<Request>>,
}

impl FakeTransport {
    fn new(responses: Vec<Result<Vec<u8>, HttpError>>) -> Self {
        FakeTransport {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn bodies(&self) -> Vec<Value> {
        self.requests
            .lock()
            .expect("lock")
            .iter()
            .map(|r| r.body.clone())
            .collect()
    }
}

impl Transport for FakeTransport {
    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &[u8],
        _timeout: Duration,
    ) -> Result<Box<dyn Read + Send>, HttpError> {
        self.requests.lock().expect("lock").push(Request {
            url: url.to_string(),
            headers: headers.to_vec(),
            body: serde_json::from_slice(body).expect("request body is JSON"),
        });
        let next = self
            .responses
            .lock()
            .expect("lock")
            .pop_front()
            .expect("a scripted response for every request");
        next.map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read + Send>)
    }
}

/// Records calls; `get` answers with text, `fail` with an error, `img` with a picture.
#[derive(Default)]
struct FakeTools {
    calls: Vec<(String, Value)>,
}

impl Tools for FakeTools {
    fn definitions(&self) -> Vec<Value> {
        vec![
            json!({"name": "get", "title": "Get", "description": "Reads a value.",
                   "inputSchema": {"type": "object", "properties": {"key": {"type": "string"}}},
                   "annotations": {"readOnlyHint": true}}),
            json!({"name": "set", "description": "Sets a value.",
                   "inputSchema": {"type": "object"}}),
        ]
    }

    fn call(&mut self, name: &str, input: &Value) -> ToolResult {
        self.calls.push((name.to_string(), input.clone()));
        match name {
            "fail" => ToolResult {
                content: vec![json!({"type": "text", "text": "no such thing"})],
                is_error: true,
            },
            _ => ToolResult {
                content: vec![json!({"type": "text", "text": format!("{name} ok")})],
                is_error: false,
            },
        }
    }
}

fn sse(events: &[Value]) -> Vec<u8> {
    let mut out = String::new();
    for e in events {
        let name = e["type"].as_str().expect("type");
        out.push_str(&format!("event: {name}\ndata: {e}\n\n"));
    }
    out.into_bytes()
}

fn start() -> Value {
    json!({"type": "message_start", "message": {"id": "msg_1", "type": "message", "role": "assistant",
        "model": "claude-opus-5-5", "content": [],
        "usage": {"input_tokens": 10, "output_tokens": 1, "cache_read_input_tokens": 5,
                  "cache_creation_input_tokens": 3}}})
}

fn block_start(index: u64, block: Value) -> Value {
    json!({"type": "content_block_start", "index": index, "content_block": block})
}

fn delta(index: u64, delta: Value) -> Value {
    json!({"type": "content_block_delta", "index": index, "delta": delta})
}

fn stop(index: u64) -> Value {
    json!({"type": "content_block_stop", "index": index})
}

fn message_delta(reason: &str) -> Value {
    json!({"type": "message_delta", "delta": {"stop_reason": reason, "stop_sequence": null},
           "usage": {"output_tokens": 42}})
}

fn end() -> Value {
    json!({"type": "message_stop"})
}

fn text_reply(text: &str) -> Vec<u8> {
    sse(&[
        start(),
        json!({"type": "ping"}),
        block_start(0, json!({"type": "text", "text": ""})),
        delta(0, json!({"type": "text_delta", "text": text})),
        stop(0),
        message_delta("end_turn"),
        end(),
    ])
}

/// A thinking block, then a tool call whose input streams in `fragments`.
fn thinking_events() -> Vec<Value> {
    vec![
        block_start(
            0,
            json!({"type": "thinking", "thinking": "", "signature": ""}),
        ),
        delta(0, json!({"type": "thinking_delta", "thinking": "Plan: "})),
        delta(0, json!({"type": "thinking_delta", "thinking": "read it."})),
        delta(
            0,
            json!({"type": "signature_delta", "signature": "sig-abc"}),
        ),
        stop(0),
    ]
}

fn tool_events(index: u64, id: &str, name: &str, fragments: &[&str]) -> Vec<Value> {
    let mut out = vec![block_start(
        index,
        json!({"type": "tool_use", "id": id, "name": name, "input": {}}),
    )];
    for f in fragments {
        out.push(delta(
            index,
            json!({"type": "input_json_delta", "partial_json": f}),
        ));
    }
    out.push(stop(index));
    out
}

fn cfg() -> Config {
    Config {
        api_key: "sk-test".into(),
        system: "You are the OpenStitch assistant.".into(),
        ..Config::default()
    }
}

fn run(
    conv: &mut Conversation,
    cfg: &Config,
    transport: &FakeTransport,
    tools: &mut FakeTools,
    text: &str,
) -> (StopKind, Vec<Event>) {
    let cancel = AtomicBool::new(false);
    let mut events = Vec::new();
    let stop = conv.run_turn(
        cfg,
        transport,
        tools,
        user_content(text, &[]),
        &cancel,
        &mut |e| events.push(e),
    );
    (stop, events)
}

#[test]
fn text_only_turn() {
    let t = FakeTransport::new(vec![Ok(text_reply("Hello."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, events) = run(&mut conv, &cfg(), &t, &mut tools, "Hi");
    assert_eq!(stop, StopKind::EndTurn);
    assert!(events.contains(&Event::Text("Hello.".into())));
    assert!(events.contains(&Event::Usage {
        input_tokens: 10,
        output_tokens: 42,
        cache_read_input_tokens: 5,
        cache_creation_input_tokens: 3,
    }));
    assert_eq!(
        conv.messages(),
        &[
            json!({"role": "user", "content": [{"type": "text", "text": "Hi"}]}),
            json!({"role": "assistant", "content": [{"type": "text", "text": "Hello."}]}),
        ]
    );
    assert!(tools.calls.is_empty());
}

#[test]
fn request_body_shape() {
    let t = FakeTransport::new(vec![Ok(text_reply("a")), Ok(text_reply("b"))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    run(&mut conv, &cfg(), &t, &mut tools, "Hi");
    let local = Config {
        base_url: "http://localhost:11434/".into(),
        ..cfg()
    };
    run(&mut conv, &local, &t, &mut tools, "Again");

    let reqs = t.requests.lock().expect("lock");
    let r = &reqs[0];
    assert_eq!(r.url, "https://api.anthropic.com/v1/messages");
    let header = |name: &str| {
        r.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(header("x-api-key").as_deref(), Some("sk-test"));
    assert_eq!(header("anthropic-version").as_deref(), Some("2026-10-21"));
    assert_eq!(header("content-type").as_deref(), Some("application/json"));
    assert_eq!(
        header("anthropic-beta").as_deref(),
        Some("server-side-fallback-2026-07-01")
    );
    let b = &r.body;
    assert_eq!(b["model"], "claude-opus-5-5");
    assert_eq!(b["max_tokens"], 64000);
    assert_eq!(b["stream"], true);
    assert_eq!(
        b["thinking"],
        json!({"type": "adaptive", "display": "summarized"})
    );
    assert_eq!(b["output_config"], json!({"effort": "high"}));
    assert_eq!(b["fallbacks"], "default");
    for absent in ["temperature", "top_p", "top_k", "tool_choice"] {
        assert!(b.get(absent).is_none(), "{absent} must not be sent");
    }
    assert_eq!(
        b["system"],
        json!([{"type": "text", "text": "You are the OpenStitch assistant.",
                "cache_control": {"type": "ephemeral"}}])
    );
    assert_eq!(
        b["tools"],
        json!([
            {"name": "get", "description": "Reads a value.",
             "input_schema": {"type": "object", "properties": {"key": {"type": "string"}}},
             "eager_input_streaming": true},
            {"name": "set", "description": "Sets a value.", "input_schema": {"type": "object"},
             "eager_input_streaming": true, "cache_control": {"type": "ephemeral"}},
        ])
    );
    for tool in b["tools"].as_array().expect("tools") {
        assert!(tool.get("strict").is_none());
    }

    let r2 = &reqs[1];
    assert_eq!(r2.url, "http://localhost:11434/v1/messages");
    assert!(r2.body.get("fallbacks").is_none());
    assert!(!r2.headers.iter().any(|(k, _)| k == "anthropic-beta"));
    // Same tools and system bytes on every request (cache prefix stability).
    assert_eq!(r2.body["tools"], b["tools"]);
    assert_eq!(r2.body["system"], b["system"]);
}

#[test]
fn tool_round_with_parallel_calls() {
    let mut first = vec![start()];
    first.extend(thinking_events());
    first.extend(tool_events(1, "toolu_1", "get", &["{\"ke", "y\": \"a\"}"]));
    first.extend(tool_events(2, "toolu_2", "fail", &["{}"]));
    first.extend([message_delta("tool_use"), end()]);
    let t = FakeTransport::new(vec![Ok(sse(&first)), Ok(text_reply("Done."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, events) = run(&mut conv, &cfg(), &t, &mut tools, "Read a");
    assert_eq!(stop, StopKind::EndTurn);
    assert_eq!(
        tools.calls,
        vec![
            ("get".to_string(), json!({"key": "a"})),
            ("fail".to_string(), json!({}))
        ]
    );
    assert!(events.contains(&Event::Thinking("Plan: ".into())));
    assert!(events.contains(&Event::ToolStart {
        id: "toolu_1".into(),
        name: "get".into()
    }));
    assert!(events.contains(&Event::ToolCall {
        id: "toolu_1".into(),
        name: "get".into(),
        input: json!({"key": "a"})
    }));
    assert!(events.contains(&Event::ToolDone {
        id: "toolu_2".into(),
        name: "fail".into(),
        is_error: true,
        summary: "no such thing".into()
    }));

    let assistant = json!({"role": "assistant", "content": [
        {"type": "thinking", "thinking": "Plan: read it.", "signature": "sig-abc"},
        {"type": "tool_use", "id": "toolu_1", "name": "get", "input": {"key": "a"}},
        {"type": "tool_use", "id": "toolu_2", "name": "fail", "input": {}},
    ]});
    let results = json!({"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": "toolu_1",
         "content": [{"type": "text", "text": "get ok"}]},
        {"type": "tool_result", "tool_use_id": "toolu_2",
         "content": [{"type": "text", "text": "no such thing"}], "is_error": true},
    ]});
    let bodies = t.bodies();
    assert_eq!(bodies.len(), 2);
    // The second request extends the first one's messages; nothing earlier changed.
    let m1 = bodies[0]["messages"].as_array().expect("messages");
    let m2 = bodies[1]["messages"].as_array().expect("messages");
    assert_eq!(&m2[..m1.len()], &m1[..]);
    assert_eq!(m2[1], assistant);
    assert_eq!(m2[2], results);
    assert_eq!(m2.len(), 3);
    assert_eq!(conv.messages().len(), 4);
    assert_eq!(conv.messages()[1], assistant);
}

#[test]
fn invalid_tool_json_is_not_run() {
    let mut first = vec![start()];
    first.extend(tool_events(0, "toolu_1", "set", &["{\"a\": ", "tru"]));
    first.extend(tool_events(1, "toolu_2", "set", &["[1]"]));
    first.extend([message_delta("tool_use"), end()]);
    let t = FakeTransport::new(vec![Ok(sse(&first)), Ok(text_reply("Sorry."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut tools, "Set it");
    assert_eq!(stop, StopKind::EndTurn);
    assert!(tools.calls.is_empty());
    let sent = &t.bodies()[1]["messages"][2]["content"];
    for (i, id) in ["toolu_1", "toolu_2"].iter().enumerate() {
        assert_eq!(sent[i]["tool_use_id"], *id);
        assert_eq!(sent[i]["is_error"], true);
        assert_eq!(
            sent[i]["content"][0]["text"],
            "tool input was not valid JSON; send it again"
        );
    }
    // The history keeps an object input so the request stays valid.
    assert_eq!(conv.messages()[1]["content"][0]["input"], json!({}));
}

#[test]
fn max_tokens_leaves_tool_unrun_and_answers_it_next_turn() {
    let mut first = vec![start()];
    first.extend(thinking_events());
    first.extend(tool_events(1, "toolu_9", "set", &["{\"value\": \"long"]));
    first.extend([message_delta("max_tokens"), end()]);
    let t = FakeTransport::new(vec![Ok(sse(&first)), Ok(text_reply("Ok."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut tools, "Write it");
    assert_eq!(stop, StopKind::MaxTokens);
    assert!(tools.calls.is_empty());
    assert_eq!(t.bodies().len(), 1);

    let (stop, _) = run(&mut conv, &cfg(), &t, &mut tools, "Shorter please");
    assert_eq!(stop, StopKind::EndTurn);
    let next = &t.bodies()[1]["messages"][2];
    assert_eq!(
        next,
        &json!({"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "toolu_9",
             "content": [{"type": "text", "text": "not run: the reply was cut off"}],
             "is_error": true},
            {"type": "text", "text": "Shorter please"},
        ]})
    );
}

#[test]
fn refusal_ends_the_turn_with_its_category() {
    let mut first = vec![start()];
    first.extend([
        block_start(0, json!({"type": "text", "text": ""})),
        delta(0, json!({"type": "text_delta", "text": "Let me"})),
        stop(0),
    ]);
    first.extend(tool_events(1, "toolu_r", "get", &["{\"key\""]));
    first.extend([
        json!({"type": "message_delta",
               "delta": {"stop_reason": "refusal", "stop_details": {"type": "refusal", "category": "cyber"}},
               "usage": {"output_tokens": 3}}),
        end(),
    ]);
    let t = FakeTransport::new(vec![Ok(sse(&first)), Ok(text_reply("Fine."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut tools, "Do it");
    assert_eq!(
        stop,
        StopKind::Refusal {
            category: Some("cyber".into())
        }
    );
    assert!(tools.calls.is_empty());
    run(&mut conv, &cfg(), &t, &mut tools, "Something else");
    let next = &t.bodies()[1]["messages"][2]["content"];
    assert_eq!(next[0]["tool_use_id"], "toolu_r");
    assert_eq!(next[0]["is_error"], true);
    assert_eq!(
        next[0]["content"][0]["text"],
        "not run: the request was declined"
    );
}

#[test]
fn refusal_before_any_output_adds_no_assistant_message() {
    let first = sse(&[
        start(),
        json!({"type": "message_delta", "delta": {"stop_reason": "refusal", "stop_details": null},
               "usage": {"output_tokens": 0}}),
        end(),
    ]);
    let t = FakeTransport::new(vec![Ok(first)]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut tools, "Hmm");
    assert_eq!(stop, StopKind::Refusal { category: None });
    assert_eq!(conv.messages().len(), 1);
}

#[test]
fn cancel_mid_stream() {
    let mut first = vec![start()];
    first.extend(tool_events(0, "toolu_c", "get", &["{}"]));
    first.extend([
        block_start(1, json!({"type": "text", "text": ""})),
        delta(1, json!({"type": "text_delta", "text": "Hel"})),
        delta(1, json!({"type": "text_delta", "text": "lo"})),
        stop(1),
        message_delta("tool_use"),
        end(),
    ]);
    let t = FakeTransport::new(vec![Ok(sse(&first)), Ok(text_reply("Ok."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let cancel = AtomicBool::new(false);
    let mut texts = Vec::new();
    let stop = conv.run_turn(
        &cfg(),
        &t,
        &mut tools,
        user_content("Go", &[]),
        &cancel,
        &mut |e| {
            if let Event::Text(s) = e {
                texts.push(s);
                cancel.store(true, Ordering::SeqCst);
            }
        },
    );
    assert_eq!(stop, StopKind::Cancelled);
    assert_eq!(texts, vec!["Hel".to_string()]);
    assert!(tools.calls.is_empty());
    // The finished tool call and the text shown so far are kept.
    assert_eq!(
        conv.messages()[1],
        json!({"role": "assistant", "content": [
            {"type": "tool_use", "id": "toolu_c", "name": "get", "input": {}},
            {"type": "text", "text": "Hel"},
        ]})
    );
    run(&mut conv, &cfg(), &t, &mut tools, "Continue");
    let next = &t.bodies()[1]["messages"][2]["content"];
    assert_eq!(next[0]["tool_use_id"], "toolu_c");
    assert_eq!(next[0]["content"][0]["text"], "stopped by the user");
    assert_eq!(next[1]["text"], "Continue");
}

#[test]
fn error_event_mid_stream() {
    let body = sse(&[
        start(),
        block_start(0, json!({"type": "text", "text": ""})),
        json!({"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}}),
    ]);
    let t = FakeTransport::new(vec![Ok(body)]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut tools, "Hi");
    match stop {
        StopKind::Error(m) => assert!(m.contains("Overloaded"), "{m}"),
        other => panic!("expected an error, got {other:?}"),
    }
    assert_eq!(conv.messages().len(), 1);
}

#[test]
fn stream_that_ends_early_is_an_error() {
    let body = sse(&[start(), block_start(0, json!({"type": "text", "text": ""}))]);
    let t = FakeTransport::new(vec![Ok(body)]);
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut FakeTools::default(), "Hi");
    assert!(matches!(stop, StopKind::Error(_)));
}

#[test]
fn http_401_is_readable_and_not_retried() {
    let t = FakeTransport::new(vec![Err(HttpError {
        status: Some(401),
        message: "invalid x-api-key".into(),
        retry_after: None,
    })]);
    let mut conv = Conversation::new();
    let (stop, events) = run(&mut conv, &cfg(), &t, &mut FakeTools::default(), "Hi");
    match stop {
        StopKind::Error(m) => {
            assert!(m.contains("API key") && m.contains("401"), "{m}");
            assert!(m.contains("invalid x-api-key"), "{m}");
        }
        other => panic!("expected an error, got {other:?}"),
    }
    assert!(events.is_empty());
    assert_eq!(t.bodies().len(), 1);
}

#[test]
fn rate_limit_is_retried_once_after_retry_after() {
    let limited = || {
        Err(HttpError {
            status: Some(429),
            message: "rate limited".into(),
            retry_after: Some(Duration::from_millis(5)),
        })
    };
    let t = FakeTransport::new(vec![limited(), Ok(text_reply("Back."))]);
    let mut conv = Conversation::new();
    let (stop, events) = run(&mut conv, &cfg(), &t, &mut FakeTools::default(), "Hi");
    assert_eq!(stop, StopKind::EndTurn);
    assert!(matches!(
        events[0],
        Event::Retrying { after, .. } if after == Duration::from_millis(5)
    ));
    assert_eq!(t.bodies().len(), 2);
    assert_eq!(t.bodies()[0], t.bodies()[1]);

    // A second failure is not retried again.
    let t = FakeTransport::new(vec![limited(), limited()]);
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut FakeTools::default(), "Hi");
    assert!(matches!(stop, StopKind::Error(ref m) if m.contains("429")));
    assert_eq!(t.bodies().len(), 2);
}

#[test]
fn refuses_to_send_the_key_unsafely() {
    let t = FakeTransport::new(vec![]);
    let mut conv = Conversation::new();
    let plain = Config {
        base_url: "http://example.com".into(),
        ..cfg()
    };
    let (stop, _) = run(&mut conv, &plain, &t, &mut FakeTools::default(), "Hi");
    assert!(matches!(stop, StopKind::Error(_)));
    let no_key = Config {
        api_key: String::new(),
        ..cfg()
    };
    let (stop, _) = run(&mut conv, &no_key, &t, &mut FakeTools::default(), "Hi");
    assert!(matches!(stop, StopKind::Error(_)));
    assert!(t.bodies().is_empty());
}

#[test]
fn more_than_32_calls_are_answered_with_errors() {
    let mut first = vec![start()];
    for i in 0..34u64 {
        first.extend(tool_events(i, &format!("toolu_{i}"), "get", &["{}"]));
    }
    first.extend([message_delta("tool_use"), end()]);
    let t = FakeTransport::new(vec![Ok(sse(&first)), Ok(text_reply("ok"))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    run(&mut conv, &cfg(), &t, &mut tools, "Many");
    assert_eq!(tools.calls.len(), 32);
    let results = &t.bodies()[1]["messages"][2]["content"];
    assert_eq!(results.as_array().expect("results").len(), 34);
    assert_eq!(results[33]["is_error"], true);
}

#[test]
fn fallback_boundary_drops_the_declined_partial() {
    let body = sse(&[
        start(),
        block_start(
            0,
            json!({"type": "thinking", "thinking": "", "signature": ""}),
        ),
        delta(0, json!({"type": "signature_delta", "signature": "s1"})),
        stop(0),
        block_start(1, json!({"type": "text", "text": "Part"})),
        stop(1),
        block_start(
            2,
            json!({"type": "fallback", "from": {"model": "claude-opus-5-5"}, "to": {"model": "claude-opus-5"}}),
        ),
        stop(2),
        block_start(3, json!({"type": "text", "text": ""})),
        delta(3, json!({"type": "text_delta", "text": "ial answer."})),
        stop(3),
        message_delta("end_turn"),
        end(),
    ]);
    let t = FakeTransport::new(vec![Ok(body)]);
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut FakeTools::default(), "Hi");
    assert_eq!(stop, StopKind::EndTurn);
    assert_eq!(
        conv.messages()[1]["content"],
        json!([{"type": "text", "text": "Part"}, {"type": "text", "text": "ial answer."}])
    );
}

#[test]
fn key_safety() {
    assert!(key_safe_for("https://api.anthropic.com"));
    assert!(key_safe_for("https://proxy.example.com:8443/anthropic"));
    assert!(key_safe_for("http://localhost:11434"));
    assert!(key_safe_for("http://127.0.0.1"));
    assert!(key_safe_for("http://[::1]:1"));
    assert!(key_safe_for("HTTP://LOCALHOST/"));
    assert!(!key_safe_for("http://example.com"));
    assert!(!key_safe_for("http://localhost.evil.com"));
    assert!(!key_safe_for("http://127.0.0.1.evil.com"));
    assert!(!key_safe_for("http://localhost@evil.com"));
    assert!(!key_safe_for("ftp://localhost"));
    assert!(!key_safe_for("https://"));
    assert!(!key_safe_for("localhost:8080"));
}

#[test]
fn mcp_results_convert() {
    let r = from_mcp_result(&json!({
        "content": [
            {"type": "text", "text": "rendered"},
            {"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"},
        ],
        "isError": false,
    }));
    assert!(!r.is_error);
    assert_eq!(
        r.content,
        vec![
            json!({"type": "text", "text": "rendered"}),
            json!({"type": "image", "source": {"type": "base64", "media_type": "image/png",
                                               "data": "iVBORw0KGgo="}}),
        ]
    );
    let e =
        from_mcp_result(&json!({"content": [{"type": "text", "text": "bad"}], "isError": true}));
    assert!(e.is_error);
}

#[test]
fn user_content_blocks() {
    let blocks = user_content("Look", &[("image/jpeg".into(), "AAAA".into())]);
    assert_eq!(
        blocks,
        vec![
            json!({"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": "AAAA"}}),
            json!({"type": "text", "text": "Look"}),
        ]
    );
}
