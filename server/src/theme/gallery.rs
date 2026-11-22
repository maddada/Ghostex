//! The themes Settings > Themes shows in its row, for `ghostex theme list` and `apply`: Default,
//! the art themes, the agent-made looks and the named presets (`theme_gallery` in
//! packages/settings-catalog/src/data/themes.rs), each with its backgrounds (Art, Live,
//! Transparent, Solid). Applying one is one `set` of its changes and its background's, so it lands
//! in the history and `ghostex theme undo` puts back what it replaced.

use ghostex_settings_catalog::theme_roles::{format_role_value, parse_role_value};
use ghostex_settings_catalog::Platform;
use serde_json::{json, Map, Value};

use super::look;

/// The themes in the row's order, as the catalog lists them for this computer.
pub(crate) fn looks() -> Vec<Value> {
    match ghostex_settings_catalog::theme_gallery(Platform::current()).to_value() {
        Value::Array(looks) => looks,
        _ => Vec::new(),
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// Whether the theme colours `look` sets for its own appearance are the ones saved now ("default"
/// meaning none saved), and how many of its other settings still hold its value.
fn match_score(look: &Value, settings: &Map<String, Value>) -> Option<usize> {
    let changes = look.get("changes").and_then(Value::as_object)?;
    let same_role = |saved: Option<&str>, wanted: &str| match (saved, wanted) {
        (None, "default") => true,
        (Some(saved), wanted) => match (parse_role_value(saved), parse_role_value(wanted)) {
            (Ok(a), Ok(b)) => format_role_value(a) == format_role_value(b),
            _ => false,
        },
        (None, _) => false,
    };
    let mut others = 0;
    let mut colours = 0;
    for (key, value) in changes {
        let wanted = value.as_str().unwrap_or_default();
        if let Some((appearance @ ("dark" | "light"), role)) = key.split_once('.') {
            let saved = look::current(settings, &format!("themeRoles.{appearance}.{role}"));
            if !same_role(saved.as_str(), wanted) {
                return None;
            }
            colours += 1;
            continue;
        }
        let now = look::current(settings, key);
        let now = match &now {
            Value::String(text) => text.clone(),
            other => crate::ghostex_cli::settings::format_value(other),
        };
        if now.eq_ignore_ascii_case(wanted) {
            others += 1;
        }
    }
    (colours > 0).then_some(others)
}

/// The theme the settings show now: of those whose colours all match, the one most of whose
/// other settings still hold (Default and Inkstone share their colours).
pub(crate) fn applied_id(looks: &[Value], settings: &Map<String, Value>) -> Option<String> {
    let mut best: Option<(usize, &Value)> = None;
    for look in looks {
        if let Some(score) = match_score(look, settings) {
            if best.is_none_or(|(top, _)| score > top) {
                best = Some((score, look));
            }
        }
    }
    best.map(|(_, look)| text(look, "id").to_string())
}

/// The background the settings show now in `light` mode: art, solid, live or transparent.
pub(crate) fn current_background(settings: &Map<String, Value>, light: bool) -> &'static str {
    let art = look::current(settings, if light { "windowArtLight" } else { "windowArt" });
    if art
        .as_str()
        .is_some_and(ghostex_settings_catalog::art_id_paints)
    {
        return "art";
    }
    let glass = look::current(settings, "windowGlass");
    match glass.as_str() {
        Some("opaque") => return "solid",
        Some("auto") if light => return "solid",
        _ => {}
    }
    if look::current(settings, "windowGlassSource").as_str() == Some("live") {
        "live"
    } else {
        "transparent"
    }
}

/// Every theme of the row: id, name, kind, appearance, its backgrounds and which it starts with,
/// and whether it is applied now.
pub(crate) fn themes(settings: &Map<String, Value>) -> Vec<Value> {
    let looks = looks();
    let applied = applied_id(&looks, settings);
    looks
        .iter()
        .map(|look| {
            json!({
                "id": text(look, "id"),
                "name": text(look, "name"),
                "kind": text(look, "kind"),
                "family": look.get("family"),
                "mode": text(look, "mode"),
                "art": look.get("art").and_then(Value::as_bool) == Some(true),
                "prompt": look.get("prompt"),
                "backgrounds": look.get("backgrounds"),
                "defaultBackground": look.get("defaultBackground"),
                "applied": applied.as_deref() == Some(text(look, "id")),
            })
        })
        .collect()
}

fn merge(into: &mut Map<String, Value>, from: Option<&Value>) {
    if let Some(from) = from.and_then(Value::as_object) {
        for (key, value) in from {
            into.insert(key.clone(), value.clone());
        }
    }
}

/// The changes applying theme `wanted` (its id or name, any case) with `background` (its default
/// when `None`) makes, and its name. A dark theme switches an app set to Light to Dark; a preset
/// brings its other appearance's partner along.
pub(crate) fn changes_for(
    wanted: &str,
    background: Option<&str>,
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
                "There is no theme \"{wanted}\". `ghostex theme list` shows them: {}.",
                looks
                    .iter()
                    .map(|look| text(look, "id"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let mut changes = Map::new();
    merge(&mut changes, look.get("changes"));
    merge(&mut changes, look.get("partnerChanges"));
    let offered: Vec<&str> = look["backgrounds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let background = match background.map(str::trim).filter(|value| !value.is_empty()) {
        Some(background) => {
            let background = background.to_ascii_lowercase();
            if !offered.contains(&background.as_str()) {
                return Err(format!(
                    "{} has no {background} background. It has: {}.",
                    text(look, "name"),
                    offered.join(", ")
                ));
            }
            Some(background)
        }
        None => look["defaultBackground"].as_str().map(str::to_string),
    };
    if let Some(background) = &background {
        merge(&mut changes, look["backgroundChanges"].get(background));
        merge(
            &mut changes,
            look["partnerBackgroundChanges"].get(background),
        );
    }
    if text(look, "mode") == "dark"
        && !changes.contains_key("sidebarTheme")
        && look::current(settings, "sidebarTheme").as_str() == Some("plain-light")
    {
        changes.insert("sidebarTheme".into(), json!("dark-2"));
    }
    Ok((text(look, "name").to_string(), changes))
}

/// The changes `background` makes for the theme applied now (in the appearance the app shows),
/// for switching the background after a theme.
pub(crate) fn background_changes_for(
    background: &str,
    settings: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    let looks = looks();
    let applied = applied_id(&looks, settings).ok_or_else(|| {
        "No theme is applied now, so there is no theme background to switch. Apply one with `ghostex theme apply <theme>`, or pick an artwork with `ghostex theme background set <image>`.".to_string()
    })?;
    let (_, changes) = changes_for(&applied, Some(background), settings)?;
    let look = looks.iter().find(|look| text(look, "id") == applied);
    let mut only = Map::new();
    for source in ["backgroundChanges", "partnerBackgroundChanges"] {
        merge(
            &mut only,
            look.and_then(|look| look[source].get(background.to_ascii_lowercase().as_str())),
        );
    }
    // The background alone: the theme's colours are on already.
    Ok(changes
        .into_iter()
        .filter(|(key, _)| only.contains_key(key))
        .collect())
}
