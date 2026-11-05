//! Formats from the survey batch (#480 VP3), checked both ways against pyembroidery 1.5.1:
//! its files read by OpenStitch, and OpenStitch's files read by it
//! (`tools/genfixtures_survey.py` writes the oracle files and reads ours).

mod common;

use common::{assert_golden, data};
use openstitch_core::{Command, Stitch, StitchPattern, Thread};
use openstitch_formats::{FileType, vp3};

/// What pyembroidery wrote or read: threads with their strings, and the stitch stream.
struct Oracle {
    threads: Vec<Thread>,
    stitches: Vec<Stitch>,
}

fn oracle(name: &str) -> Oracle {
    let v: serde_json::Value = serde_json::from_slice(&data(name)).expect("fixture JSON");
    let text = |t: &serde_json::Value, key: &str| {
        t[key]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_owned().into())
    };
    let threads = v["threads"]
        .as_array()
        .expect("threads")
        .iter()
        .map(|t| {
            let mut thread = Thread::from_hex(t["hex"].as_str().expect("hex")).expect("hex");
            thread.name = text(t, "name");
            thread.catalog_number = text(t, "catalog");
            thread.brand = text(t, "brand");
            thread
        })
        .collect();
    let stitches = v["stitches"]
        .as_array()
        .expect("stitches")
        .iter()
        .map(|row| {
            let command: Command = serde_json::from_value(row[2].clone()).expect("command");
            Stitch::new(
                row[0].as_f64().expect("x"),
                row[1].as_f64().expect("y"),
                command,
            )
        })
        .collect();
    Oracle { threads, stitches }
}

/// The survey test design as OpenStitch holds it.
fn survey_design() -> StitchPattern {
    let o = oracle("survey_input.json");
    StitchPattern {
        stitches: o.stitches,
        threads: o.threads,
        ..StitchPattern::new()
    }
}

/// Every needle or frame position in order (stitches and jumps), relative to the first,
/// repeats folded: VP3 has no jump record, so a reader may call a move either.
fn path(stitches: &[Stitch]) -> Vec<[f64; 2]> {
    let mut v: Vec<[f64; 2]> = stitches
        .iter()
        .filter(|s| matches!(s.command, Command::Stitch | Command::Jump))
        .map(|s| [s.x, s.y])
        .collect();
    // Fold moves of at most 0.1 mm: pyembroidery's writer can store a colour block's start
    // half a unit off, which its reader turns into a 0.1 mm jump and this reader rounds away.
    v.dedup_by(|b, a| (a[0] - b[0]).abs() <= 1.0 && (a[1] - b[1]).abs() <= 1.0);
    let first = v.first().copied().unwrap_or_default();
    v.iter()
        .map(|q| [q[0] - first[0], q[1] - first[1]])
        .collect()
}

/// Asserts two paths agree within ±0.1 mm (1 unit) at every point.
fn assert_same_path(ours: &[[f64; 2]], theirs: &[[f64; 2]], what: &str) {
    assert_eq!(ours.len(), theirs.len(), "{what}: point count");
    for (i, (a, b)) in ours.iter().zip(theirs).enumerate() {
        assert!(
            (a[0] - b[0]).abs() <= 1.0 && (a[1] - b[1]).abs() <= 1.0,
            "{what}: point {i} {a:?} vs {b:?}"
        );
    }
}

fn count(stitches: &[Stitch], command: Command) -> usize {
    stitches.iter().filter(|s| s.command == command).count()
}

/// RGB, brand, catalogue number and name of a thread.
type ThreadStrings = (String, Option<String>, Option<String>, Option<String>);

/// RGB, brand, catalogue number and name of each thread.
fn strings(threads: &[Thread]) -> Vec<ThreadStrings> {
    threads
        .iter()
        .map(|t| {
            (
                t.hex(),
                t.brand.as_deref().map(str::to_owned),
                t.catalog_number.as_deref().map(str::to_owned),
                t.name.as_deref().map(str::to_owned),
            )
        })
        .collect()
}

// MARK: VP3 (#480)

#[test]
fn vp3_reads_pyembroidery_file_like_pyembroidery() {
    let expected = oracle("oracle_vp3_read.json");
    let ours = vp3::read(&data("oracle.vp3")).expect("read");
    assert_same_path(
        &path(&ours.stitches),
        &path(&expected.stitches),
        "oracle.vp3",
    );
    for c in [Command::Trim, Command::ColorChange, Command::End] {
        assert_eq!(
            count(&ours.stitches, c),
            count(&expected.stitches, c),
            "{c:?}"
        );
    }
    assert_eq!(strings(&ours.threads), strings(&expected.threads));
    assert_eq!(ours.threads.len(), 12);
}

#[test]
fn vp3_reads_pyembroidery_file_of_a_real_design_like_pyembroidery() {
    let expected = oracle("oracle_kit_vp3_read.json");
    let ours = vp3::read(&data("oracle_kit.vp3")).expect("read");
    assert_same_path(&path(&ours.stitches), &path(&expected.stitches), "kit");
    for c in [Command::Trim, Command::ColorChange] {
        assert_eq!(
            count(&ours.stitches, c),
            count(&expected.stitches, c),
            "{c:?}"
        );
    }
    let hexes = |t: &[Thread]| t.iter().map(Thread::hex).collect::<Vec<_>>();
    assert_eq!(hexes(&ours.threads), hexes(&expected.threads));
    // And like the PES it was written from.
    let pes = openstitch_formats::pes::read(&data("machine-formats.pes")).expect("pes");
    assert_eq!(hexes(&ours.threads), hexes(&pes.threads));
    assert_eq!(
        count(&ours.stitches, Command::ColorChange),
        pes.count(Command::ColorChange)
    );
}

#[test]
fn vp3_reads_pyembroidery_file_like_the_design_it_was_written_from() {
    let design = survey_design();
    let ours = vp3::read(&data("oracle.vp3")).expect("read");
    assert_same_path(&path(&ours.stitches), &path(&design.stitches), "design");
    assert_eq!(
        count(&ours.stitches, Command::ColorChange),
        count(&design.stitches, Command::ColorChange)
    );
    // Every trimmed 30 mm move comes back as a trim and a jump; the 20 mm stitches as stitches.
    assert_eq!(
        count(&ours.stitches, Command::Jump),
        count(&design.stitches, Command::Jump)
    );
    // pyembroidery leaves out the zero-length stitch on each landing, which `path` folds.
    assert_eq!(strings(&ours.threads), strings(&design.threads));
}

#[test]
fn vp3_written_by_openstitch_reads_in_pyembroidery_like_the_design() {
    let design = survey_design();
    let bytes = vp3::write(&design).expect("write");
    assert_golden("openstitch.vp3", &bytes);
    let theirs = oracle("openstitch_vp3_read.json");
    assert_same_path(
        &path(&theirs.stitches),
        &path(&design.stitches),
        "pyembroidery",
    );
    assert_eq!(
        count(&theirs.stitches, Command::ColorChange),
        count(&design.stitches, Command::ColorChange)
    );
    // The design's trims and the one VP3 ends with.
    assert_eq!(
        count(&theirs.stitches, Command::Trim),
        count(&design.stitches, Command::Trim) + 1
    );
    assert_eq!(strings(&theirs.threads), strings(&design.threads));
}

#[test]
fn vp3_round_trips_stitches_commands_and_thread_strings() {
    let design = survey_design();
    let back = FileType::Vp3
        .read(&FileType::Vp3.write(&design).expect("write"))
        .expect("read");
    assert_same_path(&path(&back.stitches), &path(&design.stitches), "round trip");
    assert_eq!(
        count(&back.stitches, Command::ColorChange),
        count(&design.stitches, Command::ColorChange)
    );
    // The first move, from the centre to the first stitch, and every trimmed move.
    assert_eq!(
        count(&back.stitches, Command::Jump),
        count(&design.stitches, Command::Jump) + 1
    );
    assert_eq!(
        count(&back.stitches, Command::Stitch),
        count(&design.stitches, Command::Stitch)
    );
    assert_eq!(
        count(&back.stitches, Command::Trim),
        count(&design.stitches, Command::Trim) + 1
    );
    assert_eq!(strings(&back.threads), strings(&design.threads));
}

#[test]
fn vp3_writes_a_stop_as_a_block_of_the_same_thread() {
    let mut p = StitchPattern {
        threads: vec![Thread::rgb(0xFF0000), Thread::rgb(0x00FF00)],
        ..StitchPattern::new()
    };
    p.stitch_to(0.0, 0.0);
    p.stitch_to(10.0, 0.0);
    p.push(Command::Stop);
    p.stitch_to(20.0, 0.0);
    p.push(Command::ColorChange);
    p.stitch_to(30.0, 0.0);
    p.push(Command::End);
    let back = vp3::read(&vp3::write(&p).expect("write")).expect("read");
    let hexes: Vec<String> = back.threads.iter().map(Thread::hex).collect();
    assert_eq!(hexes, ["#ff0000", "#ff0000", "#00ff00"]);
    assert_eq!(count(&back.stitches, Command::ColorChange), 2);
}

#[test]
fn vp3_writes_an_empty_design() {
    let p = StitchPattern {
        threads: vec![Thread::rgb(0)],
        ..StitchPattern::new()
    };
    let back = vp3::read(&vp3::write(&p).expect("write")).expect("read");
    assert_eq!(count(&back.stitches, Command::Stitch), 0);
    assert_eq!(back.threads.len(), 1);
}
