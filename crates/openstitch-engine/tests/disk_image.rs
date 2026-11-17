//! Floppy disk images (#876): `transfer.diskImage` and `transfer.diskImageList`. Every image is
//! built by the test; [`independent`] reads them back with its own FAT code, written from the
//! FAT specification and sharing nothing with `openstitch_formats::fat12`.

use openstitch_engine::Session;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn fails(s: &mut Session, id: &str, params: Value) -> String {
    match s.execute(id, Some(&params)) {
        Ok(v) => panic!("{id} should fail, gave {v}"),
        Err(e) => e.to_string(),
    }
}

/// A FAT12/16 root-directory reader taking every size from the boot sector: name → contents.
fn independent(image: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let u16_at = |at: usize| usize::from(image[at]) | usize::from(image[at + 1]) << 8;
    let bytes_per_sector = u16_at(11);
    let per_cluster = usize::from(image[13]);
    let reserved = u16_at(14);
    let fats = usize::from(image[16]);
    let root_entries = u16_at(17);
    let fat_sectors = u16_at(22);
    assert_eq!(&image[510..512], &[0x55, 0xAA], "boot signature");
    assert_eq!(u16_at(19) * bytes_per_sector, image.len(), "sector count");
    let fat = &image[reserved * bytes_per_sector..][..fat_sectors * bytes_per_sector];
    // Every FAT copy is the same.
    for k in 1..fats {
        let copy = &image[(reserved + k * fat_sectors) * bytes_per_sector..][..fat.len()];
        assert_eq!(copy, fat, "FAT copy {k}");
    }
    let next = |c: usize| {
        let pair = usize::from(fat[c * 3 / 2]) | usize::from(fat[c * 3 / 2 + 1]) << 8;
        if c & 1 == 1 { pair >> 4 } else { pair & 0x0FFF }
    };
    let root = (reserved + fats * fat_sectors) * bytes_per_sector;
    let data = root + root_entries * 32;
    let cluster_bytes = per_cluster * bytes_per_sector;
    let mut out = BTreeMap::new();
    for k in 0..root_entries {
        let e = &image[root + k * 32..root + k * 32 + 32];
        match e[0] {
            0 => break,
            0xE5 => continue,
            _ => {}
        }
        if e[11] & 0x18 != 0 {
            continue; // volume label or folder
        }
        let base = String::from_utf8_lossy(&e[..8]).trim_end().to_string();
        let ext = String::from_utf8_lossy(&e[8..11]).trim_end().to_string();
        let name = if ext.is_empty() {
            base
        } else {
            format!("{base}.{ext}")
        };
        let size = usize::from(e[28])
            | usize::from(e[29]) << 8
            | usize::from(e[30]) << 16
            | usize::from(e[31]) << 24;
        let mut cluster = usize::from(e[26]) | usize::from(e[27]) << 8;
        let mut contents = Vec::new();
        let mut hops = 0;
        while contents.len() < size {
            assert!((2..0xFF0).contains(&cluster), "{name}: cluster {cluster}");
            let at = data + (cluster - 2) * cluster_bytes;
            contents.extend_from_slice(&image[at..at + cluster_bytes]);
            cluster = next(cluster);
            hops += 1;
            assert!(hops <= 4096, "{name}: chain too long");
        }
        if size > 0 {
            assert!(cluster >= 0xFF8, "{name}: chain longer than its size");
        }
        contents.truncate(size);
        out.insert(name, contents);
    }
    out
}

fn design(s: &mut Session) {
    run(
        s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 30, "height": 20, "fill": "Red"}),
    );
    run(s, "design.rename", json!({"name": "Rose garden"}));
}

#[test]
fn a_new_image_holds_the_design_and_other_files_read_back_independently() {
    let tmp = openstitch_testutil::TempDir::new("floppy");
    let mut s = Session::new(tmp.path());
    design(&mut s);
    let extra = tmp.path().join("notes for the machine.txt");
    std::fs::write(&extra, b"hoop 100x100").expect("write");
    std::fs::write(tmp.path().join("empty.exp"), b"").expect("write");
    let r = run(
        &mut s,
        "transfer.diskImage",
        json!({"path": "disk.img", "addDesign": true, "add": [extra, "empty.exp"], "label": "Designs"}),
    );
    assert_eq!(r["created"], json!(true));
    let names: Vec<&str> = r["added"]
        .as_array()
        .expect("added")
        .iter()
        .map(|a| a["name"].as_str().expect("name"))
        .collect();
    // Long names get a clear short name; the design is written as DST by default.
    assert_eq!(names, ["NOTESF~1.TXT", "EMPTY.EXP", "ROSEGA~1.DST"]);
    let image = std::fs::read(tmp.path().join("disk.img")).expect("image");
    assert_eq!(image.len(), 1_474_560);
    let files = independent(&image);
    assert_eq!(files["NOTESF~1.TXT"], b"hoop 100x100");
    assert_eq!(files["EMPTY.EXP"], b"");
    // The design on the disk is the file design.export writes.
    run(&mut s, "design.export", json!({"path": "Rose garden.dst"}));
    let exported = std::fs::read(tmp.path().join("Rose garden.dst")).expect("dst");
    // The same stitches as design.export writes; the 512-byte DST header's label is the design
    // name (design.export takes the file name).
    let on_disk = &files["ROSEGA~1.DST"];
    assert_eq!(on_disk.len(), exported.len());
    assert!(on_disk[512..] == exported[512..], "stitch records differ");
    assert!(on_disk.starts_with(b"LA:Rose garden "), "label");
    let list = run(
        &mut s,
        "transfer.diskImageList",
        json!({"path": "disk.img"}),
    );
    assert_eq!(list["label"], json!("DESIGNS"));
    assert_eq!(list["files"].as_array().map(Vec::len), Some(3));
    let used = exported.len().div_ceil(512) + 1;
    assert_eq!(list["free"], json!((2_847 - used) * 512));
    assert_eq!(list["capacity"], json!(2_847 * 512));
}

#[test]
fn a_name_on_the_disk_is_refused_unless_overwrite_and_failures_change_nothing() {
    let tmp = openstitch_testutil::TempDir::new("floppy");
    let mut s = Session::new(tmp.path());
    design(&mut s);
    let img = tmp.path().join("disk.img");
    run(
        &mut s,
        "transfer.diskImage",
        json!({"path": "disk.img", "addDesign": true, "name": "rose"}),
    );
    let before = std::fs::read(&img).expect("image");
    std::fs::write(tmp.path().join("other.dst"), b"other").expect("write");
    // One step adds a new file and repeats the design's name: refused as a whole.
    let e = fails(
        &mut s,
        "transfer.diskImage",
        json!({"path": "disk.img", "add": ["other.dst"], "addDesign": true, "name": "rose"}),
    );
    assert!(e.contains("ROSE.DST is already on the disk"), "{e}");
    assert_eq!(std::fs::read(&img).expect("image"), before, "unchanged");
    // A missing input is refused before anything changes.
    let e = fails(
        &mut s,
        "transfer.diskImage",
        json!({"path": "disk.img", "add": ["missing.dst"]}),
    );
    assert!(e.contains("missing.dst"), "{e}");
    assert_eq!(std::fs::read(&img).expect("image"), before);
    // With overwrite the file is replaced; remove takes a file off.
    run(
        &mut s,
        "transfer.diskImage",
        json!({"path": "disk.img", "add": ["other.dst"], "addDesign": true, "name": "rose", "overwrite": true}),
    );
    let files = independent(&std::fs::read(&img).expect("image"));
    assert_eq!(files.len(), 2);
    let r = run(
        &mut s,
        "transfer.diskImage",
        json!({"path": "disk.img", "remove": ["rose.dst"]}),
    );
    assert_eq!(r["removed"], json!(["rose.dst"]));
    let files = independent(&std::fs::read(&img).expect("image"));
    assert_eq!(files.keys().collect::<Vec<_>>(), ["OTHER.DST"]);
    let e = fails(
        &mut s,
        "transfer.diskImage",
        json!({"path": "disk.img", "remove": ["rose.dst"]}),
    );
    assert!(e.contains("not on the disk"), "{e}");
}

#[test]
fn a_full_disk_is_refused_and_left_as_it_was() {
    let tmp = openstitch_testutil::TempDir::new("floppy");
    let mut s = Session::new(tmp.path());
    std::fs::write(tmp.path().join("big.bin"), vec![9u8; 2_847 * 512]).expect("write");
    std::fs::write(tmp.path().join("one.bin"), b"1").expect("write");
    run(
        &mut s,
        "transfer.diskImage",
        json!({"path": "full.img", "add": ["big.bin"]}),
    );
    let img = tmp.path().join("full.img");
    let before = std::fs::read(&img).expect("image");
    assert_eq!(independent(&before)["BIG.BIN"].len(), 2_847 * 512);
    assert_eq!(
        run(
            &mut s,
            "transfer.diskImageList",
            json!({"path": "full.img"})
        )["free"],
        json!(0)
    );
    let e = fails(
        &mut s,
        "transfer.diskImage",
        json!({"path": "full.img", "add": ["one.bin"]}),
    );
    assert!(e.contains("full"), "{e}");
    assert_eq!(std::fs::read(&img).expect("image"), before);
}

#[test]
fn only_image_files_are_written_never_a_drive_device_or_other_file() {
    let tmp = openstitch_testutil::TempDir::new("floppy");
    let mut s = Session::new(tmp.path());
    for path in [
        "/dev/disk2",
        "/dev/fd0",
        "\\\\.\\A:",
        "\\\\?\\Volume{0}\\",
        "A:",
        "A:\\",
        "/Volumes/FLOPPY",
        "/Volumes/FLOPPY/",
        "designs/",
        "disk.dst",
        "disk",
    ] {
        let e = fails(&mut s, "transfer.diskImage", json!({"path": path}));
        assert!(e.contains("image file"), "{path}: {e}");
    }
    // A folder named like an image is refused.
    std::fs::create_dir(tmp.path().join("folder.img")).expect("dir");
    let e = fails(&mut s, "transfer.diskImage", json!({"path": "folder.img"}));
    assert!(e.contains("folder"), "{e}");
    // A file at the path that is not a floppy image is left alone, never overwritten.
    std::fs::write(tmp.path().join("photo.img"), b"not a floppy").expect("write");
    let e = fails(&mut s, "transfer.diskImage", json!({"path": "photo.img"}));
    assert!(e.contains("not a 1.44 MB FAT12 floppy image"), "{e}");
    assert_eq!(
        std::fs::read(tmp.path().join("photo.img")).expect("file"),
        b"not a floppy"
    );
    let e = fails(
        &mut s,
        "transfer.diskImageList",
        json!({"path": "photo.img"}),
    );
    assert!(e.contains("not a 1.44 MB"), "{e}");
}

#[test]
fn a_damaged_image_is_reported_not_read() {
    let tmp = openstitch_testutil::TempDir::new("floppy");
    let mut s = Session::new(tmp.path());
    std::fs::write(tmp.path().join("a.bin"), vec![1u8; 1_500]).expect("write");
    run(
        &mut s,
        "transfer.diskImage",
        json!({"path": "d.img", "add": ["a.bin"]}),
    );
    let img = tmp.path().join("d.img");
    let mut b = std::fs::read(&img).expect("image");
    // Cluster 3 points back to cluster 2: a loop (FAT entry 3 is the high 12 bits at 512+4).
    b[512 + 4] = (b[512 + 4] & 0x0F) | 0x20;
    b[512 + 5] = 0x00;
    std::fs::write(&img, &b).expect("write");
    let e = fails(&mut s, "transfer.diskImageList", json!({"path": "d.img"}));
    assert!(e.contains("damaged"), "{e}");
    let e = fails(
        &mut s,
        "transfer.diskImage",
        json!({"path": "d.img", "remove": ["A.BIN"]}),
    );
    assert!(e.contains("damaged"), "{e}");
    assert_eq!(std::fs::read(&img).expect("image"), b, "left as it was");
    // A truncated image.
    std::fs::write(&img, &b[..100_000]).expect("write");
    let e = fails(&mut s, "transfer.diskImageList", json!({"path": "d.img"}));
    assert!(e.contains("1.44 MB"), "{e}");
}

/// The help says it writes an image file only, never a drive, and the command is a file write
/// the Assistant asks before.
#[test]
fn the_help_says_it_writes_an_image_file_only() {
    let s = Session::new(Path::new("."));
    let spec = s
        .commands()
        .into_iter()
        .find(|c| c.id == "transfer.diskImage")
        .expect("registered");
    assert_eq!(spec.effect, openstitch_engine::Effect::File);
    assert!(
        spec.summary.contains("never a drive or device"),
        "{}",
        spec.summary
    );
    let list = s
        .commands()
        .into_iter()
        .find(|c| c.id == "transfer.diskImageList")
        .expect("registered");
    assert_eq!(list.effect, openstitch_engine::Effect::Read);
}

/// A client without file access (`--listen-no-files`, the MCP server's `--no-files`) is refused
/// both commands and is not offered them.
#[test]
fn clients_without_file_access_cannot_make_or_read_images() {
    use openstitch_engine::Capabilities;
    let tmp = openstitch_testutil::TempDir::new("floppy");
    let mut s = Session::new(tmp.path());
    s.set_capabilities(Capabilities::FULL.without_files());
    for id in ["transfer.diskImage", "transfer.diskImageList"] {
        let e = fails(&mut s, id, json!({"path": "disk.img"}));
        assert!(e.contains("not permitted"), "{id}: {e}");
        assert!(
            !s.permitted_commands().iter().any(|c| c.id == id),
            "{id} offered"
        );
    }
    assert!(!tmp.path().join("disk.img").exists(), "nothing written");
    // Read-only clients may list an image but not change one.
    s.set_capabilities(Capabilities::FULL);
    run(&mut s, "transfer.diskImage", json!({"path": "disk.img"}));
    s.set_capabilities(Capabilities::FULL.read_only());
    run(
        &mut s,
        "transfer.diskImageList",
        json!({"path": "disk.img"}),
    );
    let e = fails(&mut s, "transfer.diskImage", json!({"path": "disk.img"}));
    assert!(e.contains("not permitted"), "{e}");
}
