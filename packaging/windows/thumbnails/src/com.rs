//! The COM side: a class factory per extension, the thumbnail provider, and the two functions
//! Explorer calls in the DLL (`DllGetClassObject`, `DllCanUnloadNow`).

use crate::{clsid_value, extensions, render_rgba};
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_POINTER, S_FALSE, S_OK,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl, IStream};
use windows::Win32::UI::Shell::PropertiesSystem::{
    IInitializeWithStream, IInitializeWithStream_Impl,
};
use windows::Win32::UI::Shell::{
    IThumbnailProvider, IThumbnailProvider_Impl, WTS_ALPHATYPE, WTSAT_ARGB,
};
use windows_core::{BOOL, GUID, HRESULT, IUnknown, Interface, Ref, implement};

/// Objects alive in this DLL: Explorer may unload it when none are.
static LIVE: AtomicUsize = AtomicUsize::new(0);

/// Largest file read from the stream (the renderer's own limit).
const MAX_BYTES: usize = openstitch_render::thumbnail::MAX_INPUT as usize;

#[implement(IInitializeWithStream, IThumbnailProvider)]
struct Provider {
    extension: &'static str,
    data: Mutex<Option<Vec<u8>>>,
}

impl Provider {
    fn new(extension: &'static str) -> Self {
        LIVE.fetch_add(1, Ordering::SeqCst);
        Provider {
            extension,
            data: Mutex::new(None),
        }
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

impl IInitializeWithStream_Impl for Provider_Impl {
    fn Initialize(&self, pstream: Ref<IStream>, _grfmode: u32) -> windows_core::Result<()> {
        let stream = pstream.ok()?;
        let mut data = Vec::new();
        let mut chunk = vec![0u8; 1 << 16];
        loop {
            let mut read = 0u32;
            // SAFETY: `chunk` is valid for its length; `read` receives the count.
            #[allow(unsafe_code)]
            let hr = unsafe {
                stream.Read(
                    chunk.as_mut_ptr().cast::<c_void>(),
                    chunk.len() as u32,
                    Some(&mut read),
                )
            };
            hr.ok()?;
            if read == 0 {
                break;
            }
            data.extend_from_slice(&chunk[..read as usize]);
            if data.len() > MAX_BYTES {
                return Err(E_FAIL.into());
            }
        }
        *self
            .data
            .lock()
            .map_err(|_| windows_core::Error::from(E_FAIL))? = Some(data);
        Ok(())
    }
}

impl IThumbnailProvider_Impl for Provider_Impl {
    fn GetThumbnail(
        &self,
        cx: u32,
        phbmp: *mut HBITMAP,
        pdwalpha: *mut WTS_ALPHATYPE,
    ) -> windows_core::Result<()> {
        if phbmp.is_null() || pdwalpha.is_null() {
            return Err(E_POINTER.into());
        }
        let guard = self
            .data
            .lock()
            .map_err(|_| windows_core::Error::from(E_FAIL))?;
        let data = guard.as_ref().ok_or(windows_core::Error::from(E_FAIL))?;
        let (w, h, rgba) =
            render_rgba(data, self.extension, cx).map_err(|_| windows_core::Error::from(E_FAIL))?;
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                // Negative: top-down rows, as the pixels are.
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: `bmi` describes a w × h 32-bit bitmap; `bits` receives its pixel memory.
        #[allow(unsafe_code)]
        let bitmap = unsafe { CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)? };
        if bits.is_null() {
            return Err(E_FAIL.into());
        }
        // SAFETY: CreateDIBSection gave w × h × 4 bytes at `bits`.
        #[allow(unsafe_code)]
        let out = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), rgba.len()) };
        // Explorer takes premultiplied BGRA.
        let (out, _) = out.as_chunks_mut::<4>();
        let (px, _) = rgba.as_chunks::<4>();
        for (o, p) in out.iter_mut().zip(px) {
            let a = u16::from(p[3]);
            let pm = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
            *o = [pm(p[2]), pm(p[1]), pm(p[0]), p[3]];
        }
        // SAFETY: both pointers were checked non-null; Explorer owns the bitmap from here.
        #[allow(unsafe_code)]
        unsafe {
            *phbmp = bitmap;
            *pdwalpha = WTSAT_ARGB;
        }
        Ok(())
    }
}

#[implement(IClassFactory)]
struct Factory {
    extension: &'static str,
}

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> windows_core::Result<()> {
        if ppvobject.is_null() {
            return Err(E_POINTER.into());
        }
        // SAFETY: checked non-null.
        #[allow(unsafe_code)]
        unsafe {
            *ppvobject = std::ptr::null_mut();
        }
        if punkouter.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let provider: IUnknown = Provider::new(self.extension).into();
        // SAFETY: `riid` and `ppvobject` come from the caller, as COM defines.
        #[allow(unsafe_code)]
        unsafe {
            provider.query(riid, ppvobject).ok()
        }
    }

    fn LockServer(&self, _flock: BOOL) -> windows_core::Result<()> {
        Ok(())
    }
}

/// The extension a class id stands for.
fn extension_of(clsid: &GUID) -> Option<&'static str> {
    let v = clsid.to_u128();
    extensions()
        .into_iter()
        .enumerate()
        .find(|(i, _)| clsid_value(*i) == v)
        .map(|(_, e)| e)
}

/// COM entry point: the class factory for one of our class ids.
///
/// # Safety
/// Called by COM with valid pointers.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if ppv.is_null() || rclsid.is_null() || riid.is_null() {
        return E_POINTER;
    }
    // SAFETY: checked non-null; COM passes valid pointers.
    unsafe {
        *ppv = std::ptr::null_mut();
        let Some(extension) = extension_of(&*rclsid) else {
            return CLASS_E_CLASSNOTAVAILABLE;
        };
        let factory: IClassFactory = Factory { extension }.into();
        factory.query(riid, ppv)
    }
}

/// COM entry point: whether Explorer may unload the DLL.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if LIVE.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Gdi::{BITMAP, DeleteObject, GetObjectW, HGDIOBJ};
    use windows::Win32::UI::Shell::SHCreateMemStream;

    /// Explorer's path, without the registry: class factory → provider → stream → bitmap.
    #[test]
    #[allow(unsafe_code)]
    fn explorer_gets_a_bitmap_for_a_pes_file() {
        let pes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../crates/openstitch-engine/tests/fixtures/badge-golden.pes"),
        )
        .expect("fixture");
        let index = extensions().iter().position(|e| *e == "pes").expect("pes");
        let clsid = GUID::from_u128(clsid_value(index));
        unsafe {
            let mut factory: *mut c_void = std::ptr::null_mut();
            let hr = DllGetClassObject(&clsid, &IClassFactory::IID, &mut factory);
            assert_eq!(hr, S_OK);
            let factory = IClassFactory::from_raw(factory);
            let init: IInitializeWithStream = factory.CreateInstance(None).expect("provider");
            let stream = SHCreateMemStream(Some(&pes)).expect("stream");
            init.Initialize(&stream, 0).expect("initialize");
            assert_eq!(DllCanUnloadNow(), S_FALSE, "a provider is alive");
            let thumbs: IThumbnailProvider = init.cast().expect("IThumbnailProvider");
            let mut bitmap = HBITMAP::default();
            let mut alpha = WTS_ALPHATYPE::default();
            thumbs
                .GetThumbnail(256, &mut bitmap, &mut alpha)
                .expect("thumbnail");
            assert_eq!(alpha, WTSAT_ARGB);
            let mut info = BITMAP::default();
            let n = GetObjectW(
                HGDIOBJ(bitmap.0),
                std::mem::size_of::<BITMAP>() as i32,
                Some((&mut info as *mut BITMAP).cast()),
            );
            assert!(n > 0);
            assert_eq!(
                (info.bmWidth, info.bmHeight, info.bmBitsPixel),
                (256, 256, 32)
            );
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            drop(thumbs);
            drop(init);
        }
        assert_eq!(DllCanUnloadNow(), S_OK, "nothing alive any more");
    }

    #[test]
    #[allow(unsafe_code)]
    fn an_unknown_class_is_refused() {
        let clsid = GUID::from_u128(1);
        let mut ppv: *mut c_void = std::ptr::null_mut();
        let hr = unsafe { DllGetClassObject(&clsid, &IClassFactory::IID, &mut ppv) };
        assert_eq!(hr, CLASS_E_CLASSNOTAVAILABLE);
        assert!(ppv.is_null());
    }
}
