//! What the chat can do with a Markdown destination.
//!
//! Ported from `packages/shared/session-chat-presentation/links.ts`.

use crate::transcript::file_position::{split_file_position, FilePosition};
use crate::transcript::jsstr::{decode_uri, js_trim};

/// What a href resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkTarget {
    Url(String),
    File(String),
    /// Nothing the chat's own surfaces can show.
    Inert,
}

/// CDXC:SessionChat 2026-09-22 WHY:
/// `value[..prefix.len()]` panics whenever the cut falls inside a character, and `value` here is
/// an href out of the agent's own Markdown: `file:/` followed by any multi-byte character was
/// enough to take the host thread down. `get` answers `None` there instead.
fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// `^[a-z][a-z0-9+.-]*:` case-insensitively: any URI scheme at all (`mailto:`, `vscode:`, `data:`).
fn has_uri_scheme(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.first().is_none_or(|byte| !byte.is_ascii_alphabetic()) {
        return false;
    }
    let end = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
        .count();
    bytes.get(end) == Some(&b':')
}

/// `^[a-z]:[\\/]` case-insensitively: a Windows drive path, which also matches a one-letter scheme.
fn is_windows_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(1) == Some(&b':')
        && matches!(bytes.get(2), Some(b'\\' | b'/'))
}

/// Classifies a Markdown href into what the chat can do with it.
///
/// Image hrefs are handled before this by the image viewer, so they arrive here only when no viewer
/// can show them, in which case they behave like any other file.
pub fn classify_link_href(href: &str) -> LinkTarget {
    let trimmed = js_trim(href);
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return LinkTarget::Inert;
    }
    if starts_with_ignore_ascii_case(trimmed, "http://")
        || starts_with_ignore_ascii_case(trimmed, "https://")
    {
        return LinkTarget::Url(trimmed.to_string());
    }
    if starts_with_ignore_ascii_case(trimmed, "file://") {
        return LinkTarget::File(file_path_from_href(file_uri_path(
            &trimmed["file://".len()..],
        )));
    }
    /*
    CDXC:SessionChat 2026-09-19 WHY:
    The scheme test runs on the destination with its editor coordinates removed, because `Makefile:12`
    and `README:5` are a filename and a line, not a URI. Inline code judges those two by the file rule
    (resolve_inline_code_file_path) and the composer already strips the suffix the same way, so leaving
    it on here made the same reference clickable as inline code and inert as a link.
    */
    if !is_windows_drive_path(trimmed) && has_uri_scheme(split_file_position(trimmed).0) {
        return LinkTarget::Inert;
    }
    LinkTarget::File(file_path_from_href(trimmed))
}

/// Markdown destinations arrive percent-encoded and often carry the editor coordinates an agent
/// quoted them with; the host needs the literal path.
fn file_path_from_href(href: &str) -> String {
    split_file_position(&drive_path_without_leading_slash(decoded_file_href(href)))
        .0
        .to_string()
}

/// What follows `file://`, without a `localhost` host: `file://localhost/C:/x` names the same
/// file as `file:///C:/x`.
pub(crate) fn file_uri_path(rest: &str) -> &str {
    match rest.get(.."localhost/".len()) {
        Some(head) if head.eq_ignore_ascii_case("localhost/") => &rest["localhost".len()..],
        _ => rest,
    }
}

/// `/C:/x` (what `file:///C:/x` leaves once `file://` is gone) as `C:/x`, for any drive letter and
/// either slash. A `/C%3A/x` from a URI that encoded the drive colon counts too, because
/// `decodeURI` keeps `%3A` encoded.
///
/// CDXC:SessionChat 2026-11-02 WHY:
/// On Windows `/C:/x` is not an absolute path, so the desktop read it as a WSL path, found nothing and copied it instead of opening the file (GitHub issue #391), and most agents write Windows file links as `file:///C:/...`. It is dropped here on every platform, because a POSIX path starting with `/<letter>:/` is not something an agent links to, and a remote Windows session opened from a Mac then resolves the same path as a local one.
pub(crate) fn drive_path_without_leading_slash(decoded: String) -> String {
    let Some(rest) = decoded.strip_prefix('/') else {
        return decoded;
    };
    if is_windows_drive_path(rest) {
        return rest.to_string();
    }
    let bytes = rest.as_bytes();
    if bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && rest
            .get(1..4)
            .is_some_and(|colon| colon.eq_ignore_ascii_case("%3A"))
        && matches!(bytes.get(4), Some(b'\\' | b'/'))
    {
        return format!("{}:{}", &rest[..1], &rest[4..]);
    }
    decoded
}

fn decoded_file_href(href: &str) -> String {
    // Malformed escapes: use the raw href.
    decode_uri(href).unwrap_or_else(|| href.to_string())
}

/// Preserves editor coordinates from a Markdown destination after path decoding.
pub fn file_position_from_href(href: &str) -> Option<FilePosition> {
    split_file_position(&decoded_file_href(href)).1
}
