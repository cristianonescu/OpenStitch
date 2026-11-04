//! The OpenAI-compatible protocol, model lists and the Ollama check against a scripted
//! transport: no network.

use std::collections::VecDeque;
use std::io::Read;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use openstitch_assistant::{
    Config, Conversation, Event, HttpError, ModelInfo, Protocol, StopKind, ToolResult, Tools,
    Transport, edit_request_without_tools, list_models, ollama_tools_capability, user_content,
};
use serde_json::{Value, json};

struct Request {
    method: &'static str,
    url: String,
    headers: Vec<(String, String)>,
    body: Value,
}

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
        let r = self.requests.lock().expect("lock");
        r.iter().map(|r| r.body.clone()).collect()
    }

    fn next(&self) -> Result<Box<dyn Read + Send>, HttpError> {
        let next = self
            .responses
            .lock()
            .expect("lock")
            .pop_front()
            .expect("a scripted response for every request");
        next.map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read + Send>)
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
            method: "POST",
            url: url.to_string(),
            headers: headers.to_vec(),
            body: serde_json::from_slice(body).expect("request body is JSON"),
        });
        self.next()
    }

    fn get(
        &self,
        url: &str,
        headers: &[(String, String)],
        _timeout: Duration,
    ) -> Result<Box<dyn Read + Send>, HttpError> {
        self.requests.lock().expect("lock").push(Request {
            method: "GET",
            url: url.to_string(),
            headers: headers.to_vec(),
            body: Value::Null,
        });
        self.next()
    }
}

/// A transport written before `get` existed: the default refuses.
struct PostOnly;

impl Transport for PostOnly {
    fn post(
        &self,
        _url: &str,
        _headers: &[(String, String)],
        _body: &[u8],
        _timeout: Duration,
    ) -> Result<Box<dyn Read + Send>, HttpError> {
        unreachable!("not posted to")
    }
}

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
            "shot" => ToolResult {
                content: vec![
                    json!({"type": "text", "text": "a picture"}),
                    json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}}),
                ],
                is_error: false,
            },
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

/// `data:` lines of chunks, then `data: [DONE]`.
fn stream(chunks: &[Value]) -> Vec<u8> {
    let mut out = String::new();
    for c in chunks {
        out.push_str(&format!("data: {c}\n\n"));
    }
    out.push_str("data: [DONE]\n\n");
    out.into_bytes()
}

fn chunk(delta: Value, finish: Option<&str>) -> Value {
    json!({"id": "chatcmpl-1", "object": "chat.completion.chunk", "created": 1, "model": "m",
           "choices": [{"index": 0, "delta": delta, "finish_reason": finish}], "usage": null})
}

fn usage() -> Value {
    json!({"id": "chatcmpl-1", "object": "chat.completion.chunk", "created": 1, "model": "m",
           "choices": [], "usage": {"prompt_tokens": 12, "completion_tokens": 7, "total_tokens": 19,
           "prompt_tokens_details": {"cached_tokens": 4}}})
}

fn text_reply(text: &str) -> Vec<u8> {
    stream(&[
        chunk(json!({"role": "assistant", "content": ""}), None),
        chunk(json!({"content": text}), None),
        chunk(json!({}), Some("stop")),
        usage(),
    ])
}

fn call_start(index: u64, id: &str, name: &str, args: &str) -> Value {
    chunk(
        json!({"tool_calls": [{"index": index, "id": id, "type": "function",
                               "function": {"name": name, "arguments": args}}]}),
        None,
    )
}

fn call_args(index: u64, args: &str) -> Value {
    chunk(
        json!({"tool_calls": [{"index": index, "function": {"arguments": args}}]}),
        None,
    )
}

fn cfg() -> Config {
    Config {
        protocol: Protocol::OpenAi,
        base_url: "http://localhost:11434/v1".into(),
        model: "qwen3".into(),
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
fn text_turn() {
    let t = FakeTransport::new(vec![Ok(text_reply("Hello."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, events) = run(&mut conv, &cfg(), &t, &mut tools, "Hi");
    assert_eq!(stop, StopKind::EndTurn);
    assert_eq!(
        events,
        vec![
            Event::Text("Hello.".into()),
            Event::Usage {
                input_tokens: 12,
                output_tokens: 7,
                cache_read_input_tokens: 4,
                cache_creation_input_tokens: 0,
            },
        ]
    );
    assert_eq!(conv.protocol(), Protocol::OpenAi);
    assert_eq!(
        conv.messages(),
        &[
            json!({"role": "user", "content": "Hi"}),
            json!({"role": "assistant", "content": "Hello."}),
        ]
    );
    let reqs = t.requests.lock().expect("lock");
    assert_eq!(reqs[0].url, "http://localhost:11434/v1/chat/completions");
}

#[test]
fn request_body_and_headers() {
    let t = FakeTransport::new(vec![Ok(text_reply("a")), Ok(text_reply("b"))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    run(&mut conv, &cfg(), &t, &mut tools, "Hi");
    let body = &t.bodies()[0];
    assert_eq!(
        body,
        &json!({
            "model": "qwen3",
            "stream": true,
            "stream_options": {"include_usage": true},
            "messages": [
                {"role": "system", "content": "You are the OpenStitch assistant."},
                {"role": "user", "content": "Hi"},
            ],
            "tools": [
                {"type": "function", "function": {"name": "get", "description": "Reads a value.",
                 "parameters": {"type": "object", "properties": {"key": {"type": "string"}}}}},
                {"type": "function", "function": {"name": "set", "description": "Sets a value.",
                 "parameters": {"type": "object"}}},
            ],
        })
    );
    {
        let reqs = t.requests.lock().expect("lock");
        // No key: no Authorization header at all.
        assert_eq!(
            reqs[0].headers,
            vec![("content-type".to_string(), "application/json".to_string())]
        );
    }

    let with_key = Config {
        api_key: " sk-or-1 ".into(),
        reasoning_effort: Some("high".into()),
        ..cfg()
    };
    run(&mut conv, &with_key, &t, &mut tools, "Again");
    let body = &t.bodies()[1];
    assert_eq!(body["reasoning_effort"], "high");
    assert!(body.get("temperature").is_none());
    assert!(body.get("max_tokens").is_none());
    assert!(body.get("max_completion_tokens").is_none());
    let reqs = t.requests.lock().expect("lock");
    assert!(
        reqs[1]
            .headers
            .contains(&("Authorization".to_string(), "Bearer sk-or-1".to_string()))
    );
}

#[test]
fn tool_round_streamed_across_chunks_with_two_parallel_calls() {
    let first = stream(&[
        chunk(json!({"role": "assistant", "content": "Looking."}), None),
        call_start(0, "call_a", "get", ""),
        call_args(0, "{\"ke"),
        call_start(1, "call_b", "shot", "{"),
        call_args(0, "y\": \"x\"}"),
        call_args(1, "}"),
        chunk(json!({}), Some("tool_calls")),
        usage(),
    ]);
    let t = FakeTransport::new(vec![Ok(first), Ok(text_reply("Done."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, events) = run(&mut conv, &cfg(), &t, &mut tools, "Read it");
    assert_eq!(stop, StopKind::EndTurn);
    assert_eq!(
        tools.calls,
        vec![
            ("get".to_string(), json!({"key": "x"})),
            ("shot".to_string(), json!({})),
        ]
    );
    let starts: Vec<&Event> = events
        .iter()
        .filter(|e| matches!(e, Event::ToolStart { .. }))
        .collect();
    assert_eq!(
        starts,
        vec![
            &Event::ToolStart {
                id: "call_a".into(),
                name: "get".into()
            },
            &Event::ToolStart {
                id: "call_b".into(),
                name: "shot".into()
            },
        ]
    );
    let second = &t.bodies()[1]["messages"];
    assert_eq!(
        second[2],
        json!({"role": "assistant", "content": "Looking.", "tool_calls": [
            {"id": "call_a", "type": "function", "function": {"name": "get", "arguments": "{\"key\": \"x\"}"}},
            {"id": "call_b", "type": "function", "function": {"name": "shot", "arguments": "{}"}},
        ]})
    );
    assert_eq!(
        second[3],
        json!({"role": "tool", "tool_call_id": "call_a", "content": "get ok"})
    );
    assert_eq!(
        second[4],
        json!({"role": "tool", "tool_call_id": "call_b",
               "content": "a picture\n(image omitted: this endpoint takes text tool results)"})
    );
    assert_eq!(second.as_array().map(Vec::len), Some(5));
}

#[test]
fn invalid_arguments_are_not_run() {
    let first = stream(&[
        call_start(0, "call_x", "set", "{\"a\": "),
        chunk(json!({}), Some("tool_calls")),
    ]);
    let t = FakeTransport::new(vec![Ok(first), Ok(text_reply("Sorry."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, events) = run(&mut conv, &cfg(), &t, &mut tools, "Set it");
    assert_eq!(stop, StopKind::EndTurn);
    assert!(tools.calls.is_empty());
    assert!(events.contains(&Event::ToolDone {
        id: "call_x".into(),
        name: "set".into(),
        is_error: true,
        summary: "tool input was not valid JSON; send it again".into(),
    }));
    let msgs = &t.bodies()[1]["messages"];
    assert_eq!(msgs[2]["tool_calls"][0]["function"]["arguments"], "{}");
    assert_eq!(
        msgs[3]["content"],
        "Error: tool input was not valid JSON; send it again"
    );
}

#[test]
fn length_leaves_the_call_unrun_and_answers_it_next_turn() {
    let first = stream(&[
        call_start(0, "call_l", "set", "{\"a\": 1"),
        chunk(json!({}), Some("length")),
    ]);
    let t = FakeTransport::new(vec![Ok(first), Ok(text_reply("Ok."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut tools, "Set it");
    assert_eq!(stop, StopKind::MaxTokens);
    assert!(tools.calls.is_empty());
    assert_eq!(t.bodies().len(), 1);
    run(&mut conv, &cfg(), &t, &mut tools, "Go on");
    let msgs = &t.bodies()[1]["messages"];
    assert_eq!(msgs[2]["tool_calls"][0]["id"], "call_l");
    assert_eq!(
        msgs[3],
        json!({"role": "tool", "tool_call_id": "call_l", "content": "not run: the reply was cut off"})
    );
    assert_eq!(msgs[4], json!({"role": "user", "content": "Go on"}));
}

#[test]
fn content_filter_is_a_refusal_and_runs_nothing() {
    let first = stream(&[
        call_start(0, "call_f", "get", "{}"),
        chunk(json!({}), Some("content_filter")),
    ]);
    let t = FakeTransport::new(vec![Ok(first), Ok(text_reply("Ok."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut tools, "Do it");
    assert_eq!(
        stop,
        StopKind::Refusal {
            category: Some("content_filter".into())
        }
    );
    assert!(tools.calls.is_empty());
    run(&mut conv, &cfg(), &t, &mut tools, "Next");
    let msgs = &t.bodies()[1]["messages"];
    assert_eq!(
        msgs[3],
        json!({"role": "tool", "tool_call_id": "call_f", "content": "not run: the request was declined"})
    );
}

#[test]
fn refusal_message_ends_the_turn() {
    let first = stream(&[
        chunk(json!({"role": "assistant", "refusal": "I can't "}), None),
        chunk(json!({"refusal": "help with that."}), None),
        chunk(json!({}), Some("stop")),
    ]);
    let t = FakeTransport::new(vec![Ok(first)]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut tools, "Do it");
    assert_eq!(stop, StopKind::Refusal { category: None });
    // Nothing was said, so no assistant message is kept.
    assert_eq!(conv.messages().len(), 1);
}

#[test]
fn cancel_mid_stream_keeps_text_and_drops_unfinished_calls() {
    let first = stream(&[
        chunk(json!({"content": "Hel"}), None),
        chunk(json!({"content": "lo"}), None),
        call_start(0, "call_c", "get", "{}"),
        chunk(json!({}), Some("tool_calls")),
    ]);
    let t = FakeTransport::new(vec![Ok(first), Ok(text_reply("Ok."))]);
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
    assert_eq!(
        conv.messages()[1],
        json!({"role": "assistant", "content": "Hel"})
    );
    run(&mut conv, &cfg(), &t, &mut tools, "Continue");
    let msgs = &t.bodies()[1]["messages"];
    assert_eq!(msgs[3], json!({"role": "user", "content": "Continue"}));
}

#[test]
fn cancel_after_the_finish_answers_the_calls_next_turn() {
    let first = stream(&[
        call_start(0, "call_s", "get", "{}"),
        chunk(json!({}), Some("tool_calls")),
        usage(),
    ]);
    let t = FakeTransport::new(vec![Ok(first), Ok(text_reply("Ok."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let cancel = AtomicBool::new(false);
    let stop = conv.run_turn(
        &cfg(),
        &t,
        &mut tools,
        user_content("Go", &[]),
        &cancel,
        &mut |e| {
            if matches!(e, Event::Usage { .. }) {
                cancel.store(true, Ordering::SeqCst);
            }
        },
    );
    assert_eq!(stop, StopKind::Cancelled);
    assert!(tools.calls.is_empty());
    run(&mut conv, &cfg(), &t, &mut tools, "Continue");
    let msgs = &t.bodies()[1]["messages"];
    assert_eq!(
        msgs[3],
        json!({"role": "tool", "tool_call_id": "call_s", "content": "stopped by the user"})
    );
}

#[test]
fn stream_without_finish_reason_is_an_error() {
    let t = FakeTransport::new(vec![Ok(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n".to_vec())]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut tools, "Hi");
    assert!(matches!(stop, StopKind::Error(_)));
}

#[test]
fn error_chunk_ends_the_turn() {
    let t = FakeTransport::new(vec![Ok(stream(&[
        json!({"error": {"message": "model not loaded"}}),
    ]))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, _) = run(&mut conv, &cfg(), &t, &mut tools, "Hi");
    assert_eq!(
        stop,
        StopKind::Error("The endpoint reported an error: model not loaded".into())
    );
}

#[test]
fn rate_limit_is_retried_once() {
    let t = FakeTransport::new(vec![
        Err(HttpError {
            status: Some(429),
            message: "slow down".into(),
            retry_after: Some(Duration::from_millis(10)),
        }),
        Ok(text_reply("Ok.")),
    ]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let (stop, events) = run(&mut conv, &cfg(), &t, &mut tools, "Hi");
    assert_eq!(stop, StopKind::EndTurn);
    assert!(matches!(events[0], Event::Retrying { .. }));
}

#[test]
fn key_rules() {
    let mut tools = FakeTools::default();
    // An empty key is fine for a local server only.
    let t = FakeTransport::new(vec![Ok(text_reply("Ok.")), Ok(text_reply("Ok."))]);
    for base in ["http://localhost:11434/v1", "http://127.0.0.1:1234/v1"] {
        let c = Config {
            base_url: base.into(),
            ..cfg()
        };
        let (stop, _) = run(&mut Conversation::new(), &c, &t, &mut tools, "Hi");
        assert_eq!(stop, StopKind::EndTurn, "{base}");
    }
    let none = FakeTransport::new(vec![]);
    for base in ["https://openrouter.ai/api/v1", "http://example.com/v1"] {
        let c = Config {
            base_url: base.into(),
            ..cfg()
        };
        let (stop, _) = run(&mut Conversation::new(), &c, &none, &mut tools, "Hi");
        assert_eq!(
            stop,
            StopKind::Error("No API key is set for the assistant.".into()),
            "{base}"
        );
    }
    // A key never goes to plain http elsewhere.
    let c = Config {
        base_url: "http://example.com/v1".into(),
        api_key: "sk-1".into(),
        ..cfg()
    };
    let (stop, _) = run(&mut Conversation::new(), &c, &none, &mut tools, "Hi");
    assert!(matches!(stop, StopKind::Error(m) if m.contains("only sent over https")));
    assert!(none.requests.lock().expect("lock").is_empty());
}

#[test]
fn protocol_cannot_change_mid_conversation() {
    let t = FakeTransport::new(vec![Ok(text_reply("Ok."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    run(&mut conv, &cfg(), &t, &mut tools, "Hi");
    let anthropic = Config {
        api_key: "sk".into(),
        ..Config::default()
    };
    let (stop, _) = run(&mut conv, &anthropic, &t, &mut tools, "Hi");
    assert_eq!(
        stop,
        StopKind::Error("Start a new chat to change the protocol.".into())
    );
    assert_eq!(t.bodies().len(), 1);
}

#[test]
fn images_go_as_data_urls() {
    let t = FakeTransport::new(vec![Ok(text_reply("Nice."))]);
    let mut tools = FakeTools::default();
    let mut conv = Conversation::new();
    let cancel = AtomicBool::new(false);
    conv.run_turn(
        &cfg(),
        &t,
        &mut tools,
        user_content("Look", &[("image/jpeg".into(), "QUJD".into())]),
        &cancel,
        &mut |_| {},
    );
    assert_eq!(
        t.bodies()[0]["messages"][1]["content"],
        json!([
            {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,QUJD"}},
            {"type": "text", "text": "Look"},
        ])
    );
}

#[test]
fn model_lists() {
    // Anthropic.
    let t = FakeTransport::new(vec![Ok(serde_json::to_vec(&json!({
        "data": [{"type": "model", "id": "claude-opus-5-5", "display_name": "Claude Opus 5.5"},
                 {"type": "model", "id": "claude-haiku-5-5", "display_name": "Claude Haiku 5.5"}],
        "has_more": false, "first_id": "claude-opus-5-5", "last_id": "claude-haiku-5-5"}))
    .expect("json"))]);
    let c = Config {
        api_key: "sk-ant".into(),
        ..Config::default()
    };
    assert_eq!(
        list_models(&c, &t),
        Ok(vec![
            ModelInfo {
                id: "claude-opus-5-5".into(),
                name: Some("Claude Opus 5.5".into())
            },
            ModelInfo {
                id: "claude-haiku-5-5".into(),
                name: Some("Claude Haiku 5.5".into())
            },
        ])
    );
    {
        let reqs = t.requests.lock().expect("lock");
        assert_eq!(reqs[0].method, "GET");
        assert_eq!(
            reqs[0].url,
            "https://api.anthropic.com/v1/models?limit=1000"
        );
        assert!(
            reqs[0]
                .headers
                .contains(&("x-api-key".to_string(), "sk-ant".to_string()))
        );
        assert!(
            reqs[0]
                .headers
                .iter()
                .any(|(k, _)| k == "anthropic-version")
        );
    }

    // OpenAI-compatible: Ollama (no names, no key) and OpenRouter (names).
    let t = FakeTransport::new(vec![
        Ok(br#"{"object":"list","data":[{"id":"qwen3:8b","object":"model","created":1,"owned_by":"library"}]}"#.to_vec()),
        Ok(br#"{"data":[{"id":"openai/gpt-5","name":"OpenAI: GPT-5"}]}"#.to_vec()),
        Ok(b"<html>".to_vec()),
    ]);
    assert_eq!(
        list_models(&cfg(), &t),
        Ok(vec![ModelInfo {
            id: "qwen3:8b".into(),
            name: None
        }])
    );
    let router = Config {
        base_url: "https://openrouter.ai/api/v1/".into(),
        api_key: "sk-or".into(),
        ..cfg()
    };
    assert_eq!(
        list_models(&router, &t),
        Ok(vec![ModelInfo {
            id: "openai/gpt-5".into(),
            name: Some("OpenAI: GPT-5".into())
        }])
    );
    assert!(list_models(&router, &t).is_err());
    {
        let reqs = t.requests.lock().expect("lock");
        assert_eq!(reqs[0].url, "http://localhost:11434/v1/models");
        assert!(reqs[0].headers.is_empty());
        assert_eq!(reqs[1].url, "https://openrouter.ai/api/v1/models");
        assert_eq!(
            reqs[1].headers,
            vec![("Authorization".to_string(), "Bearer sk-or".to_string())]
        );
    }

    // Key rules, and a transport without `get`.
    let none = FakeTransport::new(vec![]);
    let unsafe_cfg = Config {
        base_url: "http://example.com/v1".into(),
        api_key: "sk".into(),
        ..cfg()
    };
    assert!(list_models(&unsafe_cfg, &none).is_err());
    assert!(list_models(&Config::default(), &none).is_err());
    assert!(none.requests.lock().expect("lock").is_empty());
    assert_eq!(
        list_models(&cfg(), &PostOnly),
        Err("This transport cannot send GET requests.".into())
    );
}

#[test]
fn ollama_capability() {
    let version = || Ok(br#"{"version":"0.12.0"}"#.to_vec());
    let t = FakeTransport::new(vec![
        version(),
        Ok(br#"{"capabilities":["completion","tools"]}"#.to_vec()),
        version(),
        Ok(br#"{"capabilities":["completion","vision"]}"#.to_vec()),
        version(),
        Ok(br#"{"modelfile":"..."}"#.to_vec()),
        Err(HttpError {
            status: Some(404),
            message: "Not Found".into(),
            retry_after: None,
        }),
        version(),
        Err(HttpError {
            status: Some(404),
            message: "model not found".into(),
            retry_after: None,
        }),
    ]);
    let base = "http://localhost:11434/v1";
    assert_eq!(ollama_tools_capability(base, "qwen3", &t), Some(true));
    assert_eq!(ollama_tools_capability(base, "llava", &t), Some(false));
    assert_eq!(ollama_tools_capability(base, "old", &t), None);
    // LM Studio has no /api/version: nothing else is asked.
    assert_eq!(
        ollama_tools_capability("http://localhost:1234/v1", "x", &t),
        None
    );
    assert_eq!(ollama_tools_capability(base, "missing", &t), None);
    let reqs = t.requests.lock().expect("lock");
    assert_eq!(reqs[0].method, "GET");
    assert_eq!(reqs[0].url, "http://localhost:11434/api/version");
    assert_eq!(reqs[1].method, "POST");
    assert_eq!(reqs[1].url, "http://localhost:11434/api/show");
    assert_eq!(reqs[1].body, json!({"model": "qwen3"}));
    assert_eq!(reqs[6].url, "http://localhost:1234/api/version");
    assert_eq!(reqs.len(), 9);
}

#[test]
fn edit_requests_without_tools() {
    assert!(edit_request_without_tools(
        "Make the heart red",
        false,
        "Sure, the heart is red now."
    ));
    assert!(edit_request_without_tools(
        "please add a border",
        false,
        "Done."
    ));
    assert!(edit_request_without_tools(
        "Can you rename the layer?",
        false,
        "Renamed."
    ));
    assert!(edit_request_without_tools(
        "could you please export it",
        false,
        "Here it is."
    ));
    // Tools were used.
    assert!(!edit_request_without_tools("Make it red", true, "Done."));
    // Questions and other openings.
    assert!(!edit_request_without_tools(
        "What is a satin stitch?",
        false,
        "A satin stitch is…"
    ));
    assert!(!edit_request_without_tools(
        "Can you explain fills?",
        false,
        "Yes."
    ));
    assert!(!edit_request_without_tools(
        "Make it bigger",
        false,
        "How much bigger?"
    ));
    assert!(!edit_request_without_tools("", false, ""));
    assert!(!edit_request_without_tools(
        "Mach es rot",
        false,
        "Erledigt."
    ));
}
