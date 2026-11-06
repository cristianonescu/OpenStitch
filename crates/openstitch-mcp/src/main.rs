//! `openstitch-mcp`: MCP server on stdio. One JSON-RPC message per line in, one per line out;
//! logs go to stderr.

use openstitch_mcp::{Config, Server};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str =
    "usage: openstitch-mcp [--root DIR]... [--read-root DIR]... [--write-root DIR]...
                      [--read-only] [--no-files]
                      [--max-request-bytes N] [--max-response-bytes N]
                      [--live [--endpoint FILE]] [--assistant]
  --root DIR         commands may read and write files under DIR (repeatable)
  --read-root DIR    commands may only read files under DIR (repeatable)
  --write-root DIR   same as --root
  --read-only        inspect and preview only: no edits, undo/redo or file writes
  --no-files         no file access at all (cannot be combined with roots)
  --live             drive the running editor (started with `openstitch --listen PORT`)
                     instead of an own design; the editor's file roots apply
  --endpoint FILE    the editor's endpoint file (default: automation.json in the
                     OpenStitch config directory); implies --live
  --assistant        for the editor's AI assistant: implies --live, and offers nine
                     meta tools (commands_list, command_describe, execute, run_batch,
                     view_render, scratch_write, skill_reference, notes_read,
                     notes_write) instead of one tool per command
With no roots, every file command is refused. Relative paths resolve against the first
write root, else the first read root.";

fn log(message: &str) {
    eprintln!("openstitch-mcp: {message}");
}

fn dir(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// The server settings, the endpoint file with `--live` (`None` inside: the default file), and
/// whether `--assistant` was given.
type Parsed = (Config, Option<Option<PathBuf>>, bool);

fn parse(mut args: impl Iterator<Item = String>) -> Result<Option<Parsed>, String> {
    let mut config = Config::default();
    let mut live = None;
    let mut assistant = false;
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--root" | "--write-root" => config.write_roots.push(dir(&value()?)),
            "--read-root" => config.read_roots.push(dir(&value()?)),
            "--max-request-bytes" | "--max-response-bytes" => {
                let n: usize = value()?
                    .parse()
                    .ok()
                    .filter(|n| *n >= 1024)
                    .ok_or_else(|| format!("{flag} must be an integer >= 1024"))?;
                if flag == "--max-request-bytes" {
                    config.max_request_bytes = n;
                } else {
                    config.max_response_bytes = n;
                }
            }
            "--live" => live = Some(live.flatten()),
            "--assistant" => {
                assistant = true;
                live = Some(live.flatten());
            }
            "--endpoint" => live = Some(Some(PathBuf::from(value()?))),
            "--read-only" => config.capabilities = config.capabilities.read_only(),
            "--no-files" => config.capabilities = config.capabilities.without_files(),
            "-h" | "--help" => return Ok(None),
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    let c = config.capabilities;
    if !c.read_files
        && !c.write_files
        && !(config.read_roots.is_empty() && config.write_roots.is_empty())
    {
        return Err("--no-files cannot be combined with --root/--read-root/--write-root".into());
    }
    if live.is_some() && !(config.read_roots.is_empty() && config.write_roots.is_empty()) {
        return Err("with --live the editor's own roots apply (openstitch --listen-root)".into());
    }
    Ok(Some((config, live, assistant)))
}

fn main() -> ExitCode {
    let (config, live, assistant) = match parse(std::env::args().skip(1)) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            log(&format!("{e}\n{USAGE}"));
            return ExitCode::from(64);
        }
    };
    let mut server = match Server::new(config) {
        Ok(server) => server,
        Err(e) => {
            log(&e.to_string());
            return ExitCode::from(66);
        }
    };
    if let Some(file) = live {
        let Some(file) = file.or_else(openstitch_remote::default_endpoint_file) else {
            log("no OpenStitch config directory; pass --endpoint FILE");
            return ExitCode::from(64);
        };
        match openstitch_remote::Client::from_file(&file) {
            Ok(client) => {
                log(&format!(
                    "ready; driving the editor at {}",
                    client.endpoint().address
                ));
                server = if assistant {
                    server.with_assistant(client)
                } else {
                    server.with_live(client)
                };
            }
            Err(e) => {
                log(&format!(
                    "cannot read {} ({e}); is the editor running with --listen?",
                    file.display()
                ));
                return ExitCode::from(66);
            }
        }
    } else if let Some(sandbox) = server.sandbox() {
        log(&format!(
            "ready; write roots: {:?}, read roots: {:?}, capabilities: {:?}",
            sandbox.write_roots(),
            sandbox.read_roots(),
            server.config().capabilities.names()
        ));
    }

    openstitch_mcp::serve_stdio(&mut server);
    ExitCode::SUCCESS
}
