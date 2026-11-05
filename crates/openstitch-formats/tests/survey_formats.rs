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

/// Every needle or frame position in order (stitches and jumps), relative to the first: VP3
/// has no jump record, so a reader may call a move either. A run of jumps is one frame move
/// (the writers fold it to its end, `layout::events`), so only its last point counts.
fn path(stitches: &[Stitch]) -> Vec<[f64; 2]> {
    let moves: Vec<&Stitch> = stitches
        .iter()
        .filter(|s| matches!(s.command, Command::Stitch | Command::Jump))
        .collect();
    let mut v: Vec<[f64; 2]> = moves
        .iter()
        .enumerate()
        .filter(|(i, s)| {
            !(s.command == Command::Jump
                && moves.get(i + 1).is_some_and(|n| n.command == Command::Jump))
        })
        .map(|(_, s)| [s.x, s.y])
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

/// The needle points in order, repeats folded, relative to the first.
fn needles(stitches: &[Stitch]) -> Vec<[f64; 2]> {
    let mut v: Vec<[f64; 2]> = stitches
        .iter()
        .filter(|s| s.command == Command::Stitch)
        .map(|s| [s.x, s.y])
        .collect();
    v.dedup();
    let first = v.first().copied().unwrap_or_default();
    v.iter()
        .map(|q| [q[0] - first[0], q[1] - first[1]])
        .collect()
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

// MARK: More machine formats (#481)

/// Our read of pyembroidery's (or the assembled) `oracle.<ext>` against pyembroidery's read
/// of the same file: needle path, trims, colour changes, stops and colours.
fn reads_like_pyembroidery(ty: FileType) {
    let ext = ty.extension();
    let expected = oracle(&format!("oracle_{ext}_read.json"));
    let mut ours = ty.read(&data(&format!("oracle.{ext}"))).expect("read");
    if ty == FileType::Ksm {
        // pyembroidery's KSM reader leaves zero-length records out; this reader keeps them
        // (a zero-length stitch is still a needle drop). Compare without them.
        let mut at = (0.0, 0.0);
        ours.stitches.retain(|s| {
            let moved = (s.x, s.y) != at;
            at = (s.x, s.y);
            moved || !matches!(s.command, Command::Stitch | Command::Jump)
        });
    }
    assert_same_path(&path(&ours.stitches), &path(&expected.stitches), ext);
    for c in [
        Command::Trim,
        Command::ColorChange,
        Command::Stop,
        Command::Jump,
    ] {
        assert_eq!(
            count(&ours.stitches, c),
            count(&expected.stitches, c),
            "{ext} {c:?}"
        );
    }
    let hexes = |t: &[Thread]| t.iter().map(Thread::hex).collect::<Vec<_>>();
    assert_eq!(hexes(&ours.threads), hexes(&expected.threads), "{ext}");
}

#[test]
fn new_readers_read_their_oracle_files_like_pyembroidery() {
    for ty in [
        FileType::Tap,
        FileType::U01,
        FileType::Tbf,
        FileType::Dsb,
        FileType::Dsz,
        FileType::Ksm,
        FileType::Zxy,
        FileType::Stx,
    ] {
        reads_like_pyembroidery(ty);
    }
}

#[test]
fn new_readers_read_the_oracle_design() {
    // The design's 12 colours (11 changes) and 12 trims, whichever way each file holds them.
    let design = survey_design();
    for ty in [
        FileType::Tap,
        FileType::U01,
        FileType::Tbf,
        FileType::Dsb,
        FileType::Dsz,
        FileType::Ksm,
        FileType::Zxy,
        FileType::Stx,
    ] {
        let ours = ty
            .read(&data(&format!("oracle.{}", ty.extension())))
            .expect("read");
        assert_eq!(
            count(&ours.stitches, Command::ColorChange),
            count(&design.stitches, Command::ColorChange),
            "{ty}"
        );
        // ZXY has no trim record: its trimmed moves are plain jump runs.
        if ty != FileType::Zxy {
            assert!(count(&ours.stitches, Command::Trim) >= 12, "{ty}");
        }
        let sewn = |s: &[Stitch]| {
            let mut v: Vec<[f64; 2]> = s
                .iter()
                .filter(|s| s.command == Command::Stitch)
                .map(|s| [s.x, s.y])
                .collect();
            v.dedup();
            v
        };
        // Every needle point of the design is sewn, at the same place relative to the first.
        let (ours, theirs) = (sewn(&ours.stitches), sewn(&design.stitches));
        let o = ours[0];
        let t = theirs[0];
        for p in &theirs {
            let want = [p[0] - t[0] + o[0], p[1] - t[1] + o[1]];
            assert!(
                ours.iter()
                    .any(|q| (q[0] - want[0]).abs() <= 1.0 && (q[1] - want[1]).abs() <= 1.0),
                "{ty}: needle point {p:?} missing"
            );
        }
    }
}

#[test]
fn new_writers_write_files_pyembroidery_reads_like_the_design() {
    let design = survey_design();
    for ty in [FileType::Tap, FileType::U01, FileType::Tbf] {
        let ext = ty.extension();
        let bytes = ty.write(&design).expect("write");
        assert_golden(&format!("openstitch.{ext}"), &bytes);
        let theirs = oracle(&format!("openstitch_{ext}_read.json"));
        let ours = ty.read(&bytes).expect("read");
        assert_same_path(&path(&theirs.stitches), &path(&ours.stitches), ext);
        // Against the design: the needle points (a stitch longer than a record is a jump and
        // a stitch in these formats, one stitch in the design).
        assert_same_path(&needles(&ours.stitches), &needles(&design.stitches), ext);
        for c in [Command::Trim, Command::ColorChange] {
            assert_eq!(
                count(&theirs.stitches, c),
                count(&ours.stitches, c),
                "{ext} {c:?}"
            );
        }
        assert_eq!(
            count(&ours.stitches, Command::ColorChange),
            count(&design.stitches, Command::ColorChange),
            "{ext}"
        );
        // TAP, like DST, ends with a jump run back to the start, which reads as a trim.
        let end_move = usize::from(ty == FileType::Tap);
        assert_eq!(
            count(&ours.stitches, Command::Trim),
            count(&design.stitches, Command::Trim) + end_move,
            "{ext}"
        );
    }
    // TBF keeps the colours; TAP and U01 hold none.
    let theirs = oracle("openstitch_tbf_read.json");
    let hexes = |t: &[Thread]| t.iter().map(Thread::hex).collect::<Vec<_>>();
    assert_eq!(hexes(&theirs.threads), hexes(&design.threads));
}

#[test]
fn u01_and_tbf_round_trip_stops_and_reuse_a_needle_for_a_colour() {
    let mut p = StitchPattern {
        threads: vec![
            Thread::rgb(0xFF0000),
            Thread::rgb(0x00FF00),
            Thread::rgb(0xFF0000),
        ],
        ..StitchPattern::new()
    };
    p.stitch_to(0.0, 0.0);
    p.stitch_to(10.0, 0.0);
    p.push(Command::Stop);
    p.stitch_to(20.0, 0.0);
    p.push(Command::ColorChange);
    p.stitch_to(30.0, 0.0);
    p.push(Command::ColorChange);
    p.stitch_to(40.0, 0.0);
    p.push(Command::End);
    for ty in [FileType::U01, FileType::Tbf] {
        let bytes = ty.write(&p).expect("write");
        let back = ty.read(&bytes).expect("read");
        assert_eq!(count(&back.stitches, Command::Stop), 1, "{ty}");
        assert_eq!(count(&back.stitches, Command::ColorChange), 2, "{ty}");
        assert_eq!(path(&back.stitches), path(&p.stitches), "{ty}");
    }
    // U01 needle records: needle 1, then 2, then 1 again for the red.
    let u01 = openstitch_formats::u01::write(&p).expect("u01");
    let needles: Vec<u8> = u01[openstitch_formats::u01::HEADER_SIZE..]
        .as_chunks::<3>()
        .0
        .iter()
        .map(|r| r[0] & 0x1F)
        .filter(|low| (0x09..=0x17).contains(low))
        .map(|low| low - 0x08)
        .collect();
    assert_eq!(needles, [1, 2, 1]);
    // TBF: the needle order and the colour table.
    let tbf = openstitch_formats::tbf::write(&p).expect("tbf");
    assert_eq!(&tbf[0x10A..0x10D], &[1, 2, 1]);
    assert_eq!(&tbf[0x20B..0x20E], b"DA:");
    assert_eq!(&tbf[0x20E..0x213], &[0x45, 0xFF, 0, 0, 0x20]);
    // The 0x600-byte header, whole records and the closing 1A.
    assert_eq!((tbf.len() - 0x600) % 3, 1);
    assert_eq!(tbf.last(), Some(&0x1A));
}

#[test]
fn read_only_formats_refuse_to_write() {
    for ty in [
        FileType::Dsb,
        FileType::Dsz,
        FileType::Ksm,
        FileType::Zxy,
        FileType::Stx,
    ] {
        assert!(!ty.can_write());
        assert!(ty.write(&survey_design()).is_err());
    }
    for ty in [FileType::Tap, FileType::Tbf, FileType::U01] {
        assert!(ty.can_write());
    }
}

#[test]
fn new_readers_reject_short_files_and_never_panic() {
    for ty in [
        FileType::Tap,
        FileType::U01,
        FileType::Tbf,
        FileType::Dsb,
        FileType::Dsz,
        FileType::Ksm,
        FileType::Zxy,
        FileType::Stx,
    ] {
        let good = data(&format!("oracle.{}", ty.extension()));
        for n in (0..good.len()).step_by(7) {
            let _ = ty.read(&good[..n]);
        }
        let mut bad = good.clone();
        for i in (0..bad.len()).step_by(3) {
            bad[i] ^= 0x5A;
            let _ = ty.read(&bad);
            bad[i] ^= 0x5A;
        }
        // Shorter than the header is an error (TAP has none).
        let header = match ty {
            FileType::Tap => 0,
            FileType::Zxy => 3,
            FileType::Stx => 24,
            _ => 101,
        };
        if header > 0 {
            assert!(ty.read(&good[..header - 1]).is_err(), "{ty}");
        }
    }
}

// MARK: Thread lists (#482)

#[test]
fn thread_lists_read_pyembroidery_files_and_pyembroidery_reads_ours() {
    use openstitch_formats::threadlist::ListType;
    let design = survey_design();
    let hexes = |t: &[Thread]| t.iter().map(Thread::hex).collect::<Vec<_>>();
    for ty in [ListType::Inf, ListType::Edr, ListType::Col] {
        let ext = ty.extension();
        // pyembroidery's file, read here.
        let ours = ty.read(&data(&format!("oracle.{ext}"))).expect("read");
        let theirs = oracle(&format!("oracle_{ext}_read.json"));
        assert_eq!(hexes(&ours), hexes(&theirs.threads), "{ext}");
        assert_eq!(hexes(&ours), hexes(&design.threads), "{ext}");
        // This writer's file, read by pyembroidery.
        let bytes = ty.write(&design.threads).expect("write");
        assert_golden(&format!("openstitch.{ext}"), &bytes);
        let theirs = oracle(&format!("openstitch_{ext}_read.json"));
        assert_eq!(hexes(&theirs.threads), hexes(&design.threads), "{ext}");
        if ty == ListType::Inf {
            let names = |t: &[Thread]| t.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
            assert_eq!(names(&ours), names(&design.threads));
            assert_eq!(names(&theirs.threads), names(&design.threads));
        }
    }
}
