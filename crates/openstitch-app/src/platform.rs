//! What differs between the desktop editor and the browser build (#693).
//!
//! On the desktop every function here is a thin pass-through to exactly what the editor called
//! before; the browser build (`wasm32`) gets its own version, because the crate behind the
//! desktop one cannot run in a browser. Phase A1 holds the release-tag check; the file dialogs,
//! clipboard, preferences and clock follow with the call sites that use them.

/// Is `tag` a release tag (`vX.Y.Z`, optionally `-alpha.N`, `-beta.N` or `-rc.N`)? On the
/// desktop, `openstitch_update::is_release_tag`; in the browser, where openstitch-update (an
/// HTTPS client) does not build, the same grammar.
#[cfg(not(target_arch = "wasm32"))]
pub fn is_release_tag(tag: &str) -> bool {
    openstitch_update::is_release_tag(tag)
}

#[cfg(target_arch = "wasm32")]
pub fn is_release_tag(tag: &str) -> bool {
    release_tag_grammar(tag)
}

/// openstitch-update's tag grammar (`version::parse_tag`) without the semver parse that ends
/// it, which every tag passing these checks satisfies. The test below holds the two together.
#[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
fn release_tag_grammar(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else {
        return false;
    };
    let (core, pre) = match rest.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (rest, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 || !parts.iter().all(|p| plain_number(p)) {
        return false;
    }
    match pre {
        None => true,
        Some(pre) => pre
            .split_once('.')
            .is_some_and(|(kind, n)| ["alpha", "beta", "rc"].contains(&kind) && plain_number(n)),
    }
}

/// A decimal number without sign, spaces or leading zeros (other than `0`), fitting a u64.
#[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
fn plain_number(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit())
        && !(s.len() > 1 && s.starts_with('0'))
        && s.parse::<u64>().is_ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_browser_tag_check_agrees_with_openstitch_update() {
        let tags = [
            "v0.1.0",
            "v1.2.3",
            "v0.1.0-alpha.13",
            "v0.2.0-beta.1",
            "v1.0.0-rc.0",
            "v10.20.30-rc.4",
            "v0.1.0-alpha",
            "v0.1.0-alpha.01",
            "v0.1.0-gamma.1",
            "v0.1.0-alpha.1.2",
            "v01.1.0",
            "v1.2",
            "v1.2.3.4",
            "1.2.3",
            "V1.2.3",
            "v1.2.3-",
            "v1.2.3-rc.",
            "v-1.2.3",
            "v1.2.+3",
            "v1. 2.3",
            "v99999999999999999999.0.0",
            "main-abc1234",
            "pr123",
            "",
            "v",
        ];
        for tag in tags {
            assert_eq!(
                super::release_tag_grammar(tag),
                openstitch_update::is_release_tag(tag),
                "{tag:?}"
            );
            assert_eq!(
                super::is_release_tag(tag),
                openstitch_update::is_release_tag(tag)
            );
        }
        assert!(super::release_tag_grammar("v0.1.0-alpha.13"));
        assert!(!super::release_tag_grammar("main-abc1234"));
    }
}
