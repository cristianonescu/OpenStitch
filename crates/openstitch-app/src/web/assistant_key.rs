//! The browser Assistant's remembered keys (#766), only after the user ticked **Remember on
//! this device**: this site's `localStorage`, one entry per model address,
//! `openstitch.ai.key <address>`. The desktop keeps keys in the system keychain instead; a page
//! has none. Nothing else stores a key: not the preferences, a file, a URL or a log.

/// The storage entry of the key for `base_url`.
fn entry(base_url: &str) -> String {
    format!("openstitch.ai.key {}", base_url.trim())
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// The key remembered for `base_url`, if any.
pub fn load(base_url: &str) -> Option<String> {
    storage()?
        .get_item(&entry(base_url))
        .ok()
        .flatten()
        .filter(|k| !k.is_empty())
}

/// Remembers `key` for `base_url` (an empty key forgets it). A browser that refuses storage
/// (a private window) keeps nothing, and the key stays for this session only.
pub fn store(base_url: &str, key: &str) {
    if key.is_empty() {
        forget(base_url);
    } else if let Some(s) = storage() {
        let _ = s.set_item(&entry(base_url), key);
    }
}

/// Forgets the key remembered for `base_url`.
pub fn forget(base_url: &str) {
    if let Some(s) = storage() {
        let _ = s.remove_item(&entry(base_url));
    }
}
