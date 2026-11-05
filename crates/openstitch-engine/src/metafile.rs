//! Windows metafiles (WMF and EMF) read as vector shapes for `import.metafile`.
//!
//! A pure-Rust record player with no platform APIs: it replays the drawing records a GDI device
//! context would, keeping the selected pen and brush, the mapping mode, window and viewport,
//! EMF's world transform and Save/RestoreDC, and turns every drawing record into one shape the
//! way [`crate::imports::parse_svg`] does: a closed figure drawn with a brush gets a fill region
//! in the brush colour, a pen draws a running line in the pen colour, a null pen or brush draws
//! nothing.
//!
//! Records read:
//! - WMF (placeable or standard): MoveTo, LineTo, Polyline, Polygon, PolyPolygon, Rectangle,
//!   RoundRect, Ellipse, Arc, Chord, Pie; CreatePenIndirect, CreateBrushIndirect (pattern
//!   brushes use the default black), SelectObject, DeleteObject; SetMapMode, Set/Offset/Scale
//!   Window and Viewport Org/Ext; SaveDC, RestoreDC.
//! - EMF: the same shapes in 32- and 16-bit forms plus PolyBezier(To), PolylineTo,
//!   PolyPolyline, ArcTo and SetArcDirection; paths (BeginPath … EndPath, CloseFigure,
//!   FillPath, StrokePath, StrokeAndFillPath, AbortPath); CreatePen, ExtCreatePen, stock
//!   objects; Set/ModifyWorldTransform.
//!
//! Everything else (text, bitmaps, regions, clipping, PolyDraw, EMF+ comments) is skipped.
//! Ellipses and arcs become cubic Béziers, so curves stay exact.
//!
//! Units: a placeable WMF gives its units per inch; an EMF's header gives the reference
//! device's size in pixels and millimetres; the metric and English mapping modes give their own
//! units. A standard WMF in MM_TEXT, or in MM_ANISOTROPIC/MM_ISOTROPIC without a viewport, is
//! read at 96 logical units per inch, like an SVG pixel.

use crate::imports::{MAX_SVG_BYTES, MAX_SVG_PATHS, MAX_SVG_POINTS, SvgShape};
use openstitch_core::Thread;
use openstitch_design::fmath::Det;
use openstitch_design::{Bezier, Contour, Vec2, v2};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Largest metafile accepted.
pub const MAX_METAFILE_BYTES: usize = MAX_SVG_BYTES;
/// Deepest SaveDC stack kept; deeper saves are ignored.
const MAX_SAVED: usize = 1024;
/// Most GDI objects tracked at once.
const MAX_OBJECTS: usize = 65_536;

const WMF_PLACEABLE_KEY: u32 = 0x9AC6_CDD7;
const EMF_SIGNATURE: u32 = 0x464D_4520;

/// One SVG pixel (96 per inch) in 0.1 mm: the unit of a standard WMF without physical units.
const PX: f64 = 254.0 / 96.0;

/// Why a metafile could not be imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetafileError {
    Parse(String),
    TooLarge(String),
}

impl std::fmt::Display for MetafileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "not a usable WMF/EMF: {e}"),
            Self::TooLarge(e) => write!(f, "metafile too large: {e}"),
        }
    }
}

/// Which metafile `data` holds, if either.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Wmf,
    Emf,
}

/// Recognises a WMF (placeable or standard) or EMF by its header.
pub fn sniff(data: &[u8]) -> Option<Kind> {
    let r = Rec(data);
    if r.u32(0) == Some(1) && r.u32(40) == Some(EMF_SIGNATURE) {
        return Some(Kind::Emf);
    }
    let off = if r.u32(0) == Some(WMF_PLACEABLE_KEY) {
        22
    } else {
        0
    };
    let kind = r.u16(off)?;
    (matches!(kind, 1 | 2) && r.u16(off + 2) == Some(9)).then_some(Kind::Wmf)
}

/// Parses a WMF or EMF into shapes in 0.1 mm (+y down), in drawing order.
pub fn parse_metafile(data: &[u8]) -> Result<Vec<SvgShape>, MetafileError> {
    if data.len() > MAX_METAFILE_BYTES {
        return Err(MetafileError::TooLarge(format!("{} bytes", data.len())));
    }
    match sniff(data) {
        Some(Kind::Emf) => parse_emf(data),
        Some(Kind::Wmf) => parse_wmf(data),
        None => Err(MetafileError::Parse("no WMF or EMF header".into())),
    }
}

fn truncated(what: &str) -> MetafileError {
    MetafileError::Parse(format!("truncated {what}"))
}

/// Little-endian reads that fail instead of panicking.
#[derive(Clone, Copy)]
struct Rec<'a>(&'a [u8]);

impl Rec<'_> {
    fn bytes<const N: usize>(&self, o: usize) -> Option<[u8; N]> {
        self.0.get(o..o.checked_add(N)?)?.try_into().ok()
    }
    fn u16(&self, o: usize) -> Option<u16> {
        self.bytes(o).map(u16::from_le_bytes)
    }
    fn i16(&self, o: usize) -> Option<f64> {
        self.bytes(o).map(|b| f64::from(i16::from_le_bytes(b)))
    }
    fn u32(&self, o: usize) -> Option<u32> {
        self.bytes(o).map(u32::from_le_bytes)
    }
    fn i32(&self, o: usize) -> Option<f64> {
        self.bytes(o).map(|b| f64::from(i32::from_le_bytes(b)))
    }
    fn f32(&self, o: usize) -> Option<f64> {
        self.bytes(o).map(|b| f64::from(f32::from_le_bytes(b)))
    }
    /// A COLORREF (red, green, blue, reserved) as a thread.
    fn color(&self, o: usize) -> Option<Thread> {
        let [r, g, b, _] = self.bytes::<4>(o)?;
        Some(Thread::rgb(
            u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b),
        ))
    }
    /// `n` points from `o`, 16-bit (`wide` false) or 32-bit, as (x, y) pairs. Fails before
    /// allocating when the record is too short for `n`.
    fn points(&self, o: usize, n: usize, wide: bool) -> Option<Vec<Vec2>> {
        let size = if wide { 8 } else { 4 };
        let end = n.checked_mul(size)?.checked_add(o)?;
        if end > self.0.len() {
            return None;
        }
        (0..n)
            .map(|i| {
                let at = o + i * size;
                if wide {
                    Some(v2(self.i32(at)?, self.i32(at + 4)?))
                } else {
                    Some(v2(self.i16(at)?, self.i16(at + 2)?))
                }
            })
            .collect()
    }
}

/// A GDI object in the object table.
#[derive(Clone, Debug)]
enum Obj {
    Pen(Option<Thread>),
    Brush(Option<Thread>),
    Other,
}

/// The device-context state SaveDC keeps.
#[derive(Clone, Debug)]
struct Dc {
    map_mode: u32,
    win_org: Vec2,
    win_ext: Vec2,
    vp_org: Vec2,
    vp_ext: Vec2,
    vp_ext_set: bool,
    /// EMF world transform `x' = m11·x + m21·y + dx`, `y' = m12·x + m22·y + dy`.
    world: [f64; 6],
    pen: Option<Thread>,
    brush: Option<Thread>,
    /// Current position, logical units.
    pos: Vec2,
    clockwise: bool,
}

impl Default for Dc {
    fn default() -> Self {
        Dc {
            map_mode: 1,
            win_org: v2(0.0, 0.0),
            win_ext: v2(1.0, 1.0),
            vp_org: v2(0.0, 0.0),
            vp_ext: v2(1.0, 1.0),
            vp_ext_set: false,
            world: IDENTITY,
            // GDI's defaults: BLACK_PEN and WHITE_BRUSH.
            pen: Some(Thread::rgb(0)),
            brush: Some(Thread::rgb(0xFF_FFFF)),
            pos: v2(0.0, 0.0),
            clockwise: false,
        }
    }
}

const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `a` then `b`.
fn compose(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

/// How logical units become 0.1 mm.
#[derive(Clone, Copy, Debug)]
enum Units {
    /// A placeable WMF: 0.1 mm per logical unit.
    Placeable(f64),
    /// 0.1 mm per device pixel (x, y), for MM_TEXT and the scaled modes.
    Device(Vec2),
}

/// A drawing segment in logical units.
#[derive(Clone, Copy, Debug)]
enum Seg {
    Line(Vec2),
    Curve(Vec2, Vec2, Vec2),
}

impl Seg {
    fn end(self) -> Vec2 {
        match self {
            Seg::Line(p) | Seg::Curve(_, _, p) => p,
        }
    }
}

/// A figure being built, already in 0.1 mm.
#[derive(Clone, Debug, Default)]
struct Fig {
    points: Vec<Vec2>,
    curves: Vec<Option<Bezier>>,
}

/// The finished contour of `f`, or `None` when it is degenerate. A closed figure whose last
/// point sits on its first keeps the last segment as the closing one.
fn finish(mut f: Fig, closed: bool) -> Option<Contour> {
    if closed {
        let n = f.points.len();
        if n > 1 && f.points[n - 1].distance(f.points[0]) < 1e-6 {
            f.points.pop();
        } else {
            f.curves.push(None);
        }
    }
    let curved = f.curves.iter().any(Option::is_some);
    let enough = if closed {
        f.points.len() >= 3 || (f.points.len() >= 2 && curved)
    } else {
        f.points.len() >= 2
    };
    if !enough || !f.points.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
        return None;
    }
    let mut c = Contour::new(f.points, closed);
    if curved {
        c.curves = f.curves;
    }
    Some(c)
}

/// An open contour closed with a straight segment, as FillPath closes figures.
fn closed(c: Contour) -> Option<Contour> {
    if c.closed {
        return Some(c);
    }
    let mut curves = c.curves;
    if curves.is_empty() {
        curves = vec![None; c.points.len().saturating_sub(1)];
    }
    finish(
        Fig {
            points: c.points,
            curves,
        },
        true,
    )
}

/// The bounding box `l, t, r, b` as centre and radii, or `None` when it is flat.
fn ellipse_of(l: f64, t: f64, r: f64, b: f64) -> Option<(Vec2, f64, f64)> {
    let (rx, ry) = ((r - l).abs() / 2.0, (b - t).abs() / 2.0);
    (rx > 0.0 && ry > 0.0).then_some((v2((l + r) / 2.0, (t + b) / 2.0), rx, ry))
}

/// The point at parametric angle `a` (counter-clockwise on screen, +y down).
fn on_ellipse(c: Vec2, rx: f64, ry: f64, a: f64) -> Vec2 {
    let (s, co) = a.dsin_cos();
    v2(c.x + rx * co, c.y - ry * s)
}

/// The parametric angle of the ray from the centre through `p`.
fn angle_of(c: Vec2, rx: f64, ry: f64, p: Vec2) -> f64 {
    (-(p.y - c.y) / ry).datan2((p.x - c.x) / rx)
}

/// Cubic segments along the ellipse from angle `a0` through `sweep` radians.
fn arc_segs(c: Vec2, rx: f64, ry: f64, a0: f64, sweep: f64) -> Vec<Seg> {
    let n = (sweep.abs() / FRAC_PI_2).ceil().clamp(1.0, 4.0) as usize;
    let d = sweep / n as f64;
    let k = 4.0 / 3.0 * (d / 4.0).dtan();
    let tangent = |a: f64| {
        let (s, co) = a.dsin_cos();
        v2(-rx * s, -ry * co)
    };
    (0..n)
        .map(|i| {
            let (a, b) = (a0 + d * i as f64, a0 + d * (i + 1) as f64);
            let (p, q) = (on_ellipse(c, rx, ry, a), on_ellipse(c, rx, ry, b));
            Seg::Curve(p + tangent(a) * k, q - tangent(b) * k, q)
        })
        .collect()
}

/// The arc of the ellipse in `l, t, r, b` from the ray through `start` to the ray through
/// `end`: its first point and segments. A start equal to the end draws the whole ellipse.
fn arc(
    (l, t, r, b): (f64, f64, f64, f64),
    start: Vec2,
    end: Vec2,
    clockwise: bool,
) -> Option<(Vec2, Vec<Seg>)> {
    let (c, rx, ry) = ellipse_of(l, t, r, b)?;
    let (a0, a1) = (angle_of(c, rx, ry, start), angle_of(c, rx, ry, end));
    let full = |s: f64| if s < 1e-9 { TAU } else { s };
    let sweep = if clockwise {
        -full((a0 - a1).rem_euclid(TAU))
    } else {
        full((a1 - a0).rem_euclid(TAU))
    };
    Some((on_ellipse(c, rx, ry, a0), arc_segs(c, rx, ry, a0, sweep)))
}

fn rectangle(l: f64, t: f64, r: f64, b: f64) -> (Vec2, Vec<Seg>) {
    (
        v2(l, t),
        vec![
            Seg::Line(v2(r, t)),
            Seg::Line(v2(r, b)),
            Seg::Line(v2(l, b)),
        ],
    )
}

fn round_rect(l: f64, t: f64, r: f64, b: f64, w: f64, h: f64) -> (Vec2, Vec<Seg>) {
    let (l, r) = (l.min(r), l.max(r));
    let (t, b) = (t.min(b), t.max(b));
    let rx = (w.abs() / 2.0).min((r - l) / 2.0);
    let ry = (h.abs() / 2.0).min((b - t) / 2.0);
    if rx <= 0.0 || ry <= 0.0 {
        return rectangle(l, t, r, b);
    }
    let q = -FRAC_PI_2;
    let mut segs = vec![Seg::Line(v2(r - rx, t))];
    segs.extend(arc_segs(v2(r - rx, t + ry), rx, ry, FRAC_PI_2, q));
    segs.push(Seg::Line(v2(r, b - ry)));
    segs.extend(arc_segs(v2(r - rx, b - ry), rx, ry, 0.0, q));
    segs.push(Seg::Line(v2(l + rx, b)));
    segs.extend(arc_segs(v2(l + rx, b - ry), rx, ry, -FRAC_PI_2, q));
    segs.push(Seg::Line(v2(l, t + ry)));
    segs.extend(arc_segs(v2(l + rx, t + ry), rx, ry, PI, q));
    (v2(l + rx, t), segs)
}

fn lines(points: &[Vec2]) -> Option<(Vec2, Vec<Seg>)> {
    let (first, rest) = points.split_first()?;
    Some((*first, rest.iter().map(|p| Seg::Line(*p)).collect()))
}

/// Cubic segments from Bézier control and end points in threes.
fn beziers(points: &[Vec2]) -> Vec<Seg> {
    (0..points.len() / 3)
        .map(|i| Seg::Curve(points[3 * i], points[3 * i + 1], points[3 * i + 2]))
        .collect()
}

/// The record player shared by WMF and EMF.
struct Player {
    dc: Dc,
    saved: Vec<Dc>,
    units: Units,
    objects: Vec<Option<Obj>>,
    shapes: Vec<SvgShape>,
    points: usize,
    /// Contours of the path being recorded (EMF BeginPath … EndPath).
    path: Option<Vec<Contour>>,
    /// The ended path, waiting for FillPath / StrokePath.
    ended: Option<Vec<Contour>>,
    /// The open figure LineTo-style records extend.
    figure: Option<Fig>,
}

impl Player {
    fn new(units: Units) -> Self {
        Player {
            dc: Dc::default(),
            saved: Vec::new(),
            units,
            objects: Vec::new(),
            shapes: Vec::new(),
            points: 0,
            path: None,
            ended: None,
            figure: None,
        }
    }

    /// Logical → 0.1 mm, +y down.
    fn map(&self, p: Vec2) -> Vec2 {
        let w = &self.dc.world;
        let (x, y) = (
            w[0] * p.x + w[2] * p.y + w[4],
            w[1] * p.x + w[3] * p.y + w[5],
        );
        let dc = &self.dc;
        match self.units {
            Units::Placeable(k) => v2(
                (x - dc.win_org.x) * dc.win_ext.x.signum() * k,
                (y - dc.win_org.y) * dc.win_ext.y.signum() * k,
            ),
            Units::Device(px) => {
                // Metric and English modes: fixed units, +y up.
                let fixed = match dc.map_mode {
                    2 => Some(1.0),            // MM_LOMETRIC, 0.1 mm
                    3 => Some(0.1),            // MM_HIMETRIC, 0.01 mm
                    4 => Some(0.254),          // MM_LOENGLISH, 0.01 in
                    5 => Some(0.0254),         // MM_HIENGLISH, 0.001 in
                    6 => Some(254.0 / 1440.0), // MM_TWIPS
                    _ => None,
                };
                if let Some(k) = fixed {
                    return v2(x * k, -y * k);
                }
                let (sx, sy) = match dc.map_mode {
                    7 | 8 => self.scale(),
                    _ => (1.0, 1.0),
                };
                v2(
                    ((x - dc.win_org.x) * sx + dc.vp_org.x) * px.x,
                    ((y - dc.win_org.y) * sy + dc.vp_org.y) * px.y,
                )
            }
        }
    }

    /// Window → viewport scale in MM_ANISOTROPIC / MM_ISOTROPIC.
    fn scale(&self) -> (f64, f64) {
        let dc = &self.dc;
        if !dc.vp_ext_set || dc.win_ext.x == 0.0 || dc.win_ext.y == 0.0 {
            return (dc.win_ext.x.signum(), dc.win_ext.y.signum());
        }
        let (sx, sy) = (dc.vp_ext.x / dc.win_ext.x, dc.vp_ext.y / dc.win_ext.y);
        if dc.map_mode == 7 {
            let m = sx.abs().min(sy.abs());
            (m.copysign(sx), m.copysign(sy))
        } else {
            (sx, sy)
        }
    }

    fn count(&mut self, n: usize) -> Result<(), MetafileError> {
        self.points = self.points.saturating_add(n);
        if self.points > MAX_SVG_POINTS {
            return Err(MetafileError::TooLarge(format!(
                "more than {MAX_SVG_POINTS} points"
            )));
        }
        Ok(())
    }

    /// `start` and `segs` mapped into a figure.
    fn fig(&mut self, start: Vec2, segs: &[Seg]) -> Result<Fig, MetafileError> {
        self.count(segs.len() + 1)?;
        let mut f = Fig {
            points: vec![self.map(start)],
            curves: Vec::with_capacity(segs.len()),
        };
        self.extend(&mut f, segs);
        Ok(f)
    }

    fn extend(&self, f: &mut Fig, segs: &[Seg]) {
        for s in segs {
            match *s {
                Seg::Line(p) => {
                    f.curves.push(None);
                    f.points.push(self.map(p));
                }
                Seg::Curve(a, b, p) => {
                    f.curves.push(Some(Bezier {
                        c1: self.map(a),
                        c2: self.map(b),
                    }));
                    f.points.push(self.map(p));
                }
            }
        }
    }

    /// Draws closed figures (filled with the brush, outlined with the pen).
    fn draw_closed(&mut self, figs: Vec<(Vec2, Vec<Seg>)>) -> Result<(), MetafileError> {
        let mut contours = Vec::with_capacity(figs.len());
        for (start, segs) in figs {
            let f = self.fig(start, &segs)?;
            contours.extend(finish(f, true));
        }
        self.draw(contours, true, true)
    }

    /// Draws an open figure with the pen.
    fn draw_open(&mut self, start: Vec2, segs: &[Seg]) -> Result<(), MetafileError> {
        let f = self.fig(start, segs)?;
        self.draw(finish(f, false).into_iter().collect(), false, true)
    }

    /// Adds contours to the path being recorded, or to the shapes.
    fn draw(
        &mut self,
        contours: Vec<Contour>,
        fill: bool,
        stroke: bool,
    ) -> Result<(), MetafileError> {
        if contours.is_empty() {
            return Ok(());
        }
        if let Some(path) = &mut self.path {
            path.extend(contours);
            return Ok(());
        }
        let fill = if fill { self.dc.brush.clone() } else { None };
        let stroke = if stroke { self.dc.pen.clone() } else { None };
        if fill.is_none() && stroke.is_none() {
            return Ok(());
        }
        if self.shapes.len() >= MAX_SVG_PATHS {
            return Err(MetafileError::TooLarge(format!(
                "more than {MAX_SVG_PATHS} shapes"
            )));
        }
        self.shapes.push(SvgShape {
            contours,
            fill,
            stroke,
            ..SvgShape::default()
        });
        Ok(())
    }

    /// Extends the open figure from the current position (LineTo, PolylineTo, ArcTo …).
    fn chain(&mut self, segs: &[Seg]) -> Result<(), MetafileError> {
        let Some(last) = segs.last() else {
            return Ok(());
        };
        self.count(segs.len())?;
        let mut f = match self.figure.take() {
            Some(f) => f,
            None => {
                self.count(1)?;
                Fig {
                    points: vec![self.map(self.dc.pos)],
                    curves: Vec::new(),
                }
            }
        };
        self.extend(&mut f, segs);
        self.figure = Some(f);
        self.dc.pos = last.end();
        Ok(())
    }

    /// Ends the open figure: into the path when recording one, else drawn with the pen.
    fn end_figure(&mut self, close: bool) -> Result<(), MetafileError> {
        let Some(f) = self.figure.take() else {
            return Ok(());
        };
        let Some(c) = finish(f, close) else {
            return Ok(());
        };
        match &mut self.path {
            Some(path) => path.push(c),
            None => self.draw(vec![c], false, true)?,
        }
        Ok(())
    }

    /// Fills and/or strokes the ended path.
    fn paint_path(&mut self, fill: bool, stroke: bool) -> Result<(), MetafileError> {
        if self.path.is_some() {
            self.end_figure(false)?;
            self.ended = self.path.take();
        }
        let Some(contours) = self.ended.take() else {
            return Ok(());
        };
        let contours = if fill {
            contours.into_iter().filter_map(closed).collect()
        } else {
            contours
        };
        self.draw(contours, fill, stroke)
    }

    fn select(&mut self, obj: Option<Obj>) {
        match obj {
            Some(Obj::Pen(t)) => self.dc.pen = t,
            Some(Obj::Brush(t)) => self.dc.brush = t,
            _ => {}
        }
    }

    fn save(&mut self) {
        if self.saved.len() < MAX_SAVED {
            self.saved.push(self.dc.clone());
        }
    }

    /// RestoreDC: negative counts back from the latest save, positive is a 1-based level.
    fn restore(&mut self, n: f64) {
        let len = self.saved.len() as f64;
        let level = if n < 0.0 { len + n } else { n - 1.0 };
        if (0.0..len).contains(&level) {
            let level = level as usize;
            self.dc = self.saved[level].clone();
            self.saved.truncate(level);
        }
    }

    fn shapes(mut self) -> Result<Vec<SvgShape>, MetafileError> {
        if self.path.is_none() {
            self.end_figure(false)?;
        }
        Ok(self.shapes)
    }
}

// --- WMF ------------------------------------------------------------------------------------

fn parse_wmf(data: &[u8]) -> Result<Vec<SvgShape>, MetafileError> {
    let r = Rec(data);
    let mut off = 0;
    let mut units = Units::Device(v2(PX, PX));
    if r.u32(0) == Some(WMF_PLACEABLE_KEY) {
        let inch = r.u16(14).ok_or_else(|| truncated("placeable header"))?;
        if inch == 0 {
            return Err(MetafileError::Parse("zero units per inch".into()));
        }
        units = Units::Placeable(254.0 / f64::from(inch));
        off = 22;
    }
    if data.len() < off + 18 {
        return Err(truncated("header"));
    }
    off += 18;
    let mut player = Player::new(units);
    while off < data.len() {
        let rec = Rec(&data[off..]);
        let (Some(words), Some(func)) = (rec.u32(0), rec.u16(4)) else {
            return Err(truncated("record"));
        };
        let len = usize::try_from(u64::from(words) * 2).unwrap_or(usize::MAX);
        if len < 6 || len > data.len() - off {
            return Err(truncated("record"));
        }
        if func == 0 {
            break; // META_EOF
        }
        if func != 0x0213 {
            player.end_figure(false)?;
        }
        let rec = Rec(&data[off..off + len]);
        wmf_record(&mut player, func, rec)?.ok_or_else(|| truncated("record"))?;
        off += len;
    }
    player.shapes()
}

/// Plays one WMF record; `None` when it is too short for its fields.
fn wmf_record(p: &mut Player, func: u16, r: Rec) -> Result<Option<()>, MetafileError> {
    // Parameter `i` (16-bit words after the 6-byte record header).
    let w = |i: usize| r.i16(6 + 2 * i);
    macro_rules! get {
        ($e:expr) => {
            match $e {
                Some(v) => v,
                None => return Ok(None),
            }
        };
    }
    match func {
        0x0103 => p.dc.map_mode = u32::from(get!(r.u16(6))), // SETMAPMODE
        0x020B => p.dc.win_org = v2(get!(w(1)), get!(w(0))), // SETWINDOWORG
        0x020C => p.dc.win_ext = v2(get!(w(1)), get!(w(0))), // SETWINDOWEXT
        0x020D => p.dc.vp_org = v2(get!(w(1)), get!(w(0))),  // SETVIEWPORTORG
        0x020E => {
            // SETVIEWPORTEXT
            p.dc.vp_ext = v2(get!(w(1)), get!(w(0)));
            p.dc.vp_ext_set = true;
        }
        0x020F => p.dc.win_org = p.dc.win_org + v2(get!(w(1)), get!(w(0))), // OFFSETWINDOWORG
        0x0211 => p.dc.vp_org = p.dc.vp_org + v2(get!(w(1)), get!(w(0))),   // OFFSETVIEWPORTORG
        0x0410 | 0x0412 => {
            // SCALEWINDOWEXT / SCALEVIEWPORTEXT: yDenom, yNum, xDenom, xNum.
            let (yd, yn, xd, xn) = (get!(w(0)), get!(w(1)), get!(w(2)), get!(w(3)));
            if xd != 0.0 && yd != 0.0 {
                let ext = if func == 0x0410 {
                    &mut p.dc.win_ext
                } else {
                    &mut p.dc.vp_ext
                };
                *ext = v2(ext.x * xn / xd, ext.y * yn / yd);
            }
        }
        0x0214 => p.dc.pos = v2(get!(w(1)), get!(w(0))), // MOVETO
        0x0213 => p.chain(&[Seg::Line(v2(get!(w(1)), get!(w(0))))])?, // LINETO
        0x0325 | 0x0324 => {
            // POLYLINE / POLYGON
            let n = usize::from(get!(r.u16(6)));
            let pts = get!(r.points(8, n, false));
            if let Some((start, segs)) = lines(&pts) {
                if func == 0x0324 {
                    p.draw_closed(vec![(start, segs)])?;
                } else {
                    p.draw_open(start, &segs)?;
                }
            }
        }
        0x0538 => {
            // POLYPOLYGON: count, counts, then every polygon's points.
            let n = usize::from(get!(r.u16(6)));
            let mut counts = Vec::with_capacity(n.min(r.0.len() / 2));
            for i in 0..n {
                counts.push(usize::from(get!(r.u16(8 + 2 * i))));
            }
            let mut at = 8 + 2 * n;
            let mut figs = Vec::new();
            for c in counts {
                let pts = get!(r.points(at, c, false));
                at += 4 * c;
                figs.extend(lines(&pts));
            }
            p.draw_closed(figs)?;
        }
        0x041B => {
            // RECTANGLE: bottom, right, top, left
            let (b, rt, t, l) = (get!(w(0)), get!(w(1)), get!(w(2)), get!(w(3)));
            p.draw_closed(vec![rectangle(l, t, rt, b)])?;
        }
        0x061C => {
            // ROUNDRECT: height, width, bottom, right, top, left
            let (h, wd) = (get!(w(0)), get!(w(1)));
            let (b, rt, t, l) = (get!(w(2)), get!(w(3)), get!(w(4)), get!(w(5)));
            p.draw_closed(vec![round_rect(l, t, rt, b, wd, h)])?;
        }
        0x0418 => {
            // ELLIPSE: bottom, right, top, left
            let (b, rt, t, l) = (get!(w(0)), get!(w(1)), get!(w(2)), get!(w(3)));
            if let Some((c, rx, ry)) = ellipse_of(l, t, rt, b) {
                let start = on_ellipse(c, rx, ry, 0.0);
                p.draw_closed(vec![(start, arc_segs(c, rx, ry, 0.0, TAU))])?;
            }
        }
        0x0817 | 0x081A | 0x0830 => {
            // ARC / PIE / CHORD: yEnd, xEnd, yStart, xStart, bottom, right, top, left
            let end = v2(get!(w(1)), get!(w(0)));
            let start = v2(get!(w(3)), get!(w(2)));
            let bx = (get!(w(7)), get!(w(6)), get!(w(5)), get!(w(4)));
            curved(p, func_kind(func), bx, start, end, false)?;
        }
        0x02FA => {
            // CREATEPENINDIRECT: style, width (x, y), colour
            let style = get!(r.u16(6));
            let color = get!(r.color(12));
            wmf_create(p, Obj::Pen((style & 0x0F != 5).then_some(color)));
        }
        0x02FC => {
            // CREATEBRUSHINDIRECT: style, colour, hatch
            let style = get!(r.u16(6));
            let color = get!(r.color(8));
            wmf_create(p, Obj::Brush((style != 1).then_some(color)));
        }
        0x0142 | 0x01F9 => wmf_create(p, Obj::Brush(Some(Thread::rgb(0)))), // pattern brushes
        0x02FB | 0x00F7 | 0x06FF => wmf_create(p, Obj::Other),              // font, palette, region
        0x012D => {
            // SELECTOBJECT
            let i = usize::from(get!(r.u16(6)));
            let obj = p.objects.get(i).cloned().flatten();
            p.select(obj);
        }
        0x01F0 => {
            // DELETEOBJECT
            let i = usize::from(get!(r.u16(6)));
            if let Some(slot) = p.objects.get_mut(i) {
                *slot = None;
            }
        }
        0x001E => p.save(),              // SAVEDC
        0x0127 => p.restore(get!(w(0))), // RESTOREDC
        _ => {}
    }
    Ok(Some(()))
}

/// A WMF object takes the lowest free slot of the object table.
fn wmf_create(p: &mut Player, obj: Obj) {
    if let Some(slot) = p.objects.iter_mut().find(|s| s.is_none()) {
        *slot = Some(obj);
    } else if p.objects.len() < MAX_OBJECTS {
        p.objects.push(Some(obj));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArcKind {
    Arc,
    Pie,
    Chord,
}

fn func_kind(func: u16) -> ArcKind {
    match func {
        0x081A => ArcKind::Pie,
        0x0830 => ArcKind::Chord,
        _ => ArcKind::Arc,
    }
}

/// Arc, Pie and Chord (`clockwise` from SetArcDirection).
fn curved(
    p: &mut Player,
    kind: ArcKind,
    bx: (f64, f64, f64, f64),
    start: Vec2,
    end: Vec2,
    clockwise: bool,
) -> Result<(), MetafileError> {
    let Some((first, segs)) = arc(bx, start, end, clockwise) else {
        return Ok(());
    };
    match kind {
        ArcKind::Arc => p.draw_open(first, &segs),
        ArcKind::Chord => p.draw_closed(vec![(first, segs)]),
        ArcKind::Pie => {
            let c = v2((bx.0 + bx.2) / 2.0, (bx.1 + bx.3) / 2.0);
            let mut all = vec![Seg::Line(first)];
            all.extend(segs);
            p.draw_closed(vec![(c, all)])
        }
    }
}

// --- EMF ------------------------------------------------------------------------------------

fn parse_emf(data: &[u8]) -> Result<Vec<SvgShape>, MetafileError> {
    let r = Rec(data);
    // Reference device: pixels (72) and millimetres (80).
    let device = |o: usize| Some((r.i32(o)?, r.i32(o + 4)?));
    let (Some((dx, dy)), Some((mx, my))) = (device(72), device(80)) else {
        return Err(truncated("header"));
    };
    let px = |mm: f64, d: f64| {
        if mm > 0.0 && d > 0.0 {
            mm * 10.0 / d
        } else {
            PX
        }
    };
    let mut player = Player::new(Units::Device(v2(px(mx, dx), px(my, dy))));
    let mut off = 0;
    while off < data.len() {
        let rec = Rec(&data[off..]);
        let (Some(kind), Some(size)) = (rec.u32(0), rec.u32(4)) else {
            return Err(truncated("record"));
        };
        let len = usize::try_from(size).unwrap_or(usize::MAX);
        if len < 8 || len > data.len() - off {
            return Err(truncated("record"));
        }
        if kind == 14 {
            break; // EMR_EOF
        }
        let chain = matches!(kind, 5 | 6 | 54 | 55 | 88 | 89);
        if player.path.is_none() && !chain {
            player.end_figure(false)?;
        }
        let rec = Rec(&data[off..off + len]);
        emf_record(&mut player, kind, rec)?.ok_or_else(|| truncated("record"))?;
        off += len;
    }
    player.shapes()
}

/// Plays one EMF record; `None` when it is too short for its fields.
fn emf_record(p: &mut Player, kind: u32, r: Rec) -> Result<Option<()>, MetafileError> {
    macro_rules! get {
        ($e:expr) => {
            match $e {
                Some(v) => v,
                None => return Ok(None),
            }
        };
    }
    let pt = |o: usize| Some(v2(r.i32(o)?, r.i32(o + 4)?));
    let rect = |o: usize| Some((r.i32(o)?, r.i32(o + 4)?, r.i32(o + 8)?, r.i32(o + 12)?));
    match kind {
        2..=6 | 85..=89 => {
            // POLYBEZIER, POLYGON, POLYLINE, POLYBEZIERTO, POLYLINETO (+16-bit forms):
            // bounds, count, points.
            let wide = kind < 85;
            let n = get!(r.u32(24)) as usize;
            let pts = get!(r.points(28, n, wide));
            match if wide { kind } else { kind - 83 } {
                2 => {
                    if let Some((first, rest)) = pts.split_first() {
                        p.draw_open(*first, &beziers(rest))?;
                    }
                }
                3 => {
                    if let Some(fig) = lines(&pts) {
                        p.draw_closed(vec![fig])?;
                    }
                }
                4 => {
                    if let Some((start, segs)) = lines(&pts) {
                        p.draw_open(start, &segs)?;
                    }
                }
                5 => p.chain(&beziers(&pts))?,
                _ => p.chain(&pts.iter().map(|q| Seg::Line(*q)).collect::<Vec<_>>())?,
            }
        }
        7 | 8 | 90 | 91 => {
            // POLYPOLYLINE, POLYPOLYGON (+16-bit): bounds, nPolys, total, counts, points.
            let wide = kind < 90;
            let n = get!(r.u32(24)) as usize;
            if n.checked_mul(4).is_none_or(|b| b > r.0.len()) {
                return Ok(None);
            }
            let counts: Vec<usize> = get!(
                (0..n)
                    .map(|i| r.u32(32 + 4 * i).map(|c| c as usize))
                    .collect::<Option<Vec<_>>>()
            );
            let mut at = 32 + 4 * n;
            let size = if wide { 8 } else { 4 };
            let mut figs = Vec::new();
            for c in counts {
                let pts = get!(r.points(at, c, wide));
                at += size * c;
                figs.extend(lines(&pts));
            }
            if kind == 8 || kind == 91 {
                p.draw_closed(figs)?;
            } else {
                for (start, segs) in figs {
                    p.draw_open(start, &segs)?;
                }
            }
        }
        9 => p.dc.win_ext = get!(pt(8)),  // SETWINDOWEXTEX
        10 => p.dc.win_org = get!(pt(8)), // SETWINDOWORGEX
        11 => {
            // SETVIEWPORTEXTEX
            p.dc.vp_ext = get!(pt(8));
            p.dc.vp_ext_set = true;
        }
        12 => p.dc.vp_org = get!(pt(8)),      // SETVIEWPORTORGEX
        17 => p.dc.map_mode = get!(r.u32(8)), // SETMAPMODE
        27 => {
            // MOVETOEX
            if p.path.is_some() {
                p.end_figure(false)?;
            }
            p.dc.pos = get!(pt(8));
        }
        54 => p.chain(&[Seg::Line(get!(pt(8)))])?, // LINETO
        42 => {
            // ELLIPSE
            let (l, t, rt, b) = get!(rect(8));
            if let Some((c, rx, ry)) = ellipse_of(l, t, rt, b) {
                let start = on_ellipse(c, rx, ry, 0.0);
                p.draw_closed(vec![(start, arc_segs(c, rx, ry, 0.0, TAU))])?;
            }
        }
        43 => {
            // RECTANGLE
            let (l, t, rt, b) = get!(rect(8));
            p.draw_closed(vec![rectangle(l, t, rt, b)])?;
        }
        44 => {
            // ROUNDRECT: box, corner size
            let (l, t, rt, b) = get!(rect(8));
            let corner = get!(pt(24));
            p.draw_closed(vec![round_rect(l, t, rt, b, corner.x, corner.y)])?;
        }
        45..=47 => {
            // ARC, CHORD, PIE: box, start, end
            let bx = get!(rect(8));
            let (start, end) = (get!(pt(24)), get!(pt(32)));
            let kind = match kind {
                46 => ArcKind::Chord,
                47 => ArcKind::Pie,
                _ => ArcKind::Arc,
            };
            let clockwise = p.dc.clockwise;
            curved(p, kind, bx, start, end, clockwise)?;
        }
        55 => {
            // ARCTO: a line from the current position to the arc's start, then the arc.
            let bx = get!(rect(8));
            let (start, end) = (get!(pt(24)), get!(pt(32)));
            if let Some((first, segs)) = arc(bx, start, end, p.dc.clockwise) {
                let mut all = vec![Seg::Line(first)];
                all.extend(segs);
                p.chain(&all)?;
            }
        }
        57 => p.dc.clockwise = get!(r.u32(8)) == 2, // SETARCDIRECTION
        38 => {
            // CREATEPEN: index, style, width (x, y), colour
            let i = get!(r.u32(8));
            let style = get!(r.u32(12));
            let color = get!(r.color(24));
            emf_create(p, i, Obj::Pen((style & 0x0F != 5).then_some(color)));
        }
        95 => {
            // EXTCREATEPEN: index, bitmap offsets/sizes, then style, width, brush style, colour
            let i = get!(r.u32(8));
            let style = get!(r.u32(28));
            let brush = get!(r.u32(36));
            let color = get!(r.color(40));
            let visible = style & 0x0F != 5 && brush != 1;
            emf_create(p, i, Obj::Pen(visible.then_some(color)));
        }
        39 => {
            // CREATEBRUSHINDIRECT: index, style, colour, hatch
            let i = get!(r.u32(8));
            let style = get!(r.u32(12));
            let color = get!(r.color(16));
            emf_create(p, i, Obj::Brush((style != 1).then_some(color)));
        }
        93 | 94 => {
            // CREATEMONOBRUSH, CREATEDIBPATTERNBRUSHPT
            let i = get!(r.u32(8));
            emf_create(p, i, Obj::Brush(Some(Thread::rgb(0))));
        }
        49 | 82 => {
            // CREATEPALETTE, EXTCREATEFONTINDIRECTW
            let i = get!(r.u32(8));
            emf_create(p, i, Obj::Other);
        }
        37 => {
            // SELECTOBJECT
            let i = get!(r.u32(8));
            let obj = if i & 0x8000_0000 != 0 {
                stock(i & 0x7FFF_FFFF)
            } else {
                p.objects.get(i as usize).cloned().flatten()
            };
            p.select(obj);
        }
        40 => {
            // DELETEOBJECT
            let i = get!(r.u32(8)) as usize;
            if let Some(slot) = p.objects.get_mut(i) {
                *slot = None;
            }
        }
        33 => p.save(),                  // SAVEDC
        34 => p.restore(get!(r.i32(8))), // RESTOREDC
        35 | 36 => {
            // SETWORLDTRANSFORM, MODIFYWORLDTRANSFORM (+ mode)
            let mut x = [0.0; 6];
            for (i, v) in x.iter_mut().enumerate() {
                *v = get!(r.f32(8 + 4 * i));
            }
            if x.iter().all(|v| v.is_finite()) {
                let mode = if kind == 35 { 4 } else { get!(r.u32(32)) };
                p.dc.world = match mode {
                    1 => IDENTITY,
                    2 => compose(x, p.dc.world),
                    3 => compose(p.dc.world, x),
                    4 => x,
                    _ => p.dc.world,
                };
            }
        }
        59 => {
            // BEGINPATH
            p.figure = None;
            p.ended = None;
            p.path = Some(Vec::new());
        }
        60 => {
            // ENDPATH
            if p.path.is_some() {
                p.end_figure(false)?;
                p.ended = p.path.take();
            }
        }
        61 => {
            // CLOSEFIGURE
            if p.path.is_some() {
                p.end_figure(true)?;
            }
        }
        62 => p.paint_path(true, false)?, // FILLPATH
        63 => p.paint_path(true, true)?,  // STROKEANDFILLPATH
        64 => p.paint_path(false, true)?, // STROKEPATH
        68 => {
            // ABORTPATH
            p.path = None;
            p.ended = None;
            p.figure = None;
        }
        _ => {}
    }
    Ok(Some(()))
}

fn emf_create(p: &mut Player, index: u32, obj: Obj) {
    let i = index as usize;
    if i >= MAX_OBJECTS {
        return;
    }
    if p.objects.len() <= i {
        p.objects.resize(i + 1, None);
    }
    p.objects[i] = Some(obj);
}

/// GDI stock objects (index without the high bit).
fn stock(i: u32) -> Option<Obj> {
    let brush = |rgb: u32| Some(Obj::Brush(Some(Thread::rgb(rgb))));
    match i {
        0 | 18 => brush(0xFF_FFFF), // WHITE_BRUSH, DC_BRUSH
        1 => brush(0xC0_C0C0),
        2 => brush(0x80_8080),
        3 => brush(0x40_4040),
        4 => brush(0),
        5 => Some(Obj::Brush(None)),
        6 => Some(Obj::Pen(Some(Thread::rgb(0xFF_FFFF)))),
        7 | 19 => Some(Obj::Pen(Some(Thread::rgb(0)))), // BLACK_PEN, DC_PEN
        8 => Some(Obj::Pen(None)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WMF writer for the tests' own fixtures.
    struct Wmf(Vec<u8>);

    impl Wmf {
        fn new(placeable_inch: Option<u16>) -> Self {
            let mut b = Vec::new();
            if let Some(inch) = placeable_inch {
                b.extend(WMF_PLACEABLE_KEY.to_le_bytes());
                b.extend([0u8; 2]); // handle
                for v in [0i16, 0, 1000, 1000] {
                    b.extend(v.to_le_bytes());
                }
                b.extend(inch.to_le_bytes());
                b.extend([0u8; 6]); // reserved, checksum
            }
            // META_HEADER: memory metafile, 9 words, version 0x0300, size, objects, max record,
            // members.
            for v in [1u16, 9, 0x0300, 0, 0, 4, 0, 0, 0] {
                b.extend(v.to_le_bytes());
            }
            Wmf(b)
        }
        fn rec(mut self, func: u16, params: &[i16]) -> Self {
            self.0.extend((3 + params.len() as u32).to_le_bytes());
            self.0.extend(func.to_le_bytes());
            for p in params {
                self.0.extend(p.to_le_bytes());
            }
            self
        }
        fn end(self) -> Vec<u8> {
            self.rec(0, &[]).0
        }
    }

    fn color_words(rgb: u32) -> [i16; 2] {
        let (r, g, b) = ((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
        [i16::from_le_bytes([r, g]), i16::from_le_bytes([b, 0])]
    }

    /// A WMF drawing a red-filled, blue-outlined rectangle, a black polyline, and an ellipse
    /// with a null pen, in 1000 units per inch.
    fn sample_wmf() -> Vec<u8> {
        let [r0, r1] = color_words(0xFF0000);
        let [b0, b1] = color_words(0x0000FF);
        Wmf::new(Some(1000))
            .rec(0x0103, &[8]) // MM_ANISOTROPIC
            .rec(0x020B, &[0, 0])
            .rec(0x020C, &[1000, 1000])
            .rec(0x02FC, &[0, r0, r1, 0]) // brush 0: solid red
            .rec(0x02FA, &[0, 1, 1, b0, b1]) // pen 1: solid blue
            .rec(0x02FA, &[5, 0, 0, 0, 0]) // pen 2: null
            .rec(0x012D, &[0])
            .rec(0x012D, &[1])
            .rec(0x041B, &[500, 400, 100, 0]) // rectangle (0,100)-(400,500)
            .rec(0x01F0, &[1]) // delete the blue pen
            .rec(0x02FA, &[0, 1, 1, 0, 0]) // pen 1 again: black
            .rec(0x012D, &[1])
            .rec(0x0325, &[3, 0, 0, 500, 0, 500, 500]) // polyline
            .rec(0x012D, &[2]) // null pen
            .rec(0x0418, &[1000, 1000, 600, 600]) // ellipse, fill only
            .end()
    }

    #[test]
    fn placeable_wmf_shapes_colours_and_units() {
        let shapes = parse_metafile(&sample_wmf()).expect("wmf");
        assert_eq!(shapes.len(), 3, "{shapes:?}");
        let rect = &shapes[0];
        assert_eq!(
            rect.fill.as_ref().map(Thread::hex).as_deref(),
            Some("#ff0000")
        );
        assert_eq!(
            rect.stroke.as_ref().map(Thread::hex).as_deref(),
            Some("#0000ff")
        );
        // 1000 units per inch: 400 units = 10.16 mm = 101.6 (0.1 mm).
        let c = &rect.contours[0];
        assert!(c.closed && c.points.len() == 4);
        assert!((c.points[1].x - 101.6).abs() < 1e-9 && (c.points[2].y - 127.0).abs() < 1e-9);
        let line = &shapes[1];
        assert!(line.fill.is_none() && !line.contours[0].closed);
        assert_eq!(
            line.stroke.as_ref().map(Thread::hex).as_deref(),
            Some("#000000")
        );
        let ellipse = &shapes[2];
        assert!(ellipse.stroke.is_none() && ellipse.fill.is_some());
        let e = &ellipse.contours[0];
        assert!(e.closed && e.has_curves() && e.points.len() == 4);
        // On the ellipse at 45°: centre (800, 800) units, radius 200.
        let Some(Some(b)) = e.curves.first() else {
            panic!("curve")
        };
        let mid = cubic_mid(e.points[0], b, e.points[1]);
        let r = (mid - v2(203.2, 203.2)).length();
        assert!((r - 50.8).abs() < 0.02, "{r}");
    }

    fn cubic_mid(p0: Vec2, b: &Bezier, p1: Vec2) -> Vec2 {
        (p0 + b.c1 * 3.0 + b.c2 * 3.0 + p1) * 0.125
    }

    #[test]
    fn standard_wmf_arcs_pies_and_polypolygons() {
        // No placeable header, MM_TEXT: 96 units per inch.
        let wmf = Wmf::new(None)
            // POLYPOLYGON: a square with a square hole.
            .rec(
                0x0538,
                &[
                    2, 4, 4, 0, 0, 96, 0, 96, 96, 0, 96, 24, 24, 72, 24, 72, 72, 24, 72,
                ],
            )
            // PIE over the box (0,0)-(96,96) from 3 o'clock to 12 o'clock (a quarter).
            .rec(0x081A, &[0, 48, 48, 96, 96, 96, 0, 0])
            // ARC, half: from 3 o'clock to 9 o'clock, counter-clockwise over the top.
            .rec(0x0817, &[48, 0, 48, 96, 96, 96, 0, 0])
            .end();
        let shapes = parse_metafile(&wmf).expect("wmf");
        assert_eq!(shapes.len(), 3);
        assert_eq!(shapes[0].contours.len(), 2);
        assert!((shapes[0].contours[0].points[2].x - 254.0).abs() < 1e-9); // 1 inch
        let pie = &shapes[1].contours[0];
        assert!(pie.closed && pie.points.len() == 3, "{pie:?}"); // centre, start, end
        assert!((pie.points[0] - v2(127.0, 127.0)).length() < 1e-9);
        assert!((pie.points[2] - v2(127.0, 0.0)).length() < 1e-9, "{pie:?}");
        let arc = &shapes[2].contours[0];
        assert!(!arc.closed && arc.points.len() == 3);
        assert!((arc.points[1] - v2(127.0, 0.0)).length() < 1e-9, "{arc:?}"); // over the top
        assert!((arc.points[2] - v2(0.0, 127.0)).length() < 1e-9);
    }

    /// An EMF writer for the tests' own fixtures; `device`/`mm` fill the header.
    struct Emf(Vec<u8>);

    impl Emf {
        fn new() -> Self {
            let mut h = vec![0u8; 88];
            h[0..4].copy_from_slice(&1u32.to_le_bytes());
            h[4..8].copy_from_slice(&88u32.to_le_bytes());
            h[40..44].copy_from_slice(&EMF_SIGNATURE.to_le_bytes());
            h[44..48].copy_from_slice(&0x10000u32.to_le_bytes());
            // 1000 × 1000 pixel device of 100 × 100 mm: 1 px = 0.1 mm.
            for (o, v) in [(72, 1000i32), (76, 1000), (80, 100), (84, 100)] {
                h[o..o + 4].copy_from_slice(&v.to_le_bytes());
            }
            Emf(h)
        }
        fn rec(mut self, kind: u32, params: &[i32]) -> Self {
            self.0.extend(kind.to_le_bytes());
            self.0.extend((8 + 4 * params.len() as u32).to_le_bytes());
            for p in params {
                self.0.extend(p.to_le_bytes());
            }
            self
        }
        fn end(self) -> Vec<u8> {
            self.rec(14, &[0, 0, 0]).0
        }
    }

    fn colorref(rgb: u32) -> i32 {
        let (r, g, b) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);
        (r | g << 8 | b << 16) as i32
    }

    #[test]
    fn emf_paths_beziers_world_transform_and_stock_objects() {
        let bounds = [0, 0, 0, 0];
        let mut bez = bounds.to_vec();
        bez.extend([4, 0, 0, 100, 0, 100, 100, 200, 100]); // POLYBEZIER: start + 1 curve
        let emf = Emf::new()
            .rec(39, &[1, 0, colorref(0x00FF00), 0]) // brush 1: green
            .rec(37, &[1])
            .rec(37, &[0x8000_0008u32 as i32]) // NULL_PEN
            // A path: a triangle closed with CloseFigure, then FillPath.
            .rec(59, &[])
            .rec(27, &[0, 0])
            .rec(54, &[100, 0])
            .rec(54, &[100, 100])
            .rec(61, &[])
            .rec(60, &[])
            .rec(62, &bounds)
            .rec(37, &[0x8000_0007u32 as i32]) // BLACK_PEN
            .rec(2, &bez)
            // World transform: scale ×2, then a rectangle (0,0)-(10,10) → 20 × 20 px.
            .rec(
                35,
                &[2.0f32, 0.0, 0.0, 2.0, 0.0, 0.0].map(|v| v.to_bits() as i32),
            )
            .rec(43, &[0, 0, 10, 10])
            .end();
        let shapes = parse_metafile(&emf).expect("emf");
        assert_eq!(shapes.len(), 3, "{shapes:?}");
        let tri = &shapes[0];
        assert!(tri.stroke.is_none());
        assert_eq!(
            tri.fill.as_ref().map(Thread::hex).as_deref(),
            Some("#00ff00")
        );
        assert!(tri.contours[0].closed && tri.contours[0].points.len() == 3);
        assert!((tri.contours[0].points[1].x - 100.0).abs() < 1e-9); // 100 px = 10 mm
        let curve = &shapes[1];
        assert!(curve.fill.is_none() && curve.contours[0].has_curves());
        assert_eq!(curve.contours[0].points.len(), 2);
        let rect = &shapes[2].contours[0];
        assert!(
            (rect.points[2] - v2(20.0, 20.0)).length() < 1e-9,
            "{rect:?}"
        );
    }

    #[test]
    fn emf_metric_mapping_mode_flips_y() {
        let emf = Emf::new()
            .rec(17, &[2]) // MM_LOMETRIC
            .rec(43, &[0, 0, 100, -50]) // 10 × 5 mm, +y up
            .end();
        let shapes = parse_metafile(&emf).expect("emf");
        let c = &shapes[0].contours[0];
        assert!((c.points[2] - v2(100.0, 50.0)).length() < 1e-9, "{c:?}");
    }

    #[test]
    fn hostile_metafiles_fail_cleanly() {
        let wmf = sample_wmf();
        for cut in [0, 3, 21, 30, 45, wmf.len() - 3] {
            let r = parse_metafile(&wmf[..cut]);
            assert!(r.is_err(), "cut {cut}: {r:?}");
        }
        // A polyline claiming 65535 points in a 10-byte record, and a zero-length record.
        let huge = Wmf::new(None).rec(0x0325, &[-1, 0]).end();
        assert!(parse_metafile(&huge).is_err());
        let mut zero = Wmf::new(None).0;
        zero.extend([0u8; 6]);
        zero[18 + 4] = 0x25;
        assert!(parse_metafile(&zero).is_err());
        // EMF: a poly-polygon with 2^30 polygons, a record size past the end, a size of 0.
        let pp = Emf::new().rec(8, &[0, 0, 0, 0, 1 << 30, 4]).end();
        assert!(parse_metafile(&pp).is_err());
        let mut past = Emf::new().rec(43, &[0, 0, 1, 1]).end();
        let n = past.len();
        past[88 + 4..88 + 8].copy_from_slice(&(n as u32).to_le_bytes());
        assert!(parse_metafile(&past).is_err());
        let mut zero = Emf::new().0;
        zero.extend([43, 0, 0, 0, 0, 0, 0, 0]);
        assert!(parse_metafile(&zero).is_err());
        // Too many shapes.
        let mut many = Wmf::new(None);
        for _ in 0..=MAX_SVG_PATHS {
            many = many.rec(0x041B, &[10, 10, 0, 0]);
        }
        assert!(matches!(
            parse_metafile(&many.end()),
            Err(MetafileError::TooLarge(_))
        ));
        // Not a metafile, NaN transforms, deep RestoreDC.
        assert!(parse_metafile(b"<svg/>").is_err());
        let nan = Emf::new()
            .rec(35, &[f32::NAN.to_bits() as i32; 6])
            .rec(34, &[-1_000_000])
            .rec(43, &[0, 0, 10, 10])
            .end();
        assert_eq!(parse_metafile(&nan).map(|s| s.len()), Ok(1));
    }
}
