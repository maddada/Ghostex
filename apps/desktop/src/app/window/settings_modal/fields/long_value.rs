//! Long opaque values (build ids, hashes, fingerprints, tokens, long paths) are never shown whole
//! in Settings: the page shows a short form and the full value on hover.
//!
//! CDXC:Settings 2026-11-09 DECISION:
//! User, of Settings > Updates' "VS Code view 6f3e4df1e6b8-p2-2dbe7754240c…": "hide this long id and
//! all such long ids in all of settings". A version that is a real version number stays; a build id
//! or hash after it is cut off, and a value with no version in it shows nothing (the name says what
//! it is). Every shortened value keeps its whole text in a hover tooltip, and stays copyable where
//! it already was.
use gpui::{SharedString, StatefulInteractiveElement};

/// The longest value shown whole.
const LONGEST_SHOWN: usize = 24;
/// A run of hex digits this long is a hash, never a version.
const HASH_RUN: usize = 12;

fn longest_hex_run(value: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for ch in value.chars() {
        if ch.is_ascii_hexdigit() {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest
}

/// Whether `value` is the kind Settings never shows whole.
pub(crate) fn is_long_value(value: &str) -> bool {
    value.chars().count() > LONGEST_SHOWN || longest_hex_run(value) >= HASH_RUN
}

/// The version to show for `version`: itself when it reads as a version, its leading version
/// number when a build id or hash follows (`2.1.3-6f3e…` is `2.1.3`), else nothing.
pub(crate) fn short_version(version: &str) -> Option<String> {
    let version = version.trim();
    if version.is_empty() {
        return None;
    }
    if !is_long_value(version) {
        return Some(version.to_string());
    }
    let start = version.strip_prefix('v').unwrap_or(version);
    let number: String = start
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect();
    let number = number.trim_end_matches('.');
    (number.contains('.') && number.len() <= LONGEST_SHOWN).then(|| number.to_string())
}

/// A long id cut to its first characters and an ellipsis (`6f3e4df1…`); short ones stay whole.
pub(crate) fn short_id(value: &str) -> String {
    if !is_long_value(value) {
        return value.to_string();
    }
    let head: String = value.chars().take(8).collect();
    format!("{head}…")
}

/// A long path cut from the front to its last two parts (`…/Ghostex/settings.json`).
pub(crate) fn short_path(path: &str) -> String {
    if path.chars().count() <= 40 {
        return path.to_string();
    }
    let separator = if path.contains('\\') && !path.contains('/') {
        '\\'
    } else {
        '/'
    };
    let parts: Vec<&str> = path
        .split(separator)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() <= 2 {
        return path.to_string();
    }
    format!(
        "…{separator}{}{separator}{}",
        parts[parts.len() - 2],
        parts[parts.len() - 1]
    )
}

/// `element` with the full value in a hover tooltip.
pub(crate) fn full_value_tooltip<E: StatefulInteractiveElement>(
    element: E,
    full: impl Into<SharedString>,
) -> E {
    let full: SharedString = full.into();
    element.tooltip(move |window, cx| {
        gpui_component::tooltip::Tooltip::new(full.clone()).build(window, cx)
    })
}
