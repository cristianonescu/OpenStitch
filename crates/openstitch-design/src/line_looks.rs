//! Hand-made line looks (beyond PE-DESIGN, off unless chosen): back stitch and a running stitch
//! whose passes follow a thickness profile (#542), a hand-stitch look on running lines and on
//! one side of a satin, and a scribble line (#540). Every random choice comes from a seed, so
//! the same settings always sew the same stitches. Lengths are in 0.1 mm.

use crate::fmath::Det;
use crate::geometry::Vec2;
use crate::paths::{Rng, Walker, normal};
use serde::{Deserialize, Serialize};

/// The shortest and longest stitch a hand-stitch look sews (0.1 mm).
pub const HAND_MIN_STITCH: f64 = 5.0;
pub const HAND_MAX_STITCH: f64 = 70.0;

/// A named shaping curve on 0..=1, shared by the variable running stitch and, later, satin
/// widths and density gradients (B6, B10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfilePreset {
    /// 1 everywhere.
    #[default]
    Constant,
    /// 0 at the start, 1 at the end.
    Linear,
    /// A half circle: 0 at both ends, 1 in the middle.
    Arc,
    /// Half a cosine wave: 0 at both ends, 1 in the middle, softer than the arc.
    Sine,
    /// 0 at the start, 1 at the end, flat at both ends.
    Smoothstep,
    /// Linear in `steps` flat steps.
    Stairs,
}

impl ProfilePreset {
    pub const ALL: [ProfilePreset; 6] = [
        ProfilePreset::Constant,
        ProfilePreset::Linear,
        ProfilePreset::Arc,
        ProfilePreset::Sine,
        ProfilePreset::Smoothstep,
        ProfilePreset::Stairs,
    ];

    pub fn id(self) -> &'static str {
        match self {
            ProfilePreset::Constant => "constant",
            ProfilePreset::Linear => "linear",
            ProfilePreset::Arc => "arc",
            ProfilePreset::Sine => "sine",
            ProfilePreset::Smoothstep => "smoothstep",
            ProfilePreset::Stairs => "stairs",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }
}

fn four() -> u32 {
    4
}

fn is_four(v: &u32) -> bool {
    *v == 4
}

/// A profile: a preset, or custom points `(t, value)` joined by straight lines.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    #[serde(default)]
    pub preset: ProfilePreset,
    /// Stairs: number of steps (2-10).
    #[serde(default = "four", skip_serializing_if = "is_four")]
    pub steps: u32,
    /// Custom points, sorted by `t`; when given they replace the preset.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<[f64; 2]>,
}

impl Profile {
    pub fn preset(preset: ProfilePreset) -> Self {
        Profile {
            preset,
            steps: 4,
            points: Vec::new(),
        }
    }

    /// The profile's value at `t` (clamped to 0..=1), in 0..=1.
    pub fn at(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        if !self.points.is_empty() {
            let p = &self.points;
            if t <= p[0][0] {
                return p[0][1].clamp(0.0, 1.0);
            }
            for w in p.windows(2) {
                if t <= w[1][0] {
                    let span = (w[1][0] - w[0][0]).max(1e-12);
                    let u = (t - w[0][0]) / span;
                    return (w[0][1] + (w[1][1] - w[0][1]) * u).clamp(0.0, 1.0);
                }
            }
            return p[p.len() - 1][1].clamp(0.0, 1.0);
        }
        match self.preset {
            ProfilePreset::Constant => 1.0,
            ProfilePreset::Linear => t,
            ProfilePreset::Arc => (1.0 - (2.0 * t - 1.0) * (2.0 * t - 1.0)).max(0.0).sqrt(),
            ProfilePreset::Sine => 0.5 - 0.5 * (2.0 * std::f64::consts::PI * t).dcos(),
            ProfilePreset::Smoothstep => t * t * (3.0 - 2.0 * t),
            ProfilePreset::Stairs => {
                let n = self.steps.clamp(2, 10) as f64;
                ((t * n).floor().min(n - 1.0) / (n - 1.0)).clamp(0.0, 1.0)
            }
        }
    }
}

fn one() -> u32 {
    1
}

fn three() -> u32 {
    3
}

fn quarter() -> f64 {
    0.25
}

/// A running stitch sewn back and forth a varying number of times along the line (#542):
/// `min_passes` where the profile is 0, `max_passes` where it is 1, both odd so the line ends at
/// its far end; the passes sit `pass_spacing` mm apart side by side, so the line swells.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProfile {
    #[serde(default)]
    pub profile: Profile,
    #[serde(default = "one")]
    pub min_passes: u32,
    #[serde(default = "three")]
    pub max_passes: u32,
    /// mm between neighbouring passes.
    #[serde(default = "quarter")]
    pub pass_spacing: f64,
}

impl Default for RunProfile {
    fn default() -> Self {
        RunProfile {
            profile: Profile::preset(ProfilePreset::Arc),
            min_passes: 1,
            max_passes: 3,
            pass_spacing: 0.25,
        }
    }
}

impl RunProfile {
    /// The number of passes where the profile is `v`: odd, from `min_passes` to `max_passes`.
    pub fn passes(&self, v: f64) -> u32 {
        let lo = self.min_passes.max(1) | 1;
        let hi = self.max_passes.max(lo) | 1;
        let steps = ((hi - lo) / 2) as f64;
        lo + 2 * (v.clamp(0.0, 1.0) * steps).round() as u32
    }
}

/// The hand-stitch look on a running line (#540): stitch lengths and tilts vary smoothly, the
/// line is sewn `passes` times back and forth, and now and then a stitch is sewn again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandStitch {
    /// Stitch lengths vary up to this much either way, % of the pitch (0-50).
    #[serde(default)]
    pub length_jitter: f64,
    /// Needle points tilt off the line up to this angle, degrees (0-20).
    #[serde(default)]
    pub angle_jitter: f64,
    /// Passes along the line (1-3).
    #[serde(default = "one")]
    pub passes: u32,
    /// Chance that a stitch is sewn again (an extra back-and-forth), % (0-50).
    #[serde(default)]
    pub extra_passes: f64,
    #[serde(default)]
    pub seed: u32,
}

impl Default for HandStitch {
    fn default() -> Self {
        HandStitch {
            length_jitter: 25.0,
            angle_jitter: 6.0,
            passes: 1,
            extra_passes: 10.0,
            seed: 1,
        }
    }
}

/// Which side of a satin the hand-stitch look moves (#540).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SatinSide {
    /// The side the satin's first needle point is on.
    #[default]
    A,
    B,
    Both,
}

/// The hand-stitch look on a satin (#540): needle points on the chosen side move in towards
/// the other side by up to `amount` % of the crossing, smoothly along the column, so the edge
/// wanders like a hand-sewn one while the other edge stays crisp.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SatinHand {
    #[serde(default)]
    pub side: SatinSide,
    /// % of each crossing (0-50).
    #[serde(default)]
    pub amount: f64,
    #[serde(default)]
    pub seed: u32,
}

/// Smooth seeded noise in -1..=1 along one axis (value noise: random values at whole numbers,
/// joined by a smoothstep), the same for the same seed on every platform.
#[derive(Clone, Copy, Debug)]
pub struct Noise(u64);

impl Noise {
    pub fn new(seed: u64) -> Self {
        Noise(seed)
    }

    fn lattice(&self, i: i64) -> f64 {
        let mut r = Rng::new(self.0 ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        r.next_f64() * 2.0 - 1.0
    }

    pub fn at(&self, x: f64) -> f64 {
        let i = x.floor();
        let f = x - i;
        let u = f * f * (3.0 - 2.0 * f);
        let (a, b) = (self.lattice(i as i64), self.lattice(i as i64 + 1));
        a + (b - a) * u
    }
}

/// Points `step` apart along `path` (the last one at its end).
fn along(path: &[Vec2], step: f64) -> Vec<(f64, Vec2)> {
    let w = Walker::new(path);
    let len = w.length();
    if len <= 1e-9 || path.len() < 2 {
        return path.first().map(|&p| vec![(0.0, p)]).unwrap_or_default();
    }
    let n = (len / step.max(1.0)).ceil().max(1.0) as usize;
    (0..=n)
        .map(|k| {
            let s = len * k as f64 / n as f64;
            (s, w.at(s))
        })
        .collect()
}

/// Back stitch (#542): two stitches forward, one back, along `path`. Points `pitch / 2`
/// apart; each forward stitch spans two of them, each back stitch one, so every stitch but the
/// last is followed by a back stitch of half its length. Ends at the path's far end.
pub fn back_stitch(path: &[Vec2], pitch: f64) -> Vec<Vec2> {
    let q: Vec<Vec2> = along(path, pitch / 2.0).into_iter().map(|p| p.1).collect();
    let m = q.len().saturating_sub(1);
    if m < 2 {
        return q;
    }
    let mut out = vec![q[0]];
    for i in 1..m {
        out.push(q[i + 1]);
        if i + 1 < m {
            out.push(q[i]);
        }
    }
    out
}

/// The variable running stitch (#542) along `path`: each `pitch` stitch is sewn forward and
/// back `profile.passes(value)` times (odd), the passes side by side `pass_spacing` apart.
pub fn variable_running(path: &[Vec2], pitch: f64, profile: &RunProfile) -> Vec<Vec2> {
    let w = Walker::new(path);
    let len = w.length();
    let pts = along(path, pitch);
    if pts.len() < 2 {
        return pts.into_iter().map(|p| p.1).collect();
    }
    let gap = profile.pass_spacing.clamp(0.0, 2.0) * 10.0;
    let mut out = vec![pts[0].1];
    for seg in pts.windows(2) {
        let ((s0, a), (s1, b)) = (seg[0], seg[1]);
        let mid = (s0 + s1) / 2.0;
        let n = profile.passes(profile.profile.at(mid / len.max(1e-9)));
        let side = normal(w.tangent(mid));
        let lane = |j: u32| side * (gap * (j as f64 - (n - 1) as f64 / 2.0));
        for j in 0..n {
            let off = lane(j);
            let (from, to) = if j.is_multiple_of(2) { (a, b) } else { (b, a) };
            if j > 0 || n > 1 {
                out.push(from + off);
            }
            out.push(to + off);
        }
        // The next stitch starts on the line.
        if n > 1 {
            out.push(b);
        }
    }
    out.dedup_by(|x, y| x.distance(*y) < 1e-9);
    out
}

/// The hand-stitch look along `path` (#540): needle points at stitch lengths varying smoothly
/// within ±`length_jitter` % of `pitch` (and within 0.5-7 mm), each moved off the line by a
/// smooth tilt of up to `angle_jitter` degrees, sewn `passes` times back and forth (each pass
/// tilted anew), with a stitch now and then sewn again forward and back.
pub fn hand_running(path: &[Vec2], pitch: f64, hand: &HandStitch) -> Vec<Vec2> {
    let w = Walker::new(path);
    let len = w.length();
    if len <= 1e-9 {
        return path.to_vec();
    }
    let seed = u64::from(hand.seed);
    let lengths = Noise::new(seed.wrapping_mul(31).wrapping_add(7));
    let jitter = hand.length_jitter.clamp(0.0, 50.0) / 100.0;
    // Arc lengths of the needle points.
    let mut at = vec![0.0];
    let mut s = 0.0;
    while s < len {
        let step = (pitch * (1.0 + jitter * lengths.at(s / (3.0 * pitch))))
            .clamp(HAND_MIN_STITCH, HAND_MAX_STITCH);
        s += step;
        if len - s < HAND_MIN_STITCH {
            break;
        }
        at.push(s);
    }
    at.push(len);
    // The last stitch took the short remainder: split it when that made it too long.
    let n = at.len();
    if n >= 2 && at[n - 1] - at[n - 2] > HAND_MAX_STITCH {
        at.insert(n - 1, (at[n - 1] + at[n - 2]) / 2.0);
    }
    let tilt = hand.angle_jitter.clamp(0.0, 20.0).to_radians().dtan();
    let passes = hand.passes.clamp(1, 3);
    let mut extra = Rng::new(seed.wrapping_mul(131).wrapping_add(3));
    let chance = hand.extra_passes.clamp(0.0, 50.0) / 100.0;
    let mut out = Vec::new();
    for pass in 0..passes {
        let side = Noise::new(seed.wrapping_add(1000 + u64::from(pass)));
        let point = |k: usize| {
            let s = at[k];
            // Ends stay on the line, so passes and neighbouring lines join cleanly.
            if k == 0 || k + 1 == at.len() {
                return w.at(s);
            }
            let local = (at[k + 1] - at[k - 1]) / 2.0;
            w.at(s) + normal(w.tangent(s)) * (side.at(s / (2.0 * pitch)) * tilt * local / 2.0)
        };
        let order: Vec<usize> = if pass.is_multiple_of(2) {
            (0..at.len()).collect()
        } else {
            (0..at.len()).rev().collect()
        };
        let pts: Vec<Vec2> = order.iter().map(|&k| point(k)).collect();
        for (i, &p) in pts.iter().enumerate() {
            if !(pass > 0 && i == 0) {
                out.push(p);
            }
            if i > 0 && i + 1 < pts.len() && extra.next_f64() < chance {
                out.push(pts[i - 1]);
                out.push(p);
            }
        }
    }
    out
}

/// A scribble along `path` (#540): a zigzag `width` wide, its points `spacing` apart, sewn
/// `repetitions` times back and forth as one run, each pass's side offsets moved by smooth
/// noise (`noise_scale` long waves), never past the band's edges.
pub fn scribble(
    path: &[Vec2],
    width: f64,
    spacing: f64,
    repetitions: u32,
    noise_scale: f64,
    seed: u32,
) -> Vec<Vec2> {
    let w = Walker::new(path);
    let pts = along(path, spacing.max(2.0));
    if pts.len() < 2 {
        return pts.into_iter().map(|p| p.1).collect();
    }
    let half = width / 2.0;
    let scale = noise_scale.max(1.0);
    let mut out = Vec::new();
    for pass in 0..repetitions.clamp(1, 6) {
        let reach = Noise::new(u64::from(seed) * 7 + u64::from(pass) * 2 + 1);
        let drift = Noise::new(u64::from(seed) * 7 + u64::from(pass) * 2 + 2);
        let order: Vec<usize> = if pass.is_multiple_of(2) {
            (0..pts.len()).collect()
        } else {
            (0..pts.len()).rev().collect()
        };
        for (i, &k) in order.iter().enumerate() {
            if pass > 0 && i == 0 {
                continue;
            }
            let (s, p) = pts[k];
            let sign = if (k + pass as usize).is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
            let x = s / scale;
            let off = (sign * (0.65 + 0.35 * reach.at(x)) * half + 0.3 * half * drift.at(x + 17.0))
                .clamp(-half, half);
            out.push(p + normal(w.tangent(s)) * off);
        }
    }
    out
}

/// The hand-stitch look on one side of a satin `run` (#540): each needle point on the chosen
/// side moves towards its partner across the crossing (the next point for side A's even
/// points, the one before for side B's odd ones) by up to `amount` % of the way, smoothly
/// along the run. Points stay on their crossing, so they stay inside the column. The first
/// and last `keep` points (tie-in and lock) are left alone.
pub fn satin_hand(run: &mut [Vec2], hand: &SatinHand, keep: usize) {
    let n = run.len();
    if n < 2 * keep + 2 {
        return;
    }
    let amount = hand.amount.clamp(0.0, 50.0) / 100.0;
    let noise = Noise::new(u64::from(hand.seed).wrapping_mul(97).wrapping_add(5));
    let original = run.to_vec();
    for k in keep..n - keep {
        let a_side = k.is_multiple_of(2);
        let moves = match hand.side {
            SatinSide::A => a_side,
            SatinSide::B => !a_side,
            SatinSide::Both => true,
        };
        if !moves {
            continue;
        }
        let partner = if a_side { k + 1 } else { k - 1 };
        let u = 0.5 + 0.5 * noise.at(k as f64 / 9.0);
        run[k] = original[k].lerp(original[partner], amount * u);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::v2;

    fn straight(len: f64) -> Vec<Vec2> {
        vec![v2(0.0, 0.0), v2(len, 0.0)]
    }

    #[test]
    fn presets_have_their_documented_values() {
        let at = |p: ProfilePreset| {
            let pr = Profile::preset(p);
            [pr.at(0.0), pr.at(0.5), pr.at(1.0)]
        };
        assert_eq!(at(ProfilePreset::Constant), [1.0, 1.0, 1.0]);
        assert_eq!(at(ProfilePreset::Linear), [0.0, 0.5, 1.0]);
        assert_eq!(at(ProfilePreset::Arc), [0.0, 1.0, 0.0]);
        let s = at(ProfilePreset::Sine);
        assert!(s[0].abs() < 1e-12 && (s[1] - 1.0).abs() < 1e-12 && s[2].abs() < 1e-12);
        assert_eq!(at(ProfilePreset::Smoothstep), [0.0, 0.5, 1.0]);
        let st = at(ProfilePreset::Stairs);
        assert_eq!(st[0], 0.0);
        assert!((st[1] - 2.0 / 3.0).abs() < 1e-12 && st[2] == 1.0);
        let custom = Profile {
            points: vec![[0.0, 0.2], [0.5, 1.0], [1.0, 0.0]],
            ..Profile::default()
        };
        let got = [custom.at(0.0), custom.at(0.25), custom.at(1.0)];
        for (g, want) in got.iter().zip([0.2, 0.6, 0.0]) {
            assert!((g - want).abs() < 1e-12, "{got:?}");
        }
    }

    #[test]
    fn back_stitch_goes_two_forward_one_back_and_ends_at_the_end() {
        let run = back_stitch(&straight(200.0), 20.0);
        assert_eq!(run.first(), Some(&v2(0.0, 0.0)));
        assert_eq!(run.last(), Some(&v2(200.0, 0.0)));
        for w in run.windows(3).step_by(2) {
            let fwd = w[1].x - w[0].x;
            let back = w[2].x - w[1].x;
            if w[2] == v2(200.0, 0.0) {
                continue;
            }
            assert!((fwd - 20.0).abs() < 1e-9, "{w:?}");
            assert!((back + 10.0).abs() < 1e-9, "{w:?}");
        }
    }

    #[test]
    fn variable_running_follows_the_arc_profile() {
        let prof = RunProfile {
            profile: Profile::preset(ProfilePreset::Arc),
            min_passes: 1,
            max_passes: 5,
            pass_spacing: 0.3,
        };
        assert_eq!(prof.passes(0.0), 1);
        assert_eq!(prof.passes(0.5), 3);
        assert_eq!(prof.passes(1.0), 5);
        assert!((1..=7).all(|_| prof.passes(2.0) <= 5));
        let run = variable_running(&straight(400.0), 20.0, &prof);
        assert_eq!(run.first(), Some(&v2(0.0, 0.0)));
        assert_eq!(run.last(), Some(&v2(400.0, 0.0)));
        // Thicker in the middle: more points over the middle stitch than the first.
        let near = |x0: f64, x1: f64| run.iter().filter(|p| p.x > x0 && p.x < x1).count();
        assert!(near(190.0, 210.0) > near(0.5, 20.0));
        // No stitch under 0.3 mm except the pass offsets (0.3 mm here).
        for w in run.windows(2) {
            let d = w[0].distance(w[1]);
            assert!(d >= 2.9 || (d - 3.0).abs() < 1e-6 || d < 1e-9, "{d}");
        }
    }

    #[test]
    fn hand_running_is_seeded_and_bounded() {
        let path = straight(600.0);
        let hand = HandStitch {
            length_jitter: 30.0,
            angle_jitter: 0.0,
            passes: 1,
            extra_passes: 0.0,
            seed: 4,
        };
        let a = hand_running(&path, 25.0, &hand);
        assert_eq!(a, hand_running(&path, 25.0, &hand));
        let b = hand_running(
            &path,
            25.0,
            &HandStitch {
                seed: 5,
                ..hand.clone()
            },
        );
        assert_ne!(a, b, "another seed sews other stitches");
        for w in a.windows(2) {
            let d = w[0].distance(w[1]);
            assert!(
                (HAND_MIN_STITCH - 1e-9..=HAND_MAX_STITCH + 1e-9).contains(&d),
                "{d}"
            );
            if w[1].x < 590.0 {
                assert!((d - 25.0).abs() <= 25.0 * 0.3 + 1e-6, "{d}");
            }
        }
        assert_eq!(a.last(), Some(&v2(600.0, 0.0)));
        // Tilts move the points off the line; three passes end at the far end.
        let tilted = hand_running(
            &path,
            25.0,
            &HandStitch {
                angle_jitter: 15.0,
                passes: 3,
                ..hand.clone()
            },
        );
        assert!(tilted.iter().any(|p| p.y.abs() > 0.1));
        assert_eq!(tilted.last(), Some(&v2(600.0, 0.0)));
    }

    #[test]
    fn scribble_stays_in_its_band_as_one_run() {
        let path = straight(400.0);
        let run = scribble(&path, 40.0, 6.0, 3, 50.0, 9);
        assert_eq!(run, scribble(&path, 40.0, 6.0, 3, 50.0, 9));
        assert_ne!(run, scribble(&path, 40.0, 6.0, 3, 50.0, 10));
        assert!(run.iter().all(|p| p.y.abs() <= 20.0 + 1e-6));
        // Both sides reached, three passes along the 40 mm line.
        assert!(run.iter().any(|p| p.y > 10.0) && run.iter().any(|p| p.y < -10.0));
        assert!(run.len() > 3 * 60);
        // One continuous run: no move longer than a zigzag stitch across the band.
        assert!(run.windows(2).all(|w| w[0].distance(w[1]) < 45.0));
        assert_eq!(run.last().map(|p| p.x.round()), Some(400.0));
    }

    #[test]
    fn satin_hand_moves_one_side_only_inside_its_crossings() {
        let mut run: Vec<Vec2> = (0..40usize)
            .map(|k| v2(k as f64 * 4.0, if k.is_multiple_of(2) { 0.0 } else { 40.0 }))
            .collect();
        let before = run.clone();
        let hand = SatinHand {
            side: SatinSide::B,
            amount: 40.0,
            seed: 2,
        };
        satin_hand(&mut run, &hand, 4);
        for k in 0..run.len() {
            if k.is_multiple_of(2) {
                assert_eq!(run[k], before[k], "side A unchanged");
            } else {
                assert!(run[k].y <= 40.0 && run[k].y >= 40.0 - 16.0 - 1e-9);
            }
        }
        assert!(run.iter().zip(&before).any(|(a, b)| a != b));
    }
}
