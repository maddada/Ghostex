use std::{collections::HashSet, fs, path::Path};

use serde_json::{Map, Value};

/// CDXC:Theming 2026-10-14 DECISION:
/// User: separate Ghostex's Ghostty config from the Ghostty app's, importing "fonts and a few things like that from Ghostty itself, but not the theme". Supersedes the 2026-09-14 decision to show the Ghostty app's theme: the palette selectors now follow the theme set in Ghostex's own terminal config, and the catalog defaults apply when it sets none (Ghostex Kanagawa Dragon in dark mode since 2026-11-20, GitHub Light Default in light mode).
/// The config's theme owns the dark palette setting; an explicit app light palette overrides its light selection.
pub(super) fn hydrate_ghostty_theme_selections(object: &mut Map<String, Value>) {
    let path = super::ghostex_terminal_config_path();
    let Some(theme) =
        configured_theme(&path, &mut HashSet::new()).filter(|theme| !theme.is_empty())
    else {
        return;
    };
    let mut light = None;
    let mut dark = None;
    for part in theme.split(',') {
        let part = part.trim();
        if let Some(name) = part.strip_prefix("light:") {
            light = Some(name.trim());
        } else if let Some(name) = part.strip_prefix("dark:") {
            dark = Some(name.trim());
        }
    }
    if light.is_none() && dark.is_none() {
        light = Some(theme.as_str());
        dark = Some(theme.as_str());
    }
    if let Some(dark) = dark.filter(|name| !name.is_empty()) {
        object.insert("terminalGhosttyTheme".into(), Value::String(dark.into()));
    }
    if object
        .get("terminalGhosttyLightTheme")
        .and_then(Value::as_str)
        .is_none_or(|name| name.is_empty())
        && let Some(light) = light.filter(|name| !name.is_empty())
    {
        object.insert(
            "terminalGhosttyLightTheme".into(),
            Value::String(light.into()),
        );
    }
}

fn configured_theme(path: &Path, visiting: &mut HashSet<std::path::PathBuf>) -> Option<String> {
    let path = fs::canonicalize(path).ok()?;
    if !visiting.insert(path.clone()) {
        return None;
    }
    let result = (|| {
        let source = fs::read_to_string(&path).ok()?;
        let mut theme = None;
        let mut includes = Vec::new();
        for line in source.lines().map(str::trim) {
            if line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(value);
            match key.trim() {
                "theme" => theme = Some(value.to_string()),
                "config-file" if value.is_empty() => includes.clear(),
                "config-file" => {
                    let value = value.strip_prefix('?').unwrap_or(value);
                    let include = if let Some(relative) = value.strip_prefix("~/") {
                        std::path::PathBuf::from(super::user_home_dir()?).join(relative)
                    } else {
                        path.parent()?.join(value)
                    };
                    includes.push(include);
                }
                _ => {}
            }
        }
        // Ghostty loads recursive config files after the containing file.
        for include in includes {
            if let Some(included) = configured_theme(&include, visiting) {
                theme = Some(included);
            }
        }
        theme
    })();
    visiting.remove(&path);
    result
}
