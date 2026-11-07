//! Per-object finish (#548): trim after, ties, and trims between letters.

use crate::Compiler;
use crate::finish::{Finish, FinishMode, LetterTrim};
use crate::geometry::{Contour, v2};
use crate::model::*;
use crate::text::TextSpec;
use openstitch_core::{Command, Stitch, Thread};
use std::ops::Range;

const RED: Thread = Thread::rgb(0xED171F);

/// Two red fill squares, `gap` (0.1 mm) apart, the first with `finish`.
fn squares(gap: f64, finish: Option<Finish>) -> Design {
    let mut d = Design::new("finish", DesignPage::default());
    for k in 0..2 {
        let mut o = DesignObject::shape(
            vec![Contour::rectangle(
                k as f64 * (100.0 + gap),
                0.0,
                100.0,
                100.0,
            )],
            Some(RegionSewing::new(RegionType::Fill, RED)),
            None,
        );
        if k == 0 {
            o.finish = finish.clone();
        }
        d.add(o);
    }
    d
}

fn sewn(d: &Design) -> (Vec<Stitch>, Vec<Range<usize>>) {
    let (p, spans) = Compiler::for_design(d).compile_traced(d);
    (p.stitches, spans.into_iter().map(|s| s.1).collect())
}

fn trims(st: &[Stitch], r: Range<usize>) -> usize {
    st[r].iter().filter(|s| s.command == Command::Trim).count()
}

#[test]
fn default_finish_sews_as_no_finish() {
    for gap in [5.0, 300.0] {
        assert_eq!(
            sewn(&squares(gap, None)).0,
            sewn(&squares(gap, Some(Finish::default()))).0
        );
    }
}

#[test]
fn trim_after_off_jumps_and_always_trims_a_short_move() {
    let (st, spans) = sewn(&squares(300.0, None));
    assert_eq!(
        trims(&st, spans[1].clone()),
        1,
        "PE-DESIGN trims a 30 mm move"
    );
    let off = Finish {
        trim: FinishMode::Off,
        ..Finish::default()
    };
    let (st, spans) = sewn(&squares(300.0, Some(off)));
    assert_eq!(trims(&st, spans[1].clone()), 0);
    assert!(
        st[spans[1].clone()]
            .iter()
            .any(|s| s.command == Command::Jump)
    );
    let always = Finish {
        trim: FinishMode::Always,
        ..Finish::default()
    };
    // Squares 0.5 mm apart: the short move is trimmed all the same.
    let (st, spans) = sewn(&squares(5.0, Some(always)));
    assert_eq!(trims(&st, spans[1].clone()), 1);
    // A trim distance: trimmed only when the next start is farther.
    let far = |d: f64| Finish {
        trim_distance: Some(d),
        ..Finish::default()
    };
    let (st, spans) = sewn(&squares(300.0, Some(far(40.0))));
    assert_eq!(
        trims(&st, spans[1].clone()),
        0,
        "30 mm is not farther than 40"
    );
    let (st, spans) = sewn(&squares(300.0, Some(far(3.0))));
    assert_eq!(trims(&st, spans[1].clone()), 1);
}

#[test]
fn tie_count_and_length_sew_exactly_that_tie() {
    let tie = Finish {
        tie_count: Some(2),
        tie_length: Some(0.6),
        ..Finish::default()
    };
    let (st, spans) = sewn(&squares(300.0, Some(tie)));
    // From the jump's landing (the run's first needle point).
    let needles: Vec<_> = st[spans[0].clone()]
        .iter()
        .skip_while(|s| s.command != Command::Jump)
        .filter(|s| matches!(s.command, Command::Stitch | Command::Jump))
        .map(|s| v2(s.x, s.y))
        .collect();
    // Start, out, back, out, back: four stitches of 0.6 mm.
    for w in needles[..5].windows(2) {
        assert!((w[0].distance(w[1]) - 6.0).abs() <= 0.5, "{w:?}");
    }
    assert!(needles[0].distance(needles[2]) < 1e-6);
    assert!(needles[0].distance(needles[4]) < 1e-6);
    // The lock before the trim (written with the move into the next object): back 0.6 mm
    // and home, twice.
    let end = spans[1].start
        + st[spans[1].clone()]
            .iter()
            .position(|s| s.command == Command::Trim)
            .expect("trimmed");
    let tail: Vec<_> = st[..end]
        .iter()
        .rev()
        .skip_while(|s| s.command != Command::Stitch)
        .take(5)
        .map(|s| v2(s.x, s.y))
        .collect();
    for w in tail[..5].windows(2) {
        assert!((w[0].distance(w[1]) - 6.0).abs() <= 0.5, "{w:?}");
    }
    // Ties off: no tie at all.
    let off = Finish {
        tie: FinishMode::Off,
        ..Finish::default()
    };
    let (st_off, spans_off) = sewn(&squares(300.0, Some(off)));
    let first: Vec<_> = st_off[spans_off[0].clone()]
        .iter()
        .filter(|s| s.command == Command::Stitch)
        .take(3)
        .map(|s| v2(s.x, s.y))
        .collect();
    assert!(
        first[0].distance(first[2]) > 1.0,
        "no step back to the start"
    );
}

/// "ab cd\nef" in Lato Bold, 10 mm, with letters trimmed `between`.
fn text(between: LetterTrim) -> (Vec<Stitch>, Range<usize>) {
    let mut d = Design::new("letters", DesignPage::default());
    let (mut o, _) = DesignObject::text(
        TextSpec::new("ab cd\nef", "lato-bold", 10.0),
        Some(RegionSewing::new(RegionType::Satin, RED)),
        None,
    )
    .expect("bundled font");
    o.finish = Some(Finish {
        between,
        ..Finish::default()
    });
    d.add(o);
    let (st, spans) = sewn(&d);
    (st, spans[0].clone())
}

#[test]
fn text_trims_between_letters_words_or_lines() {
    let count = |b| {
        let (st, r) = text(b);
        trims(&st, r)
    };
    // Trims inside letters (a letter's separate pieces) are the same whatever the setting.
    let inside = count(LetterTrim::Never);
    assert_eq!(
        count(LetterTrim::Letter) - inside,
        5,
        "after each letter but the last"
    );
    assert_eq!(count(LetterTrim::Word) - inside, 2, "before c and before e");
    assert_eq!(count(LetterTrim::Line) - inside, 1, "before e");
    assert!(
        count(LetterTrim::Auto) > inside,
        "PE-DESIGN trims between these letters"
    );
    let (st, r) = text(LetterTrim::Never);
    assert!(st[r].iter().any(|s| s.command == Command::Jump));
}

/// The longest run of stitches of 0.35 mm or less (a stitch in place on the 0.1 mm grid).
fn longest_in_place(st: &[Stitch]) -> usize {
    let (mut run, mut most) = (0, 0);
    for w in st.windows(2) {
        let short = w[1].command == Command::Stitch
            && matches!(w[0].command, Command::Stitch | Command::Jump)
            && v2(w[0].x, w[0].y).distance(v2(w[1].x, w[1].y)) <= 3.5;
        run = if short { run + 1 } else { 0 };
        most = most.max(run);
    }
    most
}

#[test]
fn a_pointed_start_ties_in_along_the_tip() {
    // #732: g11's Auto Punch photo piece "Auto Punch 41 White", a 2.5 × 0.8 mm fill whose rows
    // start at a 0.3 mm tip, 0.22 mm apart. Its own one-step tie-in, out 0.3 mm and back,
    // plus the tip's rows sewed 7 stitches in place.
    let points = [
        (467.45, 784.51),
        (469.40, 787.76),
        (470.05, 785.81),
        (468.75, 784.51),
        (472.01, 783.85),
        (472.66, 787.11),
        (471.35, 788.41),
        (474.61, 787.76),
        (477.86, 789.71),
        (472.01, 790.36),
        (470.70, 791.67),
        (466.80, 790.36),
        (455.08, 791.67),
        (453.13, 789.71),
        (458.98, 787.76),
        (462.89, 787.76),
        (464.19, 789.06),
        (466.15, 787.11),
        (466.15, 784.51),
    ];
    let piece = |finish: Finish| {
        let mut d = Design::new("tip", DesignPage::default());
        let mut region = RegionSewing::new(RegionType::Fill, RED);
        region.angle = 90.0;
        region.underlay = false;
        let mut o = DesignObject::shape(
            vec![Contour::new(
                points.iter().map(|&(x, y)| v2(x, y)).collect(),
                true,
            )],
            Some(region),
            None,
        );
        o.finish = Some(finish);
        d.add(o);
        sewn(&d).0
    };
    let tied = piece(Finish {
        tie_count: Some(1),
        ..Finish::default()
    });
    assert!(longest_in_place(&tied) <= 6, "{}", longest_in_place(&tied));
    // Every needle point of the piece is still sewn: only the tie-in's step moved.
    let untied = piece(Finish {
        tie: FinishMode::Off,
        ..Finish::default()
    });
    let needles = |st: &[Stitch]| -> Vec<_> {
        st.iter()
            .filter(|s| s.command == Command::Stitch)
            .map(|s| v2(s.x, s.y))
            .collect()
    };
    let sewn_tied = needles(&tied);
    for p in needles(&untied) {
        assert!(
            sewn_tied.iter().any(|q| q.distance(p) < 0.5),
            "{p:?} is not sewn"
        );
    }
}
