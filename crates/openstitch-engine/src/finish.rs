//! `object.setFinish` (#548, beyond PE-DESIGN): per object, when the thread is trimmed after
//! it, its tie-in and lock (count and length), and where a text trims between its letters.
//! Every setting's default is PE-DESIGN's rule, and an object back at every default stores no
//! finish, so its stitches and PES files are as before.

use crate::commands::{CommandSpec, Effect, IDS, indexes, opt, spec};
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use openstitch_design::ObjectKind;
use openstitch_design::finish::{
    Finish, FinishMode, LetterTrim, MAX_TIE_COUNT, MAX_TRIM_DISTANCE, TIE_LENGTH_RANGE,
};
use serde_json::{Value, json};

pub(crate) fn commands() -> Vec<CommandSpec> {
    vec![spec(
        "object.setFinish",
        "How objects finish and connect (beyond PE-DESIGN; every default is PE-DESIGN's rule): trim after the object, its tie-in and lock, and a text's trims between letters. Only the settings given change; null puts one back to its default. Returns each object's finish",
        Effect::Edit,
        vec![
            IDS,
            opt(
                "trim",
                "auto|always|off",
                "Trim after the object: auto trims when the next start is farther than the trim distance (PE-DESIGN), always trims however short the move, off never trims (the frame jumps; a colour change still cuts)",
            ),
            opt(
                "trimDistance",
                "mm | null",
                "With trim auto: trim only when the next start is farther than this, 0-50 mm; null uses the design's trim distance",
            ),
            opt(
                "tie",
                "auto|always|off",
                "Tie-in and lock: auto ties the runs a trim separates (PE-DESIGN), always also ties in at the object's start and locks its end when no trim separates it, off sews no tie at all (for machines or threads that need none)",
            ),
            opt(
                "tieCount",
                "int | null",
                "Out-and-back steps of each tie-in and lock, 1-5 (each step is two stitches of tieLength); null: PE-DESIGN's own tie",
            ),
            opt(
                "tieLength",
                "mm | null",
                "Length of each tie stitch, 0.2-2 mm; null: PE-DESIGN's own tie (0.3 mm)",
            ),
            opt(
                "between",
                "auto|never|letter|word|line",
                "Text only: where it trims between letters. auto: moves over 2.3 mm (PE-DESIGN); never: no trims (short moves sewn across, longer ones jump); letter: after every letter; word: between words only; line: between lines only",
            ),
            opt(
                "reset",
                "bool",
                "Back to PE-DESIGN's rules first (alone: clears the finish)",
            ),
        ],
        set_finish,
    )]
}

fn mode(p: &Params, key: &str, old: FinishMode) -> Result<FinishMode, CommandError> {
    match p.get(key) {
        None if p.is_null(key) => Ok(FinishMode::Auto),
        None => Ok(old),
        Some(_) => FinishMode::parse(&p.string(key, None)?)
            .ok_or_else(|| p.invalid(key, "must be auto, always or off")),
    }
}

fn number(
    p: &Params,
    key: &str,
    old: Option<f64>,
    lo: f64,
    hi: f64,
) -> Result<Option<f64>, CommandError> {
    if p.is_null(key) {
        return Ok(None);
    }
    if !p.has(key) {
        return Ok(old);
    }
    p.number(key, None, Some((lo, hi))).map(Some)
}

/// `object.finish` as a JSON row: every setting, defaults included.
fn finish_json(id: u64, f: &Finish) -> Value {
    json!({
        "id": id,
        "trim": f.trim.as_str(),
        "trimDistance": f.trim_distance,
        "tie": f.tie.as_str(),
        "tieCount": f.tie_count,
        "tieLength": f.tie_length,
        "between": f.between.as_str(),
    })
}

fn set_finish(s: &mut EngineState, p: &Params) -> Result<Value, CommandError> {
    let idx = indexes(p, &s.design)?;
    if idx.is_empty() {
        return Err(p.invalid("ids", "must not be empty"));
    }
    let reset = p.bool("reset", Some(false))?;
    let mut new = Vec::with_capacity(idx.len());
    for &i in &idx {
        let o = &s.design.objects[i];
        let mut f = if reset {
            Finish::default()
        } else {
            o.finish.clone().unwrap_or_default()
        };
        f.trim = mode(p, "trim", f.trim)?;
        f.tie = mode(p, "tie", f.tie)?;
        f.trim_distance = number(p, "trimDistance", f.trim_distance, 0.0, MAX_TRIM_DISTANCE)?;
        f.tie_length = number(
            p,
            "tieLength",
            f.tie_length,
            TIE_LENGTH_RANGE.0,
            TIE_LENGTH_RANGE.1,
        )?;
        f.tie_count = number(p, "tieCount", f.tie_count.map(f64::from), 1.0, 5.0)?
            .map(|n| {
                if n.fract() != 0.0 {
                    Err(p.invalid("tieCount", "must be an integer"))
                } else {
                    Ok((n as u8).min(MAX_TIE_COUNT))
                }
            })
            .transpose()?;
        if p.is_null("between") {
            f.between = LetterTrim::Auto;
        } else if p.has("between") {
            f.between = LetterTrim::parse(&p.string("between", None)?)
                .ok_or_else(|| p.invalid("between", "must be auto, never, letter, word or line"))?;
            if f.between != LetterTrim::Auto && o.kind != ObjectKind::Text {
                return Err(p.invalid("between", format!("object {} is not a text", o.id)));
            }
        }
        new.push(f);
    }
    let mut out = Vec::with_capacity(idx.len());
    for (&i, f) in idx.iter().zip(new) {
        let o = &mut s.design.objects[i];
        out.push(finish_json(o.id, &f));
        o.finish = (!f.is_default()).then_some(f);
    }
    Ok(Value::Array(out))
}
