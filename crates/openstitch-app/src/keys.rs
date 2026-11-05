//! The Assistant's API keys in the system keychain: the macOS Keychain, the Windows Credential
//! Manager or the Linux Secret Service (through `keyring`). One entry per endpoint address,
//! under a service name: "OpenStitch assistant" for the user, or the throwaway name in
//! `OPENSTITCH_KEYCHAIN_SERVICE` for a manual test run.
//!
//! The editor's own unit tests never come here (`assistant_panel::keychain` returns no entry
//! under `cfg(test)`). `tests/keychain.rs` (in the `integration` test binary) does, on purpose, with a dummy key under a test
//! service name, and only when asked (`--ignored`); CI runs it on all three systems (#401).

/// The service the user's keys are stored under.
pub const SERVICE: &str = "OpenStitch assistant";

/// The service to use: `OPENSTITCH_KEYCHAIN_SERVICE` when set, else [`SERVICE`].
pub fn service() -> String {
    std::env::var("OPENSTITCH_KEYCHAIN_SERVICE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| SERVICE.to_string())
}

/// The keychain entry for `base_url` under `service`.
pub fn entry(service: &str, base_url: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(service, base_url).ok()
}

/// The stored key, if any (an empty one counts as none).
pub fn load(entry: &keyring::Entry) -> Option<String> {
    entry.get_password().ok().filter(|k| !k.is_empty())
}

/// Stores `key`; an empty `key` removes the entry and reads it back, so "removed" means gone
/// (the owner, 2026-11-04).
pub fn store(entry: &keyring::Entry, key: &str) -> Result<(), String> {
    if key.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.to_string()),
        }
        match entry.get_password() {
            Err(keyring::Error::NoEntry) => Ok(()),
            Ok(_) => Err("the system keychain still holds the key".into()),
            Err(e) => Err(format!("could not check that the key is gone: {e}")),
        }
    } else {
        entry.set_password(key).map_err(|e| e.to_string())
    }
}
