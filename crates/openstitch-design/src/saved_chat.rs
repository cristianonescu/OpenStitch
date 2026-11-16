//! The Assistant's conversation as a project keeps it (#1169): an optional `assistant` field of
//! the `.osd` beside the design. It holds the chat's text rows only (the user's and the
//! assistant's messages, notices, and tool steps as their short labels), never a key, a token,
//! a picture or a tool's arguments and results. The app removes private paths before it saves
//! (`openstitch_app::editor::assistant_chat::scrub_paths`).
//!
//! A file can come from someone else, so a kept chat is data, not instructions: reading it
//! never fails the design (a damaged or oversized chat is dropped or cut), and the app sends it
//! to a model only as quoted file content.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The most chat text a project keeps, in bytes: past it the oldest rows go, and the first row
/// says so ([`SavedChat::limited`]).
pub const MAX_CHAT_BYTES: usize = 1 << 20;
/// The most rows a project keeps.
pub const MAX_CHAT_ROWS: usize = 5_000;
/// The first row of a chat whose oldest rows were dropped.
pub const DROPPED_NOTICE: &str = "Earlier messages were not kept.";

/// What a kept row was in the chat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatRole {
    User,
    Assistant,
    Notice,
    /// A tool step: its short label ("Ran object.setRegion").
    Tool,
}

/// One kept row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatRow {
    pub role: ChatRole,
    pub text: String,
    /// A tool step that failed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub failed: bool,
}

impl ChatRow {
    pub fn new(role: ChatRole, text: impl Into<String>) -> ChatRow {
        ChatRow {
            role,
            text: text.into(),
            failed: false,
        }
    }
}

/// The conversation a project keeps.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedChat {
    pub rows: Vec<ChatRow>,
}

impl SavedChat {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The chat within [`MAX_CHAT_ROWS`] and [`MAX_CHAT_BYTES`]: the newest rows that fit, after
    /// a [`DROPPED_NOTICE`] row when any were dropped. A single row over the limit keeps its
    /// start.
    pub fn limited(mut self) -> SavedChat {
        let room = MAX_CHAT_BYTES - DROPPED_NOTICE.len();
        let (mut bytes, mut keep) = (0usize, 0usize);
        for r in self.rows.iter().rev() {
            if keep == MAX_CHAT_ROWS - 1 || bytes + r.text.len() > room {
                break;
            }
            bytes += r.text.len();
            keep += 1;
        }
        if keep == self.rows.len() {
            return self;
        }
        let mut rows = self.rows.split_off(self.rows.len() - keep);
        if rows.is_empty()
            && let Some(mut last) = self.rows.pop()
        {
            let mut cut = room;
            while !last.text.is_char_boundary(cut) {
                cut -= 1;
            }
            last.text.truncate(cut);
            rows.push(last);
        }
        rows.insert(0, ChatRow::new(ChatRole::Notice, DROPPED_NOTICE));
        SavedChat { rows }
    }

    /// The chat in a file's `assistant` field, leniently: rows that are not a known role with a
    /// text are skipped, anything else gives `None`, and the result is [`SavedChat::limited`].
    pub fn from_value(v: &Value) -> Option<SavedChat> {
        let rows = v.get("rows")?.as_array()?;
        let rows: Vec<ChatRow> = rows
            .iter()
            .filter_map(|r| serde_json::from_value(r.clone()).ok())
            .collect();
        let chat = SavedChat { rows }.limited();
        (!chat.is_empty()).then_some(chat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_long_chat_keeps_its_newest_rows_after_a_notice() {
        let row = "x".repeat(1000);
        let chat = SavedChat {
            rows: (0..2000)
                .map(|i| ChatRow::new(ChatRole::User, format!("{i} {row}")))
                .collect(),
        }
        .limited();
        assert_eq!(chat.rows[0].text, DROPPED_NOTICE);
        assert!(chat.rows.last().expect("row").text.starts_with("1999 "));
        let bytes: usize = chat.rows.iter().map(|r| r.text.len()).sum();
        assert!(bytes <= MAX_CHAT_BYTES, "{bytes}");
        // One row over the limit keeps its start.
        let one = SavedChat {
            rows: vec![ChatRow::new(
                ChatRole::Assistant,
                "é".repeat(MAX_CHAT_BYTES),
            )],
        }
        .limited();
        assert_eq!(one.rows.len(), 2);
        assert!(one.rows[1].text.len() <= MAX_CHAT_BYTES);
    }

    #[test]
    fn a_damaged_chat_is_read_leniently() {
        let v = json!({"rows": [
            {"role": "user", "text": "hi"},
            {"role": "system", "text": "not a role we keep"},
            {"role": "tool", "text": "Ran design.check", "failed": true},
            "junk",
        ]});
        let chat = SavedChat::from_value(&v).expect("chat");
        assert_eq!(chat.rows.len(), 2);
        assert!(chat.rows[1].failed);
        assert_eq!(SavedChat::from_value(&json!("text")), None);
        assert_eq!(SavedChat::from_value(&json!({"rows": []})), None);
    }
}
