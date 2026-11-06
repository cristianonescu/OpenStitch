//! Test helpers shared by the workspace's tests. Every crate takes this as a dev-dependency only.
//!
//! [`TempDir`] is a fresh folder under the OS temp directory that is removed when the guard is
//! dropped, so test runs leave no `openstitch-*` folders behind (#366). Set
//! `OPENSTITCH_KEEP_TEMP=1` to keep the folders of failing tests for a look afterwards; their
//! paths are printed to stderr.

pub mod mock_http;

use std::ffi::OsStr;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The environment variable that keeps a failing test's folders.
pub const KEEP_ENV: &str = "OPENSTITCH_KEEP_TEMP";

/// A temporary folder, `<temp>/openstitch-<label>-<pid>-<n>-<nanos>`, created on construction
/// and removed with everything in it on drop. Derefs to [`Path`], so `dir.join("a.osd")` works.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a new, empty folder. `label` names the crate or test (`"engine"`, `"mcp-tests"`)
    /// so a kept folder says where it came from. Panics if the folder cannot be created.
    pub fn new(label: &str) -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let path = std::env::temp_dir().join(format!(
            "openstitch-{label}-{}-{n}-{nanos}",
            std::process::id()
        ));
        if let Err(e) = std::fs::create_dir_all(&path) {
            panic!("creating {}: {e}", path.display());
        }
        TempDir { path }
    }

    /// Like [`TempDir::new`], with the path canonicalized, for tests that compare it with paths
    /// the engine resolved (macOS's `/var` is a link to `/private/var`; Windows gives `\\?\`
    /// paths and long names for `RUNNER~1`).
    pub fn canonical(label: &str) -> TempDir {
        let mut dir = TempDir::new(label);
        match dir.path.canonicalize() {
            Ok(path) => dir.path = path,
            Err(e) => panic!("canonicalizing {}: {e}", dir.path.display()),
        }
        dir
    }

    /// The folder's path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

/// Also gives `PathBuf: From<&TempDir>` (std's blanket impl), so `Session::new(&dir)` works.
/// There is deliberately no `From<TempDir>`: moving the guard into a `PathBuf` would delete the
/// folder at once.
impl AsRef<OsStr> for TempDir {
    fn as_ref(&self) -> &OsStr {
        self.path.as_os_str()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if std::thread::panicking() && std::env::var_os(KEEP_ENV).is_some_and(|v| !v.is_empty()) {
            eprintln!("{KEEP_ENV}: kept {}", self.path.display());
            return;
        }
        // Best effort: a file still open on Windows must not turn a passing test into a panic.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
