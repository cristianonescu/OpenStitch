//! What the browser version cannot do, and what to say instead (#830).
//!
//! The web page runs the desktop editor itself, but a page cannot reach the system's
//! printers, USB sticks, folders, the system clipboard's pictures or the self-updater. (The
//! Assistant was one until it got its own browser panel, #766.) Each of those shows one clear message there: what it is, why, what to do
//! instead and the issue that tracks it. The desktop never shows them.

/// One thing the browser version cannot do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    Print,
    Transfer,
    Updates,
    Folders,
    ClipboardImages,
    SystemFonts,
}

impl Limit {
    pub const ALL: [Limit; 6] = [
        Limit::Print,
        Limit::Transfer,
        Limit::Updates,
        Limit::Folders,
        Limit::ClipboardImages,
        Limit::SystemFonts,
    ];

    /// Its key in `ui.browserLimits`.
    pub fn key(self) -> &'static str {
        match self {
            Limit::Print => "print",
            Limit::Transfer => "transfer",
            Limit::Updates => "updates",
            Limit::Folders => "folders",
            Limit::ClipboardImages => "clipboardImages",
            Limit::SystemFonts => "systemFonts",
        }
    }

    /// What it is, as the user knows it.
    pub fn what(self) -> &'static str {
        match self {
            Limit::Print => "Printing",
            Limit::Transfer => "Sending to a machine or USB stick",
            Limit::Updates => "Checking for updates",
            Limit::Folders => "Choosing a folder",
            Limit::ClipboardImages => "Pictures on the system clipboard",
            Limit::SystemFonts => "Lettering in the computer's own fonts",
        }
    }

    /// Why a page cannot, and what to do instead.
    pub fn instead(self) -> &'static str {
        match self {
            Limit::Print => {
                "A web page cannot reach the system's printers. Export a job sheet (PDF) from Print ▸ Job Sheet and print that."
            }
            Limit::Transfer => {
                "A web page cannot write to a USB stick or machine. Export the design (it downloads) and copy it there."
            }
            Limit::Updates => "The browser version is always the latest one: reload the page.",
            Limit::Folders => {
                "A web page cannot browse folders. Pick the files themselves, or use the desktop editor."
            }
            Limit::ClipboardImages => {
                "Copy and paste stay inside the editor here. Open the picture as a file (Image ▸ Open Image)."
            }
            Limit::SystemFonts => {
                "A web page cannot read the fonts installed on the computer. The built-in fonts are here, and Load TrueType Font adds a font file."
            }
        }
    }

    /// The issue that tracks it.
    pub fn issue(self) -> u32 {
        match self {
            Limit::SystemFonts => 833,
            _ => 830,
        }
    }

    /// The whole message the browser version shows.
    pub fn message(self) -> String {
        format!(
            "{} is not available in the browser version. {} (OpenStitch issue #{})",
            self.what(),
            self.instead(),
            self.issue()
        )
    }

    pub fn from_key(key: &str) -> Option<Limit> {
        Limit::ALL.into_iter().find(|l| l.key() == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_limit_names_itself_why_what_instead_and_its_issue() {
        for l in Limit::ALL {
            let m = l.message();
            assert!(m.starts_with(l.what()), "{m}");
            assert!(m.contains("browser version"), "{m}");
            assert!(m.contains(&format!("#{}", l.issue())), "{m}");
            assert_eq!(Limit::from_key(l.key()), Some(l));
        }
        // The Assistant has its own browser panel since #766.
        assert_eq!(Limit::from_key("assistant"), None);
        assert_eq!(Limit::from_key("nope"), None);
    }
}
