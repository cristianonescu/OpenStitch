//! The request timeout for a streamed reply, kept by this crate rather than the transport
//! (#580).
//!
//! A transport's own read timeout only fires when a single read waits too long, so a server that
//! keeps the connection busy with keep-alive bytes, SSE comments or empty chunks, but never
//! sends any of the reply, was never cut off. Two checks close that:
//!
//! - [`Watchdog`] reads the transport's stream on its own thread and ends the read with
//!   [`io::ErrorKind::TimedOut`] once no bytes at all came for the timeout, whatever the
//!   transport does. It also ends the read soon after Stop.
//! - [`Progress`] is the stream readers' clock for *useful* data: text, reasoning, tool-call
//!   pieces, the finish. Anything else (pings, comments, empty deltas, usage) does not reset it,
//!   so a stream with no reply in it ends after the timeout too.

use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How often a waiting read looks at the Stop flag.
const POLL: Duration = Duration::from_millis(100);
/// The size of each read from the transport.
const CHUNK: usize = 16 * 1024;

/// The error text when the server sent nothing useful for `timeout`.
pub(crate) fn idle_message(timeout: Duration) -> String {
    format!(
        "The server sent no reply for {} (the request timeout), so the turn was ended. \
         A slow local model may need a longer timeout in the Assistant settings.",
        seconds(timeout)
    )
}

fn seconds(d: Duration) -> String {
    let s = d.as_secs_f64();
    if s >= 1.0 {
        format!("{} s", s.round())
    } else {
        format!("{} ms", d.as_millis())
    }
}

/// A reader over another one, read on a helper thread, that gives up after `timeout` without
/// bytes or once `stop` is set.
///
/// The helper thread ends when the inner read returns (the transport's own timeout, the
/// server closing, or the next read after this reader is dropped).
pub(crate) struct Watchdog<'a> {
    rx: Receiver<io::Result<Vec<u8>>>,
    buf: Vec<u8>,
    at: usize,
    timeout: Duration,
    stop: Option<&'a AtomicBool>,
    done: bool,
}

impl<'a> Watchdog<'a> {
    pub fn new(
        mut inner: Box<dyn Read + Send>,
        timeout: Duration,
        stop: Option<&'a AtomicBool>,
    ) -> Self {
        // One chunk in flight: the helper never reads far ahead of the reader.
        let (tx, rx) = mpsc::sync_channel::<io::Result<Vec<u8>>>(1);
        std::thread::Builder::new()
            .name("assistant-stream".into())
            .spawn(move || {
                let mut buf = vec![0u8; CHUNK];
                loop {
                    let sent = match inner.read(&mut buf) {
                        Ok(0) => tx.send(Ok(Vec::new())).map(|_| false),
                        Ok(n) => tx.send(Ok(buf[..n].to_vec())).map(|_| true),
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => Ok(true),
                        Err(e) => tx.send(Err(e)).map(|_| false),
                    };
                    if !matches!(sent, Ok(true)) {
                        return;
                    }
                }
            })
            .map_or_else(
                |e| {
                    // No thread: report it as the read's error rather than hang.
                    let (tx, rx) = mpsc::sync_channel(1);
                    let _ = tx.send(Err(e));
                    Watchdog::with(rx, timeout, None)
                },
                |_| Watchdog::with(rx, timeout, stop),
            )
    }

    fn with(
        rx: Receiver<io::Result<Vec<u8>>>,
        timeout: Duration,
        stop: Option<&'a AtomicBool>,
    ) -> Self {
        Watchdog {
            rx,
            buf: Vec::new(),
            at: 0,
            timeout,
            stop,
            done: false,
        }
    }

    fn stopped(&self) -> bool {
        self.stop.as_ref().is_some_and(|s| s.load(Ordering::SeqCst))
    }
}

impl Read for Watchdog<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        if self.at >= self.buf.len() {
            if self.done {
                return Ok(0);
            }
            let start = Instant::now();
            let next = loop {
                if self.stopped() {
                    // Not `Interrupted`: `BufRead::read_until` retries those forever.
                    return Err(io::Error::other("stopped"));
                }
                let left = self.timeout.saturating_sub(start.elapsed());
                if left.is_zero() {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        idle_message(self.timeout),
                    ));
                }
                match self.rx.recv_timeout(left.min(POLL)) {
                    Ok(r) => break r,
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => {
                        self.done = true;
                        return Ok(0);
                    }
                }
            };
            let bytes = next?;
            if bytes.is_empty() {
                self.done = true;
                return Ok(0);
            }
            self.buf = bytes;
            self.at = 0;
        }
        let n = out.len().min(self.buf.len() - self.at);
        out[..n].copy_from_slice(&self.buf[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// The time since the stream last carried part of the reply.
pub(crate) struct Progress {
    last: Instant,
    timeout: Duration,
}

impl Progress {
    pub fn new(timeout: Duration) -> Self {
        Progress {
            last: Instant::now(),
            timeout,
        }
    }

    /// Part of the reply arrived.
    pub fn made(&mut self) {
        self.last = Instant::now();
    }

    /// The error that ends the turn when nothing useful came for the timeout.
    pub fn expired(&self) -> Option<String> {
        (self.last.elapsed() >= self.timeout).then(|| idle_message(self.timeout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sends `bytes` every `every`, forever (until the reader is dropped).
    struct Trickle {
        bytes: &'static [u8],
        every: Duration,
    }

    impl Read for Trickle {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            std::thread::sleep(self.every);
            let n = out.len().min(self.bytes.len());
            out[..n].copy_from_slice(&self.bytes[..n]);
            Ok(n)
        }
    }

    /// Never answers until the test drops its sender.
    struct Silent(Receiver<()>);

    impl Read for Silent {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            let _ = self.0.recv();
            Ok(0)
        }
    }

    #[test]
    fn passes_bytes_through_and_ends() {
        let mut w = Watchdog::new(
            Box::new(io::Cursor::new(b"hello world".to_vec())),
            Duration::from_secs(5),
            None,
        );
        let mut s = String::new();
        w.read_to_string(&mut s).expect("read");
        assert_eq!(s, "hello world");
    }

    #[test]
    fn a_silent_stream_times_out() {
        let (_keep, rx) = mpsc::channel();
        let mut w = Watchdog::new(Box::new(Silent(rx)), Duration::from_millis(150), None);
        let t = Instant::now();
        let e = w.read(&mut [0u8; 8]).expect_err("times out");
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
        assert!(e.to_string().contains("150 ms"), "{e}");
        assert!(t.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn stop_ends_a_waiting_read() {
        let (_keep, rx) = mpsc::channel();
        let stop = AtomicBool::new(true);
        let mut w = Watchdog::new(Box::new(Silent(rx)), Duration::from_secs(60), Some(&stop));
        let e = w.read(&mut [0u8; 8]).expect_err("stopped");
        assert_eq!(e.to_string(), "stopped");
    }

    #[test]
    fn steady_bytes_do_not_time_out_the_reader() {
        let mut w = Watchdog::new(
            Box::new(Trickle {
                bytes: b": ping\n\n",
                every: Duration::from_millis(20),
            }),
            Duration::from_millis(200),
            None,
        );
        let mut buf = [0u8; 64];
        for _ in 0..20 {
            assert!(w.read(&mut buf).expect("bytes keep coming") > 0);
        }
    }

    #[test]
    fn progress_expires() {
        let mut p = Progress::new(Duration::from_millis(30));
        assert!(p.expired().is_none());
        std::thread::sleep(Duration::from_millis(40));
        assert!(p.expired().is_some_and(|m| m.contains("30 ms")));
        p.made();
        assert!(p.expired().is_none());
        assert!(idle_message(Duration::from_secs(300)).contains("300 s"));
    }
}
