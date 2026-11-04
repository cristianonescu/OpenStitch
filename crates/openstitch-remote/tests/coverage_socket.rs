//! Coverage for the socket's idle-connection timeout (`ServerConfig::idle_timeout`, #333).

use openstitch_engine::Session;
use openstitch_remote::{Server, ServerConfig};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_millis(300);

/// Starts a one-connection server with a short idle timeout and a host thread answering requests.
fn start() -> (std::net::SocketAddr, String, Arc<AtomicBool>) {
    let server = Server::start(ServerConfig {
        idle_timeout: IDLE,
        max_connections: 1,
        ..Default::default()
    })
    .expect("server");
    let endpoint = server.endpoint().clone();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    std::thread::spawn(move || {
        let mut session = Session::new(".");
        while !flag.load(Ordering::SeqCst) {
            if let Some(job) = server.wait_next(Duration::from_millis(20)) {
                let reply = server.policy().handle(&mut session, job.request());
                job.respond(reply);
            }
        }
    });
    (endpoint.address, endpoint.token, stop)
}

fn connect(address: std::net::SocketAddr) -> TcpStream {
    let s = TcpStream::connect(address).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(10)))
        .expect("timeout");
    s
}

fn request(stream: &mut TcpStream, token: &str) -> Option<Value> {
    let line = json!({"token": token, "id": 1, "command": "object.list"}).to_string() + "\n";
    stream.write_all(line.as_bytes()).ok()?;
    let mut reply = String::new();
    match BufReader::new(stream.try_clone().ok()?).read_line(&mut reply) {
        Ok(n) if n > 0 => serde_json::from_str(&reply).ok(),
        _ => None,
    }
}

#[test]
fn a_silent_connection_is_closed_after_the_idle_timeout_and_frees_its_slot() {
    let (address, token, stop) = start();
    let mut silent = connect(address);
    let began = Instant::now();
    let mut buf = [0u8; 16];
    let n = silent.read(&mut buf).unwrap_or(0);
    let waited = began.elapsed();
    assert_eq!(
        n, 0,
        "the server closes the idle connection without a reply"
    );
    assert!(waited >= IDLE - Duration::from_millis(50), "{waited:?}");
    assert!(
        waited < Duration::from_secs(5),
        "closed by the idle timeout: {waited:?}"
    );

    // With one connection allowed, a new client is served once the idle one is gone. The server
    // frees the slot just after the socket closes (its connection thread ends after the close),
    // so a client that connects at once can still be refused; the slot must be free within a
    // bound, and no other reply is accepted (#359).
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let mut next = connect(address);
        let reply = request(&mut next, &token).expect("a reply after the idle close");
        if reply["ok"] == json!(true) {
            break;
        }
        let refused = reply["error"]
            .as_str()
            .is_some_and(|e| e.contains("too many"));
        assert!(refused, "{reply}");
        assert!(Instant::now() < deadline, "slot not freed: {reply}");
        std::thread::sleep(Duration::from_millis(20));
    }
    stop.store(true, Ordering::SeqCst);
}

#[test]
fn a_connection_that_keeps_sending_stays_open_past_the_idle_timeout() {
    let (address, token, stop) = start();
    let mut busy = connect(address);
    // Blank lines get no reply but count as activity.
    for _ in 0..6 {
        std::thread::sleep(IDLE / 3);
        busy.write_all(b"\n").expect("still open");
    }
    let reply = request(&mut busy, &token).expect("still open after twice the idle timeout");
    assert_eq!(reply["ok"], json!(true), "{reply}");
    stop.store(true, Ordering::SeqCst);
}
