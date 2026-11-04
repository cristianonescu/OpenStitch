//! The optional image generator (B17 A4): an OpenAI-compatible `POST {base_url}/images/generations`
//! endpoint, or a local server that speaks it. [`WithImages`] adds its `generate_image` tool to
//! the editor's tools; the picture goes into the scratch folder through `scratch_write`, ready for
//! `digitize.*`. Without a generator the tool says so and points the model at the vector route.
//!
//! Format source (read 2026-11-04): OpenAI API reference, "Create image"
//! (`/v1/images/generations`: `model`, `prompt`, `n`, `size`, `response_format: "b64_json"`; the
//! reply's `data[].b64_json`, or `data[].url` when a server ignores `response_format`).

use std::io::Read;
use std::time::Duration;

use serde_json::{Value, json};

use crate::openai::auth_headers;
use crate::{Config, ToolResult, Tools, Transport, describe, key_safe_for};

/// The tool's name.
pub const GENERATE_IMAGE: &str = "generate_image";
/// The largest picture read back, in bytes.
const MAX_IMAGE_BYTES: u64 = 20 << 20;

/// What the model is told when no generator is set up.
pub const NO_GENERATOR: &str = "No image generator is set up (Assistant settings ▸ Image \
generator), so no picture can be generated. Take the vector route instead: write the picture as \
a simple flat-colour SVG with scratch_write and import it with import.svg. Tell the user that you \
drew it as vector shapes because no image generator is set up.";

/// An image endpoint: the base URL ends in `/v1` (`https://api.openai.com/v1`,
/// `http://localhost:8080/v1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub request_timeout: Duration,
}

/// Generates one picture; Ok = its PNG or JPEG bytes.
pub fn generate_image(
    transport: &dyn Transport,
    cfg: &ImageConfig,
    prompt: &str,
) -> Result<Vec<u8>, String> {
    if !cfg.api_key.is_empty() && !key_safe_for(&cfg.base_url) {
        return Err(format!(
            "the image address {} is neither https nor this computer, so the key is not sent",
            cfg.base_url
        ));
    }
    let url = format!("{}/images/generations", cfg.base_url.trim_end_matches('/'));
    let mut body =
        json!({"prompt": prompt, "n": 1, "size": "1024x1024", "response_format": "b64_json"});
    if !cfg.model.is_empty() {
        body["model"] = json!(cfg.model);
    }
    let auth = Config {
        api_key: cfg.api_key.clone(),
        ..Config::default()
    };
    let mut headers = auth_headers(&auth);
    headers.push(("content-type".into(), "application/json".into()));
    let reader = transport
        .post(
            &url,
            &headers,
            body.to_string().as_bytes(),
            cfg.request_timeout,
        )
        .map_err(|e| describe(&e))?;
    let reply: Value = serde_json::from_reader(reader.take(MAX_IMAGE_BYTES * 2))
        .map_err(|e| format!("the image endpoint's reply is not JSON: {e}"))?;
    let item = reply.pointer("/data/0").ok_or_else(|| {
        let msg = reply.pointer("/error/message").and_then(Value::as_str);
        msg.map_or(
            "the image endpoint returned no picture".to_string(),
            str::to_string,
        )
    })?;
    let bytes = if let Some(b64) = item.get("b64_json").and_then(Value::as_str) {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|_| "the picture's base64 is not valid".to_string())?
    } else if let Some(link) = item.get("url").and_then(Value::as_str) {
        let mut out = Vec::new();
        transport
            .get(link, &[], cfg.request_timeout)
            .map_err(|e| describe(&e))?
            .take(MAX_IMAGE_BYTES + 1)
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        out
    } else {
        return Err("the image endpoint returned neither b64_json nor url".into());
    };
    match bytes.get(..4) {
        _ if bytes.len() as u64 > MAX_IMAGE_BYTES => Err("the picture is too large".into()),
        Some(b"\x89PNG") => Ok(bytes),
        Some([0xFF, 0xD8, 0xFF, _]) => Ok(bytes),
        _ => Err("the image endpoint returned something that is not a PNG or JPEG".into()),
    }
}

/// The editor's tools plus `generate_image`.
pub struct WithImages<'a, T: Tools> {
    pub inner: T,
    pub image: Option<ImageConfig>,
    pub transport: &'a dyn Transport,
}

impl<T: Tools> Tools for WithImages<'_, T> {
    fn definitions(&self) -> Vec<Value> {
        let mut defs = self.inner.definitions();
        defs.push(json!({
            "name": GENERATE_IMAGE,
            "description": "Generate a picture from a description with the user's image \
                generator and save it in the scratch folder; returns its path, ready for \
                digitize.autoPunch (compatMode openStitch). Ask for flat colours, bold shapes \
                and a plain background: that is what stitches well.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "prompt": {"type": "string", "minLength": 1, "description": "What to draw"},
                    "name": {"type": "string", "description": "File name ending in .png (default generated.png)"},
                },
                "required": ["prompt"],
                "additionalProperties": false,
            },
        }));
        defs
    }

    fn call(&mut self, name: &str, input: &Value) -> ToolResult {
        if name != GENERATE_IMAGE {
            return self.inner.call(name, input);
        }
        let fail = |text: &str| ToolResult {
            content: vec![json!({"type": "text", "text": text})],
            is_error: true,
        };
        let Some(cfg) = &self.image else {
            return fail(NO_GENERATOR);
        };
        let Some(prompt) = input
            .get("prompt")
            .and_then(Value::as_str)
            .filter(|p| !p.trim().is_empty())
        else {
            return fail("generate_image: 'prompt' is required");
        };
        let file = input
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("generated.png");
        let bytes = match generate_image(self.transport, cfg, prompt) {
            Ok(b) => b,
            Err(e) => return fail(&format!("The image generator failed: {e}. {NO_GENERATOR}")),
        };
        use base64::Engine;
        let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
        self.inner
            .call("scratch_write", &json!({"name": file, "base64": data}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HttpError;
    use std::sync::Mutex;

    struct Fake {
        reply: String,
        sent: Mutex<Vec<(String, String)>>,
    }

    impl Transport for Fake {
        fn post(
            &self,
            url: &str,
            _: &[(String, String)],
            body: &[u8],
            _: Duration,
        ) -> Result<Box<dyn Read + Send>, HttpError> {
            self.sent
                .lock()
                .expect("lock")
                .push((url.into(), String::from_utf8_lossy(body).into()));
            Ok(Box::new(std::io::Cursor::new(
                self.reply.clone().into_bytes(),
            )))
        }
    }

    struct Scratch(Vec<Value>);

    impl Tools for Scratch {
        fn definitions(&self) -> Vec<Value> {
            vec![json!({"name": "scratch_write"})]
        }
        fn call(&mut self, name: &str, input: &Value) -> ToolResult {
            self.0.push(json!({"name": name, "input": input}));
            ToolResult {
                content: vec![json!({"type": "text", "text": "{\"path\":\"/s/generated.png\"}"})],
                is_error: false,
            }
        }
    }

    fn config() -> ImageConfig {
        ImageConfig {
            base_url: "http://localhost:8080/v1".into(),
            api_key: String::new(),
            model: "flux".into(),
            request_timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn without_a_generator_the_tool_offers_the_vector_route() {
        let t = Fake {
            reply: String::new(),
            sent: Mutex::new(Vec::new()),
        };
        let mut tools = WithImages {
            inner: Scratch(Vec::new()),
            image: None,
            transport: &t,
        };
        assert!(
            tools
                .definitions()
                .iter()
                .any(|d| d["name"] == GENERATE_IMAGE)
        );
        let r = tools.call(GENERATE_IMAGE, &json!({"prompt": "a fox"}));
        assert!(r.is_error);
        assert!(
            r.content[0]["text"]
                .as_str()
                .is_some_and(|t| t.contains("import.svg"))
        );
        assert!(t.sent.lock().expect("lock").is_empty());
    }

    #[test]
    fn a_generated_picture_goes_into_the_scratch_folder() {
        use base64::Engine;
        let png = b"\x89PNG\r\n\x1a\nrest";
        let b64 = base64::engine::general_purpose::STANDARD.encode(png);
        let t = Fake {
            reply: json!({"data": [{"b64_json": b64}]}).to_string(),
            sent: Mutex::new(Vec::new()),
        };
        let mut tools = WithImages {
            inner: Scratch(Vec::new()),
            image: Some(config()),
            transport: &t,
        };
        let r = tools.call(GENERATE_IMAGE, &json!({"prompt": "a fox"}));
        assert!(!r.is_error, "{r:?}");
        let sent = t.sent.lock().expect("lock");
        assert_eq!(sent[0].0, "http://localhost:8080/v1/images/generations");
        let body: Value = serde_json::from_str(&sent[0].1).expect("json");
        assert_eq!(
            (body["model"].as_str(), body["response_format"].as_str()),
            (Some("flux"), Some("b64_json"))
        );
        assert_eq!(tools.inner.0[0]["input"]["base64"], json!(b64));
    }

    #[test]
    fn not_a_picture_and_unsafe_addresses_are_refused() {
        let t = Fake {
            reply: json!({"data": [{"b64_json": "aGVsbG8="}]}).to_string(),
            sent: Mutex::new(Vec::new()),
        };
        assert!(generate_image(&t, &config(), "x").is_err());
        let mut c = config();
        c.base_url = "http://images.example.com/v1".into();
        c.api_key = "k".into();
        assert!(
            generate_image(&t, &c, "x")
                .expect_err("refused")
                .contains("key is not sent")
        );
    }
}
