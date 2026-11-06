//! A tiny HTTP/1.1 server on 127.0.0.1 for tests that need a real socket: a stand-in for
//! Ollama or LM Studio (#432), so no local model server runs on the test machine. Std only;
//! every connection is answered once and closed.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// One request the server received.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub body: String,
}

/// The answer for a request: status and JSON (or any) body.
pub type Handler = dyn Fn(&Seen) -> (u16, String) + Send + Sync;

/// A running mock server. It stops accepting when dropped (the accept thread exits on the next
/// connection, which `drop` makes).
pub struct MockHttp {
    port: u16,
    seen: Arc<Mutex<Vec<Seen>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl MockHttp {
    /// Starts a server that answers every request with `handler`.
    pub fn start(handler: impl Fn(&Seen) -> (u16, String) + Send + Sync + 'static) -> MockHttp {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1");
        let port = listener.local_addr().expect("addr").port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handler: Arc<Handler> = Arc::new(handler);
        let (seen2, stop2) = (seen.clone(), stop.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stop2.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let (handler, seen) = (handler.clone(), seen2.clone());
                std::thread::spawn(move || serve(stream, &*handler, &seen));
            }
        });
        MockHttp { port, seen, stop }
    }

    /// `http://127.0.0.1:<port>`.
    pub fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The requests so far, in arrival order.
    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Drop for MockHttp {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn serve(stream: TcpStream, handler: &Handler, seen: &Mutex<Vec<Seen>>) {
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut length = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':')
            && k.trim().eq_ignore_ascii_case("content-length")
        {
            length = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let request = Seen {
        method,
        path,
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    let (status, text) = handler(&request);
    seen.lock().unwrap_or_else(|e| e.into_inner()).push(request);
    let reason = if status < 400 { "OK" } else { "Error" };
    let reply = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    let mut stream = stream;
    let _ = stream.write_all(reply.as_bytes());
    let _ = stream.flush();
}
