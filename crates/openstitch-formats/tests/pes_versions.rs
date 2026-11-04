//! PES versions 4, 5 and 7–10 (#374), read through their headers. No PE-DESIGN file of these
//! versions is in the repo yet, so each test file is pyembroidery's version 6 file
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
