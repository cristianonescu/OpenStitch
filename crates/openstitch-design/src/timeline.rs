//! The stitch-out timeline (#516): a compiled pattern as the machine sews it, in colour blocks
//! with their trims, the objects that start in them, and the machine time at every stitch. The
//! editor's stitch-out player, `design.timeline` and the Assistant share it.
//!
//! Positions count sewn stitches (needle penetrations), as the Stitch Simulator does: position
//! `n` means `n` stitches are sewn. A block runs from `start` (stitches sewn before its first
//! stitch) to `end` (after its last); a trim at `t` happens after `t` stitches. Time follows
//! [`crate::machine::estimate`] record by record, so the time at the end equals its total.

use crate::machine::{MachineSettings, TRIM_SECONDS};
use openstitch_core::{Command, StitchPattern};
use std::ops::Range;

/// What ends a colour block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndKind {
    ColorChange,
    Stop,
    End,
}

impl EndKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EndKind::ColorChange => "colorChange",
            EndKind::Stop => "stop",
            EndKind::End => "end",
        }
    }
}

/// One colour block in sewing order.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// Index into the pattern's threads.
    pub thread: usize,
    pub start: usize,
    pub end: usize,
    pub end_kind: EndKind,
    /// Positions of the trims inside the block.
    pub trims: Vec<usize>,
    /// Objects whose first stitch is in this block: (id, position of that stitch).
    pub objects: Vec<(u64, usize)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timeline {
    pub blocks: Vec<Block>,
    /// Stitches in the pattern.
    pub total: usize,
    /// Machine seconds elapsed when `n` stitches are sewn, `n` in `0..=total`; the last entry
    /// also counts what follows the last stitch (a final trim).
    pub seconds: Vec<f64>,
}

impl Timeline {
    /// The timeline of `pattern`; `spans` are the compiler's per-object record ranges
    /// ([`crate::Compiler::compile_traced`]), `settings` give the machine speed and the trim and
    /// colour-change times.
    pub fn build(
        pattern: &StitchPattern,
        spans: &[(u64, Range<usize>)],
        settings: &MachineSettings,
    ) -> Timeline {
        let per_record = 60.0 / settings.speed();
        let change = settings.machine.color_change_seconds();
        let mut t = Timeline {
            seconds: vec![0.0],
            ..Timeline::default()
        };
        // Stitches sewn before each record.
        let mut before = Vec::with_capacity(pattern.stitches.len());
        let mut elapsed = 0.0;
        let (mut thread, mut start, mut trims) = (0, 0, Vec::new());
        fn close(
            t: &mut Timeline,
            thread: usize,
            start: &mut usize,
            trims: &mut Vec<usize>,
            kind: EndKind,
        ) {
            if t.total > *start {
                t.blocks.push(Block {
                    thread,
                    start: *start,
                    end: t.total,
                    end_kind: kind,
                    trims: std::mem::take(trims),
                    objects: Vec::new(),
                });
            } else {
                trims.clear();
            }
            *start = t.total;
        }
        let mut ended = false;
        for s in &pattern.stitches {
            before.push(t.total);
            if ended {
                continue;
            }
            match s.command {
                Command::Stitch => {
                    elapsed += per_record;
                    t.total += 1;
                    t.seconds.push(elapsed);
                }
                Command::Jump => elapsed += per_record,
                Command::Trim => {
                    elapsed += TRIM_SECONDS;
                    if trims.last() != Some(&t.total) {
                        trims.push(t.total);
                    }
                }
                Command::ColorChange => {
                    elapsed += change;
                    close(&mut t, thread, &mut start, &mut trims, EndKind::ColorChange);
                    thread += 1;
                }
                Command::Stop => close(&mut t, thread, &mut start, &mut trims, EndKind::Stop),
                Command::End => {
                    close(&mut t, thread, &mut start, &mut trims, EndKind::End);
                    ended = true;
                }
            }
        }
        if !ended {
            close(&mut t, thread, &mut start, &mut trims, EndKind::End);
        }
        if let Some(last) = t.seconds.last_mut() {
            *last = elapsed.max(*last);
        }
        for (id, range) in spans {
            let first = range.clone().find(|&i| {
                pattern
                    .stitches
                    .get(i)
                    .is_some_and(|s| s.command == Command::Stitch)
            });
            if let Some(i) = first {
                let at = before[i];
                if let Some(b) = t.blocks.iter_mut().find(|b| at >= b.start && at < b.end) {
                    b.objects.push((*id, at));
                }
            }
        }
        t
    }

    /// Machine seconds when `n` stitches are sewn (the whole design past the end).
    pub fn seconds_at(&self, n: usize) -> f64 {
        self.seconds.get(n.min(self.total)).copied().unwrap_or(0.0)
    }

    pub fn total_seconds(&self) -> f64 {
        self.seconds_at(self.total)
    }

    /// The block the next stitch after position `n` belongs to (the last block at the end).
    pub fn block_at(&self, n: usize) -> Option<usize> {
        self.blocks
            .iter()
            .position(|b| n < b.end)
            .or_else(|| self.blocks.len().checked_sub(1))
    }

    fn starts(&self) -> Vec<usize> {
        self.blocks.iter().map(|b| b.start).collect()
    }

    /// Object starts in sewing order.
    pub fn object_starts(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self
            .blocks
            .iter()
            .flat_map(|b| b.objects.iter().map(|o| o.1))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Trims, in order.
    pub fn trims(&self) -> Vec<usize> {
        self.blocks.iter().flat_map(|b| b.trims.clone()).collect()
    }

    /// Where the machine pauses or cuts: every trim and every block end but the last.
    pub fn pauses(&self) -> Vec<usize> {
        let mut v = self.trims();
        v.extend(
            self.blocks
                .iter()
                .filter(|b| b.end_kind != EndKind::End)
                .map(|b| b.end),
        );
        v.sort_unstable();
        v.dedup();
        v
    }

    /// The first pause `p` with `from < p <= to`.
    pub fn first_pause_in(&self, from: usize, to: usize) -> Option<usize> {
        self.pauses().into_iter().find(|&p| p > from && p <= to)
    }

    /// Next and previous positions in a sorted list of marks.
    fn next_in(marks: &[usize], n: usize) -> Option<usize> {
        marks.iter().copied().find(|&m| m > n)
    }

    fn prev_in(marks: &[usize], n: usize) -> Option<usize> {
        marks.iter().rev().copied().find(|&m| m < n)
    }

    /// The first stitch of the next block (the end when there is none).
    pub fn next_block(&self, n: usize) -> usize {
        Self::next_in(&self.starts(), n).unwrap_or(self.total)
    }

    /// The first stitch of this block, or of the previous one when already there.
    pub fn prev_block(&self, n: usize) -> usize {
        Self::prev_in(&self.starts(), n).unwrap_or(0)
    }

    pub fn next_object(&self, n: usize) -> usize {
        Self::next_in(&self.object_starts(), n).unwrap_or(self.total)
    }

    pub fn prev_object(&self, n: usize) -> usize {
        Self::prev_in(&self.object_starts(), n).unwrap_or(0)
    }

    /// The next trim after `n`, if any.
    pub fn next_trim(&self, n: usize) -> Option<usize> {
        Self::next_in(&self.trims(), n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::{MachineType, estimate};

    /// Two colours: 3 stitches, a trim, 2 stitches; colour change; 2 stitches, a stop, 1 stitch.
    fn pattern() -> StitchPattern {
        let mut p = StitchPattern::new();
        for x in 0..3 {
            p.stitch_to(f64::from(x) * 10.0, 0.0);
        }
        p.push(Command::Trim);
        p.jump_to(100.0, 0.0);
        p.stitch_to(110.0, 0.0);
        p.stitch_to(120.0, 0.0);
        p.push(Command::ColorChange);
        p.stitch_to(0.0, 50.0);
        p.stitch_to(10.0, 50.0);
        p.push(Command::Stop);
        p.stitch_to(20.0, 50.0);
        p.push(Command::End);
        p
    }

    #[test]
    fn blocks_trims_and_stops_follow_the_pattern() {
        let p = pattern();
        let t = Timeline::build(
            &p,
            &[(7, 0..3), (8, 4..7), (9, 8..10)],
            &MachineSettings::default(),
        );
        assert_eq!(t.total, 8);
        let ends: Vec<_> = t
            .blocks
            .iter()
            .map(|b| (b.thread, b.start, b.end, b.end_kind))
            .collect();
        assert_eq!(
            ends,
            [
                (0, 0, 5, EndKind::ColorChange),
                (1, 5, 7, EndKind::Stop),
                (1, 7, 8, EndKind::End)
            ]
        );
        assert_eq!(t.blocks[0].trims, [3]);
        assert_eq!(t.trims(), [3]);
        assert_eq!(t.blocks[0].objects, [(7, 0), (8, 3)]);
        assert_eq!(t.blocks[1].objects, [(9, 5)]);
        assert_eq!(t.pauses(), [3, 5, 7]);
        assert_eq!(t.first_pause_in(3, 8), Some(5));
        assert_eq!(t.first_pause_in(0, 2), None);
    }

    #[test]
    fn stepping_lands_on_blocks_objects_and_trims() {
        let t = Timeline::build(
            &pattern(),
            &[(7, 0..3), (8, 4..7), (9, 8..10)],
            &MachineSettings::default(),
        );
        assert_eq!(t.next_block(0), 5);
        assert_eq!(t.next_block(5), 7);
        assert_eq!(t.next_block(7), 8);
        assert_eq!(t.prev_block(6), 5);
        assert_eq!(t.prev_block(5), 0);
        assert_eq!(t.next_object(0), 3);
        assert_eq!(t.next_object(3), 5);
        assert_eq!(t.prev_object(5), 3);
        assert_eq!(t.next_trim(0), Some(3));
        assert_eq!(t.next_trim(3), None);
        assert_eq!(t.block_at(4), Some(0));
        assert_eq!(t.block_at(5), Some(1));
        assert_eq!(t.block_at(8), Some(2));
    }

    #[test]
    fn time_at_the_end_equals_the_estimate() {
        let p = pattern();
        for machine in [MachineType::SingleNeedle, MachineType::MultiNeedle] {
            let m = MachineSettings {
                machine,
                speed: Some(600.0),
                ..MachineSettings::default()
            };
            let t = Timeline::build(&p, &[], &m);
            let e = estimate(&p, &m);
            assert!((t.total_seconds() - e.seconds).abs() < 1e-9, "{machine:?}");
            // 600 per minute: 0.1 s a record; three stitches in, 0.3 s.
            assert!((t.seconds_at(3) - 0.3).abs() < 1e-9);
            assert!(t.seconds.windows(2).all(|w| w[0] <= w[1]));
        }
        assert_eq!(
            Timeline::build(&StitchPattern::new(), &[], &MachineSettings::default()).total,
            0
        );
    }
}
