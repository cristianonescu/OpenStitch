//! The words printed on the sheets, by message id (#555): English here, and the window's
//! language when the editor has set one ([`set_texts`]). Values are filled into `{ $name }`
//! placeholders by [`fill`]. Per thread, as the editor prints on its own.

use std::cell::RefCell;
use std::collections::HashMap;

/// Message id and English text; `{ $name }` marks a value.
pub const TEXTS: &[(&str, &str)] = &[
    ("print-customer", "Customer"),
    ("print-order", "Order"),
    ("print-due", "Due"),
    ("print-fabric", "Fabric"),
    ("print-stabiliser", "Stabiliser"),
    ("print-field", "{ $label }: { $value }"),
    (
        "print-size",
        "Size { $width } x { $height } mm ({ $width-in } x { $height-in } in)",
    ),
    ("print-shown-actual-size", "Shown at actual size"),
    ("print-shown-at", "Shown at { $percent } % of actual size"),
    ("print-thread-colours", "Thread colours"),
    ("print-approved-by", "Approved by"),
    ("print-date", "Date"),
    ("print-signature", "Signature"),
    ("print-hoop-size", "Hoop { $hoop }   { $size }"),
    (
        "print-counts",
        "Stitches { $stitches }   Colour changes { $colours }   Stops { $stops }   Trims { $trims }",
    ),
    (
        "print-time-thread",
        "Sewing time about { $minutes } min   Thread about { $metres } m",
    ),
    ("print-notes", "Notes: { $notes }"),
    ("print-step", "Step"),
    ("print-thread", "Thread"),
    ("print-brand", "Brand"),
    ("print-code", "Code"),
    ("print-stitches", "Stitches"),
    ("print-stops", "Stops"),
    ("print-custom-hoop", "custom"),
    (
        "print-design-page",
        "Design Page { $width } x { $height } mm   Hoop { $hoop }",
    ),
    (
        "print-design-size",
        "Design size { $width } x { $height } mm   Stitches { $stitches }   Colour changes { $colours }",
    ),
    (
        "print-thread-time",
        "Thread about { $metres } m, sewing time about { $minutes } min",
    ),
    (
        "print-sheet-of",
        "Sheet { $sheet } of { $sheets } (row { $row }, column { $column })",
    ),
    (
        "print-actual-size",
        "Actual size (100 %): print without page scaling",
    ),
    ("print-reduced", "Reduced to { $percent } %"),
    ("print-colour-order", "Colour order"),
    ("print-section", "Section"),
    ("print-hoop-position", "Hoop position"),
    ("print-jumbo-frame", "Jumbo frame"),
    ("print-hooping", "Hooping"),
    ("print-frame-turned", " (frame turned 180°)"),
];

thread_local! {
    static CURRENT: RefCell<HashMap<&'static str, String>> = RefCell::new(HashMap::new());
}

/// Sets the texts this thread prints: `text(id, english)` gives each message's pattern in the
/// language (placeholders kept). [`english`] goes back to English.
pub fn set_texts(text: impl Fn(&'static str, &'static str) -> String) {
    CURRENT.with(|c| {
        let mut c = c.borrow_mut();
        c.clear();
        for (id, e) in TEXTS {
            let t = text(id, e);
            if t != *e {
                c.insert(id, t);
            }
        }
    });
}

/// Prints English again.
pub fn english() {
    CURRENT.with(|c| c.borrow_mut().clear());
}

/// Message `id`'s text in this thread's language (English when none is set).
pub fn text(id: &str) -> String {
    CURRENT
        .with(|c| c.borrow().get(id).cloned())
        .or_else(|| {
            TEXTS
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, e)| e.to_string())
        })
        .unwrap_or_else(|| id.to_string())
}

/// Message `id` with its `{ $name }` placeholders filled in from `values`.
pub fn fill(id: &str, values: &[(&str, String)]) -> String {
    let mut out = text(id);
    for (name, value) in values {
        out = out.replace(&format!("{{ ${name} }}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_fill_in_and_follow_the_language_set() {
        assert_eq!(text("print-customer"), "Customer");
        assert_eq!(
            fill("print-shown-at", &[("percent", "50".into())]),
            "Shown at 50 % of actual size"
        );
        set_texts(|_, e| format!("[{e}]"));
        assert_eq!(text("print-customer"), "[Customer]");
        assert_eq!(
            fill("print-shown-at", &[("percent", "50".into())]),
            "[Shown at 50 % of actual size]"
        );
        english();
        assert_eq!(text("print-customer"), "Customer");
        let mut ids: Vec<_> = TEXTS.iter().map(|t| t.0).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), TEXTS.len(), "unique ids");
    }
}
