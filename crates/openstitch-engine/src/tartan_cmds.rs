//! Tartan (plaid) fill (#539, beyond PE-DESIGN): `shape.tartan` turns a filled shape into one
//! fill object per colour of a sett, grouped and next to each other in the sewing order, each
//! sewing its colour's warp and weft stripes (see `openstitch_design::tartan`).

use crate::commands::{CommandSpec, Effect, opt, req, spec, thread};
use crate::params::{CommandError, Params};
use openstitch_core::Thread;
use openstitch_design::{RegionSewing, RegionType, Tartan, TartanStripe};
use serde_json::{Value, json};

/// Most stripes of one sett, and most colours.
pub const MAX_STRIPES: usize = 32;
pub const MAX_COLOURS: usize = 8;

/// The usual tartan letter codes and the Brother threads they are sewn in.
const LETTERS: [(&str, &str); 12] = [
    ("R", "Red"),
    ("G", "Emerald Green"),
    ("B", "Blue"),
    ("K", "Black"),
    ("W", "White"),
    ("Y", "Yellow"),
    ("N", "Gray"),
    ("O", "Orange"),
    ("P", "Purple"),
    ("A", "Sky Blue"),
    ("T", "Brown"),
    ("DB", "Prussian Blue"),
];

pub fn commands() -> Vec<CommandSpec> {
    vec![spec(
        "shape.tartan",
        "Tartan (plaid) fill (beyond PE-DESIGN): the shape becomes one fill object per colour of the sett, grouped and sewn one after another; each sews the warp rows of its colour's vertical stripes and the weft rows of its horizontal ones, half density each, so the colours mix where stripes cross as in a woven plaid. The stripes run along the fill direction (angle) and square to it",
        Effect::Edit,
        vec![
            req(
                "id",
                "int",
                "A closed shape (its fill settings are kept; it becomes a fill)",
            ),
            req(
                "sett",
                "[{color, width}] | \"R8 G24 W4\"",
                "The stripes, 2-32, in order: a colour (as for fill: #rrggbb, thread name or chart number) and a width in mm (0.5-100), or the usual notation: letter codes with widths in mm (R red, G green, B blue, K black, W white, Y yellow, N grey, O orange, P purple, A sky blue, T brown, DB dark blue). At most 8 colours",
            ),
            opt(
                "symmetric",
                "bool",
                "Mirror the sett about its first and last stripes, as most tartans are (default true)",
            ),
        ],
        |s, p| {
            let id = p.int("id", None, 0, i64::MAX)? as u64;
            let i = s
                .design
                .index_of(id)
                .ok_or_else(|| p.not_found(format!("object {id}")))?;
            let o = &s.design.objects[i];
            if !o.contours.iter().any(|c| c.closed) || o.text.is_some() {
                return Err(p.failed(format!("object {id} is not a closed shape")));
            }
            let stripes = sett(p)?;
            let mut colours: Vec<Thread> = Vec::new();
            let mut set: Vec<TartanStripe> = Vec::new();
            for (t, width) in stripes {
                let color = match colours
                    .iter()
                    .position(|c| (c.red, c.green, c.blue) == (t.red, t.green, t.blue))
                {
                    Some(k) => k,
                    None => {
                        colours.push(t);
                        colours.len() - 1
                    }
                };
                set.push(TartanStripe { color, width });
            }
            if colours.len() < 2 || colours.len() > MAX_COLOURS {
                return Err(p.invalid("sett", "needs 2-8 different colours"));
            }
            let symmetric = p.bool("symmetric", Some(true))?;
            let base = s.design.objects[i].clone();
            let group = match base.group {
                Some(g) => g,
                None => s.design.new_group_id(),
            };
            let mut region = base
                .region
                .clone()
                .unwrap_or_else(|| RegionSewing::new(RegionType::Fill, colours[0].clone()));
            if region.kind != RegionType::Fill {
                region = RegionSewing::new(RegionType::Fill, region.thread.clone());
            }
            region.underlay = false;
            region.gradient = None;
            region.wave_height = None;
            region.angle_lines.clear();
            let mut ids = Vec::with_capacity(colours.len());
            for (k, t) in colours.iter().enumerate() {
                let mut r = region.clone();
                r.thread = t.clone();
                r.tartan = Some(Tartan {
                    sett: set.clone(),
                    symmetric,
                    layer: k,
                });
                let mut obj = base.clone();
                obj.region = Some(r);
                obj.group = Some(group);
                if k == 0 {
                    s.design.objects[i] = obj;
                    ids.push(id);
                } else {
                    let new = s.design.add(obj);
                    let from = s.design.index_of(new).expect("just added");
                    let moved = s.design.objects.remove(from);
                    s.design.objects.insert(i + k, moved);
                    ids.push(new);
                }
            }
            Ok(json!({ "ids": ids, "group": group, "colors": colours.len() }))
        },
    )]
}

/// The `sett`: a list of {color, width} or the letter notation, as (thread, width mm).
fn sett(p: &Params) -> Result<Vec<(Thread, f64)>, CommandError> {
    let bad = |why: &str| p.invalid("sett", why.to_string());
    let width = |w: f64| -> Result<f64, CommandError> {
        if (0.5..=100.0).contains(&w) {
            Ok(w)
        } else {
            Err(bad("widths are 0.5-100 mm"))
        }
    };
    let colour = |v: &Value, k: usize| -> Result<Thread, CommandError> {
        let one = Params::new(&p.command, Some(&json!({ "color": v })))?;
        thread(&one, "color").ok().flatten().ok_or_else(|| {
            p.invalid(
                "sett",
                format!(
                    "stripe {} needs #rrggbb, a Brother thread name or chart number 1-64",
                    k + 1
                ),
            )
        })
    };
    let out: Vec<(Thread, f64)> = match p.get("sett") {
        Some(Value::String(s)) => s
            .split_whitespace()
            .enumerate()
            .map(|(k, tok)| {
                let split = tok
                    .find(|c: char| c.is_ascii_digit() || c == '.')
                    .ok_or_else(|| bad("each stripe is a letter code and a width, as R8"))?;
                let (code, w) = tok.split_at(split);
                let name = LETTERS
                    .iter()
                    .find(|(c, _)| c.eq_ignore_ascii_case(code))
                    .map(|(_, n)| *n)
                    .ok_or_else(|| {
                        p.invalid(
                            "sett",
                            format!("unknown colour code {code} in stripe {}", k + 1),
                        )
                    })?;
                let w: f64 = w.parse().map_err(|_| bad("widths are numbers (mm)"))?;
                Ok((colour(&json!(name), k)?, width(w)?))
            })
            .collect::<Result<_, CommandError>>()?,
        Some(Value::Array(a)) => a
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let c = s
                    .get("color")
                    .ok_or_else(|| bad("each stripe needs a color"))?;
                let w = s
                    .get("width")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| bad("each stripe needs a width (mm)"))?;
                Ok((colour(c, k)?, width(w)?))
            })
            .collect::<Result<_, CommandError>>()?,
        _ => {
            return Err(bad(
                "must be a list of {color, width} or a sett like \"R8 G24 W4\"",
            ));
        }
    };
    if !(2..=MAX_STRIPES).contains(&out.len()) {
        return Err(bad("needs 2-32 stripes"));
    }
    Ok(out)
}
