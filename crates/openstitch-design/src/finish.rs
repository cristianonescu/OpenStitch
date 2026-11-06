//! Per-object finish (#548, beyond PE-DESIGN): when the thread is trimmed after an object,
//! how its runs tie in and lock, and where a text trims between its letters. An object
//! without a [`Finish`] (every object PE-DESIGN or an older OpenStitch wrote) is sewn by
//! PE-DESIGN 11.50's rules, so the defaults reproduce the compiler's output exactly.

use crate::geometry::Vec2;
use serde::{Deserialize, Serialize};

/// Trim after an object, or tie its runs: by PE-DESIGN's rule, always, or never.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinishMode {
    /// PE-DESIGN's rule: a trim when the next start is farther than the trim distance; ties
    /// on the runs a trim separates.
    #[default]
    Auto,
    /// Trim after the object however short the move; tie in at its start and lock at its end
    /// even when no trim separates it.
    Always,
    /// Never trim after the object (the frame jumps; a colour change still cuts); no tie-in
    /// or lock at all.
    Off,
}

impl FinishMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" | "whenFar" => Some(FinishMode::Auto),
            "always" => Some(FinishMode::Always),
            "off" => Some(FinishMode::Off),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FinishMode::Auto => "auto",
            FinishMode::Always => "always",
            FinishMode::Off => "off",
        }
    }

    fn is_auto(&self) -> bool {
        *self == FinishMode::Auto
    }
}

/// Where a text trims between its letters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LetterTrim {
    /// PE-DESIGN's rule: a move between letters longer than 2.3 mm is trimmed.
    #[default]
    Auto,
    /// No trims between letters: short moves are sewn across, longer ones jump.
    Never,
    /// A trim after every letter.
    Letter,
    /// A trim between words only (letters of a word are joined without trims).
    Word,
    /// A trim between lines only.
    Line,
}

impl LetterTrim {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(LetterTrim::Auto),
            "never" => Some(LetterTrim::Never),
            "letter" => Some(LetterTrim::Letter),
            "word" => Some(LetterTrim::Word),
            "line" => Some(LetterTrim::Line),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LetterTrim::Auto => "auto",
            LetterTrim::Never => "never",
            LetterTrim::Letter => "letter",
            LetterTrim::Word => "word",
            LetterTrim::Line => "line",
        }
    }

    fn is_auto(&self) -> bool {
        *self == LetterTrim::Auto
    }
}

/// What lies between a letter of a text and the letter sewn before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LetterBreak {
    /// The next letter of the same word.
    Letter,
    /// A space: a new word.
    Word,
    /// A line break: a new line.
    Line,
}

/// Shortest and longest tie stitch, mm.
pub const TIE_LENGTH_RANGE: (f64, f64) = (0.2, 2.0);
/// Most out-and-back tie steps.
pub const MAX_TIE_COUNT: u8 = 5;
/// Longest trim distance, mm.
pub const MAX_TRIM_DISTANCE: f64 = 50.0;

/// An object's finish. Every field's default is PE-DESIGN's rule.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finish {
    /// Trim after the object.
    #[serde(default, skip_serializing_if = "FinishMode::is_auto")]
    pub trim: FinishMode,
    /// With [`FinishMode::Auto`]: trim when the next start is farther than this, mm (`None`:
    /// the design's trim distance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim_distance: Option<f64>,
    /// Tie-in and lock of the object's runs.
    #[serde(default, skip_serializing_if = "FinishMode::is_auto")]
    pub tie: FinishMode,
    /// Out-and-back steps of a tie-in or lock (`None`: PE-DESIGN's own tie).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tie_count: Option<u8>,
    /// Length of each tie stitch, mm (`None`: PE-DESIGN's own tie).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tie_length: Option<f64>,
    /// Text only: where it trims between letters.
    #[serde(default, skip_serializing_if = "LetterTrim::is_auto")]
    pub between: LetterTrim,
}

impl Finish {
    /// Every setting is PE-DESIGN's: the object needs no finish stored.
    pub fn is_default(&self) -> bool {
        *self == Finish::default()
    }

    /// The writer ties this object's runs itself (its generators' PE-DESIGN ties are left out):
    /// ties are off, always, or a count or length is set.
    pub fn custom_ties(&self) -> bool {
        self.tie != FinishMode::Auto || self.tie_count.is_some() || self.tie_length.is_some()
    }

    /// The tie as (out-and-back steps, stitch length in 0.1 mm), PE-DESIGN's 0.3 mm single
    /// step filling in what is not set.
    pub fn tie_steps(&self) -> (usize, f64) {
        (
            self.tie_count.unwrap_or(1).max(1) as usize,
            self.tie_length.unwrap_or(0.3) * 10.0,
        )
    }
}

/// Tie-in of `count` out-and-back steps of `len` (0.1 mm) at the start of `run`, along its
/// first stitch: each step is two stitches exactly `len` long.
pub fn tie_in(run: &mut Vec<Vec2>, count: usize, len: f64) {
    if run.len() < 2 {
        return;
    }
    let (a, b) = (run[0], run[1]);
    let d = a.distance(b);
    if d <= 1e-9 {
        return;
    }
    let out = a + (b - a) * (len / d);
    let tie: Vec<Vec2> = (0..count).flat_map(|_| [out, a]).collect();
    run.splice(1..1, tie);
}

/// Lock of `count` out-and-back steps of `len` (0.1 mm) after a last stitch from `a` to `b`:
/// back along it and home again.
pub fn lock_points(a: Vec2, b: Vec2, count: usize, len: f64) -> Vec<Vec2> {
    let d = a.distance(b);
    if d <= 1e-9 {
        return Vec::new();
    }
    let back = b + (a - b) * (len / d);
    (0..count).flat_map(|_| [back, b]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::v2;

    #[test]
    fn ties_are_exactly_count_steps_of_the_length() {
        let mut run = vec![v2(0.0, 0.0), v2(40.0, 0.0), v2(80.0, 0.0)];
        tie_in(&mut run, 2, 6.0);
        assert_eq!(
            run,
            vec![
                v2(0.0, 0.0),
                v2(6.0, 0.0),
                v2(0.0, 0.0),
                v2(6.0, 0.0),
                v2(0.0, 0.0),
                v2(40.0, 0.0),
                v2(80.0, 0.0)
            ]
        );
        let lock = lock_points(v2(40.0, 0.0), v2(80.0, 0.0), 1, 6.0);
        assert_eq!(lock, vec![v2(74.0, 0.0), v2(80.0, 0.0)]);
    }

    #[test]
    fn modes_parse_and_print() {
        for m in [FinishMode::Auto, FinishMode::Always, FinishMode::Off] {
            assert_eq!(FinishMode::parse(m.as_str()), Some(m));
        }
        for t in [
            LetterTrim::Auto,
            LetterTrim::Never,
            LetterTrim::Letter,
            LetterTrim::Word,
            LetterTrim::Line,
        ] {
            assert_eq!(LetterTrim::parse(t.as_str()), Some(t));
        }
        assert!(Finish::default().is_default());
        assert!(!Finish::default().custom_ties());
    }
}
