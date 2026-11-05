// Adapted from SolidCraft (github.com/tangency-labs/solidcraft, commit 6bca7d2), Copyright (c) 2026 SolidCraft contributors, MIT OR Apache-2.0.
//! Which ACP agents are installed here. A desktop app often starts without the shell's PATH, so
//! the usual install folders are searched too. Discovery never starts a login or a session: each
//! agent found is asked for `--version` (3 s limit), and whether it looks logged in comes only from
//! whether its own login file exists (the file is never opened).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// An agent OpenStitch knows how to start in ACP mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownAgent {
    pub id: &'static str,
    pub name: &'static str,
    /// The program to look for (on Windows also with `.exe`, `.cmd` or `.bat`).
    pub program: &'static str,
    /// Arguments that put it in ACP mode.
    pub args: &'static [&'static str],
    /// Its one-line install hint.
    pub install: &'static str,
    /// Its own login files, relative to the home folder. Empty: it keeps none we can look at.
    pub login_files: &'static [&'static str],
}

/// The agents the panel offers, in SolidCraft's order. SolidCraft's Gemini CLI (retired by Google)
/// and Antigravity (a separately downloaded `.par` server that does not print a version) are left
/// out.
pub const KNOWN_AGENTS: &[KnownAgent] = &[
    KnownAgent {
        id: "claude",
        name: "Claude Code",
        program: "claude-agent-acp",
        args: &[],
        install: "npm install -g @agentclientprotocol/claude-agent-acp",
        login_files: &[".claude/.credentials.json"],
    },
    KnownAgent {
        id: "codex",
        name: "Codex",
        program: "codex-acp",
        args: &[],
        install: "npm install -g @agentclientprotocol/codex-acp",
        login_files: &[".codex/auth.json"],
    },
    KnownAgent {
        id: "opencode",
        name: "OpenCode",
        program: "opencode",
        args: &["acp"],
        install: "npm install -g opencode-ai",
        login_files: &[".local/share/opencode/auth.json"],
    },
    KnownAgent {
        id: "grok",
        name: "XAI Grok",
        program: "grok",
        args: &["agent", "stdio"],
        install: "see https://docs.x.ai/build/overview (Grok Build installer); then run grok login",
        login_files: &[".grok/auth.json"],
    },
    KnownAgent {
        id: "copilot",
        name: "GitHub Copilot CLI",
        program: "copilot",
        args: &["--acp"],
        install: "npm install -g @github/copilot",
        login_files: &[],
    },
    KnownAgent {
        id: "qwen",
        name: "Qwen Code",
        program: "qwen",
        args: &["--acp"],
        install: "npm install -g @qwen-code/qwen-code",
        login_files: &[".qwen/oauth_creds.json"],
    },
    KnownAgent {
        id: "goose",
        name: "goose",
        program: "goose",
        args: &["acp"],
        install: "see block.github.io/goose (installer script or Homebrew)",
        login_files: &[],
    },
];

/// What discovery found out about one agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentState {
    /// Found and it started. `version` is the number from its first `--version` line (empty when
    /// it printed nothing or was still running after 3 s). `logged_in`: whether one of its login
    /// files exists; `None` when it keeps none we can look at (on macOS Claude Code keeps its
    /// login in the keychain, so a missing file says nothing there).
    Ready {
        version: String,
        logged_in: Option<bool>,
    },
    NotInstalled,
    /// Found, but `--version` failed without printing one; the reason, cut short.
    DoesNotStart(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovered {
    pub agent: &'static KnownAgent,
    /// The program found (`None` when not installed).
    pub path: Option<PathBuf>,
    pub state: AgentState,
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// PATH, then the usual places npm, Homebrew, nvm, Volta, bun and Grok put commands, and on
/// Windows npm's and Node's own folders: a launcher can start OpenStitch with a PATH that lacks
/// them. Deduplicated; only folders that exist.
pub fn search_dirs() -> Vec<PathBuf> {
    search_dirs_in(&|name| std::env::var_os(name), cfg!(windows))
}

fn search_dirs_in(var: &dyn Fn(&str) -> Option<OsString>, windows: bool) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = var("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(prefix) = var("NPM_CONFIG_PREFIX").map(PathBuf::from) {
        dirs.push(if windows {
            prefix.clone()
        } else {
            prefix.join("bin")
        });
    }
    if let Some(h) = var(if windows { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        for d in [
            ".local/bin",
            ".npm-global/bin",
            ".npm-packages/bin",
            ".volta/bin",
            ".bun/bin",
            ".cargo/bin",
            ".grok/bin",
            "bin",
        ] {
            dirs.push(h.join(d));
        }
        // nvm: every installed Node version's bin, newest name first.
        if let Ok(rd) = std::fs::read_dir(h.join(".nvm/versions/node")) {
            let mut v: Vec<PathBuf> = rd
                .flatten()
                .take(64)
                .map(|e| e.path().join("bin"))
                .collect();
            v.sort();
            dirs.extend(v.into_iter().rev());
        }
    }
    if windows {
        if let Some(a) = var("APPDATA") {
            dirs.push(PathBuf::from(a).join("npm"));
        }
        for program_files in ["ProgramFiles", "ProgramFiles(x86)"] {
            dirs.extend(var(program_files).map(|d| PathBuf::from(d).join("nodejs")));
        }
        dirs.extend(var("NVM_SYMLINK").map(PathBuf::from));
    } else {
        for d in [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/usr/bin",
        ] {
            dirs.push(PathBuf::from(d));
        }
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| d.is_absolute() && d.is_dir() && seen.insert(d.clone()));
    dirs
}

/// The file names `program` can have. On Windows only ones it can run: npm puts a Unix shell
/// script with the bare name next to the `.cmd` shim.
fn names_for(program: &str, windows: bool) -> Vec<String> {
    if windows {
        [".exe", ".cmd", ".bat"]
            .iter()
            .map(|e| format!("{program}{e}"))
            .collect()
    } else {
        vec![program.to_string()]
    }
}

/// The first `program` in `dirs`.
pub(crate) fn find_in(program: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let names = names_for(program, cfg!(windows));
    dirs.iter()
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| p.is_file())
}

/// `program` on [`search_dirs`], as [`discover`] finds it: on Windows only an `.exe`, `.cmd` or
/// `.bat`, never npm's extensionless shell script beside the `.cmd` (#401).
pub fn find_program(program: &str) -> Option<PathBuf> {
    find_in(program, &search_dirs())
}

/// A command for `program` whose PATH is `extra` followed by [`search_dirs`], so an npm adapter's
/// `#!/usr/bin/env node` finds Node even when the app started without the shell's PATH. Windows
/// `.cmd`/`.bat` shims run through `cmd /C`.
pub(crate) fn command_for(program: &Path, extra: &[PathBuf]) -> Command {
    let mut cmd = if cfg!(windows)
        && program
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
    {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(program);
        c
    } else {
        Command::new(program)
    };
    let mut dirs: Vec<PathBuf> = extra.to_vec();
    dirs.extend(search_dirs());
    if let Ok(path) = std::env::join_paths(dirs) {
        cmd.env("PATH", path);
    }
    cmd
}

/// How long an agent is given to answer `--version`.
const PROBE_LIMIT: Duration = Duration::from_secs(3);
/// The longest reason kept from a failed probe.
const PROBLEM_CHARS: usize = 300;

/// What `program --version` showed: Ok(version, maybe empty) or Err(why it does not start).
fn probe(program: &Path, limit: Duration) -> Result<String, String> {
    let mut cmd = command_for(program, &[]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: probing a console agent doesn't flash a terminal window.
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = match cmd
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            return Err(cut(&e
                .to_string()
                .replace("%1", &program.display().to_string())));
        }
    };
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                // Out of time says nothing either way (a slow machine).
                let _ = child.kill();
                let _ = child.wait();
                return Ok(String::new());
            }
        }
    };
    let out = read_capped(child.stdout.take());
    if let Some(line) = out.lines().map(str::trim).find(|l| !l.is_empty()) {
        return Ok(version_of(line));
    }
    if status.success() {
        return Ok(String::new());
    }
    let err = read_capped(child.stderr.take());
    let reason = err
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| status.to_string());
    Err(cut(&reason))
}

/// What a finished probe wrote on a pipe, up to 64 KiB.
fn read_capped(pipe: Option<impl std::io::Read>) -> String {
    let mut bytes = Vec::new();
    if let Some(p) = pipe {
        let _ = std::io::Read::read_to_end(&mut p.take(64 * 1024), &mut bytes);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn cut(text: &str) -> String {
    text.chars().take(PROBLEM_CHARS).collect()
}

/// The version number in a `--version` line (`codex-cli 0.162.1`, `2.1.295 (Claude Code)`),
/// else the line itself, cut short.
fn version_of(line: &str) -> String {
    let v = line
        .split_whitespace()
        .find(|w| {
            w.trim_start_matches('v')
                .starts_with(|c: char| c.is_ascii_digit())
        })
        .unwrap_or(line);
    v.chars().take(40).collect()
}

/// Whether `agent` looks logged in, from file existence only.
fn logged_in(agent: &KnownAgent, home: Option<&Path>) -> Option<bool> {
    if agent.login_files.is_empty() {
        return None;
    }
    // The variable that moves the agent's settings folder, with the login file's name in it.
    let moved = match agent.id {
        "claude" => Some(("CLAUDE_CONFIG_DIR", ".credentials.json")),
        "codex" => Some(("CODEX_HOME", "auth.json")),
        _ => None,
    }
    .and_then(|(var, name)| {
        std::env::var_os(var)
            .filter(|v| !v.is_empty())
            .map(|d| PathBuf::from(d).join(name))
    });
    let mut files: Vec<PathBuf> = moved.into_iter().collect();
    if let Some(h) = home {
        files.extend(agent.login_files.iter().map(|f| h.join(f)));
    }
    if files.iter().any(|f| f.is_file()) {
        Some(true)
    } else if cfg!(target_os = "macos") && agent.id == "claude" {
        None
    } else {
        Some(false)
    }
}

/// Look for every known agent on [`search_dirs`]. Blocking (the probes run in parallel, 3 s each
/// at most): call it off the UI thread.
pub fn discover() -> Vec<Discovered> {
    discover_in(&search_dirs(), home().as_deref())
}

/// [`discover`] in the given folders, with `home` for the login files (for tests and for a
/// panel that wants to look somewhere else).
pub fn discover_in(dirs: &[PathBuf], home: Option<&Path>) -> Vec<Discovered> {
    let handles: Vec<_> = KNOWN_AGENTS
        .iter()
        .map(|agent| {
            let path = find_in(agent.program, dirs);
            let logged_in = logged_in(agent, home);
            std::thread::spawn(move || {
                let state = match &path {
                    None => AgentState::NotInstalled,
                    Some(p) => match probe(p, PROBE_LIMIT) {
                        Ok(version) => AgentState::Ready { version, logged_in },
                        Err(why) => AgentState::DoesNotStart(why),
                    },
                };
                Discovered { agent, path, state }
            })
        })
        .collect();
    handles
        .into_iter()
        .zip(KNOWN_AGENTS)
        .map(|(h, agent)| {
            h.join().unwrap_or(Discovered {
                agent,
                path: None,
                state: AgentState::DoesNotStart("the check failed".into()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(version_of("codex-cli 0.162.1"), "0.162.1");
        assert_eq!(version_of("2.1.295 (Claude Code)"), "2.1.295");
        assert_eq!(version_of("v1.2"), "v1.2");
        assert_eq!(version_of("unknown"), "unknown");
    }

    #[test]
    fn search_dirs_dedupe_and_skip_missing() {
        let tmp = std::env::temp_dir();
        let path = std::env::join_paths([tmp.clone(), tmp.clone(), tmp.join("no-such-dir-xyz")])
            .expect("join");
        let dirs = search_dirs_in(
            &|name| (name == "PATH").then(|| path.clone()),
            cfg!(windows),
        );
        assert_eq!(dirs.iter().filter(|d| **d == tmp).count(), 1);
        assert!(dirs.iter().all(|d| d.is_dir()));
    }

    #[test]
    fn windows_names() {
        assert_eq!(
            names_for("codex-acp", true),
            ["codex-acp.exe", "codex-acp.cmd", "codex-acp.bat"]
        );
        assert_eq!(names_for("codex-acp", false), ["codex-acp"]);
    }

    /// npm puts an extensionless `#!/bin/sh` script beside `claude-agent-acp.cmd`; on Windows
    /// starting it fails with "not a valid Win32 application" (#401), so the `.cmd` is found.
    #[test]
    fn npm_shell_script_is_skipped_on_windows() {
        let dir = openstitch_testutil::TempDir::new("discover-npm-shim");
        std::fs::write(dir.join("claude-agent-acp"), "#!/bin/sh\n").expect("write");
        std::fs::write(dir.join("claude-agent-acp.cmd"), "@echo off\r\n").expect("write");
        let found = find_in("claude-agent-acp", &[dir.path().to_path_buf()]).expect("found");
        let name = found
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if cfg!(windows) {
            assert_eq!(name, "claude-agent-acp.cmd");
        } else {
            assert_eq!(name, "claude-agent-acp");
        }
    }
}
