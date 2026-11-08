//! Job sheets (beyond PE-DESIGN, #513): a client approval sheet and an operator worksheet, each
//! as an overview (one page) or detailed.
//!
//! - **Client:** the design at actual size when it fits the picture box, else fitted (the scale
//!   is printed), its size in mm and inches, the thread colours with brand and code, the job
//!   fields and an approval line. Detailed adds the actual-size pages.
//! - **Operator:** the job fields, hoop, size, stitches, colour changes, stops, trims and sewing
//!   time (`design.info`'s `sewingMinutes`), fabric and stabiliser, notes, a small picture and
//!   the colour sequence table (step, thread, brand, code, stitches, stops). Detailed adds one
//!   thumbnail per step, its stitches drawn over the rest of the design in grey.
//!
//! Job fields come from the design (`design.setJob`); they are never written to machine files.

use crate::pdf::Page;
use crate::sheets::{self, Align, Drawing, Item, MARGIN, Options};
use crate::texts;
use openstitch_core::{Command, StitchPattern, Thread};
use openstitch_design::{Compiler, Design, estimate};

/// Header and footer heights `sheets::place` keeps free on a page, mm.
const HEADER: f64 = 16.0;
const FOOTER: f64 = 12.0;
const BLACK: [u8; 3] = [0, 0, 0];
const GREY: [u8; 3] = [0x90, 0x90, 0x90];
const LIGHT: [u8; 3] = [0xD8, 0xD8, 0xD8];

/// The job a sheet is printed for. Empty fields are left out (the approval line is not).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Job {
    pub customer: String,
    pub order: String,
    pub due: String,
    pub notes: String,
    pub fabric: String,
    pub stabiliser: String,
    /// The shop's name, printed at the top of every sheet.
    pub shop: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    Client,
    Operator,
}

/// One colour block of the sewing order.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub thread: Option<Thread>,
    pub stitches: usize,
    pub stops: usize,
    pub trims: usize,
}

/// The colour sequence, as the machine sews it: one step per colour block, stops counted in the
/// block they pause.
pub fn steps(pattern: &StitchPattern) -> Vec<Step> {
    pattern
        .color_blocks()
        .into_iter()
        .map(|(t, block)| {
            let count = |c: Command| block.iter().filter(|s| s.command == c).count();
            Step {
                thread: t.cloned(),
                stitches: count(Command::Stitch),
                stops: count(Command::Stop),
                trims: count(Command::Trim),
            }
        })
        .collect()
}

/// Sewing time, minutes to a tenth, as `design.info` reports it (`sewingMinutes`).
pub fn sewing_minutes(design: &Design, pattern: &StitchPattern) -> f64 {
    (estimate(pattern, &design.machine).seconds / 6.0).round() / 10.0
}

fn text(at: (f64, f64), size: f64, s: impl Into<String>) -> Item {
    Item::Text {
        at,
        size,
        text: s.into(),
        rgb: BLACK,
        align: Align::Left,
    }
}

fn swatch(at: (f64, f64), t: Option<&Thread>) -> Item {
    Item::Rect {
        x: at.0,
        y: at.1 - 3.0,
        w: 6.0,
        h: 4.0,
        stroke: Some((GREY, 0.1)),
        fill: Some(t.map_or(BLACK, |t| [t.red, t.green, t.blue])),
    }
}

fn thread_label(t: Option<&Thread>) -> (String, String, String) {
    let Some(t) = t else {
        return (String::new(), String::new(), String::new());
    };
    (
        t.name.as_deref().unwrap_or(&t.hex()).to_string(),
        t.brand.as_deref().unwrap_or_default().to_string(),
        t.catalog_number.as_deref().unwrap_or_default().to_string(),
    )
}

/// The drawing area of a page on `paper` (portrait), mm.
fn area(paper: (f64, f64)) -> (f64, f64) {
    (
        paper.0 - 2.0 * MARGIN,
        paper.1 - 2.0 * MARGIN - HEADER - FOOTER,
    )
}

/// `items` (in design mm) scaled by `s` and moved so the point `from` lands on `to`.
fn moved(items: Vec<Item>, s: f64, from: (f64, f64), to: (f64, f64)) -> Vec<Item> {
    let p = |q: (f64, f64)| (to.0 + (q.0 - from.0) * s, to.1 + (q.1 - from.1) * s);
    items
        .into_iter()
        .map(|it| match it {
            Item::Polyline { points, width, rgb } => Item::Polyline {
                points: points.into_iter().map(p).collect(),
                width: width * s,
                rgb,
            },
            Item::Line {
                a,
                b,
                width,
                rgb,
                dashed,
            } => Item::Line {
                a: p(a),
                b: p(b),
                width: width * s,
                rgb,
                dashed,
            },
            other => other,
        })
        .collect()
}

/// The design fitted into the box `(x, y, w, h)` mm, never above actual size; returns the
/// items and the scale used.
fn picture(
    pattern: &StitchPattern,
    items: Vec<Item>,
    bx: (f64, f64, f64, f64),
) -> (Vec<Item>, f64) {
    let b = pattern.bounds();
    let (w, h) = ((b.width() / 10.0).max(1.0), (b.height() / 10.0).max(1.0));
    let s = (bx.2 / w).min(bx.3 / h).min(1.0);
    let to = (bx.0 + (bx.2 - w * s) / 2.0, bx.1 + (bx.3 - h * s) / 2.0);
    (moved(items, s, (b.min_x / 10.0, b.min_y / 10.0), to), s)
}

/// Job fields as `label: value` lines, empty ones left out.
fn job_lines(job: &Job, operator: bool) -> Vec<String> {
    let mut v = vec![
        ("print-customer", &job.customer),
        ("print-order", &job.order),
        ("print-due", &job.due),
    ];
    if operator {
        v.push(("print-fabric", &job.fabric));
        v.push(("print-stabiliser", &job.stabiliser));
    }
    v.into_iter()
        .filter(|(_, s)| !s.trim().is_empty())
        .map(|(k, s)| {
            texts::fill(
                "print-field",
                &[("label", texts::text(k)), ("value", s.trim().to_string())],
            )
        })
        .collect()
}

/// Notes wrapped to lines of about `chars` characters.
fn wrap(text: &str, chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > chars {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        out.push(line);
    }
    out
}

fn size_line(pattern: &StitchPattern) -> String {
    let b = pattern.bounds();
    let (w, h) = (b.width() / 10.0, b.height() / 10.0);
    texts::fill(
        "print-size",
        &[
            ("width", format!("{w:.1}")),
            ("height", format!("{h:.1}")),
            ("width-in", format!("{:.2}", w / 25.4)),
            ("height-in", format!("{:.2}", h / 25.4)),
        ],
    )
}

fn page_of(items: Vec<Item>, title: &str, paper: (f64, f64)) -> Page {
    let (aw, ah) = area(paper);
    let d = Drawing {
        items,
        extent: (0.0, 0.0, aw, ah),
    };
    sheets::place(&d, title, Some(1.0), paper).remove(0)
}

/// Shop name and job fields at the top of a sheet; returns the next free y.
fn heading(job: &Job, operator: bool, items: &mut Vec<Item>) -> f64 {
    let mut y = 4.0;
    if !job.shop.trim().is_empty() {
        items.push(text((0.0, y), 13.0, job.shop.trim()));
        y += 7.0;
    }
    for l in job_lines(job, operator) {
        items.push(text((0.0, y), 10.0, l));
        y += 5.5;
    }
    y + 2.0
}

fn client(
    design: &Design,
    pattern: &StitchPattern,
    job: &Job,
    detailed: bool,
    opts: &Options,
) -> Vec<Page> {
    let paper = opts.paper;
    let (aw, ah) = area(paper);
    let mut items = Vec::new();
    let mut y = heading(job, false, &mut items);
    // The picture: up to 60 % of the page, at actual size when it fits.
    let box_h = (ah * 0.6).min(ah - y - 60.0).max(40.0);
    let (pic, s) = picture(
        pattern,
        sheets::stitch_items(pattern, false),
        (0.0, y, aw, box_h),
    );
    items.push(Item::Rect {
        x: 0.0,
        y,
        w: aw,
        h: box_h,
        stroke: Some((LIGHT, 0.2)),
        fill: None,
    });
    items.extend(pic);
    y += box_h + 6.0;
    let scale = if (s - 1.0).abs() < 1e-9 {
        texts::text("print-shown-actual-size")
    } else {
        texts::fill(
            "print-shown-at",
            &[("percent", format!("{:.0}", s * 100.0))],
        )
    };
    items.push(text(
        (0.0, y),
        10.0,
        format!("{}   {scale}", size_line(pattern)),
    ));
    y += 7.0;
    items.push(text((0.0, y), 11.0, texts::text("print-thread-colours")));
    y += 6.0;
    // Distinct threads in sewing order, two columns.
    let mut seen: Vec<Option<Thread>> = Vec::new();
    for st in steps(pattern) {
        if !seen.contains(&st.thread) {
            seen.push(st.thread);
        }
    }
    let rows = seen.len().div_ceil(2);
    for (i, t) in seen.iter().enumerate() {
        let (x, yy) = (
            if i < rows { 0.0 } else { aw / 2.0 },
            y + (i % rows.max(1)) as f64 * 5.5,
        );
        items.push(swatch((x, yy), t.as_ref()));
        let (name, brand, code) = thread_label(t.as_ref());
        items.push(text(
            (x + 8.0, yy),
            9.0,
            format!("{name}  {brand} {code}").trim().to_string(),
        ));
    }
    y += rows as f64 * 5.5 + 4.0;
    if !job.notes.trim().is_empty() {
        for l in wrap(job.notes.trim(), 95) {
            items.push(text((0.0, y), 9.0, l));
            y += 4.5;
        }
        y += 2.0;
    }
    // The approval line.
    let line_y = (y + 12.0).min(ah - 2.0);
    for (x, w, label) in [
        (0.0, 80.0, texts::text("print-approved-by")),
        (90.0, 40.0, texts::text("print-date")),
        (140.0, aw - 140.0, texts::text("print-signature")),
    ] {
        items.push(Item::Line {
            a: (x, line_y),
            b: (x + w, line_y),
            width: 0.2,
            rgb: BLACK,
            dashed: false,
        });
        items.push(text((x, line_y + 4.0), 8.0, label));
    }
    let mut pages = vec![page_of(items, &format!("{}: approval", design.name), paper)];
    if detailed {
        let mut actual = sheets::actual_size(design, opts);
        // The sewing information page is the operator's; the client sees the design.
        actual.pop();
        pages.extend(actual);
    }
    pages
}

fn operator(
    design: &Design,
    pattern: &StitchPattern,
    job: &Job,
    detailed: bool,
    opts: &Options,
) -> Vec<Page> {
    let paper = opts.paper;
    let (aw, ah) = area(paper);
    let title = format!("{}: operator sheet", design.name);
    let mut items = Vec::new();
    let mut y = heading(job, true, &mut items);
    let all = steps(pattern);
    let e = estimate(pattern, &design.machine);
    let hoop = design
        .machine
        .hoop
        .clone()
        .unwrap_or_else(|| texts::text("print-custom-hoop"));
    let facts = [
        texts::fill(
            "print-hoop-size",
            &[("hoop", hoop), ("size", size_line(pattern))],
        ),
        texts::fill(
            "print-counts",
            &[
                ("stitches", pattern.count(Command::Stitch).to_string()),
                ("colours", pattern.count(Command::ColorChange).to_string()),
                ("stops", pattern.count(Command::Stop).to_string()),
                ("trims", pattern.count(Command::Trim).to_string()),
            ],
        ),
        texts::fill(
            "print-time-thread",
            &[
                ("minutes", sewing_minutes(design, pattern).to_string()),
                ("metres", format!("{:.1}", e.needle_total() / 1000.0)),
            ],
        ),
    ];
    // A small picture at the top right.
    let thumb = 45.0;
    let (pic, _) = picture(
        pattern,
        sheets::stitch_items(pattern, false),
        (aw - thumb, 0.0, thumb, thumb),
    );
    items.extend(pic);
    for f in facts {
        items.push(text((0.0, y), 10.0, f));
        y += 5.5;
    }
    if !job.notes.trim().is_empty() {
        y += 1.0;
        let notes = texts::fill("print-notes", &[("notes", job.notes.trim().to_string())]);
        for l in wrap(&notes, 70) {
            items.push(text((0.0, y), 9.0, l));
            y += 4.5;
        }
    }
    y = y.max(thumb + 4.0) + 4.0;
    // The colour sequence table, continued on further pages when long.
    let cols = [0.0, 10.0, 18.0, 80.0, 115.0, 140.0, 165.0];
    let header = |y: f64, items: &mut Vec<Item>| {
        for (x, h) in cols.iter().zip([
            texts::text("print-step"),
            String::new(),
            texts::text("print-thread"),
            texts::text("print-brand"),
            texts::text("print-code"),
            texts::text("print-stitches"),
            texts::text("print-stops"),
        ]) {
            items.push(text((*x, y), 9.0, h));
        }
        items.push(Item::Line {
            a: (0.0, y + 1.5),
            b: (aw, y + 1.5),
            width: 0.2,
            rgb: GREY,
            dashed: false,
        });
    };
    let mut pages = Vec::new();
    header(y, &mut items);
    y += 6.5;
    for (i, st) in all.iter().enumerate() {
        if y > ah - 4.0 {
            pages.push(page_of(std::mem::take(&mut items), &title, paper));
            y = 4.0;
            header(y, &mut items);
            y += 6.5;
        }
        let (name, brand, code) = thread_label(st.thread.as_ref());
        items.push(text((cols[0], y), 9.0, format!("{}", i + 1)));
        items.push(swatch((cols[1], y), st.thread.as_ref()));
        items.push(text((cols[2], y), 9.0, name));
        items.push(text((cols[3], y), 9.0, brand));
        items.push(text((cols[4], y), 9.0, code));
        items.push(text((cols[5], y), 9.0, st.stitches.to_string()));
        items.push(text((cols[6], y), 9.0, st.stops.to_string()));
        y += 5.5;
    }
    pages.push(page_of(items, &title, paper));
    if detailed {
        pages.extend(step_thumbnails(pattern, &title, paper));
    }
    pages
}

/// Detailed operator sheet: one thumbnail per step, its stitches in colour over the rest of the
/// design in grey.
fn step_thumbnails(pattern: &StitchPattern, title: &str, paper: (f64, f64)) -> Vec<Page> {
    let (aw, ah) = area(paper);
    let blocks = pattern.color_blocks();
    let (cell_w, cell_h) = (aw / 3.0, 62.0);
    let per_page = 3 * ((ah / cell_h).floor() as usize).max(1);
    let grey = StitchPattern {
        stitches: pattern
            .stitches
            .iter()
            .filter(|s| s.command != Command::ColorChange)
            .copied()
            .collect(),
        ..Default::default()
    };
    let background: Vec<Item> = sheets::stitch_items(&grey, false)
        .into_iter()
        .map(|it| match it {
            Item::Polyline { points, width, .. } => Item::Polyline {
                points,
                width,
                rgb: LIGHT,
            },
            other => other,
        })
        .collect();
    let mut pages = Vec::new();
    let mut items = Vec::new();
    for (i, (t, block)) in blocks.iter().enumerate() {
        let k = i % per_page;
        if k == 0 && i > 0 {
            pages.push(page_of(std::mem::take(&mut items), title, paper));
        }
        let (x, y) = ((k % 3) as f64 * cell_w, (k / 3) as f64 * cell_h);
        let one = StitchPattern {
            threads: t.iter().map(|t| (*t).clone()).collect(),
            stitches: block.clone(),
            ..Default::default()
        };
        let mut drawn = background.clone();
        drawn.extend(sheets::stitch_items(&one, false));
        let (pic, _) = picture(
            pattern,
            drawn,
            (x + 2.0, y + 8.0, cell_w - 4.0, cell_h - 12.0),
        );
        let (name, _, code) = thread_label(*t);
        items.push(text(
            (x + 2.0, y + 4.0),
            9.0,
            format!("{}. {name} {code}", i + 1),
        ));
        items.extend(pic);
    }
    if !items.is_empty() {
        pages.push(page_of(items, title, paper));
    }
    pages
}

/// A job sheet for `design`: `Client` or `Operator`, an overview or `detailed`.
pub fn job_sheet(
    design: &Design,
    job: &Job,
    sheet: Sheet,
    detailed: bool,
    opts: &Options,
) -> Vec<Page> {
    let pattern = Compiler::for_design(design).compile(design);
    match sheet {
        Sheet::Client => client(design, &pattern, job, detailed, opts),
        Sheet::Operator => operator(design, &pattern, job, detailed, opts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_wrap_at_word_boundaries() {
        assert_eq!(
            wrap("one two three four", 9),
            vec!["one two".to_string(), "three".into(), "four".into()]
        );
        assert_eq!(wrap("a\nb", 9), vec!["a".to_string(), "b".into()]);
    }

    #[test]
    fn empty_job_fields_are_left_out() {
        let job = Job {
            customer: "Ann".into(),
            fabric: "Fleece".into(),
            ..Job::default()
        };
        assert_eq!(job_lines(&job, false), vec!["Customer: Ann".to_string()]);
        assert_eq!(
            job_lines(&job, true),
            vec!["Customer: Ann".to_string(), "Fabric: Fleece".into()]
        );
    }
}
