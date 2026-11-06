//! Command palette (#550, beyond PE-DESIGN): every ribbon, menu and Quick Access command,
//! found by words from its label, ScreenTip, key or ribbon path ("arrange", "satin",
//! "export"), recently run commands first. Running an entry is the same as clicking the
//! button: the window calls the ribbon's `run` with the action, so the request and the undo
//! step are the same. The Assistant reaches the same index with `ui.searchCommands`.
//!
//! Nothing here depends on egui.

use super::ribbon::{Action, Item, TabId};

/// Most commands remembered as recent (saved in ui.json).
pub const MAX_RECENT: usize = 10;

/// The ribbon tab and group holding `a`, if it is a ribbon button.
pub fn place(a: Action) -> Option<(TabId, &'static str)> {
    TabId::ALL.into_iter().find_map(|tab| {
        tab.groups()
            .iter()
            .find(|g| {
                g.items.iter().any(|i| match i {
                    Item::Large(x) => *x == a,
                    Item::Small(xs) => xs.contains(&a),
                })
            })
            .map(|g| (tab, g.name))
    })
}

/// Where a command lives: "Home ▸ Edit" for a ribbon button, else the menu or toolbar.
pub fn path(a: Action) -> String {
    if let Some((tab, group)) = place(a) {
        return format!("{} ▸ {group}", tab.label());
    }
    use super::ribbon::{ALIGN_MENU, APPLICATION_MENU, OPTION_MENU, SELECT_SAME_MENU};
    let within = |menu: Action| format!("{} ▸ {}", path(menu), menu.info().label);
    if ALIGN_MENU.contains(&a) {
        within(Action::Align)
    } else if SELECT_SAME_MENU.contains(&a) {
        within(Action::SelectSame)
    } else if OPTION_MENU.contains(&a) {
        "Options menu".into()
    } else if APPLICATION_MENU.contains(&a) || a == Action::Exit {
        "Application menu".into()
    } else {
        match a {
            Action::Undo | Action::Redo | Action::Help | Action::Options => {
                "Quick Access Toolbar".into()
            }
            Action::CopyAttributes | Action::PasteAttributes => "Sewing Attributes pane".into(),
            _ => "Ribbon".into(),
        }
    }
}

/// How well `a` matches `query` (lower-case words); `None` when a word matches nowhere. A
/// word at the start of the label scores most, then inside the label, then the path, key
/// and ScreenTip; letters of the label in order (`exd` for "Export Design") score least.
pub fn score(a: Action, query: &str) -> Option<u32> {
    let info = a.info();
    let label = info.label.to_lowercase();
    let path = path(a).to_lowercase();
    let key = info.key.to_lowercase();
    let tip = info.tip.to_lowercase();
    let mut total = 0;
    for word in query.split_whitespace() {
        let w = word.to_lowercase();
        let s = if label.starts_with(&w) {
            100
        } else if label
            .split([' ', '/', '-'])
            .any(|part| part.starts_with(&w))
        {
            80
        } else if label.contains(&w) {
            60
        } else if path.contains(&w) || key.contains(&w) {
            40
        } else if tip.contains(&w) {
            20
        } else if subsequence(&label, &w) {
            5
        } else {
            return None;
        };
        total += s;
    }
    Some(total)
}

fn subsequence(text: &str, word: &str) -> bool {
    let mut chars = text.chars();
    word.chars().all(|c| chars.any(|t| t == c))
}

/// Commands matching `query`, best first; an empty query lists the recent ones (by key, newest
/// first), then the rest in table order. Among equal scores a recent command comes first.
pub fn search(query: &str, recent: &[String]) -> Vec<Action> {
    let rank = |a: Action| {
        recent
            .iter()
            .position(|k| k == a.info().key)
            .unwrap_or(usize::MAX)
    };
    let mut hits: Vec<(u32, usize, usize, Action)> = Action::ALL
        .iter()
        .enumerate()
        .filter_map(|(i, &a)| {
            let s = if query.trim().is_empty() {
                0
            } else {
                score(a, query)?
            };
            Some((s, rank(a), i, a))
        })
        .collect();
    hits.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    hits.into_iter().map(|h| h.3).collect()
}

/// `recent` with `key` moved to the front, at most [`MAX_RECENT`] long.
pub fn remember(recent: &mut Vec<String>, key: &str) {
    recent.retain(|k| k != key);
    recent.insert(0, key.to_string());
    recent.truncate(MAX_RECENT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_found_by_its_label() {
        for &a in Action::ALL {
            let hits = search(a.info().label, &[]);
            assert!(hits.contains(&a), "{a:?} ({}) not found", a.info().label);
        }
    }

    #[test]
    fn words_and_paths_find_commands() {
        assert_eq!(
            search("arrange copy", &[]).first(),
            Some(&Action::ArrangeCopy)
        );
        assert!(search("export", &[]).contains(&Action::Export));
        assert!(path(Action::Cut).starts_with("Home ▸"));
        assert_eq!(path(Action::Exit), "Application menu");
        assert_eq!(path(Action::SelectSameColor), "Home ▸ Select ▸ Select Same");
        assert_eq!(path(Action::DesignDatabase), "Options menu");
        assert!(search("zzzzqqq", &[]).is_empty());
    }

    #[test]
    fn recent_commands_come_first() {
        let mut recent = Vec::new();
        remember(&mut recent, "paste");
        remember(&mut recent, "export");
        remember(&mut recent, "paste");
        assert_eq!(recent, ["paste", "export"]);
        let empty = search("", &recent);
        assert_eq!(&empty[..2], &[Action::Paste, Action::Export]);
        for k in 0..20 {
            remember(&mut recent, &format!("k{k}"));
        }
        assert_eq!(recent.len(), MAX_RECENT);
    }
}
