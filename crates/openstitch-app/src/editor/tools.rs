use super::handles::{Handle, SelectionHandles};
use super::hit::{bounds_of, object_at, object_at_opts, objects_in, rect_from};
use super::pen::{PenNode, is_valid, path_request, pen_contour};
use super::points::{
    Control, PointAction, PointHit, PointRef, can_delete_points, point_actions_enabled,
    point_at_selected, smooth_curve, with_control,
};
use super::request::{CommandRequest, mm};
use super::snap::{SnapGuide, SnapLines, TOLERANCE_POINTS, snap_move};
use openstitch_design::{Contour, Design, DesignObject, ObjectKind, Rect, Transform, Vec2, v2};
use serde_json::json;
use std::collections::BTreeSet;

pub mod angle_line;
pub mod builder;
pub mod characters;
pub mod continuation;
pub mod fill_angle;
pub mod gradient;
pub mod keyboard;
pub mod knife;
pub mod needles;
pub mod punch_rungs;
pub mod satin_width;
pub mod skeleton;
pub use characters::{CharacterDrag, CharacterFrame, CharacterOverlay};
pub use continuation::Continuation;
pub use needles::NeedleOverlay;
pub use punch_rungs::RungOverlay;
pub use satin_width::WidthOverlay;
pub use skeleton::SkeletonOverlay;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Rectangle,
    Ellipse,
    Polygon,
    /// Point editing: move vertices and curve handles of the selected shape.
    Point,
    /// Bézier pen: click for corners, drag for smooth points.
    Pen,
    /// Manual Punch: click the two ends of each sewing-direction line in turn.
    Punch,
    /// Open Straight Line: click the points of an open line.
    Line,
    /// Pencil (Closed Curve): drag to draw a closed outline freehand.
    Pencil,
    /// Pencil (Open Curve): drag to draw an open line freehand.
    PencilOpen,
    /// Manual Punch Curved Block: like [`Tool::Punch`], the sides curve through the line ends.
    CurvedBlock,
    /// Manual Punch Running: click the points of a running-stitch line.
    Running,
    /// Manual Punch Feed: click the points the frame jumps through without sewing.
    Feed,
    /// Satin Skeleton: draw a satin column's centre line like the pen, with a width per point.
    Skeleton,
    /// Knife: click an open cut line; every object it crosses is cut (#477).
    Knife,
    /// Shape Builder: drag across overlapping shapes' pieces to merge (Alt: remove) them (#497).
    ShapeBuilder,
    /// Gradient: drag (or click twice) where the selected gradient fills start and end (#532).
    Gradient,
    /// Fill Angle: drag across the selected regions in the direction their rows run (#534).
    FillAngle,
    /// Angle Line: drag lines across the selected fill that its rows turn between (#534).
    AngleLine,
}

impl Tool {
    pub const ALL: [Tool; 19] = [
        Tool::Select,
        Tool::Point,
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Polygon,
        Tool::Pen,
        Tool::Punch,
        Tool::Line,
        Tool::Pencil,
        Tool::PencilOpen,
        Tool::CurvedBlock,
        Tool::Running,
        Tool::Feed,
        Tool::Skeleton,
        Tool::Knife,
        Tool::ShapeBuilder,
        Tool::Gradient,
        Tool::FillAngle,
        Tool::AngleLine,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Rectangle => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Polygon => "Polygon",
            Tool::Point => "Point",
            Tool::Pen => "Pen",
            Tool::Punch => "Punch",
            Tool::Line => "Line",
            Tool::Pencil => "Pencil",
            Tool::PencilOpen => "OpenPencil",
            Tool::CurvedBlock => "CurvedBlock",
            Tool::Running => "Running",
            Tool::Feed => "Feed",
            Tool::Skeleton => "Skeleton",
            Tool::Knife => "Knife",
            Tool::ShapeBuilder => "ShapeBuilder",
            Tool::Gradient => "Gradient",
            Tool::FillAngle => "FillAngle",
            Tool::AngleLine => "AngleLine",
        }
    }

    /// Single-key shortcut on the canvas, if the tool has one.
    pub fn shortcut(self) -> Option<char> {
        Some(match self {
            Tool::Select => 'v',
            Tool::Rectangle => 'r',
            Tool::Ellipse => 'e',
            Tool::Polygon => 'p',
            Tool::Point => 'a',
            Tool::Pen => 'b',
            Tool::Punch => 'u',
            Tool::Line => 'l',
            _ => return None,
        })
    }

    pub fn from_shortcut(c: char) -> Option<Tool> {
        Self::ALL
            .into_iter()
            .find(|t| t.shortcut() == Some(c.to_ascii_lowercase()))
    }

    /// The Manual Punch tools (they share the Manual Punch tab).
    pub fn is_punch(self) -> bool {
        matches!(
            self,
            Tool::Punch | Tool::CurvedBlock | Tool::Running | Tool::Feed
        )
    }

    /// The freehand Pencil tools.
    pub fn is_pencil(self) -> bool {
        matches!(self, Tool::Pencil | Tool::PencilOpen)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Dragged,
    Up,
    Moved,
}

/// A pointer event in model units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    pub phase: Phase,
    pub location: Vec2,
    /// Constrain: square/circle, axis-locked move, proportional scale, 15° rotate, toggle selection.
    pub shift: bool,
    /// 2 on the second press of a double click.
    pub clicks: u32,
    /// Ctrl (Cmd on macOS) held: this drag ignores Snap to Grid and Snap to Objects.
    pub free: bool,
    /// Alt held: a click on a stitch pattern selects every pattern sewing the colour under
    /// the pointer (PE-DESIGN's selection by thread colour).
    pub alt: bool,
}

impl PointerEvent {
    pub fn new(phase: Phase, location: Vec2) -> Self {
        PointerEvent {
            phase,
            location,
            shift: false,
            clicks: 1,
            free: false,
            alt: false,
        }
    }

    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }
}

/// Counts presses so a second press soon after and near the first is a double-click (the
/// canvas sets [`PointerEvent::clicks`] from it).
#[derive(Clone, Copy, Debug, Default)]
pub struct ClickCounter {
    last: Option<(f64, Vec2, u32)>,
}

impl ClickCounter {
    /// Seconds within which presses count together.
    pub const INTERVAL: f64 = 0.4;
    /// Screen points the pointer may travel between them.
    pub const SLOP: f64 = 6.0;

    /// Registers a press at `time` (seconds) and screen position `at`; returns its count.
    pub fn press(&mut self, time: f64, at: Vec2) -> u32 {
        let count = match self.last {
            Some((t, p, n)) if time - t <= Self::INTERVAL && p.distance(at) <= Self::SLOP => n + 1,
            _ => 1,
        };
        self.last = Some((time, at, count));
        count
    }
}

/// What a tool needs to know about the document when it handles an event.
pub struct ToolContext<'a> {
    pub design: &'a Design,
    pub selection: &'a BTreeSet<u64>,
    /// Model units per screen point at the current zoom.
    pub units_per_point: f64,
    /// Whether `object.scale` takes `originX`/`originY` (checked against the command registry).
    pub scale_takes_origin: bool,
}

impl ToolContext<'_> {
    /// Pointer slop in model units.
    pub fn tolerance(&self) -> f64 {
        4.0 * self.units_per_point
    }

    /// Hit radius of vertices and curve handles in model units.
    pub fn point_radius(&self) -> f64 {
        SelectionHandles::RADIUS * 1.4 * self.units_per_point
    }

    /// The shape the Point tool edits: the selection when it is exactly one visible shape.
    pub fn point_target(&self) -> Option<&DesignObject> {
        if self.selection.len() != 1 {
            return None;
        }
        let id = *self.selection.first()?;
        self.design
            .objects
            .iter()
            .find(|o| o.id == id && o.kind == ObjectKind::Shape && o.visible && !o.locked)
    }

    /// The shapes whose points the Point tool shows and edits (#1260): the one selected shape
    /// or, with several objects selected, every plain shape among them (visible, not locked,
    /// and not a Manual Punch column or a satin line, which have handles of their own).
    pub fn point_targets(&self) -> Vec<&DesignObject> {
        if self.selection.len() <= 1 {
            return self.point_target().into_iter().collect();
        }
        self.design
            .objects
            .iter()
            .filter(|o| {
                self.selection.contains(&o.id)
                    && o.kind == ObjectKind::Shape
                    && o.visible
                    && !o.locked
                    && punch_rungs::rungs(o).is_none()
                    && !satin_width::is_satin_line(o)
            })
            .collect()
    }

    /// The Point tool's target with this id.
    pub fn point_target_of(&self, id: u64) -> Option<&DesignObject> {
        self.point_targets().into_iter().find(|o| o.id == id)
    }

    /// The outline `p` names on one of the Point tool's targets.
    pub fn point_contour(&self, p: PointRef) -> Option<&Contour> {
        self.point_target_of(p.id)?.contours.get(p.contour)
    }

    /// Whether the selection holds a locked object (#502): no handles, no dragging.
    pub fn locked(&self) -> bool {
        super::objects::any_locked(self.design, self.selection)
    }

    pub fn handles(&self) -> Option<SelectionHandles> {
        if self.selection.is_empty() || self.locked() {
            return None;
        }
        bounds_of(self.design, self.selection).map(|bounds| SelectionHandles {
            bounds,
            units_per_point: self.units_per_point,
        })
    }
}

/// Feedback while a gesture is in progress; the document changes only when it ends.
#[derive(Clone, Debug, PartialEq)]
pub enum ToolPreview {
    Marquee(Rect),
    /// Outline being drawn (open while a polygon is still being clicked in).
    Outline(Contour),
    /// The selection shown transformed.
    Transform(Transform),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolEffect {
    Select(BTreeSet<u64>),
    /// Commands to run in order; `select_result` selects what the last one created.
    Execute {
        requests: Vec<CommandRequest>,
        select_result: bool,
    },
}

impl ToolEffect {
    fn run(request: CommandRequest, select_result: bool) -> Self {
        ToolEffect::Execute {
            requests: vec![request],
            select_result,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Gesture {
    Pending {
        start: Vec2,
        hit: Option<u64>,
        shift: bool,
    },
    Moving {
        start: Vec2,
    },
    Scaling {
        handle: Handle,
        start: Vec2,
        handles: SelectionHandles,
    },
    Rotating {
        start: Vec2,
        handles: SelectionHandles,
    },
    Marquee {
        start: Vec2,
        shift: bool,
        base: BTreeSet<u64>,
    },
    Drawing {
        start: Vec2,
    },
    /// Point tool: a vertex being dragged; `edited` is the contour once the drag has started.
    /// Point tool: a drag box over the outline's points, from empty space.
    PointBox {
        start: Vec2,
    },
    /// Point tool: an end of the shape's direction line being dragged (`arrow`: the end with
    /// the arrow head); `edited` is the new direction angle once the drag has started.
    DirectionEnd {
        id: u64,
        arrow: bool,
        start: Vec2,
        edited: Option<f64>,
    },
    Vertex {
        point: PointRef,
        start: Vec2,
        /// Every outline with a selected point, as moved: (object id, contour) and the
        /// outline; the dragged point's outline first.
        edited: Option<Vec<((u64, usize), Contour)>>,
    },
    /// Point tool: a curve handle being dragged.
    Control {
        id: u64,
        contour: usize,
        segment: usize,
        control: Control,
        start: Vec2,
        edited: Option<Contour>,
    },
    /// Pen tool: the anchor just placed, its handles being pulled out.
    PenAnchor {
        index: usize,
    },
    /// Satin Skeleton: a width handle of point `index` being dragged, of the line being drawn
    /// or (`stored`) of that object's skeleton; `edited` is the new width, mm.
    SkeletonWidth {
        index: usize,
        stored: Option<u64>,
        edited: Option<f64>,
    },
    /// Satin Skeleton: a point of object `id`'s stored skeleton being dragged.
    SkeletonPoint {
        id: u64,
        index: usize,
        start: Vec2,
        edited: Option<Vec2>,
    },
    /// Point tool on a text: character `index` being moved, turned or resized.
    Character {
        id: u64,
        index: usize,
        mode: CharacterDrag,
        start: Vec2,
        base: openstitch_design::CharEdit,
        frame: CharacterFrame,
        edited: Option<openstitch_design::CharEdit>,
    },
    /// Point tool on a stitch pattern: selected needle points being dragged, or a marquee.
    Needles {
        start: Vec2,
        delta: Option<Vec2>,
        marquee: bool,
    },
    /// Point tool on a satin line: one edge (`side`) of width key `index` being dragged.
    Width {
        id: u64,
        index: usize,
        side: usize,
        start: Vec2,
        to: Option<Vec2>,
    },
    /// Point tool on a Manual Punch column: one end (`side`) of rung `index` being dragged.
    Rung {
        id: u64,
        index: usize,
        side: usize,
        start: Vec2,
        to: Option<Vec2>,
    },
}

/// The one straight sewing-direction line of a filled shape, as the Point tool shows it
/// (#1260, PE-DESIGN capture g): PE-DESIGN draws a shape's fill direction as a thin red line
/// right across the shape, from a small square about 5 mm outside it to an arrow head about
/// 5 mm outside the far side. Here the line is not stored: it is the region's direction
/// angle, drawn through the middle of the shape's bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionLine {
    pub id: u64,
    /// The end with the square, 0.1 mm.
    pub from: Vec2,
    /// The end with the arrow head: the direction the angle points in.
    pub to: Vec2,
    /// The direction angle shown, degrees (the region's, or the dragged one).
    pub angle: f64,
    /// Whether the line is selected (its square is filled black).
    pub selected: bool,
}

/// How far outside the shape's bounds the direction line's ends lie: 5 mm.
pub const DIRECTION_LINE_REACH: f64 = 50.0;

/// What the Point tool shows over the selected shape: its contours (as edited while a drag is
/// in progress) and the selected vertex.
#[derive(Clone, Debug, PartialEq)]
pub struct PointOverlay {
    pub id: u64,
    pub contours: Vec<Contour>,
    pub selected: Option<(usize, usize)>,
    /// Further selected vertices of this shape (Ctrl+click or Shift+click adds them): the
    /// menu's entries act on all.
    pub also: Vec<(usize, usize)>,
}

/// Pointer-driven tool state machine: select / move / scale / rotate / marquee and the
/// rectangle, ellipse, polygon, point and pen tools. The canvas feeds it events and applies
/// its effects; every gesture ends in at most one command group, so it is one Undo.
#[derive(Clone, Debug, Default)]
pub struct ToolController {
    tool: Option<Tool>,
    preview: Option<ToolPreview>,
    gesture: Option<Gesture>,
    polygon: Vec<Vec2>,
    /// Angle Line tool: the first line drawn, until the second (the engine needs two).
    angle_lines_pending: Vec<[Vec2; 2]>,
    pen: Vec<PenNode>,
    /// Manual Punch: direction-line ends clicked so far (left, right, left, right, …), or
    /// the points of a running line or feed.
    punch: Vec<Vec2>,
    /// Pencil: the pointer trail of the stroke being drawn.
    pencil: Vec<Vec2>,
    /// Edge radius (mm) of the rectangles the Rectangle tool draws.
    corner_radius: f64,
    /// A radius per corner instead (top left, top right, bottom right, bottom left; mm).
    corner_radii: Option<[f64; 4]>,
    /// Point tool: the selected vertex.
    point: Option<PointRef>,
    /// Point tool: further selected vertices (Ctrl+click, Shift+click, a drag box), on any
    /// outline of any shape whose points show.
    more_points: Vec<PointRef>,
    /// Point tool: the vertex pressed with Ctrl or Shift held, and what was selected then;
    /// released without a drag it is added to that selection. With Ctrl (the flag) a vertex
    /// that was selected is taken out instead; with Shift alone it stays selected.
    ctrl_press: Option<(PointRef, Vec<PointRef>, bool)>,
    /// Point tool: the run of points Shift+arrow selected: the point it started at and how
    /// many points it reaches from there (positive: toward the next points of the outline,
    /// negative: toward the previous ones). It counts only while the selection is still that
    /// run.
    point_run: Option<(PointRef, i64)>,
    /// Point tool: the selection a connect gave up (the connected point is gone afterwards);
    /// put back by [`Self::restore_points`] when the connect was refused.
    given_up: Option<(PointRef, Vec<PointRef>)>,
    /// Point tool: the shape whose direction line is selected (a click on one of its ends).
    direction_end: Option<u64>,
    /// Point tool: the end the Alt-dragged end point lies on (a red square shows there), and
    /// whether the running point drag has Alt held.
    connect_hover: Option<Vec2>,
    alt_drag: bool,
    /// Last pointer location (for the Point tool's segment shortcuts).
    hover: Option<Vec2>,
    /// Point tool on a stitch pattern: selected needle point indexes, and whose they are.
    needles: BTreeSet<usize>,
    needle_owner: Option<u64>,
    /// Point tool on a Manual Punch column: the selected rung (column id, rung index).
    rung: Option<(u64, usize)>,
    /// Point tool on a satin line: the selected width key (line id, key index).
    width_key: Option<(u64, usize)>,
    /// Snap to grid: the grid step in model units.
    snap: Option<f64>,
    /// Satin Skeleton: widths (mm) of the anchors in `pen`, and the width of the next one.
    widths: Vec<f64>,
    skeleton_width: Option<f64>,
    /// Pen: width (mm) of the next point once a width was set (#707): the line becomes a satin
    /// line. `None`: a plain line.
    pen_width: Option<f64>,
    /// Pen: the width keys of the line just finished with widths (`keyboard.rs`).
    satin_keys: Option<Vec<[f64; 3]>>,
    /// Point tool on a text: the selected character (text id, character index).
    character: Option<(u64, usize)>,
    /// Snap to Objects (#473) is on, and the lines collected when the current move began.
    snap_objects: bool,
    /// Snap to guides (#799): the guidelines alone when Snap to Objects is off.
    snap_guides: bool,
    snap_lines: Option<SnapLines>,
    /// Guides the moving selection lines up with, drawn dashed.
    guides: Vec<SnapGuide>,
    /// Clicks inside a closed outline without region sewing select it (#475).
    click_inside: bool,
    /// The finished object being continued from one of its ends (#479).
    continuing: Option<Continuation>,
    /// Knife: how far (model units) a single click may be from the line it splits.
    knife_reach: f64,
}

/// Drags shorter than this (points) are clicks.
pub const DRAG_THRESHOLD: f64 = 3.0;
/// Shapes smaller than this (model units, 0.5 mm) are ignored.
pub const MINIMUM_SIZE: f64 = 5.0;

impl ToolController {
    pub fn tool(&self) -> Tool {
        self.tool.unwrap_or(Tool::Select)
    }

    pub fn preview(&self) -> Option<&ToolPreview> {
        self.preview.as_ref()
    }

    pub fn polygon(&self) -> &[Vec2] {
        &self.polygon
    }

    pub fn pen(&self) -> &[PenNode] {
        &self.pen
    }

    pub fn is_busy(&self) -> bool {
        self.gesture.is_some()
            || !self.polygon.is_empty()
            || !self.pen.is_empty()
            || !self.punch.is_empty()
            || !self.pencil.is_empty()
            || self.continuing.is_some()
    }

    /// Edge radius (mm) for new rectangles; 0 = square corners.
    pub fn corner_radius(&self) -> f64 {
        self.corner_radius
    }

    pub fn set_corner_radius(&mut self, mm: f64) {
        self.corner_radius = if mm.is_finite() {
            mm.clamp(0.0, 1000.0)
        } else {
            0.0
        };
    }

    /// A radius per corner (top left, top right, bottom right, bottom left; mm) for new
    /// rectangles, or `None` for the one Edge radius (#497).
    pub fn corner_radii(&self) -> Option<[f64; 4]> {
        self.corner_radii
    }

    pub fn set_corner_radii(&mut self, radii: Option<[f64; 4]>) {
        self.corner_radii = radii.map(|r| {
            r.map(|x| {
                if x.is_finite() {
                    x.clamp(0.0, 1000.0)
                } else {
                    0.0
                }
            })
        });
    }

    /// Snaps drawing, moving and point editing to a grid of `step` model units (`None`: off).
    pub fn set_snap(&mut self, step: Option<f64>) {
        self.snap = step.filter(|s| *s > 0.0);
    }

    pub fn snap(&self) -> Option<f64> {
        self.snap
    }

    /// Snap to Objects on or off, and whether clicks inside unfilled outlines select them.
    pub fn set_object_snapping(&mut self, on: bool, click_inside: bool) {
        self.snap_objects = on;
        self.click_inside = click_inside;
    }

    /// Snap to the guidelines on or off (#799).
    pub fn set_guide_snapping(&mut self, on: bool) {
        self.snap_guides = on;
    }

    /// The lines the moving selection snapped to (empty unless a move is snapping).
    pub fn guides(&self) -> &[SnapGuide] {
        &self.guides
    }

    /// `p` on the nearest grid crossing when snapping is on.
    pub fn snapped(&self, p: Vec2) -> Vec2 {
        self.snap.map_or(p, |s| snap_to_grid(p, s))
    }

    /// The move for a drag from `start` to `p`, and the snap guides it lined up with. With
    /// Snap to Objects an axis within reach of another object's edge or centre, the page or a
    /// guideline snaps there; otherwise, with Snap to Grid, the selection's top-left corner
    /// lands on the grid. An axis-locked move stays on its axis; `e.free` (Ctrl/Cmd) turns
    /// both off for the drag.
    fn move_by(&self, start: Vec2, e: &PointerEvent, ctx: &ToolContext) -> (Vec2, Vec<SnapGuide>) {
        let axis_lock = e.shift;
        let d = move_delta(start, e.location, axis_lock);
        let Some(b) = bounds_of(ctx.design, ctx.selection).filter(|_| !e.free) else {
            return (d, Vec::new());
        };
        let locked = |v: f64| v == 0.0 && axis_lock;
        let (mut ox, mut oy, mut guides) = (None, None, Vec::new());
        if let Some(lines) = self.snap_lines.as_ref() {
            let tol = TOLERANCE_POINTS * ctx.units_per_point;
            (ox, oy, guides) = snap_move(b, d, lines, tol);
            if locked(d.x) {
                ox = None;
                guides.retain(|g| matches!(g, SnapGuide::Horizontal(_)));
            }
            if locked(d.y) {
                oy = None;
                guides.retain(|g| matches!(g, SnapGuide::Vertical(_)));
            }
        }
        let corner = v2(b.min_x, b.min_y);
        let grid = self
            .snap
            .map(|step| snap_to_grid(corner + d, step) - corner);
        let axis = |object: Option<f64>, grid: Option<f64>, raw: f64| {
            if locked(raw) {
                0.0
            } else {
                object.or(grid).unwrap_or(raw)
            }
        };
        (
            v2(
                axis(ox, grid.map(|g| g.x), d.x),
                axis(oy, grid.map(|g| g.y), d.y),
            ),
            guides,
        )
    }

    pub fn set_tool(&mut self, t: Tool) {
        self.cancel();
        self.pen_width = None;
        self.point = None;
        self.more_points.clear();
        self.direction_end = None;
        self.character = None;
        self.tool = Some(t);
    }

    /// Escape: abandon the gesture, polygon or pen path in progress.
    pub fn cancel(&mut self) {
        self.gesture = None;
        self.connect_hover = None;
        self.alt_drag = false;
        self.snap_lines = None;
        self.guides.clear();
        self.polygon.clear();
        self.angle_lines_pending.clear();
        self.pen.clear();
        self.widths.clear();
        self.punch.clear();
        self.pencil.clear();
        self.continuing = None;
        self.preview = None;
    }

    pub fn handle(&mut self, mut e: PointerEvent, ctx: &ToolContext) -> Vec<ToolEffect> {
        self.hover = Some(e.location);
        // Drawing tools place every point on the grid.
        if !matches!(self.tool(), Tool::Select | Tool::Point | Tool::ShapeBuilder)
            && !self.tool().is_pencil()
        {
            e.location = self.snapped(e.location);
        }
        match self.tool() {
            Tool::Select => self.select_tool(e, ctx),
            Tool::Rectangle | Tool::Ellipse => self.drag_shape_tool(e),
            Tool::Polygon | Tool::Line => self.polygon_tool(e, ctx),
            Tool::Point => self.point_tool(e, ctx),
            Tool::Pen => self.pen_tool(e, ctx),
            Tool::Pencil | Tool::PencilOpen => self.pencil_tool(e, ctx),
            Tool::Punch | Tool::CurvedBlock | Tool::Running | Tool::Feed => self.punch_tool(e, ctx),
            Tool::Skeleton => self.skeleton_tool(e, ctx),
            Tool::Knife => self.knife_tool(e, ctx),
            Tool::ShapeBuilder => self.builder_tool(e, ctx),
            Tool::Gradient => self.gradient_tool(e, ctx),
            Tool::FillAngle => self.fill_angle_tool(e, ctx),
            Tool::AngleLine => self.angle_line_tool(e, ctx),
        }
    }

    /// Return (or a double-click outside the Point tool): close the polygon, finish the pen
    /// path open.
    pub fn finish(&mut self) -> Vec<ToolEffect> {
        match self.tool() {
            Tool::Polygon | Tool::Line => self.finish_polygon(),
            Tool::Pen => self.finish_pen(false),
            Tool::Skeleton => self.finish_skeleton(),
            Tool::Knife => self.finish_knife(),
            t if t.is_punch() => self.finish_punch(),
            _ => vec![],
        }
    }

    // MARK: point tool

    /// The selected vertex, if it still exists on one of the Point tool's targets.
    pub fn selected_point(&self, ctx: &ToolContext) -> Option<PointRef> {
        let p = self.point?;
        (p.index < ctx.point_contour(p)?.points.len()).then_some(p)
    }

    /// What lies under `p` for the Point tool, on which of its targets: a vertex, a handle of
    /// a selected vertex (no other handle shows) or, when neither is there, a side.
    fn point_hit<'a>(&self, p: Vec2, ctx: &'a ToolContext) -> Option<(&'a DesignObject, PointHit)> {
        let selected = self.selected_points(ctx);
        let mut side = None;
        for o in ctx.point_targets() {
            let own: Vec<(usize, usize)> = selected
                .iter()
                .filter(|q| q.id == o.id)
                .map(|q| (q.contour, q.index))
                .collect();
            match point_at_selected(o, p, ctx.point_radius(), ctx.tolerance(), &own) {
                Some(hit @ PointHit::Segment { .. }) => {
                    side = side.or(Some((o, hit)));
                }
                Some(hit) => return Some((o, hit)),
                None => {}
            }
        }
        side
    }

    /// Every selected vertex: the one clicked last first, then the others (Ctrl+click,
    /// Shift+click, a drag box), on any outline of any shape whose points show (#1260), as
    /// long as they still exist.
    pub fn selected_points(&self, ctx: &ToolContext) -> Vec<PointRef> {
        let Some(first) = self.selected_point(ctx) else {
            return vec![];
        };
        let mut out = vec![first];
        for p in &self.more_points {
            if !out.contains(p)
                && ctx
                    .point_contour(*p)
                    .is_some_and(|c| p.index < c.points.len())
            {
                out.push(*p);
            }
        }
        out
    }

    /// The points of the run from `anchor` over `span` further points of its outline, the
    /// anchor first; a closed outline's order goes round.
    fn run_points(anchor: PointRef, span: i64, c: &Contour) -> Vec<PointRef> {
        let n = c.points.len() as i64;
        (0..=span.abs())
            .map(|j| {
                let i = anchor.index as i64 + j * span.signum();
                PointRef {
                    index: if c.closed {
                        i.rem_euclid(n)
                    } else {
                        i.clamp(0, n - 1)
                    } as usize,
                    ..anchor
                }
            })
            .collect()
    }

    /// Ctrl+arrow and Shift+arrow with points selected (PE-DESIGN capture f, 2026-11-19). No
    /// point moves. `forward` (Right, Down) goes toward the next point in the outline's order,
    /// otherwise (Left, Up) toward the previous one.
    ///
    /// - Ctrl+arrow (`extend` false) moves the selection to the neighbouring point of the
    ///   point the selection started at, and only that point is selected then. At the end of
    ///   an open line it stays on the end point.
    /// - Shift+arrow (`extend`) grows the selection by one point a press from the point it
    ///   started at, and shrinks it again when pressed the other way; past the start it grows
    ///   on the other side. It stops at the ends of an open line.
    ///
    /// Not captured, chosen here: on a closed outline the order goes round (Ctrl+arrow from
    /// the last point selects the first; Shift+arrow grows until every point is selected);
    /// the point the selection started at is the one selected first (of a drag box's points,
    /// the last in the outline's order); with points on several outlines or shapes the keys
    /// work on the outline of that point and leave only its run selected.
    ///
    /// False, changing nothing, when no outline point is selected with the Point tool.
    pub fn step_points(&mut self, forward: bool, extend: bool, ctx: &ToolContext) -> bool {
        if self.tool() != Tool::Point {
            return false;
        }
        let selected = self.selected_points(ctx);
        let Some(&oldest) = selected.last() else {
            return false;
        };
        // The run so far, while the selection is still exactly it.
        let run = self.point_run.filter(|&(a, k)| {
            ctx.point_contour(a).is_some_and(|c| {
                let points = Self::run_points(a, k, c);
                points.len() == selected.len() && points.iter().all(|p| selected.contains(p))
            })
        });
        let (anchor, span) = run.unwrap_or((oldest, 0));
        let Some(c) = ctx.point_contour(anchor) else {
            return false;
        };
        let n = c.points.len() as i64;
        let step = if forward { 1 } else { -1 };
        if extend {
            let (low, high) = if c.closed {
                (1 - n, n - 1)
            } else {
                (-(anchor.index as i64), n - 1 - anchor.index as i64)
            };
            let span = (span + step).clamp(low, high);
            let points = Self::run_points(anchor, span, c);
            self.point = Some(anchor);
            self.more_points = points.into_iter().skip(1).collect();
            self.point_run = Some((anchor, span));
        } else {
            let to = Self::run_points(anchor, step, c)
                .last()
                .copied()
                .unwrap_or(anchor);
            self.point = Some(to);
            self.more_points.clear();
            self.point_run = Some((to, 0));
        }
        true
    }

    /// The selected vertices by outline: (object id, contour, indexes ascending), the outline
    /// of the vertex clicked last first. The engine's point commands take one outline a call,
    /// so an edit of points on several outlines is one command per outline, in one undo step.
    pub fn selected_by_outline(&self, ctx: &ToolContext) -> Vec<(u64, usize, Vec<usize>)> {
        let mut out: Vec<(u64, usize, Vec<usize>)> = Vec::new();
        for p in self.selected_points(ctx) {
            match out.iter_mut().find(|g| g.0 == p.id && g.1 == p.contour) {
                Some(g) => g.2.push(p.index),
                None => out.push((p.id, p.contour, vec![p.index])),
            }
        }
        for g in &mut out {
            g.2.sort_unstable();
        }
        out
    }

    /// Selects vertices `indexes` of contour `contour` of the Point tool's one target (the
    /// first as the clicked one). False, changing nothing, when there is not exactly one
    /// target or a vertex does not exist.
    pub fn select_points(&mut self, contour: usize, indexes: &[usize], ctx: &ToolContext) -> bool {
        let Some(o) = ctx.point_target() else {
            return false;
        };
        let refs: Vec<PointRef> = indexes
            .iter()
            .map(|&index| PointRef {
                id: o.id,
                contour,
                index,
            })
            .collect();
        self.select_point_refs(&refs, ctx)
    }

    /// Selects `points` (the first as the clicked one), on any outline of any shape whose
    /// points the Point tool shows. False, changing nothing, when the list is empty or one of
    /// them does not exist there.
    pub fn select_point_refs(&mut self, points: &[PointRef], ctx: &ToolContext) -> bool {
        if self.tool() != Tool::Point
            || points.is_empty()
            || points.iter().any(|p| {
                !ctx.point_contour(*p)
                    .is_some_and(|c| p.index < c.points.len())
            })
        {
            return false;
        }
        self.point = Some(points[0]);
        self.more_points = points[1..].to_vec();
        self.direction_end = None;
        true
    }

    /// The middle of the shape whose direction line shows, and its region's angle: the one
    /// selected shape, when its region has one straight sewing direction (a region type with
    /// a direction, not Auto direction, no curved direction and no several direction lines).
    fn direction_base<'a>(&self, ctx: &'a ToolContext) -> Option<(&'a DesignObject, Vec2, f64)> {
        if !self.shows_outline_points(ctx) {
            return None;
        }
        let o = ctx.point_target()?;
        let r = o.region.as_ref()?;
        if r.auto_direction
            || r.direction_curve.is_some()
            || r.direction_lines.len() >= 2
            || !fill_angle::has_direction(r.kind)
        {
            return None;
        }
        let b = bounds_of(ctx.design, &BTreeSet::from([o.id]))?;
        Some((
            o,
            v2((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0),
            r.angle,
        ))
    }

    /// The direction line the Point tool shows for the selected shape; `None` for a shape
    /// without one straight sewing direction (see [`DirectionLine`]). While an end is dragged
    /// it shows the angle the drag gives.
    pub fn direction_line(&self, ctx: &ToolContext) -> Option<DirectionLine> {
        let (o, centre, stored) = self.direction_base(ctx)?;
        let angle = match self.gesture {
            Some(Gesture::DirectionEnd {
                id,
                edited: Some(a),
                ..
            }) if id == o.id => a,
            _ => stored,
        };
        let b = bounds_of(ctx.design, &BTreeSet::from([o.id]))?;
        // Stored angles turn counter-clockwise on screen (y runs down).
        let d = v2(angle.to_radians().cos(), -angle.to_radians().sin());
        let reach = |half: f64, along: f64| {
            if along.abs() < 1e-9 {
                f64::INFINITY
            } else {
                half / along.abs()
            }
        };
        let inside =
            reach((b.max_x - b.min_x) / 2.0, d.x).min(reach((b.max_y - b.min_y) / 2.0, d.y));
        let half = inside + DIRECTION_LINE_REACH;
        Some(DirectionLine {
            id: o.id,
            from: centre - d * half,
            to: centre + d * half,
            angle,
            selected: self.direction_end == Some(o.id),
        })
    }

    /// Selects the shown direction line (as a click on one of its ends does) or drops that
    /// selection; the outline points are deselected. False when no direction line shows.
    pub fn select_direction_line(&mut self, on: bool, ctx: &ToolContext) -> bool {
        let Some(l) = self.direction_line(ctx) else {
            return false;
        };
        self.direction_end = on.then_some(l.id);
        if on {
            self.point = None;
            self.more_points.clear();
        }
        true
    }

    /// Selects the vertex of the Point tool's target under `at` for its right-click menu: a
    /// vertex that is already selected keeps the whole selection, another becomes the only
    /// one. False when no vertex is there (no menu opens).
    pub fn select_point_at(&mut self, at: Vec2, ctx: &ToolContext) -> bool {
        if self.tool() != Tool::Point
            || ctx.needle_target().is_some()
            || ctx.rung_target().is_some()
            || ctx.satin_target().is_some()
            || ctx.text_target().is_some()
        {
            return false;
        }
        let Some((o, PointHit::Vertex { contour, index })) = self.point_hit(at, ctx) else {
            return false;
        };
        let point = PointRef {
            id: o.id,
            contour,
            index,
        };
        if !self.selected_points(ctx).contains(&point) {
            self.point = Some(point);
            self.more_points.clear();
        }
        self.gesture = None;
        true
    }

    /// What is selected of the outline whose points show, for the hint line; `None` when the
    /// Point tool is not showing an outline's points.
    pub fn point_selection(&self, ctx: &ToolContext) -> Option<super::hints::PointSelection> {
        use super::hints::PointSelection as S;
        if !self.shows_outline_points(ctx) {
            return None;
        }
        if self.direction_line(ctx).is_some_and(|l| l.selected) {
            return Some(S::DirectionLine);
        }
        let selected = self.selected_points(ctx);
        Some(match selected.as_slice() {
            [] => S::None,
            [p] => {
                let c = ctx.point_contour(*p)?;
                let end = !c.closed && (p.index == 0 || p.index + 1 == c.points.len());
                if end {
                    S::LineEnd
                } else if c.is_curve_point(p.index) {
                    S::Curve
                } else {
                    S::Straight
                }
            }
            _ => S::Several,
        })
    }

    /// Whether the Point tool is showing the points of an outline (not needle points, a
    /// column's rungs, a satin line's widths or a text's characters).
    pub fn shows_outline_points(&self, ctx: &ToolContext) -> bool {
        self.tool() == Tool::Point
            && !ctx.point_targets().is_empty()
            && ctx.needle_target().is_none()
            && ctx.rung_target().is_none()
            && ctx.satin_target().is_none()
            && ctx.text_target().is_none()
    }

    /// Which entries of the point menu are available for the selected vertices, in
    /// [`PointAction::MENU`] order; `None` when no vertex is selected. With points on several
    /// outlines or shapes an entry is available when it is for the points of one of them
    /// ([`point_actions_enabled`]); Split at Point needs exactly one point in all.
    pub fn point_actions(&self, ctx: &ToolContext) -> Option<[bool; 7]> {
        if self.tool() != Tool::Point {
            return None;
        }
        let groups = self.selected_by_outline(ctx);
        if groups.is_empty() {
            return None;
        }
        let total: usize = groups.iter().map(|g| g.2.len()).sum();
        let mut out = [false; 7];
        for (id, contour, indexes) in &groups {
            let c = ctx.point_contour(PointRef {
                id: *id,
                contour: *contour,
                index: 0,
            })?;
            for (o, e) in out.iter_mut().zip(point_actions_enabled(c, indexes)) {
                *o |= e;
            }
        }
        out[5] &= total == 1;
        Some(out)
    }

    /// Runs one entry of the point menu on the selected vertices: one command per outline
    /// that has selected points the entry applies to, all in one Undo. Nothing happens when
    /// the entry is not available.
    pub fn point_action(&mut self, action: PointAction, ctx: &ToolContext) -> Vec<ToolEffect> {
        let Some(slot) = PointAction::MENU.iter().position(|a| *a == action) else {
            return vec![];
        };
        if !self.point_actions(ctx).is_some_and(|e| e[slot]) {
            return vec![];
        }
        let mut requests = Vec::new();
        for (id, contour, indexes) in self.selected_by_outline(ctx) {
            let Some(c) = ctx.point_contour(PointRef {
                id,
                contour,
                index: 0,
            }) else {
                continue;
            };
            match action.kind() {
                Some(kind) => {
                    if point_actions_enabled(c, &indexes)[slot] {
                        requests.push(CommandRequest::new(
                            "object.setPointKind",
                            json!({ "id": id, "contour": contour, "indexes": indexes, "kind": kind }),
                        ));
                    }
                }
                None if action == PointAction::Split => requests.push(CommandRequest::new(
                    "object.splitAtPoint",
                    json!({ "id": id, "contour": contour, "index": indexes[0] }),
                )),
                // Delete stays available where the outline cannot lose a point, and does
                // nothing there (PE-DESIGN's does nothing at two points).
                None => {
                    if can_delete_points(c, &indexes) {
                        requests.push(CommandRequest::new(
                            "object.deletePoint",
                            json!({ "id": id, "contour": contour, "indexes": indexes }),
                        ));
                    }
                }
            }
        }
        if requests.is_empty() {
            return vec![];
        }
        if action.kind().is_none() {
            self.point = None;
            self.more_points.clear();
        }
        vec![ToolEffect::Execute {
            requests,
            select_result: false,
        }]
    }

    /// The vertices and handles to draw while the Point tool is active: one overlay per shape
    /// whose points show (#1260: every selected plain shape).
    pub fn point_overlays(&self, ctx: &ToolContext) -> Vec<PointOverlay> {
        if self.tool() != Tool::Point || ctx.rung_target().is_some() || ctx.satin_target().is_some()
        {
            return vec![];
        }
        let selected = self.selected_points(ctx);
        ctx.point_targets()
            .into_iter()
            .map(|o| {
                let mut contours = o.contours.clone();
                match &self.gesture {
                    Some(Gesture::Vertex {
                        edited: Some(list), ..
                    }) => {
                        for ((id, k), c) in list {
                            if *id == o.id && *k < contours.len() {
                                contours[*k] = c.clone();
                            }
                        }
                    }
                    Some(Gesture::Control {
                        id,
                        contour,
                        edited: Some(c),
                        ..
                    }) if *id == o.id && *contour < contours.len() => {
                        contours[*contour] = c.clone();
                    }
                    _ => {}
                }
                let mut own = selected
                    .iter()
                    .filter(|p| p.id == o.id)
                    .map(|p| (p.contour, p.index));
                PointOverlay {
                    id: o.id,
                    contours,
                    selected: own.next(),
                    also: own.collect(),
                }
            })
            .collect()
    }

    /// The overlay of the first shape whose points show ([`Self::point_overlays`]).
    pub fn point_overlay(&self, ctx: &ToolContext) -> Option<PointOverlay> {
        self.point_overlays(ctx).into_iter().next()
    }

    /// Where a point at `from` lands for a drag from `start` to `p`: on the grid with Snap to
    /// Grid on. With Shift (`axis_lock`) the drag runs only along the axis it mostly follows
    /// from its start, horizontally or vertically, and the point's other coordinate stays
    /// what it was (the grid does not move it either), as PE-DESIGN's does (the owner's hand
    /// check, 2026-11-19). Ctrl changes nothing.
    fn point_drag_target(&self, from: Vec2, start: Vec2, p: Vec2, axis_lock: bool) -> Vec2 {
        let d = move_delta(start, p, axis_lock);
        let mut to = self.snapped(from + d);
        if axis_lock {
            if d.x == 0.0 {
                to.x = from.x;
            }
            if d.y == 0.0 {
                to.y = from.y;
            }
        }
        to
    }

    fn point_tool(&mut self, e: PointerEvent, ctx: &ToolContext) -> Vec<ToolEffect> {
        if let Some(o) = ctx.needle_target() {
            return self.needle_tool(e, ctx, o);
        }
        if let Some(o) = ctx.rung_target() {
            return self.rung_tool(e, ctx, o);
        }
        if let Some(o) = ctx.satin_target() {
            return self.width_tool(e, ctx, o);
        }
        if let Some(o) = ctx.text_target() {
            return self.character_tool(e, ctx, o);
        }
        let p = e.location;
        match (e.phase, self.gesture.clone()) {
            (Phase::Down, _) => {
                self.given_up = None;
                // Any press drops the direction line's selection; one on its ends sets it.
                self.direction_end = None;
                let hit = self.point_hit(p, ctx);
                if !matches!(
                    hit,
                    Some((_, PointHit::Vertex { .. } | PointHit::Handle { .. }))
                ) && let Some(l) = self.direction_line(ctx)
                    && let Some((_, arrow)) = [(l.from, false), (l.to, true)]
                        .into_iter()
                        .find(|(q, _)| q.distance(p) <= 1.5 * ctx.point_radius())
                {
                    // An end of the direction line: the line is selected (its square turns
                    // black), and a drag turns the sewing direction.
                    self.point = None;
                    self.more_points.clear();
                    self.direction_end = Some(l.id);
                    self.gesture = Some(Gesture::DirectionEnd {
                        id: l.id,
                        arrow,
                        start: p,
                        edited: None,
                    });
                    return vec![];
                }
                match hit {
                    Some((o, PointHit::Vertex { contour, index })) => {
                        let point = PointRef {
                            id: o.id,
                            contour,
                            index,
                        };
                        // Ctrl+click or Shift+click (PE-DESIGN's gestures) adds the vertex to
                        // the selection (Ctrl+click takes a selected one out), on release; a drag with either key
                        // held still moves the vertex (Shift along one axis, Ctrl as without).
                        self.ctrl_press =
                            (e.free || e.shift).then(|| (point, self.selected_points(ctx), e.free));
                        // A press on a selected point keeps the whole selection (a drag moves
                        // them all); on another point it selects that one alone. (It used to
                        // keep the point clicked before selected as well, #1260.)
                        let selected = self.selected_points(ctx);
                        let rest: Vec<PointRef> = if selected.contains(&point) {
                            selected.into_iter().filter(|p| *p != point).collect()
                        } else {
                            Vec::new()
                        };
                        self.point = Some(point);
                        self.more_points = rest;
                        self.gesture = Some(Gesture::Vertex {
                            point,
                            start: p,
                            edited: None,
                        });
                        vec![]
                    }
                    Some((
                        o,
                        PointHit::Handle {
                            contour,
                            segment,
                            control,
                        },
                    )) => {
                        self.gesture = Some(Gesture::Control {
                            id: o.id,
                            contour,
                            segment,
                            control,
                            start: p,
                            edited: None,
                        });
                        vec![]
                    }
                    Some((
                        o,
                        PointHit::Segment {
                            contour,
                            segment,
                            t,
                        },
                    )) => {
                        // One click on a side inserts a point there and selects it, as
                        // PE-DESIGN's Select Point does; pressing and dragging inserts only
                        // (it has no segment drag). The second click of a double-click lands
                        // on the new point.
                        self.more_points.clear();
                        if e.clicks >= 2 || !(0.001..=0.999).contains(&t) {
                            self.point = None;
                            return vec![];
                        }
                        self.point = Some(PointRef {
                            id: o.id,
                            contour,
                            index: segment + 1,
                        });
                        vec![ToolEffect::run(
                            CommandRequest::new(
                                "object.insertPoint",
                                json!({ "id": o.id, "contour": contour, "segment": segment, "t": (t * 1000.0).round() / 1000.0 }),
                            ),
                            false,
                        )]
                    }
                    None => {
                        // A drag from here is a box over the points; a click selects the
                        // object under it or, on empty space, clears the selection (on release).
                        self.point = None;
                        self.more_points.clear();
                        self.gesture = Some(Gesture::PointBox { start: p });
                        vec![]
                    }
                }
            }
            (
                Phase::Dragged,
                Some(Gesture::DirectionEnd {
                    id, arrow, start, ..
                }),
            ) => {
                // The line turns about the middle of the shape: the dragged end points from
                // (the arrow head) or toward (the square) the middle. Whole tenths of a degree.
                let edited = self
                    .direction_base(ctx)
                    .filter(|_| p.distance(start) >= DRAG_THRESHOLD * ctx.units_per_point)
                    .map(|(_, centre, _)| {
                        let v = if arrow { p - centre } else { centre - p };
                        let a = (-v.y).atan2(v.x).to_degrees().rem_euclid(360.0);
                        ((a * 10.0).round() / 10.0).rem_euclid(360.0)
                    });
                self.gesture = Some(Gesture::DirectionEnd {
                    id,
                    arrow,
                    start,
                    edited,
                });
                vec![]
            }
            (
                Phase::Up,
                Some(Gesture::DirectionEnd {
                    id,
                    edited: Some(angle),
                    ..
                }),
            ) => {
                self.gesture = None;
                vec![ToolEffect::run(
                    CommandRequest::new("object.setRegion", json!({ "id": id, "angle": angle })),
                    false,
                )]
            }
            (Phase::Dragged, Some(Gesture::PointBox { start })) => {
                self.preview = (p.distance(start) >= DRAG_THRESHOLD * ctx.units_per_point)
                    .then(|| ToolPreview::Marquee(rect_from(start, p)));
                vec![]
            }
            (Phase::Up, Some(Gesture::PointBox { start })) => {
                self.gesture = None;
                if self.preview.take().is_none() {
                    return match object_at(ctx.design, p, ctx.tolerance()) {
                        Some(id) if ctx.selection.contains(&id) => vec![],
                        Some(id) => vec![ToolEffect::Select(BTreeSet::from([id]))],
                        None => vec![ToolEffect::Select(BTreeSet::new())],
                    };
                }
                // Every point inside the box, on every outline of every shape whose points
                // show (PE-DESIGN: "dragging a box over two shapes selects points of both
                // shapes", the owner's hand check, 2026-11-19).
                let r = rect_from(start, p);
                let mut picked = Vec::new();
                for o in ctx.point_targets() {
                    for (contour, c) in o.contours.iter().enumerate() {
                        for (index, q) in c.points.iter().enumerate() {
                            if (r.min_x..=r.max_x).contains(&q.x)
                                && (r.min_y..=r.max_y).contains(&q.y)
                            {
                                picked.push(PointRef {
                                    id: o.id,
                                    contour,
                                    index,
                                });
                            }
                        }
                    }
                }
                let mut picked = picked.into_iter();
                self.point = picked.next();
                self.more_points = picked.collect();
                vec![]
            }
            (Phase::Dragged, Some(Gesture::Vertex { point, start, .. })) => {
                let Some(from) = ctx
                    .point_contour(point)
                    .and_then(|c| c.points.get(point.index).copied())
                else {
                    return vec![];
                };
                let edited =
                    (p.distance(start) >= DRAG_THRESHOLD * ctx.units_per_point).then(|| {
                        let to = self.point_drag_target(from, start, p, e.shift);
                        // Dragging one of several selected points moves them all, on every
                        // outline and shape, by what the dragged one moves.
                        let d = to - from;
                        let mut out = Vec::new();
                        for (id, contour, indexes) in self.selected_by_outline(ctx) {
                            let Some(c) = ctx.point_contour(PointRef {
                                id,
                                contour,
                                index: 0,
                            }) else {
                                continue;
                            };
                            let mut moved = c.clone();
                            for i in indexes {
                                let here = id == point.id && contour == point.contour;
                                let at = if here && i == point.index {
                                    to
                                } else {
                                    moved.points[i] + d
                                };
                                moved.move_point(i, at);
                            }
                            out.push(((id, contour), moved));
                        }
                        out
                    });
                // Alt held: the dragged end point connects to an end it lies on.
                self.alt_drag = e.alt;
                self.connect_hover = edited
                    .as_ref()
                    .and_then(|list| list.first())
                    .and_then(|(_, c)| c.points.get(point.index).copied())
                    .filter(|_| e.alt && self.selected_points(ctx).len() == 1)
                    .and_then(|v| self.connect_target(point, v, ctx))
                    .map(|t| t.2);
                self.gesture = Some(Gesture::Vertex {
                    point,
                    start,
                    edited,
                });
                vec![]
            }
            (
                Phase::Dragged,
                Some(Gesture::Control {
                    id,
                    contour,
                    segment,
                    control,
                    start,
                    ..
                }),
            ) => {
                let Some(c) = ctx.point_contour(PointRef {
                    id,
                    contour,
                    index: 0,
                }) else {
                    return vec![];
                };
                // Shift or Ctrl held changes nothing for a handle (PE-DESIGN: "no diff", the
                // owner's hand check, 2026-11-19): no axis lock, the same snapping.
                let edited = (p.distance(start) >= DRAG_THRESHOLD * ctx.units_per_point)
                    .then(|| with_control(c, segment, control, self.snapped(p)));
                self.gesture = Some(Gesture::Control {
                    id,
                    contour,
                    segment,
                    control,
                    start,
                    edited,
                });
                vec![]
            }
            (Phase::Up, Some(g)) => {
                self.gesture = None;
                self.connect_hover = None;
                self.alt_drag = false;
                if let (Some((point, mut all, ctrl)), Gesture::Vertex { edited: None, .. }) =
                    (self.ctrl_press.take(), &g)
                {
                    // Ctrl+click on a selected point takes it out (PE-DESIGN's manual, read
                    // 2026-11-19); Shift+click only adds (the owner's hand check of the same
                    // day; whether it also takes out is not known yet).
                    match all.iter().position(|p| *p == point) {
                        Some(k) if ctrl => {
                            all.remove(k);
                        }
                        Some(_) => {}
                        None => all.insert(0, point),
                    }
                    self.point = all.first().copied();
                    self.more_points = all.into_iter().skip(1).collect();
                    return vec![];
                }
                match g {
                    Gesture::Vertex {
                        point,
                        edited: Some(list),
                        ..
                    } => {
                        let Some(v) = list
                            .first()
                            .and_then(|(_, c)| c.points.get(point.index).copied())
                        else {
                            return vec![];
                        };
                        let groups = self.selected_by_outline(ctx);
                        let total: usize = groups.iter().map(|g| g.2.len()).sum();
                        let from = ctx
                            .point_contour(point)
                            .and_then(|c| c.points.get(point.index).copied());
                        if let (true, Some(from)) = (total > 1, from) {
                            // One object.movePoints per outline, all in one undo step.
                            let requests = groups
                                .into_iter()
                                .map(|(id, contour, indexes)| {
                                    CommandRequest::new(
                                        "object.movePoints",
                                        json!({ "id": id, "contour": contour, "indexes": indexes, "dx": mm(v.x - from.x), "dy": mm(v.y - from.y) }),
                                    )
                                })
                                .collect();
                            return vec![ToolEffect::Execute {
                                requests,
                                select_result: false,
                            }];
                        }
                        let mut requests = vec![CommandRequest::new(
                            "object.setPoint",
                            json!({ "id": point.id, "contour": point.contour, "index": point.index, "x": mm(v.x), "y": mm(v.y) }),
                        )];
                        // Alt held on release: an end point dropped on the line's other end is
                        // connected to it (the line is closed as Close closes it, the two ends
                        // one point); dropped on an end of another open line, the two lines
                        // become one object.
                        if e.alt
                            && let Some(to) = self.connect_target(point, v, ctx)
                        {
                            requests = Self::connect_requests(point, to);
                            self.give_up_points();
                        }
                        vec![ToolEffect::Execute {
                            requests,
                            select_result: false,
                        }]
                    }
                    Gesture::Control {
                        id,
                        contour,
                        segment,
                        edited: Some(c),
                        ..
                    } => match c.curve(segment) {
                        Some(z) => vec![ToolEffect::run(
                            CommandRequest::new(
                                "object.setSegment",
                                json!({ "id": id, "contour": contour, "segment": segment, "curve": [[mm(z.c1.x), mm(z.c1.y)], [mm(z.c2.x), mm(z.c2.y)]] }),
                            ),
                            false,
                        )],
                        None => vec![],
                    },
                    _ => vec![],
                }
            }
            _ => vec![],
        }
    }

    /// The two ends of `o` when it is an open line the Point tool can connect: a plain shape
    /// (visible, not locked, not a Manual Punch column or a satin line) of one open outline.
    fn open_line_ends(o: &DesignObject) -> Option<[(usize, Vec2); 2]> {
        if o.kind != ObjectKind::Shape
            || !o.visible
            || o.locked
            || punch_rungs::rungs(o).is_some()
            || satin_width::is_satin_line(o)
        {
            return None;
        }
        let [c] = o.contours.as_slice() else {
            return None;
        };
        let n = c.points.len();
        (!c.closed && n >= 2).then(|| [(0, c.points[0]), (n - 1, c.points[n - 1])])
    }

    /// The end points `point` can be connected to with Alt + drag, as (object id, vertex
    /// index, position): PE-DESIGN marks each with an x while Alt is held (its manual, read
    /// 2026-11-19). `point` must be an end of an open line whose points show. They are the
    /// same line's other end (a line of three points or more: connecting closes it) and both
    /// ends of every other open line of the design, selected or not.
    pub fn connect_ends(&self, point: PointRef, ctx: &ToolContext) -> Vec<(u64, usize, Vec2)> {
        let Some(own) = ctx.point_target_of(point.id) else {
            return vec![];
        };
        let Some(ends) = Self::open_line_ends(own) else {
            return vec![];
        };
        let Some(other) = ends.iter().find(|e| e.0 != point.index) else {
            return vec![];
        };
        if !ends.iter().any(|e| e.0 == point.index) {
            return vec![];
        }
        let mut out = Vec::new();
        if own.contours[0].points.len() >= 3 {
            out.push((own.id, other.0, other.1));
        }
        for o in &ctx.design.objects {
            if o.id != own.id
                && let Some(ends) = Self::open_line_ends(o)
            {
                out.extend(ends.map(|(index, at)| (o.id, index, at)));
            }
        }
        out
    }

    /// Where an end point of an open line dropped at `at` with Alt held connects: the nearest
    /// of [`Self::connect_ends`] within the point radius.
    fn connect_target(
        &self,
        point: PointRef,
        at: Vec2,
        ctx: &ToolContext,
    ) -> Option<(u64, usize, Vec2)> {
        self.connect_ends(point, ctx)
            .into_iter()
            .filter(|e| e.2.distance(at) <= ctx.point_radius())
            .min_by(|a, b| a.2.distance(at).total_cmp(&b.2.distance(at)))
    }

    /// The commands that connect end point `point` to the end `to`: the point is put on it,
    /// then the line is closed as Close closes it (its own other end) or the two lines are
    /// joined into one object (`shape.joinLines`: the dragged line keeps its id, sewing
    /// attributes and place in the sewing order; the two ends become one point).
    fn connect_requests(point: PointRef, to: (u64, usize, Vec2)) -> Vec<CommandRequest> {
        vec![
            CommandRequest::new(
                "object.setPoint",
                json!({ "id": point.id, "contour": point.contour, "index": point.index, "x": mm(to.2.x), "y": mm(to.2.y) }),
            ),
            if to.0 == point.id {
                CommandRequest::new(
                    "object.closeOutline",
                    json!({ "id": point.id, "contour": point.contour }),
                )
            } else {
                CommandRequest::new(
                    "shape.joinLines",
                    json!({ "ids": [point.id, to.0], "tolerance": 0.1 }),
                )
            },
        ]
    }

    /// Connects the one selected end point to the end `to` (object id, vertex index), as
    /// Alt + drag onto it does: one undo step. `None`, changing nothing, unless exactly one
    /// point is selected and `to` is one of its [`Self::connect_ends`].
    pub fn connect_point(
        &mut self,
        to: (u64, usize),
        ctx: &ToolContext,
    ) -> Option<Vec<ToolEffect>> {
        if self.tool() != Tool::Point {
            return None;
        }
        let [point] = self.selected_points(ctx)[..] else {
            return None;
        };
        let target = self
            .connect_ends(point, ctx)
            .into_iter()
            .find(|e| (e.0, e.1) == to)?;
        self.give_up_points();
        Some(vec![ToolEffect::Execute {
            requests: Self::connect_requests(point, target),
            select_result: false,
        }])
    }

    /// Drops the point selection for a connect, remembering it for [`Self::restore_points`].
    fn give_up_points(&mut self) {
        self.given_up = self.point.map(|p| (p, self.more_points.clone()));
        self.point = None;
        self.more_points.clear();
    }

    /// Puts back the selection the last connect gave up: the app calls it when the connect's
    /// commands were refused, so a refused connect leaves the selection as it was. Nothing
    /// happens when no connect gave one up.
    pub fn restore_points(&mut self) {
        if let Some((point, more)) = self.given_up.take() {
            self.point = Some(point);
            self.more_points = more;
        }
    }

    /// The x marks to draw: every end the one selected end point can connect to, while Alt
    /// is held (`alt`) or an Alt + drag of it is running.
    pub fn connect_marks(&self, alt: bool, ctx: &ToolContext) -> Vec<Vec2> {
        if self.tool() != Tool::Point || !(alt || self.alt_drag) {
            return vec![];
        }
        let [point] = self.selected_points(ctx)[..] else {
            return vec![];
        };
        self.connect_ends(point, ctx)
            .into_iter()
            .map(|e| e.2)
            .collect()
    }

    /// The end the Alt-dragged point lies on now: a red square is drawn there, and releasing
    /// connects the two.
    pub fn connect_overlap(&self) -> Option<Vec2> {
        self.connect_hover
    }

    /// Arrow keys with the Point tool: moves the selected vertices by (`dx`, `dy`) (0.1 mm
    /// units) in one command. `None` when no vertex is selected (the arrows then nudge the
    /// selected objects as usual).
    pub fn nudge_points(&mut self, dx: f64, dy: f64, ctx: &ToolContext) -> Option<Vec<ToolEffect>> {
        if self.tool() != Tool::Point {
            return None;
        }
        let groups = self.selected_by_outline(ctx);
        if groups.is_empty() {
            return None;
        }
        // One object.movePoints per outline with selected points, all in one undo step.
        let requests = groups
            .into_iter()
            .map(|(id, contour, indexes)| {
                CommandRequest::new(
                    "object.movePoints",
                    json!({ "id": id, "contour": contour, "indexes": indexes, "dx": mm(dx), "dy": mm(dy) }),
                )
            })
            .collect();
        Some(vec![ToolEffect::Execute {
            requests,
            select_result: false,
        }])
    }

    /// Whether a right-click at `at` with the Point tool opens the object's menu (Add,
    /// Delete and Reverse Direction Line, Open, Close): on the shape whose points show, but
    /// not on one of its points (that opens the point menu).
    pub fn object_menu_at(&self, at: Vec2, ctx: &ToolContext) -> bool {
        if self.tool() != Tool::Point {
            return false;
        }
        let Some(o) = ctx.point_target() else {
            return false;
        };
        match self.point_hit(at, ctx) {
            Some((_, PointHit::Vertex { .. })) => false,
            Some(_) => true,
            None => object_at(ctx.design, at, ctx.tolerance()) == Some(o.id),
        }
    }

    /// Delete with the Point tool: removes the selected vertex. `None` when no vertex is
    /// selected (Delete then removes objects as usual).
    pub fn delete_point(&mut self, ctx: &ToolContext) -> Option<Vec<ToolEffect>> {
        if self.tool() != Tool::Point {
            return None;
        }
        // The shape's one direction line is selected: the last line cannot be deleted
        // (PE-DESIGN capture g), and the key does not fall through to deleting the shape.
        if self.direction_line(ctx).is_some_and(|l| l.selected) {
            return Some(vec![]);
        }
        if let Some(effects) = self.delete_needles(ctx) {
            return Some(effects);
        }
        if let Some(effects) = self.delete_rung(ctx) {
            return Some(effects);
        }
        if let Some(effects) = self.delete_width_key(ctx) {
            return Some(effects);
        }
        let groups = self.selected_by_outline(ctx);
        if groups.is_empty() {
            return None;
        }
        // One object.deletePoint per outline, all in one undo step. An outline that cannot
        // lose its selected points (it keeps 2) is left as it is and the others lose theirs.
        // When no outline can, nothing is deleted and the selection stays (PE-DESIGN's
        // Delete does nothing at two points), as with the menu's Delete.
        let requests: Vec<CommandRequest> = groups
            .into_iter()
            .filter(|(id, contour, indexes)| {
                ctx.point_contour(PointRef {
                    id: *id,
                    contour: *contour,
                    index: 0,
                })
                .is_some_and(|c| can_delete_points(c, indexes))
            })
            .map(|(id, contour, indexes)| {
                CommandRequest::new(
                    "object.deletePoint",
                    json!({ "id": id, "contour": contour, "indexes": indexes }),
                )
            })
            .collect();
        if requests.is_empty() {
            return Some(vec![]);
        }
        self.point = None;
        self.more_points.clear();
        Some(vec![ToolEffect::Execute {
            requests,
            select_result: false,
        }])
    }

    /// Point tool shortcut: makes the segment under the pointer a smooth curve (`curved`) or
    /// straight. Nothing happens when it already is, or no segment is near the pointer.
    pub fn set_segment_under_pointer(
        &mut self,
        curved: bool,
        ctx: &ToolContext,
    ) -> Vec<ToolEffect> {
        if self.tool() != Tool::Point || self.gesture.is_some() {
            return vec![];
        }
        let (Some(o), Some(p)) = (ctx.point_target(), self.hover) else {
            return vec![];
        };
        let Some((contour, segment)) = o
            .contours
            .iter()
            .enumerate()
            .filter_map(|(ci, c)| super::points::nearest_segment(c, p).map(|(s, _, d)| (ci, s, d)))
            .filter(|&(.., d)| d <= 2.0 * ctx.tolerance())
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(ci, s, _)| (ci, s))
        else {
            return vec![];
        };
        let c = &o.contours[contour];
        if c.curve(segment).is_some() == curved {
            return vec![];
        }
        let params = if curved {
            let z = smooth_curve(c, segment);
            json!({ "id": o.id, "contour": contour, "segment": segment, "curve": [[mm(z.c1.x), mm(z.c1.y)], [mm(z.c2.x), mm(z.c2.y)]] })
        } else {
            json!({ "id": o.id, "contour": contour, "segment": segment, "straight": true })
        };
        vec![ToolEffect::run(
            CommandRequest::new("object.setSegment", params),
            false,
        )]
    }

    // MARK: pen tool

    /// Anchor/handle pairs of the pen anchor being placed (both handles once dragged).
    pub fn pen_handles(&self) -> Vec<(Vec2, Vec2)> {
        let Some(n) = self.pen.last() else {
            return vec![];
        };
        [n.out, n.incoming()]
            .into_iter()
            .flatten()
            .map(|h| (n.at, h))
            .collect()
    }

    fn pen_preview(&mut self, cursor: Option<Vec2>) {
        let mut nodes = self.pen.clone();
        nodes.extend(cursor.map(PenNode::corner));
        self.preview =
            (!nodes.is_empty()).then(|| ToolPreview::Outline(pen_contour(&nodes, false)));
    }

    /// Click adds a corner, click-drag pulls out symmetric handles, clicking the first anchor
    /// closes the path.
    fn pen_tool(&mut self, e: PointerEvent, ctx: &ToolContext) -> Vec<ToolEffect> {
        let p = e.location;
        match (e.phase, self.gesture.clone()) {
            (Phase::Down, _) => {
                if self.pen.is_empty() && self.start_continuation(ctx, p) {
                    self.gesture = Some(Gesture::PenAnchor { index: 0 });
                    self.pen_preview(None);
                    return vec![];
                }
                if self.pen.len() >= 2
                    && self.continuing.is_none()
                    && p.distance(self.pen[0].at) <= 2.0 * ctx.tolerance()
                {
                    return self.finish_pen(true);
                }
                if self
                    .pen
                    .last()
                    .is_some_and(|last| last.at.distance(p) <= ctx.tolerance() / 2.0)
                {
                    return vec![];
                }
                self.pen.push(PenNode::corner(p));
                self.pen_point_placed();
                self.gesture = Some(Gesture::PenAnchor {
                    index: self.pen.len() - 1,
                });
                self.pen_preview(None);
                vec![]
            }
            (Phase::Dragged, Some(Gesture::PenAnchor { index })) => {
                if let Some(n) = self.pen.get_mut(index) {
                    n.out = (p.distance(n.at) >= DRAG_THRESHOLD * ctx.units_per_point).then_some(p);
                }
                self.pen_preview(None);
                vec![]
            }
            (Phase::Up, Some(Gesture::PenAnchor { .. })) => {
                self.gesture = None;
                vec![]
            }
            (Phase::Moved, _) if !self.pen.is_empty() => {
                self.pen_preview(Some(p));
                vec![]
            }
            _ => vec![],
        }
    }

    /// Ends the pen path as one `shape.path` (closed or open); too few anchors are discarded.
    pub fn finish_pen(&mut self, closed: bool) -> Vec<ToolEffect> {
        let nodes = std::mem::take(&mut self.pen);
        self.preview = None;
        self.gesture = None;
        if let Some(c) = self.continuing.filter(|_| self.tool() == Tool::Pen) {
            return self.extend_pen(c, &nodes);
        }
        if self.tool() != Tool::Pen || !is_valid(&nodes, closed) {
            return vec![];
        }
        if closed {
            self.satin_keys = None;
        } else {
            self.record_satin_keys(&nodes);
        }
        self.widths.clear();
        vec![ToolEffect::run(path_request(&nodes, closed), true)]
    }

    fn select_tool(&mut self, e: PointerEvent, ctx: &ToolContext) -> Vec<ToolEffect> {
        let p = e.location;
        match (e.phase, self.gesture.clone()) {
            (Phase::Down, _) => {
                if let Some((handles, h)) = ctx
                    .handles()
                    .and_then(|hs| hs.handle_at(p).map(|h| (hs, h)))
                {
                    self.gesture = Some(if h == Handle::Rotate {
                        Gesture::Rotating { start: p, handles }
                    } else {
                        Gesture::Scaling {
                            handle: h,
                            start: p,
                            handles,
                        }
                    });
                    return vec![];
                }
                let hit = object_at_opts(ctx.design, p, ctx.tolerance(), self.click_inside);
                self.gesture = Some(Gesture::Pending {
                    start: p,
                    hit,
                    shift: e.shift,
                });
                if e.alt
                    && let Some(o) = hit
                        .and_then(|id| ctx.design.index_of(id))
                        .map(|i| &ctx.design.objects[i])
                    && let Some(colour) = openstitch_engine::layout::colour_at(o, p)
                {
                    // PE-DESIGN: Alt-click on a stitch pattern's colour block selects every
                    // block of that colour.
                    let same = ctx
                        .design
                        .objects
                        .iter()
                        .filter(|o| {
                            o.visible && openstitch_engine::layout::colours(o).contains(&colour)
                        })
                        .map(|o| o.id)
                        .collect();
                    return vec![ToolEffect::Select(same)];
                }
                match hit {
                    Some(id) if e.shift => {
                        let mut s = ctx.selection.clone();
                        if !s.remove(&id) {
                            s.insert(id);
                        }
                        vec![ToolEffect::Select(s)]
                    }
                    Some(id) if !ctx.selection.contains(&id) => {
                        vec![ToolEffect::Select(BTreeSet::from([id]))]
                    }
                    Some(_) => vec![],
                    None if e.shift => vec![],
                    None => vec![ToolEffect::Select(BTreeSet::new())],
                }
            }
            (Phase::Dragged, Some(Gesture::Pending { start, hit, shift })) => {
                if p.distance(start) < DRAG_THRESHOLD * ctx.units_per_point {
                    return vec![];
                }
                let movable = !ctx.selection.is_empty() && !ctx.locked();
                self.gesture = Some(if hit.is_some() && !shift && movable {
                    self.snap_lines = if self.snap_objects {
                        Some(SnapLines::collect(ctx.design, ctx.selection))
                    } else {
                        self.snap_guides
                            .then(|| SnapLines::guides(ctx.design))
                            .filter(|l| !l.xs.is_empty() || !l.ys.is_empty())
                    };
                    Gesture::Moving { start }
                } else {
                    Gesture::Marquee {
                        start,
                        shift,
                        base: if shift {
                            ctx.selection.clone()
                        } else {
                            BTreeSet::new()
                        },
                    }
                });
                self.select_tool(e, ctx)
            }
            (Phase::Dragged, Some(Gesture::Moving { start })) => {
                let (d, guides) = self.move_by(start, &e, ctx);
                self.guides = guides;
                self.preview = Some(ToolPreview::Transform(Transform::translation(d.x, d.y)));
                vec![]
            }
            (
                Phase::Dragged,
                Some(Gesture::Scaling {
                    handle,
                    start,
                    handles,
                }),
            ) => {
                let s = handles.scale(handle, start, p, e.shift);
                self.preview = Some(ToolPreview::Transform(Transform::scaling(
                    s.sx, s.sy, s.origin,
                )));
                vec![]
            }
            (Phase::Dragged, Some(Gesture::Rotating { start, handles })) => {
                let deg = handles.rotation(start, p, e.shift);
                self.preview = Some(ToolPreview::Transform(Transform::rotation(
                    deg,
                    handles.bounds.center(),
                )));
                vec![]
            }
            (Phase::Dragged, Some(Gesture::Marquee { start, shift, base })) => {
                let r = rect_from(start, p);
                self.preview = Some(ToolPreview::Marquee(r));
                let inside = objects_in(ctx.design, &r);
                let sel = if shift {
                    base.symmetric_difference(&inside).copied().collect()
                } else {
                    inside
                };
                vec![ToolEffect::Select(sel)]
            }
            (Phase::Up, Some(g)) => {
                self.gesture = None;
                self.preview = None;
                let moved = match g {
                    Gesture::Moving { start } => Some(self.move_by(start, &e, ctx).0),
                    _ => None,
                };
                self.snap_lines = None;
                self.guides.clear();
                finish_select(g, e, ctx, moved)
            }
            _ => vec![],
        }
    }

    fn drag_shape_tool(&mut self, e: PointerEvent) -> Vec<ToolEffect> {
        let ellipse = self.tool() == Tool::Ellipse;
        match (e.phase, self.gesture.clone()) {
            (Phase::Down, _) => {
                self.gesture = Some(Gesture::Drawing { start: e.location });
                self.preview = None;
                vec![]
            }
            (Phase::Dragged, Some(Gesture::Drawing { start })) => {
                self.preview = Some(ToolPreview::Outline(outline(
                    drag_rect(start, e.location, e.shift),
                    ellipse,
                )));
                vec![]
            }
            (Phase::Up, Some(Gesture::Drawing { start })) => {
                self.gesture = None;
                self.preview = None;
                let r = drag_rect(start, e.location, e.shift);
                if r.width() < MINIMUM_SIZE || r.height() < MINIMUM_SIZE {
                    return vec![];
                }
                let c = r.center();
                let request = if ellipse {
                    CommandRequest::new(
                        "shape.ellipse",
                        json!({ "cx": mm(c.x), "cy": mm(c.y), "rx": mm(r.width() / 2.0), "ry": mm(r.height() / 2.0), "name": "Ellipse" }),
                    )
                } else {
                    let mut params = json!({ "x": mm(r.min_x), "y": mm(r.min_y), "width": mm(r.width()), "height": mm(r.height()), "name": "Rectangle" });
                    if let Some(radii) = self.corner_radii {
                        params["cornerRadii"] = json!(radii);
                    } else if self.corner_radius > 0.0 {
                        params["cornerRadius"] = json!(self.corner_radius);
                    }
                    CommandRequest::new("shape.rectangle", params)
                };
                vec![ToolEffect::run(request, true)]
            }
            _ => vec![],
        }
    }

    /// Click to add vertices; click the first vertex, double-click or press Return to close
    /// (Open Straight Line: double-click or Return ends the open line).
    fn polygon_tool(&mut self, e: PointerEvent, ctx: &ToolContext) -> Vec<ToolEffect> {
        match e.phase {
            Phase::Down => {
                if self.polygon.is_empty()
                    && self.tool() == Tool::Line
                    && self.start_continuation(ctx, e.location)
                {
                    self.preview = Some(ToolPreview::Outline(Contour::new(
                        self.polygon.clone(),
                        false,
                    )));
                    return vec![];
                }
                let closes_on_first = self.tool() == Tool::Polygon
                    && self.polygon.len() >= 3
                    && e.location.distance(self.polygon[0]) <= 2.0 * ctx.tolerance();
                if e.clicks >= 2 || closes_on_first {
                    return self.finish_polygon();
                }
                if self
                    .polygon
                    .last()
                    .is_some_and(|last| last.distance(e.location) <= ctx.tolerance() / 2.0)
                {
                    return vec![];
                }
                self.polygon.push(e.location);
                self.preview = Some(ToolPreview::Outline(Contour::new(
                    self.polygon.clone(),
                    false,
                )));
                vec![]
            }
            Phase::Moved | Phase::Dragged => {
                if !self.polygon.is_empty() {
                    let mut pts = self.polygon.clone();
                    pts.push(e.location);
                    self.preview = Some(ToolPreview::Outline(Contour::new(pts, false)));
                }
                vec![]
            }
            Phase::Up => vec![],
        }
    }

    /// Manual Punch: each click is the next end of a direction line (left, then right);
    /// double-click or Return finishes. The preview walks each line and the side to the next.
    fn punch_tool(&mut self, e: PointerEvent, ctx: &ToolContext) -> Vec<ToolEffect> {
        match e.phase {
            Phase::Down => {
                if e.clicks >= 2 {
                    return self.finish_punch();
                }
                if self.punch.is_empty()
                    && self.continuing.is_none()
                    && matches!(self.tool(), Tool::Running | Tool::Punch | Tool::CurvedBlock)
                    && self.start_continuation(ctx, e.location)
                {
                    self.preview = Some(ToolPreview::Outline(self.punch_preview(None)));
                    return vec![];
                }
                if self
                    .punch
                    .last()
                    .is_some_and(|last| last.distance(e.location) <= ctx.tolerance() / 2.0)
                {
                    return vec![];
                }
                self.punch.push(e.location);
                self.preview = Some(ToolPreview::Outline(self.punch_preview(None)));
                vec![]
            }
            Phase::Moved | Phase::Dragged => {
                if !self.punch.is_empty() {
                    self.preview = Some(ToolPreview::Outline(self.punch_preview(Some(e.location))));
                }
                vec![]
            }
            Phase::Up => vec![],
        }
    }

    pub fn punch(&self) -> &[Vec2] {
        &self.punch
    }

    /// Blocks show the ladder of direction lines; Running and Feed the line through the points.
    fn punch_preview(&self, hover: Option<Vec2>) -> Contour {
        match self.tool() {
            Tool::Running | Tool::Feed => {
                let mut pts = self.punch.clone();
                pts.extend(hover);
                Contour::new(pts, false)
            }
            _ => ladder(&self.punch, hover),
        }
    }

    /// Ends the Manual Punch shape: complete line pairs become one `shape.column` (curved for
    /// Curved Block); an unpaired last click is dropped; fewer than two lines make nothing.
    /// Running makes an open running line, Feed a `shape.feed`; both need two points.
    pub fn finish_punch(&mut self) -> Vec<ToolEffect> {
        let pts = std::mem::take(&mut self.punch);
        self.preview = None;
        if let Some(c) = self.continuing {
            return match self.tool() {
                Tool::Running => self.extend_line(c, &pts),
                Tool::Punch | Tool::CurvedBlock => self.extend_column(c, &pts),
                _ => {
                    self.continuing = None;
                    vec![]
                }
            };
        }
        let points = || -> Vec<serde_json::Value> {
            pts.iter().map(|p| json!([mm(p.x), mm(p.y)])).collect()
        };
        match self.tool() {
            Tool::Running if pts.len() >= 2 => {
                return vec![ToolEffect::run(
                    CommandRequest::new(
                        "shape.polygon",
                        json!({ "points": points(), "closed": false, "name": "Running" }),
                    ),
                    true,
                )];
            }
            Tool::Feed if pts.len() >= 2 => {
                return vec![ToolEffect::run(
                    CommandRequest::new("shape.feed", json!({ "points": points() })),
                    true,
                )];
            }
            Tool::Punch | Tool::CurvedBlock if pts.len() >= 4 => {}
            _ => return vec![],
        }
        let lines: Vec<_> = pts
            .as_chunks::<2>()
            .0
            .iter()
            .map(|w| json!([[mm(w[0].x), mm(w[0].y)], [mm(w[1].x), mm(w[1].y)]]))
            .collect();
        let mut params = json!({ "lines": lines });
        if self.tool() == Tool::CurvedBlock {
            params["curved"] = json!(true);
        }
        vec![ToolEffect::run(
            CommandRequest::new("shape.column", params),
            true,
        )]
    }

    /// Closes the polygon in progress (Return). Fewer than three vertices are discarded; an
    /// open line needs two.
    pub fn finish_polygon(&mut self) -> Vec<ToolEffect> {
        let pts = std::mem::take(&mut self.polygon);
        self.preview = None;
        let closed = match self.tool() {
            Tool::Polygon => true,
            Tool::Line => false,
            _ => return vec![],
        };
        if let Some(c) = self.continuing.filter(|_| !closed) {
            return self.extend_line(c, &pts);
        }
        if pts.len() < if closed { 3 } else { 2 } {
            return vec![];
        }
        let points: Vec<_> = pts.iter().map(|p| json!([mm(p.x), mm(p.y)])).collect();
        let name = if closed { "Polygon" } else { "Line" };
        vec![ToolEffect::run(
            CommandRequest::new(
                "shape.polygon",
                json!({ "points": points, "closed": closed, "name": name }),
            ),
            true,
        )]
    }

    /// Pencil: press and drag to trace; releasing ends the stroke as one `shape.freehand`,
    /// simplified to within about a screen point and a half.
    fn pencil_tool(&mut self, e: PointerEvent, ctx: &ToolContext) -> Vec<ToolEffect> {
        let p = e.location;
        match e.phase {
            Phase::Down => {
                self.pencil = vec![p];
                self.preview = None;
                vec![]
            }
            Phase::Dragged if !self.pencil.is_empty() => {
                if self
                    .pencil
                    .last()
                    .is_some_and(|q| q.distance(p) >= ctx.units_per_point)
                    && self.pencil.len() < openstitch_design::freehand::MAX_POINTS
                {
                    self.pencil.push(p);
                }
                self.preview = Some(ToolPreview::Outline(Contour::new(
                    self.pencil.clone(),
                    false,
                )));
                vec![]
            }
            Phase::Up if !self.pencil.is_empty() => {
                let mut trail = std::mem::take(&mut self.pencil);
                self.preview = None;
                if trail.last() != Some(&p) && trail.len() < openstitch_design::freehand::MAX_POINTS
                {
                    trail.push(p);
                }
                let closed = self.tool() == Tool::Pencil;
                let size = Rect::enclosing(trail.iter().copied())
                    .map_or(0.0, |r| r.width().max(r.height()));
                if size < MINIMUM_SIZE {
                    return vec![];
                }
                let tolerance = (1.5 * ctx.units_per_point / 10.0).clamp(0.05, 2.0);
                let points: Vec<_> = trail.iter().map(|q| json!([mm(q.x), mm(q.y)])).collect();
                let name = if closed { "Freehand" } else { "Freehand line" };
                vec![ToolEffect::run(
                    CommandRequest::new(
                        "shape.freehand",
                        json!({ "points": points, "closed": closed, "tolerance": (tolerance * 100.0).round() / 100.0, "name": name }),
                    ),
                    true,
                )]
            }
            _ => vec![],
        }
    }

    /// The Pencil trail being drawn.
    pub fn pencil(&self) -> &[Vec2] {
        &self.pencil
    }
}

/// Manual Punch preview: each direction line, then along the side to the next line's same end
/// (so the path zigzags left-right, right-left), plus the line to the pointer.
fn ladder(clicks: &[Vec2], hover: Option<Vec2>) -> Contour {
    let mut pts: Vec<Vec2> = Vec::new();
    for (i, pair) in clicks.chunks(2).enumerate() {
        if pair.len() == 2 && i % 2 == 1 {
            pts.extend([pair[1], pair[0]]);
        } else {
            pts.extend(pair.iter().copied());
        }
    }
    pts.extend(hover);
    Contour::new(pts, false)
}

fn move_delta(start: Vec2, p: Vec2, axis_lock: bool) -> Vec2 {
    let d = p - start;
    if !axis_lock {
        d
    } else if d.x.abs() >= d.y.abs() {
        v2(d.x, 0.0)
    } else {
        v2(0.0, d.y)
    }
}

/// `p` rounded to the nearest crossing of a grid of `step` from the page origin.
pub fn snap_to_grid(p: Vec2, step: f64) -> Vec2 {
    v2((p.x / step).round() * step, (p.y / step).round() * step)
}

/// Ends a Select-tool gesture; `moved` is the (snapped) move of a Moving gesture.
fn finish_select(
    g: Gesture,
    e: PointerEvent,
    ctx: &ToolContext,
    moved: Option<Vec2>,
) -> Vec<ToolEffect> {
    let p = e.location;
    let sel = ctx.selection;
    match g {
        Gesture::Moving { start } => {
            let d = moved.unwrap_or_else(|| move_delta(start, p, e.shift));
            if mm(d.x) == 0.0 && mm(d.y) == 0.0 {
                return vec![];
            }
            vec![ToolEffect::run(
                CommandRequest::move_by(sel, d.x, d.y),
                false,
            )]
        }
        Gesture::Scaling {
            handle,
            start,
            handles,
        } => {
            let s = handles.scale(handle, start, p, e.shift);
            if (s.sx - 1.0).abs() < 1e-4 && (s.sy - 1.0).abs() < 1e-4 {
                return vec![];
            }
            let requests = CommandRequest::scale_about(
                sel,
                s.sx,
                s.sy,
                s.origin,
                handles.bounds,
                ctx.scale_takes_origin,
            );
            vec![ToolEffect::Execute {
                requests,
                select_result: false,
            }]
        }
        Gesture::Rotating { start, handles } => {
            let deg = handles.rotation(start, p, e.shift);
            if deg.abs() < 0.01 {
                return vec![];
            }
            vec![ToolEffect::run(CommandRequest::rotate(sel, deg), false)]
        }
        Gesture::Pending { .. }
        | Gesture::Marquee { .. }
        | Gesture::Drawing { .. }
        | Gesture::Vertex { .. }
        | Gesture::PointBox { .. }
        | Gesture::DirectionEnd { .. }
        | Gesture::Control { .. }
        | Gesture::PenAnchor { .. }
        | Gesture::SkeletonWidth { .. }
        | Gesture::SkeletonPoint { .. }
        | Gesture::Character { .. }
        | Gesture::Needles { .. }
        | Gesture::Rung { .. }
        | Gesture::Width { .. } => vec![],
    }
}

fn drag_rect(start: Vec2, p: Vec2, square: bool) -> Rect {
    if !square {
        return rect_from(start, p);
    }
    let side = (p.x - start.x).abs().max((p.y - start.y).abs());
    let end = v2(
        start.x + if p.x < start.x { -side } else { side },
        start.y + if p.y < start.y { -side } else { side },
    );
    rect_from(start, end)
}

fn outline(r: Rect, ellipse: bool) -> Contour {
    if ellipse {
        Contour::ellipse(r.center(), r.width() / 2.0, r.height() / 2.0, 72)
    } else {
        Contour::rectangle(r.min_x, r.min_y, r.width(), r.height())
    }
}

#[cfg(test)]
mod punch_tests {
    use super::*;

    fn click(t: &mut ToolController, ctx: &ToolContext, at: Vec2, clicks: u32) -> Vec<ToolEffect> {
        let mut e = PointerEvent::new(Phase::Down, at);
        e.clicks = clicks;
        let mut out = t.handle(e, ctx);
        out.extend(t.handle(PointerEvent::new(Phase::Up, at), ctx));
        out
    }

    #[test]
    fn punch_clicks_make_one_column() {
        let design = Design::default();
        let sel = BTreeSet::new();
        let ctx = ToolContext {
            design: &design,
            selection: &sel,
            units_per_point: 1.0,
            scale_takes_origin: false,
        };
        let mut t = ToolController::default();
        t.set_tool(Tool::Punch);
        assert_eq!(Tool::from_shortcut('U'), Some(Tool::Punch));
        for p in [
            v2(100.0, 100.0),
            v2(100.0, 200.0),
            v2(400.0, 100.0),
            v2(350.0, 200.0),
            v2(500.0, 350.0),
        ] {
            assert!(click(&mut t, &ctx, p, 1).is_empty());
        }
        assert!(t.is_busy());
        // Preview: line 0 down, side across, line 1 up, then the pending click.
        let Some(ToolPreview::Outline(c)) = t.preview() else {
            panic!("preview")
        };
        assert_eq!(c.points[2], v2(350.0, 200.0));
        assert_eq!(c.points[3], v2(400.0, 100.0));
        let effects = click(&mut t, &ctx, v2(400.0, 350.0), 1);
        assert!(effects.is_empty());
        let effects = click(&mut t, &ctx, v2(400.0, 350.0), 2);
        let [
            ToolEffect::Execute {
                requests,
                select_result,
            },
        ] = effects.as_slice()
        else {
            panic!("one command")
        };
        assert!(*select_result);
        assert_eq!(requests[0].command, "shape.column");
        assert_eq!(
            requests[0].params["lines"],
            json!([
                [[10.0, 10.0], [10.0, 20.0]],
                [[40.0, 10.0], [35.0, 20.0]],
                [[50.0, 35.0], [40.0, 35.0]]
            ])
        );
        let mut s = openstitch_engine::Session::new(std::env::temp_dir());
        assert!(
            s.execute(requests[0].command, Some(&requests[0].params))
                .is_ok()
        );
        assert!(!t.is_busy());
    }

    #[test]
    fn punch_needs_two_lines_and_escape_cancels() {
        let design = Design::default();
        let sel = BTreeSet::new();
        let ctx = ToolContext {
            design: &design,
            selection: &sel,
            units_per_point: 1.0,
            scale_takes_origin: false,
        };
        let mut t = ToolController::default();
        t.set_tool(Tool::Punch);
        for p in [v2(0.0, 0.0), v2(0.0, 100.0), v2(100.0, 0.0)] {
            click(&mut t, &ctx, p, 1);
        }
        assert!(t.finish().is_empty(), "one line and a half is not a column");
        for p in [v2(0.0, 0.0), v2(0.0, 100.0)] {
            click(&mut t, &ctx, p, 1);
        }
        t.cancel();
        assert!(!t.is_busy() && t.punch().is_empty());
    }
}
