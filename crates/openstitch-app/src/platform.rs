//! What differs between the desktop editor and the browser build (#693).
//!
//! On the desktop every function here is a thin pass-through to exactly what the editor called
//! before; the browser build (`wasm32`) gets its own version, because the crate behind the
//! desktop one cannot run in a browser. Phase A1 holds the release-tag check, phase A2 the
//! system clipboard; the file dialogs, preferences and clock follow with their call sites.

/// A clock for timing on the window's thread: `std::time::Instant` on the desktop; a browser's
/// (`web_time`, the page's `performance.now()`), as std's panics there.
#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;
#[cfg(target_arch = "wasm32")]
pub use web_time::Instant;

/// Runs `job` off the window's thread: a new thread on the desktop. A browser page has one
/// thread, so there the job runs at once; its result waits in its channel for the next frame.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_job(job: impl FnOnce() + Send + 'static) {
    std::thread::spawn(job);
}

#[cfg(target_arch = "wasm32")]
pub fn run_job(job: impl FnOnce() + Send + 'static) {
    job();
}

/// A file's bytes when it is at most `max` long (desktop: the file system; browser: the files
/// the page put in memory, `openstitch_engine::fsio::memfs`).
#[cfg(not(target_arch = "wasm32"))]
pub fn read_small(path: &std::path::Path, max: u64) -> Option<Vec<u8>> {
    if std::fs::metadata(path).ok()?.len() > max {
        return None;
    }
    std::fs::read(path).ok()
}

#[cfg(target_arch = "wasm32")]
pub fn read_small(path: &std::path::Path, max: u64) -> Option<Vec<u8>> {
    use openstitch_engine::fsio::memfs;
    if memfs::len(path).ok()? > max {
        return None;
    }
    memfs::read(path).ok()
}

/// An RGBA picture on the system clipboard: `width × height` pixels, 4 bytes each, by rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardImage {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
}

/// Why the browser build has no system clipboard here: a page reads it only asynchronously and
/// after a user gesture, so copy and paste stay inside the editor.
#[cfg(target_arch = "wasm32")]
const NO_CLIPBOARD: &str = "the browser does not give the system clipboard to the editor";

/// The system clipboard's text (desktop: arboard).
#[cfg(not(target_arch = "wasm32"))]
pub fn clipboard_text() -> Result<String, String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .map_err(|e| e.to_string())
}

/// Puts `text` on the system clipboard (desktop: arboard).
#[cfg(not(target_arch = "wasm32"))]
pub fn set_clipboard_text(text: String) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|e| e.to_string())
}

/// The system clipboard's picture (desktop: arboard).
#[cfg(not(target_arch = "wasm32"))]
pub fn clipboard_image() -> Result<ClipboardImage, String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_image())
        .map(|img| ClipboardImage {
            width: img.width,
            height: img.height,
            bytes: img.bytes.into_owned(),
        })
        .map_err(|e| e.to_string())
}

/// Puts a picture on the system clipboard (desktop: arboard).
#[cfg(not(target_arch = "wasm32"))]
pub fn set_clipboard_image(image: ClipboardImage) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| {
            c.set_image(arboard::ImageData {
                width: image.width,
                height: image.height,
                bytes: image.bytes.into(),
            })
        })
        .map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
pub fn clipboard_text() -> Result<String, String> {
    Err(NO_CLIPBOARD.into())
}

#[cfg(target_arch = "wasm32")]
pub fn set_clipboard_text(_text: String) -> Result<(), String> {
    Err(NO_CLIPBOARD.into())
}

#[cfg(target_arch = "wasm32")]
pub fn clipboard_image() -> Result<ClipboardImage, String> {
    Err(NO_CLIPBOARD.into())
}

#[cfg(target_arch = "wasm32")]
pub fn set_clipboard_image(_image: ClipboardImage) -> Result<(), String> {
    Err(NO_CLIPBOARD.into())
}

/// Is `tag` a release tag (`vX.Y.Z`, optionally `-alpha.N`, `-beta.N` or `-rc.N`)? On the
/// desktop, `openstitch_update::is_release_tag`; in the browser, where openstitch-update (an
/// HTTPS client) does not build, the same grammar.
#[cfg(not(target_arch = "wasm32"))]
pub fn is_release_tag(tag: &str) -> bool {
    openstitch_update::is_release_tag(tag)
}

#[cfg(target_arch = "wasm32")]
pub fn is_release_tag(tag: &str) -> bool {
    release_tag_grammar(tag)
}

/// openstitch-update's tag grammar (`version::parse_tag`) without the semver parse that ends
/// it, which every tag passing these checks satisfies. The test below holds the two together.
#[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
fn release_tag_grammar(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else {
        return false;
    };
    let (core, pre) = match rest.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (rest, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 || !parts.iter().all(|p| plain_number(p)) {
        return false;
    }
    match pre {
        None => true,
        Some(pre) => pre
            .split_once('.')
            .is_some_and(|(kind, n)| ["alpha", "beta", "rc"].contains(&kind) && plain_number(n)),
    }
}

/// A decimal number without sign, spaces or leading zeros (other than `0`), fitting a u64.
#[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
fn plain_number(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit())
        && !(s.len() > 1 && s.starts_with('0'))
        && s.parse::<u64>().is_ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_browser_tag_check_agrees_with_openstitch_update() {
        let tags = [
            "v0.1.0",
            "v1.2.3",
            "v0.1.0-alpha.13",
            "v0.2.0-beta.1",
            "v1.0.0-rc.0",
            "v10.20.30-rc.4",
            "v0.1.0-alpha",
            "v0.1.0-alpha.01",
            "v0.1.0-gamma.1",
            "v0.1.0-alpha.1.2",
            "v01.1.0",
            "v1.2",
            "v1.2.3.4",
            "1.2.3",
            "V1.2.3",
            "v1.2.3-",
            "v1.2.3-rc.",
            "v-1.2.3",
            "v1.2.+3",
            "v1. 2.3",
            "v99999999999999999999.0.0",
            "main-abc1234",
            "pr123",
            "",
            "v",
        ];
        for tag in tags {
            assert_eq!(
                super::release_tag_grammar(tag),
                openstitch_update::is_release_tag(tag),
                "{tag:?}"
            );
            assert_eq!(
                super::is_release_tag(tag),
                openstitch_update::is_release_tag(tag)
            );
        }
        assert!(super::release_tag_grammar("v0.1.0-alpha.13"));
        assert!(!super::release_tag_grammar("main-abc1234"));
    }
}

/// Where an editor with no preferences file keeps its preferences (#797): the browser page
/// registers its `localStorage` here; the desktop has `ui.json` and registers nothing. Both
/// functions are the store's own, so a failing store (a private window, storage turned off)
/// reads as empty and writes as nothing.
#[derive(Clone, Copy)]
pub struct PrefsStore {
    pub read: fn() -> Option<String>,
    pub write: fn(&str),
}

thread_local! {
    static PREFS_STORE: std::cell::Cell<Option<PrefsStore>> = const { std::cell::Cell::new(None) };
}

/// Registers the preferences store (the web page does, before the editor starts).
pub fn set_prefs_store(store: PrefsStore) {
    PREFS_STORE.with(|s| s.set(Some(store)));
}

/// The stored preferences' JSON, if a store is registered and holds any.
pub fn stored_prefs() -> Option<String> {
    PREFS_STORE.with(|s| s.get()).and_then(|s| (s.read)())
}

/// Writes the preferences' JSON to the registered store; false when there is none.
pub fn store_prefs(text: &str) -> bool {
    match PREFS_STORE.with(|s| s.get()) {
        Some(s) => {
            (s.write)(text);
            true
        }
        None => false,
    }
}
