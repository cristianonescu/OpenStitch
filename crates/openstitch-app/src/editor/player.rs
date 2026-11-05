//! The stitch-out player (#516), without egui: stepping by stitch, colour block, object and
//! trim over a [`Timeline`], playback that can pause where the machine pauses, the Auto speed,
//! the counters and following the needle. Positions are the Stitch Simulator's: `Some(n)` shows
//! `n` stitches, `None` the whole design.

use super::viewport::{ScreenRect, Viewport};
use openstitch_design::Vec2;
use openstitch_design::timeline::Timeline;

/// A player step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Start,
    End,
    StitchBack,
    StitchForward,
    /// The first stitch of this block, or of the previous one when already there.
    BlockBack,
    /// The first stitch of the next block.
    BlockForward,
    ObjectBack,
    ObjectForward,
    /// The next trim.
    NextTrim,
}

impl Step {
    pub fn key(self) -> &'static str {
        match self {
            Step::Start => "start",
            Step::End => "end",
            Step::StitchBack => "stitchBack",
            Step::StitchForward => "stitch",
            Step::BlockBack => "blockBack",
            Step::BlockForward => "block",
            Step::ObjectBack => "objectBack",
            Step::ObjectForward => "object",
            Step::NextTrim => "trim",
        }
    }

    pub const ALL: [Step; 9] = [
        Step::Start,
        Step::End,
        Step::StitchBack,
        Step::StitchForward,
        Step::BlockBack,
        Step::BlockForward,
        Step::ObjectBack,
        Step::ObjectForward,
        Step::NextTrim,
    ];

    pub fn from_key(k: &str) -> Option<Step> {
        Step::ALL.into_iter().find(|s| s.key() == k)
    }
}

/// A position as a stitch count.
pub fn at(t: &Timeline, pos: Option<usize>) -> usize {
    pos.map_or(t.total, |n| n.min(t.total))
}

/// The simulator position for stitch count `n` (`None` at the end).
pub fn position(t: &Timeline, n: usize) -> Option<usize> {
    (n < t.total).then_some(n)
}

/// Where `s` moves the player from `pos`.
pub fn step(t: &Timeline, pos: Option<usize>, s: Step) -> Option<usize> {
    let n = at(t, pos);
    let to = match s {
        Step::Start => 0,
        Step::End => t.total,
        Step::StitchBack => n.saturating_sub(1),
        Step::StitchForward => n + 1,
        Step::BlockBack => t.prev_block(n),
        Step::BlockForward => t.next_block(n),
        Step::ObjectBack => t.prev_object(n),
        Step::ObjectForward => t.next_object(n),
        Step::NextTrim => t.next_trim(n).unwrap_or(n),
    };
    position(t, to.min(t.total))
}

/// The Auto speed: the whole design in [`AUTO_SECONDS`], stitches per second.
pub const AUTO_SECONDS: f64 = 12.0;

pub fn auto_speed(t: &Timeline) -> f64 {
    (t.total as f64 / AUTO_SECONDS).max(1.0)
}

/// Speed presets, stitches per second.
pub const SPEEDS: [f64; 7] = [12.0, 50.0, 200.0, 400.0, 1000.0, 2000.0, 5000.0];

/// Playback from `pos` by `stitches`: the new position and whether it stopped at a pause
/// (a trim, a stop or a colour change) because `pause` is on, or at the end.
pub fn advance(
    t: &Timeline,
    pos: Option<usize>,
    stitches: usize,
    pause: bool,
) -> (Option<usize>, bool) {
    let n = at(t, pos);
    let mut to = (n + stitches).min(t.total);
    let mut stopped = to >= t.total;
    if pause && let Some(p) = t.first_pause_in(n, to) {
        to = p;
        stopped = true;
    }
    (position(t, to), stopped)
}

/// `m:ss` (or `h:mm:ss` from an hour).
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// "stitch n / N · m:ss / m:ss": the needle's stitch and machine time out of the totals.
pub fn counter(t: &Timeline, pos: Option<usize>) -> String {
    let n = at(t, pos);
    format!(
        "stitch {n} / {} · {} / {}",
        t.total,
        clock(t.seconds_at(n)),
        clock(t.total_seconds())
    )
}

/// The stitch count at fraction `f` (0..1) of the timeline's width (click or drag to seek).
pub fn seek(t: &Timeline, f: f64) -> Option<usize> {
    let f = if f.is_finite() {
        f.clamp(0.0, 1.0)
    } else {
        0.0
    };
    position(t, (f * t.total as f64).round() as usize)
}

/// Follow the needle: the viewport moved so `needle` (model units) sits in the middle when it
/// has left the inner 80 % of `screen`; `None` while it is inside.
pub fn follow(vp: &Viewport, screen: ScreenRect, needle: Vec2) -> Option<Viewport> {
    let p = vp.to_screen(needle);
    let (mx, my) = (screen.width * 0.1, screen.height * 0.1);
    let inside = p.x >= screen.x + mx
        && p.x <= screen.x + screen.width - mx
        && p.y >= screen.y + my
        && p.y <= screen.y + screen.height - my;
    if inside {
        return None;
    }
    let c = screen.center();
    let mut out = *vp;
    out.offset.x += c.x - p.x;
    out.offset.y += c.y - p.y;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_core::{Command, StitchPattern};
    use openstitch_design::MachineSettings;
    use openstitch_design::v2;

    /// 3 stitches, trim, 2 stitches | colour change | 2 stitches, stop, 1 stitch.
    fn timeline() -> Timeline {
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
        Timeline::build(
            &p,
            &[(1, 0..3), (2, 4..7), (3, 8..10)],
            &MachineSettings::default(),
        )
    }

    #[test]
    fn keys_land_on_blocks_objects_and_trims() {
        let t = timeline();
        assert_eq!(step(&t, Some(1), Step::BlockForward), Some(5), "Shift+→");
        assert_eq!(
            step(&t, Some(5), Step::BlockBack),
            Some(0),
            "Shift+← from a block start"
        );
        assert_eq!(step(&t, Some(6), Step::BlockBack), Some(5));
        assert_eq!(step(&t, Some(0), Step::ObjectForward), Some(3), "Ctrl+→");
        assert_eq!(step(&t, Some(4), Step::ObjectBack), Some(3));
        assert_eq!(step(&t, Some(0), Step::NextTrim), Some(3), "T");
        assert_eq!(
            step(&t, Some(3), Step::NextTrim),
            Some(3),
            "no later trim: stays"
        );
        assert_eq!(
            step(&t, Some(7), Step::StitchForward),
            None,
            "past the last: the end"
        );
        assert_eq!(step(&t, None, Step::StitchBack), Some(7));
        assert_eq!(step(&t, None, Step::Start), Some(0));
        assert_eq!(step(&t, Some(2), Step::End), None);
        assert_eq!(step(&t, Some(7), Step::BlockForward), None);
        for s in Step::ALL {
            assert_eq!(Step::from_key(s.key()), Some(s));
        }
    }

    #[test]
    fn pause_at_trims_stops_at_each_trim_stop_and_colour_change() {
        let t = timeline();
        let mut pos = Some(0);
        let mut stops = Vec::new();
        loop {
            let (next, stopped) = advance(&t, pos, 100, true);
            if stopped {
                stops.push(at(&t, next));
            }
            if next.is_none() {
                break;
            }
            pos = next;
        }
        assert_eq!(stops, [3, 5, 7, 8], "trim, colour change, stop, end");
        assert_eq!(advance(&t, Some(0), 100, false), (None, true));
        assert_eq!(advance(&t, Some(0), 2, true), (Some(2), false));
    }

    #[test]
    fn auto_plays_any_design_in_about_twelve_seconds() {
        let mut t = timeline();
        t.total = 30_000;
        let speed = auto_speed(&t);
        assert!((t.total as f64 / speed - AUTO_SECONDS).abs() < 2.0);
        assert_eq!(auto_speed(&Timeline::default()), 1.0);
    }

    #[test]
    fn counters_and_seek() {
        let t = timeline();
        // Default speed 800 per minute: 0.075 s a record.
        assert_eq!(counter(&t, Some(3)), "stitch 3 / 8 · 0:00 / 0:04");
        assert_eq!(clock(3725.0), "1:02:05");
        assert_eq!(clock(65.4), "1:05");
        assert_eq!(seek(&t, 0.5), Some(4));
        assert_eq!(seek(&t, 1.0), None);
        assert_eq!(seek(&t, f64::NAN), Some(0));
    }

    #[test]
    fn following_the_needle_recentres_only_when_it_leaves_the_view() {
        let screen = ScreenRect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 300.0,
        };
        let vp = Viewport {
            scale: 1.0,
            offset: v2(0.0, 0.0),
        };
        assert!(follow(&vp, screen, v2(200.0, 150.0)).is_none());
        let moved = follow(&vp, screen, v2(1000.0, 150.0)).expect("left the view");
        let p = moved.to_screen(v2(1000.0, 150.0));
        assert!((p.x - 200.0).abs() < 1e-9 && (p.y - 150.0).abs() < 1e-9);
    }
}
