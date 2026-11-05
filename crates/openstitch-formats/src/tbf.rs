//! Tajima TBF (#481): a 0x600-byte header, then 3-byte records `dx dy ctrl` (signed bytes,
//! 0.1 mm, y up) and a closing `1A`.
//!
//! | Offset | Content |
//! |---|---|
//! | 0x000 | `3.00`, spaces to 0x80 |
//! | 0x080 | DST-like lines ending in CR: `LA:` name (16), `ST:` records (7), `CO:` colour blocks (3), `+X:` `-X:` `+Y:` `-Y:` extents (5, y up), `AX:` `AY:` end point (sign and 5), `TP:` 32 characters |
//! | | `JC:3` CR, `DO:` one needle byte per colour block in 256 bytes, CR |
//! | | `DA:` per colour block `45 r g b 20`, spaces to 72 entries, CR, `1A`, spaces to 0x600 |
//!
//! `ctrl`: `80` stitch, `90` jump, `86` trim, `81` needle change (to the next needle in
//! `DO:`), `40` stop, `8F` end; anything else ends the design.
//!
//! There is no public description: the layout is what pyembroidery 1.5.1 writes, read from
//! its output files, and the control values are what its reader does with crafted files (its
//! code was not read or copied). Moves are split at 10 mm, as pyembroidery writes them. The
//! writer gives each distinct colour its own needle in the order the colours are first sewn
//! (after 15 it starts again at 1). Writing is experimental: no file from Tajima software and
//! no sew-out yet.

use crate::io::{ByteReader, ByteWriter, padded, signed8};
use crate::records::{self, Builder, Op};
use crate::{FormatError, Result};
use openstitch_core::{Command, StitchPattern, Thread};

pub const HEADER_SIZE: usize = 0x600;
pub const MAX_DELTA: i64 = 100;
pub const NEEDLES: usize = 15;
/// Colour entries the `DA:` table holds.
pub const MAX_COLORS: usize = 72;
const DO_AT: usize = 0x10A;
const DA_AT: usize = 0x20E;

pub fn read(data: &[u8]) -> Result<StitchPattern> {
    if data.len() < HEADER_SIZE {
        return Err(FormatError::Truncated(
            "TBF: shorter than its 0x600-byte header".into(),
        ));
    }
    let mut b = Builder::new("TBF");
    crate::dst::parse_header(&data[0x80..DO_AT], &mut b.pattern);
    b.pattern.metadata.retain(|k, _| !k.starts_with("dst."));
    let mut colours = ByteReader::new(&data[DA_AT..0x376]);
    while let Ok(e) = colours.read(5) {
        if e[0] != 0x45 {
            break;
        }
        b.pattern
            .threads
            .push(Thread::rgb(u32::from_be_bytes([0, e[1], e[2], e[3]])));
    }
    for r in data[HEADER_SIZE..].chunks_exact(3) {
        let (dx, dy) = (signed8(r[0]), -signed8(r[1]));
        match r[2] {
            0x80 => b.moved(dx, dy, Command::Stitch)?,
            0x90 => b.moved(dx, dy, Command::Jump)?,
            0x86 => b.command(Command::Trim)?,
            0x81 => b.needle()?,
            0x40 => b.command(Command::Stop)?,
            _ => break,
        }
    }
    Ok(b.finish())
}

fn sign(v: i64) -> char {
    if v < 0 { '-' } else { '+' }
}

pub fn write(pattern: &StitchPattern) -> Result<Vec<u8>> {
    let ops = records::ops(pattern, MAX_DELTA);
    let needles = records::needles(pattern, NEEDLES);
    if needles.len() > MAX_COLORS {
        return Err(FormatError::ValueOutOfRange(format!(
            "TBF: {} colour blocks (at most {MAX_COLORS})",
            needles.len()
        )));
    }
    let mut body = ByteWriter::default();
    for op in &ops {
        match *op {
            Op::Stitch(dx, dy) => body.append(&[dx as u8, (-dy) as u8, 0x80]),
            Op::Jump(dx, dy) => body.append(&[dx as u8, (-dy) as u8, 0x90]),
            Op::Trim => body.append(&[0, 0, 0x86]),
            Op::Stop => body.append(&[0, 0, 0x40]),
            Op::Needle(_) => body.append(&[0, 0, 0x81]),
            Op::End => body.append(&[0, 0, 0x8F]),
        }
    }
    let ((min_x, min_y, max_x, max_y), (end_x, end_y)) = records::extents(&ops);
    let mut w = ByteWriter::default();
    w.ascii("3.00");
    w.fill(0x20, 0x80 - w.len());
    let name = pattern.name().unwrap_or("Untitled");
    w.ascii(&format!("LA:{}\r", padded(name, 16)));
    w.ascii(&format!("ST:{:>7}\r", body.len() / 3));
    w.ascii(&format!("CO:{:>3}\r", needles.len()));
    w.ascii(&format!("+X:{max_x:>5}\r-X:{:>5}\r", -min_x));
    w.ascii(&format!("+Y:{:>5}\r-Y:{max_y:>5}\r", -min_y));
    w.ascii(&format!("AX:{}{:>5}\r", sign(end_x), end_x.abs()));
    w.ascii(&format!("AY:{}{:>5}\r", sign(-end_y), end_y.abs()));
    w.ascii(&format!("TP:{}\r", padded("EG/", 32)));
    w.ascii("JC:3\rDO:");
    if w.len() != DO_AT {
        return Err(FormatError::ValueOutOfRange(
            "TBF: the name does not fit the header line".into(),
        ));
    }
    for &n in &needles {
        w.u8(n as i64);
    }
    w.fill(0, DO_AT + 256 - w.len());
    w.ascii("\rDA:");
    for (i, _) in needles.iter().enumerate() {
        let t = pattern
            .threads
            .get(i)
            .or(pattern.threads.last())
            .cloned()
            .unwrap_or_else(|| Thread::rgb(0));
        w.append(&[0x45, t.red, t.green, t.blue, 0x20]);
    }
    w.fill(0x20, 0x376 - w.len());
    w.append(&[0x0D, 0x1A]);
    w.fill(0x20, HEADER_SIZE - w.len());
    w.append(&body.bytes);
    w.u8(0x1A);
    Ok(w.bytes)
}
