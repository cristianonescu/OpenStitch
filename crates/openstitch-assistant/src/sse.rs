//! A Server-Sent Events reader (the WHATWG event-stream format) over any `BufRead`.

use std::io::{self, BufRead};
use std::time::{Duration, Instant};

/// One dispatched event: its `event:` name (empty when the stream gave none) and its data,
/// the `data:` lines joined with `\n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SseEvent {
    pub event: String,
    pub data: String,
}

pub(crate) struct SseReader<R> {
    inner: R,
    line: Vec<u8>,
    /// Ends the read when lines keep coming but no event has for this long: a stream of
    /// comments or blank lines only (#580).
    idle: Option<Duration>,
    since: Instant,
}

impl<R: BufRead> SseReader<R> {
    pub fn new(inner: R) -> Self {
        SseReader {
            inner,
            line: Vec::new(),
            idle: None,
            since: Instant::now(),
        }
    }

    /// A reader that gives up with [`io::ErrorKind::TimedOut`] after `idle` without an event.
    pub fn with_idle(inner: R, idle: Duration) -> Self {
        SseReader {
            idle: Some(idle),
            ..SseReader::new(inner)
        }
    }

    fn quiet_too_long(&self) -> io::Result<()> {
        match self.idle {
            Some(idle) if self.since.elapsed() >= idle => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                crate::idle::idle_message(idle),
            )),
            _ => Ok(()),
        }
    }

    /// The next event, or `None` at the end of the stream. A blank line dispatches; an event
    /// still being built when the stream ends is dispatched too. Comment lines (`:`), `id:`
    /// and `retry:` are skipped.
    pub fn next_event(&mut self) -> io::Result<Option<SseEvent>> {
        let mut event = String::new();
        let mut data: Option<String> = None;
        loop {
            self.line.clear();
            let n = self.inner.read_until(b'\n', &mut self.line)?;
            if n == 0 {
                return Ok(data.map(|data| SseEvent { event, data }));
            }
            let mut line: &[u8] = &self.line;
            if line.ends_with(b"\n") {
                line = &line[..line.len() - 1];
            }
            if line.ends_with(b"\r") {
                line = &line[..line.len() - 1];
            }
            if line.is_empty() {
                if let Some(data) = data.take() {
                    self.since = Instant::now();
                    return Ok(Some(SseEvent { event, data }));
                }
                self.quiet_too_long()?;
                // A blank line with no data dispatches nothing (the event name resets).
                event.clear();
                continue;
            }
            let text = String::from_utf8_lossy(line);
            if text.starts_with(':') {
                self.quiet_too_long()?;
                continue;
            }
            let (field, value) = match text.find(':') {
                Some(i) => {
                    let v = &text[i + 1..];
                    (&text[..i], v.strip_prefix(' ').unwrap_or(v))
                }
                None => (&text[..], ""),
            };
            match field {
                "event" => event = value.to_string(),
                "data" => match &mut data {
                    Some(d) => {
                        d.push('\n');
                        d.push_str(value);
                    }
                    None => data = Some(value.to_string()),
                },
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(text: &str) -> Vec<SseEvent> {
        let mut r = SseReader::new(text.as_bytes());
        let mut out = Vec::new();
        while let Some(e) = r.next_event().expect("read") {
            out.push(e);
        }
        out
    }

    fn ev(event: &str, data: &str) -> SseEvent {
        SseEvent {
            event: event.into(),
            data: data.into(),
        }
    }

    #[test]
    fn events_lines_and_dispatch() {
        let got =
            all("event: ping\ndata: {}\n\n: a comment\nevent: a\r\ndata: 1\r\ndata:2\r\n\r\n");
        assert_eq!(got, vec![ev("ping", "{}"), ev("a", "1\n2")]);
    }

    #[test]
    fn blank_lines_without_data_dispatch_nothing() {
        let got = all("\n\nevent: x\n\ndata: y\n\n");
        assert_eq!(got, vec![ev("", "y")]);
    }

    #[test]
    fn unterminated_last_event_is_dispatched() {
        assert_eq!(all("event: e\ndata: z"), vec![ev("e", "z")]);
    }

    #[test]
    fn comments_alone_time_out() {
        struct Pings;
        impl io::Read for Pings {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                std::thread::sleep(Duration::from_millis(10));
                let b = b": ping

";
                out[..b.len()].copy_from_slice(b);
                Ok(b.len())
            }
        }
        let mut r = SseReader::with_idle(io::BufReader::new(Pings), Duration::from_millis(80));
        let e = r.next_event().expect_err("no event ever comes");
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn field_without_colon_and_ids_are_ignored() {
        assert_eq!(all("id: 4\nretry: 10\ndata\n\n"), vec![ev("", "")]);
    }
}
