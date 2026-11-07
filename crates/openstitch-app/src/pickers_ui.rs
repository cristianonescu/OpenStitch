//! Picture pickers with a live hover preview (#656, the rest of #553; beyond PE-DESIGN, off
//! unless chosen: Options ▸ Picture pickers with live preview, `ui.set picturePickers`).
//!
//! The Sewing Attributes pane's Region and Line sewing types, programmable fill patterns and
//! motifs show as tiles: each tile is the program's own stitches of that choice on a sample
//! shape (compiled and rendered by `openstitch_render::thumbnail`, not a drawn picture).
//! Hovering a tile re-stitches the selection on the canvas: the change runs on a fork of the
//! session (`Session::fork`) and only the window's stitches are swapped, so the design and its
//! undo history are untouched; moving off the tiles brings the design's own stitches back, and
//! a click commits the change as one undo step. The Assistant previews with `ui.preview`.

use crate::app::EditorApp;
use crate::open_job::Compiled;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use openstitch_app::editor::render::PatternStats;
use openstitch_engine::Session;
use serde_json::{Value, json};
use std::collections::HashMap;

/// Tile size in pixels (shown at half size for a sharp picture).
const TILE_PX: u32 = 112;
const TILE_POINTS: f32 = 56.0;

/// One choice of a picker.
pub struct Tile {
    /// The parameter value (`fill`, `satin`, a pattern id…).
    pub value: String,
    pub label: String,
    /// How the sample shape is set up for this choice: `object.setRegion` or `object.setLine`
    /// parameters (without `ids`).
    pub sample: Sample,
}

/// The sample shape a tile shows.
#[derive(Clone, Debug)]
pub enum Sample {
    /// A 24 × 16 mm rectangle with this region sewing and no line sewing.
    Region(Value),
    /// An open 24 × 12 mm arc with this line sewing.
    Line(Value),
}

/// The stitches the canvas shows instead of the design's while a preview is on.
pub struct Preview {
    /// The previewed command, as JSON (the cache key).
    key: String,
    /// The design's revision it was made for, and that design: a later revision of the same
    /// design keeps it (a recompile without an edit), any edit drops it.
    revision: u64,
    base: openstitch_design::Design,
    pub generation: u64,
    pub compiled: Compiled,
    /// Set by `ui.preview`: stays until cleared or the design changes, not only while hovered.
    pinned: bool,
}

/// A preview asked for: the command and whether `ui.preview` pinned it.
struct Wanted {
    key: String,
    request: CommandRequest,
    pinned: bool,
}

/// A preview compiling on a worker.
struct PreviewJob {
    wanted: Wanted,
    revision: u64,
    base: openstitch_design::Design,
    rx: std::sync::mpsc::Receiver<Result<Compiled, String>>,
}

#[derive(Default)]
pub struct Pickers {
    pending: Option<PreviewJob>,
    /// The latest preview asked for while another compiles.
    next: Option<Wanted>,
    /// Rendered tiles by sample key (`None`: it could not be rendered).
    tiles: HashMap<String, Option<egui::TextureHandle>>,
    pub preview: Option<Preview>,
    generation: u64,
    /// A tile was hovered this frame.
    hovered: bool,
    /// `--show pickers`: open this picker in the next frame.
    pub force_open: Option<&'static str>,
}

impl Pickers {
    /// Tiles rendered so far.
    #[cfg(test)]
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }
}

impl Preview {
    pub fn stats(&self) -> &PatternStats {
        &self.compiled.stats
    }
}

impl EditorApp {
    /// Shows `request` on the canvas without changing the design (a hover, `ui.preview`).
    /// The stitches compile on a worker (a stippling preview can take seconds), so the window
    /// keeps drawing the design until they are ready; only the latest request waits its turn.
    /// `wait` blocks until it is shown (`ui.preview`, tests). Reuses a preview of the same
    /// command and revision.
    pub fn preview_request(
        &mut self,
        request: &CommandRequest,
        pinned: bool,
        wait: bool,
    ) -> Result<(), String> {
        let key = json!([request.command, request.params]).to_string();
        if let Some(p) = &mut self.pickers.preview
            && p.key == key
            && p.revision == self.revision
        {
            p.pinned |= pinned;
            return Ok(());
        }
        let wanted = Wanted {
            key,
            request: request.clone(),
            pinned,
        };
        match &self.pickers.pending {
            Some(job) if job.wanted.key == wanted.key && job.revision == self.revision => {}
            Some(_) => self.pickers.next = Some(wanted),
            None => self.start_preview(wanted),
        }
        if wait {
            while self.pickers.pending.is_some() {
                self.finish_preview(true)?;
            }
            if self.live_preview().is_none() {
                return Err("the preview was replaced before it was shown".into());
            }
        }
        Ok(())
    }

    fn start_preview(&mut self, wanted: Wanted) {
        let mut fork = self.session.fork();
        let (tx, rx) = std::sync::mpsc::channel();
        let request = wanted.request.clone();
        openstitch_app::platform::run_job(move || {
            let out = fork
                .execute(request.command, Some(&request.params))
                .map(|_| Compiled::of(fork.design()))
                .map_err(|e| e.to_string());
            let _ = tx.send(out);
        });
        self.pickers.pending = Some(PreviewJob {
            wanted,
            revision: self.revision,
            base: self.session.design().clone(),
            rx,
        });
    }

    /// Takes a finished preview (waiting for it when `block`) and starts the next one.
    fn finish_preview(&mut self, block: bool) -> Result<(), String> {
        let Some(job) = &self.pickers.pending else {
            return Ok(());
        };
        let out = if block {
            job.rx.recv().map_err(|_| "the preview stopped".to_string())
        } else {
            match job.rx.try_recv() {
                Ok(out) => Ok(out),
                Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
                Err(_) => Err("the preview stopped".to_string()),
            }
        };
        let job = self.pickers.pending.take().expect("pending");
        let result = out.and_then(|r| r).map(|compiled| {
            if job.revision == self.revision || job.base == *self.session.design() {
                self.pickers.generation += 1;
                self.pickers.preview = Some(Preview {
                    key: job.wanted.key,
                    revision: self.revision,
                    base: job.base,
                    generation: self.pickers.generation,
                    compiled,
                    pinned: job.wanted.pinned,
                });
            }
        });
        if let Some(next) = self.pickers.next.take() {
            self.start_preview(next);
        }
        result
    }

    /// Whether a preview is being compiled (the window keeps repainting meanwhile).
    pub fn preview_pending(&self) -> bool {
        self.pickers.pending.is_some()
    }

    pub fn clear_preview(&mut self) -> bool {
        self.pickers.next = None;
        self.pickers.pending = None;
        self.pickers.preview.take().is_some()
    }

    /// The preview the canvas shows now, if any (dropped once the design has changed).
    pub fn live_preview(&self) -> Option<&Preview> {
        self.pickers
            .preview
            .as_ref()
            .filter(|p| p.revision == self.revision)
    }

    /// Start of a frame: no tile hovered yet.
    pub fn pickers_frame_start(&mut self) {
        self.pickers.hovered = false;
    }

    /// End of a frame: a hover preview ends when no tile is hovered.
    pub fn pickers_frame_end(&mut self) {
        // A recompile without an edit keeps the preview.
        let revision = self.revision;
        if let Some(p) = &mut self.pickers.preview
            && p.revision != revision
            && p.base == *self.session.design()
        {
            p.revision = revision;
        }
        if let Err(e) = self.finish_preview(false) {
            self.status = format!("Preview: {e}");
        }
        if !self.pickers.hovered {
            // Hover previews not shown yet are not wanted any more.
            if self.pickers.next.as_ref().is_some_and(|w| !w.pinned) {
                self.pickers.next = None;
            }
            if self
                .pickers
                .pending
                .as_ref()
                .is_some_and(|j| !j.wanted.pinned)
            {
                self.pickers.pending = None;
            }
        }
        let stale =
            self.pickers.preview.as_ref().is_some_and(|p| {
                p.revision != self.revision || (!p.pinned && !self.pickers.hovered)
            });
        if stale {
            self.pickers.preview = None;
        }
    }

    /// `ui.preview`: `{command, params}` previews a design change, `clear: true` ends it.
    pub(crate) fn preview_command(&mut self, params: Option<&Value>) -> Result<Value, String> {
        let clear = params
            .and_then(|v| v.get("clear"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if clear {
            return Ok(json!({ "cleared": self.clear_preview() }));
        }
        let Some(command) = params
            .and_then(|v| v.get("command"))
            .and_then(Value::as_str)
        else {
            return Ok(match self.live_preview() {
                Some(p) => json!({ "previewing": true, "stitches": p.stats().stitches }),
                None => json!({ "previewing": false }),
            });
        };
        let spec = self
            .session
            .commands()
            .into_iter()
            .find(|c| c.id == command)
            .ok_or_else(|| format!("unknown command '{command}'"))?;
        if spec.effect != openstitch_engine::Effect::Edit {
            return Err(format!(
                "'{command}' does not change the design: nothing to preview"
            ));
        }
        let id: &'static str = spec.id;
        let p = params
            .and_then(|v| v.get("params"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        self.preview_request(&CommandRequest::new(id, p), true, true)?;
        let shown = self.live_preview().expect("just made");
        Ok(json!({
            "previewing": true,
            "stitches": shown.stats().stitches,
            "designStitches": self.stats.stitches,
            "note": "The design is unchanged: run the command to keep it, or clear the preview",
        }))
    }
}

/// The sample shape's stitches for `sample`.
fn sample_pattern(sample: &Sample) -> Result<openstitch_core::StitchPattern, String> {
    let mut s = Session::new(std::env::temp_dir());
    let run =
        |s: &mut Session, id: &str, p: Value| s.execute(id, Some(&p)).map_err(|e| e.to_string());
    let (shape, set, other) = match sample {
        Sample::Region(params) => (
            run(
                &mut s,
                "shape.rectangle",
                json!({ "x": 0, "y": 0, "width": 24, "height": 16 }),
            )?,
            ("object.setRegion", params.clone()),
            ("object.setLine", json!({ "type": "none" })),
        ),
        Sample::Line(params) => (
            run(
                &mut s,
                "shape.arc",
                json!({ "cx": 12, "cy": 12, "rx": 12, "ry": 9, "start": 180, "sweep": 180 }),
            )?,
            ("object.setLine", params.clone()),
            ("object.setRegion", json!({ "type": "none" })),
        ),
    };
    let ids = CommandRequest::ids(&CommandRequest::created_ids(&shape));
    // A shape with no region (the open arc) refuses region settings: that is fine.
    let _ = run(&mut s, other.0, merge(other.1, &ids));
    run(&mut s, set.0, merge(set.1, &ids))?;
    let d = s.design();
    Ok(openstitch_design::Compiler::for_design(d).compile(d))
}

fn merge(mut params: Value, ids: &Value) -> Value {
    if let Value::Object(m) = &mut params {
        m.insert("ids".into(), ids.clone());
    }
    params
}

fn sample_key(sample: &Sample) -> String {
    match sample {
        Sample::Region(p) => format!("region:{p}"),
        Sample::Line(p) => format!("line:{p}"),
    }
}

/// The tile's picture, rendered once per sample.
fn tile_texture(
    app: &mut EditorApp,
    ctx: &egui::Context,
    sample: &Sample,
) -> Option<egui::TextureHandle> {
    let key = sample_key(sample);
    if let Some(t) = app.pickers.tiles.get(&key) {
        return t.clone();
    }
    let texture = sample_pattern(sample)
        .and_then(|p| openstitch_render::thumbnail::png(&p, TILE_PX))
        .ok()
        .and_then(|png| image::load_from_memory(&png).ok())
        .map(|img| {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            ctx.load_texture(format!("picker-{key}"), image, egui::TextureOptions::LINEAR)
        });
    app.pickers.tiles.insert(key, texture.clone());
    texture
}

/// A picker: a button with the current choice that opens a grid of tiles. Hovering a tile
/// previews `request(value)` on the canvas; a click returns the value to set.
pub fn picker(
    app: &mut EditorApp,
    ui: &mut egui::Ui,
    id: &'static str,
    current: &str,
    tiles: &[Tile],
    request: &dyn Fn(&str) -> CommandRequest,
) -> Option<String> {
    let label = tiles
        .iter()
        .find(|t| t.value == current)
        .map_or(current.to_string(), |t| t.label.clone());
    let button = ui
        .button(label)
        .on_hover_text("Choose from pictures; hover one to see it on the design");
    let force = (app.pickers.force_open == Some(id)).then(|| {
        app.pickers.force_open = None;
        egui::SetOpenCommand::Bool(true)
    });
    let cmd = if button.clicked() {
        Some(egui::SetOpenCommand::Toggle)
    } else {
        force
    };
    let ctx = ui.ctx().clone();
    let mut chosen = None;
    let mut hovered = None;
    egui::Popup::from_response(&button)
        .id(ui.make_persistent_id(id))
        .open_memory(cmd)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    egui::Grid::new(("picker-grid", id))
                        .spacing(egui::vec2(6.0, 6.0))
                        .show(ui, |ui| {
                            for (i, t) in tiles.iter().enumerate() {
                                let tex = tile_texture(app, &ctx, &t.sample);
                                let r = ui
                                    .vertical_centered(|ui| {
                                        ui.set_width(TILE_POINTS + 24.0);
                                        let r = match &tex {
                                            Some(tex) => ui.add(
                                                egui::Button::image(
                                                    egui::Image::new(tex).fit_to_exact_size(
                                                        egui::vec2(TILE_POINTS, TILE_POINTS),
                                                    ),
                                                )
                                                .selected(t.value == current),
                                            ),
                                            None => ui.add(
                                                egui::Button::new("(no picture)")
                                                    .selected(t.value == current),
                                            ),
                                        };
                                        ui.label(egui::RichText::new(&t.label).small());
                                        r
                                    })
                                    .inner
                                    .on_hover_text(&t.label);
                                if r.hovered() {
                                    hovered = Some(t.value.clone());
                                }
                                if r.clicked() {
                                    chosen = Some(t.value.clone());
                                }
                                if i % 4 == 3 {
                                    ui.end_row();
                                }
                            }
                        });
                });
        });
    if let Some(v) = &hovered
        && *v != current
        && chosen.is_none()
    {
        app.pickers.hovered = true;
        if let Err(e) = app.preview_request(&request(v), false, false) {
            app.status = format!("Preview: {e}");
        }
    }
    if chosen.is_some() {
        app.clear_preview();
        ui.close();
    }
    chosen.filter(|v| v != current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;

    fn demo() -> EditorApp {
        EditorApp::new(Launch {
            demo: true,
            ..Launch::default()
        })
    }

    #[test]
    fn every_region_and_line_type_has_a_rendered_tile() {
        for (k, _) in openstitch_app::editor::inspector::region_types()
            .into_iter()
            .skip(1)
        {
            let s = Sample::Region(
                json!({ "type": openstitch_app::editor::inspector::region_name(k) }),
            );
            let p = sample_pattern(&s).unwrap_or_else(|e| panic!("{k:?}: {e}"));
            assert!(p.count(openstitch_core::Command::Stitch) > 0, "{k:?} sews");
            assert!(openstitch_render::thumbnail::png(&p, TILE_PX).is_ok());
        }
        for (k, _) in openstitch_app::editor::inspector::line_types()
            .into_iter()
            .skip(1)
        {
            let s =
                Sample::Line(json!({ "type": openstitch_app::editor::inspector::line_name(k) }));
            let p = sample_pattern(&s).unwrap_or_else(|e| panic!("{k:?}: {e}"));
            assert!(p.count(openstitch_core::Command::Stitch) > 0, "{k:?} sews");
        }
    }

    #[test]
    fn a_preview_changes_only_the_shown_stitches_and_goes_away() {
        let mut app = demo();
        app.selection = [1].into();
        let design = app.session.design().clone();
        let undo = app.undo_groups.clone();
        let shown = app.pattern.clone();
        let r = CommandRequest::new("object.setRegion", json!({ "ids": [1], "type": "satin" }));
        app.preview_request(&r, false, true).expect("preview");
        let p = app.live_preview().expect("previewing");
        assert_ne!(p.compiled.pattern, shown, "the canvas shows other stitches");
        assert_eq!(app.session.design(), &design, "the design is untouched");
        assert_eq!(app.undo_groups, undo, "no undo step");
        assert!(!app.session.can_redo());
        // Leaving the tiles: the next frame with no hover ends it.
        app.pickers_frame_start();
        app.pickers_frame_end();
        assert!(
            app.live_preview().is_none(),
            "back to the design's stitches"
        );
        assert_eq!(app.pattern, shown);
    }

    #[test]
    fn a_hover_preview_compiles_off_the_ui_thread_and_only_the_latest_waits() {
        let mut app = demo();
        let first = CommandRequest::new("object.setRegion", json!({ "ids": [1], "type": "satin" }));
        let last = CommandRequest::new("object.setRegion", json!({ "ids": [1], "type": "fill" }));
        app.preview_request(&first, false, false).expect("first");
        assert!(app.preview_pending(), "compiling on a worker");
        assert!(app.live_preview().is_none(), "the design shows meanwhile");
        app.preview_request(&last, false, false).expect("last");
        let started = std::time::Instant::now();
        // Frames while the tile stays hovered.
        while app.preview_pending() || app.live_preview().is_none() {
            assert!(started.elapsed().as_secs() < 60, "the preview never came");
            app.pickers.hovered = true;
            app.pickers_frame_end();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let shown = app.live_preview().expect("shown");
        assert_eq!(
            shown.key,
            json!([last.command, last.params]).to_string(),
            "the latest hover is what shows"
        );
        // Moving off the tiles drops it, and a pending one too.
        app.preview_request(&first, false, false).expect("again");
        app.pickers_frame_start();
        app.pickers_frame_end();
        assert!(!app.preview_pending() && app.live_preview().is_none());
    }

    #[test]
    fn the_assistant_previews_and_clears_without_an_undo_step() {
        let mut app = demo();
        let undo = app.undo_groups.clone();
        let design = app.session.design().clone();
        let r = app
            .ui_command(
                "ui.preview",
                Some(&json!({ "command": "object.setRegion", "params": { "ids": [1], "type": "stippling" } })),
            )
            .expect("preview");
        assert_eq!(r["previewing"], json!(true));
        assert!(r["stitches"].as_u64().is_some());
        // Pinned: a frame without hover keeps it.
        app.pickers_frame_start();
        app.pickers_frame_end();
        assert!(app.live_preview().is_some());
        assert_eq!(app.session.design(), &design);
        assert_eq!(app.undo_groups, undo);
        let state = app.ui_command("ui.preview", None).expect("state");
        assert_eq!(state["previewing"], json!(true));
        let r = app
            .ui_command("ui.preview", Some(&json!({ "clear": true })))
            .expect("clear");
        assert_eq!(r["cleared"], json!(true));
        assert!(
            app.ui_command("ui.preview", Some(&json!({ "command": "ui.state" })))
                .is_err(),
            "only design changes preview"
        );
    }

    #[test]
    fn an_edit_drops_a_pinned_preview() {
        let mut app = demo();
        app.ui_command(
            "ui.preview",
            Some(&json!({ "command": "object.setRegion", "params": { "ids": [1], "type": "satin" } })),
        )
        .expect("preview");
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 1, "y": 1, "width": 5, "height": 5 }),
        ))
        .expect("edit");
        assert!(app.live_preview().is_none(), "made for the old design");
    }

    #[test]
    fn picture_pickers_are_off_by_default_and_set_through_ui_set() {
        let mut app = demo();
        assert!(
            !app.ui.prefs.picture_pickers,
            "PE-DESIGN's lists by default"
        );
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "picturePickers", "value": true })),
        )
        .expect("set");
        assert!(app.ui.prefs.picture_pickers);
    }

    /// #656's 200 ms: a hover preview of a 10,000-stitch fill. Ignored by default; run with
    /// `cargo test -p openstitch-app --release --bin openstitch preview_bench -- --ignored --nocapture`.
    #[test]
    #[ignore = "benchmark"]
    fn preview_bench_10k_stitch_fill() {
        let mut app = EditorApp::new(Launch::default());
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 0, "y": 0, "width": 90, "height": 90 }),
        ))
        .expect("rectangle");
        let id = app.session.design().objects[0].id;
        println!("design: {} stitches", app.stats.stitches);
        for kind in ["satin", "stippling", "fill"] {
            let r = CommandRequest::new("object.setRegion", json!({ "ids": [id], "type": kind }));
            let t = std::time::Instant::now();
            app.preview_request(&r, false, true).expect("preview");
            println!("preview {kind}: {:?}", t.elapsed());
            app.clear_preview();
        }
    }
}

#[cfg(test)]
mod frame_tests {
    use crate::app::{EditorApp, Launch};
    use eframe::egui;

    /// `--show pickers`: the Region Type pictures open over a satin preview; the pane's frames
    /// settle (no frame is thrown away for another layout pass once the tiles are drawn).
    #[test]
    fn the_open_picker_draws_its_tiles_and_settles() {
        let mut app = EditorApp::new(Launch {
            demo: true,
            select: vec![1],
            show: vec!["pickers".into()],
            ..Launch::default()
        });
        assert!(app.live_preview().is_some(), "{}", app.status);
        let ctx = egui::Context::default();
        for i in 0..4 {
            let mut discard = false;
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                crate::panels::sewing_attributes(&mut app, ui);
                discard = ui.ctx().will_discard();
            });
            out.textures_delta.clear();
            assert!(i == 0 || !discard, "frame {i} was thrown away");
        }
        assert!(
            app.pickers.tile_count() >= openstitch_app::editor::inspector::region_types().len(),
            "every region type has its picture"
        );
    }
}
