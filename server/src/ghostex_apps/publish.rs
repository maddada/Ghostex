//! `/api/publishGhostexApp` `{name, html, icon: {kind, base64}, sourcePath?, sessionRef?}`: stores
//! an app `ghostex app show` prepared, as a new app or the next version of the app made from the
//! same file, and answers `{id, name, version, created, path, newPermissions}`.

use base64::Engine as _;
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

use super::{
    icon::{check_icon, IconKind},
    manifest::declared_powers,
    serve::APP_ROUTE_PREFIX,
    store::{clip_chars, publish, NewVersion, APP_MAX_BYTES, APP_NAME_MAX_CHARS},
};

pub(crate) const PUBLISH_GHOSTEX_APP_ENDPOINT: &str = "/api/publishGhostexApp";
/// The JSON body carries the app as an escaped string, which can grow it to twice its size, plus
/// its icon in base64.
pub(crate) const PUBLISH_GHOSTEX_APP_BODY_LIMIT_BYTES: usize = 2 * APP_MAX_BYTES + 1024 * 1024;

pub(super) fn publish_app(
    state_dir: &std::path::Path,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
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
        .map(|name| clip_chars(name, APP_NAME_MAX_CHARS))
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
    check_icon(kind, &bytes).map_err(DomainStateError::bad_request)?;
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
            session_ref: text("sessionRef"),
            icon: (kind, &bytes),
            powers: &powers,
        },
    )
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not save the app: {error}"),
    })?;
    Ok(json!({
        "id": published.id,
        "name": name,
        "version": published.version,
        "created": published.created,
        "path": format!("{APP_ROUTE_PREFIX}{}", published.id),
        "permissions": powers.iter().map(|power| power.name()).collect::<Vec<_>>(),
        "newPermissions": published.new_powers.iter().map(|power| power.name()).collect::<Vec<_>>(),
    }))
}
