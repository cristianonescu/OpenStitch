//! PES versions 4, 5 and 7–10 as PE-DESIGN 11.50's Save As writes them (#374). PE-DESIGN offers
//! these versions for stitch data only: below Ver.11.0 it asks "OK to save the data as stitch
//! data?" and writes the design as one `CEmbOne` stitch group, with no editable objects; a
//! design with editable objects is offered Ver.6.0 and up only. So OpenStitch writes them from
//! the stitches, as the version 6 writer does, in the layouts of PE-DESIGN's own files
//! (tests/fixtures/pe11/captures-2026-11-07/pes-versions, 24 Save As files of five designs; docs/FILE_FORMATS.md).
//!
//! What a version holds, in the order of the file:
//! - v4: the hoop and display fields, no thread table; the runs name PEC chart colours.
//! - v5: an image name, the design's transform and the thread table (0110's UTF-16 strings).
//! - v7: version 6's page fields (custom page, page size).
//! - v8: Rotate 90 Degrees as its own field.
//! - v9 and v10: 0110's header without the hoop sections: hoop id, the hoop in mm and inches
//!   and its name; v10 a few more fields and run lengths over 65 535 points.
//!
//! The 144 bytes PE-DESIGN gives each thread after the PEC section hold a picture of the
//! thread's number in its own small font; OpenStitch writes them blank, as its `#PES0110`
//! writer does (PE-DESIGN and the machine read files with blank ones).

use crate::io::ByteWriter;
use crate::pes::{
    EMB_ONE, METADATA_KEYS, SECTION_END, SEW_SEG, Version, WriteOptions, segments_with,
    write_string8, write_string16,
};
use crate::{FormatError, Result, pec, pes11};
use openstitch_core::{StitchPattern, round_coordinate};

/// The versions this module writes.
pub(crate) fn writes(v: Version) -> bool {
    matches!(
        v,
        Version::V4 | Version::V5 | Version::V7 | Version::V8 | Version::V9 | Version::V10
    )
}

/// The signature of `v`.
fn magic(v: Version) -> &'static str {
    match v {
        Version::V4 => "#PES0040",
        Version::V5 => "#PES0050",
        Version::V7 => "#PES0070",
        Version::V8 => "#PES0080",
        Version::V9 => "#PES0090",
        _ => "#PES0100",
    }
}

/// The display fields every version keeps after the hoop and page (background and grid
/// colours, grid and axis switches, grid interval), as the version 6 writer writes them.
const DISPLAY: [u16; 8] = [7, 0x13, 1, 1, 0, 100, 1, 0];

/// The needle points' offset in the file's page coordinates, 0.1 mm.
const ORIGIN: f64 = pes11::PAGE_ORIGIN as f64;

pub(crate) fn write(input: &StitchPattern, options: &WriteOptions) -> Result<Vec<u8>> {
    let v = options.version;
    if options.truncated {
        return Err(FormatError::UnsupportedFormat(format!(
            "{}: PE-DESIGN writes these versions with their stitch group only",
            magic(v)
        )));
    }
    let pattern = pec::prepare(input);
    if pattern.threads.len() > usize::from(u16::MAX) {
        return Err(FormatError::ValueOutOfRange("PES: too many threads".into()));
    }
    let mut w = ByteWriter::default();
    w.ascii(magic(v));
    let pec_at = w.len();
    w.u32le(0);
    w.u16le(1);
    // `01` a single-needle hoop page, `10` a multi-needle custom page, as PE-DESIGN writes them.
    w.ascii(if options.custom_page { "10" } else { "01" });
    for key in METADATA_KEYS {
        write_string8(&mut w, pattern.metadata.get(key).map(String::as_str));
    }
    header(&mut w, options);
    if v != Version::V4 {
        pes11::write_threads(&mut w, &pattern.threads);
    }
    blocks(&mut w, &pattern, v);
    w.patch32le(pec_at, w.len() as i64);
    let pec_start = w.len();
    // A hoop turned 90°: the machine stitches turned too, as PE-DESIGN writes them.
    let list = if options.hoop_rotated {
        pec::write_section(&pec::turned_for_hoop(&pattern), &mut w)?
    } else {
        pec::write_section(&pattern, &mut w)?
    };
    let pec_header = &mut w.bytes[pec_start..];
    pes11::patch_pec_header(pec_header, !options.custom_page);
    // Version 5 leaves 0x2D a space, version 4 0x1F too (props-v4, props-v5).
    if v == Version::V4 || v == Version::V5 {
        pec_header[0x2D] = 0x20;
    }
    if v == Version::V4 {
        pec_header[0x1F] = 0x20;
    }
    match v {
        Version::V4 => {
            w.append(&list);
            w.fill(0x20, 128_usize.saturating_sub(list.len()));
        }
        Version::V5 => {
            w.append(&list);
            w.fill(0x20, 128_usize.saturating_sub(list.len()));
            for _ in &pattern.threads {
                w.fill(0, 0x90);
            }
            for t in &pattern.threads {
                w.append(&[t.red, t.green, t.blue]);
            }
        }
        _ => pes11::write_addendum(&mut w, &list, &pattern.threads, !options.custom_page),
    }
    Ok(w.bytes)
}

fn words(w: &mut ByteWriter, words: &[u16]) {
    for &v in words {
        w.u16le(i64::from(v));
    }
}

fn affine(w: &mut ByteWriter) {
    for v in [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] {
        w.f32le(v);
    }
}

/// The header fields after the metadata strings, up to the thread table.
fn header(w: &mut ByteWriter, o: &WriteOptions) {
    let v = o.version;
    let (hw, hh) = o.hoop_mm.unwrap_or((100, 100));
    let (pw, ph) = o.page_mm.unwrap_or((200, 200));
    let rot = u16::from(o.hoop_rotated);
    let custom = u16::from(o.custom_page);
    // Before version 8 a turned hoop is written with its width and height swapped (rot-v4 to
    // rot-v7: 180 × 130 for the 130 × 180 hoop; version 8 writes 130 × 180 and Rotate 1).
    let (sw, sh) = if o.hoop_rotated { (hh, hw) } else { (hw, hh) };
    match v {
        Version::V4 | Version::V5 => {
            words(w, &[0, sw, sh, rot]);
            words(w, &DISPLAY);
            if v == Version::V4 {
                w.fill(0, 4);
            } else {
                w.u8(0); // image file name
                affine(w);
                words(w, &[0, 0, 0]); // programmable fills, motifs, feather patterns
            }
        }
        Version::V7 => {
            words(w, &[0, custom, sw, sh, 0, pw, ph, 100, 100, 100]);
            words(w, &DISPLAY);
            w.u8(0);
            affine(w);
            words(w, &[0, 0, 0]);
        }
        Version::V8 => {
            words(w, &[0, custom, hw, hh, rot, 0, pw, ph, 100, 100, 100]);
            words(w, &DISPLAY);
            w.u8(0);
            affine(w);
            words(w, &[4, 0, 0, 0]);
        }
        _ => {
            // 0110's fields without the hoop sections: page mode, hoop id 0, the hoop in mm and
            // in 0.1 inch, no hoop name (props-v9, props-v10).
            let inch = |mm: u16| (f64::from(mm) / 2.54).round() as u16;
            words(w, &[0, custom, 0, hw, hh, inch(hw), inch(hh)]);
            w.u8(0);
            words(w, &[rot, pw, ph, 0, 100]);
            words(w, &DISPLAY[..7]);
            if v == Version::V10 {
                words(w, &[0, 0, 0, 1]);
            }
            words(w, &[0x0A]);
            w.fill(0, 5);
            affine(w);
            w.fill(0, 8);
            words(w, &[4, 0, 0, 0]);
        }
    }
}

/// The design as one `CEmbOne` stitch group in page coordinates, its `CSewSeg` runs, the colour
/// log and the version's trailer.
fn blocks(w: &mut ByteWriter, pattern: &StitchPattern, v: Version) {
    if pattern.stitches.is_empty() {
        words(w, &[0, 0, 0]);
        return;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for s in &pattern.stitches {
        let (x, y) = (round_coordinate(s.x) as f64, round_coordinate(s.y) as f64);
        x0 = x0.min(x);
        x1 = x1.max(x);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    let file = |c: f64| (c + ORIGIN).round().clamp(-32768.0, 32767.0) as i16 as u16;
    // Version 4 has no thread table: its runs name PEC chart colours, as version 1's do.
    let chart = if v == Version::V4 {
        Version::V1
    } else {
        Version::V6
    };
    let runs = segments_with(pattern, chart, -ORIGIN, -ORIGIN, true);
    words(w, &[1, 0xFFFF, 0]);
    write_string16(w, EMB_ONE);
    for _ in 0..2 {
        words(w, &[file(x0), file(y0), file(x1), file(y1)]);
    }
    affine(w);
    let size = |d: f64| d.round().clamp(0.0, 65535.0) as u16;
    words(w, &[1, file(x0), file(y1), size(x1 - x0), size(y1 - y0)]);
    w.fill(0, 8);
    words(
        w,
        &[runs.len().min(usize::from(u16::MAX)) as u16, 0xFFFF, 0],
    );
    write_string16(w, SEW_SEG);
    let mut log: Vec<(u16, u16)> = Vec::new();
    for (i, run) in runs.iter().enumerate() {
        if i > 0 {
            words(w, &[SECTION_END]);
        }
        if log.last().is_none_or(|&(_, c)| c != run.color) {
            log.push((i as u16, run.color));
        }
        words(w, &[run.flag, run.color]);
        if v == Version::V10 {
            // Version 10 writes every run's length as FFFF and a u32 (props-v10: FFFF 036B 0000).
            w.u16le(0xFFFF);
            w.u32le(run.points.len() as i64);
        } else {
            w.u16le(run.points.len().min(usize::from(u16::MAX)) as i64);
        }
        for &(x, y) in &run.points {
            w.u16le(i64::from(x));
            w.u16le(i64::from(y));
        }
    }
    w.u16le(log.len() as i64);
    for &(run, color) in &log {
        words(w, &[run, color]);
    }
    // The trailer: zeros, then one entry per colour change (all 24 Save As files).
    let base = match v {
        Version::V4 => 0,
        Version::V5 | Version::V7 => 8,
        Version::V8 => 14,
        _ => 16,
    };
    w.fill(0, base);
    for k in 0..log.len() {
        w.u32le(0);
        w.u32le(k as i64);
        if matches!(v, Version::V9 | Version::V10) {
            w.u32le(1);
        }
    }
}
