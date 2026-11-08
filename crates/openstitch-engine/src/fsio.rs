//! File access for the commands that open, save and export designs (#556).
//!
//! On the desktop every function is the `std::fs` call of the same name, so native behaviour
//! is unchanged. A browser (`wasm32`) has no file system: there the files live in memory. The
//! web editor puts the bytes of a file the user picked with [`memfs::put`] before it runs
//! `project.open` or `stitches.import`, and takes what `project.save` or `design.export` wrote
//! with [`memfs::take`] to hand it to the browser as a download.

#[cfg(not(target_arch = "wasm32"))]
use std::{io, path::Path};

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read(path: &Path) -> io::Result<Vec<u8>> {
    std::fs::read(path)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    std::fs::write(path, bytes)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn len(path: &Path) -> io::Result<u64> {
    std::fs::metadata(path).map(|m| m.len())
}

#[cfg(all(not(target_arch = "wasm32"), feature = "formats"))]
pub(crate) fn remove_file(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

#[cfg(target_arch = "wasm32")]
pub(crate) use memfs::{len, read, remove_file, write};

/// A file's size, as `std::fs::metadata(path)?.len()` (#693: the files the user picks or drops
/// in a browser are in memory, so readers of picked files ask here).
pub(crate) struct Meta(u64);

impl Meta {
    pub(crate) fn len(&self) -> u64 {
        self.0
    }
}

pub(crate) fn metadata(path: &std::path::Path) -> std::io::Result<Meta> {
    len(path).map(Meta)
}

/// On the desktop the file commands still write real files at the path they resolved (the same
/// `Access` checks and sandbox), and never touch the in-memory files.
#[cfg(all(test, not(target_arch = "wasm32"), feature = "formats"))]
mod native_tests {
    use crate::Session;
    use serde_json::json;

    #[test]
    fn save_and_export_write_real_files_at_the_given_path() {
        let dir = openstitch_testutil::TempDir::new("fsio-native");
        memfs_clear_and_check(|| {
            let mut s = Session::new(dir.path());
            s.execute(
                "design.new",
                Some(&json!({ "name": "t", "width": 100, "height": 100 })),
            )
            .expect("new");
            s.execute(
                "shape.ellipse",
                Some(&json!({ "cx": 50, "cy": 50, "rx": 20, "ry": 15, "fill": "#1f4fa0" })),
            )
            .expect("rect");
            let osd = dir.path().join("t.osd");
            let dst = dir.path().join("t.dst");
            s.execute(
                "project.save",
                Some(&json!({ "path": osd.display().to_string() })),
            )
            .expect("save");
            s.execute("design.export", Some(&json!({ "path": "t.dst" })))
                .expect("export");
            assert!(std::fs::metadata(&osd).is_ok_and(|m| m.len() > 0));
            assert!(std::fs::metadata(&dst).is_ok_and(|m| m.len() > 0));
            s.execute(
                "project.open",
                Some(&json!({ "path": osd.display().to_string() })),
            )
            .expect("open");
        });
    }

    fn memfs_clear_and_check(f: impl FnOnce()) {
        super::memfs::clear();
        f();
        assert!(
            super::memfs::paths().is_empty(),
            "the desktop never uses the memory files"
        );
    }
}

/// The in-memory files of the browser build: one flat map from path to bytes, per thread (a
/// browser tab runs the editor on one thread). Built natively too, so its tests run everywhere.
pub mod memfs {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io;
    use std::path::{Path, PathBuf};

    thread_local! {
        static FILES: RefCell<BTreeMap<PathBuf, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
    }

    fn missing(path: &Path) -> io::Error {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} not found", path.display()),
        )
    }

    /// Stores a file, replacing one at the same path.
    pub fn put(path: impl Into<PathBuf>, bytes: Vec<u8>) {
        FILES.with(|f| f.borrow_mut().insert(path.into(), bytes));
    }

    /// Removes a file and returns its bytes.
    pub fn take(path: impl AsRef<Path>) -> Option<Vec<u8>> {
        FILES.with(|f| f.borrow_mut().remove(path.as_ref()))
    }

    /// The stored paths, sorted.
    pub fn paths() -> Vec<PathBuf> {
        FILES.with(|f| f.borrow().keys().cloned().collect())
    }

    /// Removes every file.
    pub fn clear() {
        FILES.with(|f| f.borrow_mut().clear());
    }

    pub fn read(path: &Path) -> io::Result<Vec<u8>> {
        FILES.with(|f| f.borrow().get(path).cloned().ok_or_else(|| missing(path)))
    }

    pub fn write(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
        put(path, bytes.as_ref().to_vec());
        Ok(())
    }

    pub fn len(path: &Path) -> io::Result<u64> {
        FILES.with(|f| {
            f.borrow()
                .get(path)
                .map(|b| b.len() as u64)
                .ok_or_else(|| missing(path))
        })
    }

    pub fn remove_file(path: &Path) -> io::Result<()> {
        take(path).map(|_| ()).ok_or_else(|| missing(path))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn files_round_trip_and_are_removed() {
            clear();
            let p = Path::new("/web/a.pes");
            assert!(read(p).is_err());
            put(p, vec![1, 2, 3]);
            assert_eq!(len(p).ok(), Some(3));
            write(Path::new("/web/b.dst"), [9]).ok();
            assert_eq!(paths().len(), 2);
            assert_eq!(read(p).ok(), Some(vec![1, 2, 3]));
            assert!(remove_file(p).is_ok());
            assert!(remove_file(p).is_err());
            assert_eq!(take("/web/b.dst"), Some(vec![9]));
            assert!(paths().is_empty());
        }
    }
}
