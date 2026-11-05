//! Data Stitch STX (#481, read only): a header whose little-endian u32 at offset 20 is where the
//! stitches start, then 2-byte records `dx dy` (signed bytes, 0.1 mm, y up) with an `80` escape:
//!
//! | Bytes | Meaning |
//! |---|---|
//! | `dx dy` (`dx` not `80`) | stitch |
//! | `80 01 dx dy` | colour change, then a jump by `(dx, dy)` unless it is zero |
//! | `80 02 dx dy` | stitch, even when `dx` is `80` (the escape for a −12.8 mm move) |
//! | `80 04 dx dy` | jump |
//! | `80 80 .. ..` | trim (the last two bytes unused) |
//! | `80` and anything else | end of the design |
//!
//! Colours are not read: the file holds a thread table and a GIF preview (libembroidery's notes
//! describe them, with fields of unknown meaning), but pyembroidery 1.5.1, the test oracle, reads
//! no colours from any STX (3,000 random headers tried), so there is nothing to check a reader
//! against. The table above is what pyembroidery's reader does with every escape value, found
//! by reading crafted files with it; its code was not read or copied; libembroidery's notes give
//! the offset field. No STX written by Data Stitch software was available.

use crate::io::signed8;
use crate::records::Builder;
use crate::{FormatError, Result};
use openstitch_core::{Command, StitchPattern};

pub fn read(data: &[u8]) -> Result<StitchPattern> {
    let start = data
        .get(20..24)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .ok_or_else(|| FormatError::Truncated("STX: shorter than its header".into()))?;
    let mut b = Builder::new("STX");
    let records = data.get(start..).unwrap_or_default();
    let mut i = 0;
    while let Some(&[x, y]) = records.get(i..i + 2) {
        i += 2;
        if x != 0x80 {
            b.moved(signed8(x), -signed8(y), Command::Stitch)?;
            continue;
        }
        let tail = records
            .get(i..i + 2)
            .map(|t| (signed8(t[0]), -signed8(t[1])));
        match (y, tail) {
            (0x02, Some((dx, dy))) => {
                i += 2;
                b.moved(dx, dy, Command::Stitch)?;
            }
            (0x01, Some((dx, dy))) => {
                i += 2;
                b.command(Command::ColorChange)?;
                if dx != 0.0 || dy != 0.0 {
                    b.moved(dx, dy, Command::Jump)?;
                }
            }
            (0x04, Some((dx, dy))) => {
                i += 2;
                b.moved(dx, dy, Command::Jump)?;
            }
            (0x80, Some(_)) => {
                i += 2;
                b.command(Command::Trim)?;
            }
            _ => break,
        }
    }
    Ok(b.finish())
}
