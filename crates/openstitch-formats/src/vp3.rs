//! Husqvarna Viking / Pfaff VP3 (#480): big-endian, built from nested blocks, each a three-byte
//! tag and a u32 length of what follows the length field. Header positions are micrometres
//! with y up; stitch records are 0.1 mm with y down. Strings are a u16 byte length and the
//! text: UTF-16BE in the file and design headers, UTF-8 in the thread blocks.
//!
//! ```text
//! "%vsm%" 00, producer (UTF-16)
//! 00 02 00 len                        file block
//!   settings "" (UTF-16), right, -top, left, -bottom (µm), u32 record count,
//!   u16 colour blocks, 0C 00, 01 (one design)
//!   00 03 00 len                      design block
//!     centre x, -centre y, 00 00 00, -w/2, w/2, -h/2, h/2, w, h (µm), notes "" (UTF-16),
//!     64 64, 4096, 0, 0, 4096, "xxPP" 01 00, producer (UTF-16), u16 colour blocks
//!     00 05 00 len                    one per colour block
//!       start x, -start y (µm from the centre), thread, shift x, -shift y (µm, end − start)
//!       00 01 00 len 0A F6 00 records, then 00
//! ```
//!
//! A thread is `01 00 r g b 00 00 00 05 28` (one colour, no transition, rayon, weight 40) and
//! three UTF-8 strings: catalogue number, colour name, brand. The machine shows them.
//!
//! Records: `dx dy` (±127) is a stitch, `80 01 dx dy 80 02` (i16 BE each) a longer one, `80 03`
//! a trim. VP3 has no jump record: a frame move is the next record, after a trim when the
//! design trims there. The reader returns a record read right after a trim, or first in a
//! colour block, as a jump; every other record is a stitch (an untrimmed jump comes back as a
//! stitch, as pyembroidery reads it). Colour changes start a new colour block; a stop also
//! starts one, with the same thread. The design ends with a trim.
//!
//! Sources: Roidery's VP3 notes (`crates/emb-formats/src/vp3.rs`, MIT OR Apache-2.0, credited
//! in THIRD_PARTY_NOTICES.md) and files written by pyembroidery 1.5.1, the test oracle. No
//! reference file from Husqvarna or Pfaff software or a machine: the writer is experimental.

use crate::layout::{self, Event};
use crate::{FormatError, Result, repeating_stops_palette};
use openstitch_core::{Command, Stitch, StitchPattern, Thread};
use std::borrow::Cow;

pub const MAGIC: &[u8; 6] = b"%vsm%\0";
/// The producer string pyembroidery and other writers put in both headers (spaces included).
pub const PRODUCER: &str = "Produced by     Software Ltd";
/// Largest move of a short record, per axis, in 0.1 mm.
pub const MAX_DELTA: i64 = 127;
/// Most colour blocks the writer stores.
pub const MAX_COLORS: usize = 255;
/// Upper bound on the records a reader accepts.
pub const MAX_RECORDS: usize = 4_000_000;

const FILE_TAG: [u8; 3] = [0, 2, 0];
const DESIGN_TAG: [u8; 3] = [0, 3, 0];
const BLOCK_TAG: [u8; 3] = [0, 5, 0];
const STITCH_TAG: [u8; 3] = [0, 1, 0];

// MARK: Writer

#[derive(Default)]
struct Out(Vec<u8>);

impl Out {
    fn u16(&mut self, v: u16) {
        self.0.extend(v.to_be_bytes());
    }
    fn i32(&mut self, v: i64) -> Result<()> {
        let v = i32::try_from(v)
            .map_err(|_| FormatError::ValueOutOfRange(format!("VP3 position {v} µm")))?;
        self.0.extend(v.to_be_bytes());
        Ok(())
    }
    fn utf16(&mut self, s: &str) {
        let units: Vec<u16> = s.encode_utf16().take(u16::MAX as usize / 2).collect();
        self.u16((units.len() * 2) as u16);
        for u in units {
            self.u16(u);
        }
    }
    fn utf8(&mut self, s: &str) {
        let mut end = s.len().min(u16::MAX as usize);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.u16(end as u16);
        self.0.extend(&s.as_bytes()[..end]);
    }
    /// Starts a block; returns where its length goes, for [`Out::close`].
    fn open(&mut self, tag: [u8; 3]) -> usize {
        self.0.extend(tag);
        self.0.extend([0; 4]);
        self.0.len() - 4
    }
    fn close(&mut self, at: usize) -> Result<()> {
        let len = u32::try_from(self.0.len() - at - 4)
            .map_err(|_| FormatError::ValueOutOfRange("VP3 block over 4 GB".into()))?;
        self.0[at..at + 4].copy_from_slice(&len.to_be_bytes());
        Ok(())
    }
}

/// One colour block's records, with the needle positions (0.1 mm, centred, y down) where it
/// starts and ends.
struct Block {
    start: (i64, i64),
    end: (i64, i64),
    records: Vec<u8>,
    count: usize,
}

fn long_record(out: &mut Vec<u8>, dx: i64, dy: i64) -> Result<()> {
    let word = |v: i64| {
        i16::try_from(v).map_err(|_| {
            FormatError::ValueOutOfRange(format!("VP3 move of {:.1} mm", v as f64 / 10.0))
        })
    };
    let (dx, dy) = (word(dx)?, word(dy)?);
    out.extend([0x80, 0x01]);
    out.extend(dx.to_be_bytes());
    out.extend(dy.to_be_bytes());
    out.extend([0x80, 0x02]);
    Ok(())
}

fn blocks(pattern: &StitchPattern) -> Result<Vec<Block>> {
    let mut out = vec![Block {
        start: (0, 0),
        end: (0, 0),
        records: Vec::new(),
        count: 0,
    }];
    let (mut x, mut y) = (0_i64, 0_i64);
    // A record right after the start of a colour block moves without a trim before it.
    let mut block_start = true;
    for e in layout::events(pattern) {
        let b = out.last_mut().expect("one block");
        match e {
            Event::Move {
                x: tx,
                y: ty,
                trimmed,
            } => {
                if trimmed && !block_start {
                    b.records.extend([0x80, 0x03]);
                    b.count += 1;
                }
                let (dx, dy) = (tx - x, ty - y);
                if dx != 0 || dy != 0 {
                    // Always the long form, so a reader tells the move from a stitch.
                    long_record(&mut b.records, dx, dy)?;
                    b.count += 1;
                }
                (x, y) = (tx, ty);
                b.end = (x, y);
            }
            Event::Stitch { x: tx, y: ty } => {
                let (dx, dy) = (tx - x, ty - y);
                if dx.abs() <= MAX_DELTA && dy.abs() <= MAX_DELTA {
                    b.records.extend([dx as u8, dy as u8]);
                } else {
                    long_record(&mut b.records, dx, dy)?;
                }
                b.count += 1;
                (x, y) = (tx, ty);
                b.end = (x, y);
                block_start = false;
            }
            Event::ColorChange | Event::Stop => {
                out.push(Block {
                    start: (x, y),
                    end: (x, y),
                    records: Vec::new(),
                    count: 0,
                });
                block_start = true;
            }
            Event::End => {
                // The machine does not cut at the end by itself.
                b.records.extend([0x80, 0x03]);
                b.count += 1;
                break;
            }
        }
    }
    Ok(out)
}

/// The thread written for colour block `i`: the pattern's own when it has one (a stop repeats
/// the current thread), else black.
fn block_threads(pattern: &StitchPattern, count: usize) -> Vec<Thread> {
    let mut palette = repeating_stops_palette(pattern);
    let fill = palette.last().cloned().unwrap_or_else(|| Thread::rgb(0));
    palette.resize(count, fill);
    palette
}

fn thread(out: &mut Out, t: &Thread) {
    out.0
        .extend([1, 0, t.red, t.green, t.blue, 0, 0, 0, 5, 0x28]);
    out.utf8(t.catalog_number.as_deref().unwrap_or(""));
    out.utf8(t.name.as_deref().unwrap_or(""));
    out.utf8(t.brand.as_deref().unwrap_or(""));
}

/// Writes the pattern centred on the middle of its needle points (see [`crate::layout`]),
/// with each thread's catalogue number, name and brand.
pub fn write(pattern: &StitchPattern) -> Result<Vec<u8>> {
    let blocks = blocks(pattern)?;
    if blocks.len() > MAX_COLORS {
        return Err(FormatError::ValueOutOfRange(format!(
            "VP3: {} colour blocks (at most {MAX_COLORS})",
            blocks.len()
        )));
    }
    let threads = block_threads(pattern, blocks.len());
    // Bounds of the needle points in µm, from the centred records.
    let mut points = layout::events(pattern).into_iter().filter_map(|e| match e {
        Event::Stitch { x, y } => Some((x * 100, y * 100)),
        _ => None,
    });
    let first = points.next().unwrap_or((0, 0));
    let (left, top, right, bottom) = points.fold((first.0, first.1, first.0, first.1), |b, p| {
        (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1))
    });
    let (w, h) = (right - left, bottom - top);
    let (cx, cy) = (left + w / 2, top + h / 2);
    let records: usize = blocks.iter().map(|b| b.count).sum();

    let mut out = Out(MAGIC.to_vec());
    out.utf16(PRODUCER);
    let file = out.open(FILE_TAG);
    out.utf16("");
    for v in [right, -top, left, -bottom] {
        out.i32(v)?;
    }
    out.i32(records.min(i32::MAX as usize) as i64)?;
    out.u16(blocks.len() as u16);
    out.0.extend([0x0C, 0x00, 0x01]);

    let design = out.open(DESIGN_TAG);
    out.i32(cx)?;
    out.i32(-cy)?;
    out.0.extend([0, 0, 0]);
    for v in [-w / 2, w / 2, -h / 2, h / 2, w, h] {
        out.i32(v)?;
    }
    out.utf16("");
    out.0.extend([0x64, 0x64]);
    for v in [4096, 0, 0, 4096] {
        out.i32(v)?;
    }
    out.0.extend(b"xxPP\x01\x00");
    out.utf16(PRODUCER);
    out.u16(blocks.len() as u16);
    for (b, t) in blocks.iter().zip(&threads) {
        let at = out.open(BLOCK_TAG);
        out.i32(b.start.0 * 100 - cx)?;
        out.i32(-(b.start.1 * 100 - cy))?;
        thread(&mut out, t);
        out.i32((b.end.0 - b.start.0) * 100)?;
        out.i32(-(b.end.1 - b.start.1) * 100)?;
        let stitches = out.open(STITCH_TAG);
        out.0.extend([0x0A, 0xF6, 0x00]);
        out.0.extend(&b.records);
        out.close(stitches)?;
        out.0.push(0);
        out.close(at)?;
    }
    out.close(design)?;
    out.close(file)?;
    Ok(out.0)
}

// MARK: Reader

/// Big-endian bounds-checked cursor.
struct In<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> In<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self
            .at
            .checked_add(n)
            .and_then(|end| self.bytes.get(self.at..end))
            .ok_or_else(|| FormatError::Truncated(format!("VP3: need {n} bytes at {}", self.at)))?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn i32(&mut self) -> Result<i64> {
        let b = self.take(4)?;
        Ok(i64::from(i32::from_be_bytes([b[0], b[1], b[2], b[3]])))
    }
    fn string(&mut self) -> Result<&'a [u8]> {
        let n = self.u16()?;
        self.take(usize::from(n))
    }
    fn utf8(&mut self) -> Result<Option<Cow<'static, str>>> {
        let s = String::from_utf8_lossy(self.string()?).trim().to_owned();
        Ok((!s.is_empty()).then_some(Cow::Owned(s)))
    }
    fn utf16(&mut self) -> Result<String> {
        let b = self.string()?;
        let units: Vec<u16> = b
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        Ok(String::from_utf16_lossy(&units))
    }
    /// Reads a block's tag and length; returns the offset where the block ends.
    fn block(&mut self, tag: [u8; 3]) -> Result<usize> {
        let found = self.take(3)?;
        if found != tag {
            return Err(FormatError::BadSignature(format!(
                "VP3 block {found:02x?} at {}, expected {tag:02x?}",
                self.at - 3
            )));
        }
        let len = self.i32()?;
        usize::try_from(len)
            .ok()
            .and_then(|l| self.at.checked_add(l))
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| FormatError::Truncated(format!("VP3 block of {len} bytes")))
    }
}

fn read_thread(r: &mut In) -> Result<Thread> {
    let colors = r.u8()?;
    r.u8()?; // transition
    let mut t = Thread::rgb(0);
    for i in 0..colors {
        let c = r.take(3)?;
        if i == 0 {
            (t.red, t.green, t.blue) = (c[0], c[1], c[2]);
        }
        r.take(3)?; // part, length
    }
    r.take(2)?; // thread type, weight
    t.catalog_number = r.utf8()?;
    t.name = r.utf8()?;
    t.brand = r.utf8()?;
    Ok(t)
}

/// Micrometres to 0.1 mm.
fn tenth(v: i64) -> f64 {
    (v as f64 / 100.0).round()
}

pub fn read(data: &[u8]) -> Result<StitchPattern> {
    if !data.starts_with(&MAGIC[..5]) {
        return Err(FormatError::BadSignature("VP3: no %vsm% signature".into()));
    }
    let mut r = In { bytes: data, at: 6 };
    r.string()?; // producer
    r.block(FILE_TAG)?;
    let settings = r.utf16()?;
    r.take(16 + 4)?; // bounds, record count
    r.take(2 + 2 + 1)?; // colours, 0C 00, designs
    r.block(DESIGN_TAG)?;
    let (cx, cy) = (r.i32()?, -r.i32()?);
    r.take(3 + 24)?;
    let notes = r.utf16()?;
    r.take(2 + 16 + 6)?;
    r.string()?; // producer
    let colors = usize::from(r.u16()?);

    let mut pattern = StitchPattern::new();
    for (key, value) in [("vp3.settings", settings), ("comments", notes)] {
        if !value.trim().is_empty() {
            pattern.metadata.insert(key.into(), value);
        }
    }
    let (mut x, mut y) = (0.0_f64, 0.0_f64);
    for i in 0..colors {
        let end = r.block(BLOCK_TAG)?;
        let sx = tenth(r.i32()? + cx);
        let sy = tenth(-r.i32()? + cy);
        if i > 0 {
            pattern
                .stitches
                .push(Stitch::new(x, y, Command::ColorChange));
        }
        if (sx, sy) != (x, y) {
            (x, y) = (sx, sy);
            pattern.stitches.push(Stitch::new(x, y, Command::Jump));
        }
        pattern.threads.push(read_thread(&mut r)?);
        r.take(8)?; // shift
        let records_end = r.block(STITCH_TAG)?.min(end);
        r.take(3)?;
        let records = data.get(r.at..records_end).unwrap_or_default();
        let mut moving = true;
        let mut k = 0;
        while k + 1 < records.len() {
            if pattern.stitches.len() > MAX_RECORDS {
                return Err(FormatError::ValueOutOfRange(format!(
                    "VP3: more than {MAX_RECORDS} records"
                )));
            }
            let (a, b) = (records[k], records[k + 1]);
            k += 2;
            let (dx, dy) = if a != 0x80 {
                (f64::from(a as i8), f64::from(b as i8))
            } else {
                match b {
                    0x01 => {
                        let Some(w) = records.get(k..k + 4) else {
                            break;
                        };
                        k += 4;
                        if records.get(k..k + 2) == Some(&[0x80, 0x02]) {
                            k += 2;
                        }
                        let dx = f64::from(i16::from_be_bytes([w[0], w[1]]));
                        let dy = f64::from(i16::from_be_bytes([w[2], w[3]]));
                        if moving {
                            x += dx;
                            y += dy;
                            pattern.stitches.push(Stitch::new(x, y, Command::Jump));
                            continue;
                        }
                        (dx, dy)
                    }
                    0x03 => {
                        pattern.stitches.push(Stitch::new(x, y, Command::Trim));
                        moving = true;
                        continue;
                    }
                    _ => continue,
                }
            };
            x += dx;
            y += dy;
            pattern.stitches.push(Stitch::new(x, y, Command::Stitch));
            moving = false;
        }
        r.at = end;
    }
    pattern.stitches.push(Stitch::new(x, y, Command::End));
    Ok(pattern)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> StitchPattern {
        let mut p = StitchPattern::new();
        let mut red = Thread::rgb(0xE01010).with_name("Poppy");
        red.catalog_number = Some("1234".into());
        red.brand = Some("Acme Rayon".into());
        p.threads = vec![red, Thread::rgb(0x1040C0)];
        p.stitch_to(0.0, 0.0);
        p.stitch_to(10.0, 5.0);
        p.push(Command::Trim);
        p.jump_to(300.0, 5.0);
        p.stitch_to(300.0, 5.0);
        p.stitch_to(450.0, 40.0); // a 15 mm stitch: one long record
        p.push(Command::ColorChange);
        p.stitch_to(460.0, 60.0);
        p.push(Command::End);
        p
    }

    #[test]
    fn block_lengths_run_to_their_ends() {
        let bytes = write(&sample()).expect("write");
        assert!(bytes.starts_with(MAGIC));
        let at = 6 + 2 + PRODUCER.len() * 2 + 3;
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().expect("4 bytes")) as usize;
        assert_eq!(at + 4 + len, bytes.len());
    }

    #[test]
    fn thread_strings_round_trip_and_empty_ones_read_as_none() {
        let back = read(&write(&sample()).expect("write")).expect("read");
        let t = &back.threads[0];
        assert_eq!(t.hex(), "#e01010");
        assert_eq!(t.catalog_number.as_deref(), Some("1234"));
        assert_eq!(t.name.as_deref(), Some("Poppy"));
        assert_eq!(t.brand.as_deref(), Some("Acme Rayon"));
        let blue = &back.threads[1];
        assert_eq!((blue.name.clone(), blue.brand.clone()), (None, None));
    }

    #[test]
    fn long_names_are_cut_at_a_character_boundary() {
        let mut p = sample();
        p.threads[0].name = Some("é".repeat(40_000).into());
        let back = read(&write(&p).expect("write")).expect("read");
        let name = back.threads[0].name.clone().expect("name");
        assert_eq!(name.chars().count(), u16::MAX as usize / 2);
        assert_eq!(back.threads[0].brand.as_deref(), Some("Acme Rayon"));
    }

    #[test]
    fn moves_and_trims_come_back() {
        let back = read(&write(&sample()).expect("write")).expect("read");
        let commands: Vec<Command> = back.stitches.iter().map(|s| s.command).collect();
        use Command::*;
        assert_eq!(
            commands,
            [
                Jump,
                Stitch,
                Stitch,
                Trim,
                Jump,
                Stitch,
                Stitch,
                ColorChange,
                Stitch,
                Trim,
                End
            ]
        );
        // The 15 mm stitch is one stitch, not a jump.
        let s = back.stitches[6];
        let r = back.stitches[5];
        assert_eq!((s.x - r.x, s.y - r.y), (150.0, 35.0));
    }

    #[test]
    fn too_many_colours_is_out_of_range() {
        let mut p = StitchPattern::new();
        for i in 0..=MAX_COLORS {
            p.threads.push(Thread::rgb(i as u32));
            p.stitch_to(i as f64, 0.0);
            p.push(Command::ColorChange);
        }
        assert!(matches!(write(&p), Err(FormatError::ValueOutOfRange(_))));
    }

    #[test]
    fn damaged_input_never_panics() {
        let good = write(&sample()).expect("write");
        for n in 0..good.len() {
            let _ = read(&good[..n]);
        }
        let mut bad = good.clone();
        for i in 0..bad.len() {
            bad[i] ^= 0xA5;
            let _ = read(&bad);
            bad[i] ^= 0xA5;
        }
        assert!(read(b"%vsm%").is_err());
        assert!(read(b"nothing").is_err());
    }
}
