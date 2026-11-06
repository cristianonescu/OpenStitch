//! #432: picking a model for a local endpoint, against a mock Ollama and a mock LM Studio on a
//! real 127.0.0.1 socket through the real `ureq` transport. No model server runs.

use openstitch_assistant::{Config, Protocol, UreqTransport, pick_model};
use openstitch_testutil::mock_http::{MockHttp, Seen};
use serde_json::{Value, json};

/// An Ollama with these `(model, capabilities)`.
fn ollama(models: &'static [(&'static str, &'static [&'static str])]) -> MockHttp {
    MockHttp::start(move |r: &Seen| match (r.method.as_str(), r.path.as_str()) {
        ("GET", "/v1/models") => {
            let data: Vec<Value> = models
                .iter()
                .map(|(id, _)| json!({ "id": id, "object": "model" }))
                .collect();
            (200, json!({ "object": "list", "data": data }).to_string())
        }
        ("GET", "/api/version") => (200, json!({ "version": "0.40.14" }).to_string()),
        ("POST", "/api/show") => {
            let asked: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
            match models.iter().find(|(id, _)| asked["model"] == json!(id)) {
                Some((_, caps)) => (200, json!({ "capabilities": caps }).to_string()),
                None => (404, json!({ "error": "model not found" }).to_string()),
            }
        }
        _ => (404, "{}".into()),
    })
}

fn config(base: String) -> Config {
    Config {
        base_url: base,
        protocol: Protocol::OpenAi,
        model: "claude-opus-5-5".into(),
        ..Config::default()
    }
}

#[test]
fn ollama_preset_takes_the_first_model_with_tools() {
    let server = ollama(&[
        ("llava:7b", &["completion", "vision"]),
        ("qwen3:8b", &["completion", "tools", "thinking"]),
        ("gpt-oss:20b", &["completion", "tools"]),
    ]);
    let cfg = config(format!("{}/v1", server.origin()));
    assert_eq!(pick_model(&cfg, &UreqTransport).as_deref(), Ok("qwen3:8b"));
    let shows: Vec<String> = server
        .seen()
        .into_iter()
        .filter(|s| s.path == "/api/show")
        .map(|s| s.body)
        .collect();
    assert_eq!(shows.len(), 2, "stops at the first model with tools");
}

#[test]
fn ollama_without_a_tool_model_says_so() {
    let server = ollama(&[("llava:7b", &["completion", "vision"])]);
    let cfg = config(format!("{}/v1", server.origin()));
    let e = pick_model(&cfg, &UreqTransport).expect_err("no tools");
    assert!(e.contains("tools"), "{e}");
}

#[test]
fn lm_studio_takes_its_first_model() {
    let server = MockHttp::start(|r: &Seen| match r.path.as_str() {
        "/v1/models" => (
            200,
            json!({ "object": "list", "data": [
                { "id": "qwen2.5-7b-instruct", "object": "model" },
                { "id": "text-embedding-nomic", "object": "model" } ] })
            .to_string(),
        ),
        // LM Studio has no /api/version.
        _ => (404, json!({ "error": "Unexpected endpoint" }).to_string()),
    });
    let cfg = config(format!("{}/v1", server.origin()));
    assert_eq!(
        pick_model(&cfg, &UreqTransport).as_deref(),
        Ok("qwen2.5-7b-instruct")
    );
    assert!(
        !server.seen().iter().any(|s| s.path == "/api/show"),
        "nothing asked of a server that is not Ollama"
    );
}

#[test]
fn an_empty_list_or_no_server_is_an_error() {
    let server = MockHttp::start(|_| (200, json!({ "object": "list", "data": [] }).to_string()));
    let e =
        pick_model(&config(format!("{}/v1", server.origin())), &UreqTransport).expect_err("empty");
    assert!(e.contains("lists no models"), "{e}");
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        l.local_addr().expect("addr").port()
    };
    assert!(
        pick_model(
            &config(format!("http://127.0.0.1:{port}/v1")),
            &UreqTransport
        )
        .is_err()
    );
}
