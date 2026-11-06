//! Arched text as concentric rings (#456).

use crate::geometry::v2;
use crate::text::{self, TextAlign, TextError, TextSpec, arc_point, arc_radius_needed};

fn spec(text: &str, arc: f64) -> TextSpec {
    let mut s = TextSpec::new(text, "lato-bold", 8.0);
    s.align = TextAlign::Center;
    s.arc = Some(arc);
    s
}

#[test]
fn rings_are_one_line_spacing_apart() {
    // Arching up: the centre 40 mm below the first baseline, each further ring 9 mm inward.
    let (r, step) = (400.0, 90.0);
    for k in 0..3 {
        let down = k as f64 * step;
        for along in [-200.0, 0.0, 150.0] {
            let (q, _) = arc_point(r, down, along);
            let radius = q.distance(v2(0.0, r));
            assert!((radius - (r - down)).abs() < 0.01, "{k} {along}: {radius}");
        }
        // The middle of every line stays below the one before.
        assert!((arc_point(r, down, 0.0).0.y - down).abs() < 1e-9);
    }
    // Arching down: the centre above, each further ring outward.
    for k in 0..3 {
        let down = k as f64 * step;
        let (q, _) = arc_point(-r, down, 120.0);
        assert!((q.distance(v2(0.0, -r)) - (r + down)).abs() < 0.01);
    }
}

#[test]
fn letters_turn_with_the_ring_and_read_upright_both_ways() {
    for (arc, sign) in [(40.0, 1.0), (-40.0, -1.0)] {
        let l = text::layout(&spec("NAME\nYEAR\nMOTTO", arc), None).expect("layout");
        let turns: Vec<f64> = l.outlined().filter_map(text::Glyph::turn).collect();
        assert_eq!(turns.len(), 13);
        // First line N A M E: the outer letters turned most, opposite ways; arching down
        // turns them the other way round, so both read left to right and upright.
        let (n, e) = (turns[0], turns[3]);
        assert!(n * sign < -5.0 && e * sign > 5.0, "{arc}: {n} {e}");
        assert!(turns[1].abs() < n.abs());
    }
}

#[test]
fn too_tight_a_ring_names_the_radius_needed() {
    let long = "A LONG LINE OF LETTERS ON A RING";
    let err = text::layout(&spec(long, 10.0), None).expect_err("too tight");
    let TextError::ArcTooTight(need) = err else {
        panic!("{err:?}")
    };
    assert!(need > 10.0, "{need}");
    assert!(err.to_string().contains("at least"));
    // At that radius it fits.
    assert!(text::layout(&spec(long, need), None).is_ok());
    assert!(text::layout(&spec(long, -need), None).is_ok());
    // A third line arching up needs a larger first ring; arching down it does not.
    let lens = [100.0, 100.0, 100.0];
    let up = arc_radius_needed(&lens, 90.0, 80.0, true);
    let down = arc_radius_needed(&lens, 90.0, 80.0, false);
    assert!((up - (80.0 + 180.0)).abs() < 1e-9, "{up}");
    assert!((down - 80.0).abs() < 1e-9, "{down}");
}

#[test]
fn no_arc_lays_out_as_before() {
    let mut s = spec("AB\nCD", 40.0);
    s.arc = None;
    let straight = text::layout(&s, None).expect("layout");
    // An arc below the smallest radius (a file edited by hand) lays out straight too.
    let plain = text::layout(
        &TextSpec {
            arc: Some(0.0),
            ..s
        },
        None,
    )
    .expect("layout");
    assert_eq!(straight, plain);
    assert!(
        straight
            .outlined()
            .all(|g| g.turn().is_some_and(|t| t.abs() < 1e-9))
    );
}
