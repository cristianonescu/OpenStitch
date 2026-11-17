//! A Server-Sent Events reader (the WHATWG event-stream format): [`SseReader`] over any
//! `BufRead`, and [`SseDecoder`] for bytes pushed as they arrive (a browser's `fetch` stream).

use std::io::{self, BufRead};
use std::time::Duration;

use web_time::Instant;

/// One dispatched event: its `event:` name (empty when the stream gave none) and its data,
/// the `data:` lines joined with `\n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// The event being built, one line at a time.
#[derive(Default)]
struct Parser {
    event: String,
    data: Option<String>,
}

/// What one line did.
enum Line {
    /// A blank line ended an event.
    Dispatch(SseEvent),
    /// A comment, or a blank line with no data: nothing for the reply.
    Quiet,
    /// A field (`event:`, `data:`, or one that is skipped).
    Field,
}

impl Parser {
    /// One line, without its line ending. Comment lines (`:`), `id:` and `retry:` are skipped.
    fn line(&mut self, line: &[u8]) -> Line {
        if line.is_empty() {
            let event = std::mem::take(&mut self.event);
            return match self.data.take() {
                Some(data) => Line::Dispatch(SseEvent { event, data }),
                // A blank line with no data dispatches nothing (the event name resets).
                None => Line::Quiet,
            };
        }
        let text = String::from_utf8_lossy(line);
        if text.starts_with(':') {
            return Line::Quiet;
        }
        let (field, value) = match text.find(':') {
            Some(i) => {
                let v = &text[i + 1..];
                (&text[..i], v.strip_prefix(' ').unwrap_or(v))
            }
            None => (&text[..], ""),
        };
        match field {
            "event" => self.event = value.to_string(),
            "data" => match &mut self.data {
                Some(d) => {
                    d.push('\n');
                    d.push_str(value);
                }
                None => self.data = Some(value.to_string()),
            },
            _ => {}
        }
        Line::Field
    }

    /// The stream ended: an event still being built is dispatched.
    fn end(&mut self) -> Option<SseEvent> {
        let event = std::mem::take(&mut self.event);
        self.data.take().map(|data| SseEvent { event, data })
    }
}

/// The line without its `\n` or `\r\n`.
fn trim_eol(mut line: &[u8]) -> &[u8] {
    if line.ends_with(b"\n") {
        line = &line[..line.len() - 1];
    }
    if line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    line
}

fn quiet_error(idle: Duration) -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, crate::idle::idle_message(idle))
}

pub(crate) struct SseReader<R> {
    inner: R,
    line: Vec<u8>,
    parser: Parser,
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
            parser: Parser::default(),
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
            Some(idle) if self.since.elapsed() >= idle => Err(quiet_error(idle)),
            _ => Ok(()),
        }
    }

    /// The next event, or `None` at the end of the stream. A blank line dispatches; an event
    /// still being built when the stream ends is dispatched too.
    pub fn next_event(&mut self) -> io::Result<Option<SseEvent>> {
        loop {
            self.line.clear();
            let n = self.inner.read_until(b'\n', &mut self.line)?;
            if n == 0 {
                return Ok(self.parser.end());
            }
            match self.parser.line(trim_eol(&self.line)) {
                Line::Dispatch(ev) => {
                    self.since = Instant::now();
                    return Ok(Some(ev));
                }
                Line::Quiet => self.quiet_too_long()?,
                Line::Field => {}
            }
        }
    }
}

/// An SSE parser fed with bytes as they arrive, for a stream that cannot block (#766).
pub struct SseDecoder {
    pending: Vec<u8>,
    parser: Parser,
    idle: Duration,
    since: Instant,
}

impl SseDecoder {
    /// A decoder that fails with [`io::ErrorKind::TimedOut`] once lines kept coming for `idle`
    /// without an event (comments or blank lines only, #580).
    pub fn new(idle: Duration) -> Self {
        SseDecoder {
            pending: Vec::new(),
            parser: Parser::default(),
            idle,
            since: Instant::now(),
        }
    }

    /// The events the bytes completed, in order.
    pub fn push(&mut self, bytes: &[u8]) -> io::Result<Vec<SseEvent>> {
        self.pending.extend_from_slice(bytes);
        let mut events = Vec::new();
        let mut start = 0;
        while let Some(i) = self.pending[start..].iter().position(|&b| b == b'\n') {
            let end = start + i + 1;
            match self.parser.line(trim_eol(&self.pending[start..end])) {
                Line::Dispatch(ev) => {
                    self.since = Instant::now();
                    events.push(ev);
                }
                Line::Quiet if events.is_empty() && self.since.elapsed() >= self.idle => {
                    return Err(quiet_error(self.idle));
                }
                Line::Quiet | Line::Field => {}
            }
            start = end;
        }
        self.pending.drain(..start);
        Ok(events)
    }

    /// The stream ended: the last line and an event still being built.
    pub fn finish(&mut self) -> Option<SseEvent> {
        if !self.pending.is_empty() {
            let rest = std::mem::take(&mut self.pending);
            if let Line::Dispatch(ev) = self.parser.line(trim_eol(&rest)) {
                return Some(ev);
            }
        }
        self.parser.end()
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

    /// #766: bytes pushed in any pieces give the events the reader gives.
    #[test]
    fn the_decoder_matches_the_reader_for_any_split() {
        let text = "event: ping\ndata: {}\n\n: a comment\nevent: a\r\ndata: 1\r\ndata:2\r\n\r\nevent: e\ndata: z";
        let want = all(text);
        for piece in 1..=text.len() {
            let mut d = SseDecoder::new(Duration::from_secs(60));
            let mut got = Vec::new();
            for chunk in text.as_bytes().chunks(piece) {
                got.extend(d.push(chunk).expect("push"));
            }
            got.extend(d.finish());
            assert_eq!(got, want, "pieces of {piece}");
        }
    }

    #[test]
    fn the_decoder_times_out_on_comments_alone() {
        let mut d = SseDecoder::new(Duration::from_millis(20));
        assert!(d.push(b": ping\n").expect("early").is_empty());
        std::thread::sleep(Duration::from_millis(30));
        let e = d.push(b": ping\n").expect_err("quiet too long");
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
    }
}
