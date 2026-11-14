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

/// Writes a whole file safely (#1093): the bytes go to a temporary file in the same folder,
/// are flushed to the disk, and the temporary file is renamed over the target. A crash, a power
/// cut or a full disk during the write leaves the earlier file whole; on an error the temporary
/// file is removed. A rename within one folder works on every file system, FAT32 sticks too.
///
/// As `std::fs::write` did: a target that is a link is written where it points (the link
/// stays), a read-only target is refused, the target keeps its permissions, and a missing or
/// read-only folder gives the same error.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    write_with(path, bytes.as_ref(), |file, bytes| {
        use std::io::Write;
        file.write_all(bytes)
    })
}

/// [`write`] with the step that fills the temporary file given, so a test can fail it half-way.
#[cfg(not(target_arch = "wasm32"))]
fn write_with(
    path: &Path,
    bytes: &[u8],
    fill: impl FnOnce(&mut std::fs::File, &[u8]) -> io::Result<()>,
) -> io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    // A link is followed: the file it points to is replaced, not the link.
    let target = match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => std::fs::canonicalize(path)?,
        _ => path.to_path_buf(),
    };
    let old = std::fs::metadata(&target).ok();
    if let Some(m) = &old {
        if m.is_dir() {
            // As `std::fs::write` reports it.
            return std::fs::write(&target, bytes);
        }
        if m.permissions().readonly() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} is read-only", target.display()),
            ));
        }
    }
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = target.with_file_name(format!(
        ".{name}.{}-{}.openstitch-tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    let written = fill(&mut file, bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| match &old {
            Some(m) => std::fs::set_permissions(&tmp, m.permissions()),
            None => Ok(()),
        });
    drop(file);
    let done = written.and_then(|()| std::fs::rename(&tmp, &target));
    if done.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    done
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

    /// #1093: a write that fails half-way leaves the earlier file byte for byte, and no
    /// temporary file behind.
    #[test]
    fn a_write_that_fails_half_way_leaves_the_earlier_file_whole() {
        let dir = openstitch_testutil::TempDir::new("fsio-safe");
        let target = dir.path().join("design.osd");
        let earlier = b"the good design".to_vec();
        std::fs::write(&target, &earlier).expect("earlier");
        let new = vec![7u8; 100_000];
        let failed = super::write_with(&target, &new, |file, bytes| {
            use std::io::Write;
            file.write_all(&bytes[..bytes.len() / 2])?;
            Err(std::io::Error::other("the disk is full"))
        });
        assert_eq!(
            failed.map_err(|e| e.to_string()),
            Err("the disk is full".into())
        );
        assert_eq!(std::fs::read(&target).expect("read"), earlier);
        assert_eq!(names(dir.path()), ["design.osd"], "no temporary file left");
        // A good write replaces it, and leaves nothing else either.
        super::write(&target, &new).expect("write");
        assert_eq!(std::fs::read(&target).expect("read"), new);
        assert_eq!(names(dir.path()), ["design.osd"]);
        // A new file too.
        super::write(&dir.path().join("new.dst"), [1, 2, 3]).expect("new");
        assert_eq!(names(dir.path()), ["design.osd", "new.dst"]);
    }

    /// #1093: the errors stay what `std::fs::write` gave: a missing folder, a read-only file
    /// (left as it was), a folder in the way.
    #[test]
    fn safe_writing_reports_the_same_errors() {
        let dir = openstitch_testutil::TempDir::new("fsio-safe-errors");
        let missing = dir.path().join("no-such-folder").join("a.osd");
        let plain = std::fs::write(&missing, b"x").expect_err("plain");
        let safe = super::write(&missing, b"x").expect_err("safe");
        assert_eq!(safe.kind(), plain.kind());
        assert_eq!(safe.to_string(), plain.to_string());
        let locked = dir.path().join("locked.osd");
        std::fs::write(&locked, b"keep").expect("locked");
        let mut perms = std::fs::metadata(&locked).expect("meta").permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&locked, perms.clone()).expect("read-only");
        let refused = super::write(&locked, b"new").expect_err("read-only");
        assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(std::fs::read(&locked).expect("read"), b"keep");
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&locked, perms).expect("writable again");
        let folder = dir.path().join("folder.osd");
        std::fs::create_dir(&folder).expect("folder");
        let plain = std::fs::write(&folder, b"x").expect_err("plain");
        let safe = super::write(&folder, b"x").expect_err("safe");
        assert_eq!(safe.kind(), plain.kind());
        assert_eq!(names(dir.path()), ["folder.osd", "locked.osd"]);
    }

    /// #1093: writing through a link replaces the file it points to; the link stays.
    #[cfg(unix)]
    #[test]
    fn a_link_stays_a_link() {
        let dir = openstitch_testutil::TempDir::new("fsio-safe-link");
        let real = dir.path().join("real.osd");
        let link = dir.path().join("link.osd");
        std::fs::write(&real, b"old").expect("real");
        std::os::unix::fs::symlink(&real, &link).expect("link");
        super::write(&link, b"new").expect("write");
        assert!(
            std::fs::symlink_metadata(&link)
                .expect("meta")
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read(&real).expect("read"), b"new");
    }

    fn names(dir: &std::path::Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .expect("dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
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
