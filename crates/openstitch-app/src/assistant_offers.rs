//! What the Assistant offers besides its reply (#1172): clarifying questions on a card of answer
//! chips, suggested replies (one message and up to four chips), and fix chips when Check Design
//! reports errors or warnings at the end of a turn.
//!
//! The model hands them over with the `ask_user` and `suggest_replies` tools (`ui.askUser`,
//! `ui.suggestReplies`), the same for the Claude API, OpenAI-compatible endpoints and coding
//! agents. Questions do not hold the turn: the model ends it, and the card sends the answers as
//! one ordinary user message. Everything the model sends is untrusted text: it is made plain
//! (no control characters, one line except the suggested message) and cut to a length, and a
//! click only fills the composer or sends a message as the user, which the chat shows and the
//! usual permission questions follow.

use crate::app::EditorApp;
use openstitch_assistant as claude;
use crate::assistant_panel::Item;
use eframe::egui::{self, RichText};
use openstitch_app::editor::ui_texts::t;
use openstitch_app::tf;
use serde_json::{Value, json};

/// At most this many questions on one card.
pub const MAX_QUESTIONS: usize = 10;
/// Answer chips per question.
pub const MIN_CHOICES: usize = 2;
pub const MAX_CHOICES: usize = 4;
/// Suggestion chips after a reply.
pub const MAX_CHIPS: usize = 4;
/// Longest texts kept, in characters.
pub const QUESTION_CHARS: usize = 200;
pub const CHOICE_CHARS: usize = 60;
pub const CHIP_CHARS: usize = 80;
pub const MESSAGE_CHARS: usize = 300;
/// Findings listed in a fix chip's message; the rest are counted.
pub const MAX_FINDINGS: usize = 30;

#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub text: String,
    pub choices: Vec<String>,
    /// The chip the user picked.
    pub picked: Option<usize>,
    /// What the user typed instead; it wins over a picked chip.
    pub typed: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Questionnaire {
    pub intro: String,
    pub questions: Vec<Question>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Suggestions {
    pub message: String,
    pub chips: Vec<String>,
}

/// A fix chip: its label and the message a click sends.
#[derive(Clone, Debug, PartialEq)]
pub struct FixChip {
    pub label: String,
    pub message: String,
}

#[derive(Default)]
pub struct Offers {
    /// What the model sent during the running turn, shown when it ends.
    asked: Option<Questionnaire>,
    suggested: Option<Suggestions>,
    /// Shown since the last turn: in conversation `chat`, while no user message follows item
    /// `after` (a new chat, a typed message or a click puts them away).
    chat: u64,
    after: usize,
    pub questions: Option<Questionnaire>,
    pub suggestions: Option<Suggestions>,
    pub fixes: Vec<FixChip>,
    /// Set while a job of the Assistant's own turn runs, so only the Assistant (not a script on
    /// the automation socket) can ask or suggest.
    pub from_assistant: bool,
}

/// Plain text of at most `max` characters: control characters go, whitespace runs become one
/// space (line breaks too unless `lines`), and a cut ends with "…".
pub fn plain(s: &str, max: usize, lines: bool) -> String {
    let mut out = String::new();
    let mut space = false;
    for c in s.chars() {
        if c == '\n' && lines {
            out.truncate(out.trim_end_matches(' ').len());
            out.push('\n');
            space = false;
        } else if c.is_whitespace() || c.is_control() {
            space = !out.is_empty() && !out.ends_with('\n');
        } else {
            if space {
                out.push(' ');
                space = false;
            }
            out.push(c);
        }
    }
    let out = out.trim().to_string();
    if out.chars().count() <= max {
        return out;
    }
    let mut cut: String = out.chars().take(max.saturating_sub(1)).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    cut
}

fn str_list(v: &Value, key: &str) -> Result<Vec<String>, String> {
    v.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("'{key}' must be a list of strings"))?
        .iter()
        .map(|s| {
            s.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("'{key}' must be a list of strings"))
        })
        .collect()
}

/// `ui.askUser`'s params: `questions` [{question, choices}], 1 to [`MAX_QUESTIONS`], each with
/// [`MIN_CHOICES`] to [`MAX_CHOICES`] answers, and an optional `intro`.
pub fn parse_questions(params: &Value) -> Result<Questionnaire, String> {
    let list = params
        .get("questions")
        .and_then(Value::as_array)
        .filter(|l| (1..=MAX_QUESTIONS).contains(&l.len()))
        .ok_or_else(|| format!("'questions' must be a list of 1 to {MAX_QUESTIONS} questions"))?;
    let mut questions = Vec::new();
    for q in list {
        let text = plain(
            q.get("question").and_then(Value::as_str).unwrap_or(""),
            QUESTION_CHARS,
            false,
        );
        if text.is_empty() {
            return Err("every question needs its 'question' text".into());
        }
        let mut choices: Vec<String> = Vec::new();
        for c in str_list(q, "choices")? {
            let c = plain(&c, CHOICE_CHARS, false);
            if !c.is_empty() && !choices.contains(&c) {
                choices.push(c);
            }
        }
        if !(MIN_CHOICES..=MAX_CHOICES).contains(&choices.len()) {
            return Err(format!(
                "'{text}': give {MIN_CHOICES} to {MAX_CHOICES} different answer choices"
            ));
        }
        questions.push(Question {
            text,
            choices,
            picked: None,
            typed: String::new(),
        });
    }
    Ok(Questionnaire {
        intro: plain(
            params.get("intro").and_then(Value::as_str).unwrap_or(""),
            MESSAGE_CHARS,
            false,
        ),
        questions,
    })
}

/// `ui.suggestReplies`'s params: `message` and 1 to [`MAX_CHIPS`] `chips`.
pub fn parse_suggestions(params: &Value) -> Result<Suggestions, String> {
    let message = plain(
        params.get("message").and_then(Value::as_str).unwrap_or(""),
        MESSAGE_CHARS,
        true,
    );
    if message.is_empty() {
        return Err("'message' must be the one suggested reply".into());
    }
    let mut chips: Vec<String> = Vec::new();
    for c in str_list(params, "chips")? {
        let c = plain(&c, CHIP_CHARS, false);
        if !c.is_empty() && !chips.contains(&c) && c != message {
            chips.push(c);
        }
    }
    if chips.is_empty() || chips.len() > MAX_CHIPS {
        return Err(format!("'chips' must hold 1 to {MAX_CHIPS} short replies"));
    }
    Ok(Suggestions { message, chips })
}

/// The questions as the chat keeps them: one Assistant row (a saved chat keeps it as text).
pub fn questions_text(q: &Questionnaire) -> String {
    let mut s = if q.intro.is_empty() {
        t("ui-assistant-questions-heading").to_string()
    } else {
        q.intro.clone()
    };
    for (i, question) in q.questions.iter().enumerate() {
        s.push_str(&format!(
            "\n{}. {} ({})",
            i + 1,
            question.text,
            question.choices.join(" / ")
        ));
    }
    s
}

/// The one user message the card sends: each question's answer, typed or picked, or "your
/// choice".
pub fn answers_message(q: &Questionnaire) -> String {
    let mut s = t("ui-answers-message-heading").to_string();
    for (i, question) in q.questions.iter().enumerate() {
        let typed = plain(&question.typed, MESSAGE_CHARS, false);
        let answer = if !typed.is_empty() {
            typed
        } else if let Some(c) = question.picked.and_then(|p| question.choices.get(p)) {
            c.clone()
        } else {
            t("ui-answer-your-choice").to_string()
        };
        s.push_str(&format!("\n{}. {}: {answer}", i + 1, question.text));
    }
    s
}

/// How a turn ended, which picks the fixed suggestions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// An error, a refusal or a cut-off answer.
    Failed,
    /// Check Design reports errors or warnings (the fix chips show too).
    Findings,
    /// The turn made or changed the design, and it checks clean.
    Changed,
    /// The turn only answered (no change).
    Answered,
}

impl Ending {
    pub fn of(end: &claude::StopKind, edits: bool, findings: bool) -> Ending {
        if matches!(
            end,
            claude::StopKind::Error(_)
                | claude::StopKind::MaxTokens
                | claude::StopKind::Refusal { .. }
        ) {
            Ending::Failed
        } else if findings {
            Ending::Findings
        } else if edits {
            Ending::Changed
        } else {
            Ending::Answered
        }
    }
}

/// The fixed suggestions when the model sent none (#1172: there are always chips), one set per
/// way a turn can end, all translated.
pub fn fallback(ending: Ending) -> Suggestions {
    let (message, chips) = match ending {
        Ending::Failed => (
            "ui-suggest-try-again-message",
            vec!["ui-suggest-try-again", "ui-suggest-what-went-wrong"],
        ),
        Ending::Findings => (
            "ui-suggest-fix-then-show-message",
            vec!["ui-suggest-why-do-they-matter", "ui-suggest-show-design"],
        ),
        Ending::Changed => (
            "ui-suggest-improve-message",
            vec![
                "ui-suggest-show-design",
                "ui-suggest-how-long",
                "ui-suggest-export-pes",
            ],
        ),
        Ending::Answered => (
            "ui-suggest-make-it-message",
            vec!["ui-suggest-tell-me-more", "ui-suggest-what-can-you-make"],
        ),
    };
    Suggestions {
        message: t(message).to_string(),
        chips: chips.into_iter().map(|c| t(c).to_string()).collect(),
    }
}

/// A finding as a fix message lists it: severity, rule, objects (as design.check names them),
/// its message, and whether it has an automatic fix.
fn finding_line(f: &Value) -> String {
    let ids: Vec<String> = f["ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .map(|id| format!("#{id}"))
        .collect();
    let mut line = format!(
        "- {} {}",
        f["severity"].as_str().unwrap_or("warning"),
        f["rule"].as_str().unwrap_or("?")
    );
    if !ids.is_empty() {
        line.push_str(&format!(" ({})", ids.join(" ")));
    }
    line.push_str(&format!(": {}", f["message"].as_str().unwrap_or("")));
    if f["fix"].is_null() {
        line.push_str(&format!(" [{}]", t("ui-finding-no-automatic-fix")));
    }
    line
}

fn listed(head: &str, findings: &[&Value]) -> String {
    let mut s = head.to_string();
    for f in findings.iter().take(MAX_FINDINGS) {
        s.push('\n');
        s.push_str(&finding_line(f));
    }
    if findings.len() > MAX_FINDINGS {
        s.push('\n');
        s.push_str(&tf!(
            "ui-findings-and-more",
            count = findings.len() - MAX_FINDINGS
        ));
    }
    s
}

/// The fix chips for a `design.check` result: "Fix the N errors", "Fix the N warnings" and
/// "Explain them" (notes do not count, as in the status bar's readiness line).
pub fn fix_chips(check: &Value) -> Vec<FixChip> {
    let all: Vec<&Value> = check["findings"].as_array().into_iter().flatten().collect();
    let of = |s: &str| -> Vec<&Value> {
        all.iter()
            .copied()
            .filter(|f| f["severity"] == s)
            .collect()
    };
    let (errors, warnings) = (of("error"), of("warning"));
    let mut chips = Vec::new();
    if !errors.is_empty() {
        chips.push(FixChip {
            label: tf!("ui-fix-the-errors", count = errors.len()),
            message: listed(t("ui-fix-the-errors-message"), &errors),
        });
    }
    if !warnings.is_empty() {
        chips.push(FixChip {
            label: tf!("ui-fix-the-warnings", count = warnings.len()),
            message: listed(t("ui-fix-the-warnings-message"), &warnings),
        });
    }
    if !chips.is_empty() {
        let both: Vec<&Value> = errors.iter().chain(&warnings).copied().collect();
        chips.push(FixChip {
            label: t("ui-explain-the-findings").to_string(),
            message: listed(t("ui-explain-the-findings-message"), &both),
        });
    }
    chips
}

/// `ui.askUser` and `ui.suggestReplies`: only during the Assistant's own turn.
pub fn command(app: &mut EditorApp, command: &str, params: Option<&Value>) -> Result<Value, String> {
    let panel = &mut app.assistant.panel;
    if !panel.busy() || !panel.offers.from_assistant {
        return Err("only the Assistant can ask the user, during its own turn".into());
    }
    let empty = json!({});
    let params = params.unwrap_or(&empty);
    match command {
        "ui.askUser" => {
            let q = parse_questions(params)?;
            let n = q.questions.len();
            panel.offers.asked = Some(q);
            Ok(json!({
                "shown": n,
                "next": "End your turn now without doing the work; the user's answers come as their next message.",
            }))
        }
        _ => {
            let s = parse_suggestions(params)?;
            let n = s.chips.len();
            panel.offers.suggested = Some(s);
            Ok(json!({
                "shown": n,
                "next": "Shown under your reply; nothing is sent until the user clicks.",
            }))
        }
    }
}

/// At the end of a turn: the questions it asked (added to the chat as one row), else the
/// suggestions (the model's, or the fixed set for how the turn ended), and the fix chips from
/// Check Design.
pub fn end_turn(app: &mut EditorApp, edits: bool, end: &claude::StopKind) {
    crate::quality_ui::update(app);
    let fixes = fix_chips(&app.quality.check);
    let ending = Ending::of(end, edits, !fixes.is_empty());
    let panel = &mut app.assistant.panel;
    let asked = panel.offers.asked.take();
    let suggested = panel.offers.suggested.take();
    if let Some(q) = &asked {
        panel.items.push(Item::Assistant(questions_text(q)));
    }
    let (chat, after) = (panel.chat_id(), panel.items.len());
    let o = &mut panel.offers;
    o.chat = chat;
    o.after = after;
    o.suggestions = if asked.is_some() {
        None
    } else {
        Some(suggested.unwrap_or_else(|| fallback(ending)))
    };
    o.fixes = if asked.is_some() { Vec::new() } else { fixes };
    o.questions = asked;
}

/// Whether the offers belong to what the chat shows now.
fn current(app: &EditorApp) -> bool {
    let panel = &app.assistant.panel;
    let o = &panel.offers;
    !panel.busy()
        && o.chat == panel.chat_id()
        && o.after <= panel.items.len()
        && !panel.items[o.after..]
            .iter()
            .any(|i| matches!(i, Item::User(_)))
}

/// The card or the chips at the top of the composer, and what a click on them does: send a
/// message as the user, or put the suggested reply into the composer.
pub fn row(app: &mut EditorApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    match self::ui(app, ui) {
        Some(Click::Send(text)) => {
            crate::assistant_panel::send(app, text, crate::assistant_panel::waker(ctx));
        }
        Some(Click::Edit(text)) => app.assistant.panel.input = text,
        None => {}
    }
}

/// What a click asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Click {
    /// Send this as the user's message.
    Send(String),
    /// Put this into the composer.
    Edit(String),
}

/// The card or the chips, at the top of the composer. Every control is a button, so Tab
/// reaches it and Enter or Space presses it; the model's text is shown as plain text only.
pub fn ui(app: &mut EditorApp, ui: &mut egui::Ui) -> Option<Click> {
    if !current(app) {
        return None;
    }
    let chat = app.assistant.panel.chat_id();
    let o = &mut app.assistant.panel.offers;
    let mut click = None;
    if let Some(q) = &mut o.questions {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new(t("ui-assistant-questions-heading")).strong());
            egui::ScrollArea::vertical()
                .id_salt(("assistant-questions", chat))
                .max_height(320.0)
                .show(ui, |ui| {
                    for (i, question) in q.questions.iter_mut().enumerate() {
                        ui.label(format!("{}. {}", i + 1, question.text));
                        ui.horizontal_wrapped(|ui| {
                            for (k, c) in question.choices.iter().enumerate() {
                                let on = question.picked == Some(k) && question.typed.is_empty();
                                if ui.add(egui::Button::new(c.as_str()).selected(on)).clicked() {
                                    question.picked = Some(k);
                                    question.typed.clear();
                                }
                            }
                        });
                        ui.add(
                            egui::TextEdit::singleline(&mut question.typed)
                                .id_salt(("assistant-answer", chat, i))
                                .hint_text(t("ui-or-type-your-own-answer"))
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(4.0);
                    }
                });
            ui.horizontal(|ui| {
                if ui.button(t("ui-send-answers")).clicked() {
                    click = Some(Click::Send(answers_message(q)));
                }
                if ui
                    .button(t("ui-use-your-judgment"))
                    .on_hover_text(t("ui-use-your-judgment-tip"))
                    .clicked()
                {
                    click = Some(Click::Send(t("ui-use-your-judgment-message").to_string()));
                }
            });
        });
        return click;
    }
    if let Some(s) = &o.suggestions {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(s.message.as_str()).italics());
            if ui
                .button(t("ui-send-suggested-reply"))
                .on_hover_text(t("ui-send-suggested-reply-tip"))
                .clicked()
            {
                click = Some(Click::Send(s.message.clone()));
            }
            if ui
                .button(t("ui-edit-suggested-reply"))
                .on_hover_text(t("ui-edit-suggested-reply-tip"))
                .clicked()
            {
                click = Some(Click::Edit(s.message.clone()));
            }
        });
    }
    if !o.fixes.is_empty() || o.suggestions.is_some() {
        ui.horizontal_wrapped(|ui| {
            for f in &o.fixes {
                if ui.button(f.label.as_str()).clicked() {
                    click = Some(Click::Send(f.message.clone()));
                }
            }
            for c in o.suggestions.iter().flat_map(|s| &s.chips) {
                if ui.button(c.as_str()).clicked() {
                    click = Some(Click::Send(c.clone()));
                }
            }
        });
    }
    click
}

/// `--show assistant-questions` and `--show assistant-suggestions` (#1172): the panel with a
/// question card, or with a reply's suggested message and chips and the fix chips for a real
/// Check Design warning (a 2 × 1 mm fill), for the docs screenshots. Changes nothing on disk.
pub fn show_sample(app: &mut EditorApp, questions: bool) {
    app.ui.prefs.ai.enabled = true;
    app.assistant.panel.open = true;
    if !questions {
        app.execute(&openstitch_app::editor::CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 40, "y": 20, "width": 2, "height": 1, "fill": "Red" }),
        ));
    }
    crate::quality_ui::update(app);
    let fixes = fix_chips(&app.quality.check);
    let panel = &mut app.assistant.panel;
    panel
        .items
        .push(Item::User("make me a badge for my cycling club".into()));
    if questions {
        let mut q = parse_questions(&json!({
            "intro": "A few quick choices so the badge fits your club and your machine.",
            "questions": [
                { "question": "What shape should the badge be?", "choices": ["Round", "Shield", "Oval"] },
                { "question": "How big should it be?", "choices": ["60 mm", "80 mm", "100 mm"] },
                { "question": "What should the centre picture be?", "choices": ["Road bike", "Wheel", "Mountains with a road"] },
                { "question": "Which colours?", "choices": ["Navy and white", "Black and yellow", "Green and gold"] },
                { "question": "What will it be stitched on?", "choices": ["Twill patch", "Jersey knit", "Cap"] },
            ],
        }))
        .expect("sample questions");
        q.questions[0].picked = Some(0);
        q.questions[1].picked = Some(1);
        q.questions[3].typed = "Blue and yellow, our club colours".into();
        panel.items.push(Item::Assistant(
            "I've put a few questions on screen. Answer what you can, or tell me to use my judgment."
                .into(),
        ));
        panel.items.push(Item::Assistant(questions_text(&q)));
        panel.offers.questions = Some(q);
    } else {
        panel.items.push(Item::Assistant(
            "Here is the badge: a round 80 mm crest with a road bike in the centre and \"Hilltop Wheelers\" arced along the top, in blue and yellow with a thick satin border."
                .into(),
        ));
        panel.offers.suggestions = Some(Suggestions {
            message: "Add \"Est. 1987\" under the bike".into(),
            chips: vec![
                "Make it 100 mm".into(),
                "Use a shield shape".into(),
                "Export it as PES".into(),
            ],
        });
        panel.offers.fixes = fixes;
    }
    let (chat, after) = (panel.chat_id(), panel.items.len());
    panel.offers.chat = chat;
    panel.offers.after = after;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_text_is_made_plain_and_cut() {
        assert_eq!(plain("  a\tb\u{7}\n c  ", 80, false), "a b c");
        assert_eq!(plain("line one\nline two", 80, true), "line one\nline two");
        let long = "x".repeat(100);
        let cut = plain(&long, 60, false);
        assert_eq!(cut.chars().count(), 60);
        assert!(cut.ends_with('…'));
        assert_eq!(plain("[link](https://x)", 80, false), "[link](https://x)");
    }

    #[test]
    fn questions_need_two_to_four_choices_and_at_most_ten() {
        let one = |choices: Value| json!({ "questions": [{ "question": "Size?", "choices": choices }] });
        assert!(parse_questions(&one(json!(["A"]))).is_err());
        assert!(parse_questions(&one(json!(["A", "A"]))).is_err(), "duplicates");
        assert!(parse_questions(&one(json!(["A", "B", "C", "D", "E"]))).is_err());
        let q = parse_questions(&one(json!(["100 mm", "130 mm"]))).expect("ok");
        assert_eq!(q.questions[0].choices, ["100 mm", "130 mm"]);
        let many: Vec<Value> = (0..11)
            .map(|i| json!({ "question": format!("Q{i}?"), "choices": ["a", "b"] }))
            .collect();
        assert!(parse_questions(&json!({ "questions": many })).is_err());
        assert!(parse_questions(&json!({ "questions": [] })).is_err());
    }

    #[test]
    fn answers_are_typed_picked_or_left_to_the_assistant() {
        let mut q = parse_questions(&json!({ "questions": [
            { "question": "Size?", "choices": ["100 mm", "130 mm"] },
            { "question": "Fabric?", "choices": ["Cotton", "Towel"] },
            { "question": "Colours?", "choices": ["Club colours", "Two"] },
        ]}))
        .expect("ok");
        q.questions[0].picked = Some(1);
        q.questions[1].picked = Some(0);
        q.questions[1].typed = "polo shirt piqué".into();
        let m = answers_message(&q);
        assert!(m.starts_with("Answers:"), "{m}");
        assert!(m.contains("1. Size?: 130 mm"), "{m}");
        assert!(m.contains("2. Fabric?: polo shirt piqué"), "typed wins: {m}");
        assert!(m.contains("3. Colours?: your choice"), "{m}");
        let row = questions_text(&q);
        assert!(row.contains("1. Size? (100 mm / 130 mm)"), "{row}");
    }

    #[test]
    fn suggestions_need_a_message_and_one_to_four_chips() {
        let s = parse_suggestions(&json!({ "message": "Make it 80 mm", "chips": ["Bigger", "Bigger", "Make it 80 mm", "Red"] }))
            .expect("ok");
        assert_eq!(s.chips, ["Bigger", "Red"], "duplicates and the message itself go");
        assert!(parse_suggestions(&json!({ "message": "", "chips": ["a"] })).is_err());
        assert!(parse_suggestions(&json!({ "message": "m", "chips": [] })).is_err());
        assert!(parse_suggestions(&json!({ "message": "m", "chips": ["a", "b", "c", "d", "e"] })).is_err());
        // One fixed set per way a turn ends, each different.
        let sets: Vec<Suggestions> = [
            Ending::Failed,
            Ending::Findings,
            Ending::Changed,
            Ending::Answered,
        ]
        .into_iter()
        .map(fallback)
        .collect();
        for f in &sets {
            assert!(!f.message.is_empty() && !f.chips.is_empty() && f.chips.len() <= MAX_CHIPS);
        }
        for (i, a) in sets.iter().enumerate() {
            assert!(sets[i + 1..].iter().all(|b| b != a));
        }
        let error = claude::StopKind::Error("x".into());
        assert_eq!(Ending::of(&error, true, true), Ending::Failed);
        let done = claude::StopKind::EndTurn;
        assert_eq!(Ending::of(&done, true, true), Ending::Findings);
        assert_eq!(Ending::of(&done, true, false), Ending::Changed);
        assert_eq!(Ending::of(&done, false, false), Ending::Answered);
    }

    #[test]
    fn fix_chips_count_errors_and_warnings_and_list_each_finding() {
        let check = json!({ "errors": 1, "warnings": 2, "notes": 1, "findings": [
            { "severity": "error", "rule": "satinTooWide", "ids": [3], "message": "Satin 12.0 mm wide", "fix": { "command": "object.setRegion" } },
            { "severity": "warning", "rule": "tinyObject", "ids": [5], "message": "0.4 mm object", "fix": null },
            { "severity": "warning", "rule": "longStitch", "ids": [6, 7], "message": "15.2 mm stitch", "fix": null },
            { "severity": "info", "rule": "moreColoursThanNeedles", "ids": [], "message": "7 colours", "fix": null },
        ]});
        let chips = fix_chips(&check);
        let labels: Vec<&str> = chips.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Fix the error", "Fix the 2 warnings", "Explain them"]);
        let errors = &chips[0].message;
        assert!(errors.contains("Fix only what has a fix"), "{errors}");
        assert!(errors.contains("- error satinTooWide (#3): Satin 12.0 mm wide"), "{errors}");
        assert!(!errors.contains("tinyObject"));
        let warnings = &chips[1].message;
        assert!(warnings.contains("- warning longStitch (#6 #7): 15.2 mm stitch [no automatic fix]"), "{warnings}");
        assert!(!chips[2].message.contains("moreColoursThanNeedles"), "notes do not count");
        assert!(fix_chips(&json!({ "findings": [] })).is_empty());
        let many: Vec<Value> = (0..35)
            .map(|i| json!({ "severity": "warning", "rule": "tinyObject", "ids": [i], "message": "m", "fix": null }))
            .collect();
        let m = &fix_chips(&json!({ "findings": many }))[0].message;
        assert_eq!(m.lines().filter(|l| l.starts_with("- ")).count(), MAX_FINDINGS);
        assert!(m.contains("and 5 more"), "{m}");
    }
}
