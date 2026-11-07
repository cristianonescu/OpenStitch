//! Accessibility helpers (#499). egui names most controls from their text; a combo box, slider
//! or text field whose label is drawn beside it (in a grid) gets no name, so a screen reader
//! announces only "combo box". [`name`] gives it the label the user sees.
//! `a11y_tests.rs` audits the whole window and each dialog for unnamed controls.

use eframe::egui;

/// Names `response`'s control for screen readers (AccessKit), without changing what it shows.
pub fn name(response: &egui::Response, label: &str) {
    let label = label.trim();
    if label.is_empty() {
        return;
    }
    response
        .ctx
        .accesskit_node_builder(response.id, |node| node.set_label(label.to_string()));
}

/// [`name`] for a slider's response and for the number field drawn inside it (egui's slider
/// links the field to its text by `labelled_by`, which leaves the field without a name of its own).
pub fn name_slider(response: &egui::Response, label: &str) {
    name(response, label);
    let inner: Vec<egui::Id> = response.ctx.viewport(|v| {
        v.this_pass
            .widgets
            .get_layer(response.layer_id)
            .filter(|w| w.id != response.id && response.rect.contains_rect(w.rect))
            .map(|w| w.id)
            .collect()
    });
    for id in inner {
        response
            .ctx
            .accesskit_node_builder(id, |node| node.set_label(label.trim().to_string()));
    }
}

/// [`name`] for a combo box's or popup button's response, passed through.
pub fn named(response: egui::Response, label: &str) -> egui::Response {
    name(&response, label);
    response
}
