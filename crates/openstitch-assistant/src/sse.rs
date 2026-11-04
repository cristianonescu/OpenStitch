//! A Server-Sent Events reader (the WHATWG event-stream format) over any `BufRead`.

use std::io::{self, BufRead};

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
}

impl<R: BufRead> SseReader<R> {
    pub fn new(inner: R) -> Self {
        SseReader {
            inner,
            line: Vec::new(),
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
                    return Ok(Some(SseEvent { event, data }));
                }
                // A blank line with no data dispatches nothing (the event name resets).
                event.clear();
                continue;
            }
            let text = String::from_utf8_lossy(line);
            if text.starts_with(':') {
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
    fn field_without_colon_and_ids_are_ignored() {
        assert_eq!(all("id: 4\nretry: 10\ndata\n\n"), vec![ev("", "")]);
    }
}
