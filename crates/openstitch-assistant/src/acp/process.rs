// Adapted from SolidCraft (github.com/tangency-labs/solidcraft, commit 6bca7d2), Copyright (c) 2026 SolidCraft contributors, MIT OR Apache-2.0.
//! The agent as a child process: its stdout is read on a thread, its stdin written on another,
//! its stderr kept as a short tail to explain a failed start.
//!
//! Agents start helpers of their own (Node adapters start the CLI they wrap, which starts the MCP
//! servers it was given), so stopping only the program we started would leave those running. The
//! agent therefore gets a process group of its own on Unix (`process_group(0)`; the group is
//! signalled with the `kill` program, since the workspace forbids `unsafe` and so `libc`), and on
//! Windows `taskkill /T` walks its tree. [`AgentProcess::shutdown`] closes stdin first (an ACP
//! agent ends at end of input: <https://agentclientprotocol.com/protocol/v1/transports>), then asks
//! the group to stop, then stops it. Everything is addressed by the ids of the processes we
//! started, never by name.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Longest line read from the agent; a longer one is dropped.
const MAX_LINE: usize = 8 << 20;
/// Lines of the agent's stderr kept to explain a failed start.
const STDERR_LINES: usize = 40;
/// How long an agent gets to end by itself once its input is closed, and then once asked to stop.
const GRACE: Duration = Duration::from_millis(1200);
/// Once the agent has ended, how long the stderr reader may sit on an empty, still-open pipe
/// before the exit is reported anyway: only a helper process still holding the agent's stderr
/// does that. Time the reader spends unscheduled on a busy machine does not count (#426).
const STDERR_DRAIN: Duration = Duration::from_secs(2);
/// The same when the reader was already waiting before the exit was seen: then it cannot show
/// whether the agent's last words are still in the pipe for a reader the machine has not run
/// yet, so it gets far longer than the starvation measured on a loaded 4-core PC (over 2 s).
const STDERR_DRAIN_UNSEEN: Duration = Duration::from_secs(10);
/// The longest wait after stdout closes before the exit is reported whatever stderr does: an
/// agent that closed stdout but keeps running.
const STDERR_BACKSTOP: Duration = Duration::from_secs(30);

/// The agent's stderr tail.
pub(crate) type Tail = Arc<Mutex<VecDeque<String>>>;

/// Whether the agent process has ended (or is being stopped, which takes it out of the slot).
fn agent_ended(child: &Mutex<Option<Child>>) -> bool {
    child.lock().map_or(true, |mut c| match c.as_mut() {
        None => true,
        Some(child) => !matches!(child.try_wait(), Ok(None)),
    })
}

pub(crate) struct AgentProcess {
    /// Shared with the stdout reader, which checks whether the agent has ended.
    child: Arc<Mutex<Option<Child>>>,
    /// Lines for the agent's stdin; `None` once stopped (dropping the sender closes stdin).
    to_agent: Arc<Mutex<Option<Sender<String>>>>,
    pub(crate) tail: Tail,
}

/// Writes lines to the agent from any thread.
#[derive(Clone)]
pub(crate) struct Writer(Arc<Mutex<Option<Sender<String>>>>);

impl Writer {
    pub(crate) fn send(&self, msg: &serde_json::Value) {
        if let Ok(slot) = self.0.lock()
            && let Some(tx) = slot.as_ref()
        {
            // serde_json never writes a raw newline inside a message, as the transport requires.
            let _ = tx.send(msg.to_string());
        }
    }
}

/// Start the agent in a process group of its own (Unix) or without a console window (Windows).
fn detach_group(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashes up for a console agent.
        cmd.creation_flags(0x0800_0000);
    }
}

/// Signal the agent's whole process group (`TERM` or `KILL`). The group's id is the agent's own
/// process id, as it was started with `process_group(0)`. `kill -SIG -PGID` with no `--`: busybox'
/// `kill` takes none, and procps, util-linux and BSD `kill` all read a negative id as a group.
#[cfg(unix)]
fn signal_group(pgid: u32, signal: &str) {
    if pgid <= 1 {
        return;
    }
    let _ = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(format!("-{pgid}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Windows: end the agent and every process it started.
#[cfg(windows)]
fn kill_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Wait up to `limit` for the child to end; whether it did.
fn wait_for(child: &mut Child, limit: Duration) -> bool {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => return false,
            Err(_) => return true,
        }
    }
}

/// End the agent and everything it started. Blocks for at most about three seconds.
fn end_tree(mut child: Child) {
    let pid = child.id();
    // Its input is already closed: a well-behaved agent ends now and stops its own helpers.
    let ended = wait_for(&mut child, GRACE);
    #[cfg(unix)]
    {
        // Helpers can outlive the agent: the group is signalled whether or not it ended. The
        // group id stays reserved while any member lives, so it can't name someone else's.
        signal_group(pid, "TERM");
        if !ended {
            wait_for(&mut child, GRACE);
        }
        signal_group(pid, "KILL");
    }
    #[cfg(windows)]
    {
        if !ended {
            kill_tree(pid);
        }
    }
    #[cfg(not(any(unix, windows)))]
    let _ = (pid, ended);
    let _ = child.kill();
    let _ = child.wait();
}

impl AgentProcess {
    /// Start `cmd` (already given its program, arguments, folder and environment). Each stdout
    /// line goes to `on_line` on the reader thread; `on_exit` runs there once stdout closes.
    pub(crate) fn spawn(
        mut cmd: Command,
        mut on_line: impl FnMut(&str) + Send + 'static,
        on_exit: impl FnOnce() + Send + 'static,
    ) -> std::io::Result<AgentProcess> {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        detach_group(&mut cmd);
        let mut child = cmd.spawn()?;
        let (Some(mut stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            end_tree(child);
            return Err(std::io::Error::other(
                "the agent's pipes could not be opened",
            ));
        };
        let (tx, rx) = channel::<String>();
        std::thread::spawn(move || {
            for line in rx {
                if stdin
                    .write_all(line.as_bytes())
                    .and_then(|()| stdin.write_all(b"\n"))
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    break;
                }
            }
            // Every sender is gone: stdin closes here, which is how an ACP agent learns to end.
        });
        let child = Arc::new(Mutex::new(Some(child)));
        let child_w = child.clone();
        let tail: Tail = Arc::new(Mutex::new(VecDeque::new()));
        let tail_w = tail.clone();
        // Dropped when the stderr reader ends: the stdout reader waits for that before it
        // reports the exit, so the tail holds the agent's last words (#426).
        let (stderr_open, stderr_closed) = channel::<()>();
        // When the stderr reader last went to wait on an empty pipe (None while it has data in
        // hand): how the stdout reader tells a helper holding stderr open from a reader that
        // simply has not run yet on a busy machine.
        let waiting: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let waiting_w = waiting.clone();
        std::thread::spawn(move || {
            let _open = stderr_open;
            let mut reader = BufReader::new(stderr);
            loop {
                if reader.buffer().is_empty()
                    && let Ok(mut w) = waiting_w.lock()
                {
                    *w = Some(Instant::now());
                }
                let mut line = String::new();
                let read = reader.read_line(&mut line);
                if let Ok(mut w) = waiting_w.lock() {
                    *w = None;
                }
                match read {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                if let Ok(mut t) = tail_w.lock() {
                    if t.len() >= STDERR_LINES {
                        t.pop_front();
                    }
                    let line = line.trim_end_matches(['\r', '\n']);
                    t.push_back(line.chars().take(400).collect());
                }
            }
        });
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut buf = Vec::new();
                match (&mut reader)
                    .take(MAX_LINE as u64 + 1)
                    .read_until(b'\n', &mut buf)
                {
                    Ok(0) | Err(_) => break,
                    Ok(_) if buf.len() > MAX_LINE => {
                        // Skip the rest of an oversized line.
                        let mut rest = Vec::new();
                        if reader.read_until(b'\n', &mut rest).is_err() {
                            break;
                        }
                    }
                    Ok(_) => on_line(&String::from_utf8_lossy(&buf)),
                }
            }
            // Wait until stderr has been read to its end, however slowly this machine runs, so
            // the exit is reported with the agent's last words (#426). A helper that inherited
            // stderr and outlives the agent keeps it open. That is told apart from a stderr
            // reader that has not been scheduled yet by the reader's own state: once the agent
            // has exited, its last words are in the pipe, and a reader that went to wait on an
            // empty pipe after that and is still waiting STDERR_DRAIN later has read them all.
            // A reader that was already waiting when the exit was seen proves less (it may not
            // have run since the last words arrived), so it is given STDERR_DRAIN_UNSEEN.
            let closed_stdout = Instant::now();
            let mut exit_seen: Option<Instant> = None;
            while let Err(RecvTimeoutError::Timeout) =
                stderr_closed.recv_timeout(Duration::from_millis(50))
            {
                if exit_seen.is_none() && agent_ended(&child_w) {
                    exit_seen = Some(Instant::now());
                }
                let drained = exit_seen.is_some_and(|seen| {
                    waiting.lock().ok().and_then(|w| *w).is_some_and(|since| {
                        if since >= seen {
                            since.elapsed() >= STDERR_DRAIN
                        } else {
                            seen.elapsed() >= STDERR_DRAIN_UNSEEN
                        }
                    })
                });
                if drained || closed_stdout.elapsed() >= STDERR_BACKSTOP {
                    break;
                }
            }
            on_exit();
        });
        Ok(AgentProcess {
            child,
            to_agent: Arc::new(Mutex::new(Some(tx))),
            tail,
        })
    }

    pub(crate) fn writer(&self) -> Writer {
        Writer(self.to_agent.clone())
    }

    /// The agent's last stderr lines, joined.
    pub(crate) fn stderr_tail(&self) -> String {
        tail_text(&self.tail)
    }

    /// End the agent and its helpers; blocks until they are gone (about three seconds at most).
    pub(crate) fn shutdown(&self) {
        if let Ok(mut slot) = self.to_agent.lock() {
            *slot = None;
        }
        let child = self.child.lock().ok().and_then(|mut c| c.take());
        if let Some(child) = child {
            end_tree(child);
        }
    }
}

pub(crate) fn tail_text(tail: &Tail) -> String {
    tail.lock()
        .map(|t| t.iter().cloned().collect::<Vec<_>>().join("\n"))
        .unwrap_or_default()
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    /// An agent that ends while a helper it started still holds its stderr (not its stdout) is
    /// reported as ended once the pipe has stayed empty long enough, with its last words in the
    /// tail, well before the helper ends.
    #[test]
    fn the_exit_is_reported_while_a_helper_holds_stderr() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo last words >&2; sleep 60 >/dev/null & exit 0"]);
        let (tx, rx) = channel();
        let started = Instant::now();
        let process = AgentProcess::spawn(
            cmd,
            |_| {},
            move || {
                let _ = tx.send(());
            },
        )
        .expect("spawn sh");
        rx.recv_timeout(STDERR_DRAIN_UNSEEN + Duration::from_secs(15))
            .expect("the exit is reported before the helper ends");
        assert!(started.elapsed() >= STDERR_DRAIN);
        assert_eq!(process.stderr_tail(), "last words");
    }
}
