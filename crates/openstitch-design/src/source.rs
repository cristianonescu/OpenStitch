//! The machine file a design was opened from (File ▸ Open of a PES). PE-DESIGN's own files hold
//! more than OpenStitch reads into a design: editable objects of classes it does not import,
//! Design Page and hoop fields, the thread table with its charts. Saving the design again as
//! the same file, unchanged, writes the file's own bytes back, so nothing is lost (#375). Any
//! change to the design since it was opened (its [`fingerprint`] differs) makes the export
//! generate the file from the design instead.

use crate::model::Design;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

/// Largest source file kept (the PES reader's own limit).
pub const MAX_SOURCE_BYTES: usize = 64 << 20;

/// The file as opened, and the design's fingerprint right after opening it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFile {
    /// Lower-case extension, e.g. `pes`.
    pub format: String,
    /// The PES version (`0110`, `0060`…), when the file is a PES.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub bytes: SourceBytes,
    /// [`fingerprint`] of the design when it was opened.
    pub fingerprint: u64,
}

impl SourceFile {
    /// Whether `design` is still exactly what was opened.
    pub fn unchanged(&self, design: &Design) -> bool {
        fingerprint(design) == self.fingerprint
    }
}

/// File bytes, shared between undo copies of a design and stored base64-encoded in JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceBytes(pub Arc<[u8]>);

impl Serialize for SourceBytes {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use base64::Engine;
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for SourceBytes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use base64::Engine;
        let text = String::deserialize(d)?;
        if text.len() > MAX_SOURCE_BYTES / 3 * 4 + 4 {
            return Err(serde::de::Error::custom("source file too large"));
        }
        base64::engine::general_purpose::STANDARD
            .decode(text.as_bytes())
            .map(|b| SourceBytes(b.into()))
            .map_err(|_| serde::de::Error::custom("source file is not base64"))
    }
}

/// A hash of everything in the design except its source file (FNV-1a over its JSON form, which
/// is stable across `.osd` saves: serde_json writes floats so they read back exactly).
pub fn fingerprint(design: &Design) -> u64 {
    let mut d = design.clone();
    d.source = None;
    // The job (#513) is printed on job sheets, never sewn: setting it leaves the file unchanged.
    d.job = None;
    let json = serde_json::to_vec(&d).unwrap_or_default();
    json.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DesignObject, DesignPage, StitchBlock};
    use openstitch_core::{Command, Stitch, Thread};

    fn opened() -> Design {
        let mut d = Design::new("rose", DesignPage::default());
        d.add(DesignObject::stitches(StitchBlock {
            stitches: vec![
                Stitch::new(0.0, 0.0, Command::Stitch),
                Stitch::new(10.5, 3.25, Command::Stitch),
            ],
            threads: vec![Thread::rgb(0x00ff_0000)],
        }));
        d.source = Some(SourceFile {
            format: "pes".into(),
            version: Some("0110".into()),
            bytes: SourceBytes(vec![1, 2, 3].into()),
            fingerprint: fingerprint(&d),
        });
        d
    }

    #[test]
    fn an_unchanged_design_matches_its_source_through_a_project_save() {
        let d = opened();
        let source = d.source.clone().expect("source");
        assert!(source.unchanged(&d));
        let json = serde_json::to_string(&d).expect("save");
        let back: Design = serde_json::from_str(&json).expect("load");
        assert_eq!(back.source, d.source);
        assert!(source.unchanged(&back));
    }

    #[test]
    fn any_change_breaks_the_match() {
        let source = opened().source.expect("source");
        let mut moved = opened();
        moved.objects[0]
            .stitch_block
            .as_mut()
            .expect("block")
            .stitches[1]
            .x += 1.0;
        let mut renamed = opened();
        renamed.name = "tulip".into();
        let mut page = opened();
        page.page.width += 100.0;
        for d in [moved, renamed, page] {
            assert!(!source.unchanged(&d));
        }
    }
}
