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
