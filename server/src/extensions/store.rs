use std::{collections::BTreeMap, fs, io::Write, path::Path};

use uuid::Uuid;

use super::{
    ExtensionError, ExtensionManifest, ExtensionResult, ExtensionStatePatch, ExtensionStoreEntry,
    ExtensionTerminalPlacement,
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

pub(crate) fn default_store_entry(manifest: &ExtensionManifest) -> ExtensionStoreEntry {
    ExtensionStoreEntry {
        enabled: true,
        pinned: false,
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
        version: manifest.version.clone(),
        granted_permissions: manifest.permissions.clone(),
    }
}

pub(crate) fn store_entry_for_install(
    manifest: &ExtensionManifest,
    previous: Option<&ExtensionStoreEntry>,
) -> ExtensionStoreEntry {
    let mut entry = default_store_entry(manifest);
    let Some(previous) = previous else {
        return entry;
    };
    entry.enabled = previous.enabled;
    entry.pinned = previous.pinned;
    entry.terminal_placement = previous.terminal_placement;
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
    if let Some(placement) = patch.placement {
        if !manifest.placements.contains(&placement) {
            return Err(ExtensionError::bad_request(format!(
                "Extension {} does not support the requested placement.",
                manifest.name
            )));
        }
        entry.placement = Some(placement);
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
