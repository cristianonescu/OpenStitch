//! Native editable project file (`.osd`, JSON): encoding, and decoding with the checks a
//! hostile file needs. Here rather than in the engine so readers that only need the file
//! (the Design Database, thumbnailers) do not depend on the command engine.

use crate::fonts::{self, DesignFont, FontData, MAX_DESIGN_FONTS, MAX_TOTAL_FONT_BYTES};
use crate::saved_chat::SavedChat;
use crate::{Design, text};
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "openstitch.design";
/// 2: direction angles are counter-clockwise on screen (PE-DESIGN's convention); version 1
/// stored them clockwise (see [`migrate_v1_angles`]).
/// 3: text size and line spacing follow PE-DESIGN's metrics; texts of older files keep the
/// capital-height metrics they were made with (see [`migrate_v2_text`]).
/// 4: a zigzag net turns with its Direction (#1043); nets of older files, which sewed on the
/// page axes whatever their angle, get Direction 0 (see [`migrate_v3_zigzag_net`]).
/// 5: a plain net's default Spacing is PE-DESIGN's 3.0 mm (#1110); nets of older files that
/// left it unset keep the 4.0 mm they sewed with (see [`migrate_v4_net_spacing`]).
/// 6: a zigzag net's Spacing is the gap between bands, as PE-DESIGN's (#1109), and its band
/// is no longer capped at 0.8 × the line pitch; nets of older files keep their lines and
/// bands (see [`migrate_v5_zigzag_net_spacing`]).
///
/// An optional `assistant` field beside `design` keeps the Assistant's conversation (#1169,
/// [`SavedChat`]). It needed no version step: readers before it ignore the field (alpha.15
/// opens such a file, and drops the chat if it saves it again).
pub const VERSION: u32 = 6;
pub const EXTENSION: &str = "osd";
pub const MAX_SIZE: usize = 64 << 20;

#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    version: u32,
    design: Design,
    /// Read leniently ([`SavedChat::from_value`]): a damaged chat never fails the design.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assistant: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Header {
    format: String,
    version: u32,
}

pub fn encode(design: &Design) -> Result<Vec<u8>, String> {
    encode_with_chat(design, None)
}

/// The project file with the Assistant's conversation (#1169); an empty or absent chat writes no
/// `assistant` field.
pub fn encode_with_chat(design: &Design, chat: Option<&SavedChat>) -> Result<Vec<u8>, String> {
    let assistant = match chat.filter(|c| !c.is_empty()) {
        Some(c) => Some(serde_json::to_value(c.clone().limited()).map_err(|e| e.to_string())?),
        None => None,
    };
    let env = Envelope {
        format: FORMAT.into(),
        version: VERSION,
        design: design.clone(),
        assistant,
    };
    serde_json::to_vec_pretty(&env).map_err(|e| e.to_string())
}

pub fn decode(data: &[u8]) -> Result<Design, String> {
    decode_with_chat(data).map(|(d, _)| d)
}

/// The design and the conversation it keeps, if any (#1169).
pub fn decode_with_chat(data: &[u8]) -> Result<(Design, Option<SavedChat>), String> {
    if data.len() > MAX_SIZE {
        return Err(format!("file larger than {} MiB", MAX_SIZE >> 20));
    }
    let header: Header =
        serde_json::from_slice(data).map_err(|_| "not an OpenStitch design file".to_string())?;
    if header.format != FORMAT {
        return Err(format!("unexpected format '{}'", header.format));
    }
    if header.version > VERSION {
        return Err(format!(
            "file version {} is newer than supported ({VERSION})",
            header.version
        ));
    }
    let mut env: Envelope =
        serde_json::from_slice(data).map_err(|e| format!("damaged design file: {e}"))?;
    validate(&env.design)?;
    if header.version < 2 {
        migrate_v1_angles(&mut env.design);
    }
    if header.version < 3 {
        migrate_v2_text(&mut env.design);
    }
    if header.version < 4 {
        migrate_v3_zigzag_net(&mut env.design);
    }
    if header.version < 5 {
        migrate_v4_net_spacing(&mut env.design);
    }
    if header.version < 6 {
        migrate_v5_zigzag_net_spacing(&mut env.design);
    }
    let chat = env.assistant.as_ref().and_then(SavedChat::from_value);
    Ok((env.design, chat))
}

/// Before version 6 a zigzag net's Spacing was its line pitch (8.0 mm unset) and its band at
/// most 0.8 × that pitch. Since then Spacing is the gap between bands, as in PE-DESIGN, and
/// the band is the zigzag width whatever the gap (#1109). Those nets get the band they sewed
/// and the gap that leaves the lines where they were: pitch − band. A net at both defaults
/// (8.0 mm lines, 3.0 mm band) is the new defaults (5.0 mm gap) and stays unset.
pub fn migrate_v5_zigzag_net_spacing(design: &mut Design) {
    for r in design.objects.iter_mut().filter_map(|o| o.region.as_mut()) {
        if r.kind != crate::model::RegionType::ZigzagNetFill
            || (r.spacing.is_none() && r.pattern_width.is_none())
        {
            continue;
        }
        let pitch = r.spacing.unwrap_or(8.0);
        let width = r.pattern_size_mm().0;
        let band = width.min(pitch * 0.8);
        if r.pattern_width.is_some() || band != width {
            r.pattern_width = Some(band);
        }
        r.spacing = Some(pitch - band);
    }
}

/// Before version 5 a plain net with no Spacing of its own sewed its lines 4.0 mm apart; the
/// default is now PE-DESIGN's 3.0 mm, so those nets get 4.0 and sew as they did (#1110).
pub fn migrate_v4_net_spacing(design: &mut Design) {
    for r in design.objects.iter_mut().filter_map(|o| o.region.as_mut()) {
        if r.kind == crate::model::RegionType::NetFill && r.spacing.is_none() {
            r.spacing = Some(4.0);
        }
    }
}

/// Before version 4 a zigzag net sewed on the page axes whatever its Direction (often 45°,
/// kept from the fill it was switched from). Those nets get Direction 0, so they sew as they
/// did (#1043).
pub fn migrate_v3_zigzag_net(design: &mut Design) {
    for r in design.objects.iter_mut().filter_map(|o| o.region.as_mut()) {
        if r.kind == crate::model::RegionType::ZigzagNetFill {
            r.angle = 0.0;
        }
    }
}

/// Before version 3 a text's size was its capital height and 100 % line spacing the font's
/// line height. Those texts keep [`text::TextMetrics::CapHeight`], so they lay out unchanged.
pub fn migrate_v2_text(design: &mut Design) {
    for t in design.objects.iter_mut().filter_map(|o| o.text.as_mut()) {
        t.metrics = text::TextMetrics::CapHeight;
    }
}

/// Version 1 files stored direction angles clockwise on screen (from +x towards +y, y down).
/// Since version 2 they turn counter-clockwise, as in PE-DESIGN, so a version-1 file sews the
/// same stitches after loading: region angles are negated (0 ≤ a < 360) and stem line slants,
/// measured from the line, become 180 − a (an unset slant, 45°, becomes 135°).
pub fn migrate_v1_angles(design: &mut Design) {
    for o in &mut design.objects {
        if let Some(r) = &mut o.region {
            r.angle = crate::region::stored_angle(r.angle);
        }
        if let Some(l) = &mut o.line
            && l.kind == crate::model::LineType::Stem
        {
            // Unset meant 45° from the line, clockwise.
            l.angle = Some(180.0 - l.angle.unwrap_or(45.0));
        }
    }
}

/// Checks a design read from an untrusted source (objects, coordinates, text, fonts).
pub fn validate(d: &Design) -> Result<(), String> {
    validate_fonts(&d.fonts)?;
    let mut ids: Vec<u64> = d.objects.iter().map(|o| o.id).collect();
    ids.sort_unstable();
    if ids.windows(2).any(|w| w[0] == w[1]) {
        return Err("duplicate object ids".into());
    }
    if d.next_object_id <= ids.last().copied().unwrap_or(0) {
        return Err("nextObjectID not above existing ids".into());
    }
    let finite = d.objects.iter().all(|o| {
        o.points()
            .iter()
            .all(|p| p.x.is_finite() && p.y.is_finite())
            && o.contours.iter().all(|c| {
                c.curves.len() <= c.segment_count()
                    && c.curves.iter().flatten().all(|b| {
                        [b.c1, b.c2]
                            .iter()
                            .all(|q| q.x.is_finite() && q.y.is_finite())
                    })
            })
    });
    let text_ok = d.objects.iter().all(|o| {
        o.text.as_ref().is_none_or(|t| {
            t.transform.is_finite()
                && [
                    t.size,
                    t.letter_spacing,
                    t.line_spacing,
                    t.line_spacing_percent,
                ]
                .iter()
                .all(|v| v.is_finite())
                && t.characters.windows(2).all(|w| w[0].index < w[1].index)
                && t.characters.iter().all(|e| {
                    [e.x, e.y, e.rotation].iter().all(|v| v.is_finite())
                        && e.size.is_none_or(|s| s.is_finite() && s > 0.0)
                })
        })
    });
    let skeletons_ok = d.objects.iter().all(|o| {
        o.region
            .as_ref()
            .and_then(|r| r.skeleton.as_ref())
            .is_none_or(|s| s.validate().is_ok())
    });
    if !skeletons_ok {
        return Err("invalid satin skeleton".into());
    }
    if d.objects
        .iter()
        .any(|o| o.hand_edits.as_ref().is_some_and(|h| !h.is_valid()))
    {
        return Err("invalid hand edits".into());
    }
    if !finite || !text_ok || !(d.page.width > 0.0 && d.page.height > 0.0) {
        return Err("invalid coordinates".into());
    }
    Ok(())
}

/// Checks the fonts of a design read from a file.
fn validate_fonts(fonts_in: &[DesignFont]) -> Result<(), String> {
    if fonts_in.len() > MAX_DESIGN_FONTS {
        return Err(format!("more than {MAX_DESIGN_FONTS} fonts"));
    }
    let total: usize = fonts_in.iter().map(DesignFont::byte_size).sum();
    if total > MAX_TOTAL_FONT_BYTES {
        return Err("embedded fonts too large".into());
    }
    for (i, f) in fonts_in.iter().enumerate() {
        if !fonts::valid_id(&f.id)
            || text::font(&f.id).is_some_and(|b| b.id == f.id)
            || fonts_in[..i].iter().any(|g| g.id == f.id)
        {
            return Err(format!("bad or duplicate font id '{}'", f.id));
        }
        match &f.data {
            FontData::TrueType { face, bytes } => {
                fonts::check_truetype(&bytes.0, *face)
                    .map_err(|e| format!("font '{}': {e}", f.id))?;
            }
            FontData::Embroidery(e) => e.validate().map_err(|e| format!("font '{}': {e}", f.id))?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Contour;
    use crate::model::{DesignObject, DesignPage, LineSewing, LineType, RegionSewing, RegionType};
    use crate::{Compiler, Design};
    use openstitch_core::Thread;

    /// A 30° fill square and a stem-stitch outline, angles in the version-2 convention.
    fn design() -> Design {
        let mut d = Design::new("angles", DesignPage::default());
        let mut r = RegionSewing::new(RegionType::Fill, Thread::rgb(0xff0000));
        r.angle = 30.0;
        let mut l = LineSewing::new(LineType::Stem, Thread::rgb(0));
        l.angle = Some(60.0);
        d.add(DesignObject::shape(
            vec![Contour::rectangle(0.0, 0.0, 300.0, 200.0)],
            Some(r),
            Some(l),
        ));
        d
    }

    #[test]
    fn version_2_keeps_angles() {
        let bytes = encode(&design()).expect("encode");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["version"], VERSION);
        let back = decode(&bytes).expect("decode");
        assert_eq!(back, design());
    }

    #[test]
    fn version_1_angles_are_migrated_and_sew_the_same() {
        // The same design as a version-1 file wrote it: angles clockwise on screen.
        let mut v: serde_json::Value =
            serde_json::from_slice(&encode(&design()).expect("encode")).expect("json");
        v["version"] = serde_json::json!(1);
        let o = &mut v["design"]["objects"][0];
        o["region"]["angle"] = serde_json::json!(-30.0);
        o["line"]["angle"] = serde_json::json!(120.0);
        let old = decode(&serde_json::to_vec(&v).expect("bytes")).expect("decode");
        let (r, l) = (
            old.objects[0].region.as_ref().expect("region"),
            old.objects[0].line.as_ref().expect("line"),
        );
        assert_eq!((r.angle, l.angle), (30.0, Some(60.0)));
        let c = Compiler::default();
        assert_eq!(c.compile(&old).stitches, c.compile(&design()).stitches);
        // An unset stem slant was 45° clockwise: 135° now.
        o_unset(&mut v);
        let old = decode(&serde_json::to_vec(&v).expect("bytes")).expect("decode");
        assert_eq!(
            old.objects[0].line.as_ref().expect("line").angle,
            Some(135.0)
        );
    }

    /// #1043: a zigzag net from a version-3 file sewed on the page axes whatever its angle, so
    /// it loads at Direction 0 and sews as before; a version-4 file keeps its angle.
    #[test]
    fn version_3_zigzag_nets_load_at_direction_zero() {
        let mut d = Design::new("net", DesignPage::default());
        let mut r = RegionSewing::new(RegionType::ZigzagNetFill, Thread::rgb(0xff0000));
        r.angle = 45.0;
        d.add(DesignObject::shape(
            vec![Contour::rectangle(0.0, 0.0, 300.0, 200.0)],
            Some(r),
            None,
        ));
        let mut v: serde_json::Value =
            serde_json::from_slice(&encode(&d).expect("encode")).expect("json");
        let angle = |v: &serde_json::Value| {
            decode(&serde_json::to_vec(v).expect("json"))
                .expect("decode")
                .objects[0]
                .region
                .as_ref()
                .expect("region")
                .angle
        };
        assert_eq!(angle(&v), 45.0, "version 4 keeps the angle");
        v["version"] = serde_json::json!(3);
        assert_eq!(angle(&v), 0.0, "version 3 sewed on the axes");
    }

    /// #1110: a plain net from a version-4 file with no Spacing of its own sewed 4.0 mm
    /// apart, so it loads with 4.0 and sews as before; a version-5 file keeps the new 3.0
    /// default, and a set Spacing is kept either way.
    #[test]
    fn version_4_plain_nets_keep_their_four_millimetre_spacing() {
        let mut d = Design::new("net", DesignPage::default());
        let r = RegionSewing::new(RegionType::NetFill, Thread::rgb(0xff0000));
        assert_eq!(r.spacing_mm(), 3.0, "PE-DESIGN's default");
        d.add(DesignObject::shape(
            vec![Contour::rectangle(0.0, 0.0, 300.0, 200.0)],
            Some(r.clone()),
            None,
        ));
        let mut set = r;
        set.spacing = Some(2.5);
        d.add(DesignObject::shape(
            vec![Contour::rectangle(400.0, 0.0, 300.0, 200.0)],
            Some(set),
            None,
        ));
        let mut v: serde_json::Value =
            serde_json::from_slice(&encode(&d).expect("encode")).expect("json");
        let spacings = |v: &serde_json::Value| -> Vec<Option<f64>> {
            decode(&serde_json::to_vec(v).expect("json"))
                .expect("decode")
                .objects
                .iter()
                .map(|o| o.region.as_ref().expect("region").spacing)
                .collect()
        };
        assert_eq!(
            spacings(&v),
            vec![None, Some(2.5)],
            "version 5 keeps the default"
        );
        v["version"] = serde_json::json!(4);
        assert_eq!(
            spacings(&v),
            vec![Some(4.0), Some(2.5)],
            "version 4 sewed 4 mm apart"
        );
    }

    /// #1109: a version-5 zigzag net's Spacing was its line pitch and its band at most 0.8 of
    /// it. It loads with the band it sewed and the gap that keeps its lines in place; one at
    /// both defaults stays unset.
    #[test]
    fn version_5_zigzag_nets_keep_their_lines_and_bands() {
        let net = |spacing: Option<f64>, width: Option<f64>| {
            let mut d = Design::new("net", DesignPage::default());
            let mut r = RegionSewing::new(RegionType::ZigzagNetFill, Thread::rgb(0xff0000));
            (r.spacing, r.pattern_width) = (spacing, width);
            d.add(DesignObject::shape(
                vec![Contour::rectangle(0.0, 0.0, 300.0, 200.0)],
                Some(r),
                None,
            ));
            d
        };
        let as_v5 = |d: &Design| {
            let mut v: serde_json::Value =
                serde_json::from_slice(&encode(d).expect("encode")).expect("json");
            v["version"] = serde_json::json!(5);
            decode(&serde_json::to_vec(&v).expect("bytes")).expect("decode")
        };
        let kept = |d: &Design| {
            let r = d.objects[0].region.as_ref().expect("region");
            (r.spacing, r.pattern_width)
        };
        // (stored pitch, stored width) -> (gap, band): band + gap is the old pitch.
        for (old, new) in [
            ((None, None), (None, None)),
            ((Some(8.0), None), (Some(5.0), None)),
            ((Some(10.0), Some(4.0)), (Some(6.0), Some(4.0))),
            // Capped: a 10 mm width on a 5 mm pitch sewed a 4 mm band.
            ((Some(5.0), Some(10.0)), (Some(1.0), Some(4.0))),
            // The default 3 mm band capped by a 3 mm pitch: 2.4 mm.
            ((Some(3.0), None), (Some(3.0 - 3.0 * 0.8), Some(3.0 * 0.8))),
            // The command's 0.5 mm minimum pitch: a 0.4 mm band and a 0.1 mm gap, below the
            // inspector's ranges, kept as they are (no validation or clamp on load).
            ((Some(0.5), None), (Some(0.5 - 0.5 * 0.8), Some(0.5 * 0.8))),
        ] {
            assert_eq!(kept(&as_v5(&net(old.0, old.1))), new, "{old:?}");
        }
        // A version-6 file keeps the gap as stored.
        let d = net(Some(5.0), Some(10.0));
        assert_eq!(
            kept(&decode(&encode(&d).expect("encode")).expect("decode")),
            kept(&d)
        );
    }

    /// #1109: a version-5 zigzag net loads with its pitch turned into a band and a gap, and sews
    /// the stitches the code before #1109 sewed, also where the old cap narrowed the band
    /// (pitch 0.5 and 3.0 with the 3 mm default; width 10 on pitch 5) and on a turned net.
    /// Each print is the stitch count and an FNV-1a hash of every command and point to
    /// 0.0001 mm, taken from the same 30 × 20 mm nets compiled at main 61c16a0f (before this
    /// change, the stored values sewn with the old cap).
    #[test]
    fn version_5_zigzag_nets_sew_the_stitches_they_sewed() {
        let stitch_print = |d: &Design| -> (usize, u64) {
            let p = crate::Compiler::default().compile(d);
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for q in &p.stitches {
                let s = format!(
                    "{:?}{}:{};",
                    q.command,
                    (q.x * 1000.0).round() as i64,
                    (q.y * 1000.0).round() as i64
                );
                for b in s.bytes() {
                    h ^= u64::from(b);
                    h = h.wrapping_mul(0x0100_0000_01b3);
                }
            }
            (p.stitches.len(), h)
        };
        // (stored pitch, stored width, Direction) -> the print at 61c16a0f.
        type Net = (f64, Option<f64>, f64);
        let before: [(Net, (usize, u64)); 8] = [
            ((0.5, None, 0.0), (15806, 1429652117529704550)),
            ((3.0, None, 0.0), (2221, 18352461383984626860)),
            ((5.0, Some(10.0), 0.0), (1307, 14602736324947324305)),
            ((8.0, None, 0.0), (914, 10421643497049895347)),
            ((0.5, None, 30.0), (16008, 3828908425474527964)),
            ((3.0, None, 30.0), (2625, 10215742557738257950)),
            ((5.0, Some(10.0), 30.0), (1755, 13102416351260724298)),
            ((8.0, None, 30.0), (1058, 11595861973528435010)),
        ];
        let mut changed = Vec::new();
        for ((pitch, width, angle), old) in before {
            let mut d = Design::new("net", DesignPage::default());
            let mut r = RegionSewing::new(RegionType::ZigzagNetFill, Thread::rgb(0xff0000));
            (r.spacing, r.pattern_width, r.angle) = (Some(pitch), width, angle);
            d.add(DesignObject::shape(
                vec![Contour::rectangle(0.0, 0.0, 300.0, 200.0)],
                Some(r),
                None,
            ));
            let mut v: serde_json::Value =
                serde_json::from_slice(&encode(&d).expect("encode")).expect("json");
            v["version"] = serde_json::json!(5);
            let migrated = decode(&serde_json::to_vec(&v).expect("bytes")).expect("decode");
            let now = stitch_print(&migrated);
            if now != old {
                changed.push(format!("{pitch} {width:?} {angle}°: {old:?} -> {now:?}"));
            }
        }
        assert!(changed.is_empty(), "{changed:#?}");
    }

    /// #1130: a motif line's default Run pitch went from 2.5 to PE-DESIGN's 5.0 with no `.osd`
    /// version step, because every file stores `pitch` on every line (the field is required):
    /// a version-6 motif line made at the old default loads at 2.5 and sews what it sewed, and
    /// a line without `pitch` is refused rather than given the new default.
    #[test]
    fn older_motif_lines_keep_the_pitch_they_were_saved_with() {
        let design = |pitch: f64| {
            let mut d = Design::new("motif", DesignPage::default());
            let mut l = LineSewing::new(LineType::Motif, Thread::rgb(0xff0000));
            l.pitch = pitch;
            // The Run pitch sews the runs between motifs: with the default gap of 0 a motif
            // line sews the same at either pitch, so this one has a 4 mm gap.
            l.spacing = Some(4.0);
            d.add(DesignObject::shape(
                vec![Contour::rectangle(0.0, 0.0, 400.0, 400.0)],
                None,
                Some(l),
            ));
            d
        };
        assert_eq!(LineSewing::new(LineType::Motif, Thread::rgb(0)).pitch, 5.0);
        let old = design(2.5);
        let mut v: serde_json::Value =
            serde_json::from_slice(&encode(&old).expect("encode")).expect("json");
        v["version"] = serde_json::json!(6);
        assert_eq!(
            v["design"]["objects"][0]["line"]["pitch"],
            serde_json::json!(2.5)
        );
        let loaded = decode(&serde_json::to_vec(&v).expect("bytes")).expect("decode");
        assert_eq!(loaded.objects[0].line.as_ref().expect("line").pitch, 2.5);
        let sew = |d: &Design| crate::Compiler::default().compile(d).stitches;
        let (was, now, new) = (sew(&old), sew(&loaded), sew(&design(5.0)));
        assert_eq!(was.len(), now.len());
        assert!(was.iter().zip(&now).all(|(a, b)| a.x == b.x && a.y == b.y));
        assert_ne!(
            was.len(),
            new.len(),
            "the default changes what a new motif line sews"
        );
        v["design"]["objects"][0]["line"]
            .as_object_mut()
            .expect("line")
            .remove("pitch");
        assert!(decode(&serde_json::to_vec(&v).expect("bytes")).is_err());
    }

    #[test]
    fn version_2_texts_keep_their_capital_height_metrics() {
        let mut d = Design::new("t", DesignPage::default());
        let spec = text::TextSpec::new("H", text::DEFAULT_FONT, 10.0);
        let (o, _) = DesignObject::text(spec, None, None).expect("text");
        d.add(o);
        let mut v: serde_json::Value =
            serde_json::from_slice(&encode(&d).expect("encode")).expect("json");
        assert!(v["design"]["objects"][0]["text"].get("metrics").is_none());
        assert_eq!(decode(&encode(&d).expect("encode")).expect("decode"), d);
        v["version"] = serde_json::json!(2);
        let old = decode(&serde_json::to_vec(&v).expect("bytes")).expect("decode");
        let t = old.objects[0].text.as_ref().expect("text");
        assert_eq!(t.metrics, text::TextMetrics::CapHeight);
        // Saved again, the old basis is kept.
        let again: serde_json::Value =
            serde_json::from_slice(&encode(&old).expect("encode")).expect("json");
        assert_eq!(
            again["design"]["objects"][0]["text"]["metrics"],
            "capHeight"
        );
    }

    fn o_unset(v: &mut serde_json::Value) {
        v["design"]["objects"][0]["line"]
            .as_object_mut()
            .expect("line")
            .remove("angle");
    }

    /// #1169: a chat is kept beside the design and read back as it was.
    #[test]
    fn a_project_keeps_its_chat_beside_the_design() {
        use crate::saved_chat::{ChatRole, ChatRow, SavedChat};
        let chat = SavedChat {
            rows: vec![
                ChatRow::new(ChatRole::User, "Make the square blue"),
                ChatRow {
                    failed: true,
                    ..ChatRow::new(ChatRole::Tool, "Ran object.setRegion")
                },
                ChatRow::new(ChatRole::Assistant, "Done."),
            ],
        };
        let bytes = encode_with_chat(&design(), Some(&chat)).expect("encode");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["version"], VERSION, "no version step");
        assert_eq!(v["assistant"]["rows"][1]["role"], "tool");
        let (d, back) = decode_with_chat(&bytes).expect("decode");
        assert_eq!((d, back), (design(), Some(chat)));
        // No chat, or an empty one: no field.
        for c in [None, Some(&SavedChat::default())] {
            let v: serde_json::Value =
                serde_json::from_slice(&encode_with_chat(&design(), c).expect("encode"))
                    .expect("json");
            assert!(v.get("assistant").is_none());
        }
        // A damaged chat never fails the design.
        let mut v = v;
        v["assistant"] = serde_json::json!({"rows": "nonsense"});
        let (d, back) = decode_with_chat(&serde_json::to_vec(&v).expect("bytes")).expect("decode");
        assert_eq!((d, back), (design(), None));
    }

    /// #1169: alpha.15's reader (this envelope, without the `assistant` field, and the same
    /// header check) opens a file that keeps a chat: the design is intact, the chat ignored.
    #[test]
    fn alpha_15_opens_a_project_that_keeps_a_chat() {
        use crate::saved_chat::{ChatRole, ChatRow, SavedChat};
        #[derive(Deserialize)]
        struct Alpha15Envelope {
            format: String,
            version: u32,
            design: Design,
        }
        let chat = SavedChat {
            rows: vec![ChatRow::new(ChatRole::User, "hello")],
        };
        let bytes = encode_with_chat(&design(), Some(&chat)).expect("encode");
        let old: Alpha15Envelope = serde_json::from_slice(&bytes).expect("alpha.15 reads it");
        assert_eq!((old.format.as_str(), old.version), (FORMAT, 6));
        assert!(old.version <= 6, "alpha.15 refuses only versions above 6");
        assert_eq!(old.design, design());
    }
}
