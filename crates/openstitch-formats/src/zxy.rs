//! ZSK ZXY (#481, read only): byte 0 unused, a u16 big-endian count of bytes to skip at 1–2,
//! then 3-byte records `ctrl |dx| |dy|` in 0.1 mm from offset `3 + skip`; a trailing partial
//! record is ignored.
//!
//! | `ctrl` | Meaning |
//! |---|---|
//! | bit `02` set | jump (whatever else is set) |
//! | else bit `20` set | needle change (the record does not move); with x byte `FF`, the end of the design |
//! | else any of `01`, `10`, `40`, `80` set | ignored |
//! | else | stitch |
//!
//! Bit `08` makes the x move negative (left), bit `04` makes the y move positive (down; y is up
//! otherwise). No colours are stored.
//!
//! There is no public description. The table is what pyembroidery 1.5.1's reader (the test
//! oracle) does with every control value and header layout, found by reading crafted files with
//! it; its code was not read or copied. No ZXY written by ZSK software was available.

use crate::records::Builder;
use crate::{FormatError, Result};
use openstitch_core::{Command, StitchPattern};

pub fn read(data: &[u8]) -> Result<StitchPattern> {
    let skip = data
        .get(1..3)
        .map(|b| usize::from(u16::from_be_bytes([b[0], b[1]])))
        .ok_or_else(|| FormatError::Truncated("ZXY: shorter than its 3-byte header".into()))?;
    let mut b = Builder::new("ZXY");
    let records = data.get(3 + skip..).unwrap_or_default();
    for r in records.as_chunks::<3>().0 {
        let (ctrl, bx, by) = (r[0], f64::from(r[1]), f64::from(r[2]));
        let dx = if ctrl & 0x08 != 0 { -bx } else { bx };
        let dy = if ctrl & 0x04 != 0 { by } else { -by };
        if ctrl & 0x02 != 0 {
            b.moved(dx, dy, Command::Jump)?;
        } else if ctrl & 0x20 != 0 {
            if r[1] == 0xFF {
                break;
            }
            b.needle()?;
        } else if ctrl & 0xD1 == 0 {
            b.moved(dx, dy, Command::Stitch)?;
        }
    }
    Ok(b.finish())
}
