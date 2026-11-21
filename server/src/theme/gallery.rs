//! The themes Settings > Themes shows in its row, for `ghostex theme list` and `apply`: Default,
//! the art themes, then the agent-made looks (`theme_gallery` in
//! packages/settings-catalog/src/data/theme_gallery.rs). Applying one is one `set` of its changes,
//! so it lands in the history and `ghostex theme undo` puts back what it replaced.

use ghostex_settings_catalog::theme_roles::{format_role_value, parse_role_value};
use ghostex_settings_catalog::Platform;
use serde_json::{json, Map, Value};

use super::look;

/// The themes in the row's order, as the catalog lists them for this computer.
fn looks() -> Vec<Value> {
    match ghostex_settings_catalog::theme_gallery(Platform::current()).to_value() {
        Value::Array(looks) => looks,
        _ => Vec::new(),
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// Whether `look` is what the settings show now: its artwork (or none) and every theme colour it
/// sets, the way Settings marks its tile.
fn applied(look: &Value, settings: &Map<String, Value>) -> bool {
    let Some(changes) = look.get("changes").and_then(Value::as_object) else {
        return false;
    };
    let same_role = |a: &str, b: &str| match (parse_role_value(a), parse_role_value(b)) {
        (Ok(a), Ok(b)) => format_role_value(a) == format_role_value(b),
        _ => false,
    };
    let mut colours = 0;
    for (key, value) in changes {
        let wanted = value.as_str().unwrap_or_default();
        if key == "windowArt" {
            let art = look::current(settings, key);
            if art.as_str().unwrap_or("none") != wanted {
                return false;
            }
            continue;
        }
        let Some((appearance @ ("dark" | "light"), role)) = key.split_once('.') else {
            continue;
        };
        colours += 1;
        let now = look::current(settings, &format!("themeRoles.{appearance}.{role}"));
        if !now.as_str().is_some_and(|now| same_role(now, wanted)) {
            return false;
        }
    }
    colours > 0
}

/// Every theme of the row: id, name, whether it is an art theme, and whether it is applied now
/// (the first match only, as Settings marks one tile).
pub(crate) fn themes(settings: &Map<String, Value>) -> Vec<Value> {
    let mut marked = false;
    looks()
        .iter()
        .map(|look| {
            let is_applied = !marked && applied(look, settings);
            marked |= is_applied;
            json!({
                "id": text(look, "id"),
                "name": text(look, "name"),
                "art": look.get("art").and_then(Value::as_bool) == Some(true),
                "prompt": text(look, "prompt"),
                "applied": is_applied,
            })
        })
        .collect()
}

/// The changes applying theme `wanted` (its id or name, any case) makes, and its name. The themes
/// are dark themes, so an app set to Light switches to Dark with one.
pub(crate) fn changes_for(
    wanted: &str,
    settings: &Map<String, Value>,
) -> Result<(String, Map<String, Value>), String> {
    let wanted = wanted.trim();
    let looks = looks();
    let look = looks
        .iter()
        .find(|look| {
            text(look, "id").eq_ignore_ascii_case(wanted)
                || text(look, "name").eq_ignore_ascii_case(wanted)
        })
        .ok_or_else(|| {
            format!(
                "There is no theme \"{wanted}\". The themes are: {}.",
                looks
                    .iter()
                    .map(|look| text(look, "id"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let mut changes = look
        .get("changes")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if look::current(settings, "sidebarTheme").as_str() == Some("plain-light") {
        changes.insert("sidebarTheme".into(), json!("dark-2"));
    }
    Ok((text(look, "name").to_string(), changes))
}
