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

/// The storage entries of remembered keys, for every address.
fn entries(s: &web_sys::Storage) -> Vec<String> {
    let n = s.length().unwrap_or(0);
    (0..n)
        .filter_map(|i| s.key(i).ok().flatten())
        .filter(|k| k.starts_with("openstitch.ai.key "))
        .collect()
}

/// How many addresses have a remembered key.
pub fn count() -> usize {
    storage().map_or(0, |s| entries(&s).len())
}

/// Forgets the keys remembered for every address; returns how many there were.
pub fn forget_all() -> usize {
    let Some(s) = storage() else {
        return 0;
    };
    let all = entries(&s);
    for k in &all {
        let _ = s.remove_item(k);
    }
    all.len()
}
