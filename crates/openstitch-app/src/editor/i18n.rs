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
            CURRENT.with(|l| l.set(c));
            true
        }
        None => false,
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
    let mut out: Vec<(&str, String)> = Vec::new();
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
            let Some((_, text)) = out.last_mut().filter(|_| open) else {
                return Err(at("an indented line belongs under a message"));
            };
            text.push('\n');
            text.push_str(&unescape(line.trim()).map_err(|e| at(&e))?);
            continue;
        }
        let (id, value) = line
            .split_once('=')
            .ok_or_else(|| at("expected `id = text`"))?;
        let id = id.trim();
        if !is_id(id) {
            return Err(at(&format!("`{id}` is not a message id")));
        }
        if out.iter().any(|(i, _)| *i == id) {
            return Err(at(&format!("`{id}` appears twice")));
        }
        out.push((id, unescape(value.trim()).map_err(|e| at(&e))?));
        open = true;
    }
    Ok(out)
}

/// Fluent's identifier: a letter, then letters, digits, `-` and `_`.
fn is_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Text with Fluent's brace literals resolved; any other placeable is refused.
fn unescape(text: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some(r) = rest.strip_prefix("{\"{\"}") {
            out.push('{');
            rest = r;
        } else if let Some(r) = rest.strip_prefix("{\"}\"}") {
            out.push('}');
            rest = r;
        } else {
            return Err(
                "placeables are not supported yet; write braces as {\"{\"} and {\"}\"}".into(),
            );
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// `text` written for a catalogue: braces as Fluent literals, line breaks as indented lines.
pub fn escape(text: &str) -> String {
    text.replace('{', "\u{0}")
        .replace('}', "{\"}\"}")
        .replace('\u{0}', "{\"{\"}")
        .replace('\n', "\n    ")
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
    fn slugs_are_message_ids() {
        assert_eq!(slug("Color / Sew Type"), "color-sew-type");
        assert_eq!(slug("Hoop Settings"), "hoop-settings");
        assert!(is_id(&format!("group-{}", slug("Shapes (Outline)"))));
    }
}
