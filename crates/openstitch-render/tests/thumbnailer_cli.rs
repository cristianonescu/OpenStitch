//! Pins `openstitch-thumbnailer`'s command line, the shared interface of the file-manager
//! thumbnail handlers (docs/DISTRIBUTION.md "Shared thumbnail interface"): the Linux
//! thumbnailer entry, the macOS Quick Look extension (#557) and the Windows Explorer handler
//! (#669) call it. Changing a flag here breaks them; add flags, never rename or drop one.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn kit_design() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/sew/machine-checks/machine-formats.pes")
}

fn thumbnailer(args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_openstitch-thumbnailer"))
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run openstitch-thumbnailer");
    if let Some(bytes) = stdin {
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(bytes)
            .expect("write stdin");
    }
    child.wait_with_output().expect("wait")
}

/// A PNG's width and height from its IHDR chunk.
fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "a PNG");
    let be = |i: usize| u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]);
    (be(16), be(20))
}

#[test]
fn files_form_size_flag_and_default() {
    let dir = openstitch_testutil::TempDir::new("thumbnailer-cli-files");
    let design = kit_design();
    let design = design.to_str().expect("utf-8 path");
    for (args, side) in [
        (vec!["-s", "64"], 64),
        (vec!["--size", "96"], 96),
        (vec![], 256),
    ] {
        let out = dir.join(format!("t{side}.png"));
        let mut all = args.clone();
        all.extend([design, out.to_str().expect("utf-8 path")]);
        let o = thumbnailer(&all, None);
        assert!(o.status.success(), "{all:?}: {o:?}");
        assert_eq!(png_size(&std::fs::read(&out).expect("png")), (side, side));
    }
}

#[test]
fn stdin_form_writes_the_same_png_to_stdout() {
    let dir = openstitch_testutil::TempDir::new("thumbnailer-cli-stdin");
    let bytes = std::fs::read(kit_design()).expect("design");
    let o = thumbnailer(&["-s", "128", "--stdin", "pes"], Some(&bytes));
    assert!(o.status.success(), "{o:?}");
    assert_eq!(png_size(&o.stdout), (128, 128));
    // The same picture as the files form.
    let file = dir.join("f.png");
    let design = kit_design();
    let f = thumbnailer(
        &[
            "-s",
            "128",
            design.to_str().expect("utf-8"),
            file.to_str().expect("utf-8"),
        ],
        None,
    );
    assert!(f.status.success(), "{f:?}");
    assert_eq!(o.stdout, std::fs::read(&file).expect("png"));
    // The extension's case and a leading dot do not matter.
    let o = thumbnailer(&["--stdin", ".PES"], Some(&bytes));
    assert_eq!(png_size(&o.stdout), (256, 256));
}

#[test]
fn exit_codes_failures_and_usage() {
    // A malformed file: exit 1, a message on stderr, nothing on stdout.
    let o = thumbnailer(&["--stdin", "pes"], Some(b"#PES0001 not a design"));
    assert_eq!(o.status.code(), Some(1), "{o:?}");
    assert!(o.stdout.is_empty());
    assert!(String::from_utf8_lossy(&o.stderr).starts_with("openstitch-thumbnailer: "));
    // An unknown extension fails the same way.
    let o = thumbnailer(&["--stdin", "txt"], Some(b"hello"));
    assert_eq!(o.status.code(), Some(1), "{o:?}");
    // Usage errors: exit 2.
    for args in [
        &["--stdin"][..],
        &["--stdin", "pes", "extra"],
        &["-s", "0", "a.pes", "b.png"],
        &["only-one"],
    ] {
        assert_eq!(thumbnailer(args, None).status.code(), Some(2), "{args:?}");
    }
}
