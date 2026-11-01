//! The ready-to-sew kit (`examples/sew`): calibration designs, machine checks and a design for
//! each feature, which Help ▸ Ready-to-Sew Designs opens. The release packages carry a copy (the
//! macOS bundle's `Resources/sew`, the Linux packages' `share/openstitch/sew`, `sew\` beside
//! `openstitch.exe` in the Windows zip and MSI); without one (the Flatpak, whose sandboxed files
//! the host's file manager cannot see), the menu opens the kit on GitHub.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The kit on GitHub: descriptions, previews and the files to download.
pub const ONLINE: &str = "https://github.com/cristianonescu/OpenStitch/tree/main/examples/sew";

/// The packaged kit for the editor at `exe`, if the package carries one (its `manifest.json`
/// marks it), as a plain path the file manager takes (no `..`, no Windows `\\?\` prefix).
pub fn bundled(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let kit = [
        dir.join("../Resources/sew"),
        dir.join("../share/openstitch/sew"),
        dir.join("sew"),
    ]
    .into_iter()
    .find(|p| p.join("manifest.json").is_file())?;
    let kit = std::fs::canonicalize(&kit).unwrap_or(kit);
    let s = kit.to_string_lossy();
    Some(
        s.strip_prefix(r"\\?\")
            .map_or_else(|| kit.clone(), PathBuf::from),
    )
}

/// Opens `folder` in the system's file manager (Explorer, Finder, the desktop's Files): a
/// browser would show a `file://` URL as a directory listing instead (egui's `open_url` always
/// starts the browser). The opener is not waited for; Explorer exits with 1 even when it opens.
pub fn open_folder(folder: &Path) -> std::io::Result<()> {
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener).arg(folder).spawn().map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_packaged_kit_beside_the_editor() {
        let dir = std::env::temp_dir().join(format!("openstitch-sew-kit-{}", std::process::id()));
        let exe = dir.join("Contents/MacOS/openstitch");
        std::fs::create_dir_all(dir.join("Contents/MacOS")).expect("temp dir");
        assert_eq!(bundled(&exe), None, "no kit packaged");
        std::fs::create_dir_all(dir.join("Contents/Resources/sew")).expect("temp dir");
        std::fs::write(dir.join("Contents/Resources/sew/manifest.json"), "{}").expect("temp dir");
        let kit = bundled(&exe).expect("the packaged kit");
        assert!(kit.ends_with("Contents/Resources/sew"), "{kit:?}");
        assert!(!kit.to_string_lossy().contains(".."), "{kit:?}");
        std::fs::remove_dir_all(&dir).expect("temp dir");
    }
}
