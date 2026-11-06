//! Free text envelopes (#535, beyond PE-DESIGN): `text.setEnvelope` turns a Text Transform
//! preset into a free envelope (the same stitches) or starts one from a shape (bridge,
//! pennant, perspective, diamond); `text.bendEnvelope` moves one of its six handles (the ends
//! and the middle of the top and bottom edge), mirrored about the text's centre line when
//! asked; `text.envelopeHandles` tells where the handles are. A free envelope cannot be
//! written as one of PE-DESIGN's presets: PES 0110 export refuses it, as it refuses the other
//! text attributes PE-DESIGN cannot hold.

use crate::CommandSpec;
use crate::commands::{Effect, IDS, MM, opt, relayout, req, spec, text_index};
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use openstitch_design::text::{
    self, CustomEnvelope, EnvelopeEdge, EnvelopeHandle, MAX_ENVELOPE_KNOTS,
};
use serde_json::{Value, json};

pub(crate) fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "text.setEnvelope",
            "Give text a free envelope edited with handles (beyond PE-DESIGN): from its Text Transform preset (the same stitches until a handle moves), from a shape, or from explicit edges; 'none' removes it",
            Effect::Edit,
            vec![
                IDS,
                opt(
                    "shape",
                    "preset|bridge|pennant|perspective|diamond|none",
                    "preset: the text's Text Transform preset (not the arches 1 and 2); bridge: the top bows down, the baseline straight; pennant: tapering left to right; perspective: the top edge shorter; diamond: tall in the middle; none: straight again (default preset)",
                ),
                opt(
                    "amount",
                    "0-0.45",
                    "How strong a shape is, as a share of the letters' height (default 0.3)",
                ),
                opt(
                    "top",
                    "[[x, y]]",
                    "Top edge knots, evenly spaced along the text, as fractions of the straight text's capital-height box (x from its left, y from its top, y down); 2-129 points; needs bottom",
                ),
                opt("bottom", "[[x, y]]", "Bottom edge knots, as top"),
            ],
            set_envelope,
        ),
        spec(
            "text.bendEnvelope",
            "Move one handle of a text's free envelope (one undo step per move): the edge bends smoothly; symmetric mirrors the move on the other side of the text's centre line",
            Effect::Edit,
            vec![
                req("id", "int", "Text object with a free envelope"),
                req("edge", "top|bottom", "Which edge"),
                req("handle", "left|middle|right", "Which handle of the edge"),
                opt("dx", "mm", "Move right on the page (default 0)"),
                opt("dy", "mm", "Move down on the page (default 0)"),
                opt(
                    "symmetric",
                    "bool",
                    "Mirror the move on the opposite handle (Shift-drag; the middle handle then moves only up or down)",
                ),
            ],
            bend_envelope,
        ),
        spec(
            "text.envelopeHandles",
            "Where a text's free envelope handles are on the page (mm): top and bottom, each left, middle, right",
            Effect::Read,
            vec![req("id", "int", "Text object")],
            |s, p| {
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = text_index(s, p, id)?;
                let spec = s.design.objects[i].text.as_ref().expect("text");
                let h = text::envelope_handles(spec, &s.design.fonts)
                    .ok_or_else(|| p.failed(format!("object {id} has no free envelope")))?;
                let pt = |v: openstitch_design::Vec2| json!([v.x / MM, v.y / MM]);
                let edge = |e: &[openstitch_design::Vec2; 3]| json!({ "left": pt(e[0]), "middle": pt(e[1]), "right": pt(e[2]) });
                Ok(json!({ "top": edge(&h[0]), "bottom": edge(&h[1]) }))
            },
        ),
    ]
}

fn knots(p: &Params, key: &str) -> Result<Vec<(f64, f64)>, CommandError> {
    let arr = p.array(key)?;
    if !(2..=MAX_ENVELOPE_KNOTS).contains(&arr.len()) {
        return Err(p.invalid(key, format!("needs 2-{MAX_ENVELOPE_KNOTS} points")));
    }
    arr.iter()
        .map(|v| {
            let xy = v.as_array().filter(|a| a.len() == 2);
            match xy.and_then(|a| a[0].as_f64().zip(a[1].as_f64())) {
                Some((x, y))
                    if x.is_finite() && y.is_finite() && x.abs() <= 3.0 && y.abs() <= 3.0 =>
                {
                    Ok((x, y))
                }
                _ => Err(p.invalid(key, "each point must be [x, y], fractions within ±3")),
            }
        })
        .collect()
}

fn set_envelope(s: &mut EngineState, p: &Params) -> Result<Value, CommandError> {
    let ids = p.ids()?;
    if ids.is_empty() {
        return Err(p.invalid("ids", "must not be empty"));
    }
    let explicit = p.has("top") || p.has("bottom");
    let shape = p.string("shape", Some(if explicit { "edges" } else { "preset" }))?;
    let amount = p.opt("amount", 0.3, 0.0, 0.45)?;
    let mut out = Vec::new();
    for id in ids {
        let i = text_index(s, p, id)?;
        let mut spec = s.design.objects[i].text.clone().expect("text");
        if spec.path.is_some() {
            return Err(p.failed(format!(
                "object {id} is fitted to an outline; release it first"
            )));
        }
        let envelope = match shape.as_str() {
            "none" => None,
            "edges" => Some(CustomEnvelope {
                top: knots(p, "top")?,
                bottom: knots(p, "bottom")?,
                scale: 1.0,
            }),
            "preset" => {
                let n = spec.envelope.ok_or_else(|| {
                    p.failed(format!(
                        "object {id} has no Text Transform preset; give a shape"
                    ))
                })?;
                Some(
                    CustomEnvelope::from_preset(n, spec.envelope_level).ok_or_else(|| {
                        p.failed(
                            "the arches (1, 2) set text on a circle and have no envelope to edit",
                        )
                    })?,
                )
            }
            other => Some(CustomEnvelope::shape(other, amount).ok_or_else(|| {
                p.invalid(
                    "shape",
                    "must be preset, bridge, pennant, perspective, diamond or none",
                )
            })?),
        };
        // A free envelope replaces the preset; the text stays straight-horizontal.
        if envelope.is_some() {
            spec.envelope = None;
            spec.envelope_level = None;
        }
        spec.custom_envelope = envelope;
        s.design.objects[i].text = Some(spec);
        out.push(relayout(s, p, i)?);
    }
    Ok(Value::Array(out))
}

fn bend_envelope(s: &mut EngineState, p: &Params) -> Result<Value, CommandError> {
    let id = p.int("id", None, 0, i64::MAX)? as u64;
    let i = text_index(s, p, id)?;
    let mut spec = s.design.objects[i].text.clone().expect("text");
    let edge = match p.string("edge", None)?.as_str() {
        "top" => EnvelopeEdge::Top,
        "bottom" => EnvelopeEdge::Bottom,
        _ => return Err(p.invalid("edge", "must be top or bottom")),
    };
    let handle = match p.string("handle", None)?.as_str() {
        "left" => EnvelopeHandle::Left,
        "middle" => EnvelopeHandle::Middle,
        "right" => EnvelopeHandle::Right,
        _ => return Err(p.invalid("handle", "must be left, middle or right")),
    };
    let dx = p.opt("dx", 0.0, -1000.0, 1000.0)? * MM;
    let dy = p.opt("dy", 0.0, -1000.0, 1000.0)? * MM;
    let symmetric = p.bool("symmetric", Some(false))?;
    let ((_, width), (_, height)) = text::envelope_frame(&spec, &s.design.fonts)
        .filter(|((_, w), (_, h))| *w > 1e-6 && *h > 1e-6)
        .ok_or_else(|| p.failed(format!("object {id} has no letters to bend")))?;
    // The page move in the text's own frame (its rotation and scale undone).
    let t = spec.transform;
    let det = t.a * t.d - t.b * t.c;
    if det.abs() < 1e-12 {
        return Err(p.failed(format!("object {id} is scaled flat")));
    }
    let (lx, ly) = ((t.d * dx - t.c * dy) / det, (-t.b * dx + t.a * dy) / det);
    let Some(e) = spec.custom_envelope.as_mut() else {
        return Err(p.failed(format!(
            "object {id} has no free envelope; text.setEnvelope first"
        )));
    };
    e.bend(edge, handle, (lx / width, ly / height), symmetric);
    if !e.is_valid() {
        return Err(p.failed("the handle would leave the envelope too far from the text"));
    }
    s.design.objects[i].text = Some(spec);
    relayout(s, p, i)
}
