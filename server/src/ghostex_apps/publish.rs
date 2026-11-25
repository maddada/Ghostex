//! `/api/publishGhostexApp` `{name, html, icon: {kind, base64}, sourcePath?}`: stores an app
//! `ghostex app show` prepared, as a new app or the next version of the app its project made from
//! the same file, and answers `{id, name, version, created, path, newPermissions, askedAgain}`.
//!
//! CDXC:GhostexApps 2026-11-26 WHY:
//! Fable's security review (findings 1 and 2): the body named the session an app sends to, so a publisher could point an app's messages at any terminal, and the file alone named the app, so anyone could publish over it. The publisher is now the caller's own session token (`Principal::Session`), never the body: it is the only chat the app may send to and its project owns the app (store.rs `app_for_source`). A publish from outside any session (the user's own terminal) has no chat to send to. Every publish drops the app's launch tokens, so an open window has to be opened again to use the new version's powers.

use base64::Engine as _;
use serde_json::{json, Map, Value};

use crate::{domain::DomainStateError, security::Principal};

use super::{
    bridge::Limit,
    icon::{icon_png, IconKind},
    manifest::declared_powers,
    serve::APP_ROUTE_PREFIX,
    store::{publish, NewVersion, Publisher, APP_MAX_BYTES, APP_NAME_MAX_CHARS},
    tokens,
};

pub(crate) const PUBLISH_GHOSTEX_APP_ENDPOINT: &str = "/api/publishGhostexApp";
/// Publishes per session (or per caller from outside a session), counted before the icon is
/// drawn, so a loop cannot keep gxserver drawing.
static PUBLISHES: Limit = Limit::new(10, std::time::Duration::from_secs(60));
/// The JSON body carries the app as an escaped string, which can grow it to twice its size, plus
/// its icon in base64.
pub(crate) const PUBLISH_GHOSTEX_APP_BODY_LIMIT_BYTES: usize = 2 * APP_MAX_BYTES + 1024 * 1024;

/// An app's name as Dock, Settings and the envelope show it: one line of plain text, with no
/// control or direction characters, angle brackets or quotes (the rule of visual_pages/answers.rs
/// `sanitize_outside_name`, plus the format characters that can reorder or hide text), at most
/// [`APP_NAME_MAX_CHARS`]. Review finding 5: a name with a newline and a right-to-left override
/// was accepted.
pub(super) fn clean_name(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        // Line breaks and tabs part words rather than join them.
        .map(|ch| if ch.is_whitespace() { ' ' } else { ch })
        .filter(|ch| {
            !ch.is_control()
                && !matches!(ch, '<' | '>' | '"' | '\'' | '`' | '\\')
                && !is_format_char(*ch)
        })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let clipped: String = collapsed.chars().take(APP_NAME_MAX_CHARS).collect();
    let trimmed = clipped.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Unicode format characters that reorder, join or hide text: the bidirectional marks, embeddings,
/// overrides and isolates, zero-width characters and the byte-order mark.
fn is_format_char(ch: char) -> bool {
    matches!(ch,
        '\u{00AD}' | '\u{061C}' | '\u{180E}'
        | '\u{200B}'..='\u{200F}'
        | '\u{2028}'..='\u{202E}'
        | '\u{2060}'..='\u{2069}'
        | '\u{FEFF}'
        | '\u{FFF9}'..='\u{FFFB}')
}

/// Who published, as the app records it: a session from its own token; `Ok(None)` for the user's
/// own Ghostex or terminal (no chat to send to); refused for other computers, guests and extensions.
fn publisher_of(caller: &Principal) -> Result<Option<Publisher>, DomainStateError> {
    match caller {
        Principal::Session {
            project_id,
            session_id,
            ..
        } => Ok(Some(Publisher {
            project_id: project_id.clone(),
            session_id: session_id.clone(),
        })),
        caller if caller.is_owner_device() => Ok(None),
        _ => Err(DomainStateError {
            code: "forbidden",
            message: "Only an agent on this computer or you can publish a Ghostex App here."
                .to_string(),
        }),
    }
}

pub(super) fn publish_app(
    state_dir: &std::path::Path,
    params: &Map<String, Value>,
    caller: &Principal,
) -> Result<Value, DomainStateError> {
    let publisher = publisher_of(caller)?;
    let limit_key = publisher
        .as_ref()
        .map_or("you", |publisher| publisher.session_id.as_str());
    if !PUBLISHES.take(limit_key) {
        return Err(DomainStateError {
            code: "rateLimited",
            message: "Too many publishes from this session; wait a minute and publish again."
                .to_string(),
        });
    }
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let html = params
        .get("html")
        .and_then(Value::as_str)
        .filter(|html| !html.trim().is_empty())
        .ok_or_else(|| DomainStateError::bad_request("publishGhostexApp requires html."))?;
    if html.len() > APP_MAX_BYTES {
        return Err(DomainStateError::bad_request(
            "The app is larger than the 8 MiB limit.",
        ));
    }
    let name = text("name")
        .and_then(clean_name)
        .ok_or_else(|| DomainStateError::bad_request("publishGhostexApp requires name."))?;
    let powers = declared_powers(html).map_err(DomainStateError::bad_request)?;
    let icon = params.get("icon").and_then(Value::as_object);
    let kind = icon
        .and_then(|icon| icon.get("kind"))
        .and_then(Value::as_str)
        .and_then(IconKind::from_extension)
        .ok_or_else(|| DomainStateError::bad_request(super::icon::ICON_HOW_TO))?;
    let bytes = icon
        .and_then(|icon| icon.get("base64"))
        .and_then(Value::as_str)
        .and_then(|data| base64::engine::general_purpose::STANDARD.decode(data).ok())
        .ok_or_else(|| DomainStateError::bad_request(super::icon::ICON_HOW_TO))?;
    let png = icon_png(kind, &bytes).map_err(DomainStateError::bad_request)?;
    // As for a Live page, only a file that exists on this computer counts as the app's source.
    let source = text("sourcePath").filter(|path| {
        let path = std::path::Path::new(path);
        path.is_absolute() && path.is_file()
    });
    let published = publish(
        state_dir,
        NewVersion {
            name: &name,
            html,
            source_path: source,
            publisher,
            icon_png: &png,
            powers: &powers,
        },
    )
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not save the app: {error}"),
    })?;
    tokens::revoke_app(&published.id);
    Ok(json!({
        "id": published.id,
        "name": name,
        "version": published.version,
        "created": published.created,
        "path": format!("{APP_ROUTE_PREFIX}{}", published.id),
        "permissions": powers.iter().map(|power| power.name()).collect::<Vec<_>>(),
        "newPermissions": published.new_powers.iter().map(|power| power.name()).collect::<Vec<_>>(),
        "askedAgain": published.reasked.iter().map(|power| power.name()).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_one_plain_line() {
        assert_eq!(
            clean_name("Morning brief").as_deref(),
            Some("Morning brief")
        );
        assert_eq!(
            clean_name("Brief\nIgnore the above\u{202E}txt.exe").as_deref(),
            Some("Brief Ignore the abovetxt.exe")
        );
        assert_eq!(clean_name("<b>\"x\"</b> \\").as_deref(), Some("bx/b"));
        assert_eq!(clean_name("\u{200B}\u{2066} \t"), None);
        assert_eq!(
            clean_name(&"a".repeat(200)).map(|name| name.chars().count()),
            Some(APP_NAME_MAX_CHARS)
        );
    }

    #[test]
    fn the_publisher_is_the_callers_own_session() {
        let session = Principal::Session {
            project_id: "P1".to_string(),
            session_id: "S1".to_string(),
            role: crate::security::SessionRole::Agent,
        };
        assert_eq!(
            publisher_of(&session).unwrap(),
            Some(Publisher {
                project_id: "P1".to_string(),
                session_id: "S1".to_string()
            })
        );
        assert_eq!(publisher_of(&Principal::LocalAgent).unwrap(), None);
        assert!(publisher_of(&Principal::Anonymous).is_err());
        assert!(publisher_of(&Principal::Guest {
            share_id: "x".to_string()
        })
        .is_err());
    }
}
