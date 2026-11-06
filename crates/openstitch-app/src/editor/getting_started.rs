//! Getting started (#554, beyond PE-DESIGN): the start cards an empty new design shows, the
//! samples File ▸ Open Sample offers from the ready-to-sew kit, and the steps of Help ▸ Getting
//! Started. Every card and step runs an existing ribbon command, so `ui.run` reaches them too.
//!
//! Nothing here depends on egui.

use super::ribbon::Action;
use std::path::{Path, PathBuf};

/// What a start card or a Getting Started step does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// Runs a ribbon command as a click would.
    Run(Action),
    /// Opens the Text dialog (Text ▸ Text…).
    Text,
    /// Opens the Template Wizard.
    Template,
    /// Opens File ▸ Open Sample.
    Sample,
}

/// One start card: key (`ui.gettingStarted`), title, one line, and what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Card {
    pub key: &'static str,
    pub title: &'static str,
    pub text: &'static str,
    pub start: Start,
    /// The ribbon command whose icon the card shows.
    pub icon: Action,
}

pub const CARDS: &[Card] = &[
    Card {
        key: "draw",
        title: "Draw a shape",
        text: "Drag a rectangle; circles, outlines and curves are beside it",
        start: Start::Run(Action::Rectangle),
        icon: Action::Rectangle,
    },
    Card {
        key: "lettering",
        title: "Add lettering",
        text: "Type a name or a word and pick a font",
        start: Start::Text,
        icon: Action::TextTools,
    },
    Card {
        key: "picture",
        title: "Digitize a picture",
        text: "Show a picture behind the design, then Auto Punch traces it",
        start: Start::Run(Action::OpenImage),
        icon: Action::AutoPunch,
    },
    Card {
        key: "import",
        title: "Import a design",
        text: "Add a PES, DST, JEF or other embroidery file",
        start: Start::Run(Action::ImportPatterns),
        icon: Action::ImportPatterns,
    },
    Card {
        key: "template",
        title: "Use a template",
        text: "Badges, labels and names from the Template Wizard",
        start: Start::Template,
        icon: Action::Wizard,
    },
    Card {
        key: "sample",
        title: "Open a sample",
        text: "A ready design from the kit to look at and change",
        start: Start::Sample,
        icon: Action::Open,
    },
];

pub fn card(key: &str) -> Option<&'static Card> {
    CARDS.iter().find(|c| c.key == key)
}

/// File ▸ Open Sample: key, title and the file in the kit (`examples/sew`).
pub const SAMPLES: &[(&str, &str, &str)] = &[
    (
        "sampler",
        "Satin and fill sampler",
        "fills/fill-options.pes",
    ),
    ("shapes", "Shapes", "shapes/shape-tools.pes"),
    (
        "frames",
        "Decorative frames",
        "shapes/decorative-frames.pes",
    ),
    ("lettering", "Lettering", "lettering/fonts-and-text.pes"),
    ("monograms", "Monograms", "lettering/monograms.pes"),
    ("applique", "Appliqué", "wizards/applique.pes"),
];

/// Where the kit is: the one packaged with the editor at `exe`, else the source tree's
/// `examples/sew` (a development build).
pub fn kit(exe: Option<&Path>) -> Option<PathBuf> {
    exe.and_then(super::sew_kit::bundled).or_else(|| {
        let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sew");
        dev.join("manifest.json").is_file().then_some(dev)
    })
}

/// The sample `key`'s file in `kit`, when it is there.
pub fn sample_path(kit: &Path, key: &str) -> Option<PathBuf> {
    let (_, _, file) = SAMPLES.iter().find(|s| s.0 == key)?;
    let p = kit.join(file);
    p.is_file().then_some(p)
}

/// Whether the start cards show: switched on, and the design is empty (anything added hides
/// them).
pub fn cards_shown(enabled: bool, objects: usize) -> bool {
    enabled && objects == 0
}

/// Help ▸ Getting Started: the page's text (docs/GETTING_STARTED.md).
pub const PAGE: &str = include_str!("../../../../docs/GETTING_STARTED.md");

/// The page's step buttons: label and what each runs, in the page's order.
pub const STEPS: &[(&str, Start)] = &[
    ("Draw a shape", Start::Run(Action::Rectangle)),
    ("Add lettering", Start::Text),
    ("Watch it sew", Start::Run(Action::PaneSimulator)),
    ("Save", Start::Run(Action::Save)),
    ("Open a sample", Start::Sample),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sample_is_in_the_kit_and_cards_hide_once_something_is_added() {
        let kit = kit(None).expect("the source tree's kit");
        let manifest = std::fs::read_to_string(kit.join("manifest.json")).expect("manifest");
        for (key, _, file) in SAMPLES {
            assert!(manifest.contains(&format!("\"{file}\"")), "{file}");
            assert!(sample_path(&kit, key).is_some(), "{key}");
        }
        assert!(sample_path(&kit, "nope").is_none());
        assert!(cards_shown(true, 0) && !cards_shown(true, 1) && !cards_shown(false, 0));
        let mut keys: Vec<_> = CARDS.iter().map(|c| c.key).collect();
        keys.dedup();
        assert_eq!(keys.len(), CARDS.len());
        for c in CARDS {
            if let Start::Run(a) = c.start {
                assert!(a.info().available, "{}", c.key);
            }
        }
        // Every step has its heading on the page.
        for (label, _) in STEPS {
            assert!(PAGE.contains(&format!("## {label}")), "{label}");
        }
    }
}
