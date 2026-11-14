use std::{collections::BTreeMap, fs, io::Write, path::Path};

use uuid::Uuid;

use super::{
    ExtensionError, ExtensionInstallApproval, ExtensionKind, ExtensionManifest, ExtensionResult,
    ExtensionStatePatch, ExtensionStoreEntry, ExtensionTerminalPlacement,
};

pub(crate) type ExtensionStore = BTreeMap<String, ExtensionStoreEntry>;

pub(crate) fn read_store(path: &Path) -> ExtensionResult<ExtensionStore> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
            ExtensionError::internal(format!(
                "Could not parse extension state store {}: {error}",
                path.display()
            ))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(ExtensionError::internal(format!(
            "Could not read extension state store {}: {error}",
            path.display()
        ))),
    }
}

pub(crate) fn write_store(path: &Path, store: &ExtensionStore) -> ExtensionResult<()> {
    let parent = path.parent().ok_or_else(|| {
        ExtensionError::internal(format!("Extension store has no parent: {}", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        ExtensionError::internal(format!(
            "Could not create extension state directory {}: {error}",
            parent.display()
        ))
    })?;
    let bytes = serde_json::to_vec_pretty(store).map_err(|error| {
        ExtensionError::internal(format!(
            "Could not serialize extension state store: {error}"
        ))
    })?;
    let temp_path = parent.join(format!(".extensions-store-{}.tmp", Uuid::new_v4()));
    let write_result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temp_path, path)
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(ExtensionError::internal(format!(
            "Could not write extension state store {}: {error}",
            path.display()
        )));
    }
    Ok(())
}

/// CDXC:Extensions 2026-10-27 DECISION:
/// User: "i enabled lazygit extension and i dont see a way to actually use it pls fix". A terminal-pane extension has no view tab, so a fresh install is pinned: its icon sits in the work area header, one click from opening. Unpinning it in Settings is kept across updates (`store_entry_for_install`).
pub(crate) fn default_store_entry(manifest: &ExtensionManifest) -> ExtensionStoreEntry {
    ExtensionStoreEntry {
        enabled: true,
        pinned: manifest.kind == Some(ExtensionKind::TerminalPane),
        chat_bar_auto_open: false,
        placement: manifest.default_placement,
        terminal_placement: ExtensionTerminalPlacement::SplitRight,
        preferences: manifest
            .preferences
            .iter()
            .filter_map(|preference| {
                preference
                    .default
                    .clone()
                    .map(|value| (preference.name.clone(), value))
            })
            .collect(),
        storage: BTreeMap::new(),
        version: manifest.version.clone(),
        // Grants come only from an install the user approved; a payload with no store entry gets none.
        granted_permissions: Vec::new(),
        sidebar_mode_hidden: false,
        sidebar_sections_hidden: false,
    }
}

/// The stored state for a payload being installed over `previous`.
///
/// CDXC:Extensions 2026-10-25 WHY:
/// Every install used to grant whatever the new payload's manifest declared, so clicking Update (which skipped the consent dialog) silently granted newly declared permissions such as exec, and a reinstall brought back permissions the user had revoked (security audit 2026-10-25). Grants now come from the approval the consent dialog sends (only permissions it listed), or, for an update without one, from the previous grants the new version still declares; a first install without an approval gets no permissions (2026-11-08, GitHub #495 follow-up: installs are the owner's alone, from Settings > Extensions, and its consent dialog always sends one).
pub(crate) fn store_entry_for_install(
    manifest: &ExtensionManifest,
    previous: Option<&ExtensionStoreEntry>,
    approval: Option<&ExtensionInstallApproval>,
) -> ExtensionStoreEntry {
    let mut entry = default_store_entry(manifest);
    entry.granted_permissions = match (approval, previous) {
        (Some(approval), _) => manifest
            .permissions
            .iter()
            .copied()
            .filter(|permission| approval.permissions.contains(permission))
            .collect(),
        (None, Some(previous)) => previous
            .granted_permissions
            .iter()
            .copied()
            .filter(|permission| manifest.permissions.contains(permission))
            .collect(),
        // An install with no consent gets no permissions (#495 follow-up): only the owner
        // installs, from Settings > Extensions, whose consent dialog sends the approval.
        (None, None) => Vec::new(),
    };
    let Some(previous) = previous else {
        return entry;
    };
    entry.enabled = previous.enabled;
    entry.pinned = previous.pinned;
    entry.chat_bar_auto_open = previous.chat_bar_auto_open;
    entry.terminal_placement = previous.terminal_placement;
    entry.storage = previous.storage.clone();
    entry.sidebar_mode_hidden = previous.sidebar_mode_hidden;
    entry.sidebar_sections_hidden = previous.sidebar_sections_hidden;
    if previous
        .placement
        .is_some_and(|placement| manifest.placements.contains(&placement))
    {
        entry.placement = previous.placement;
    }
    for preference in &manifest.preferences {
        if let Some(value) = previous.preferences.get(&preference.name) {
            entry
                .preferences
                .insert(preference.name.clone(), value.clone());
        }
    }
    entry
}

/// Refuses an install that asks for more than the user approved: with an approval, a different version, a background process or a remote website it did not show (permissions it did not tick are granted none); without one, an update that adds a background process or a new remote website (those need the consent dialog).
pub(crate) fn check_install_approval(
    manifest: &ExtensionManifest,
    previous: Option<&ExtensionManifest>,
    approval: Option<&ExtensionInstallApproval>,
) -> ExtensionResult<()> {
    let runs_command = |manifest: &ExtensionManifest| {
        manifest
            .server
            .as_ref()
            .is_some_and(|server| server.command.is_some())
    };
    let remote_url = |manifest: &ExtensionManifest| {
        manifest
            .server
            .as_ref()
            .and_then(|server| server.url.clone())
    };
    match approval {
        Some(approval) => {
            let new_url = remote_url(manifest);
            // A permission the approval leaves out is not refused, only not granted
            // (`store_entry_for_install`): the owner may tick fewer than an extension asks for
            // (Settings > Extensions > Install from folder…, 2026-11-08). A background process
            // and a remote website can't be left out, so they must have been shown.
            if approval.version != manifest.version
                || (runs_command(manifest) && !approval.background_process)
                || (new_url.is_some() && new_url != approval.remote_url)
            {
                return Err(ExtensionError::bad_request(format!(
                    "This version of {} asks for more than you approved. Review it again before installing.",
                    manifest.name
                )));
            }
        }
        None => {
            let Some(previous) = previous else {
                return Ok(());
            };
            let new_url = remote_url(manifest);
            if (runs_command(manifest) && !runs_command(previous))
                || (new_url.is_some() && new_url != remote_url(previous))
            {
                return Err(ExtensionError::bad_request(format!(
                    "This version of {} adds a background process or a remote website. Update it from Settings > Extensions so you can approve that first.",
                    manifest.name
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn apply_state_patch(
    manifest: &ExtensionManifest,
    entry: &mut ExtensionStoreEntry,
    patch: ExtensionStatePatch,
) -> ExtensionResult<()> {
    if let Some(enabled) = patch.enabled {
        entry.enabled = enabled;
    }
    if let Some(pinned) = patch.pinned {
        entry.pinned = pinned;
    }
    if let Some(chat_bar_auto_open) = patch.chat_bar_auto_open {
        entry.chat_bar_auto_open = chat_bar_auto_open;
    }
    if let Some(placement) = patch.placement {
        if !manifest.placements.contains(&placement) {
            return Err(ExtensionError::bad_request(format!(
                "Extension {} does not support the requested placement.",
                manifest.name
            )));
        }
        entry.placement = Some(placement);
    }
    if let Some(hidden) = patch.sidebar_mode_hidden {
        entry.sidebar_mode_hidden = hidden;
    }
    if let Some(hidden) = patch.sidebar_sections_hidden {
        entry.sidebar_sections_hidden = hidden;
    }
    if let Some(terminal_placement) = patch.terminal_placement {
        entry.terminal_placement = terminal_placement;
    }
    if let Some(preferences) = patch.preferences {
        let preference_names = manifest
            .preferences
            .iter()
            .map(|preference| preference.name.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for (name, value) in &preferences {
            if !preference_names.contains(name.as_str()) {
                return Err(ExtensionError::bad_request(format!(
                    "Extension {} does not declare preference {:?}.",
                    manifest.name, name
                )));
            }
            if !value.is_string() && !value.is_boolean() && !value.is_number() {
                return Err(ExtensionError::bad_request(format!(
                    "Preference {name:?} must be a string, boolean, or number."
                )));
            }
        }
        entry.preferences.extend(preferences);
    }
    if let Some(storage) = patch.storage {
        if storage
            .keys()
            .any(|key| key.trim().is_empty() || key.len() > 128)
        {
            return Err(ExtensionError::bad_request(
                "Extension storage keys must be 1-128 characters.",
            ));
        }
        entry.storage.extend(storage);
    }
    if let Some(granted_permissions) = patch.granted_permissions {
        if granted_permissions
            .iter()
            .any(|permission| !manifest.permissions.contains(permission))
        {
            return Err(ExtensionError::bad_request(format!(
                "Granted permissions for {} must be declared by its manifest.",
                manifest.name
            )));
        }
        entry.granted_permissions = granted_permissions;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(value: serde_json::Value) -> ExtensionManifest {
        serde_json::from_value(value).expect("manifest")
    }

    fn base(extra: serde_json::Value) -> serde_json::Value {
        let mut value = serde_json::json!({
            "name": "lazygit",
            "title": "Lazygit",
            "description": "Lazygit in a terminal pane.",
            "version": "0.1.0",
            "author": "Ghostex",
            "icon": "icon.svg",
            "categories": ["Developer Tools"],
        });
        for (key, field) in extra.as_object().expect("object") {
            value[key] = field.clone();
        }
        value
    }

    #[test]
    fn a_fresh_terminal_pane_install_is_pinned_to_the_header() {
        let terminal = manifest(base(serde_json::json!({
            "permissions": ["exec"],
            "kind": "terminal-pane",
            "terminal": { "command": "lazygit", "cwd": "{projectPath}", "requires": ["lazygit"] },
        })));
        assert!(store_entry_for_install(&terminal, None, None).pinned);
        let mut unpinned = store_entry_for_install(&terminal, None, None);
        unpinned.pinned = false;
        assert!(!store_entry_for_install(&terminal, Some(&unpinned), None).pinned);
    }

    #[test]
    fn a_fresh_view_install_is_not_pinned() {
        let view = manifest(base(serde_json::json!({
            "placements": ["view"],
            "defaultPlacement": "view",
            "server": { "static": "dist/" },
        })));
        assert!(!store_entry_for_install(&view, None, None).pinned);
    }
}
