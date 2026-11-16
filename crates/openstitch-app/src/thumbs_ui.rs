//! The Sewing Order list's small pictures as textures (#1185): each object drawn from its own
//! stitches (`openstitch_app::editor::thumbs`), cached per object and redrawn only when its
//! stitches, colours or the display scale change.
//!
//! - Only rows on screen are drawn, and drawing stops for the frame after [`BUDGET`] of work, so
//!   a long list never holds up typing or dragging. A row waiting for its picture shows the one
//!   it had, or its outlines drawn the quick way, and the next frame carries on.
//! - The cache holds at most [`CAP_BYTES`] of pictures, dropping the least recently shown first,
//!   so tens of thousands of objects or several open designs cannot grow it without limit. A
//!   dropped picture is drawn again when its row comes back on screen.
//! - A `--screenshot` run draws every picture at once, so no screenshot shows a waiting row.

use eframe::egui::{self, Color32, Sense, Stroke, vec2};
use openstitch_app::editor::thumbs::{self, Source};
use openstitch_app::platform::Instant;
use openstitch_core::StitchPattern;
use openstitch_design::DesignObject;
use std::collections::HashMap;
use std::ops::Range;
use std::time::Duration;

/// Drawing time allowed per frame, all pictures together.
pub const BUDGET: Duration = Duration::from_millis(4);
/// Most picture bytes kept (RGBA): about 3,100 rows' pictures at 52 px (a 30 pt tile at 2x).
pub const CAP_BYTES: usize = 32 << 20;
/// The tile behind a picture whose colours are all near white.
const DARK_TILE: Color32 = Color32::from_gray(110);
/// Space between a tile's edge and its picture, points.
const INSET: f32 = 2.0;

struct Entry {
    /// [`Source::key`] of the picture.
    key: u64,
    /// The design revision the key was last checked at.
    revision: u64,
    texture: egui::TextureHandle,
    dark: bool,
    /// The pass it was last shown in (least recently shown goes first).
    shown: u64,
    bytes: usize,
}

pub struct ThumbCache {
    /// The design revision `ranges` and `changes` are for.
    revision: Option<u64>,
    ranges: HashMap<u64, Vec<Range<usize>>>,
    changes: Vec<usize>,
    entries: HashMap<u64, Entry>,
    bytes: usize,
    pass: u64,
    spent: Duration,
    /// Draw every picture at once (a `--screenshot` run).
    pub unlimited: bool,
    /// Pictures drawn, for the tests.
    pub drawn: usize,
    /// Time spent on thumbnail work in all, for the timing test.
    pub work: Duration,
    /// Most picture bytes kept: [`CAP_BYTES`].
    cap: usize,
    /// The drawing before #1185 (every row's outlines on every frame), for the timing test.
    #[cfg(test)]
    legacy: bool,
}

impl Default for ThumbCache {
    fn default() -> Self {
        ThumbCache {
            revision: None,
            ranges: HashMap::new(),
            changes: Vec::new(),
            entries: HashMap::new(),
            bytes: 0,
            pass: 0,
            spent: Duration::ZERO,
            unlimited: false,
            drawn: 0,
            work: Duration::ZERO,
            cap: CAP_BYTES,
            #[cfg(test)]
            legacy: false,
        }
    }
}

/// The design state a picture is cut from.
pub struct Compiled<'a> {
    pub pattern: &'a StitchPattern,
    pub spans: &'a [(u64, Range<usize>)],
    pub revision: u64,
}

impl ThumbCache {
    fn begin(&mut self, ctx: &egui::Context, c: &Compiled) {
        let pass = ctx.cumulative_pass_nr();
        if pass != self.pass {
            self.pass = pass;
            self.spent = Duration::ZERO;
        }
        if self.revision != Some(c.revision) {
            self.revision = Some(c.revision);
            self.ranges = thumbs::ranges_by_object(c.spans);
            self.changes = thumbs::colour_changes(c.pattern);
        }
    }

    fn over_budget(&self) -> bool {
        !self.unlimited && self.spent >= BUDGET
    }

    /// The texture and tile shade for `o` at `px` pixels, drawing it if it changed and the
    /// frame's budget allows; `None` while it waits.
    fn picture(
        &mut self,
        ctx: &egui::Context,
        c: &Compiled,
        o: &DesignObject,
        px: u32,
    ) -> Option<(egui::TextureId, bool)> {
        let pass = self.pass;
        if let Some(e) = self.entries.get_mut(&o.id)
            && e.revision == c.revision
            && e.texture.size() == [px as usize; 2]
        {
            e.shown = pass;
            return Some((e.texture.id(), e.dark));
        }
        if self.over_budget() {
            return self.stale(o.id);
        }
        let start = Instant::now();
        let ranges = self.ranges.get(&o.id).map_or(&[][..], Vec::as_slice);
        let src = Source::of(o, thumbs::object_pattern(c.pattern, &self.changes, ranges));
        let key = src.key(o, px);
        if let Some(e) = self.entries.get_mut(&o.id)
            && e.key == key
        {
            e.revision = c.revision;
            e.shown = pass;
            self.spent += start.elapsed();
            self.work += start.elapsed();
            return Some((e.texture.id(), e.dark));
        }
        let rgba = src.draw(o, px);
        let result = rgba.map(|rgba| {
            let image = egui::ColorImage::from_rgba_unmultiplied([px as usize; 2], &rgba);
            let texture = ctx.load_texture(
                format!("order-thumb-{}", o.id),
                image,
                egui::TextureOptions::LINEAR,
            );
            let dark = thumbs::needs_dark_tile(&src.colours());
            let id = texture.id();
            let bytes = rgba.len();
            if let Some(old) = self.entries.insert(
                o.id,
                Entry {
                    key,
                    revision: c.revision,
                    texture,
                    dark,
                    shown: pass,
                    bytes,
                },
            ) {
                self.bytes -= old.bytes;
            }
            self.bytes += bytes;
            self.drawn += 1;
            self.evict();
            (id, dark)
        });
        self.spent += start.elapsed();
        self.work += start.elapsed();
        result
    }

    /// The picture a row had before its object changed, while the new one waits.
    fn stale(&mut self, id: u64) -> Option<(egui::TextureId, bool)> {
        let pass = self.pass;
        self.entries.get_mut(&id).map(|e| {
            e.shown = pass;
            (e.texture.id(), e.dark)
        })
    }

    /// Drops the least recently shown pictures until the cache is within its cap.
    fn evict(&mut self) {
        while self.bytes > self.cap {
            let Some((&id, _)) = self
                .entries
                .iter()
                .filter(|(_, e)| e.shown != self.pass)
                .min_by_key(|(_, e)| e.shown)
            else {
                return;
            };
            if let Some(e) = self.entries.remove(&id) {
                self.bytes -= e.bytes;
            }
        }
    }

    /// Pictures held, and their bytes.
    #[cfg(test)]
    pub fn held(&self) -> (usize, usize) {
        (self.entries.len(), self.bytes)
    }
}

/// A small picture of `o`, `size` points square, as the page shows it: its own stitches in its
/// threads, or its outlines filled when it has no stitches to show (#1185).
pub fn thumbnail(
    ui: &mut egui::Ui,
    cache: &mut ThumbCache,
    compiled: &Compiled,
    o: &DesignObject,
    size: f32,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    #[cfg(test)]
    if cache.legacy {
        let start = Instant::now();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, Color32::WHITE);
        quick_outlines(&painter, rect, o);
        cache.work += start.elapsed();
        return resp;
    }
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    cache.begin(ui.ctx(), compiled);
    // The picture sits 2 pt inside its tile, so the tile shows round it: a white object on its
    // grey tile reads as a shape, not as an empty tile.
    let inner = rect.shrink(INSET);
    let px = (inner.width() * ui.ctx().pixels_per_point()).round().max(1.0) as u32;
    let picture = cache.picture(ui.ctx(), compiled, o, px);
    let painter = ui.painter_at(rect);
    let dark = picture.is_some_and(|(_, d)| d);
    painter.rect_filled(rect, 2.0, if dark { DARK_TILE } else { Color32::WHITE });
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    match picture {
        Some((texture, _)) => {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(texture, inner, uv, Color32::WHITE);
        }
        None => {
            quick_outlines(&painter, rect, o);
            ui.ctx().request_repaint();
        }
    }
    resp
}

/// A row's outlines drawn straight away while its picture waits for the next frame: closed
/// contours in the fill colour and the line colour, open ones in the line colour.
fn quick_outlines(painter: &egui::Painter, rect: egui::Rect, o: &DesignObject) {
    let Some(b) = o.bounds() else { return };
    let inner = rect.shrink(3.0);
    let k = (f64::from(inner.width()) / b.width().max(1.0))
        .min(f64::from(inner.height()) / b.height().max(1.0));
    let ox = f64::from(inner.center().x) - (b.min_x + b.width() / 2.0) * k;
    let oy = f64::from(inner.center().y) - (b.min_y + b.height() / 2.0) * k;
    let at = |x: f64, y: f64| egui::pos2((x * k + ox) as f32, (y * k + oy) as f32);
    // bare-swatch: the object's outlines in its thread colours, not a colour list.
    let rgb = |t: &openstitch_core::Thread| Color32::from_rgb(t.red, t.green, t.blue);
    if let Some(block) = &o.stitch_block {
        // A stitch object: every n-th needle point in its first thread.
        let step = (block.stitches.len() / 400).max(1);
        let color = block.threads.first().map_or(Color32::GRAY, rgb);
        let pts: Vec<egui::Pos2> = block
            .stitches
            .iter()
            .step_by(step)
            .map(|s| at(s.x, s.y))
            .collect();
        painter.add(egui::Shape::line(pts, Stroke::new(1.0, color)));
    }
    let fill = o.region.as_ref().map(|r| rgb(&r.thread));
    let [r, g, bl] = thumbs::NO_LINE;
    let line = o
        .line
        .as_ref()
        .map_or(Color32::from_rgb(r, g, bl), |l| rgb(&l.thread));
    for c in &o.contours {
        let pts: Vec<egui::Pos2> = c.points.iter().map(|p| at(p.x, p.y)).collect();
        if pts.len() < 2 {
            continue;
        }
        if c.closed {
            if let Some(f) = fill {
                painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(2.5, f)));
            }
            painter.add(egui::Shape::closed_line(pts, Stroke::new(1.0, line)));
        } else {
            painter.add(egui::Shape::line(pts, Stroke::new(1.0, line)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{EditorApp, Launch};
    use openstitch_app::editor::CommandRequest;
    use openstitch_core::Thread;
    use openstitch_design::{
        Contour, Design, DesignObject, LineSewing, LineType, RegionSewing, RegionType,
    };

    /// `n` small squares on the page, a few of them line-only or hidden, as a long list.
    fn design(n: usize) -> Design {
        let mut d = Design::default();
        for k in 0..n {
            let (x, y) = ((k % 40) as f64 * 25.0, (k / 40) as f64 * 25.0);
            let thread = Thread::rgb(0x10_20_30 * (k as u32 % 7 + 1));
            let mut o = DesignObject::shape(
                vec![Contour::rectangle(x, y, 20.0, 20.0)],
                (k % 5 != 0).then(|| RegionSewing::new(RegionType::Fill, thread.clone())),
                (k % 5 == 0).then(|| LineSewing::new(LineType::Running, thread)),
            );
            o.visible = k % 10 != 3;
            d.add(o);
        }
        d
    }

    fn app_with(n: usize) -> EditorApp {
        let mut app = EditorApp::new(Launch::default());
        app.session.adopt_design(design(n));
        app.refresh();
        app
    }

    /// One frame of the Sewing Order list in a 320 x 900 pt window; the time it took.
    fn list_frame(ctx: &egui::Context, app: &mut EditorApp) -> Duration {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                vec2(320.0, 900.0),
            )),
            ..Default::default()
        };
        let start = Instant::now();
        let mut out = ctx.run_ui(raw, |ui| crate::panels::object_list(app, ui));
        let took = start.elapsed();
        out.textures_delta.clear();
        took
    }

    fn median(mut v: Vec<Duration>) -> Duration {
        v.sort();
        v[v.len() / 2]
    }

    fn ms(d: Duration) -> f64 {
        d.as_secs_f64() * 1e3
    }

    /// #1185's measurement: the list of 1,000 objects before (every row's outlines on every
    /// frame) and after (pictures cached, only rows on screen drawn, within the budget), on a
    /// still frame and with one object moved every frame (a recompile each frame, harsher than
    /// a drag, which changes nothing until it ends). Printed with `--nocapture`.
    #[test]
    fn a_list_of_1000_objects_stays_within_the_budget() {
        // Each frame: the whole frame's time and the thumbnails' share of it.
        fn run(app: &mut EditorApp, ctx: &egui::Context, n: usize, moving: bool) -> (Vec<Duration>, Vec<Duration>) {
            let id = app.session.design().objects[0].id;
            let (mut frames, mut work) = (Vec::new(), Vec::new());
            for _ in 0..n {
                if moving {
                    app.execute(&CommandRequest::new(
                        "object.move",
                        serde_json::json!({ "ids": [id], "dx": 0.1, "dy": 0.0 }),
                    ));
                }
                let w = app.thumbs.work;
                frames.push(list_frame(ctx, app));
                work.push(app.thumbs.work - w);
            }
            (frames, work)
        }
        fn max(v: &[Duration]) -> Duration {
            v.iter().copied().max().unwrap_or_default()
        }

        // Before: every row's outlines on every frame. One frame first, for egui's own start-up.
        let ctx = egui::Context::default();
        let mut app = app_with(1000);
        app.thumbs.legacy = true;
        list_frame(&ctx, &mut app);
        let (before, before_work) = run(&mut app, &ctx, 20, false);
        let (before_moving, before_moving_work) = run(&mut app, &ctx, 20, true);

        // After, from an empty cache.
        let ctx = egui::Context::default();
        let mut app = app_with(1000);
        let (start, start_work) = run(&mut app, &ctx, 5, false);
        let shown = app.thumbs.drawn;
        let (after, after_work) = run(&mut app, &ctx, 20, false);
        assert_eq!(app.thumbs.drawn, shown, "a still list draws nothing again");
        let redrawn = app.thumbs.drawn;
        let (after_moving, after_moving_work) = run(&mut app, &ctx, 20, true);
        let (held, bytes) = app.thumbs.held();
        println!(
            "#1185 list of 1,000 objects, 320 x 900 pt, release build. Before: a frame {:.2} ms, \
             of it thumbnails {:.2} ms; moving one object each frame {:.2} ms, thumbnails {:.2} ms. \
             After: the first 5 frames {:?} ms, thumbnails {:?} ms ({shown} pictures for the rows \
             on screen); then a frame {:.2} ms, thumbnails {:.3} ms; moving one object each frame \
             {:.2} ms, thumbnails {:.3} ms (most {:.3} ms; {} pictures redrawn in 20 frames); \
             {held} pictures held, {} KiB",
            ms(median(before)),
            ms(median(before_work)),
            ms(median(before_moving)),
            ms(median(before_moving_work)),
            start.iter().map(|d| (ms(*d) * 10.0).round() / 10.0).collect::<Vec<_>>(),
            start_work.iter().map(|d| (ms(*d) * 100.0).round() / 100.0).collect::<Vec<_>>(),
            ms(median(after)),
            ms(median(after_work)),
            ms(median(after_moving)),
            ms(median(after_moving_work.clone())),
            ms(max(&after_moving_work)),
            app.thumbs.drawn - redrawn,
            bytes / 1024,
        );
        assert!(shown > 10 && shown < 100, "only rows on screen are drawn: {shown}");
        assert!(bytes <= CAP_BYTES);
        // Only the moved object's picture is drawn again, once per move at most.
        assert!(app.thumbs.drawn - redrawn <= 20, "{}", app.thumbs.drawn - redrawn);
    }
    #[test]
    fn a_screenshot_run_draws_every_row_on_screen_at_once() {
        let ctx = egui::Context::default();
        let mut app = app_with(1000);
        app.thumbs.unlimited = true;
        list_frame(&ctx, &mut app);
        let drawn = app.thumbs.drawn;
        assert!(drawn > 10, "{drawn}");
        list_frame(&ctx, &mut app);
        assert_eq!(app.thumbs.drawn, drawn, "nothing was left waiting");
    }

    #[test]
    fn pictures_follow_the_display_scale() {
        let ctx = egui::Context::default();
        let mut app = app_with(3);
        app.thumbs.unlimited = true;
        list_frame(&ctx, &mut app);
        let sizes = |app: &EditorApp| -> Vec<[usize; 2]> {
            app.thumbs.entries.values().map(|e| e.texture.size()).collect()
        };
        assert!(sizes(&app).iter().all(|s| *s == [26, 26]), "{:?}", sizes(&app));
        ctx.set_pixels_per_point(2.0);
        list_frame(&ctx, &mut app);
        list_frame(&ctx, &mut app);
        assert!(sizes(&app).iter().all(|s| *s == [52, 52]), "{:?}", sizes(&app));
    }

    #[test]
    fn the_cache_drops_the_least_recently_shown_pictures_past_its_cap() {
        let ctx = egui::Context::default();
        let mut app = app_with(10);
        app.thumbs.unlimited = true;
        app.thumbs.cap = 3 * 26 * 26 * 4;
        let objects = app.session.design().objects.clone();
        for o in &objects {
            let compiled = Compiled {
                pattern: &app.pattern,
                spans: &app.quality.spans,
                revision: app.revision,
            };
            let thumbs = &mut app.thumbs;
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                thumbnail(ui, thumbs, &compiled, o, 30.0);
            });
            out.textures_delta.clear();
        }
        let (held, bytes) = app.thumbs.held();
        assert_eq!((held, bytes), (3, 3 * 26 * 26 * 4));
        let kept: std::collections::BTreeSet<u64> = app.thumbs.entries.keys().copied().collect();
        let last: std::collections::BTreeSet<u64> =
            objects.iter().rev().take(3).map(|o| o.id).collect();
        assert_eq!(kept, last, "the three shown last are kept");
    }
}