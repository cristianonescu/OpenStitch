//! The real system keychain, with a dummy key under a throwaway service name (#401): the
//! Windows Credential Manager, the Linux Secret Service or the macOS Keychain. Ignored by
//! default, so a developer's `cargo test` never touches their keychain; CI runs it with
//! `cargo test -p openstitch-app --test integration -- --ignored keychain::` on all three systems.
//!
//! It goes through `openstitch_app::keys`, the same functions the Assistant's Save key and
//! Remove key use. The key is a made-up string, never a real one.

use openstitch_app::keys;

#[test]
#[ignore = "touches the system keychain; run with --ignored"]
fn a_dummy_key_is_saved_read_back_and_removed() {
    let service = format!("OpenStitch keychain test {}", std::process::id());
    let url = "https://example.invalid/v1";
    let dummy = "dummy-not-a-real-key-0123456789";
    let entry = keys::entry(&service, url).expect("a keychain entry");
    assert_eq!(keys::load(&entry), None, "a fresh service holds nothing");

    keys::store(&entry, dummy).expect("save the dummy key");
    // A fresh entry, as the next launch would make it.
    let again = keys::entry(&service, url).expect("entry");
    assert_eq!(keys::load(&again).as_deref(), Some(dummy), "read back");

    // Another address under the same service is a different entry.
    let other = keys::entry(&service, "http://localhost:11434/v1").expect("entry");
    assert_eq!(keys::load(&other), None, "one entry per address");

    // Saving again replaces the key.
    keys::store(&again, "dummy-second-value").expect("replace");
    assert_eq!(
        keys::load(&keys::entry(&service, url).expect("entry")).as_deref(),
        Some("dummy-second-value")
    );

    // Remove key: removed and read back as gone; removing again is not an error.
    keys::store(&again, "").expect("remove");
    assert_eq!(
        keys::load(&keys::entry(&service, url).expect("entry")),
        None,
        "gone"
    );
    keys::store(&again, "").expect("removing twice is fine");
}

#[test]
fn the_service_name_can_be_pointed_elsewhere() {
    // Only the name: nothing is read or written here.
    assert_eq!(keys::SERVICE, "OpenStitch assistant");
    assert!(!keys::service().trim().is_empty());
}
