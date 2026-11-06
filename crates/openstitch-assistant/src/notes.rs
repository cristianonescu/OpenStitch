//! The Assistant's memory between conversations (#491): a short Markdown notes file the user can
//! read, edit and clear, kept beside `ui.json`.
//!
//! The notes are plain Markdown with `## Section` headings ([`SECTIONS`]). The model reads them
//! in its system prompt at the start of each conversation ([`Notes::prompt_block`]) and changes
//! them with `notes_write` ([`Notes::write`]), which the editor puts behind the permission gate.
//! The whole file is capped at [`MAX_NOTES_BYTES`]: a write that would pass the cap is refused
//! with a message asking the model to summarise instead.
//!
//! Nothing here sends the notes anywhere; the editor puts them only into requests to the
//! provider the user chose.

use std::path::Path;

/// The sections `notes_write` takes, in the order a new file lists them.
pub const SECTIONS: [&str; 5] = ["Machine", "Threads", "Fabric", "Preferences", "Lessons"];

/// The largest notes file, in bytes.
pub const MAX_NOTES_BYTES: usize = 8 * 1024;

/// The longest text one `notes_write` takes, in characters.
pub const MAX_WRITE_CHARS: usize = 1000;

/// The file name in the config folder (beside `ui.json`).
pub const FILE_NAME: &str = "assistant-notes.md";

/// How `notes_write` changes a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteMode {
    /// Add the text as one more `- ` line at the end of the section.
    Append,
    /// The text becomes the whole section; empty text removes the section.
    Replace,
}

impl WriteMode {
    pub fn from_key(key: &str) -> Option<WriteMode> {
        match key {
            "append" => Some(WriteMode::Append),
            "replace" => Some(WriteMode::Replace),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            WriteMode::Append => "append",
            WriteMode::Replace => "replace",
        }
    }
}

/// The notes: Markdown text, as the user and the model left it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notes {
    text: String,
}

/// The canonical section name for `name` (case-insensitive), or the error the model is told.
pub fn section_name(name: &str) -> Result<&'static str, String> {
    let name = name.trim().trim_start_matches('#').trim();
    SECTIONS
        .iter()
        .find(|s| s.eq_ignore_ascii_case(name))
        .copied()
        .ok_or_else(|| {
            format!(
                "unknown section '{name}'; the sections are {}",
                SECTIONS.join(", ")
            )
        })
}

/// Why `text` must not be remembered: it looks like a password, key or token. The notes hold
/// embroidery facts, never secrets (`skills/openstitch-embroidery/SKILL.md`).
pub fn secret_reason(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    const WORDS: [&str; 7] = [
        "password",
        "passwd",
        "api key",
        "api_key",
        "apikey",
        "secret key",
        "private key",
    ];
    if WORDS.iter().any(|w| lower.contains(w)) {
        return Some("it mentions a password or key");
    }
    let token_like = text
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_')))
        .any(|w| {
            let starts = ["sk-", "ghp_", "gho_", "github_pat_", "xox", "AKIA", "AIza"]
                .iter()
                .any(|p| w.starts_with(p) && w.len() >= 16);
            let long = w.len() >= 32
                && w.chars().any(|c| c.is_ascii_digit())
                && w.chars().any(|c| c.is_ascii_alphabetic());
            starts || long
        });
    token_like.then_some("it holds something that looks like a key or token")
}

/// One `## ` section: its heading line's name and its body lines.
struct Section {
    name: String,
    body: Vec<String>,
}

impl Notes {
    /// Notes from text the user typed or a file held.
    pub fn from_text(text: &str) -> Notes {
        Notes {
            text: normalise(text),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.text.len()
    }

    /// Splits into the text before the first heading and the sections.
    fn split(&self) -> (Vec<String>, Vec<Section>) {
        let mut preamble = Vec::new();
        let mut sections: Vec<Section> = Vec::new();
        for line in self.text.lines() {
            if let Some(name) = line.strip_prefix("## ") {
                sections.push(Section {
                    name: name.trim().to_string(),
                    body: Vec::new(),
                });
            } else if let Some(s) = sections.last_mut() {
                s.body.push(line.to_string());
            } else {
                preamble.push(line.to_string());
            }
        }
        (preamble, sections)
    }

    fn join(preamble: &[String], sections: &[Section]) -> String {
        let mut out = String::new();
        let pre = preamble.join("\n");
        if !pre.trim().is_empty() {
            out.push_str(pre.trim_end());
            out.push_str("\n\n");
        }
        for s in sections {
            out.push_str("## ");
            out.push_str(&s.name);
            out.push('\n');
            let body = s.body.join("\n");
            let body = body.trim_matches('\n').trim_end();
            if !body.is_empty() {
                out.push_str(body);
                out.push('\n');
            }
            out.push('\n');
        }
        normalise(&out)
    }

    /// The body of one section, if it is there.
    pub fn section(&self, name: &str) -> Option<String> {
        let name = section_name(name).ok()?;
        let (_, sections) = self.split();
        sections
            .into_iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
            .map(|s| s.body.join("\n").trim().to_string())
    }

    /// What `notes_write` does: the notes after appending to or replacing `section`, or why not.
    /// The notes are unchanged on an error.
    pub fn write(&mut self, section: &str, text: &str, mode: WriteMode) -> Result<(), String> {
        let name = section_name(section)?;
        let text = text.trim();
        if text.chars().count() > MAX_WRITE_CHARS {
            return Err(format!(
                "the text is longer than {MAX_WRITE_CHARS} characters; keep each note short"
            ));
        }
        if mode == WriteMode::Append && text.is_empty() {
            return Err("append needs some text".into());
        }
        if let Some(why) = secret_reason(text) {
            return Err(format!(
                "not remembered: {why}. Never store passwords, keys or tokens"
            ));
        }
        let (preamble, mut sections) = self.split();
        let at = sections
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name));
        match mode {
            WriteMode::Append => {
                let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
                let line = if line.starts_with("- ") {
                    line
                } else {
                    format!("- {line}")
                };
                match at {
                    Some(i) => {
                        let body = &mut sections[i].body;
                        while body.last().is_some_and(|l| l.trim().is_empty()) {
                            body.pop();
                        }
                        body.push(line);
                    }
                    None => sections.push(Section {
                        name: name.to_string(),
                        body: vec![line],
                    }),
                }
            }
            WriteMode::Replace => {
                let body: Vec<String> = text.lines().map(str::to_string).collect();
                match (at, text.is_empty()) {
                    (Some(i), true) => {
                        sections.remove(i);
                    }
                    (Some(i), false) => sections[i].body = body,
                    (None, true) => {}
                    (None, false) => sections.push(Section {
                        name: name.to_string(),
                        body,
                    }),
                }
            }
        }
        let next = Notes::join(&preamble, &sections);
        if next.len() > MAX_NOTES_BYTES {
            return Err(format!(
                "the notes would be {} bytes, over the limit of {MAX_NOTES_BYTES}; nothing was \
                 written. Summarise first: notes_write with mode replace and a shorter version of \
                 a section (Lessons first), then try again",
                next.len()
            ));
        }
        self.text = next;
        Ok(())
    }

    /// Text the user typed in the panel, checked against the cap.
    pub fn set_text(&mut self, text: &str) -> Result<(), String> {
        let next = normalise(text);
        if next.len() > MAX_NOTES_BYTES {
            return Err(format!(
                "{} bytes is over the limit of {MAX_NOTES_BYTES}; shorten the notes",
                next.len()
            ));
        }
        self.text = next;
        Ok(())
    }

    /// What goes after the instructions in the system prompt: None when there is nothing to
    /// remember.
    pub fn prompt_block(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        Some(format!(
            "\n\n# What you remember about this user\nNotes kept between conversations (notes_read \
             shows them; notes_write changes them, with the user's permission). Use them without \
             asking again; when the user says one is wrong, correct it.\n\n<notes>\n{}\n</notes>",
            self.text.trim_end()
        ))
    }

    /// Reads the notes file; a missing file is empty notes. Only the first [`MAX_NOTES_BYTES`]
    /// × 4 bytes are read, so a damaged file cannot fill the prompt.
    pub fn load(path: &Path) -> std::io::Result<Notes> {
        use std::io::Read;
        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Notes::default()),
            Err(e) => return Err(e),
        };
        let mut bytes = Vec::new();
        file.take((MAX_NOTES_BYTES * 4) as u64)
            .read_to_end(&mut bytes)?;
        let text = String::from_utf8_lossy(&bytes);
        let mut text = text.into_owned();
        if text.len() > MAX_NOTES_BYTES {
            let mut cut = MAX_NOTES_BYTES;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
        }
        Ok(Notes::from_text(&text))
    }

    /// Writes the notes file (through a temporary file, so a crash leaves the old one); empty
    /// notes remove it.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if self.is_empty() {
            return match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("md.tmp");
        std::fs::write(&tmp, self.text.as_bytes())?;
        std::fs::rename(&tmp, path)
    }
}

/// `\n` line ends, no trailing blank lines, one final newline (none when empty).
fn normalise(text: &str) -> String {
    let t = text.replace("\r\n", "\n");
    let t = t.trim_end();
    if t.trim().is_empty() {
        String::new()
    } else {
        format!("{t}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_makes_sections_in_the_order_written_and_adds_lines() {
        let mut n = Notes::default();
        n.write("threads", "Madeira Polyneon 40", WriteMode::Append)
            .expect("w");
        n.write(
            "Machine",
            "Brother NV880E, 130 x 180 mm hoop",
            WriteMode::Append,
        )
        .expect("w");
        n.write(
            "Threads",
            "- owns red 1747 and black 1800",
            WriteMode::Append,
        )
        .expect("w");
        assert_eq!(
            n.text(),
            "## Threads\n- Madeira Polyneon 40\n- owns red 1747 and black 1800\n\n\
             ## Machine\n- Brother NV880E, 130 x 180 mm hoop\n"
        );
        assert_eq!(
            n.section("threads").as_deref(),
            Some("- Madeira Polyneon 40\n- owns red 1747 and black 1800")
        );
    }

    #[test]
    fn replace_rewrites_or_removes_a_section_and_keeps_the_rest() {
        let mut n = Notes::from_text(
            "Intro the user typed.\n\n## Lessons\n- a\n- b\n\n## Fabric\n- knits\n",
        );
        n.write("Lessons", "- summary of a and b", WriteMode::Replace)
            .expect("w");
        assert_eq!(
            n.text(),
            "Intro the user typed.\n\n## Lessons\n- summary of a and b\n\n## Fabric\n- knits\n"
        );
        n.write("lessons", "", WriteMode::Replace).expect("w");
        assert_eq!(n.text(), "Intro the user typed.\n\n## Fabric\n- knits\n");
    }

    #[test]
    fn unknown_sections_empty_appends_and_secrets_are_refused_and_change_nothing() {
        let mut n = Notes::from_text("## Fabric\n- cotton\n");
        let before = n.clone();
        assert!(
            n.write("Diary", "x", WriteMode::Append)
                .expect_err("unknown section")
                .contains("Machine")
        );
        assert!(n.write("Fabric", "  ", WriteMode::Append).is_err());
        assert!(
            n.write("Preferences", "my password is hunter2", WriteMode::Append)
                .is_err()
        );
        assert!(
            n.write(
                "Preferences",
                "key sk-test-abcdefghijklmnop",
                WriteMode::Append
            )
            .is_err()
        );
        assert!(
            n.write(
                "Preferences",
                "token 3f9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e",
                WriteMode::Append
            )
            .is_err()
        );
        assert_eq!(n, before);
        // Thread numbers and sizes are not secrets.
        n.write("Threads", "Brother 900 white, 130x180", WriteMode::Append)
            .expect("w");
    }

    #[test]
    fn over_the_cap_is_refused_with_a_request_to_summarise() {
        let mut n = Notes::default();
        let line = "x".repeat(900);
        let mut refused = None;
        for _ in 0..20 {
            if let Err(e) = n.write("Lessons", &format!("{line} y1"), WriteMode::Append) {
                refused = Some(e);
                break;
            }
        }
        let e = refused.expect("refused before 20 writes");
        assert!(e.contains("Summarise"), "{e}");
        assert!(n.bytes() <= MAX_NOTES_BYTES);
        assert!(n.set_text(&"y ".repeat(MAX_NOTES_BYTES)).is_err());
    }

    #[test]
    fn prompt_block_only_with_notes() {
        assert_eq!(Notes::default().prompt_block(), None);
        let n = Notes::from_text("## Threads\n- Madeira Polyneon\n");
        let b = n.prompt_block().expect("block");
        assert!(
            b.contains("<notes>\n## Threads\n- Madeira Polyneon\n</notes>"),
            "{b}"
        );
    }

    #[test]
    fn files_round_trip_and_empty_notes_remove_the_file() {
        let dir = openstitch_testutil::TempDir::new("notes");
        let path = dir.path().join(FILE_NAME);
        assert_eq!(
            Notes::load(&path).expect("missing is empty"),
            Notes::default()
        );
        let mut n = Notes::default();
        n.write("Machine", "NV880E", WriteMode::Append).expect("w");
        n.save(&path).expect("save");
        assert_eq!(Notes::load(&path).expect("load"), n);
        Notes::default().save(&path).expect("clear");
        assert!(!path.exists());
        std::fs::write(&path, "a".repeat(MAX_NOTES_BYTES * 3)).expect("w");
        assert_eq!(
            Notes::load(&path).expect("load").bytes(),
            MAX_NOTES_BYTES + 1
        );
    }
}
