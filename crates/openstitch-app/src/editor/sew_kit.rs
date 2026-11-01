//! The ready-to-sew kit (`examples/sew`): calibration designs, machine checks and a design for
//! each feature, which Help ▸ Ready-to-Sew Designs opens. The release packages carry a copy (the
//! macOS bundle's `Resources/sew`, the Linux packages' `share/openstitch/sew`, `sew\` beside
//! `openstitch.exe` in the Windows zip); without one, the menu opens the kit on GitHub.

use std::path::{Path, PathBuf};

/// The kit on GitHub: descriptions, previews and the files to download.
pub const ONLINE: &str = "https://github.com/cristianonescu/OpenStitch/tree/main/examples/sew";

/// The packaged kit for the editor at `exe`, if the package carries one (its `manifest.json`
/// marks it).
pub fn bundled(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    [
        dir.join("../Resources/sew"),
        dir.join("../share/openstitch/sew"),
        dir.join("sew"),
    ]
    .into_iter()
    .find(|p| p.join("manifest.json").is_file())
}

/// What Help ▸ Ready-to-Sew Designs opens: the packaged kit as a `file://` URL (the system shows
/// the folder), else [`ONLINE`].
pub fn target(exe: Option<&Path>) -> String {
    exe.and_then(bundled)
        .map_or_else(|| ONLINE.to_string(), |p| file_url(&p))
}

/// A `file://` URL for an absolute path, with the characters a URL cannot hold escaped
/// (`C:\Program Files\…` becomes `file:///C:/Program%20Files/…`).
fn file_url(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let mut url = String::from(if s.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                url.push(char::from(b))
            }
            _ => url.push_str(&format!("%{b:02X}")),
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_packaged_kit_beside_the_editor_and_falls_back_to_github() {
        let dir = std::env::temp_dir().join(format!("openstitch-sew-kit-{}", std::process::id()));
        let exe = dir.join("Contents/MacOS/openstitch");
        std::fs::create_dir_all(dir.join("Contents/MacOS")).expect("temp dir");
        assert_eq!(target(Some(&exe)), ONLINE, "no kit packaged");
        std::fs::create_dir_all(dir.join("Contents/Resources/sew")).expect("temp dir");
        std::fs::write(dir.join("Contents/Resources/sew/manifest.json"), "{}").expect("temp dir");
        let url = target(Some(&exe));
        assert!(
            url.starts_with("file://") && url.ends_with("Contents/MacOS/../Resources/sew"),
            "{url}"
        );
        std::fs::remove_dir_all(&dir).expect("temp dir");
        assert_eq!(target(None), ONLINE);
    }

    #[test]
    fn file_urls_escape_spaces_and_take_windows_paths() {
        assert_eq!(
            file_url(Path::new(r"C:\Program Files\OpenStitch\sew")),
            "file:///C:/Program%20Files/OpenStitch/sew"
        );
        assert_eq!(
            file_url(Path::new("/opt/open stitch/sew")),
            "file:///opt/open%20stitch/sew"
        );
    }
}
