//! Barudan DSB (#481, read only): a 512-byte header laid out like DST's, then 3-byte records
//! `ctrl |dy| |dx|` in 0.1 mm. `ctrl` 0x40: the move goes down (y up otherwise), 0x20: it
//! goes left; low five bits 0 stitch, 1 jump. Whole bytes: `E7` trim, `E8` stop, `E9`–`F7`
//! needle change; anything else ends the design. Control records do not move.
//!
//! Layout from Roidery's notes (MIT OR Apache-2.0), every control byte checked against
//! pyembroidery 1.5.1's reader (the test oracle).

use crate::records::Builder;
use crate::{FormatError, Result, dst};
use openstitch_core::{Command, StitchPattern};

pub fn read(data: &[u8]) -> Result<StitchPattern> {
    let header = data
        .get(..dst::HEADER_SIZE)
        .ok_or_else(|| FormatError::Truncated("DSB: shorter than its 512-byte header".into()))?;
    let mut b = Builder::new("DSB");
    dst::parse_header(header, &mut b.pattern);
    b.pattern.metadata.retain(|k, _| !k.starts_with("dst."));
    for r in data[dst::HEADER_SIZE..].chunks_exact(3) {
        let (ctrl, by, bx) = (r[0], f64::from(r[1]), f64::from(r[2]));
        let dy = if ctrl & 0x40 != 0 { by } else { -by };
        let dx = if ctrl & 0x20 != 0 { -bx } else { bx };
        match (ctrl, ctrl & 0x1F) {
            (_, 0) => b.moved(dx, dy, Command::Stitch)?,
            (_, 1) => b.moved(dx, dy, Command::Jump)?,
            (0xE7, _) => b.command(Command::Trim)?,
            (0xE8, _) => b.command(Command::Stop)?,
            (0xE9..=0xF7, _) => b.needle()?,
            _ => break,
        }
    }
    Ok(b.finish())
}
