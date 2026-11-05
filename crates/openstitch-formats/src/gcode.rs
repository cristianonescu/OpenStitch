//! G-code for DIY and CNC embroidery machines (#488): write only, with a parser for the tests.
//!
//! The output is plain text, millimetres, absolute coordinates (`G21`, `G90`), y up as CNC
//! machines take it (the design's +y down is negated). A header of comments records the
//! machine profile settings. Per record:
//!
//! | Record | G-code |
//! |---|---|
//! | jump | `G0 X.. Y..` (travel, the needle stays up) |
//! | stitch | `G1 X.. Y.. F..` then the needle command, if one is set (one penetration) |
//! | trim | the trim command if set, else the comment `; trim` |
//! | colour change | `M0 ; colour change N: #rrggbb brand code name` (the machine pauses) |
//! | stop | `M0 ; stop` |
//! | end | `M2 ; end` |
//!
//! There is no standard for embroidery G-code; the needle and trim commands differ per build
//! (Marlin and GRBL machines often use an M-code or a Z move), so both are settings, empty by
//! default. pyembroidery's G-code writer was not used or consulted.

use openstitch_core::{Command, StitchPattern};

/// What a machine needs besides the moves.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// Written after every `G1` stitch move (one needle penetration), e.g. `M3`; empty: none.
    pub needle: String,
    /// Written for a trim; empty: a `; trim` comment.
    pub trim: String,
    /// Feed rate for stitch moves, mm/min.
    pub feed: f64,
    /// Free text for the header (the machine's name).
    pub profile: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            needle: String::new(),
            trim: String::new(),
            feed: 6000.0,
            profile: String::new(),
        }
    }
}

fn mm(v: f64) -> String {
    let s = format!("{:.2}", v / 10.0);
    if s == "-0.00" { "0.00".into() } else { s }
}

fn one_line(s: &str) -> String {
    s.replace(['\r', '\n', ';', '(', ')'], " ")
        .trim()
        .to_owned()
}

pub fn write(pattern: &StitchPattern, o: &Options) -> Vec<u8> {
    let mut g = String::new();
    g.push_str("; OpenStitch G-code for embroidery machines\n");
    if !o.profile.is_empty() {
        g.push_str(&format!("; machine: {}\n", one_line(&o.profile)));
    }
    g.push_str(&format!(
        "; units mm, absolute, y up; needle command: {}; trim command: {}; feed {:.0} mm/min\n",
        if o.needle.is_empty() {
            "none"
        } else {
            &o.needle
        },
        if o.trim.is_empty() { "none" } else { &o.trim },
        o.feed
    ));
    let stitches = pattern.count(Command::Stitch);
    g.push_str(&format!(
        "; {stitches} stitches, {} colours\n",
        pattern.color_block_count()
    ));
    g.push_str("G21\nG90\n");
    let mut block = 0;
    for s in &pattern.stitches {
        let (x, y) = (mm(s.x), mm(-s.y));
        match s.command {
            Command::Jump => g.push_str(&format!("G0 X{x} Y{y}\n")),
            Command::Stitch => {
                g.push_str(&format!("G1 X{x} Y{y} F{:.0}\n", o.feed));
                if !o.needle.is_empty() {
                    g.push_str(&o.needle);
                    g.push('\n');
                }
            }
            Command::Trim => {
                if o.trim.is_empty() {
                    g.push_str("; trim\n");
                } else {
                    g.push_str(&o.trim);
                    g.push('\n');
                }
            }
            Command::ColorChange => {
                block += 1;
                let t = pattern.threads.get(block);
                let what = t.map_or(String::new(), |t| {
                    one_line(&format!(
                        "{} {} {} {}",
                        t.hex(),
                        t.brand.as_deref().unwrap_or(""),
                        t.catalog_number.as_deref().unwrap_or(""),
                        t.name.as_deref().unwrap_or("")
                    ))
                });
                g.push_str(&format!("M0 ; colour change {}: {what}\n", block + 1));
            }
            Command::Stop => g.push_str("M0 ; stop\n"),
            Command::End => {
                g.push_str("M2 ; end\n");
                break;
            }
        }
    }
    g.into_bytes()
}

/// What [`parse`] finds: G0/G1 points (mm, y up, `true` for G1), colour changes and stops.
#[derive(Debug, Default, PartialEq)]
pub struct Parsed {
    pub moves: Vec<(f64, f64, bool)>,
    pub color_changes: usize,
    pub stops: usize,
}

/// Reads back the moves and pauses of G-code written by [`write`].
pub fn parse(text: &str) -> Result<Parsed, String> {
    let mut p = Parsed::default();
    for (n, line) in text.lines().enumerate() {
        let (code, comment) = line.split_once(';').unwrap_or((line, ""));
        let mut words = code.split_whitespace();
        match words.next() {
            Some(w @ ("G0" | "G1")) => {
                let (mut x, mut y) = (None, None);
                for word in words {
                    let v = word.get(1..).and_then(|v| v.parse::<f64>().ok());
                    match word.chars().next() {
                        Some('X') => x = v,
                        Some('Y') => y = v,
                        _ => {}
                    }
                }
                let (Some(x), Some(y)) = (x, y) else {
                    return Err(format!("line {}: a move without X and Y", n + 1));
                };
                p.moves.push((x, y, w == "G1"));
            }
            Some("M0") if comment.trim_start().starts_with("colour change") => {
                p.color_changes += 1;
            }
            Some("M0") => p.stops += 1,
            _ => {}
        }
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_core::Thread;

    fn sample() -> StitchPattern {
        let mut p = StitchPattern::new();
        p.threads = vec![
            Thread::rgb(0xFF0000),
            Thread::rgb(0x0000FF).with_name("Navy"),
        ];
        p.stitch_to(0.0, 0.0);
        p.stitch_to(12.3, -45.6);
        p.push(Command::Trim);
        p.jump_to(200.0, 300.0);
        p.push(Command::Stop);
        p.push(Command::ColorChange);
        p.stitch_to(201.0, 301.0);
        p.push(Command::End);
        p
    }

    #[test]
    fn moves_come_back_to_the_hundredth_of_a_millimetre() {
        let o = Options {
            needle: "M3".into(),
            ..Options::default()
        };
        let text = String::from_utf8(write(&sample(), &o)).expect("utf8");
        let parsed = parse(&text).expect("parse");
        let want: Vec<(f64, f64, bool)> = sample()
            .stitches
            .iter()
            .filter(|s| matches!(s.command, Command::Stitch | Command::Jump))
            .map(|s| (s.x / 10.0, -s.y / 10.0, s.command == Command::Stitch))
            .collect();
        assert_eq!(parsed.moves.len(), want.len());
        for (a, b) in parsed.moves.iter().zip(&want) {
            assert!(
                (a.0 - b.0).abs() <= 0.01 && (a.1 - b.1).abs() <= 0.01,
                "{a:?} {b:?}"
            );
            assert_eq!(a.2, b.2);
        }
        assert_eq!((parsed.color_changes, parsed.stops), (1, 1));
        assert_eq!(text.matches("\nM3\n").count(), 3);
        assert!(text.contains("M0 ; colour change 2: #0000ff   Navy"));
        assert!(text.contains("; trim\n") && text.ends_with("M2 ; end\n"));
    }

    #[test]
    fn a_trim_command_replaces_the_comment() {
        let o = Options {
            trim: "M5".into(),
            profile: "Test (bench); v1".into(),
            ..Options::default()
        };
        let text = String::from_utf8(write(&sample(), &o)).expect("utf8");
        assert!(text.contains("\nM5\n") && !text.contains("; trim\n"));
        assert!(text.contains("; machine: Test  bench   v1\n"));
    }
}
