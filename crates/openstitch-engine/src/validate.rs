//! Checking a command's parameters without running it (#494), so a batch can report every
//! mistake at once, before any step runs.
//!
//! The check reads the same parameter list the catalogue documents (`req`/`opt` with a kind
//! string such as `mm`, `int 1-64`, `fill|satin` or `[[x, y]] mm`): required parameters are
//! present, no parameter is unknown, and each value has the documented type, integer range and
//! enum value. Anything a kind string does not describe (millimetre limits, whether an object
//! id exists, file paths) is left to the run, so a value this check passes can still fail when
//! the step runs; one it refuses would always fail or be ignored.

use serde_json::Value;

use crate::commands::ParamDoc;

/// One problem with one parameter of one command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamProblem {
    /// The parameter's name, or `params` when the parameters are not an object.
    pub param: String,
    pub message: String,
}

impl std::fmt::Display for ParamProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parameter '{}' {}", self.param, self.message)
    }
}

/// A documented parameter: its name, kind string and whether it is required.
pub type ParamShape<'a> = (&'a str, &'a str, bool);

/// Every problem with `params` against `docs`, in parameter order: missing required ones first
/// in doc order, then unknown and badly typed ones in the order given. Empty = fine to run.
///
/// `params` may be absent or `null` (no parameters). A JSON `null` value counts as absent, as
/// in [`Params`](crate::Params).
pub fn check_params<'a>(
    docs: impl IntoIterator<Item = ParamShape<'a>>,
    params: Option<&Value>,
) -> Vec<ParamProblem> {
    let docs: Vec<ParamShape<'a>> = docs.into_iter().collect();
    let problem = |param: &str, message: String| ParamProblem {
        param: param.to_string(),
        message,
    };
    let empty = serde_json::Map::new();
    let values = match params {
        None | Some(Value::Null) => &empty,
        Some(Value::Object(o)) => o,
        Some(_) => return vec![problem("params", "must be an object".into())],
    };
    let given = |k: &str| values.get(k).is_some_and(|v| !v.is_null());
    let mut out = Vec::new();
    for &(name, kind, required) in &docs {
        let present = if is_ids(name, kind) {
            given("ids") || given("id")
        } else {
            given(name)
        };
        if required && !present {
            out.push(problem(
                name,
                if is_ids(name, kind) {
                    "is missing (give ids or id)".into()
                } else {
                    "is missing".into()
                },
            ));
        }
    }
    for (key, value) in values {
        let doc = docs
            .iter()
            .find(|(name, kind, _)| *name == key || (key == "id" && is_ids(name, kind)));
        let Some(&(name, kind, _)) = doc else {
            let known: Vec<&str> = docs.iter().map(|(n, ..)| *n).collect();
            out.push(problem(
                key,
                if known.is_empty() {
                    "is not a parameter of this command (it takes none)".into()
                } else {
                    format!(
                        "is not a parameter of this command; it takes {}",
                        known.join(", ")
                    )
                },
            ));
            continue;
        };
        if value.is_null() {
            continue;
        }
        let kind = if key == "id" && is_ids(name, kind) {
            "int"
        } else if is_ids(name, kind) {
            "[int]"
        } else {
            kind
        };
        if let Some(message) = check_value(kind, value) {
            out.push(problem(key, message));
        }
    }
    out
}

/// [`check_params`] over a command's [`ParamDoc`]s.
pub fn check_docs(docs: &[ParamDoc], params: Option<&Value>) -> Vec<ParamProblem> {
    check_params(docs.iter().map(|d| (d.name, d.kind, d.required)), params)
}

/// [`check_params`] over a JSON parameter list as `commands.describe` and the editor's command
/// list give it: `[{"name", "type", "required"}]`.
pub fn check_listed(list: &[Value], params: Option<&Value>) -> Vec<ParamProblem> {
    let field = |p: &Value, k: &str| p.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let owned: Vec<(String, String, bool)> = list
        .iter()
        .map(|p| {
            (
                field(p, "name"),
                field(p, "type"),
                p.get("required").and_then(Value::as_bool).unwrap_or(false),
            )
        })
        .filter(|(n, ..)| !n.is_empty())
        .collect();
    check_params(
        owned.iter().map(|(n, k, r)| (n.as_str(), k.as_str(), *r)),
        params,
    )
}

fn is_ids(name: &str, kind: &str) -> bool {
    name == "ids" && kind.contains("id: int")
}

/// Why `value` does not fit `kind`, or `None` when it fits or the kind says too little to tell.
fn check_value(kind: &str, value: &Value) -> Option<String> {
    let kind = kind.trim();
    let kind = kind.strip_suffix("| null").map_or(kind, str::trim);
    let base = kind.strip_suffix(" mm").unwrap_or(kind);
    let number = |v: &Value| v.as_f64().filter(|n| n.is_finite());
    let integer = |v: &Value| number(v).filter(|n| n.fract() == 0.0);
    let pair = |v: &Value| {
        v.as_array()
            .is_some_and(|a| a.len() == 2 && a.iter().all(|n| number(n).is_some()))
    };
    match base {
        "string" | "digits" | "#rrggbb" => (!value.is_string()).then(|| "must be a string".into()),
        "bool" => (!value.is_boolean()).then(|| "must be true or false".into()),
        "int" => integer(value)
            .is_none()
            .then(|| "must be an integer".into()),
        "number" | "mm" | "degrees" | "factor" | "percent" => number(value)
            .is_none()
            .then(|| "must be a finite number".into()),
        _ if base.ends_with("/mm") => number(value)
            .is_none()
            .then(|| "must be a finite number".into()),
        "color" => match value {
            Value::String(_) => None,
            v if integer(v).is_some_and(|n| (1.0..=64.0).contains(&n)) => None,
            // A chart's thread (#796): {builtin, code | index} or {color, code, brand, description}.
            Value::Object(o) if o.contains_key("builtin") || o.contains_key("color") => None,
            _ => Some(
                "must be #rrggbb, a Brother thread name, chart number 1-64 or {builtin, code}"
                    .into(),
            ),
        },
        "[int]" => match value.as_array() {
            Some(a) if a.iter().all(|v| v.as_u64().is_some()) => None,
            _ => Some("must be an array of non-negative integers".into()),
        },
        "[string]" => match value.as_array() {
            Some(a) if a.iter().all(Value::is_string) => None,
            _ => Some("must be an array of strings".into()),
        },
        "[mm]" => match value.as_array() {
            Some(a) if a.iter().all(|v| number(v).is_some()) => None,
            _ => Some("must be an array of numbers (mm)".into()),
        },
        "[x, y]" => (!pair(value)).then(|| "must be [x, y] in mm".into()),
        "[[x, y]]" => match value.as_array() {
            Some(a) if a.iter().all(pair) => None,
            _ => Some("must be an array of [x, y] points in mm".into()),
        },
        _ => {
            if let Some(bounds) = kind.strip_prefix("int ") {
                let (lo, hi) = range(bounds)?;
                return match integer(value) {
                    Some(n) if (lo..=hi).contains(&n) => None,
                    _ => Some(format!("must be an integer in {lo}...{hi}")),
                };
            }
            if let Some((lo, hi)) = range(kind) {
                return match number(value) {
                    Some(n) if (lo..=hi).contains(&n) => None,
                    _ => Some(format!("must be a number in {lo}...{hi}")),
                };
            }
            if is_enum(kind) {
                let options: Vec<&str> = kind.split('|').collect();
                return (!in_enum(&options, value))
                    .then(|| format!("must be one of {}", options.join(", ")));
            }
            // `[a|b|c]`: an array of those values.
            if let Some(inner) = kind.strip_prefix('[').and_then(|k| k.strip_suffix(']'))
                && is_enum(inner)
            {
                let options: Vec<&str> = inner.split('|').collect();
                return match value.as_array() {
                    Some(a) if a.iter().all(|v| in_enum(&options, v)) => None,
                    _ => Some(format!("must be an array of {}", options.join(", "))),
                };
            }
            None
        }
    }
}

/// Whether `value` is one of `options`: a string matched without case, or a number whose
/// written form is one (`1|2` takes `1`).
fn in_enum(options: &[&str], value: &Value) -> bool {
    match value {
        Value::String(s) => options.iter().any(|o| o.eq_ignore_ascii_case(s)),
        Value::Number(n) => options.iter().any(|o| {
            o.parse::<f64>()
                .ok()
                .zip(n.as_f64())
                .is_some_and(|(a, b)| a == b)
        }),
        _ => false,
    }
}

/// `a|b|c` with nothing open-ended (`pes|dst|...` lists only some of the values).
fn is_enum(kind: &str) -> bool {
    kind.contains('|')
        && !kind.contains(' ')
        && !kind.contains("...")
        && !kind.contains('…')
        && !kind.contains('[')
        && kind.split('|').all(|o| !o.is_empty())
}

/// `lo-hi` or `lo–hi` (en dash) with numeric bounds, such as `1-64`, `0–1`, `-100–100`.
fn range(text: &str) -> Option<(f64, f64)> {
    let text = text.trim();
    let (lo, hi) = match text.split_once('–') {
        Some(p) => p,
        None => {
            // A hyphen after the first character: `-5-5` splits after `-5`.
            let at = text.char_indices().skip(1).find(|&(_, c)| c == '-')?.0;
            (&text[..at], &text[at + 1..])
        }
    };
    let lo: f64 = lo.trim().parse().ok()?;
    let hi: f64 = hi.trim().parse().ok()?;
    (lo <= hi).then_some((lo, hi))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DOCS: &[ParamShape<'static>] = &[
        ("ids", "[int] | id: int", true),
        ("width", "mm", false),
        ("count", "int 1-10", false),
        ("type", "fill|satin|none", false),
        ("color", "color", false),
        ("points", "[[x, y]] mm", false),
        ("at", "[x, y] mm | null", false),
        ("strength", "0–1", false),
        ("format", "pes|dst|...", false),
        ("name", "string", false),
        ("on", "bool", false),
        ("extra", "{min?, max?}", false),
    ];

    fn check(v: Value) -> Vec<ParamProblem> {
        check_params(DOCS.iter().copied(), Some(&v))
    }

    fn names(p: &[ParamProblem]) -> Vec<&str> {
        p.iter().map(|p| p.param.as_str()).collect()
    }

    #[test]
    fn good_params_pass() {
        assert!(
            check(
                json!({"ids": [1, 2], "width": 2.5, "count": 3, "type": "Satin",
            "color": "#ff0000", "points": [[0, 0], [1.5, 2]], "at": null, "strength": 0.5,
            "format": "anything", "name": "x", "on": true, "extra": {"min": 1}})
            )
            .is_empty()
        );
        assert!(check(json!({"id": 4, "color": 12, "at": [1, 2]})).is_empty());
    }

    #[test]
    fn every_problem_is_reported_at_once() {
        let p = check(
            json!({"widht": 3, "count": 11, "type": "zigzag", "on": "yes",
            "color": 65, "points": [[0, 0], [1]], "strength": 2, "width": "3mm"}),
        );
        assert_eq!(p.len(), 9, "{p:?}");
        assert_eq!(p[0].param, "ids", "missing ones come first");
        assert!(p[0].message.contains("ids or id"));
        let by = |n: &str| p.iter().find(|p| p.param == n).expect(n).message.clone();
        assert!(by("widht").contains("not a parameter") && by("widht").contains("width"));
        assert_eq!(by("count"), "must be an integer in 1...10");
        assert_eq!(by("type"), "must be one of fill, satin, none");
        assert_eq!(by("on"), "must be true or false");
        assert!(by("color").contains("1-64"));
        assert!(by("points").contains("[x, y]"));
        assert_eq!(by("strength"), "must be a number in 0...1");
        assert_eq!(by("width"), "must be a finite number");
    }

    #[test]
    fn params_must_be_an_object_and_none_is_fine() {
        assert_eq!(names(&check(json!([1]))), ["params"]);
        assert!(check_params([("a", "mm", false)], None).is_empty());
        let p = check_params([], Some(&json!({"x": 1})));
        assert!(p[0].message.contains("takes none"));
    }

    #[test]
    fn listed_params_and_ranges() {
        let list = vec![
            json!({"name": "size", "type": "int 64-2048", "required": true}),
            json!({"name": "mode", "type": "a|b"}),
        ];
        let p = check_listed(&list, Some(&json!({"size": 10, "mode": "c"})));
        let mut n = names(&p);
        n.sort_unstable();
        assert_eq!(n, ["mode", "size"]);
        assert_eq!(range("-100–100"), Some((-100.0, 100.0)));
        assert_eq!(range("0.25-50"), Some((0.25, 50.0)));
        assert_eq!(range("-5-5"), Some((-5.0, 5.0)));
        assert_eq!(range("x-y"), None);
        assert!(!is_enum("pes|dst|…") && !is_enum("[int] | id: int") && is_enum("x|y"));
        let list = vec![
            json!({"name": "st", "type": "1|2"}),
            json!({"name": "parts", "type": "[regionColor|lineColor]"}),
        ];
        assert!(check_listed(&list, Some(&json!({"st": 2, "parts": ["lineColor"]}))).is_empty());
        let bad = check_listed(&list, Some(&json!({"st": 3, "parts": ["x"]})));
        let mut n = names(&bad);
        n.sort_unstable();
        assert_eq!(n, ["parts", "st"]);
    }
}
