//! The tool loop where every step waits (#766), as in a browser page: the request, every piece
//! of the stream and every tool call are pending once before they are ready. The turn must give
//! the same history and events as the desktop's blocking `run_turn` on the same script.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::io::{self, Read};
use std::pin::Pin;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use openstitch_assistant::fetch::{
    AsyncTools, Events, Fetch, LocalFuture, SseDecoder, SseEvent, block_on,
};
use openstitch_assistant::{
    Config, Conversation, Event, HttpError, Protocol, StopKind, ToolResult, Tools, Transport,
    user_content,
};
use serde_json::{Value, json};

/// Pending once, then ready: a promise that has not settled on the first look.
struct Yield(bool);

impl Future for Yield {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// The scripted replies, as the blocking transport.
struct Script(Mutex<VecDeque<Vec<u8>>>);

impl Script {
    fn new(replies: &[Vec<u8>]) -> Self {
        Script(Mutex::new(replies.iter().cloned().collect()))
    }
    fn next(&self) -> Vec<u8> {
        self.0
            .lock()
            .expect("lock")
            .pop_front()
            .expect("a reply for every request")
    }
}

impl Transport for Script {
    fn post(
        &self,
        _: &str,
        _: &[(String, String)],
        _: &[u8],
        _: Duration,
    ) -> Result<Box<dyn Read + Send>, HttpError> {
        Ok(Box::new(io::Cursor::new(self.next())))
    }
}

/// The scripted replies, as a page's fetch: everything waits, and the body comes in pieces.
struct Waiting(Script);

impl Fetch for Waiting {
    fn post<'a>(
        &'a self,
        _: &'a str,
        _: &'a [(String, String)],
        _: &'a [u8],
        timeout: Duration,
        _: &'a AtomicBool,
    ) -> LocalFuture<'a, Result<Box<dyn Events + 'a>, HttpError>> {
        Box::pin(async move {
            Yield(false).await;
            let body = self.0.next();
            Ok(Box::new(Pieces {
                chunks: body.chunks(7).map(<[u8]>::to_vec).collect(),
                decoder: SseDecoder::new(timeout),
                ready: VecDeque::new(),
            }) as Box<dyn Events>)
        })
    }

    fn sleep<'a>(&'a self, _: Duration, _: &'a AtomicBool) -> LocalFuture<'a, ()> {
        Box::pin(Yield(false))
    }
}

struct Pieces {
    chunks: VecDeque<Vec<u8>>,
    decoder: SseDecoder,
    ready: VecDeque<SseEvent>,
}

impl Events for Pieces {
    fn next_event(&mut self) -> LocalFuture<'_, io::Result<Option<SseEvent>>> {
        Box::pin(async move {
            loop {
                if let Some(ev) = self.ready.pop_front() {
                    return Ok(Some(ev));
                }
                Yield(false).await;
                match self.chunks.pop_front() {
                    Some(c) => {
                        let events = self.decoder.push(&c)?;
                        self.ready.extend(events);
                    }
                    None => return Ok(self.decoder.finish()),
                }
            }
        })
    }
}

fn definitions() -> Vec<Value> {
    vec![json!({"name": "get", "description": "Reads a value.",
                "inputSchema": {"type": "object", "properties": {"key": {"type": "string"}}}})]
}

fn answer(name: &str) -> ToolResult {
    ToolResult {
        content: vec![json!({"type": "text", "text": format!("{name} ok")})],
        is_error: false,
    }
}

struct Now(Vec<String>);

impl Tools for Now {
    fn definitions(&self) -> Vec<Value> {
        definitions()
    }
    fn call(&mut self, name: &str, _: &Value) -> ToolResult {
        self.0.push(name.into());
        answer(name)
    }
}

/// Tools whose calls wait (an approval card), and may set Stop while they wait.
struct Later {
    calls: Vec<String>,
    stop: Option<&'static AtomicBool>,
}

impl AsyncTools for Later {
    fn definitions(&self) -> Vec<Value> {
        definitions()
    }
    fn call<'a>(&'a mut self, name: &'a str, _: &'a Value) -> LocalFuture<'a, ToolResult> {
        Box::pin(async move {
            Yield(false).await;
            self.calls.push(name.into());
            if let Some(stop) = self.stop {
                stop.store(true, Ordering::SeqCst);
            }
            answer(name)
        })
    }
}

fn anthropic_sse(events: &[Value]) -> Vec<u8> {
    let mut out = String::new();
    for e in events {
        out.push_str(&format!(
            "event: {}\ndata: {e}\n\n",
            e["type"].as_str().expect("type")
        ));
    }
    out.into_bytes()
}

fn anthropic_script() -> Vec<Vec<u8>> {
    let start = json!({"type": "message_start", "message": {"id": "m", "type": "message",
        "role": "assistant", "content": [], "usage": {"input_tokens": 3, "output_tokens": 1}}});
    let stop = |reason: &str| json!({"type": "message_delta", "delta": {"stop_reason": reason}, "usage": {"output_tokens": 9}});
    vec![
        anthropic_sse(&[
            start.clone(),
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Let me look."}}),
            json!({"type": "content_block_stop", "index": 0}),
            json!({"type": "content_block_start", "index": 1, "content_block": {"type": "tool_use", "id": "t1", "name": "get", "input": {}}}),
            json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": "{\"key\":"}}),
            json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": "\"a\"}"}}),
            json!({"type": "content_block_stop", "index": 1}),
            stop("tool_use"),
            json!({"type": "message_stop"}),
        ]),
        anthropic_sse(&[
            start,
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Done."}}),
            json!({"type": "content_block_stop", "index": 0}),
            stop("end_turn"),
            json!({"type": "message_stop"}),
        ]),
    ]
}

fn openai_script() -> Vec<Vec<u8>> {
    let chunk = |delta: Value, finish: Value| {
        format!(
            "data: {}\n\n",
            json!({"choices": [{"index": 0, "delta": delta, "finish_reason": finish}]})
        )
    };
    vec![
        [
            chunk(json!({"content": "Let me look."}), Value::Null),
            chunk(
                json!({"tool_calls": [{"index": 0, "id": "c1", "type": "function",
                       "function": {"name": "get", "arguments": "{\"key\":\"a\"}"}}]}),
                Value::Null,
            ),
            chunk(json!({}), json!("tool_calls")),
            "data: [DONE]\n\n".to_string(),
        ]
        .concat()
        .into_bytes(),
        [
            chunk(json!({"content": "Done."}), Value::Null),
            chunk(json!({}), json!("stop")),
            "data: [DONE]\n\n".to_string(),
        ]
        .concat()
        .into_bytes(),
    ]
}

fn config(protocol: Protocol) -> Config {
    Config {
        protocol,
        base_url: match protocol {
            Protocol::Anthropic => "https://api.example.com".into(),
            Protocol::OpenAi => "https://api.example.com/v1".into(),
        },
        api_key: "test-key".into(),
        ..Config::default()
    }
}

/// The blocking turn and the waiting one, on the same script.
fn both(protocol: Protocol, script: Vec<Vec<u8>>) {
    let cfg = config(protocol);
    let stop = AtomicBool::new(false);
    let mut sync_events = Vec::new();
    let mut sync_chat = Conversation::new();
    let mut now = Now(Vec::new());
    let sync_end = sync_chat.run_turn(
        &cfg,
        &Script::new(&script),
        &mut now,
        user_content("hi", &[]),
        &stop,
        &mut |e| sync_events.push(e),
    );

    let async_events = RefCell::new(Vec::<Event>::new());
    let mut async_chat = Conversation::new();
    let mut later = Later {
        calls: Vec::new(),
        stop: None,
    };
    let async_end = block_on(async_chat.run_turn_async(
        &cfg,
        &Waiting(Script::new(&script)),
        &mut later,
        user_content("hi", &[]),
        &stop,
        &mut |e| async_events.borrow_mut().push(e),
    ));

    assert_eq!(sync_end, StopKind::EndTurn);
    assert_eq!(async_end, sync_end);
    assert_eq!(async_chat.messages(), sync_chat.messages());
    assert_eq!(*async_events.borrow(), sync_events);
    assert_eq!(later.calls, now.0);
    assert_eq!(later.calls, vec!["get".to_string()]);
}

#[test]
fn a_waiting_anthropic_turn_matches_the_blocking_one() {
    both(Protocol::Anthropic, anthropic_script());
}

#[test]
fn a_waiting_openai_turn_matches_the_blocking_one() {
    both(Protocol::OpenAi, openai_script());
}

/// Stop pressed while a tool call waits (on its approval card) ends the turn after that call;
/// the call's result is in the history, and no further request is sent.
#[test]
fn stop_during_a_waiting_tool_call_cancels_the_turn() {
    static STOP: AtomicBool = AtomicBool::new(false);
    let mut chat = Conversation::new();
    let mut later = Later {
        calls: Vec::new(),
        stop: Some(&STOP),
    };
    let end = block_on(chat.run_turn_async(
        &config(Protocol::Anthropic),
        &Waiting(Script::new(&anthropic_script())),
        &mut later,
        user_content("hi", &[]),
        &STOP,
        &mut |_| {},
    ));
    assert_eq!(end, StopKind::Cancelled);
    assert_eq!(later.calls, vec!["get".to_string()]);
    // The user message, the assistant's tool call and its result.
    assert_eq!(chat.messages().len(), 3);
    assert_eq!(chat.messages()[2]["content"][0]["tool_use_id"], json!("t1"));
}
