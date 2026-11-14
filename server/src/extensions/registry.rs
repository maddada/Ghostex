use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::paths::GxserverPaths;

use super::{
    activate_staged_payload, apply_state_patch, catalog_zip_url, check_install_approval,
    default_store_entry, fetch_catalog, read_manifest, read_store, stage_local_payload,
    stage_zip_payload, store_entry_for_install, validate_extension_id, write_store, ExtensionBadge,
    ExtensionCatalogSnapshot, ExtensionError, ExtensionInstallApproval, ExtensionLaunchContext,
    ExtensionResult, ExtensionRuntime, ExtensionRuntimeStatus, ExtensionStatePatch,
    ExtensionStoreEntry, InstalledExtension,
};

#[derive(Clone)]
pub(crate) struct ExtensionRegistry {
    pub(super) extensions_dir: PathBuf,
    pub(super) store_file: PathBuf,
    gate: Arc<Mutex<()>>,
    pub(super) runtime: ExtensionRuntime,
    /// Sidebar and side panel documents (panel.rs).
    pub(super) panels: super::PanelStore,
    /// The home folder whose agents get an extension's skill (skills.rs).
    agent_home: PathBuf,
}

impl ExtensionRegistry {
    pub(crate) fn new_with_api_url(paths: &GxserverPaths, api_url: String) -> Self {
        let registry = Self {
            extensions_dir: paths.extensions_dir.clone(),
            store_file: paths.extensions_store_file.clone(),
            gate: Arc::new(Mutex::new(())),
            runtime: ExtensionRuntime::new(api_url),
            panels: super::PanelStore::new(&paths.extensions_dir),
            agent_home: paths.agent_config_home_dir().to_path_buf(),
        };
        // CDXC:Workarea 2026-09-22 DECISION:
        // User: remove the original Storybook extension and keep only the built-in view.
        if registry.installed_dir().join("storybook").exists() {
            if let Err(error) = registry.uninstall("storybook") {
                eprintln!("Could not retire the replaced Storybook extension: {error}");
            }
        }
        registry
    }

    pub(crate) fn list(&self) -> ExtensionResult<Vec<InstalledExtension>> {
        let _guard = self.lock()?;
        self.scan_installed()
    }

    pub(crate) fn catalog(&self) -> ExtensionResult<ExtensionCatalogSnapshot> {
        let _guard = self.lock()?;
        fetch_catalog(&self.extensions_dir)
    }

    pub(crate) fn install_local(
        &self,
        source: &Path,
        approval: Option<&ExtensionInstallApproval>,
    ) -> ExtensionResult<InstalledExtension> {
        let _guard = self.lock()?;
        let (work_dir, staged_payload, manifest) =
            stage_local_payload(&self.extensions_dir, source)?;
        let result = self.activate_install(&staged_payload, manifest, approval);
        let _ = fs::remove_dir_all(work_dir);
        result
    }

    pub(crate) fn install_from_catalog(
        &self,
        id: &str,
        approval: Option<&ExtensionInstallApproval>,
    ) -> ExtensionResult<InstalledExtension> {
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
        self.install_zip_locked(id, &zip_url, &entry.sha256, approval)
    }

    pub(crate) fn install_zip(
        &self,
        id: &str,
        url: &str,
        expected_sha256: &str,
        approval: Option<&ExtensionInstallApproval>,
    ) -> ExtensionResult<InstalledExtension> {
        let _guard = self.lock()?;
        self.install_zip_locked(id, url, expected_sha256, approval)
    }

    pub(crate) fn uninstall(&self, id: &str) -> ExtensionResult<()> {
        validate_extension_id(id)?;
        self.runtime.stop(id)?;
        self.runtime.forget_page_token(id);
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
        if let Some(name) = previous_store
            .get(id)
            .and_then(|state| state.skill_installed.as_deref())
        {
            super::skills::remove_skill(&self.agent_home, id, name);
        }
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
        self.panels.remove_extension(id)?;
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
        // A payload that was never installed through Ghostex gets no store entry here either, so
        // it can't be turned into one `start` would run (see `start`).
        let state = store.get_mut(id).ok_or_else(|| {
            ExtensionError::bad_request(format!(
                "Extension {id:?} wasn't installed through Ghostex. Install it in Settings > Extensions."
            ))
        })?;
        apply_state_patch(&manifest, state, patch)?;
        if !state.enabled {
            self.runtime.stop(id)?;
            self.runtime.forget_page_token(id);
        }
        let installed = InstalledExtension {
            runtime: self.runtime_status(id, &manifest, state),
            badge: self.runtime.badge(id),
            id: id.to_string(),
            manifest,
            state: state.clone(),
            skill_install: None,
        };
        write_store(&self.store_file, &store)?;
        Ok(installed)
    }

    /// The skill side of an install: a first install puts the skill in when the user asked for
    /// it, an update refreshes the one the user installed (removing it if the new version ships
    /// none or renamed it), and nothing replaces a skill this extension didn't put there.
    /// Returns what happened, for the install card.
    fn settle_install_skill(
        &self,
        id: &str,
        manifest: &super::ExtensionManifest,
        state: &mut ExtensionStoreEntry,
        approval: Option<&ExtensionInstallApproval>,
    ) -> Option<serde_json::Value> {
        let previous = state.skill_installed.take();
        let asked = approval.is_some_and(|approval| approval.skill);
        let Some(skill) = &manifest.skill else {
            if let Some(previous) = previous {
                super::skills::remove_skill(&self.agent_home, id, &previous);
            }
            return None;
        };
        if let Some(previous) = previous
            .as_deref()
            .filter(|previous| *previous != skill.name)
        {
            super::skills::remove_skill(&self.agent_home, id, previous);
        }
        if previous.is_none() && !asked {
            return None;
        }
        let payload_dir = self.installed_dir().join(id);
        let outcome = super::skills::install_skill(&self.agent_home, id, &payload_dir, skill);
        if outcome.is_ok() {
            state.skill_installed = Some(skill.name.clone());
        }
        Some(super::skills::skill_report(
            &skill.name,
            &outcome.map(|()| true),
        ))
    }

    fn install_zip_locked(
        &self,
        id: &str,
        url: &str,
        expected_sha256: &str,
        approval: Option<&ExtensionInstallApproval>,
    ) -> ExtensionResult<InstalledExtension> {
        let (work_dir, staged_payload, manifest) =
            stage_zip_payload(&self.extensions_dir, id, url, expected_sha256)?;
        let result = self.activate_install(&staged_payload, manifest, approval);
        let _ = fs::remove_dir_all(work_dir);
        result
    }

    fn activate_install(
        &self,
        staged_payload: &Path,
        manifest: super::ExtensionManifest,
        approval: Option<&ExtensionInstallApproval>,
    ) -> ExtensionResult<InstalledExtension> {
        let id = manifest.name.clone();
        if id == "storybook" {
            return Err(ExtensionError::bad_request(
                "Storybook is now a built-in view. Open it from the project view picker.",
            ));
        }
        let previous_manifest = read_manifest(&self.installed_dir().join(&id), Some(&id)).ok();
        check_install_approval(&manifest, previous_manifest.as_ref(), approval)?;
        self.runtime.stop(&id)?;
        self.runtime.forget_page_token(&id);
        let mut store = read_store(&self.store_file)?;
        let mut state = store_entry_for_install(&manifest, store.get(&id), approval);
        activate_staged_payload(&self.installed_dir(), &id, staged_payload)?;
        let skill_install = self.settle_install_skill(&id, &manifest, &mut state, approval);
        store.insert(id.clone(), state.clone());
        write_store(&self.store_file, &store)?;
        Ok(InstalledExtension {
            runtime: self.runtime_status(&id, &manifest, &state),
            badge: self.runtime.badge(&id),
            skill_install,
            id,
            manifest,
            state,
        })
    }

    /// The listing itself. Callers that must not wait behind an install (it holds the gate through
    /// its download) call it without the gate: every write it could see is an atomic rename.
    pub(super) fn scan_installed(&self) -> ExtensionResult<Vec<InstalledExtension>> {
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
            if id == "storybook" {
                continue;
            }
            let manifest = read_manifest(&entry.path(), Some(&id))?;
            // A payload never installed through Ghostex lists as off: it can't start or be
            // served (see `start`), only uninstalled.
            let state = store
                .get(&id)
                .cloned()
                .unwrap_or_else(|| ExtensionStoreEntry {
                    enabled: false,
                    ..default_store_entry(&manifest)
                });
            installed.push(InstalledExtension {
                runtime: self.runtime_status(&id, &manifest, &state),
                badge: self.runtime.badge(&id),
                skill_install: None,
                id,
                manifest,
                state,
            });
        }
        Ok(installed)
    }

    pub(super) fn installed_dir(&self) -> PathBuf {
        self.extensions_dir.join("installed")
    }

    pub(crate) fn static_root(&self, id: &str) -> ExtensionResult<PathBuf> {
        validate_extension_id(id)?;
        let payload_dir = self.installed_dir().join(id);
        let manifest = read_manifest(&payload_dir, Some(id)).map_err(|error| {
            if payload_dir.exists() {
                error
            } else {
                ExtensionError::not_found(format!("Extension {id:?} is not installed."))
            }
        })?;
        // Never served for a payload that wasn't installed through Ghostex (see `start`).
        let state = read_store(&self.store_file)?
            .get(id)
            .cloned()
            .ok_or_else(|| {
                ExtensionError::not_found(format!(
                    "Extension {id:?} wasn't installed through Ghostex."
                ))
            })?;
        if !state.enabled {
            return Err(ExtensionError::not_found(format!(
                "Extension {id:?} is disabled."
            )));
        }
        let static_dir = manifest
            .server
            .and_then(|server| server.static_dir)
            .ok_or_else(|| {
                ExtensionError::not_found(format!("Extension {id:?} is not a static extension."))
            })?;
        let canonical_payload = fs::canonicalize(&payload_dir).map_err(|error| {
            ExtensionError::internal(format!(
                "Could not resolve installed extension {id}: {error}"
            ))
        })?;
        let canonical_static = fs::canonicalize(payload_dir.join(static_dir)).map_err(|error| {
            ExtensionError::internal(format!(
                "Could not resolve static files for extension {id}: {error}"
            ))
        })?;
        if !canonical_static.starts_with(&canonical_payload) || !canonical_static.is_dir() {
            return Err(ExtensionError::bad_request(format!(
                "Static files for extension {id:?} must stay inside its installed payload."
            )));
        }
        Ok(canonical_static)
    }

    pub(crate) fn static_icon_path(
        &self,
        id: &str,
        requested_relative: &Path,
    ) -> ExtensionResult<Option<PathBuf>> {
        validate_extension_id(id)?;
        let payload_dir = self.installed_dir().join(id);
        let manifest = read_manifest(&payload_dir, Some(id)).map_err(|error| {
            if payload_dir.exists() {
                error
            } else {
                ExtensionError::not_found(format!("Extension {id:?} is not installed."))
            }
        })?;
        // The extension's icon, and a sidebar extension's mode and side panel icons.
        let icons = std::iter::once(manifest.icon.clone())
            .chain(
                manifest
                    .sidebar
                    .as_ref()
                    .and_then(|sidebar| sidebar.mode.as_ref())
                    .and_then(|mode| mode.icon.clone()),
            )
            .chain(
                manifest
                    .side_panel
                    .as_ref()
                    .and_then(|panel| panel.icon.clone()),
            )
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let Some(manifest_icon) = icons.into_iter().find(|icon| requested_relative == icon) else {
            return Ok(None);
        };
        let canonical_payload = fs::canonicalize(&payload_dir).map_err(|error| {
            ExtensionError::internal(format!(
                "Could not resolve installed extension {id}: {error}"
            ))
        })?;
        let canonical_icon =
            fs::canonicalize(payload_dir.join(manifest_icon)).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    ExtensionError::not_found(format!("Extension {id:?} icon was not found."))
                } else {
                    ExtensionError::internal(format!(
                        "Could not resolve installed extension {id} icon: {error}"
                    ))
                }
            })?;
        if !canonical_icon.starts_with(&canonical_payload) || !canonical_icon.is_file() {
            return Err(ExtensionError::bad_request(format!(
                "Icon for extension {id:?} must stay inside its installed payload."
            )));
        }
        Ok(Some(canonical_icon))
    }

    pub(crate) fn start(
        &self,
        id: &str,
        context: ExtensionLaunchContext,
    ) -> ExtensionResult<ExtensionRuntimeStatus> {
        validate_extension_id(id)?;
        if id == "storybook" {
            return Err(ExtensionError::bad_request(
                "Storybook is now a built-in view.",
            ));
        }
        let _guard = self.lock()?;
        let payload_dir = self.installed_dir().join(id);
        let manifest = read_manifest(&payload_dir, Some(id)).map_err(|error| {
            if payload_dir.exists() {
                error
            } else {
                ExtensionError::not_found(format!("Extension {id:?} is not installed."))
            }
        })?;
        // CDXC:Extensions 2026-11-08 DECISION:
        // GitHub #495 follow-up (Q1860, "settings only"): only an extension installed through
        // Ghostex has a store entry, so a payload put into the extensions folder some other way is
        // never started, whoever asks. The owner installs it from Settings > Extensions.
        let state = read_store(&self.store_file)?.get(id).cloned().ok_or_else(|| {
            ExtensionError::bad_request(format!(
                "Extension {id:?} wasn't installed through Ghostex, so it can't start. Install it in Settings > Extensions."
            ))
        })?;
        if !state.enabled {
            return Err(ExtensionError::bad_request(format!(
                "Extension {id:?} is disabled."
            )));
        }
        self.runtime.start(&payload_dir, &manifest, context)
    }

    pub(crate) fn stop(&self, id: &str) -> ExtensionResult<ExtensionRuntimeStatus> {
        validate_extension_id(id)?;
        self.runtime.stop(id)
    }

    pub(crate) fn status(&self, id: &str) -> ExtensionResult<ExtensionRuntimeStatus> {
        validate_extension_id(id)?;
        let payload_dir = self.installed_dir().join(id);
        let manifest = read_manifest(&payload_dir, Some(id)).map_err(|error| {
            if payload_dir.exists() {
                error
            } else {
                ExtensionError::not_found(format!("Extension {id:?} is not installed."))
            }
        })?;
        let state = read_store(&self.store_file)?
            .get(id)
            .cloned()
            .unwrap_or_else(|| default_store_entry(&manifest));
        Ok(self.runtime_status(id, &manifest, &state))
    }

    pub(crate) fn set_badge(
        &self,
        id: &str,
        lines: Vec<String>,
    ) -> ExtensionResult<ExtensionBadge> {
        validate_extension_id(id)?;
        if !self.installed_dir().join(id).is_dir() {
            return Err(ExtensionError::not_found(format!(
                "Extension {id:?} is not installed."
            )));
        }
        self.runtime.set_badge(id, lines)
    }

    pub(crate) fn authorize_api_token(
        &self,
        headers: &axum::http::HeaderMap,
        endpoint: &str,
    ) -> Option<String> {
        self.runtime.authorize(headers, endpoint)
    }

    pub(crate) fn stop_all(&self) {
        self.runtime.stop_all();
    }

    fn runtime_status(
        &self,
        id: &str,
        manifest: &super::ExtensionManifest,
        state: &super::ExtensionStoreEntry,
    ) -> ExtensionRuntimeStatus {
        if state.enabled {
            self.runtime.status(id, manifest)
        } else {
            ExtensionRuntimeStatus::stopped()
        }
    }

    pub(super) fn lock(&self) -> ExtensionResult<std::sync::MutexGuard<'_, ()>> {
        self.gate
            .lock()
            .map_err(|_| ExtensionError::internal("Extension registry lock was poisoned."))
    }
}
