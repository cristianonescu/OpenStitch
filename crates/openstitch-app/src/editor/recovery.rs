//! Autosave and recovery (#549, beyond PE-DESIGN). Each editor window keeps one snapshot of
//! its design in the `recovery` folder beside ui.json: `<id>.osd` (the project file, fonts
//! included), `<id>.json` (what it was: file, name, time) and `<id>.lock`, which the window
//! holds locked (an OS advisory lock) while it runs. A snapshot whose lock nobody holds was
//! left by a window that crashed or lost power, and is offered at the next start. A normal
//! close removes the window's three files. Snapshots are written to a temporary file and
//! renamed, so a crash during a write never leaves a broken one; the original file is never
//! touched, and a restored design is untitled until saved.
//!
//! Nothing here depends on egui.

use serde_json::{Value, json};
use std::fs::File;
use std::path::{Path, PathBuf};

/// The folder beside ui.json.
pub const FOLDER: &str = "recovery";
/// Seconds without a change before a snapshot is written.
pub const IDLE_SECONDS: f64 = 3.0;
/// Default for the longest time between snapshots while changes keep coming (seconds).
pub const DEFAULT_INTERVAL: f64 = 30.0;
pub const MIN_INTERVAL: f64 = 5.0;
pub const MAX_INTERVAL: f64 = 600.0;

/// What a snapshot is of, saved beside it.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    /// The design's file when it had one (never written to by recovery).
    pub original: Option<PathBuf>,
    /// The design's name (title bar).
    pub name: String,
    /// When the snapshot was written, seconds since 1970.
    pub time: u64,
    /// The process that wrote it (informational; the lock decides).
    pub process: u32,
}

impl Info {
    pub fn to_json(&self) -> Value {
        let mut v = json!({ "name": self.name, "time": self.time, "process": self.process });
        if let Some(p) = &self.original {
            v["original"] = json!(p.to_string_lossy());
        }
        v
    }

    pub fn from_json(v: &Value) -> Option<Info> {
        Some(Info {
            original: v.get("original").and_then(Value::as_str).map(PathBuf::from),
            name: v.get("name")?.as_str()?.to_string(),
            time: v.get("time")?.as_u64()?,
            process: u32::try_from(v.get("process")?.as_u64()?).ok()?,
        })
    }
}

/// A snapshot left by a window that is no longer running.
#[derive(Clone, Debug, PartialEq)]
pub struct Orphan {
    pub id: String,
    pub info: Info,
}

impl Orphan {
    /// `<name>, <file>` as the recovery dialog shows it.
    pub fn title(&self) -> String {
        match &self.original {
            Some(p) => format!("{} ({})", self.info.name, p.display()),
            None => self.info.name.clone(),
        }
    }
}

impl std::ops::Deref for Orphan {
    type Target = Info;
    fn deref(&self) -> &Info {
        &self.info
    }
}

/// One window's snapshot slot: its id and the lock it holds while it runs.
#[derive(Debug)]
pub struct Slot {
    dir: PathBuf,
    id: String,
    _lock: File,
}

impl Slot {
    /// Takes a new slot in `dir` (created if missing), named after this process and `n`.
    pub fn take(dir: &Path, n: u32) -> std::io::Result<Slot> {
        std::fs::create_dir_all(dir)?;
        let id = format!("{}-{n}", std::process::id());
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(format!("{id}.lock")))?;
        lock.try_lock().map_err(std::io::Error::other)?;
        Ok(Slot {
            dir: dir.to_path_buf(),
            id,
            _lock: lock,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes the snapshot now (see [`write_snapshot`]).
    pub fn write(&self, osd: &[u8], info: &Info) -> std::io::Result<()> {
        write_snapshot(&self.dir, &self.id, osd, info)
    }

    /// Removes the snapshot (a normal close, or autosave turned off); the lock stays held.
    pub fn clear(&self) {
        for ext in ["osd", "json"] {
            let _ = std::fs::remove_file(self.dir.join(format!("{}.{ext}", self.id)));
        }
    }

    /// Whether a snapshot is on disk.
    pub fn exists(&self) -> bool {
        self.dir.join(format!("{}.osd", self.id)).is_file()
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.clear();
        let _ = std::fs::remove_file(self.dir.join(format!("{}.lock", self.id)));
    }
}

/// Writes slot `id`'s snapshot in `dir`: `osd` (the project file's bytes) and `info`, each to
/// a temporary file renamed into place. Called off the UI thread.
pub fn write_snapshot(dir: &Path, id: &str, osd: &[u8], info: &Info) -> std::io::Result<()> {
    atomic_write(&dir.join(format!("{id}.osd")), osd)?;
    atomic_write(&dir.join(format!("{id}.json")), info.to_json().to_string().as_bytes())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Snapshots in `dir` whose window has ended (their lock is free or gone), oldest first.
pub fn orphans(dir: &Path) -> Vec<Orphan> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let Some(id) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        if !dir.join(format!("{id}.osd")).is_file() || held(&dir.join(format!("{id}.lock"))) {
            continue;
        }
        let Some(info) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| Info::from_json(&v))
        else {
            continue;
        };
        out.push(Orphan { id, info });
    }
    out.sort_by_key(|o| o.info.time);
    out
}

/// Whether another running window holds this lock.
fn held(lock: &Path) -> bool {
    match File::open(lock) {
        Ok(f) => f.try_lock().is_err(),
        Err(_) => false,
    }
}

/// The project text of an orphan.
pub fn read(dir: &Path, id: &str) -> std::io::Result<String> {
    check_id(id)?;
    std::fs::read_to_string(dir.join(format!("{id}.osd")))
}

/// Removes an orphan's files (Discard).
pub fn discard(dir: &Path, id: &str) -> std::io::Result<()> {
    check_id(id)?;
    if held(&dir.join(format!("{id}.lock"))) {
        return Err(std::io::Error::other("that window is still open"));
    }
    for ext in ["osd", "json", "lock", "tmp"] {
        let p = dir.join(format!("{id}.{ext}"));
        if p.exists() {
            std::fs::remove_file(p)?;
        }
    }
    Ok(())
}

/// Ids are `<process>-<n>`: digits and one dash, never a path.
fn check_id(id: &str) -> std::io::Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 32
        && id.chars().all(|c| c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("no recovery file '{id}'")))
    }
}

/// When to write: after [`IDLE_SECONDS`] without a change, and at least every `interval`
/// seconds while changes keep coming; never for an unchanged design.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timer {
    /// The design revision last written (`None`: nothing written yet).
    written: Option<u64>,
    /// Revision and time of the last change seen.
    changed: Option<(u64, f64)>,
    /// When the first change after the last write was seen.
    since: Option<f64>,
}

impl Timer {
    /// Notes `revision` at `now`; whether a snapshot is due.
    pub fn due(&mut self, revision: u64, now: f64, interval: f64) -> bool {
        if self.written == Some(revision) {
            self.changed = None;
            self.since = None;
            return false;
        }
        match self.changed {
            Some((r, _)) if r == revision => {}
            _ => {
                self.changed = Some((revision, now));
                self.since.get_or_insert(now);
            }
        }
        let (_, at) = self.changed.expect("set above");
        now - at >= IDLE_SECONDS || now - self.since.unwrap_or(now) >= interval
    }

    /// A snapshot of `revision` was written (or the design was saved: nothing to keep).
    pub fn written(&mut self, revision: u64) {
        self.written = Some(revision);
        self.changed = None;
        self.since = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str) -> Info {
        Info {
            original: Some(PathBuf::from("design.osd")),
            name: name.into(),
            time: 7,
            process: std::process::id(),
        }
    }

    #[test]
    fn idle_and_interval() {
        let mut t = Timer::default();
        assert!(!t.due(1, 0.0, 30.0));
        assert!(!t.due(1, 2.9, 30.0));
        assert!(t.due(1, 3.0, 30.0));
        t.written(1);
        assert!(!t.due(1, 100.0, 30.0), "unchanged");
        // Continuous editing: a change every second still writes after the interval.
        let mut wrote = None;
        for k in 0..40 {
            if t.due(2 + k, 100.0 + k as f64, 30.0) {
                wrote = Some(k);
                break;
            }
        }
        assert_eq!(wrote, Some(30));
    }

    #[test]
    fn a_running_window_is_not_an_orphan_and_a_closed_one_leaves_nothing() {
        let tmp = openstitch_testutil::TempDir::new("recovery");
        let dir = tmp.path().join(FOLDER);
        let a = Slot::take(&dir, 1).expect("slot");
        let b = Slot::take(&dir, 2).expect("slot");
        assert_ne!(a.id(), b.id());
        a.write(b"{\"a\":1}", &info("A")).expect("write");
        b.write(b"{\"b\":1}", &info("B")).expect("write");
        assert!(a.exists() && b.exists(), "two windows, two snapshots");
        assert!(orphans(&dir).is_empty(), "both still running");
        assert!(discard(&dir, a.id()).is_err());
        drop(a);
        assert!(orphans(&dir).is_empty(), "a normal close removes the snapshot");
        assert!(!dir.join(format!("{}.tmp", b.id())).exists());
    }

    #[test]
    fn a_crashed_window_is_offered_once_its_lock_is_free() {
        let tmp = openstitch_testutil::TempDir::new("recovery");
        let dir = tmp.path().join(FOLDER);
        // A window that crashed: its files are there, nobody holds its lock.
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("99999-1.osd"), "{\"x\":1}").expect("osd");
        std::fs::write(
            dir.join("99999-1.json"),
            info("Rose").to_json().to_string(),
        )
        .expect("json");
        std::fs::write(dir.join("99999-1.lock"), "").expect("lock");
        let found = orphans(&dir);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Rose");
        assert_eq!(read(&dir, "99999-1").expect("read"), "{\"x\":1}");
        assert!(read(&dir, "../ui").is_err());
        discard(&dir, "99999-1").expect("discard");
        assert!(orphans(&dir).is_empty());
        assert!(std::fs::read_dir(&dir).expect("dir").next().is_none());
    }
}
