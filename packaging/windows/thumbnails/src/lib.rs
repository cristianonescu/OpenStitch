//! Windows Explorer thumbnails for embroidery files (#669).
//!
//! Explorer asks a shell extension registered for a file type for a thumbnail: it creates the
//! extension's COM class, hands it the file's bytes as a stream (`IInitializeWithStream`) and asks
//! for a bitmap (`IThumbnailProvider::GetThumbnail`). A stream carries no file name, so each file
//! extension gets its own class id ([`clsid`]); the class knows which format to read. The
//! drawing is [`openstitch_render::thumbnail`], the same as the macOS Quick Look extension and
//! `openstitch-thumbnailer`.
//!
//! `packaging/windows/openstitch.wxs` registers the DLL for every extension in [`EXTENSIONS`];
//! the test `the_installer_registers_every_extension` keeps the two in step.

use openstitch_formats::FileType;

/// The extensions with a thumbnail: OpenStitch designs, then every machine format OpenStitch
/// reads, in [`FileType::ALL`] order. The index is part of the class id: append, never reorder.
pub fn extensions() -> Vec<&'static str> {
    std::iter::once("osd")
        .chain(FileType::ALL.iter().map(|t| t.extension()))
        .collect()
}

/// The class id of extension number `index`: `4f70656e-5374-6974-6368-54686d620000` + index
/// ("OpenStitchThmb").
pub fn clsid_value(index: usize) -> u128 {
    0x4f70656e_5374_6974_6368_54686d620000_u128 + index as u128
}

/// The class id as the registry writes it: `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}`, upper case.
pub fn clsid_string(index: usize) -> String {
    let v = clsid_value(index);
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}",
        (v >> 96) as u32,
        (v >> 80) as u16,
        (v >> 64) as u16,
        (v >> 48) as u16,
        v & 0xffff_ffff_ffff
    )
}

/// The file's thumbnail as straight-alpha RGBA, `size` × `size` (clamped to the renderer's
/// limits): `(width, height, pixels)`.
pub fn render_rgba(data: &[u8], extension: &str, size: u32) -> Result<(u32, u32, Vec<u8>), String> {
    let pattern = openstitch_render::thumbnail::load_bytes(data, extension)?;
    // Nothing to draw: Explorer keeps the file type's own icon.
    if pattern.count(openstitch_core::Command::Stitch) == 0 {
        return Err("no stitches".into());
    }
    let png = openstitch_render::thumbnail::png(&pattern, size)?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(format!(
            "unexpected PNG {:?} {:?}",
            info.color_type, info.bit_depth
        ));
    }
    buf.truncate(info.buffer_size());
    Ok((info.width, info.height, buf))
}

#[cfg(windows)]
mod com;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .expect("repo root")
    }

    #[test]
    fn class_ids_are_fixed_and_distinct() {
        assert_eq!(clsid_string(0), "{4F70656E-5374-6974-6368-54686D620000}");
        assert_eq!(clsid_string(1), "{4F70656E-5374-6974-6368-54686D620001}");
        let exts = extensions();
        assert_eq!(exts[0], "osd");
        assert_eq!(exts[1], "pes", "FileType::ALL starts with PES");
        let mut ids: Vec<String> = (0..exts.len()).map(clsid_string).collect();
        ids.dedup();
        assert_eq!(ids.len(), exts.len());
    }

    /// The script the issue asks for: every design in the kit renders.
    #[test]
    fn every_kit_design_renders_a_thumbnail() {
        let mut n = 0;
        let mut stack = vec![repo().join("examples").join("sew")];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).expect("kit folder").flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let Some(ext) = p.extension().and_then(|e| e.to_str()) else {
                    continue;
                };
                let ext = ext.to_ascii_lowercase();
                if !extensions().contains(&ext.as_str()) {
                    continue;
                }
                let data = std::fs::read(&p).expect("read");
                let (w, h, px) = render_rgba(&data, &ext, 256)
                    .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                assert_eq!((w, h), (256, 256), "{}", p.display());
                assert!(
                    px.chunks(4).any(|c| c[3] > 0),
                    "{}: nothing drawn",
                    p.display()
                );
                n += 1;
            }
        }
        assert!(n >= 10, "only {n} kit designs found");
    }

    #[test]
    fn the_installer_registers_every_extension() {
        let wxs = std::fs::read_to_string(repo().join("packaging/windows/openstitch.wxs"))
            .expect("openstitch.wxs");
        for (i, ext) in extensions().iter().enumerate() {
            let clsid = clsid_string(i);
            assert!(
                wxs.contains(&format!(
                    "Software\\Classes\\CLSID\\{clsid}\\InprocServer32"
                )),
                "openstitch.wxs does not register the class for .{ext} ({clsid})"
            );
            assert!(
                wxs.contains(&format!(
                    "Software\\Classes\\.{ext}\\ShellEx\\{{e357fccd-a995-4576-b01f-234630154e96}}\" Value=\"{clsid}\""
                )),
                "openstitch.wxs does not hook .{ext} to its thumbnail class"
            );
        }
    }

    /// The DLL draws exactly what `openstitch_render::thumbnail` draws (the same renderer as
    /// Quick Look and `openstitch-thumbnailer`).
    #[test]
    fn the_pixels_are_the_shared_renderers() {
        let pes =
            std::fs::read(repo().join("crates/openstitch-engine/tests/fixtures/badge-golden.pes"))
                .expect("fixture");
        let (w, h, px) = render_rgba(&pes, "pes", 200).expect("render");
        let pattern = openstitch_render::thumbnail::load_bytes(&pes, "pes").expect("load");
        let png = openstitch_render::thumbnail::png(&pattern, 200).expect("png");
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().expect("info");
        let mut buf = vec![0; reader.output_buffer_size().expect("size")];
        let info = reader.next_frame(&mut buf).expect("frame");
        buf.truncate(info.buffer_size());
        assert_eq!((w, h), (info.width, info.height));
        assert_eq!(px, buf, "the same pixels");
    }

    #[test]
    fn a_broken_file_is_an_error_not_a_panic() {
        assert!(render_rgba(b"not a design", "pes", 128).is_err());
        assert!(render_rgba(b"", "dst", 128).is_err());
    }
}
