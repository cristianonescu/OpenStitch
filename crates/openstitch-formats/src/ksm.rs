//! Pfaff KSM (#481, read only): a 512-byte header (not read), then 3-byte records
//! `|dy| |dx| ctrl` in 0.1 mm. `ctrl` 0x20: the move goes down (y up otherwise), 0x40: it
//! goes left. Its low five bits:
//!
//! | Low bits | Meaning |
//! |---|---|
//! | `00`, `17`, `18`, `19`, `1B` | a move: a jump while the machine is moving after a trim or needle change, else a stitch; `17`–`1B` end the moving |
//! | `05` | stitch, then stop |
//! | `07`, `13`, `1D` | stitch (when it moves), then trim; the next moves are jumps |
//! | `0B`–`12` | stitch (when it moves), then needle change; the next moves are jumps |
//! | anything else | end of the design |
//!
//! No colours are stored. There is no public description: the table is what pyembroidery
//! 1.5.1's reader (the test oracle) does with every control value, found by reading crafted
//! files with it (its code was not read or copied); libembroidery's KSM notes give the header
//! size and record order.

use crate::dst;
use crate::records::Builder;
use crate::{FormatError, Result};
use openstitch_core::{Command, StitchPattern};

pub fn read(data: &[u8]) -> Result<StitchPattern> {
    let records = data
        .get(dst::HEADER_SIZE..)
        .ok_or_else(|| FormatError::Truncated("KSM: shorter than its 512-byte header".into()))?;
    let mut b = Builder::new("KSM");
    let mut moving = false;
    for r in records.chunks_exact(3) {
        let (by, bx, ctrl) = (f64::from(r[0]), f64::from(r[1]), r[2]);
        let dy = if ctrl & 0x20 != 0 { by } else { -by };
        let dx = if ctrl & 0x40 != 0 { -bx } else { bx };
        let step = |b: &mut Builder| {
            if dx != 0.0 || dy != 0.0 {
                b.moved(dx, dy, Command::Stitch)
            } else {
                Ok(())
            }
        };
        match ctrl & 0x1F {
            low @ (0x00 | 0x17 | 0x18 | 0x19 | 0x1B) => {
                let command = if moving {
                    Command::Jump
                } else {
                    Command::Stitch
                };
                b.moved(dx, dy, command)?;
                if low != 0 {
                    moving = false;
                }
            }
            0x05 => {
                step(&mut b)?;
                b.command(Command::Stop)?;
            }
            0x07 | 0x13 | 0x1D => {
                step(&mut b)?;
                b.command(Command::Trim)?;
                moving = true;
            }
            0x0B..=0x12 => {
                step(&mut b)?;
                b.needle()?;
                moving = true;
            }
            _ => break,
        }
    }
    Ok(b.finish())
}
