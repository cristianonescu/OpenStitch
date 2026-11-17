//! Which coding-agent session goes with which project's chat (#1170), so that reopening a
//! project on the same PC can continue the agent's own session (ACP `session/load`) instead of
//! sending the chat as quoted text (#1169).
//!
//! The record lives in the user's config folder (beside `ui.json`), never in the `.osd`: a
//! session id means something only on this PC, for this account, while the agent keeps that
//! session, and it points outside the project. Each record holds a digest of the project file's
//! path (not the path: the file names no project in the clear), the agent, the agent's session
//! id, and a fingerprint of the chat the file was saved with; a session is
//! offered again only for the same agent and a chat with the same fingerprint, so a file changed
//! elsewhere (another PC, an older copy, an edit by hand) falls back to the quoted text.
//!
//! Nothing here depends on egui.

use openstitch_design::saved_chat::SavedChat;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The file beside `ui.json`.
pub const FILE_NAME: &str = "assistant-sessions.json";
/// The most projects remembered; the oldest go first.
pub const MAX_RECORDS: usize = 200;
/// The longest session id kept (claude-agent-acp's are 36-character UUIDs).
const MAX_ID: usize = 200;

/// One project's agent session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSession {
    /// [`project_key`]: a digest of the project file's path, not the path.
    pub project: String,
    /// The coding agent (`openstitch_assistant::KNOWN_AGENTS` id).
    pub agent: String,
    /// The agent's own session id.
    pub session: String,
    /// [`fingerprint`] of the chat the file was saved with.
    pub chat: String,
    /// When it was recorded (seconds since 1970), to drop the oldest past [`MAX_RECORDS`].
    pub saved: u64,
}

/// Every record, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sessions {
    pub records: Vec<AgentSession>,
}

/// FNV-1a, 64 bits, from `basis`.
fn fnv(text: &str, basis: u64) -> u64 {
    let mut h = basis;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// A project file's key: a 128-bit digest (two FNV-1a passes) of its absolute path as the editor
/// holds it (nothing resolved or read), so the file holds no project path in the clear. It finds
/// a record for a path; it is not a secret.
pub fn project_key(project: &Path) -> String {
    let path = std::path::absolute(project)
        .unwrap_or_else(|_| project.to_path_buf())
        .display()
        .to_string();
    format!(
        "{:016x}{:016x}",
        fnv(&path, 0xcbf2_9ce4_8422_2325),
        fnv(&path, 0x6c62_272e_07bb_0142)
    )
}

/// A short digest of a saved chat (FNV-1a, 64 bits, over its JSON form): equal for the chat a
/// save wrote and the chat the file gives back when opened.
pub fn fingerprint(chat: &SavedChat) -> String {
    let text = serde_json::to_string(chat).unwrap_or_default();
    format!("{:016x}", fnv(&text, 0xcbf2_9ce4_8422_2325))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Sessions {
    /// Reads the file; a missing or unreadable one is empty (the chat then goes as quoted text).
    pub fn load(path: &Path) -> Sessions {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Sessions::default();
        };
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let text = |r: &Value, k: &str| {
            r[k].as_str()
                .filter(|s| !s.is_empty() && s.len() <= 4096)
                .map(str::to_string)
        };
        let mut records: Vec<AgentSession> = v["sessions"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|r| {
                        Some(AgentSession {
                            project: text(r, "project")?,
                            agent: text(r, "agent")?,
                            session: text(r, "session").filter(|s| s.len() <= MAX_ID)?,
                            chat: text(r, "chat")?,
                            saved: r["saved"].as_u64().unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        records.sort_by_key(|r| r.saved);
        let extra = records.len().saturating_sub(MAX_RECORDS);
        records.drain(..extra);
        Sessions { records }
    }

    /// Writes the file (through a temporary file, so a crash leaves the old one); on Unix only the
    /// user can read it.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let body = json!({
            "version": 1,
            "sessions": self.records.iter().map(|r| json!({
                "project": r.project, "agent": r.agent, "session": r.session,
                "chat": r.chat, "saved": r.saved,
            })).collect::<Vec<_>>(),
        });
        let tmp: PathBuf = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&body)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, path)
    }

    /// The session to continue for `project` with `agent`, when its record was made for exactly
    /// this chat.
    pub fn find(&self, project: &Path, agent: &str, chat: &SavedChat) -> Option<&str> {
        let key = project_key(project);
        let print = fingerprint(chat);
        self.records
            .iter()
            .rev()
            .find(|r| r.project == key)
            .filter(|r| r.agent == agent && r.chat == print)
            .map(|r| r.session.as_str())
    }

    /// Records that `project` was saved with `chat`, from `agent`'s session `session`. Returns
    /// whether anything changed (the caller then writes the file).
    pub fn record(&mut self, project: &Path, agent: &str, session: &str, chat: &SavedChat) -> bool {
        if session.is_empty() || session.len() > MAX_ID || agent.is_empty() {
            return false;
        }
        let key = project_key(project);
        let print = fingerprint(chat);
        if self.records.last().is_some_and(|r| {
            r.project == key && r.agent == agent && r.session == session && r.chat == print
        }) {
            return false;
        }
        self.records.retain(|r| r.project != key);
        self.records.push(AgentSession {
            project: key,
            agent: agent.to_string(),
            session: session.to_string(),
            chat: print,
            saved: now(),
        });
        let extra = self.records.len().saturating_sub(MAX_RECORDS);
        self.records.drain(..extra);
        true
    }

    /// Forgets `project`'s session. Returns whether there was one.
    pub fn forget(&mut self, project: &Path) -> bool {
        let key = project_key(project);
        let before = self.records.len();
        self.records.retain(|r| r.project != key);
        self.records.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_design::saved_chat::{ChatRole, ChatRow};

    fn chat(text: &str) -> SavedChat {
        SavedChat {
            rows: vec![
                ChatRow::new(ChatRole::User, text),
                ChatRow::new(ChatRole::Assistant, "OK"),
            ],
        }
    }

    #[test]
    fn a_session_is_offered_only_for_the_same_agent_and_chat() {
        let dir = openstitch_testutil::TempDir::new("assistant-sessions");
        let project = dir.join("badge.osd");
        std::fs::write(&project, "{}").expect("project");
        let file = dir.join(FILE_NAME);
        let mut s = Sessions::load(&file);
        assert!(s.records.is_empty());
        assert!(s.record(&project, "claude", "abc-1", &chat("hi")));
        // The same record again changes nothing.
        assert!(!s.record(&project, "claude", "abc-1", &chat("hi")));
        s.save(&file).expect("save");
        let text = std::fs::read_to_string(&file).expect("read");
        assert!(
            !text.contains("badge"),
            "no project path in the clear: {text}"
        );
        assert_eq!(project_key(&project).len(), 32);
        let back = Sessions::load(&file);
        assert_eq!(back, s);
        assert_eq!(back.find(&project, "claude", &chat("hi")), Some("abc-1"));
        // Another agent, or a chat changed elsewhere: no session.
        assert_eq!(back.find(&project, "codex", &chat("hi")), None);
        assert_eq!(back.find(&project, "claude", &chat("hello")), None);
        assert_eq!(
            back.find(&dir.join("other.osd"), "claude", &chat("hi")),
            None
        );
        // A newer save replaces the record.
        let mut s = back;
        assert!(s.record(&project, "claude", "abc-2", &chat("hello")));
        assert_eq!(s.records.len(), 1);
        assert_eq!(s.find(&project, "claude", &chat("hello")), Some("abc-2"));
        assert!(s.forget(&project));
        assert!(!s.forget(&project));
        assert_eq!(s.find(&project, "claude", &chat("hello")), None);
    }

    #[test]
    fn the_file_stays_small_and_a_broken_one_reads_as_empty() {
        let dir = openstitch_testutil::TempDir::new("assistant-sessions-limit");
        let file = dir.join(FILE_NAME);
        let mut s = Sessions::default();
        for i in 0..MAX_RECORDS + 5 {
            assert!(s.record(&dir.join(format!("p{i}.osd")), "claude", "id", &chat("x")));
        }
        assert_eq!(s.records.len(), MAX_RECORDS);
        assert!(
            s.records[0].project == project_key(&dir.join("p5.osd")),
            "{:?}",
            s.records[0]
        );
        // Ids that are empty or too long are not kept.
        assert!(!s.record(&dir.join("q.osd"), "claude", "", &chat("x")));
        assert!(!s.record(&dir.join("q.osd"), "claude", &"x".repeat(300), &chat("x")));
        std::fs::write(&file, "not json").expect("write");
        assert!(Sessions::load(&file).records.is_empty());
        assert!(Sessions::load(&dir.join("missing.json")).records.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn only_the_user_can_read_the_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = openstitch_testutil::TempDir::new("assistant-sessions-mode");
        let file = dir.join(FILE_NAME);
        let mut s = Sessions::default();
        s.record(&dir.join("a.osd"), "claude", "id", &chat("x"));
        s.save(&file).expect("save");
        let mode = std::fs::metadata(&file).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn the_fingerprint_follows_the_chat() {
        assert_eq!(fingerprint(&chat("a")), fingerprint(&chat("a")));
        assert_ne!(fingerprint(&chat("a")), fingerprint(&chat("b")));
        assert_eq!(fingerprint(&chat("a")).len(), 16);
    }
}
