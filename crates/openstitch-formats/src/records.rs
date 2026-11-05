//! Shared pieces of the relative-record machine formats added for #481 (U01, TBF, DSB, DSZ,
//! KSM): a reader-side builder and a writer-side list of operations.

use crate::layout::{self, Event};
use crate::{FormatError, Result};
use openstitch_core::{Command, Stitch, StitchPattern, Thread};

/// Upper bound on the records a reader accepts.
pub const MAX_RECORDS: usize = 4_000_000;

/// Builds a pattern from relative moves (0.1 mm, +y down).
pub(crate) struct Builder {
    pub pattern: StitchPattern,
    x: f64,
    y: f64,
    format: &'static str,
}

impl Builder {
    pub fn new(format: &'static str) -> Self {
        Self {
            pattern: StitchPattern::new(),
            x: 0.0,
            y: 0.0,
            format,
        }
    }

    fn push(&mut self, command: Command) -> Result<()> {
        if self.pattern.stitches.len() >= MAX_RECORDS {
            return Err(FormatError::ValueOutOfRange(format!(
                "{}: more than {MAX_RECORDS} records",
                self.format
            )));
        }
        self.pattern
            .stitches
            .push(Stitch::new(self.x, self.y, command));
        Ok(())
    }

    /// A stitch or jump by `(dx, dy)`.
    pub fn moved(&mut self, dx: f64, dy: f64, command: Command) -> Result<()> {
        self.x += dx;
        self.y += dy;
        self.push(command)
    }

    pub fn command(&mut self, command: Command) -> Result<()> {
        self.push(command)
    }

    /// A needle change: a colour change once something has been sewn (the needle set at the
    /// start of a file chooses the first colour).
    pub fn needle(&mut self) -> Result<()> {
        if self.pattern.count(Command::Stitch) > 0 {
            self.push(Command::ColorChange)?;
        }
        Ok(())
    }

    pub fn finish(mut self) -> StitchPattern {
        self.pattern
            .stitches
            .push(Stitch::new(self.x, self.y, Command::End));
        self.pattern
    }
}

/// One record a writer lays out: moves are relative, 0.1 mm, +y down, and within `max` on
/// each axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    Stitch(i64, i64),
    Jump(i64, i64),
    Trim,
    Stop,
    /// Needle change to colour block `n` (0 for the first, written before the first stitch).
    Needle(usize),
    End,
}

/// The pattern, centred as every writer centres it (`layout::events`), as records: a needle
/// set first; a trim before each trimmed frame move (none before the first); frame moves as
/// jumps of at most `max`; a stitch longer than `max` as jumps and one stitch for the rest, so
/// the needle goes down once; no move back to the origin at the end.
pub(crate) fn ops(pattern: &StitchPattern, max: i64) -> Vec<Op> {
    let mut out = vec![Op::Needle(0)];
    let (mut x, mut y) = (0_i64, 0_i64);
    let mut block = 0;
    let mut sewn = false;
    for e in layout::events(pattern) {
        match e {
            Event::Move {
                x: tx,
                y: ty,
                trimmed,
            } => {
                if trimmed && sewn && !matches!(out.last(), Some(Op::Trim | Op::Needle(_))) {
                    out.push(Op::Trim);
                }
                if (tx, ty) != (x, y) {
                    for (dx, dy) in layout::split_proportional(tx - x, ty - y, max) {
                        out.push(Op::Jump(dx, dy));
                    }
                }
                (x, y) = (tx, ty);
            }
            Event::Stitch { x: tx, y: ty } => {
                let pieces = layout::split_proportional(tx - x, ty - y, max);
                let (last, head) = pieces.split_last().expect("at least one piece");
                for &(dx, dy) in head {
                    out.push(Op::Jump(dx, dy));
                }
                out.push(Op::Stitch(last.0, last.1));
                (x, y) = (tx, ty);
                sewn = true;
            }
            Event::ColorChange => {
                block += 1;
                out.push(Op::Needle(block));
            }
            Event::Stop => out.push(Op::Stop),
            Event::End => break,
        }
    }
    out.push(Op::End);
    out
}

/// The needle (1 to `needles`) for each colour block: one needle per distinct colour, in the
/// order the colours are first sewn, starting again at 1 after `needles` distinct colours.
pub(crate) fn needles(pattern: &StitchPattern, needles: usize) -> Vec<usize> {
    let blocks = pattern.count(Command::ColorChange) + 1;
    let mut seen: Vec<u32> = Vec::new();
    (0..blocks)
        .map(|i| {
            let rgb = pattern
                .threads
                .get(i)
                .or(pattern.threads.last())
                .map_or(0, Thread::rgb_value);
            let at = seen.iter().position(|&c| c == rgb).unwrap_or_else(|| {
                seen.push(rgb);
                seen.len() - 1
            });
            at % needles + 1
        })
        .collect()
}

/// The pattern's bounds over the needle points and frame moves, centred (0.1 mm, +y down):
/// `(min x, min y, max x, max y)`, and the last position.
pub(crate) fn extents(ops: &[Op]) -> ((i64, i64, i64, i64), (i64, i64)) {
    let (mut x, mut y) = (0_i64, 0_i64);
    let mut b = (0, 0, 0, 0);
    for op in ops {
        if let Op::Stitch(dx, dy) | Op::Jump(dx, dy) = *op {
            x += dx;
            y += dy;
            b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
        }
    }
    (b, (x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_colour_keeps_its_needle() {
        let mut p = StitchPattern::new();
        p.threads = vec![
            Thread::rgb(0xFF0000),
            Thread::rgb(0x00FF00),
            Thread::rgb(0xFF0000),
            Thread::rgb(0x0000FF),
        ];
        for _ in 0..3 {
            p.stitch_to(0.0, 0.0);
            p.push(Command::ColorChange);
        }
        p.stitch_to(0.0, 0.0);
        assert_eq!(needles(&p, 15), [1, 2, 1, 3]);
        assert_eq!(needles(&p, 2), [1, 2, 1, 1]);
    }

    #[test]
    fn long_stitches_are_jumps_and_one_stitch() {
        let mut p = StitchPattern::new();
        p.threads = vec![Thread::rgb(0)];
        p.stitch_to(0.0, 0.0);
        p.stitch_to(250.0, 0.0);
        p.push(Command::Trim);
        p.jump_to(250.0, 300.0);
        p.stitch_to(250.0, 300.0);
        p.push(Command::End);
        let ops = ops(&p, 100);
        // Centred: x −125..125, y −150..150; the longer axis of a split move is full.
        assert_eq!(
            ops,
            [
                Op::Needle(0),
                Op::Jump(-83, -100),
                Op::Jump(-42, -50),
                Op::Stitch(0, 0),
                Op::Jump(100, 0),
                Op::Jump(100, 0),
                Op::Stitch(50, 0),
                Op::Trim,
                Op::Jump(0, 100),
                Op::Jump(0, 100),
                Op::Jump(0, 100),
                Op::Stitch(0, 0),
                Op::End
            ]
        );
    }
}
