use openstitch_design::{Rect, Vec2, v2};

/// Screen-space rectangle in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ScreenRect {
    pub fn center(&self) -> Vec2 {
        v2(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// A canvas under this many points on a side shows nothing a view could be made for.
    pub const MIN_USABLE: f64 = 32.0;

    /// Whether a view can be fitted to, or kept in, this canvas (#1266): a window squeezed to
    /// nothing or minimised gives a canvas of a few points or none, and a view made for that
    /// (a page 20 points wide) is of no use once the window is large again.
    pub fn usable(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
            && self.width >= Self::MIN_USABLE
            && self.height >= Self::MIN_USABLE
    }
}

/// Maps model units (0.1 mm, +y down) to screen points: `screen = offset + model × scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Screen points per model unit.
    pub scale: f64,
    /// Screen position of the model origin.
    pub offset: Vec2,
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport {
            scale: 0.5,
            offset: v2(0.0, 0.0),
        }
    }
}

impl Viewport {
    /// Zoom limits: 0.02 pt per 0.1 mm (a 2 m page in ~400 pt) to 40 pt per 0.1 mm.
    pub const MIN_SCALE: f64 = 0.02;
    pub const MAX_SCALE: f64 = 40.0;

    pub fn to_screen(&self, p: Vec2) -> Vec2 {
        self.offset + p * self.scale
    }

    pub fn to_model(&self, s: Vec2) -> Vec2 {
        (s - self.offset) * (1.0 / self.scale)
    }

    /// Model units per screen point; pointer tolerances are defined in points.
    pub fn units_per_point(&self) -> f64 {
        1.0 / self.scale
    }

    /// Zooms by `factor` keeping the model point under `anchor` (screen) in place.
    pub fn zoom_about(&mut self, anchor: Vec2, factor: f64) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let model = self.to_model(anchor);
        self.scale = (self.scale * factor).clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        self.offset = anchor - model * self.scale;
    }

    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.offset = self.offset + v2(dx, dy);
    }

    /// Fits `content` (model) centred into `screen`, leaving `margin` points on every side.
    pub fn fit(content: Rect, screen: ScreenRect, margin: f64) -> Viewport {
        let w = (screen.width - 2.0 * margin).max(1.0);
        let h = (screen.height - 2.0 * margin).max(1.0);
        let scale = (w / content.width().max(1.0))
            .min(h / content.height().max(1.0))
            .clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        let offset = screen.center() - content.center() * scale;
        Viewport { scale, offset }
    }

    /// Whether the view can draw anything: a finite scale above 0 and a finite offset.
    pub fn is_sane(&self) -> bool {
        self.scale.is_finite()
            && self.scale > 0.0
            && self.offset.x.is_finite()
            && self.offset.y.is_finite()
    }

    /// The view after the canvas went from `from` to `to` (the window resized, a pane opened):
    /// the design point in the middle of the canvas stays there and the zoom is kept (#1266).
    pub fn recentred(self, from: ScreenRect, to: ScreenRect) -> Viewport {
        let d = to.center() - from.center();
        Viewport {
            scale: self.scale,
            offset: self.offset + d,
        }
    }

    /// The view made safe for `screen` (#1266): a view that cannot draw (scale 0, NaN or
    /// infinite) becomes the fit of `content`; the scale is brought into the zoom limits about
    /// the canvas's middle; and the pan is limited so that a strip of `content` (the Design
    /// Page with the design) at least [`Self::KEEP_IN_VIEW`] points wide, or a quarter of a
    /// smaller canvas, stays inside the canvas. A view already inside those limits is returned
    /// unchanged, bit for bit.
    pub fn kept_in_view(self, content: Rect, screen: ScreenRect) -> Viewport {
        if !self.is_sane() {
            return Viewport::fit(content, screen, 24.0);
        }
        let mut vp = self;
        if !(Self::MIN_SCALE..=Self::MAX_SCALE).contains(&vp.scale) {
            let mid = screen.center();
            let model = vp.to_model(mid);
            vp.scale = vp.scale.clamp(Self::MIN_SCALE, Self::MAX_SCALE);
            vp.offset = mid - model * vp.scale;
        }
        // The offset on one axis: as it is while `content` (`lo..hi`, model) reaches `m` points
        // into the canvas (`from`, `size` wide), else the nearest one that does. It is set from
        // the canvas's edge, not moved by a difference, so an offset far away loses nothing to
        // rounding.
        let scale = vp.scale;
        let keep = |offset: f64, lo: f64, hi: f64, from: f64, size: f64| {
            let m = Self::KEEP_IN_VIEW.min(size / 4.0);
            if offset + hi * scale < from + m {
                from + m - hi * scale
            } else if offset + lo * scale > from + size - m {
                from + size - m - lo * scale
            } else {
                offset
            }
        };
        vp.offset = v2(
            keep(
                vp.offset.x,
                content.min_x,
                content.max_x,
                screen.x,
                screen.width,
            ),
            keep(
                vp.offset.y,
                content.min_y,
                content.max_y,
                screen.y,
                screen.height,
            ),
        );
        if vp.is_sane() {
            vp
        } else {
            Viewport::fit(content, screen, 24.0)
        }
    }

    /// Points of the page or design that always stay inside the canvas ([`Self::kept_in_view`]).
    pub const KEEP_IN_VIEW: f64 = 48.0;

    /// Zoom as a percentage where 100 % shows 1 mm as 1 point × `points_per_mm`.
    pub fn zoom_percent(&self, points_per_mm: f64) -> f64 {
        self.scale * 10.0 / points_per_mm * 100.0
    }

    /// Zoom Box (#518): the view that makes the model rectangle `content` fill `screen`
    /// (the tighter side touches the edges), or `None` for a box too small to mean anything.
    pub fn zoom_box(content: Rect, screen: ScreenRect) -> Option<Viewport> {
        (content.width() > 1e-6 && content.height() > 1e-6)
            .then(|| Viewport::fit(content, screen, 0.0))
    }
}

/// Points per mm at 100 % without a calibration: a 96 dpi screen, as before #518.
pub const DEFAULT_POINTS_PER_MM: f64 = 96.0 / 25.4;

/// Calibration limits: from a 48 dpi to a 600 dpi display, in points per mm.
pub const CALIBRATION_RANGE: std::ops::RangeInclusive<f64> = 1.8..=24.0;

/// Points per mm from a calibration line `points` long that measures `mm` on a ruler held to
/// the screen; `None` outside [`CALIBRATION_RANGE`].
pub fn calibrated(points: f64, mm: f64) -> Option<f64> {
    let k = points / mm;
    (mm > 0.0 && k.is_finite() && CALIBRATION_RANGE.contains(&k)).then_some(k)
}

#[cfg(test)]
mod zoom_tests {
    use super::*;

    #[test]
    fn a_zoom_box_fills_the_view() {
        let screen = ScreenRect {
            x: 10.0,
            y: 20.0,
            width: 800.0,
            height: 600.0,
        };
        let r = Rect {
            min_x: 100.0,
            min_y: 200.0,
            max_x: 500.0,
            max_y: 400.0,
        };
        let vp = Viewport::zoom_box(r, screen).expect("box");
        let a = vp.to_screen(v2(r.min_x, r.min_y));
        let b = vp.to_screen(v2(r.max_x, r.max_y));
        // 400 × 200 into 800 × 600: the width fills the view exactly, centred.
        assert!(((b.x - a.x) / screen.width - 1.0).abs() < 0.02);
        assert!(((a.y + b.y) / 2.0 - screen.center().y).abs() < 1e-9);
        assert!(Viewport::zoom_box(Rect { max_x: 100.0, ..r }, screen).is_none());
    }

    #[test]
    fn a_view_is_kept_in_the_canvas_and_a_good_one_is_left_as_it_is() {
        let screen = ScreenRect {
            x: 286.0,
            y: 150.0,
            width: 430.0,
            height: 567.0,
        };
        let page = Rect {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1000.0,
            max_y: 1000.0,
        };
        // The fit, and views with the page partly outside: unchanged, bit for bit.
        let fit = Viewport::fit(page, screen, 24.0);
        assert_eq!(fit.kept_in_view(page, screen), fit);
        let mut partly = fit;
        partly.zoom_about(v2(300.0, 160.0), 7.3);
        partly.pan(-211.7, -206.9);
        assert_eq!(partly.kept_in_view(page, screen), partly);
        // Panned off each side, near and very far: a strip of the page 48 points wide is back
        // inside, exactly at the canvas's edge, and the zoom is the same.
        for far in [600.0, 1.0e7, 1.0e30, 1.0e300] {
            for (dx, dy) in [(far, 0.0), (-far, 0.0), (0.0, far), (0.0, -far)] {
                let mut away = fit;
                away.pan(dx, dy);
                let kept = away.kept_in_view(page, screen);
                assert_eq!(kept.scale, fit.scale);
                let (a, b) = (
                    kept.to_screen(v2(0.0, 0.0)),
                    kept.to_screen(v2(1000.0, 1000.0)),
                );
                let m = Viewport::KEEP_IN_VIEW;
                let edge = if dx > 0.0 {
                    a.x - (screen.x + screen.width - m)
                } else if dx < 0.0 {
                    b.x - (screen.x + m)
                } else if dy > 0.0 {
                    a.y - (screen.y + screen.height - m)
                } else {
                    b.y - (screen.y + m)
                };
                assert!(edge.abs() < 1e-6, "panned by ({dx}, {dy}): {kept:?}");
            }
        }
        // A scale outside the limits comes back to them about the canvas's middle.
        let tiny = Viewport {
            scale: 1.0e-6,
            offset: screen.center() - v2(400.0, 300.0) * 1.0e-6,
        };
        let kept = tiny.kept_in_view(page, screen);
        assert_eq!(kept.scale, Viewport::MIN_SCALE);
        let mid = kept.to_model(screen.center());
        assert!((mid.x - 400.0).abs() < 1e-6 && (mid.y - 300.0).abs() < 1e-6);
        // What cannot draw becomes the fit.
        for bad in [0.0, -2.0, f64::NAN, f64::INFINITY] {
            let v = Viewport {
                scale: bad,
                offset: v2(5.0, 5.0),
            };
            assert_eq!(v.kept_in_view(page, screen), fit, "scale {bad}");
        }
        let v = Viewport {
            scale: 0.5,
            offset: v2(f64::NAN, 0.0),
        };
        assert_eq!(v.kept_in_view(page, screen), fit);
    }

    #[test]
    fn a_resized_canvas_keeps_its_middle_and_a_canvas_of_no_size_is_not_usable() {
        let from = ScreenRect {
            x: 286.0,
            y: 150.0,
            width: 430.0,
            height: 567.0,
        };
        let to = ScreenRect {
            x: 0.0,
            y: 120.0,
            width: 900.0,
            height: 300.0,
        };
        let vp = Viewport {
            scale: 0.7624,
            offset: v2(-30.2, 132.3),
        };
        let moved = vp.recentred(from, to);
        assert_eq!(moved.scale, vp.scale);
        let (a, b) = (vp.to_model(from.center()), moved.to_model(to.center()));
        assert!((a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
        // There and back is the view it was.
        let back = moved.recentred(to, from);
        assert!(
            (back.offset.x - vp.offset.x).abs() < 1e-9
                && (back.offset.y - vp.offset.y).abs() < 1e-9
        );
        assert!(from.usable() && to.usable());
        for (width, height) in [
            (0.0, 0.0),
            (10.0, 400.0),
            (400.0, 31.9),
            (-5.0, 400.0),
            (f64::NAN, 400.0),
        ] {
            assert!(
                !ScreenRect {
                    width,
                    height,
                    ..from
                }
                .usable(),
                "{width} x {height}"
            );
        }
    }

    #[test]
    fn calibration_sets_what_100_percent_means() {
        // A 100 mm ruler covers 470 points on this screen.
        let k = calibrated(470.0, 100.0).expect("plausible");
        // 1 mm (10 model units) = k points.
        let vp = Viewport {
            scale: k / 10.0,
            offset: v2(0.0, 0.0),
        };
        assert!((vp.zoom_percent(k) - 100.0).abs() < 1e-9);
        // So a 100 mm line is 100 mm on screen at 100 %: 470 points.
        assert!(
            (vp.to_screen(v2(1000.0, 0.0)).x - vp.to_screen(v2(0.0, 0.0)).x - 470.0).abs() < 1e-9
        );
        assert_eq!(calibrated(5.0, 100.0), None);
        assert_eq!(calibrated(470.0, 0.0), None);
        assert!((DEFAULT_POINTS_PER_MM - 3.7795).abs() < 1e-4);
    }
}
