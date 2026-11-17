//! The browser's model connection (#766): [`BrowserFetch`] sends the turn's requests with the
//! page's own `fetch` and reads the streamed reply as it arrives, without blocking the page.
//!
//! - The request goes from the page straight to the endpoint the user chose, with the key the
//!   user entered. So the endpoint must allow the page's origin (CORS). The Claude API does that
//!   for requests that carry [`BROWSER_ACCESS_HEADER`]; a local Ollama does it for the origins
//!   in `OLLAMA_ORIGINS`.
//! - A timer checks Stop and the request timeout every 100 ms, as the desktop's watchdog does
//!   (#580). Either one aborts the request, which closes the connection.

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use js_sys::{Array, Object, Promise, Uint8Array};
use serde_json::Value;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortController, Headers, ReadableStreamDefaultReader, RequestInit, RequestMode, Response,
};
use web_time::Instant;

use crate::HttpError;
use crate::fetch::{Events, Fetch, LocalFuture, SseDecoder, SseEvent};
use crate::idle::idle_message;

/// The header that lets a web page call the Claude API directly with the user's own key.
pub const BROWSER_ACCESS_HEADER: &str = "anthropic-dangerous-direct-browser-access";

/// How often a wait looks at Stop and the timeout.
const POLL: Duration = Duration::from_millis(100);
/// The most read of an error response.
const MAX_ERROR_CHARS: usize = 64 * 1024;

/// [`Fetch`] over the page's `fetch()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct BrowserFetch;

/// The message when the browser refused or could not send the request. A page sees no detail
/// (a CORS refusal and a server that is down look the same to it).
pub const UNREACHABLE: &str = "Could not reach the endpoint from this page. It may be down, or \
it may not allow requests from this page's address (CORS). The Claude API allows them; a local \
Ollama needs OLLAMA_ORIGINS set to this page's address.";

impl Fetch for BrowserFetch {
    fn post<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        body: &'a [u8],
        timeout: Duration,
        cancel: &'a AtomicBool,
    ) -> LocalFuture<'a, Result<Box<dyn Events + 'a>, HttpError>> {
        Box::pin(async move {
            let failed = |message: String| HttpError {
                status: None,
                message,
                retry_after: None,
            };
            let controller = AbortController::new().map_err(|_| failed(UNREACHABLE.into()))?;
            let request =
                request(headers, body, &controller).map_err(|_| failed(UNREACHABLE.into()))?;
            let window = web_sys::window().ok_or_else(|| failed("no browser window".into()))?;
            let started = Instant::now();
            let response = match wait(
                &window.fetch_with_str_and_init(url, &request),
                started,
                timeout,
                cancel,
            )
            .await
            {
                Waited::Done(Ok(r)) => r.unchecked_into::<Response>(),
                Waited::Done(Err(_)) => return Err(failed(UNREACHABLE.into())),
                Waited::Stopped => {
                    controller.abort();
                    return Err(failed("Stopped.".into()));
                }
                Waited::TimedOut => {
                    controller.abort();
                    return Err(failed(
                        "The server did not answer in time (timed out).".into(),
                    ));
                }
            };
            let status = response.status();
            if !(200..300).contains(&status) {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .ok()
                    .flatten()
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .filter(|s| s.is_finite() && *s >= 0.0)
                    .map(Duration::from_secs_f64);
                let text = match response.text() {
                    Ok(p) => match wait(&p, Instant::now(), timeout, cancel).await {
                        Waited::Done(Ok(t)) => t.as_string().unwrap_or_default(),
                        _ => String::new(),
                    },
                    Err(_) => String::new(),
                };
                controller.abort();
                let text: String = text.chars().take(MAX_ERROR_CHARS).collect();
                return Err(HttpError {
                    status: Some(status),
                    message: error_message(&text),
                    retry_after,
                });
            }
            let reader = response
                .body()
                .and_then(|b| {
                    b.get_reader()
                        .dyn_into::<ReadableStreamDefaultReader>()
                        .ok()
                })
                .ok_or_else(|| failed("The reply had no body.".into()))?;
            Ok(Box::new(BrowserEvents {
                reader,
                controller,
                decoder: SseDecoder::new(timeout),
                ready: VecDeque::new(),
                read: None,
                timeout,
                last: Instant::now(),
                cancel,
                ended: false,
            }) as Box<dyn Events + 'a>)
        })
    }

    fn sleep<'a>(&'a self, wait: Duration, cancel: &'a AtomicBool) -> LocalFuture<'a, ()> {
        Box::pin(async move {
            let until = Instant::now() + wait;
            while !cancel.load(Ordering::SeqCst) {
                let now = Instant::now();
                if now >= until {
                    break;
                }
                let _ = JsFuture::from(tick((until - now).min(POLL), &JsValue::UNDEFINED)).await;
            }
        })
    }
}

/// The request: POST, the headers (plus [`BROWSER_ACCESS_HEADER`] for the Claude Messages API),
/// the body, and the abort signal. No cookies or credentials of the page go with it.
fn request(
    headers: &[(String, String)],
    body: &[u8],
    controller: &AbortController,
) -> Result<RequestInit, JsValue> {
    let h = Headers::new()?;
    for (k, v) in headers {
        h.append(k, v)?;
    }
    if headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("anthropic-version"))
    {
        h.append(BROWSER_ACCESS_HEADER, "true")?;
    }
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_mode(RequestMode::Cors);
    init.set_credentials(web_sys::RequestCredentials::Omit);
    init.set_headers(&h);
    init.set_body(&Uint8Array::from(body));
    init.set_signal(Some(&controller.signal()));
    Ok(init)
}

/// The `error.message` of an API error body, else its first line (as the desktop's transport).
fn error_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(m) = v.pointer("/error/message").and_then(Value::as_str) {
            return m.to_string();
        }
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            return m.to_string();
        }
    }
    body.lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(300)
        .collect()
}

enum Waited {
    Done(Result<JsValue, JsValue>),
    TimedOut,
    Stopped,
}

/// Waits for `promise`, at most until `timeout` after `since`, looking at Stop every 100 ms.
async fn wait(promise: &Promise, since: Instant, timeout: Duration, cancel: &AtomicBool) -> Waited {
    // The timer's own value, so the race tells it from the promise's.
    let mark: JsValue = Object::new().into();
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Waited::Stopped;
        }
        let left = timeout.saturating_sub(since.elapsed());
        if left.is_zero() {
            return Waited::TimedOut;
        }
        let race = Promise::race(&Array::of2(promise, &tick(left.min(POLL), &mark)));
        match JsFuture::from(race).await {
            Ok(v) if v == mark => {}
            other => return Waited::Done(other),
        }
    }
}

/// A promise that resolves with `value` after `after`.
fn tick(after: Duration, value: &JsValue) -> Promise {
    let ms = i32::try_from(after.as_millis()).unwrap_or(i32::MAX);
    Promise::new(&mut |resolve, _reject| {
        let value = value.clone();
        let done = Closure::once_into_js(move || {
            let _ = resolve.call1(&JsValue::NULL, &value);
        });
        if let Some(w) = web_sys::window() {
            let _ =
                w.set_timeout_with_callback_and_timeout_and_arguments_0(done.unchecked_ref(), ms);
        }
    })
}

/// The reply's events, decoded from the body's chunks as they arrive.
struct BrowserEvents<'a> {
    reader: ReadableStreamDefaultReader,
    controller: AbortController,
    decoder: SseDecoder,
    ready: VecDeque<SseEvent>,
    /// A read still under way (a timer tick won the last race).
    read: Option<Promise>,
    timeout: Duration,
    /// When bytes last arrived.
    last: Instant,
    cancel: &'a AtomicBool,
    ended: bool,
}

impl Events for BrowserEvents<'_> {
    fn next_event(&mut self) -> LocalFuture<'_, io::Result<Option<SseEvent>>> {
        Box::pin(async move {
            loop {
                if let Some(ev) = self.ready.pop_front() {
                    return Ok(Some(ev));
                }
                if self.ended {
                    return Ok(None);
                }
                let read = self.read.get_or_insert_with(|| self.reader.read()).clone();
                match wait(&read, self.last, self.timeout, self.cancel).await {
                    Waited::Done(Ok(chunk)) => {
                        self.read = None;
                        let done = js_sys::Reflect::get(&chunk, &"done".into())
                            .ok()
                            .and_then(|d| d.as_bool())
                            .unwrap_or(true);
                        if done {
                            self.ended = true;
                            self.ready.extend(self.decoder.finish());
                            continue;
                        }
                        let value = js_sys::Reflect::get(&chunk, &"value".into())
                            .unwrap_or(JsValue::UNDEFINED);
                        let bytes = Uint8Array::new(&value).to_vec();
                        if !bytes.is_empty() {
                            self.last = Instant::now();
                        }
                        let events = self.decoder.push(&bytes)?;
                        self.ready.extend(events);
                    }
                    Waited::Done(Err(_)) => {
                        self.read = None;
                        return Err(io::Error::other("the connection closed"));
                    }
                    Waited::Stopped => {
                        self.controller.abort();
                        return Err(io::Error::other("stopped"));
                    }
                    Waited::TimedOut => {
                        self.controller.abort();
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            idle_message(self.timeout),
                        ));
                    }
                }
            }
        })
    }
}

impl Drop for BrowserEvents<'_> {
    /// A reply left unread (Stop, an error) closes its connection.
    fn drop(&mut self) {
        self.controller.abort();
    }
}
