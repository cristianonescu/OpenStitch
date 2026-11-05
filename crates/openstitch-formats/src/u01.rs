//! Barudan U01 (#481): a 256-byte little-endian header, then 3-byte records `ctrl |dy| |dx|`
//! in 0.1 mm ending with `F8 00 00`. No colours: a colour block is a needle.
//!
//! Header: 0x80 ASCII `0`, i16 min x, −max y, max x, −min y (y up), u32 0, u32 record count,
//! i16 end x, −end y, zeros to 0x100.
//!
//! `ctrl` has 0x80 set, 0x40 when the move goes down (or not at all), 0x20 when it goes left
//! (or not at all), and in its low five bits: `00` stitch, `01` jump, `02`–`05` speed change
//! then stitch (even) or jump (odd), `06`/`07` trim then move, `08` stop, `09`–`17` needle
//! 1–15, `18` end.
//!
//! Layout from Roidery's notes (MIT OR Apache-2.0), checked against pyembroidery 1.5.1 both
//! ways. The writer starts on needle 1 and gives each distinct colour its own needle in the
//! order the colours are first sewn (after 15 colours it starts again at 1); frame moves and
//! stitches over 12.7 mm are split as in [`crate::records::ops`]. Writing is experimental: no
//! reference file from Barudan software and no sew-out yet.

use crate::io::ByteWriter;
use crate::records::{self, Builder, Op};
use crate::{FormatError, Result};
use openstitch_core::{Command, StitchPattern};

pub const HEADER_SIZE: usize = 0x100;
pub const MAX_DELTA: i64 = 127;
pub const NEEDLES: usize = 15;

pub fn read(data: &[u8]) -> Result<StitchPattern> {
    let records = data
        .get(HEADER_SIZE..)
        .ok_or_else(|| FormatError::Truncated("U01: shorter than its 256-byte header".into()))?;
    let mut b = Builder::new("U01");
    for r in records.as_chunks::<3>().0 {
        let (ctrl, by, bx) = (r[0], f64::from(r[1]), f64::from(r[2]));
        let dy = if ctrl & 0x40 != 0 { by } else { -by };
        let dx = if ctrl & 0x20 != 0 { -bx } else { bx };
        let moved = dx != 0.0 || dy != 0.0;
        match ctrl & 0x1F {
            0x00 | 0x02 | 0x04 => b.moved(dx, dy, Command::Stitch)?,
            0x01 | 0x03 | 0x05 => b.moved(dx, dy, Command::Jump)?,
            0x06 | 0x07 => {
                b.command(Command::Trim)?;
                if moved {
                    b.moved(dx, dy, Command::Jump)?;
                }
            }
            low @ 0x08..=0x17 => {
                if moved {
                    b.moved(dx, dy, Command::Jump)?;
                }
                if low == 0x08 {
                    b.command(Command::Stop)?;
                } else {
                    b.needle()?;
                }
            }
            _ => break,
        }
    }
    Ok(b.finish())
}

fn record(w: &mut ByteWriter, dx: i64, dy: i64, low: u8) {
    let mut ctrl = 0x80 | low;
    if dy >= 0 {
        ctrl |= 0x40;
    }
    if dx <= 0 {
        ctrl |= 0x20;
    }
    w.append(&[ctrl, dy.unsigned_abs() as u8, dx.unsigned_abs() as u8]);
}

pub fn write(pattern: &StitchPattern) -> Result<Vec<u8>> {
    let ops = records::ops(pattern, MAX_DELTA);
    let needles = records::needles(pattern, NEEDLES);
    let mut body = ByteWriter::default();
    for op in &ops {
        match *op {
            Op::Stitch(dx, dy) => record(&mut body, dx, dy, 0x00),
            Op::Jump(dx, dy) => record(&mut body, dx, dy, 0x01),
            Op::Trim => record(&mut body, 0, 0, 0x07),
            Op::Stop => record(&mut body, 0, 0, 0x08),
            Op::Needle(i) => {
                let n = needles.get(i).copied().unwrap_or(1);
                record(&mut body, 0, 0, 0x08 + n as u8);
            }
            Op::End => body.append(&[0xF8, 0, 0]),
        }
    }
    let records = body.len() / 3;
    if records > u32::MAX as usize {
        return Err(FormatError::ValueOutOfRange(format!(
            "U01: {records} records"
        )));
    }
    let ((min_x, min_y, max_x, max_y), (end_x, end_y)) = records::extents(&ops);
    let mut w = ByteWriter::default();
    w.fill(b'0', 0x80);
    for v in [min_x, -max_y, max_x, -min_y] {
        w.u16le(v.clamp(i16::MIN.into(), i16::MAX.into()));
    }
    w.u32le(0);
    w.u32le(records as i64);
    w.u16le(end_x.clamp(i16::MIN.into(), i16::MAX.into()));
    w.u16le((-end_y).clamp(i16::MIN.into(), i16::MAX.into()));
    w.fill(0, HEADER_SIZE - w.len());
    w.append(&body.bytes);
    Ok(w.bytes)
}
