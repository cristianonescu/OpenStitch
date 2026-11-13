//! Husqvarna Viking SHV (read only). Big-endian. A 0x56-byte text header, a length-prefixed
//! design name, a 1-bit preview (`width` bytes × ⌈`height` / 2⌉ rows plus 4 bytes), a colour
//! count and 18 unknown bytes, then 14 bytes per colour: u32 BE number of 2-byte record words
//! in the block, a SHV chart index and 9 unknown bytes. The stitch records follow the table:
//! PE-DESIGN 11.50's own SHV export (`tests/fixtures/pe11/sample-basic.shv`) starts them at the
//! table's end with exactly as many words as the counts add up to. pyembroidery's reader starts
//! 2 bytes before the end; a file with one word fewer than the counts (as pyembroidery's layout
//! implies) is read that way. Records: signed byte `(dx, dy)` with y down; `80 01 dx(i16) dy(i16)`
//! moves and starts jump mode (later short records are jumps), `80 02` ends jump mode with a
//! needle point where the move ends (PE-DESIGN writes no record for that stitch; pyembroidery
//! reads it as a move only, #349), `80 03` is skipped. A colour block ends after its word
//! count; there is no explicit colour change.
//!
//! Follows pyembroidery 1.5.1's `ShvReader` (MIT), the oracle.
//!
//! The writer ([`write`], issue #320) lays the file out as PE-DESIGN 11.50's export of
//! `sample-basic` does: the header text, the design name (the file's name when the exporter
//! gives it, as PE-DESIGN names it: pe-long.SHV), the preview size `w`, `h` and
//! `w / 2, h / 2` twice, the preview, the colour count, the six bytes `C4 28 00 30 00 00` (not
//! described anywhere; copied), the needle points' bounds as i16 right, bottom, left, top, the
//! total word count (u32), then per colour block its word count, its nearest SHV chart entry,
//! five zero bytes and the block's start point (i16 x, y). A frame move is `80 01 dx dy 80 02`
//! to the first stitch after it, which has no record of its own: PE-DESIGN goes straight to the
//! stitch after a jump's landing (`machine/pe-moves.SHV`); `80 03` comes before a move longer
//! than [`LONG_MOVE`] (38.0 mm) on either axis (#349); a stitch over 12.7 mm is one such move
//! to its end (pe-long.SHV). The preview is 4 bits a pixel, column by column (x), each column bottom to top, one pixel a
//! millimetre with a one-pixel margin (`w` = width in whole millimetres + 2): PE's size fields
//! are matched, its pixels only approximately (PE's own drawing rule is not known). A pixel
//! holds the block's chart entry modulo 15 (15 for 0), which gives PE's 11 and 3 for its
//! 800 RED (entry 41) and Prussian Blue (entry 33); other entries are not yet checked. A Brother
//! chart colour takes the chart entry PE-DESIGN gives it ([`PE_SHV_CODES`], #349).
//! Neither pyembroidery nor libembroidery has a working SHV writer.

use crate::io::{ByteReader, signed8};
use crate::layout::{self, Event};
use crate::{FormatError, Result};
use openstitch_core::{Command, SHV_CHART, Stitch, StitchPattern, Thread, nearest_index};

/// The text every SHV file starts with (0x56 bytes).
pub const HEADER_TEXT: &[u8; 0x56] =
    b"Embroidery disk created using software licensed from Viking Sewing Machines AB, Sweden";
/// The largest stitch record step (12.7 mm).
pub const MAX_DELTA: i64 = 127;
/// A move longer than this on either axis (0.1 mm) has `80 03` before it, as PE-DESIGN 11.50
/// writes it (#349): none before moves up to 36.7 mm on their longer axis (sample-basic,
/// machine/pe-c17.SHV, pe-long.SHV, pe-moves.SHV: 14.3–36.7 mm, a 43.4 mm move whose longer
/// axis is 36.0 mm among them), one before every move from 38.7 mm (pe-moves.SHV, pe-c17.SHV:
/// 38.7–72.0 mm). PE-DESIGN's own threshold lies between those two; 1.5 in (38.1 mm) fits.
pub const LONG_MOVE: i64 = 380;

pub fn read(data: &[u8]) -> Result<StitchPattern> {
    let mut reader = ByteReader::new(data);
    let mut pattern = StitchPattern::new();
    reader.skip(0x56)?;
    let name_length = usize::from(reader.u8()?);
    if let Ok(name) = std::str::from_utf8(reader.read(name_length)?) {
        pattern.set_name(name);
    }
    let width = usize::from(reader.u8()?);
    let height = usize::from(reader.u8()?);
    reader.skip(4 + height.div_ceil(2) * width)?;
    let color_count = usize::from(reader.u8()?);
    reader.skip(18)?;
    let mut words_per_color = Vec::with_capacity(color_count);
    for _ in 0..color_count {
        let words = reader.read(4)?;
        words_per_color.push(u32::from_be_bytes([words[0], words[1], words[2], words[3]]));
        let code = usize::from(reader.u8()?);
        pattern.threads.push(
            SHV_CHART[code % SHV_CHART.len()]
                .clone()
                .unwrap_or_else(|| Thread::rgb(0)),
        );
        reader.skip(9)?;
    }
    // PE-DESIGN's layout: the records start at the table's end and fill exactly the counted
    // words. pyembroidery's: they start 2 bytes earlier, so one word fewer follows the table.
    // Reading PE's file the pyembroidery way adds a stitch at the origin and, one word short at
    // the end, an extra colour change (issue #135).
    let counted: u64 = words_per_color.iter().map(|&w| u64::from(w)).sum();
    let following = (data.len() - reader.offset) as u64 / 2;
    if following < counted {
        reader.offset = reader
            .offset
            .checked_sub(2)
            .ok_or_else(|| FormatError::Truncated("SHV colour table".into()))?;
    }

    let signed16 = |b: &[u8]| f64::from(i16::from_be_bytes([b[0], b[1]]));
    let (mut x, mut y) = (0.0_f64, 0.0_f64);
    let mut in_jump = false;
    let mut since_stop = 0_u64;
    let mut block = 0;
    let mut max = words_per_color.first().map_or(0, |&w| u64::from(w));
    while reader.remaining() >= 2 {
        let (b0, b1) = (reader.u8()?, reader.u8()?);
        if since_stop >= max {
            pattern
                .stitches
                .push(Stitch::new(x, y, Command::ColorChange));
            since_stop = 0;
            block += 1;
            max = words_per_color
                .get(block)
                .map_or(u64::from(u32::MAX), |&w| u64::from(w));
        }
        if b0 == 0x80 {
            since_stop += 1;
            match b1 {
                3 => continue,
                2 => {
                    // Where a move ends the needle goes down: PE-DESIGN 11.50 writes no record
                    // for that stitch (its DST of sample-basic has a stitch on every landing;
                    // pe-moves.SHV moves straight to a block's first needle point, #349).
                    // pyembroidery reads it as a move only.
                    if in_jump {
                        pattern.stitches.push(Stitch::new(x, y, Command::Stitch));
                    }
                    in_jump = false;
                    continue;
                }
                1 => {
                    since_stop += 2;
                    let Ok(d) = reader.read(4) else {
                        break;
                    };
                    x += signed16(&d[0..2]);
                    y += signed16(&d[2..4]);
                    in_jump = true;
                    pattern.stitches.push(Stitch::new(x, y, Command::Jump));
                    continue;
                }
                _ => {} // read as an ordinary record, as the reference does
            }
        }
        since_stop += 1;
        x += signed8(b0);
        y += signed8(b1);
        let command = if in_jump {
            Command::Jump
        } else {
            Command::Stitch
        };
        pattern.stitches.push(Stitch::new(x, y, command));
    }
    pattern.stitches.push(Stitch::new(x, y, Command::End));
    Ok(pattern)
}

/// The SHV chart entry PE-DESIGN 11.50 writes for a Brother chart colour, by PEC chart index
/// (machine/pe-c17.SHV: 15 colours; sample-basic.shv). It is not always the nearest entry
/// (800 RED is 41, not 3). Other colours take the nearest entry.
pub const PE_SHV_CODES: [(usize, u8); 15] = [
    (1, 33),
    (3, 38),
    (5, 41),
    (8, 10),
    (10, 37),
    (13, 5),
    (20, 0),
    (25, 42),
    (30, 36),
    (33, 25),
    (37, 6),
    (41, 38),
    (45, 22),
    (52, 25),
    (58, 5),
];

/// The SHV chart entry for a thread: PE-DESIGN's for a Brother chart colour it was seen to map
/// ([`PE_SHV_CODES`]), else the nearest.
fn chart_code(t: &Thread) -> u8 {
    let rgb = t.rgb_value();
    PE_SHV_CODES
        .iter()
        .find(|(i, _)| {
            openstitch_core::PEC_CHART
                .get(*i)
                .and_then(Option::as_ref)
                .is_some_and(|c| c.rgb_value() == rgb)
        })
        .map_or_else(
            || nearest_index(t, &SHV_CHART).unwrap_or(0) as u8,
            |&(_, c)| c,
        )
}

/// Draws a line of `value` pixels from `a` to `b` (Bresenham) into a column-major preview of
/// `w` columns of `h` pixels, each column stored bottom to top.
fn draw(pixels: &mut [u8], (w, h): (i64, i64), a: (i64, i64), b: (i64, i64), value: u8) {
    let (mut x, mut y) = a;
    let (dx, dy) = ((b.0 - x).abs(), -(b.1 - y).abs());
    let (sx, sy) = ((b.0 - x).signum(), (b.1 - y).signum());
    let mut err = dx + dy;
    loop {
        if (0..w).contains(&x) && (0..h).contains(&y) {
            pixels[(x * h + (h - 1 - y)) as usize] = value;
        }
        if (x, y) == b {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// One colour block being written: its records, its start point and its needle points.
#[derive(Default)]
struct Block {
    records: Vec<u8>,
    start: (i64, i64),
    points: Vec<(i64, i64)>,
}

impl Block {
    fn jump(&mut self, dx: i64, dy: i64) {
        self.records.extend_from_slice(&[0x80, 0x01]);
        self.records.extend_from_slice(&(dx as i16).to_be_bytes());
        self.records.extend_from_slice(&(dy as i16).to_be_bytes());
        self.records.extend_from_slice(&[0x80, 0x02]);
    }

    fn words(&self) -> u32 {
        (self.records.len() / 2) as u32
    }
}

/// Writes a Husqvarna Viking SHV as PE-DESIGN 11.50 exports it (see the module notes).
pub fn write(input: &StitchPattern) -> Result<Vec<u8>> {
    let palette = crate::repeating_stops_palette(input);
    let events = layout::events(input);

    let mut blocks = vec![Block::default()];
    let (mut x, mut y) = (0_i64, 0_i64);
    let mut i = 0;
    while i < events.len() {
        let block = blocks.last_mut().expect("a block");
        match events[i] {
            Event::Move { x: tx, y: ty, .. } => {
                // The move ends on a needle point, the first stitch after it, which has no record
                // of its own: PE-DESIGN 11.50 goes straight to the stitch after the jump's landing
                // (machine/pe-moves.SHV, #349), and a stitch on the landing itself is that point.
                let (tx, ty) = match layout::next_stitch(&events, i) {
                    Some(p) => {
                        i += 1;
                        p
                    }
                    None => (tx, ty),
                };
                // `80 03` before a move longer than [`LONG_MOVE`] on either axis, as PE-DESIGN.
                if (tx - x).abs().max((ty - y).abs()) > LONG_MOVE {
                    block.records.extend_from_slice(&[0x80, 0x03]);
                }
                block.jump(tx - x, ty - y);
                (x, y) = (tx, ty);
                block.points.push((x, y));
            }
            Event::Stitch { x: tx, y: ty } => {
                // A stitch over 12.7 mm is one move to its end, whose end is a needle point
                // (pe-long.SHV: its 15 mm satin stitches, #349).
                let (dx, dy) = (tx - x, ty - y);
                if dx.abs() > MAX_DELTA || dy.abs() > MAX_DELTA {
                    block.jump(dx, dy);
                } else {
                    block
                        .records
                        .extend_from_slice(&[dx as i8 as u8, dy as i8 as u8]);
                }
                (x, y) = (tx, ty);
                block.points.push((x, y));
            }
            Event::ColorChange | Event::Stop => blocks.push(Block {
                start: (x, y),
                ..Block::default()
            }),
            Event::End => break,
        }
        i += 1;
    }
    let n = blocks.len();
    if n > 255 {
        return Err(FormatError::ValueOutOfRange(format!(
            "SHV holds at most 255 colour blocks, the design has {n}"
        )));
    }
    let codes: Vec<u8> = (0..n)
        .map(|b| palette.get(b).or(palette.last()).map_or(0, chart_code))
        .collect();

    // The needle points' bounds (the stream is centred on them).
    let (min_x, min_y, max_x, max_y) = blocks
        .iter()
        .flat_map(|b| b.points.iter().copied())
        .fold(None, |b: Option<(i64, i64, i64, i64)>, (px, py)| {
            Some(b.map_or((px, py, px, py), |b| {
                (b.0.min(px), b.1.min(py), b.2.max(px), b.3.max(py))
            }))
        })
        .unwrap_or((0, 0, 0, 0));
    // One pixel a millimetre plus a margin, coarser for a design over 253 mm.
    let unit = 10 * (1 + (max_x - min_x).max(max_y - min_y) / 2530);
    let (w, h) = ((max_x - min_x) / unit + 2, (max_y - min_y) / unit + 2);
    let mut pixels = vec![0_u8; (w * h) as usize];
    let pixel = |(px, py): (i64, i64)| ((px - min_x + unit) / unit, (py - min_y) / unit);
    let mut previous: Option<(i64, i64)> = None;
    for (block, code) in blocks.iter().zip(&codes) {
        let value = match code % 15 {
            0 => 15,
            v => v,
        };
        for &p in &block.points {
            let q = pixel(p);
            draw(&mut pixels, (w, h), previous.unwrap_or(q), q, value);
            previous = Some(q);
        }
    }

    // PE-DESIGN names the design after the file it exports to (pe-long.SHV: `pe-long`); the
    // exporter records the file stem as the PEC label.
    let name = input
        .metadata
        .get(crate::pec::LABEL_KEY)
        .map(String::as_str)
        .or_else(|| input.name())
        .unwrap_or("")
        .as_bytes();
    let name = &name[..name.len().min(255)];
    let mut out = HEADER_TEXT.to_vec();
    out.push(name.len() as u8);
    out.extend_from_slice(name);
    let (half_w, half_h) = ((w / 2) as u8, (h / 2) as u8);
    out.extend_from_slice(&[w as u8, h as u8, half_w, half_h, half_w, half_h]);
    for column in pixels.chunks(h as usize) {
        for pair in column.chunks(2) {
            out.push(pair[0] << 4 | pair.get(1).copied().unwrap_or(0));
        }
    }
    out.push(n as u8);
    out.extend_from_slice(&[0xC4, 0x28, 0x00, 0x30, 0x00, 0x00]);
    for v in [max_x, max_y, min_x, min_y] {
        out.extend_from_slice(&(v as i16).to_be_bytes());
    }
    let total: u32 = blocks.iter().map(Block::words).sum();
    out.extend_from_slice(&total.to_be_bytes());
    for (block, &code) in blocks.iter().zip(&codes) {
        out.extend_from_slice(&block.words().to_be_bytes());
        out.push(code);
        out.extend_from_slice(&[0; 5]);
        out.extend_from_slice(&(block.start.0 as i16).to_be_bytes());
        out.extend_from_slice(&(block.start.1 as i16).to_be_bytes());
    }
    for block in &blocks {
        out.extend_from_slice(&block.records);
    }
    Ok(out)
}
