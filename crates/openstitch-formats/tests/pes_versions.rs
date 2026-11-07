//! PES versions 4, 5 and 7–10 (#374). The last tests read and write PE-DESIGN 11.50's own Save
//! As files (fixtures/pe11/captures-2026-11-07/pes-versions). The first ones read pyembroidery's version 6 file
//! (`oracle_v6_chart.pes`, with metadata, a chart thread table and a stitch group) with its
//! header re-laid in the version's layout as pyembroidery 1.5.1's reader documents it: the
//! fixed bytes after the metadata (v5 24, v6/v7 36, v8 38, v9/v10 14 + hoop name + 30/38),
//! the image file name, then 24/24/26/34 bytes before the pattern counts. Version 4 ends after
//! the metadata. Each must read as the version 6 file reads: stitches, threads, metadata, the
//! stitch group. Internal regression, not PE-DESIGN evidence (docs/FILE_FORMATS.md).

mod common;

use common::data;
use openstitch_formats::pes;

/// Offset just after the five metadata strings of a v4+ header.
fn metadata_end(b: &[u8]) -> usize {
    let mut at = 12 + 4; // signature, PEC offset, u16 hoop, two digits
    for _ in 0..5 {
        at += 1 + usize::from(b[at]);
    }
    at
}

/// The v6 file `v6` laid out as `version`; `hoop` names the hoop for v9/v10.
fn relaid(v6: &[u8], version: &str) -> Vec<u8> {
    let m = metadata_end(v6);
    // v6 after the metadata: 36 fixed bytes, an empty image name, 24 bytes, then the counts.
    assert_eq!(v6[m + 36], 0, "empty image name");
    let rest = &v6[m + 36 + 1 + 24..];
    let mut out = Vec::new();
    out.extend_from_slice(format!("#PES{version}").as_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&v6[12..m]);
    if version == "0040" {
        // Version 4: the metadata, then (pyembroidery reads no more) the PEC section.
        let pec = u32::from_le_bytes(v6[8..12].try_into().expect("u32")) as usize;
        let at = out.len() as u32;
        out[8..12].copy_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&v6[pec..]);
        return out;
    }
    let (first, hoop_then, before_counts) = match version {
        "0050" => (24, None, 24),
        "0070" => (36, None, 24),
        "0080" => (38, None, 26),
        "0090" => (14, Some(30), 34),
        "0100" => (14, Some(38), 34),
        _ => unreachable!(),
    };
    out.extend(std::iter::repeat_n(0, first));
    if let Some(after) = hoop_then {
        out.push(8);
        out.extend_from_slice(b"nv880e-S");
        out.extend(std::iter::repeat_n(0, after));
    }
    out.push(0); // image file name
    out.extend(std::iter::repeat_n(0, before_counts));
    let shift = out.len() as i64 - (m + 36 + 1 + 24) as i64;
    out.extend_from_slice(rest);
    let pec = u32::from_le_bytes(v6[8..12].try_into().expect("u32")) as i64 + shift;
    out[8..12].copy_from_slice(&(pec as u32).to_le_bytes());
    out
}

#[test]
fn versions_5_and_7_to_10_read_as_version_6() {
    let v6 = data("oracle_v6_chart.pes");
    let want = pes::read(&v6).expect("v6");
    let want_objects = pes::read_objects(&v6).expect("v6 objects");
    assert!(!want_objects.threads.is_empty() && !want_objects.objects.is_empty());
    for version in ["0050", "0070", "0080", "0090", "0100"] {
        let b = relaid(&v6, version);
        let got = pes::read(&b).unwrap_or_else(|e| panic!("{version}: {e}"));
        assert_eq!(got.stitches, want.stitches, "{version}");
        assert_eq!(got.threads, want.threads, "{version}");
        let o = pes::read_objects(&b).expect("objects");
        assert_eq!(o.version, version);
        assert_eq!(o.threads, want_objects.threads, "{version}");
        assert_eq!(o.objects, want_objects.objects, "{version}");
        assert_eq!(o.unparsed, None, "{version}");
        for key in ["name", "category", "author", "keywords", "comments"] {
            assert_eq!(
                o.metadata.get(key),
                want_objects.metadata.get(key),
                "{version} {key}"
            );
        }
        assert_eq!(
            o.metadata.get("pes.hoopName").map(String::as_str),
            matches!(version, "0090" | "0100").then_some("nv880e-S"),
            "{version}"
        );
    }
}

#[test]
fn version_4_reads_its_metadata_and_stitches() {
    let v6 = data("oracle_v6_chart.pes");
    let want = pes::read(&v6).expect("v6");
    let b = relaid(&v6, "0040");
    let got = pes::read(&b).expect("v4");
    assert_eq!(got.stitches, want.stitches);
    // No thread table before version 5: the PEC chart's colours.
    assert_eq!(got.threads.len(), want.threads.len());
    let o = pes::read_objects(&b).expect("objects");
    assert_eq!(o.version, "0040");
    let w = pes::read_objects(&v6).expect("v6 objects");
    for key in ["name", "category", "author", "keywords", "comments"] {
        assert_eq!(o.metadata.get(key), w.metadata.get(key), "{key}");
    }
}

/// The PE-DESIGN Save As file `name` (fixtures/pe11/captures-2026-11-07/pes-versions).
fn save_as(name: &str) -> Vec<u8> {
    data(&format!("pe11/captures-2026-11-07/pes-versions/{name}.pes"))
}

/// Where the PEC section's thumbnails end: the addendum starts there.
fn pec_end(b: &[u8]) -> usize {
    let pec = u32::from_le_bytes(b[8..12].try_into().expect("u32")) as usize;
    let g = pec + 512;
    let graphic = usize::from(b[g + 2]) | usize::from(b[g + 3]) << 8 | usize::from(b[g + 4]) << 16;
    let colours = usize::from(b[pec + 48]) + 1;
    g + graphic + (colours + 1) * 228
}

/// `b` with its pictures blanked: the PEC section's thumbnails (PE-DESIGN draws them with its
/// own renderer) and the 144 bytes after the PEC section that PE-DESIGN gives each thread (the
/// thread's number in its own font; OpenStitch leaves them blank, as in its `#PES0110` files).
fn blank_thread_pictures(b: &[u8]) -> Vec<u8> {
    let mut b = b.to_vec();
    let pec = u32::from_le_bytes(b[8..12].try_into().expect("u32")) as usize;
    let colours = usize::from(b[pec + 48]) + 1;
    let end = pec_end(&b);
    b[end - (colours + 1) * 228..end].fill(0);
    let at = end + 128;
    if b.len() >= at + colours * 144 && &b[..8] != b"#PES0040" {
        b[at..at + colours * 144].fill(0);
    }
    b
}

/// The pattern of a Save As file placed where PE-DESIGN put it: its stitch group's corner, less
/// the file's page offset of 50 mm. `turned`: the file's machine stitches are turned for a
/// hoop with Rotate 90 Degrees, so they are turned back first (`pec::turned_for_hoop`).
fn placed(b: &[u8], turned: bool, file: &str) -> openstitch_core::StitchPattern {
    let mut p = pes::read(b).expect("read");
    // The header's own strings (the reader names an unnamed design after its PEC label), and
    // the PEC label PE-DESIGN gives a file: its name's first 8 characters.
    let header = pes::read_objects(b).map(|o| o.metadata).unwrap_or_default();
    for key in ["name", "category", "author", "keywords", "comments"] {
        match header.get(key) {
            Some(v) => p.metadata.insert(key.into(), v.clone()),
            None => p.metadata.remove(key),
        };
    }
    p.metadata
        .insert(openstitch_formats::pec::LABEL_KEY.into(), file.into());
    if turned {
        let r = p.bounds();
        let (cx, cy) = ((r.min_x + r.max_x) / 2.0, (r.min_y + r.max_y) / 2.0);
        for s in &mut p.stitches {
            let (dx, dy) = (s.x - cx, s.y - cy);
            s.x = cx + dy;
            s.y = cy - dx;
        }
    }
    let at = b.windows(7).position(|w| w == b"CEmbOne").expect("CEmbOne") + 7;
    let x0 = f64::from(i16::from_le_bytes([b[at], b[at + 1]])) - 500.0;
    let y0 = f64::from(i16::from_le_bytes([b[at + 2], b[at + 3]])) - 500.0;
    let (mx, my) = p
        .stitches
        .iter()
        .fold((f64::MAX, f64::MAX), |(x, y), s| (x.min(s.x), y.min(s.y)));
    for s in &mut p.stitches {
        s.x += x0 - mx;
        s.y += y0 - my;
    }
    p
}

const SAVE_AS: [&str; 6] = ["4", "5", "7", "8", "9", "10"];

/// PE-DESIGN's own Save As files of a stitch-data design (our version 6 file re-saved by PE,
/// compat pes-design-properties), read and written again in their version, come out byte for
/// byte: header, thread table, stitch group, trailer, PEC section and addendum.
#[test]
fn pe_design_save_as_versions_rewrite_byte_for_byte() {
    for v in SAVE_AS {
        let pe = save_as(&format!("props-v{v}"));
        let options = pes::WriteOptions::parse(v).expect("version");
        let ours =
            pes::write_with(&placed(&pe, false, &format!("props-v{v}")), &options).expect("write");
        // The metadata strings are the source's: OpenStitch's v6 file of 2026-10 wrote UTF-8
        // and bare line feeds, which PE-DESIGN kept; OpenStitch now writes Windows-1252 and
        // CR LF (pes::encode_header_string). Everything else must match.
        let (a, b) = (metadata_end(&ours), metadata_end(&pe));
        assert_eq!(ours[..8], pe[..8], "v{v}");
        assert_eq!(ours.len() - a, pe.len() - b, "v{v}");
        let at = |x: &[u8]| u32::from_le_bytes(x[8..12].try_into().expect("u32")) as usize;
        assert_eq!(at(&ours) - a, at(&pe) - b, "v{v}: PEC offset");
        assert_eq!(
            blank_thread_pictures(&ours)[a..],
            blank_thread_pictures(&pe)[b..],
            "v{v}"
        );
    }
    for bad in ["4t", "10t", "3", "11t"] {
        assert_eq!(pes::WriteOptions::parse(bad), None, "{bad}");
    }
}

/// The 130 × 180 hoop turned 90 degrees (fixtures/pe11/hoops/s-130x180-rot90): everything
/// from the thread table on matches PE-DESIGN's file; the header's hoop fields too, in each
/// version's way of writing a turned hoop. Its display fields (grid, axes) are the source
/// design's and stay OpenStitch's defaults.
#[test]
fn a_turned_hoop_is_written_as_each_version_writes_it() {
    for v in SAVE_AS {
        let pe = save_as(&format!("rot-v{v}"));
        let options = pes::WriteOptions {
            hoop_mm: Some((130, 180)),
            hoop_rotated: true,
            ..pes::WriteOptions::parse(v).expect("version")
        };
        let ours = blank_thread_pictures(
            &pes::write_with(&placed(&pe, true, &format!("rot-v{v}")), &options).expect("write"),
        );
        let pe = blank_thread_pictures(&pe);
        let tail = |b: &[u8]| {
            let at = b.windows(7).position(|w| w == b"CEmbOne").expect("CEmbOne") - 10;
            let pec = u32::from_le_bytes(b[8..12].try_into().expect("u32")) as usize;
            (b[at..pec].to_vec(), b[pec..].to_vec())
        };
        assert_eq!(tail(&ours), tail(&pe), "v{v}");
        let m = metadata_end(&pe);
        let hoop = |b: &[u8]| -> Vec<u16> {
            (0..5)
                .map(|k| u16::from_le_bytes([b[m + 2 * k], b[m + 2 * k + 1]]))
                .collect()
        };
        match v {
            "4" | "5" => assert_eq!(hoop(&ours)[..4], [0, 180, 130, 1], "v{v}"),
            "7" => assert_eq!(hoop(&ours), [0, 0, 180, 130, 0], "v{v}"),
            "8" => assert_eq!(hoop(&ours), [0, 0, 130, 180, 1], "v{v}"),
            _ => {}
        }
        if matches!(v, "4" | "5" | "7" | "8") {
            assert_eq!(hoop(&ours), hoop(&pe), "v{v}");
        }
    }
}

/// The Save As files of editable designs (text, two shapes in two colours, a 200 × 200 page),
/// v7–v10: they read as their stitches, and OpenStitch's rewrite reads back the same.
#[test]
fn editable_designs_saved_as_old_versions_read_and_round_trip() {
    for name in ["text", "two", "page"] {
        for v in ["7", "8", "9", "10"] {
            let pe = save_as(&format!("{name}-v{v}"));
            let p = pes::read(&pe).unwrap_or_else(|e| panic!("{name}-v{v}: {e}"));
            assert!(p.stitches.len() > 100, "{name}-v{v}");
            let o = pes::read_objects(&pe).expect("objects");
            assert_eq!(o.version, format!("{:0>3}0", v), "{name}-v{v}");
            assert!(!o.threads.is_empty(), "{name}-v{v}");
            let options = pes::WriteOptions {
                custom_page: true,
                ..pes::WriteOptions::parse(v).expect("version")
            };
            let back = pes::read(&pes::write_with(&p, &options).expect("write")).expect("read");
            // The PEC section's stitches are relative to the first one.
            let shape = |q: &openstitch_core::StitchPattern| -> Vec<(f64, f64)> {
                let (x0, y0) = (q.stitches[0].x, q.stitches[0].y);
                q.stitches.iter().map(|s| (s.x - x0, s.y - y0)).collect()
            };
            assert_eq!(shape(&back), shape(&p), "{name}-v{v}");
            // The thread table may list a colour no block sews (text: black); the PEC blocks'
            // threads come back.
            assert!(!back.threads.is_empty(), "{name}-v{v}");
            assert_eq!(back.threads, p.threads[..back.threads.len()], "{name}-v{v}");
        }
    }
}
