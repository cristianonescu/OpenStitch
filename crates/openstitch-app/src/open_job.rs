//! Open and import in the background (#558). A 500,000-stitch DST took 0.84-1.12 s to open on
//! the UI thread (release build, `open_bench_tests.rs`), so File ▸ Open, Import, the recent
//! files and dropped files run on a worker: it forks the session ([`Session::fork`]), runs the
//! same commands the synchronous path runs, compiles the stitches, and hands back the design.
//! The window keeps repainting and shows a progress window with Cancel after a short delay;
//! the result is adopted as one undo step. Cancel (or an edit made meanwhile) drops the
//! worker's result and leaves the current design untouched.

use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use openstitch_app::editor::render::PatternStats;
use openstitch_design::{Design, timeline::Timeline};
use openstitch_engine::Session;
use serde_json::{Value, json};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

/// The progress window waits this long, so quick opens never flash it.
const SHOW_AFTER: Duration = Duration::from_millis(300);

/// What a job does with the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    /// File ▸ Open: the file becomes the design (a machine file a new design holding it).
    Open,
    /// Import into design: the file is added as a stitch pattern, centred and selected.
    Import,
}

/// The stages a job reports, in order.
pub const STAGES: [&str; 3] = ["Reading the file", "Building the stitches", "Finishing"];

/// A design's compiled stitches and what the window derives from them.
pub struct Compiled {
    pub pattern: openstitch_core::StitchPattern,
    pub spans: Vec<(u64, Range<usize>)>,
    pub timeline: Timeline,
    pub stats: PatternStats,
    pub estimate: openstitch_design::Estimate,
}

impl Compiled {
    pub fn of(d: &Design) -> Compiled {
        let (pattern, spans) =
            openstitch_design::compile::Compiler::for_design(d).compile_traced(d);
        Compiled {
            timeline: Timeline::build(&pattern, &spans, &d.machine),
            stats: PatternStats::of(&pattern),
            estimate: openstitch_design::estimate(&pattern, &d.machine),
            pattern,
            spans,
        }
    }
}

/// What the worker hands back.
pub struct Loaded {
    pub design: Design,
    /// The last command's result (the import's created ids).
    pub result: Value,
    pub compiled: Compiled,
}

/// A running open or import.
pub struct OpenJob {
    pub path: PathBuf,
    pub kind: JobKind,
    /// The open added a tab for it (#552): a failed or cancelled open closes that tab again.
    pub added_tab: bool,
    started: Instant,
    /// The design's revision when the job started; a later edit makes the result stale.
    revision: u64,
    cancel: Arc<AtomicBool>,
    stage: Arc<AtomicU8>,
    rx: Receiver<Result<Loaded, String>>,
}

impl OpenJob {
    /// Starts `steps` on a fork of `session`. `Import` steps are one `stitches.import`; the
    /// worker centres what it created.
    pub fn start(
        session: &Session,
        revision: u64,
        path: &Path,
        kind: JobKind,
        steps: Vec<CommandRequest>,
    ) -> OpenJob {
        let mut fork = session.fork();
        let cancel = Arc::new(AtomicBool::new(false));
        let stage = Arc::new(AtomicU8::new(0));
        let (tx, rx) = channel();
        let (c, st) = (cancel.clone(), stage.clone());
        std::thread::spawn(move || {
            let out = run(&mut fork, kind, &steps, &c, &st);
            // The receiver is gone when the job was cancelled: nothing to tell.
            let _ = tx.send(out);
        });
        OpenJob {
            path: path.to_path_buf(),
            kind,
            added_tab: false,
            started: Instant::now(),
            revision,
            cancel,
            stage,
            rx,
        }
    }

    pub fn stage(&self) -> usize {
        usize::from(self.stage.load(Ordering::Relaxed)).min(STAGES.len() - 1)
    }

    pub fn seconds(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// `ui.loading`'s answer.
    pub fn describe(&self) -> Value {
        json!({
            "running": true,
            "path": self.path.display().to_string(),
            "kind": match self.kind { JobKind::Open => "open", JobKind::Import => "import" },
            "stage": STAGES[self.stage()],
            "seconds": (self.seconds() * 10.0).round() / 10.0,
        })
    }

    /// Tells the worker to stop; its result, if any, is dropped with the job.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Blocks until the worker is done (tests, `--script` runs).
    pub fn wait(&self) -> Option<Result<Loaded, String>> {
        self.rx.recv().ok()
    }

    fn poll(&self) -> Option<Result<Loaded, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("the open stopped unexpectedly".into())),
        }
    }
}

fn run(
    s: &mut Session,
    kind: JobKind,
    steps: &[CommandRequest],
    cancel: &AtomicBool,
    stage: &AtomicU8,
) -> Result<Loaded, String> {
    let stopped = || cancel.load(Ordering::Relaxed);
    let mut result = Value::Null;
    for r in steps {
        if stopped() {
            return Err("cancelled".into());
        }
        result = s
            .execute(r.command, Some(&r.params))
            .map_err(|e| e.to_string())?;
    }
    if kind == JobKind::Import {
        let ids = CommandRequest::created_ids(&result);
        s.execute(
            "object.center",
            Some(&json!({ "ids": CommandRequest::ids(&ids) })),
        )
        .map_err(|e| e.to_string())?;
    }
    stage.store(1, Ordering::Relaxed);
    if stopped() {
        return Err("cancelled".into());
    }
    let compiled = Compiled::of(s.design());
    stage.store(2, Ordering::Relaxed);
    if stopped() {
        return Err("cancelled".into());
    }
    Ok(Loaded {
        design: s.design().clone(),
        result,
        compiled,
    })
}

impl EditorApp {
    /// File ▸ Open in the background; see [`EditorApp::open_path`] for what it opens.
    pub fn open_in_background(&mut self, p: &Path) {
        let steps = self.open_steps(p);
        self.start_job(p, JobKind::Open, steps);
    }

    /// Import into design in the background.
    pub fn import_in_background(&mut self, p: &Path) {
        let steps = vec![CommandRequest::new(
            "stitches.import",
            json!({ "path": p.display().to_string() }),
        )];
        self.start_job(p, JobKind::Import, steps);
    }

    fn start_job(&mut self, p: &Path, kind: JobKind, steps: Vec<CommandRequest>) {
        if let Some(old) = self.open_job.take() {
            old.cancel();
        }
        self.open_job = Some(OpenJob::start(&self.session, self.revision, p, kind, steps));
        self.status = format!("Opening {}…", p.display());
        // A scripted run waits, so its next step sees the design.
        if self.scripted() {
            self.finish_job_blocking();
        }
    }

    /// Cancels the running job; `false` when none runs.
    pub fn cancel_job(&mut self) -> bool {
        let Some(job) = self.open_job.take() else {
            return false;
        };
        job.cancel();
        self.close_tab_added_for(&job);
        self.status = format!(
            "Cancelled opening {}; the design is unchanged",
            job.path.display()
        );
        true
    }

    /// Waits for the running job and applies it.
    pub fn finish_job_blocking(&mut self) {
        if let Some(job) = self.open_job.take() {
            let out = job.wait().unwrap_or_else(|| Err("the open stopped".into()));
            self.apply_job(&job, out);
        }
    }

    /// Each frame: applies a finished job, or keeps the window repainting and shows progress.
    pub fn poll_job(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.open_job else {
            return;
        };
        if let Some(out) = job.poll() {
            let job = self.open_job.take().expect("job");
            self.apply_job(&job, out);
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        if job.started.elapsed() >= SHOW_AFTER {
            progress_window(self, ctx);
        }
    }

    /// Closes the empty tab an open added (#552), while it is still the active, untouched one.
    fn close_tab_added_for(&mut self, job: &OpenJob) {
        if job.added_tab && self.revision == job.revision {
            self.close_active_tab();
        }
    }

    fn apply_job(&mut self, job: &OpenJob, out: Result<Loaded, String>) {
        let loaded = match out {
            Ok(l) => l,
            Err(e) => {
                self.close_tab_added_for(job);
                self.status = format!("Could not open {}: {e}", job.path.display());
                return;
            }
        };
        if self.revision != job.revision {
            self.status = format!(
                "The design changed while {} was opening; nothing was replaced. Open it again",
                job.path.display()
            );
            return;
        }
        if self.session.adopt_design(loaded.design) {
            self.undo_groups.push(1);
            self.redo_groups.clear();
        }
        self.install(loaded.compiled);
        let p = &job.path;
        match job.kind {
            JobKind::Open => self.after_open(p),
            JobKind::Import => {
                self.selection = CommandRequest::created_ids(&loaded.result);
                self.status = format!("Imported {}", p.display());
            }
        }
    }
}

/// The progress window: file, stage, elapsed time and Cancel. Modal, so nothing is edited
/// under a running open.
fn progress_window(app: &mut EditorApp, ctx: &egui::Context) {
    let Some(job) = &app.open_job else { return };
    let verb = match job.kind {
        JobKind::Open => "Opening",
        JobKind::Import => "Importing",
    };
    let name = job.path.file_name().map_or_else(
        || job.path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let stage = job.stage();
    let seconds = job.seconds();
    let mut cancel = false;
    egui::Modal::new(egui::Id::unique("open-job-progress")).show(ctx, |ui| {
        ui.set_width(340.0);
        ui.heading(format!("{verb} {name}"));
        ui.add_space(6.0);
        ui.label(format!(
            "{} (step {} of {}), {seconds:.0} s",
            STAGES[stage],
            stage + 1,
            STAGES.len()
        ));
        ui.add(egui::ProgressBar::new((stage as f32 + 0.5) / STAGES.len() as f32).animate(true));
        ui.add_space(6.0);
        cancel = ui
            .button("Cancel")
            .on_hover_text("Stop opening; the current design stays as it is")
            .clicked()
            || ui.input(|i| i.key_pressed(egui::Key::Escape));
    });
    if cancel {
        app.cancel_job();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;

    fn big_dst(dir: &Path, n: usize) -> PathBuf {
        let mut p = openstitch_core::StitchPattern::new();
        p.threads.push(openstitch_core::Thread::rgb(0x00_40_80));
        for i in 0..n {
            p.stitch_to((i % 200) as f64 * 0.3, (i / 200) as f64 * 0.3);
        }
        p.push(openstitch_core::Command::End);
        let path = dir.join("big.dst");
        std::fs::write(&path, openstitch_formats::dst::write(&p).expect("dst")).expect("write");
        path
    }

    fn rectangle(app: &mut EditorApp) {
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 5, "y": 5, "width": 20, "height": 20 }),
        ))
        .expect("rectangle");
    }

    #[test]
    fn a_background_open_matches_the_direct_one_and_undoes_in_one_step() {
        let dir = openstitch_testutil::TempDir::new("app-open-job");
        let file = big_dst(&dir, 5_000);
        let mut direct = EditorApp::new(Launch::default());
        direct.open_path(&file);

        let mut app = EditorApp::new(Launch::default());
        rectangle(&mut app);
        let before = app.session.design().clone();
        app.open_in_background(&file);
        assert!(app.open_job.is_some(), "runs on a worker");
        assert_eq!(app.session.design(), &before, "untouched while it runs");
        app.finish_job_blocking();
        assert_eq!(
            app.session.design().objects.len(),
            direct.session.design().objects.len()
        );
        assert_eq!(
            app.pattern, direct.pattern,
            "the same stitches as a direct open"
        );
        assert_eq!(app.stats.stitches, direct.stats.stitches);
        assert!(app.status.starts_with("Opened"), "{}", app.status);
        app.undo();
        assert_eq!(
            app.session.design(),
            &before,
            "one Undo brings the old design back"
        );
    }

    #[test]
    fn cancel_leaves_the_design_untouched() {
        let dir = openstitch_testutil::TempDir::new("app-open-job");
        let file = big_dst(&dir, 5_000);
        let mut app = EditorApp::new(Launch::default());
        rectangle(&mut app);
        let before = app.session.design().clone();
        let undo = app.undo_groups.clone();
        app.open_in_background(&file);
        let r = app.ui_command("ui.loading", Some(&json!({ "cancel": true })));
        assert_eq!(r.expect("cancel")["cancelled"], json!(true));
        assert!(app.open_job.is_none());
        assert_eq!(app.session.design(), &before);
        assert_eq!(app.undo_groups, undo, "no undo step");
        assert!(app.status.contains("unchanged"), "{}", app.status);
        let idle = app.ui_command("ui.loading", None).expect("state");
        assert_eq!(idle["running"], json!(false));
    }

    #[test]
    fn a_background_import_is_added_centred_and_selected() {
        let dir = openstitch_testutil::TempDir::new("app-open-job");
        let file = big_dst(&dir, 2_000);
        let mut app = EditorApp::new(Launch::default());
        rectangle(&mut app);
        app.import_in_background(&file);
        let running = app.ui_command("ui.loading", None).expect("state");
        assert_eq!(running["kind"], json!("import"));
        app.finish_job_blocking();
        let d = app.session.design();
        assert_eq!(d.objects.len(), 2, "added beside the rectangle");
        assert_eq!(app.selection.len(), 1, "the import is selected");
        let b = d.objects[1].bounds().expect("bounds");
        assert!((b.center().x - d.page.center().x).abs() < 1.0, "centred");
    }

    #[test]
    fn an_edit_during_the_open_keeps_the_edit_and_drops_the_result() {
        let dir = openstitch_testutil::TempDir::new("app-open-job");
        let file = big_dst(&dir, 2_000);
        let mut app = EditorApp::new(Launch::default());
        app.open_in_background(&file);
        rectangle(&mut app);
        let edited = app.session.design().clone();
        app.finish_job_blocking();
        assert_eq!(app.session.design(), &edited, "the edit wins");
        assert!(app.status.contains("changed while"), "{}", app.status);
    }

    #[test]
    fn a_failed_open_says_why_and_changes_nothing() {
        let dir = openstitch_testutil::TempDir::new("app-open-job");
        let bad = dir.join("bad.osd");
        std::fs::write(&bad, b"not a design").expect("write");
        let mut app = EditorApp::new(Launch::default());
        rectangle(&mut app);
        let before = app.session.design().clone();
        app.open_in_background(&bad);
        app.finish_job_blocking();
        assert_eq!(app.session.design(), &before);
        assert!(app.status.starts_with("Could not open"), "{}", app.status);
    }
}
