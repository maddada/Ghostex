use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::paths::GxserverPaths;

use super::{
    activate_staged_payload, apply_state_patch, catalog_zip_url, default_store_entry,
    fetch_catalog, read_manifest, read_store, stage_local_payload, stage_zip_payload,
    store_entry_for_install, validate_extension_id, write_store, ExtensionCatalogSnapshot,
    ExtensionError, ExtensionResult, ExtensionStatePatch, InstalledExtension,
};

#[derive(Clone)]
pub(crate) struct ExtensionRegistry {
    extensions_dir: PathBuf,
    store_file: PathBuf,
    gate: Arc<Mutex<()>>,
}

impl ExtensionRegistry {
    pub(crate) fn new(paths: &GxserverPaths) -> Self {
        Self {
            extensions_dir: paths.extensions_dir.clone(),
            store_file: paths.extensions_store_file.clone(),
            gate: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn list(&self) -> ExtensionResult<Vec<InstalledExtension>> {
        let _guard = self.lock()?;
        self.list_locked()
    }

    pub(crate) fn catalog(&self) -> ExtensionResult<ExtensionCatalogSnapshot> {
        let _guard = self.lock()?;
        fetch_catalog(&self.extensions_dir)
    }

    pub(crate) fn install_local(&self, source: &Path) -> ExtensionResult<InstalledExtension> {
        let _guard = self.lock()?;
        let (work_dir, staged_payload, manifest) =
            stage_local_payload(&self.extensions_dir, source)?;
        let result = self.activate_install(&staged_payload, manifest);
        let _ = fs::remove_dir_all(work_dir);
        result
    }

    pub(crate) fn install_from_catalog(&self, id: &str) -> ExtensionResult<InstalledExtension> {
        validate_extension_id(id)?;
        let _guard = self.lock()?;
        let snapshot = fetch_catalog(&self.extensions_dir)?;
        let entry = snapshot
            .catalog
            .extensions
            .iter()
            .find(|entry| entry.manifest.name == id)
            .ok_or_else(|| {
                ExtensionError::not_found(format!(
                    "Extension {id:?} was not found in the configured catalog."
                ))
            })?;
        let zip_url = catalog_zip_url(&snapshot.url, &entry.zip)?;
        self.install_zip_locked(id, &zip_url, &entry.sha256)
    }

    pub(crate) fn install_zip(
        &self,
        id: &str,
        url: &str,
        expected_sha256: &str,
    ) -> ExtensionResult<InstalledExtension> {
        let _guard = self.lock()?;
        self.install_zip_locked(id, url, expected_sha256)
    }

    pub(crate) fn uninstall(&self, id: &str) -> ExtensionResult<()> {
        validate_extension_id(id)?;
        let _guard = self.lock()?;
        let destination = self.installed_dir().join(id);
        let metadata = fs::symlink_metadata(&destination).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ExtensionError::not_found(format!("Extension {id:?} is not installed."))
            } else {
                ExtensionError::internal(format!(
                    "Could not inspect installed extension {id}: {error}"
                ))
            }
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(ExtensionError::internal(format!(
                "Installed extension path is not a normal directory: {}",
                destination.display()
            )));
        }
        let previous_store = read_store(&self.store_file)?;
        let mut updated_store = previous_store.clone();
        updated_store.remove(id);
        write_store(&self.store_file, &updated_store)?;
        fs::remove_dir_all(&destination).map_err(|error| {
            let restore_error = write_store(&self.store_file, &previous_store).err();
            ExtensionError::internal(match restore_error {
                Some(restore_error) => format!(
                    "Could not uninstall extension {id}: {error}. Restoring its state also failed: {restore_error}"
                ),
                None => format!("Could not uninstall extension {id}: {error}"),
            })
        })?;
        Ok(())
    }

    pub(crate) fn update_state(
        &self,
        id: &str,
        patch: ExtensionStatePatch,
    ) -> ExtensionResult<InstalledExtension> {
        validate_extension_id(id)?;
        let _guard = self.lock()?;
        let manifest =
            read_manifest(&self.installed_dir().join(id), Some(id)).map_err(|error| {
                if !self.installed_dir().join(id).exists() {
                    ExtensionError::not_found(format!("Extension {id:?} is not installed."))
                } else {
                    error
                }
            })?;
        let mut store = read_store(&self.store_file)?;
        let state = store
            .entry(id.to_string())
            .or_insert_with(|| default_store_entry(&manifest));
        apply_state_patch(&manifest, state, patch)?;
        let installed = InstalledExtension {
            id: id.to_string(),
            manifest,
            state: state.clone(),
        };
        write_store(&self.store_file, &store)?;
        Ok(installed)
    }

    fn install_zip_locked(
        &self,
        id: &str,
        url: &str,
        expected_sha256: &str,
    ) -> ExtensionResult<InstalledExtension> {
        let (work_dir, staged_payload, manifest) =
            stage_zip_payload(&self.extensions_dir, id, url, expected_sha256)?;
        let result = self.activate_install(&staged_payload, manifest);
        let _ = fs::remove_dir_all(work_dir);
        result
    }

    fn activate_install(
        &self,
        staged_payload: &Path,
        manifest: super::ExtensionManifest,
    ) -> ExtensionResult<InstalledExtension> {
        let id = manifest.name.clone();
        let mut store = read_store(&self.store_file)?;
        let state = store_entry_for_install(&manifest, store.get(&id));
        activate_staged_payload(&self.installed_dir(), &id, staged_payload)?;
        store.insert(id.clone(), state.clone());
        write_store(&self.store_file, &store)?;
        Ok(InstalledExtension {
            id,
            manifest,
            state,
        })
    }

    fn list_locked(&self) -> ExtensionResult<Vec<InstalledExtension>> {
        let installed_dir = self.installed_dir();
        match fs::create_dir_all(&installed_dir) {
            Ok(()) => {}
            Err(error) => {
                return Err(ExtensionError::internal(format!(
                    "Could not create installed extensions directory {}: {error}",
                    installed_dir.display()
                )));
            }
        }
        let store = read_store(&self.store_file)?;
        let mut entries = fs::read_dir(&installed_dir)
            .map_err(|error| {
                ExtensionError::internal(format!(
                    "Could not scan installed extensions directory {}: {error}",
                    installed_dir.display()
                ))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                ExtensionError::internal(format!("Could not scan installed extensions: {error}"))
            })?;
        entries.sort_by_key(|entry| entry.file_name());
        let mut installed = Vec::new();
        for entry in entries {
            let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
                ExtensionError::internal(format!(
                    "Could not inspect installed extension {}: {error}",
                    entry.path().display()
                ))
            })?;
            if metadata.file_type().is_symlink() {
                return Err(ExtensionError::internal(format!(
                    "Installed extension path may not be a symlink: {}",
                    entry.path().display()
                )));
            }
            if !metadata.is_dir() {
                continue;
            }
            let id = entry
                .file_name()
                .into_string()
                .map_err(|_| ExtensionError::internal("Installed extension id is not UTF-8."))?;
            validate_extension_id(&id)?;
            let manifest = read_manifest(&entry.path(), Some(&id))?;
            let state = store
                .get(&id)
                .cloned()
                .unwrap_or_else(|| default_store_entry(&manifest));
            installed.push(InstalledExtension {
                id,
                manifest,
                state,
            });
        }
        Ok(installed)
    }

    fn installed_dir(&self) -> PathBuf {
        self.extensions_dir.join("installed")
    }

    fn lock(&self) -> ExtensionResult<std::sync::MutexGuard<'_, ()>> {
        self.gate
            .lock()
            .map_err(|_| ExtensionError::internal("Extension registry lock was poisoned."))
    }
}
