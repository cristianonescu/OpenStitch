//! Stitch Table (#501, beyond PE-DESIGN): every needle point of the selected stitch pattern as
//! a row (index, X and Y in mm, command, colour block). X and Y are typed or dragged exactly
//! (`stitches.setPoint`); Insert adds a point on the line into the first selected row
//! (`stitches.insertPoint`), Delete removes the selected rows (`stitches.deletePoints`); each is
//! one undo step. Selected rows are the Point tool's needle selection, shown on the canvas.
//! Only the visible rows are drawn, so a 20,000-point pattern scrolls without lag.
//! Opened with the Sewing Attributes pane's Stitch Table button or `ui.stitchTable`.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui::{self, RichText};
use openstitch_app::editor::{CommandRequest, Tool};
use openstitch_core::Command;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Model units per mm.
const MM: f64 = 10.0;

#[derive(Default)]
pub struct StitchTable {
    pub open: bool,
    /// A cell being edited: (row, x mm, y mm), sent when the edit ends.
    draft: Option<(usize, f64, f64)>,
    /// The row the last plain click selected (Shift-click selects up to it).
    anchor: Option<usize>,
    /// Rows selected from outside the table (`ui.stitchTable`): scroll to this one once.
    scroll_to: Option<usize>,
    /// The rows of the last revision and object shown.
    cache: Option<(u64, u64, std::rc::Rc<Rows>)>,
}

/// The stitch pattern the table shows: the selection when it is exactly one stitch pattern.
pub fn target(app: &EditorApp) -> Option<u64> {
    if app.selection.len() != 1 {
        return None;
    }
    let id = *app.selection.first()?;
    let d = app.session.design();
    let o = &d.objects[d.index_of(id)?];
    let pattern = o.kind == openstitch_design::ObjectKind::Stitches && o.stitch_block.is_some();
    // A live shape or text (#501 part 2): its stitches, edited by hand on the first change.
    let live = matches!(
        o.kind,
        openstitch_design::ObjectKind::Shape | openstitch_design::ObjectKind::Text
    ) && (o.region.is_some() || o.line.is_some());
    (pattern || live).then_some(id)
}

/// The rows of `id`: a stitch pattern's needle points, or a live object's hand edits (where it
/// is now) or generated stitches; with their colour blocks, thread count and whether they were
/// edited by hand. Cached per design revision.
fn rows_of(app: &mut EditorApp, id: u64) -> std::rc::Rc<Rows> {
    if let Some((rev, cid, rows)) = &app.stitch_table.cache
        && *rev == app.revision
        && *cid == id
    {
        return rows.clone();
    }
    let d = app.session.design();
    let i = d.index_of(id).expect("target");
    let o = &d.objects[i];
    let (block, edited) = match (&o.stitch_block, o.kind) {
        (Some(b), openstitch_design::ObjectKind::Stitches) => (b.clone(), false),
        _ => match openstitch_design::hand_edits::current(o) {
            Some(b) => (b, true),
            None => {
                let p = openstitch_design::Compiler::for_design(d).compile_object(d, i);
                let stitches = p
                    .stitches
                    .into_iter()
                    .filter(|s| s.command != Command::End)
                    .collect();
                (
                    openstitch_design::StitchBlock {
                        stitches,
                        threads: p.threads,
                    },
                    false,
                )
            }
        },
    };
    let rows = std::rc::Rc::new(Rows {
        colours: openstitch_design::stitch_edit::colors(&block),
        blocks: block.threads.len(),
        stitches: block.stitches,
        edited,
    });
    app.stitch_table.cache = Some((app.revision, id, rows.clone()));
    rows
}

/// What the table lists.
pub struct Rows {
    stitches: Vec<openstitch_core::Stitch>,
    colours: Vec<usize>,
    blocks: usize,
    edited: bool,
}

fn command_name(c: Command) -> &'static str {
    match c {
        Command::Stitch => "stitch",
        Command::Jump => "jump",
        Command::Trim => "trim",
        Command::ColorChange => "colour change",
        Command::Stop => "stop",
        Command::End => "end",
    }
}

impl EditorApp {
    /// Sets needle point `index` of pattern `id` to (`x`, `y`) mm, one undo step.
    pub fn table_set_point(&mut self, id: u64, index: usize, x: f64, y: f64) -> bool {
        self.execute(&CommandRequest::new(
            "stitches.setPoint",
            json!({ "id": id, "index": index, "x": x, "y": y }),
        ))
        .is_some()
    }

    /// Insert: a point on the line into the first selected row (its midpoint), selected.
    pub fn table_insert(&mut self, id: u64) -> bool {
        let Some(&at) = self.tools.needle_selection(id).first() else {
            self.status = "Stitch Table: select the row to insert before".into();
            return false;
        };
        let Some(r) = self.execute(&CommandRequest::new(
            "stitches.insertPoint",
            json!({ "id": id, "index": at }),
        )) else {
            return false;
        };
        let new = r["index"].as_u64().map_or(at, |n| n as usize);
        self.tools.set_needle_selection(id, BTreeSet::from([new]));
        true
    }

    /// Delete: the selected rows, one undo step.
    pub fn table_delete(&mut self, id: u64) -> bool {
        let rows = self.tools.needle_selection(id);
        if rows.is_empty() {
            self.status = "Stitch Table: select the rows to delete".into();
            return false;
        }
        let done = self
            .execute(&CommandRequest::new(
                "stitches.deletePoints",
                json!({ "id": id, "indexes": rows.iter().collect::<Vec<_>>() }),
            ))
            .is_some();
        if done {
            self.tools.set_needle_selection(id, BTreeSet::new());
        }
        done
    }

    /// Selects rows, showing them on the canvas with the Point tool.
    pub fn table_select(&mut self, id: u64, rows: BTreeSet<usize>) {
        if self.tools.tool() != Tool::Point {
            self.tools.set_tool(Tool::Point);
        }
        self.tools.set_needle_selection(id, rows);
    }

    /// `ui.stitchTable`: open or close the table; `select` picks rows; returns what it shows.
    pub(crate) fn stitch_table_command(&mut self, params: Option<&Value>) -> Result<Value, String> {
        if let Some(open) = params.and_then(|v| v.get("open")).and_then(Value::as_bool) {
            self.stitch_table.open = open;
        }
        let Some(id) = target(self) else {
            return Ok(json!({ "open": self.stitch_table.open, "id": null,
                "note": "select one stitch pattern (ui.select) to fill the table" }));
        };
        let n = rows_of(self, id).stitches.len();
        if let Some(sel) = params.and_then(|v| v.get("select")) {
            let rows: BTreeSet<usize> = serde_json::from_value(sel.clone())
                .map_err(|_| "select is a list of row indexes".to_string())?;
            if let Some(bad) = rows.iter().find(|&&r| r >= n) {
                return Err(format!("no row {bad}: the pattern has {n} points"));
            }
            self.stitch_table.scroll_to = rows.first().copied();
            self.table_select(id, rows);
        }
        Ok(json!({
            "open": self.stitch_table.open,
            "id": id,
            "points": n,
            "selected": self.tools.needle_selection(id),
            "note": "Edit rows with stitches.setPoint, stitches.insertPoint and stitches.deletePoints (stitches.points lists them)",
        }))
    }
}

/// The Stitch Table window.
pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    if !app.stitch_table.open {
        return;
    }
    let mut open = true;
    egui::Window::new("Stitch Table")
        .open(&mut open)
        .default_size([380.0, 460.0])
        .resizable(true)
        .show(ctx, |ui| table(app, ui));
    app.stitch_table.open = open;
}

fn table(app: &mut EditorApp, ui: &mut egui::Ui) {
    let Some(id) = target(app) else {
        ui.label(RichText::new(t("ui-select-one-stitch-pattern-to-list")).weak());
        return;
    };
    let all = rows_of(app, id);
    let (rows, colours, blocks) = (&all.stitches, &all.colours, all.blocks);
    let selected = app.tools.needle_selection(id);
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{} points, {} colour blocks", rows.len(), blocks));
        if all.edited {
            ui.separator();
            ui.label(t("ui-edited-by-hand"));
            if ui
                .button(t("ui-regenerate"))
                .on_hover_text(t("ui-drop-the-hand-edits-the-object-tip"))
                .clicked()
            {
                app.execute(&CommandRequest::new(
                    "object.regenerate",
                    json!({ "ids": [id] }),
                ));
            }
        }
        ui.separator();
        if ui
            .add_enabled(!selected.is_empty(), egui::Button::new(t("ui-insert")))
            .on_hover_text(t("ui-add-a-needle-point-halfway-along-tip"))
            .clicked()
        {
            app.table_insert(id);
        }
        if ui
            .add_enabled(!selected.is_empty(), egui::Button::new(t("ui-delete")))
            .on_hover_text(t("ui-delete-the-selected-rows-a-jump-tip"))
            .clicked()
        {
            app.table_delete(id);
        }
    });
    ui.separator();
    let widths = [56.0, 84.0, 84.0, 96.0, 40.0];
    ui.horizontal(|ui| {
        for (w, h) in widths
            .iter()
            .zip(["#", "X (mm)", "Y (mm)", "Command", "Colour"])
        {
            ui.add_sized([*w, 18.0], egui::Label::new(RichText::new(h).strong()));
        }
    });
    let row_h = ui.spacing().interact_size.y + 2.0;
    let mut clicked: Option<(usize, egui::Modifiers)> = None;
    let mut commit: Option<(usize, f64, f64)> = None;
    let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
    if let Some(r) = app.stitch_table.scroll_to.take() {
        let spacing = ui.spacing().item_spacing.y;
        scroll = scroll.vertical_scroll_offset((r.saturating_sub(2)) as f32 * (row_h + spacing));
    }
    scroll.show_rows(ui, row_h, rows.len(), |ui, range| {
        for i in range {
            let st = rows[i];
            let draft = app.stitch_table.draft.filter(|d| d.0 == i);
            let (mut x, mut y) = draft.map_or((st.x / MM, st.y / MM), |d| (d.1, d.2));
            ui.horizontal(|ui| {
                let r = ui.add_sized(
                    [widths[0], row_h - 2.0],
                    egui::Button::selectable(selected.contains(&i), format!("{i}")),
                );
                if r.clicked() {
                    clicked = Some((i, ui.input(|inp| inp.modifiers)));
                }
                let mut edited = false;
                let mut ended = false;
                for (k, v) in [(1, &mut x), (2, &mut y)] {
                    let r = ui.add_sized(
                        [widths[k], row_h - 2.0],
                        egui::DragValue::new(v).speed(0.01).max_decimals(2),
                    );
                    let axis = if k == 1 { "X" } else { "Y" };
                    crate::a11y::name(&r, &format!("Point {i} {axis} (mm)"));
                    edited |= r.changed();
                    ended |= r.drag_stopped() || r.lost_focus();
                }
                if edited {
                    app.stitch_table.draft = Some((i, x, y));
                }
                if ended && app.stitch_table.draft.is_some_and(|d| d.0 == i) {
                    commit = Some((i, x, y));
                }
                ui.add_sized(
                    [widths[3], row_h - 2.0],
                    egui::Label::new(command_name(st.command)),
                );
                ui.add_sized(
                    [widths[4], row_h - 2.0],
                    egui::Label::new(format!("{}", colours.get(i).map_or(0, |c| c + 1))),
                );
            });
        }
    });
    if let Some((i, x, y)) = commit {
        app.stitch_table.draft = None;
        let st = rows[i];
        if (x - st.x / MM).abs() > 1e-9 || (y - st.y / MM).abs() > 1e-9 {
            app.table_set_point(id, i, x, y);
        }
    }
    if let Some((i, m)) = clicked {
        let mut sel = selected.clone();
        if m.command {
            if !sel.remove(&i) {
                sel.insert(i);
            }
        } else if m.shift {
            let a = app.stitch_table.anchor.unwrap_or(i);
            sel = (a.min(i)..=a.max(i)).collect();
        } else {
            sel = BTreeSet::from([i]);
            app.stitch_table.anchor = Some(i);
        }
        app.table_select(id, sel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;

    /// A stitch pattern of `n` stitches selected in a fresh editor.
    fn pattern(n: usize) -> (EditorApp, u64) {
        let dir = openstitch_testutil::TempDir::new("app-stitch-table");
        let mut p = openstitch_core::StitchPattern::new();
        p.threads.push(openstitch_core::Thread::rgb(0x00_40_80));
        for i in 0..n {
            p.stitch_to((i % 100) as f64 * 2.0, (i / 100) as f64 * 2.0);
        }
        p.push(Command::End);
        let file = dir.join("p.dst");
        std::fs::write(&file, openstitch_formats::dst::write(&p).expect("dst")).expect("write");
        let mut app = EditorApp::new(Launch::default());
        app.import(&file);
        let id = *app.selection.first().expect("imported");
        (app, id)
    }

    fn point(app: &EditorApp, id: u64, i: usize) -> (f64, f64) {
        let d = app.session.design();
        let b = d.objects[d.index_of(id).expect("id")]
            .stitch_block
            .as_ref()
            .expect("block");
        (b.stitches[i].x, b.stitches[i].y)
    }

    #[test]
    fn a_typed_x_moves_that_point_exactly_in_one_undo_step() {
        let (mut app, id) = pattern(50);
        let before = app.undo_groups.len();
        let (_, y) = point(&app, id, 5);
        assert!(app.table_set_point(id, 5, 12.3, y / MM));
        assert_eq!(point(&app, id, 5).0, 123.0, "12.3 mm exactly");
        assert_eq!(app.undo_groups.len(), before + 1);
        app.undo();
        assert_ne!(point(&app, id, 5).0, 123.0);
    }

    #[test]
    fn insert_and_delete_rows_are_one_undo_step_each() {
        let (mut app, id) = pattern(20);
        let n = |app: &EditorApp| {
            let d = app.session.design();
            d.objects[d.index_of(id).expect("id")]
                .stitch_block
                .as_ref()
                .expect("b")
                .stitches
                .len()
        };
        let start = n(&app);
        assert!(!app.table_insert(id), "nothing selected");
        app.ui_command(
            "ui.stitchTable",
            Some(&json!({ "open": true, "select": [4] })),
        )
        .expect("select");
        assert_eq!(
            app.tools.tool(),
            Tool::Point,
            "selected rows show on the canvas"
        );
        let undo = app.undo_groups.len();
        assert!(app.table_insert(id));
        assert_eq!(n(&app), start + 1);
        assert_eq!(app.undo_groups.len(), undo + 1);
        let new = *app.tools.needle_selection(id).first().expect("the new row");
        assert_eq!(new, 4, "the new point sits before the old row 4");
        app.ui_command("ui.stitchTable", Some(&json!({ "select": [2, 3, 7] })))
            .expect("select");
        assert!(app.table_delete(id));
        assert_eq!(n(&app), start + 1 - 3);
        assert_eq!(app.undo_groups.len(), undo + 2);
        app.undo();
        assert_eq!(n(&app), start + 1);
    }

    #[test]
    fn a_20000_point_table_draws_only_its_visible_rows() {
        let (mut app, _) = pattern(20_000);
        app.stitch_table.open = true;
        let ctx = egui::Context::default();
        let t = std::time::Instant::now();
        let mut shapes = 0;
        for _ in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| window(&mut app, ui.ctx()));
            out.textures_delta.clear();
            shapes = out.shapes.len();
        }
        assert!(shapes < 2_000, "{shapes} shapes: rows are not virtualised");
        assert!(
            t.elapsed().as_secs_f64() < 10.0,
            "three frames took {:?}",
            t.elapsed()
        );
    }

    #[test]
    fn a_live_fill_lists_its_stitches_and_keeps_a_hand_edit() {
        // #501 part 2: the table on a live fill edits it by hand; it stays a fill.
        let mut app = EditorApp::new(Launch::default());
        let id = app
            .execute(&CommandRequest::new(
                "shape.rectangle",
                json!({ "x": 10, "y": 10, "width": 30, "height": 20 }),
            ))
            .expect("rectangle")["id"]
            .as_u64()
            .expect("id");
        app.selection = BTreeSet::from([id]);
        assert_eq!(target(&app), Some(id), "a live fill has a table");
        let r = app
            .ui_command("ui.stitchTable", Some(&json!({ "open": true })))
            .expect("open");
        let n = r["points"].as_u64().expect("points");
        assert!(n > 50, "its generated stitches: {n}");
        assert!(!rows_of(&mut app, id).edited);
        assert!(app.table_set_point(id, 7, 12.3, 15.0));
        let rows = rows_of(&mut app, id);
        assert!(rows.edited, "now edited by hand");
        assert_eq!(rows.stitches[7].x, 123.0);
        let d = app.session.design();
        let o = &d.objects[d.index_of(id).expect("id")];
        assert!(o.region.is_some() && o.hand_edits.is_some(), "still a fill");
    }

    #[test]
    fn ui_stitch_table_needs_one_stitch_pattern_and_checks_rows() {
        let mut app = EditorApp::new(Launch::default());
        let r = app
            .ui_command("ui.stitchTable", Some(&json!({ "open": true })))
            .expect("open");
        assert_eq!(r["id"], Value::Null);
        assert!(app.stitch_table.open);
        let (mut app, id) = pattern(10);
        let r = app.ui_command("ui.stitchTable", None).expect("state");
        assert_eq!(r["id"], json!(id));
        assert!(
            app.ui_command("ui.stitchTable", Some(&json!({ "select": [99] })))
                .is_err()
        );
    }
}
