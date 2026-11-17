//! What a turn needs from the outside world, so one tool loop serves the desktop and the browser
//! (#766).
//!
//! The turn loops ([`crate::Conversation::run_turn_async`]) are `async` over three small traits:
//! [`Fetch`] sends a request and waits, [`Events`] reads the reply's Server-Sent Events, and
//! [`AsyncTools`] runs a tool call. A browser page has one thread, so there each of them waits
//! for a JavaScript promise (`browser.rs`) and a tool call may wait for the user's answer on an
//! approval card.
//!
//! The desktop keeps its blocking [`Transport`] and [`Tools`]: [`Blocking`] and [`SyncTools`]
//! wrap them in futures that are ready at once, and [`block_on`] runs the turn to its end on the
//! calling thread. Nothing in a desktop turn ever waits on a waker.

use std::future::Future;
use std::io::{self, BufReader};
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use serde_json::Value;
use web_time::Instant;

use crate::idle::Watchdog;
use crate::sse::SseReader;
use crate::{HttpError, ToolResult, Tools, Transport};

pub use crate::sse::{SseDecoder, SseEvent};

/// A boxed future that stays on its thread (a browser page has only one).
pub type LocalFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// The events of one streamed reply.
pub trait Events {
    /// The next event, or `None` at the end of the stream. The request timeout ends it with
    /// [`io::ErrorKind::TimedOut`]; Stop or a broken connection ends it with another error.
    fn next_event(&mut self) -> LocalFuture<'_, io::Result<Option<SseEvent>>>;
}

/// Sends the model requests.
pub trait Fetch {
    /// POST; Ok = the event stream of a 2xx reply. `timeout` bounds the wait for the response
    /// headers and then every quiet spell of the stream; `cancel` (Stop) ends either soon.
    fn post<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        body: &'a [u8],
        timeout: Duration,
        cancel: &'a AtomicBool,
    ) -> LocalFuture<'a, Result<Box<dyn Events + 'a>, HttpError>>;

    /// Waits `wait`, or less once `cancel` is set (the pause before the one retry).
    fn sleep<'a>(&'a self, wait: Duration, cancel: &'a AtomicBool) -> LocalFuture<'a, ()>;
}

/// The tools the model may call, where a call may have to wait (for the user's approval).
pub trait AsyncTools {
    /// MCP-style definitions: {name, description, inputSchema, ...}, in a fixed order.
    fn definitions(&self) -> Vec<Value>;
    fn call<'a>(&'a mut self, name: &'a str, input: &'a Value) -> LocalFuture<'a, ToolResult>;
}

/// A blocking [`Transport`] as a [`Fetch`]: every future is ready when it is first polled.
pub struct Blocking<'t>(pub &'t dyn Transport);

impl Fetch for Blocking<'_> {
    fn post<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        body: &'a [u8],
        timeout: Duration,
        cancel: &'a AtomicBool,
    ) -> LocalFuture<'a, Result<Box<dyn Events + 'a>, HttpError>> {
        let reply = self.0.post(url, headers, body, timeout).map(|reader| {
            // The stream is read on a helper thread that gives up after the timeout or Stop
            // (#580); the reader gives up on a stream of comments alone.
            let sse = SseReader::with_idle(
                BufReader::new(Watchdog::new(reader, timeout, Some(cancel))),
                timeout,
            );
            Box::new(BlockingEvents(sse)) as Box<dyn Events + 'a>
        });
        Box::pin(std::future::ready(reply))
    }

    fn sleep<'a>(&'a self, wait: Duration, cancel: &'a AtomicBool) -> LocalFuture<'a, ()> {
        let until = Instant::now() + wait;
        loop {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            let now = Instant::now();
            if now >= until {
                break;
            }
            std::thread::sleep((until - now).min(Duration::from_millis(50)));
        }
        Box::pin(std::future::ready(()))
    }
}

struct BlockingEvents<'a>(SseReader<BufReader<Watchdog<'a>>>);

impl Events for BlockingEvents<'_> {
    fn next_event(&mut self) -> LocalFuture<'_, io::Result<Option<SseEvent>>> {
        Box::pin(std::future::ready(self.0.next_event()))
    }
}

/// Blocking [`Tools`] as [`AsyncTools`].
pub struct SyncTools<'t>(pub &'t mut dyn Tools);

impl AsyncTools for SyncTools<'_> {
    fn definitions(&self) -> Vec<Value> {
        self.0.definitions()
    }

    fn call<'a>(&'a mut self, name: &'a str, input: &'a Value) -> LocalFuture<'a, ToolResult> {
        Box::pin(std::future::ready(self.0.call(name, input)))
    }
}

/// Runs a future to its end on this thread. For the desktop's turns, whose every step blocks
/// and is ready when polled; a step that is not ready is polled again after a yield.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(out) = future.as_mut().poll(&mut cx) {
            return out;
        }
        std::thread::yield_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A future that is pending once before it is ready, as a browser promise is.
    struct Later(bool);

    impl Future for Later {
        type Output = u8;
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u8> {
            if self.0 {
                Poll::Ready(7)
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    #[test]
    fn block_on_runs_ready_and_pending_futures() {
        assert_eq!(block_on(async { 3 }), 3);
        assert_eq!(block_on(Later(false)), 7);
    }

    #[test]
    fn a_cancelled_sleep_ends_at_once() {
        struct Never;
        impl Transport for Never {
            fn post(
                &self,
                _: &str,
                _: &[(String, String)],
                _: &[u8],
                _: Duration,
            ) -> Result<Box<dyn std::io::Read + Send>, HttpError> {
                unreachable!()
            }
        }
        let stop = AtomicBool::new(true);
        let t = Instant::now();
        block_on(Blocking(&Never).sleep(Duration::from_secs(5), &stop));
        assert!(t.elapsed() < Duration::from_secs(1));
    }
}
