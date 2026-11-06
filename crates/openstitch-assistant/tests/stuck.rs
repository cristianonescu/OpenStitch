//! A server that never finishes its reply (#580): the request timeout ends the turn, whether
//! the server sends nothing at all or keeps the connection busy with keep-alives that carry no
//! reply. Fake providers only: no network, no model.

use std::io::{self, Read};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use openstitch_assistant::{
    Config, Conversation, Event, HttpError, Protocol, StopKind, ToolResult, Tools, Transport,
    user_content,
};
use serde_json::{Value, json};

/// The timeout these tests run with; the editor's default is 300 s.
const TIMEOUT: Duration = Duration::from_millis(300);
/// How long a stuck turn may take to end: the timeout plus generous slack for a busy CI host.
const ENDS_WITHIN: Duration = Duration::from_secs(10);

/// Answers each POST with the next scripted body reader.
struct Provider(Mutex<Vec<Box<dyn Read + Send>>>);

impl Provider {
    fn new(bodies: Vec<Box<dyn Read + Send>>) -> Self {
        Provider(Mutex::new(bodies.into_iter().rev().collect()))
    }
}

impl Transport for Provider {
    fn post(
        &self,
        _url: &str,
        _headers: &[(String, String)],
        _body: &[u8],
        _timeout: Duration,
    ) -> Result<Box<dyn Read + Send>, HttpError> {
        Ok(self
            .0
            .lock()
            .expect("lock")
            .pop()
            .expect("a scripted reply for every request"))
    }
}

/// Sent the headers, then never sends a byte (until the test ends and drops the sender).
struct NeverAnswers(Receiver<()>);

impl Read for NeverAnswers {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        let _ = self.0.recv();
        Ok(0)
    }
}

fn never_answers() -> (Sender<()>, Box<dyn Read + Send>) {
    let (tx, rx) = channel();
    (tx, Box::new(NeverAnswers(rx)))
}

/// Sends `lines` in turn, one every `every`, then repeats the last one forever.
struct Trickle {
    lines: Vec<String>,
    at: usize,
    every: Duration,
}

impl Read for Trickle {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        std::thread::sleep(self.every);
        let line = &self.lines[self.at.min(self.lines.len() - 1)];
        self.at += 1;
        let n = out.len().min(line.len());
        out[..n].copy_from_slice(&line.as_bytes()[..n]);
        Ok(n)
    }
}

fn trickle(lines: &[String], every: Duration) -> Box<dyn Read + Send> {
    Box::new(Trickle {
        lines: lines.to_vec(),
        at: 0,
        every,
    })
}

struct NoTools;

impl Tools for NoTools {
    fn definitions(&self) -> Vec<Value> {
        Vec::new()
    }
    fn call(&mut self, _: &str, _: &Value) -> ToolResult {
        unreachable!("no tools in these turns")
    }
}

fn openai() -> Config {
    Config {
        protocol: Protocol::OpenAi,
        base_url: "http://localhost:11434/v1".into(),
        model: "qwen3:4b".into(),
        request_timeout: TIMEOUT,
        ..Config::default()
    }
}

fn claude() -> Config {
    Config {
        api_key: "sk-test".into(),
        request_timeout: TIMEOUT,
        ..Config::default()
    }
}

fn chunk(delta: Value, finish: Option<&str>) -> String {
    let c = json!({"id": "c", "object": "chat.completion.chunk", "created": 1, "model": "m",
                   "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]});
    format!("data: {c}\n\n")
}

fn turn(
    cfg: &Config,
    provider: &Provider,
    cancel: &AtomicBool,
) -> (StopKind, Vec<Event>, Duration) {
    let start = Instant::now();
    let mut events = Vec::new();
    let stop = Conversation::new().run_turn(
        cfg,
        provider,
        &mut NoTools,
        user_content("add a 30 mm circle", &[]),
        cancel,
        &mut |e| events.push(e),
    );
    (stop, events, start.elapsed())
}

fn assert_timed_out(stop: &StopKind, took: Duration) {
    match stop {
        StopKind::Error(m) => assert!(m.contains("sent no reply for 300 ms"), "{m}"),
        other => panic!("expected the timeout error, got {other:?}"),
    }
    assert!(took >= TIMEOUT, "ended before the timeout: {took:?}");
    assert!(took < ENDS_WITHIN, "took {took:?}");
}

#[test]
fn a_provider_that_never_answers_times_out() {
    for cfg in [openai(), claude()] {
        let (_keep, body) = never_answers();
        let p = Provider::new(vec![body]);
        let (stop, events, took) = turn(&cfg, &p, &AtomicBool::new(false));
        assert_timed_out(&stop, took);
        assert!(events.is_empty(), "{events:?}");
    }
}

#[test]
fn keep_alive_chunks_without_a_reply_time_out() {
    // Empty deltas every 20 ms: bytes keep coming, so no read ever waits for the timeout.
    let empty = chunk(json!({}), None);
    let p = Provider::new(vec![trickle(&[empty], Duration::from_millis(20))]);
    let (stop, _, took) = turn(&openai(), &p, &AtomicBool::new(false));
    assert_timed_out(&stop, took);
}

#[test]
fn comments_and_pings_without_a_reply_time_out() {
    let p = Provider::new(vec![trickle(
        &[": keep-alive\n\n".to_string()],
        Duration::from_millis(20),
    )]);
    let (stop, _, took) = turn(&openai(), &p, &AtomicBool::new(false));
    assert_timed_out(&stop, took);

    let ping = "event: ping\ndata: {\"type\": \"ping\"}\n\n".to_string();
    let p = Provider::new(vec![trickle(&[ping], Duration::from_millis(20))]);
    let (stop, _, took) = turn(&claude(), &p, &AtomicBool::new(false));
    assert_timed_out(&stop, took);
}

#[test]
fn a_model_that_keeps_reasoning_is_not_cut_off_and_shows_thinking() {
    // 20 reasoning pieces 50 ms apart: 1 s in all, over three times the timeout, but never
    // 300 ms without a piece. Then the answer.
    let mut lines: Vec<String> = (0..20)
        .map(|i| chunk(json!({"reasoning": format!("step {i}. ")}), None))
        .collect();
    lines.push(chunk(json!({"content": "Done."}), None));
    lines.push(chunk(json!({}), Some("stop")));
    lines.push("data: [DONE]\n\n".to_string());
    let p = Provider::new(vec![trickle(&lines, Duration::from_millis(50))]);
    let (stop, events, took) = turn(&openai(), &p, &AtomicBool::new(false));
    assert_eq!(stop, StopKind::EndTurn);
    assert!(took > TIMEOUT * 2, "{took:?}");
    let thinking: String = events
        .iter()
        .filter_map(|e| match e {
            Event::Thinking(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert!(thinking.starts_with("step 0. step 1.") && thinking.contains("step 19."));
    assert!(events.contains(&Event::Text("Done.".into())));
}

#[test]
fn stop_ends_a_silent_turn_at_once() {
    let (_keep, body) = never_answers();
    let p = Provider::new(vec![body]);
    let cfg = Config {
        request_timeout: Duration::from_secs(60),
        ..openai()
    };
    let cancel = AtomicBool::new(false);
    let (stop, took) = std::thread::scope(|s| {
        s.spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            cancel.store(true, Ordering::SeqCst);
        });
        let (stop, _, took) = turn(&cfg, &p, &cancel);
        (stop, took)
    });
    assert_eq!(stop, StopKind::Cancelled);
    assert!(
        took < ENDS_WITHIN,
        "Stop waited for the 60 s timeout: {took:?}"
    );
}
