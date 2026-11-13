//! Crash reports (#878): when the editor hits an internal error, a small text file is written
//! beside the autosave snapshots that the user can attach to a bug report. Nothing is ever sent
//! anywhere: Help ▸ Report a Problem only opens GitHub's new-issue page in the browser with the
//! version and system filled in, and the user decides whether to submit it.
//!
//! A report holds the version, system and build, the panic message and where it happened, a
//! backtrace of function names (file paths dropped), and the names of the last commands run
//! (never their parameters). Paths under the user's home folder are replaced by `~`. Reports
//! older than [`KEEP_DAYS`] days are removed.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How many recent command names a report lists.
pub const RECENT: usize = 20;
/// Reports older than this many days are removed at start.
pub const KEEP_DAYS: u64 = 30;
/// Where Help ▸ Report a Problem goes: GitHub's new-issue page for the project.
pub const ISSUES_NEW: &str = "https://github.com/cristianonescu/OpenStitch/issues/new";
/// The file whose time marks the newest report the user has already seen.
const SEEN: &str = "crash-seen";

static COMMANDS: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// Remembers that command `id` ran (its name only), for the next report.
pub fn note_command(id: &str) {
    if let Ok(mut q) = COMMANDS.lock() {
        if q.len() >= RECENT {
            q.pop_front();
        }
        q.push_back(id.to_string());
    }
}

/// The names of the last commands run, oldest first.
pub fn recent_commands() -> Vec<String> {
    COMMANDS
        .lock()
        .map(|q| q.iter().cloned().collect())
        .unwrap_or_default()
}

/// Version, system and architecture, as Copy diagnostics puts them on the clipboard.
pub fn diagnostics(version_line: &str) -> String {
    format!(
        "OpenStitch {version_line}\nSystem: {} ({})\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// The user's home folder, if the system says where it is.
fn home() -> Option<String> {
    ["USERPROFILE", "HOME"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|h| h.len() > 3)
}

/// `text` with every path under `home` (either slash, any case) and every `C:\Users\<name>`,
/// `/Users/<name>` and `/home/<name>` replaced by `~`.
pub fn scrub(text: &str, home: Option<&str>) -> String {
    let mut out = text.to_string();
    if let Some(h) = home {
        for form in [h.to_string(), h.replace('\\', "/"), h.replace('/', "\\")] {
            out = replace_ignore_case(&out, &form, "~");
        }
    }
    for prefix in ["c:\\users\\", "c:/users/", "/users/", "/home/"] {
        out = replace_user_folders(&out, prefix);
    }
    out
}

fn replace_ignore_case(text: &str, what: &str, with: &str) -> String {
    if what.is_empty() {
        return text.to_string();
    }
    let lower = text.to_ascii_lowercase();
    let what = what.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    while let Some(i) = lower[from..].find(&what) {
        out.push_str(&text[from..from + i]);
        out.push_str(with);
        from += i + what.len();
    }
    out.push_str(&text[from..]);
    out
}

/// Replaces `<prefix><name>` (case-insensitive prefix) by `~`, the name running to the next
/// slash, quote or white space.
fn replace_user_folders(text: &str, prefix: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    while let Some(i) = lower[from..].find(prefix) {
        let start = from + i;
        let name_from = start + prefix.len();
        let name_len = text[name_from..]
            .find(|c: char| matches!(c, '/' | '\\' | '"' | '\'') || c.is_whitespace())
            .unwrap_or(text.len() - name_from);
        out.push_str(&text[from..start]);
        out.push('~');
        from = name_from + name_len;
    }
    out.push_str(&text[from..]);
    out
}

/// A backtrace with its frames' function names only: the `at <file>:<line>` lines go.
pub fn frames(backtrace: &str) -> String {
    backtrace
        .lines()
        .filter(|l| !l.trim_start().starts_with("at "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The text of a report.
pub fn report_text(
    version_line: &str,
    panic: &str,
    backtrace: &str,
    commands: &[String],
    home: Option<&str>,
) -> String {
    let text = format!(
        "OpenStitch crash report\n\n{}\nWhat happened:\n{panic}\n\nLast commands (names only):\n{}\n\nBacktrace (function names):\n{}\n",
        diagnostics(version_line),
        if commands.is_empty() {
            "(none)".to_string()
        } else {
            commands.join("\n")
        },
        frames(backtrace),
    );
    scrub(&text, home)
}

/// (year, month, day, hour, minute, second) of `t`, UTC.
fn civil(t: SystemTime) -> (i64, u32, u32, u32, u32, u32) {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        (rem % 3600 / 60) as u32,
        (rem % 60) as u32,
    )
}

/// Writes `text` as `crash-YYYY-MM-DD-HHMMSS.txt` (UTC) in `dir`, made if needed.
pub fn write_report(dir: &Path, text: &str, now: SystemTime) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let (y, mo, d, h, mi, s) = civil(now);
    let path = dir.join(format!(
        "crash-{y:04}-{mo:02}-{d:02}-{h:02}{mi:02}{s:02}.txt"
    ));
    std::fs::write(&path, text)?;
    Ok(path)
}

/// The reports in `dir` with their modification times, newest first.
fn reports(dir: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut out: Vec<(PathBuf, SystemTime)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt"))
        })
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.1));
    out
}

/// Removes reports in `dir` older than [`KEEP_DAYS`] days at `now`; how many went.
pub fn prune(dir: &Path, now: SystemTime) -> usize {
    let keep = Duration::from_secs(KEEP_DAYS * 86_400);
    reports(dir)
        .into_iter()
        .filter(|(_, t)| now.duration_since(*t).is_ok_and(|age| age > keep))
        .filter(|(p, _)| std::fs::remove_file(p).is_ok())
        .count()
}

/// The newest report in `dir` that the user has not seen yet.
pub fn unseen(dir: &Path) -> Option<PathBuf> {
    let seen = std::fs::metadata(dir.join(SEEN))
        .and_then(|m| m.modified())
        .ok();
    let (path, t) = reports(dir).into_iter().next()?;
    seen.is_none_or(|s| t > s).then_some(path)
}

/// Marks every report in `dir` as seen.
pub fn mark_seen(dir: &Path) -> std::io::Result<()> {
    std::fs::write(dir.join(SEEN), b"")
}

/// GitHub's new-issue page with a title and the body's headings and diagnostics filled in.
pub fn report_url(version_line: &str) -> String {
    let body = format!(
        "**What happened**\n\n\n**Steps to reproduce**\n\n\n**Diagnostics**\n```\n{}```\n\nIf OpenStitch wrote a crash report, you can attach it here (Help ▸ Report a Problem shows the folder).\n",
        diagnostics(version_line)
    );
    format!(
        "{ISSUES_NEW}?title={}&body={}",
        percent_encode("Problem: "),
        percent_encode(&body)
    )
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Installs the panic hook that writes a report in `dir` (after the default hook's message).
pub fn install_hook(dir: PathBuf, version_line: String) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let text = report_text(
            &version_line,
            &info.to_string(),
            &backtrace,
            &recent_commands(),
            home().as_deref(),
        );
        let _ = write_report(&dir, &text, SystemTime::now());
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openstitch-crash-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dir");
        d
    }

    /// The report holds the message and command names, and nothing that points at the user:
    /// no home path, no user folder, no file paths from the backtrace.
    #[test]
    fn a_report_holds_what_happened_and_no_personal_paths() {
        note_command("object.setRegion");
        note_command("design.export");
        let home = if cfg!(windows) {
            r"C:\Users\ana"
        } else {
            "/home/ana"
        };
        let panic = format!(
            "panicked at crates/x.rs:1:2:\ncannot read {home}{}design.pes",
            if cfg!(windows) { "\\" } else { "/" }
        );
        let backtrace = "   0: openstitch::app::frame\n             at /home/ana/src/app.rs:10:5\n   1: std::rt::lang_start\n             at C:\\Users\\ana\\rust\\rt.rs:1:1";
        let text = report_text(
            "Version 0.1.0",
            &panic,
            backtrace,
            &recent_commands(),
            Some(home),
        );
        assert!(text.contains("cannot read ~"), "{text}");
        assert!(
            text.contains("object.setRegion") && text.contains("design.export"),
            "{text}"
        );
        assert!(text.contains("openstitch::app::frame"), "{text}");
        for leak in [
            "ana",
            "/home/",
            "C:\\Users",
            "\\Users\\",
            "/Users/",
            "rt.rs",
            "app.rs",
        ] {
            assert!(!text.contains(leak), "{leak} in {text}");
        }
    }

    /// A panic caught in a test writes a report through the hook's own functions.
    #[test]
    fn a_caught_panic_writes_a_report_file() {
        let dir = temp("hook");
        let r = std::panic::catch_unwind(|| panic!("forced for the test"));
        let msg = r
            .err()
            .and_then(|p| p.downcast_ref::<&str>().map(|s| s.to_string()))
            .expect("message");
        let text = report_text("Version 0.1.0", &msg, "", &[], None);
        let path = write_report(&dir, &text, SystemTime::now()).expect("written");
        let name = path.file_name().and_then(|n| n.to_str()).expect("name");
        assert!(
            name.starts_with("crash-") && name.ends_with(".txt"),
            "{name}"
        );
        assert!(
            std::fs::read_to_string(&path)
                .expect("read")
                .contains("forced for the test")
        );
        assert_eq!(unseen(&dir), Some(path));
        mark_seen(&dir).expect("seen");
        assert_eq!(unseen(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reports_older_than_thirty_days_are_removed() {
        let dir = temp("prune");
        let now = SystemTime::now();
        let old = write_report(&dir, "old", now - Duration::from_secs(40 * 86_400)).expect("old");
        let new = write_report(&dir, "new", now).expect("new");
        // The files' times are now; prune as if 31 days had passed since the old one was made.
        let later = now + Duration::from_secs(31 * 86_400);
        std::fs::write(&old, "old").expect("touch");
        assert_eq!(prune(&dir, now), 0, "both are fresh today");
        assert_eq!(prune(&dir, later), 2, "both are past 30 days a month later");
        assert!(!old.exists() && !new.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_date_in_a_file_name_is_the_utc_date() {
        let t = UNIX_EPOCH + Duration::from_secs(1_763_000_000);
        assert_eq!(civil(t), (2025, 11, 13, 2, 13, 20));
        assert_eq!(civil(UNIX_EPOCH), (1970, 1, 1, 0, 0, 0));
    }

    /// The issue page gets the version and system and nothing else from the machine.
    #[test]
    fn report_a_problem_opens_a_prefilled_issue_page() {
        let url = report_url("Version 0.1.0-alpha.14");
        assert!(url.starts_with(ISSUES_NEW), "{url}");
        assert!(url.contains("0.1.0-alpha.14"), "{url}");
        assert!(url.contains(std::env::consts::OS), "{url}");
        if let Some(h) = home() {
            assert!(!url.contains(&percent_encode(&h)), "{url}");
        }
        assert!(!url.contains(' ') && !url.contains('\n'), "{url}");
    }
}
