//! The Rust side of the macOS Quick Look thumbnail extension (#557, #1182).
//!
//! The extension (`../ThumbnailProvider.swift`, `../Renderer.swift`) links this static library
//! and calls [`openstitch_ql_thumbnail`] in its own process. It cannot run a helper program:
//! Quick Look's sandbox for thumbnail extensions refuses to start any process (#1182). The
//! drawing is [`openstitch_render::thumbnail`], the same pixels as `openstitch-thumbnailer`,
//! `stitchtool thumbnail` and the Windows Explorer handler.
//!
//! The C declarations are in `../openstitch_quicklook.h`; keep the two in step.

use openstitch_render::thumbnail;
use std::ffi::{CStr, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// The PNG was drawn; the buffer holds it.
pub const OK: i32 = 0;
/// The file could not be read or drawn; the buffer holds a UTF-8 message.
pub const FAILED: i32 = 1;

/// Bytes handed to Swift. Swift gives every buffer back to [`openstitch_ql_free`].
#[repr(C)]
pub struct OpenStitchBuffer {
    pub ptr: *mut u8,
    pub len: usize,
}

impl OpenStitchBuffer {
    #[cfg(test)]
    const EMPTY: Self = Self {
        ptr: std::ptr::null_mut(),
        len: 0,
    };

    fn from_vec(bytes: Vec<u8>) -> Self {
        let len = bytes.len();
        let ptr = Box::into_raw(bytes.into_boxed_slice()).cast::<u8>();
        Self { ptr, len }
    }
}

/// A `size` × `size` PNG of a design file's bytes, its format named by `extension` (`pes`,
/// `.PES`, `osd`, ...): the safe core of [`openstitch_ql_thumbnail`]. A panic in the renderer
/// is an error here, never an abort of the extension.
pub fn render(data: &[u8], extension: &str, size: u32) -> Result<Vec<u8>, String> {
    let extension = extension.trim_start_matches('.');
    catch_unwind(AssertUnwindSafe(|| {
        thumbnail::load_bytes(data, extension).and_then(|p| thumbnail::png(&p, size))
    }))
    .unwrap_or_else(|_| Err("the renderer failed".into()))
}

/// Draws the thumbnail of `len` bytes at `data`, a file in the format `extension` (a
/// NUL-terminated UTF-8 string), `size` pixels square (clamped to 16-1024). Returns [`OK`] with
/// the PNG in `*out`, or [`FAILED`] with a message in `*out`. `*out` is always set; the caller
/// frees it with [`openstitch_ql_free`].
///
/// # Safety
/// `data` points to `len` readable bytes (or is null with `len` 0), `extension` is null or a
/// NUL-terminated string, and `out` points to writable memory for one [`OpenStitchBuffer`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn openstitch_ql_thumbnail(
    data: *const u8,
    len: usize,
    extension: *const c_char,
    size: u32,
    out: *mut OpenStitchBuffer,
) -> i32 {
    if out.is_null() {
        return FAILED;
    }
    let bytes: &[u8] = if data.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: the caller promises `len` readable bytes at `data`.
        unsafe { std::slice::from_raw_parts(data, len) }
    };
    let extension = if extension.is_null() {
        ""
    } else {
        // SAFETY: the caller promises a NUL-terminated string.
        unsafe { CStr::from_ptr(extension) }.to_str().unwrap_or("")
    };
    let (code, buffer) = match render(bytes, extension, size) {
        Ok(png) => (OK, png),
        Err(message) => (FAILED, message.into_bytes()),
    };
    // SAFETY: the caller promises `out` is writable.
    unsafe { out.write(OpenStitchBuffer::from_vec(buffer)) };
    code
}

/// Frees a buffer [`openstitch_ql_thumbnail`] returned. A null buffer is ignored.
///
/// # Safety
/// `buffer` came from [`openstitch_ql_thumbnail`] and has not been freed yet.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn openstitch_ql_free(buffer: OpenStitchBuffer) {
    if buffer.ptr.is_null() {
        return;
    }
    // SAFETY: `ptr` and `len` are a boxed slice from `OpenStitchBuffer::from_vec`.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(buffer.ptr, buffer.len)) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::path::Path;
    use std::time::{Duration, Instant};

    fn kit() -> Vec<std::path::PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../examples/sew");
        let mut files = Vec::new();
        let mut dirs = vec![root];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).expect("examples/sew") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "pes" || e == "osd") {
                    files.push(path);
                }
            }
        }
        assert!(!files.is_empty());
        files
    }

    fn call(data: &[u8], extension: &str, size: u32) -> (i32, Vec<u8>) {
        let ext = CString::new(extension).expect("ext");
        let mut out = OpenStitchBuffer::EMPTY;
        // SAFETY: valid slice, string and output.
        let code = unsafe {
            openstitch_ql_thumbnail(data.as_ptr(), data.len(), ext.as_ptr(), size, &mut out)
        };
        assert!(!out.ptr.is_null());
        // SAFETY: `out` was just filled by openstitch_ql_thumbnail.
        let bytes = unsafe { std::slice::from_raw_parts(out.ptr, out.len) }.to_vec();
        // SAFETY: freed once.
        unsafe { openstitch_ql_free(out) };
        (code, bytes)
    }

    #[test]
    fn every_kit_design_gives_the_shared_renderers_png() {
        for path in kit() {
            let data = std::fs::read(&path).expect("read");
            let ext = path.extension().and_then(|e| e.to_str()).expect("ext");
            let (code, png) = call(&data, ext, 256);
            assert_eq!(code, OK, "{}", path.display());
            let expected = thumbnail::load(&path)
                .and_then(|p| thumbnail::png(&p, 256))
                .expect("renderer");
            assert_eq!(png, expected, "{}", path.display());
        }
    }

    #[test]
    fn the_extension_may_have_a_dot_and_any_case() {
        let path = kit()
            .into_iter()
            .find(|p| p.extension().is_some_and(|e| e == "pes"));
        let data = std::fs::read(path.expect("a kit .pes")).expect("read");
        assert_eq!(call(&data, ".PES", 64).0, OK);
    }

    #[test]
    fn malformed_unknown_and_empty_input_fail_quickly_with_a_message() {
        let started = Instant::now();
        for (data, ext) in [
            (&b"#PES0001 not a design"[..], "pes"),
            (&b"junk"[..], "osd"),
            (&b"junk"[..], "txt"),
            (&b""[..], "pes"),
            (&b""[..], ""),
        ] {
            let (code, message) = call(data, ext, 256);
            assert_eq!(code, FAILED, "{ext}");
            assert!(!message.is_empty(), "{ext}");
            assert!(String::from_utf8(message).is_ok());
        }
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_truncated_or_huge_pes_fails_without_hanging() {
        let path = kit()
            .into_iter()
            .find(|p| p.extension().is_some_and(|e| e == "pes"));
        let data = std::fs::read(path.expect("a kit .pes")).expect("read");
        let started = Instant::now();
        for cut in [16, data.len() / 3, data.len() / 2, data.len() - 1] {
            let (code, _) = call(&data[..cut], "pes", 256);
            assert!(code == OK || code == FAILED);
        }
        // Over the 64 MiB cap: refused before decoding.
        let huge = vec![0u8; (thumbnail::MAX_INPUT + 1) as usize];
        assert_eq!(call(&huge, "pes", 256).0, FAILED);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn null_pointers_are_refused_not_dereferenced() {
        // SAFETY: null output is checked before any write.
        let code = unsafe {
            openstitch_ql_thumbnail(
                std::ptr::null(),
                0,
                std::ptr::null(),
                64,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(code, FAILED);
        let mut out = OpenStitchBuffer::EMPTY;
        // SAFETY: null data with len 0 and a null extension are allowed.
        let code =
            unsafe { openstitch_ql_thumbnail(std::ptr::null(), 0, std::ptr::null(), 64, &mut out) };
        assert_eq!(code, FAILED);
        // SAFETY: freed once; then a null buffer is ignored.
        unsafe { openstitch_ql_free(out) };
        unsafe { openstitch_ql_free(OpenStitchBuffer::EMPTY) };
    }
}
