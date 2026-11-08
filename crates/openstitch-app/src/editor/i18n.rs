//! Interface languages (#555): the text the window shows, looked up by message id in the chosen
//! language, with the English text in the code as the source and the fallback.
//!
//! A language is a catalogue in `assets/i18n/<code>.ftl`, written in a subset of Project
//! Fluent's syntax (`id = text`, `#` comments, indented continuation lines, `{"{"}` and `{"}"}`
//! for braces), so the files stay readable by Fluent's own tools. `assets/i18n/en.ftl` lists every
//! id with its English text (generated from the code, see the test
//! `the_english_catalogue_is_current`): a translator copies it and translates the right-hand
//! sides. An id missing from a catalogue shows its English text, never the id. `pseudo` is a
//! built-in test language: the English text accented and in brackets, so anything still
//! hard-coded stands out on screen.
//!
//! The language is per thread (the window draws on one): [`set_language`] once per frame from
//! the user's preference. Texts are `&'static str`, so the window's tables keep their types; each
//! language's texts are made once and kept for the session.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::OnceLock;

/// English: the text in the code.
pub const ENGLISH: &str = "en";
/// The test language: accented English in brackets.
pub const PSEUDO: &str = "pseudo";

/// The translated catalogues shipped with OpenStitch: code, name (in that language), file.
/// Adding a language is a file in `assets/i18n/` and a line here.
const CATALOGUES: &[(&str, &str, &str)] = &[];

thread_local! {
    static CURRENT: Cell<&'static str> = const { Cell::new(ENGLISH) };
    /// Pseudo texts made so far, by English text.
    static PSEUDO_TEXTS: RefCell<HashMap<&'static str, &'static str>> = RefCell::new(HashMap::new());
}

/// The shipped catalogues' codes and files (for the checks that their ids exist).
pub fn shipped() -> impl Iterator<Item = (&'static str, &'static str)> {
    CATALOGUES.iter().map(|(code, _, source)| (*code, *source))
}

/// Every language the window offers: code and name, English first.
pub fn languages() -> Vec<(&'static str, &'static str)> {
    let mut out = vec![(ENGLISH, "English")];
    out.extend(CATALOGUES.iter().map(|(code, name, _)| (*code, *name)));
    out.push((PSEUDO, "Pseudo-locale (test)"));
    out
}

/// Whether `code` is one of [`languages`].
pub fn is_language(code: &str) -> bool {
    languages().iter().any(|(c, _)| *c == code)
}

/// The language this thread shows.
pub fn language() -> &'static str {
    CURRENT.with(Cell::get)
}

/// Shows `code` on this thread from now on; an unknown code is refused (`false`) and the
/// language stays.
pub fn set_language(code: &str) -> bool {
    match languages().into_iter().find(|(c, _)| *c == code) {
        Some((c, _)) => {
            if CURRENT.with(|l| l.replace(c)) != c {
                // The printed sheets follow (#555).
                if c == ENGLISH {
                    openstitch_print::texts::english();
                } else {
                    openstitch_print::texts::set_texts(pattern);
                }
            }
            true
        }
        None => false,
    }
}

/// Message `id`'s pattern in this thread's language with its placeholders kept: for a text
/// filled in elsewhere (the printed sheets).
pub fn pattern(id: &str, english: &str) -> String {
    match language() {
        ENGLISH => english.to_string(),
        PSEUDO => {
            let mut out = String::from("[");
            let mut depth = 0;
            for c in english.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                out.push(if depth == 0 { accent(c) } else { c });
            }
            out.push(']');
            out
        }
        code => catalogue(code)
            .and_then(|c| c.get(id).copied())
            .unwrap_or(english)
            .to_string(),
    }
}

/// The text for message `id` in this thread's language; `english` when the language has none.
pub fn text(id: &str, english: &'static str) -> &'static str {
    match language() {
        ENGLISH => english,
        PSEUDO => pseudo(english),
        code => catalogue(code)
            .and_then(|c| c.get(id).copied())
            .unwrap_or(english),
    }
}

/// Whether `text` is a message with variables (Fluent placeables `{ $name }`), not plain text.
pub fn is_message(text: &str) -> bool {
    text.contains("{ $")
}

/// Message `id` in this thread's language with its variables filled in: `english` is the
/// message in a subset of Fluent's syntax, `{ $name }` for a variable and
/// `{ $n -> [one] … *[other] … }` (one variant a line) for a choice by the number's plural
/// category or an exact value. A variable `args` does not give shows as `{$name}`.
pub fn message(id: &str, english: &'static str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let lang = language();
    let pattern = match lang {
        ENGLISH | PSEUDO => english,
        code => catalogue(code)
            .and_then(|c| c.get(id).copied())
            .unwrap_or(english),
    };
    let values: Vec<(&str, String)> = args.iter().map(|(n, v)| (*n, v.to_string())).collect();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    let out = format_seq(&chars, &mut i, false, &values, lang);
    if lang == PSEUDO {
        format!("[{out}]")
    } else {
        out
    }
}

/// The plural category of `n` in `lang` (CLDR's names). English's rule for now: "one" for
/// exactly 1, else "other"; a shipped language adds its own rule here.
pub fn plural_category(lang: &str, n: f64) -> &'static str {
    match lang {
        _ if n == 1.0 => "one",
        _ => "other",
    }
}

/// Formats a pattern from `i`; in a variant (`variant`), stops before the next variant or the
/// select's closing brace.
fn format_seq(
    s: &[char],
    i: &mut usize,
    variant: bool,
    args: &[(&str, String)],
    lang: &str,
) -> String {
    let mut out = String::new();
    while *i < s.len() {
        let c = s[*i];
        if c == '{' {
            out.push_str(&placeable(s, i, args, lang));
            continue;
        }
        if variant && c == '}' {
            break;
        }
        if variant && c == '\n' {
            let mut j = *i + 1;
            while j < s.len() && s[j] == ' ' {
                j += 1;
            }
            if j >= s.len() || matches!(s[j], '[' | '*' | '}') {
                *i = j;
                break;
            }
        }
        out.push(if lang == PSEUDO { accent(c) } else { c });
        *i += 1;
    }
    out
}

/// `{ $name }` or a select, from the `{` at `i`.
fn placeable(s: &[char], i: &mut usize, args: &[(&str, String)], lang: &str) -> String {
    let skip = |i: &mut usize| {
        while *i < s.len() && s[*i].is_whitespace() {
            *i += 1;
        }
    };
    *i += 1;
    skip(i);
    if s.get(*i) != Some(&'$') {
        // Not a variable: shown as it is.
        return "{".into();
    }
    *i += 1;
    let start = *i;
    while *i < s.len() && (s[*i].is_ascii_alphanumeric() || s[*i] == '_' || s[*i] == '-') {
        *i += 1;
    }
    let name: String = s[start..*i].iter().collect();
    let value = args
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v.clone());
    skip(i);
    if s.get(*i) == Some(&'}') {
        *i += 1;
        return value.unwrap_or_else(|| format!("{{${name}}}"));
    }
    if s.get(*i) != Some(&'-') || s.get(*i + 1) != Some(&'>') {
        return value.unwrap_or_default();
    }
    *i += 2;
    let mut variants: Vec<(String, bool, String)> = Vec::new();
    loop {
        skip(i);
        match s.get(*i) {
            None => break,
            Some('}') => {
                *i += 1;
                break;
            }
            _ => {}
        }
        let default = s.get(*i) == Some(&'*');
        if default {
            *i += 1;
        }
        if s.get(*i) != Some(&'[') {
            break;
        }
        *i += 1;
        let k = *i;
        while *i < s.len() && s[*i] != ']' {
            *i += 1;
        }
        let key: String = s[k..*i].iter().collect::<String>().trim().to_string();
        *i += 1;
        while *i < s.len() && s[*i] == ' ' {
            *i += 1;
        }
        let text = format_seq(s, i, true, args, lang);
        variants.push((key, default, text.trim_end().to_string()));
    }
    let v = value.unwrap_or_default();
    let category = v
        .trim()
        .parse::<f64>()
        .ok()
        .map(|n| plural_category(lang, n));
    variants
        .iter()
        .find(|(k, _, _)| *k == v.trim())
        .or_else(|| {
            variants
                .iter()
                .find(|(k, _, _)| Some(k.as_str()) == category)
        })
        .or_else(|| variants.iter().find(|(_, d, _)| *d))
        .map(|(_, _, t)| t.clone())
        .unwrap_or_default()
}

/// The variables a message uses (`$name`), in order of first use.
pub fn variables(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, _) in text.match_indices('$') {
        let name: String = text[i + 1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// `english` accented and in brackets: "[Ópéñ]".
fn pseudo(english: &'static str) -> &'static str {
    PSEUDO_TEXTS.with(|p| {
        *p.borrow_mut().entry(english).or_insert_with(|| {
            let accented: String = english.chars().map(accent).collect();
            Box::leak(format!("[{accented}]").into_boxed_str())
        })
    })
}

fn accent(c: char) -> char {
    match c {
        'a' => 'á',
        'e' => 'é',
        'i' => 'í',
        'o' => 'ó',
        'u' => 'ú',
        'n' => 'ñ',
        'A' => 'Á',
        'E' => 'É',
        'I' => 'Í',
        'O' => 'Ó',
        'U' => 'Ú',
        'N' => 'Ñ',
        c => c,
    }
}

type Texts = HashMap<&'static str, &'static str>;

/// A shipped catalogue, parsed once.
fn catalogue(code: &str) -> Option<&'static Texts> {
    static PARSED: OnceLock<Vec<Texts>> = OnceLock::new();
    let parsed = PARSED.get_or_init(|| {
        CATALOGUES
            .iter()
            .map(|(_, _, source)| {
                // The tests parse every catalogue, so a broken file never ships.
                parse(source)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(id, text)| (id, &*Box::leak(text.into_boxed_str())))
                    .collect()
            })
            .collect()
    });
    let i = CATALOGUES.iter().position(|(c, _, _)| *c == code)?;
    parsed.get(i)
}

/// Reads a catalogue: `(id, text)` in file order, or the first problem with its line number.
pub fn parse(source: &str) -> Result<Vec<(&str, String)>, String> {
    // Each message's lines are joined first, then read as one text (a plural choice spans
    // lines); the line number in an error is the message's first.
    let mut raw: Vec<(&str, String, usize)> = Vec::new();
    let mut open = false;
    for (n, line) in source.lines().enumerate() {
        let at = |e: &str| format!("line {}: {e}", n + 1);
        if line.trim().is_empty() {
            open = false;
            continue;
        }
        if line.starts_with('#') {
            open = false;
            continue;
        }
        if line.starts_with([' ', '\t']) {
            let Some((_, text, _)) = raw.last_mut().filter(|_| open) else {
                return Err(at("an indented line belongs under a message"));
            };
            text.push('\n');
            text.push_str(line.trim());
            continue;
        }
        let (id, value) = line
            .split_once('=')
            .ok_or_else(|| at("expected `id = text`"))?;
        let id = id.trim();
        if !is_id(id) {
            return Err(at(&format!("`{id}` is not a message id")));
        }
        if raw.iter().any(|(i, _, _)| *i == id) {
            return Err(at(&format!("`{id}` appears twice")));
        }
        raw.push((id, value.trim().to_string(), n + 1));
        open = true;
    }
    raw.into_iter()
        .map(|(id, text, line)| {
            unescape(&text)
                .map(|t| (id, t))
                .map_err(|e| format!("line {line}: {e}"))
        })
        .collect()
}

/// Fluent's identifier: a letter, then letters, digits, `-` and `_`.
fn is_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Text with Fluent's string literals (`{"{"}`, `{"  "}` for spaces at an end) resolved; any
/// other placeable is refused.
fn unescape(text: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        // A message's placeable (`{ $n }`, a select): kept for `message` to fill in.
        if rest.starts_with('{') && rest[1..].trim_start().starts_with('$') {
            let mut depth = 0;
            let mut end = None;
            for (k, ch) in rest.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(k);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let end = end.ok_or("a placeable is not closed")?;
            out.push_str(&rest[..=end]);
            rest = &rest[end + 1..];
            continue;
        }
        let literal = rest
            .strip_prefix("{\"")
            .and_then(|r| r.find("\"}").map(|j| (&r[..j], &r[j + 2..])));
        match literal {
            Some((text, after)) if !text.contains('"') => {
                out.push_str(text);
                rest = after;
            }
            _ => {
                return Err(
                    "placeables are not supported yet; write braces as {\"{\"} and {\"}\"}".into(),
                );
            }
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// `text` written for a catalogue: braces as Fluent literals, spaces at either end in a literal
/// (a catalogue value is trimmed), line breaks as indented lines. A message's placeables stay
/// as they are.
pub fn escape(text: &str) -> String {
    if is_message(text) {
        return text.replace('\n', "\n    ");
    }
    let core = text.trim();
    let (lead, trail) = (
        &text[..text.len() - text.trim_start().len()],
        &text[text.trim_end().len()..],
    );
    let core = core
        .replace('{', "\u{0}")
        .replace('}', "{\"}\"}")
        .replace('\u{0}', "{\"{\"}")
        .replace('\n', "\n    ");
    let wrap = |w: &str| {
        if w.is_empty() {
            String::new()
        } else {
            format!("{{\"{w}\"}}")
        }
    };
    format!("{}{core}{}", wrap(lead), wrap(trail))
}

/// A message id made from a name: lowercase, every run of other characters one `-`
/// ("Color / Sew Type" → "color-sew-type").
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_text_in_the_code_and_pseudo_brackets_it() {
        assert_eq!(language(), ENGLISH);
        assert_eq!(text("action-open-label", "Open"), "Open");
        assert!(set_language(PSEUDO));
        assert_eq!(text("action-open-label", "Open"), "[Ópéñ]");
        assert!(
            std::ptr::eq(text("x", "Open"), text("y", "Open")),
            "made once"
        );
        assert!(!set_language("xx"), "unknown");
        assert_eq!(language(), PSEUDO, "kept");
        assert!(set_language(ENGLISH));
        assert_eq!(text("action-open-label", "Open"), "Open");
    }

    #[test]
    fn catalogues_read_fluent_messages_and_refuse_what_they_cannot() {
        let src = "# A comment\nhello = Hallo\nlong = Erste Zeile\n    zweite Zeile\n\nbrace = {\"{\"}a{\"}\"}\n";
        let m = parse(src).expect("parses");
        assert_eq!(
            m,
            vec![
                ("hello", "Hallo".to_string()),
                ("long", "Erste Zeile\nzweite Zeile".to_string()),
                ("brace", "{a}".to_string()),
            ]
        );
        assert!(parse("1x = y").expect_err("bad id").contains("line 1"));
        assert!(parse("a = b\na = c").expect_err("twice").contains("twice"));
        assert!(
            parse("a = {name}")
                .expect_err("placeable")
                .contains("placeables")
        );
        assert!(parse("  stray").is_err());
        assert!(parse("no equals").is_err());
        // escape() round-trips through parse().
        assert_eq!(
            parse(&format!("p = {}", escape("  (folded) "))).expect("spaces")[0].1,
            "  (folded) "
        );
        let tricky = "Keys {\"a\": 1}\nnext";
        let line = format!("t = {}", escape(tricky));
        assert_eq!(parse(&line).expect("round trip")[0].1, tricky);
    }

    #[test]
    fn every_shipped_catalogue_parses() {
        for (code, name, source) in CATALOGUES {
            assert!(!name.is_empty(), "{code}");
            parse(source).unwrap_or_else(|e| panic!("assets/i18n/{code}.ftl: {e}"));
        }
        assert!(is_language(ENGLISH) && is_language(PSEUDO));
    }

    #[test]
    fn messages_fill_in_variables_and_choose_plurals() {
        let moved = "Moved { $n ->\n[one] one item\n*[other] { $n } items\n} to the trash";
        let n = |v: usize| message("x", moved, &[("n", &v)]);
        assert_eq!(n(1), "Moved one item to the trash");
        assert_eq!(n(3), "Moved 3 items to the trash");
        assert_eq!(n(0), "Moved 0 items to the trash");
        let opened = "Opened { $path }";
        assert_eq!(message("y", opened, &[("path", &"a.pes")]), "Opened a.pes");
        assert_eq!(
            message("y", opened, &[]),
            "Opened {$path}",
            "a missing value shows"
        );
        assert_eq!(variables(moved), vec!["n".to_string()]);
        // Through the catalogue format and back.
        let line = format!("m = {}", escape(moved));
        let back = &parse(&line).expect("parses")[0].1;
        assert_eq!(back, moved);
        // The pseudo-locale accents the text, not the values.
        assert!(set_language(PSEUDO));
        assert_eq!(
            message("y", opened, &[("path", &"a.pes")]),
            "[Ópéñéd a.pes]"
        );
        assert!(set_language(ENGLISH));
    }

    /// The printed sheets follow the language (#555).
    #[test]
    fn printed_sheets_follow_the_language() {
        assert!(set_language(ENGLISH));
        assert_eq!(openstitch_print::texts::text("print-customer"), "Customer");
        assert!(set_language(PSEUDO));
        assert_eq!(
            openstitch_print::texts::text("print-customer"),
            "[Cústómér]"
        );
        assert_eq!(
            openstitch_print::texts::fill("print-reduced", &[("percent", "50".into())]),
            "[Rédúcéd tó 50 %]",
            "values filled in, not accented"
        );
        assert!(set_language(ENGLISH));
        assert_eq!(openstitch_print::texts::text("print-customer"), "Customer");
    }

    #[test]
    fn slugs_are_message_ids() {
        assert_eq!(slug("Color / Sew Type"), "color-sew-type");
        assert_eq!(slug("Hoop Settings"), "hoop-settings");
        assert!(is_id(&format!("group-{}", slug("Shapes (Outline)"))));
    }
}
