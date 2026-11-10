use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

use super::*;

pub struct ComponentStore {
    manifest: OnDemandManifest,
    root: PathBuf,
}

struct ScopedPathCleanup {
    paths: Vec<PathBuf>,
}

impl ScopedPathCleanup {
    fn new(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            paths: paths.into_iter().collect(),
        }
    }
}

impl Drop for ScopedPathCleanup {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = remove_component_store_path(path);
        }
    }
}

struct AtomicPathReplacement {
    backup: PathBuf,
    destination: PathBuf,
    previous_moved: bool,
    replacement_installed: bool,
    committed: bool,
}

impl AtomicPathReplacement {
    fn prepare(destination: &Path, backup: &Path) -> Result<Self, String> {
        let mut replacement = Self {
            backup: backup.to_path_buf(),
            destination: destination.to_path_buf(),
            previous_moved: false,
            replacement_installed: false,
            committed: false,
        };
        match destination.symlink_metadata() {
            Ok(_) => {
                fs::rename(destination, backup).map_err(|error| {
                    format!(
                        "Could not preserve existing component store path {}: {error}",
                        destination.display()
                    )
                })?;
                replacement.previous_moved = true;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Could not inspect existing component store path {}: {error}",
                    destination.display()
                ));
            }
        }
        Ok(replacement)
    }

    fn install(&mut self, staged: &Path) -> Result<(), String> {
        fs::rename(staged, &self.destination).map_err(|error| {
            format!(
                "Could not atomically install component store path {}: {error}",
                self.destination.display()
            )
        })?;
        self.replacement_installed = true;
        Ok(())
    }

    fn commit(mut self) -> Result<(), String> {
        if self.previous_moved {
            remove_component_store_path(&self.backup).map_err(|error| {
                format!(
                    "Could not remove replaced component store path {}: {error}",
                    self.backup.display()
                )
            })?;
            self.previous_moved = false;
        }
        self.committed = true;
        Ok(())
    }
}

impl Drop for AtomicPathReplacement {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if self.replacement_installed {
            let _ = remove_component_store_path(&self.destination);
        }
        if self.previous_moved {
            let _ = fs::rename(&self.backup, &self.destination);
        }
    }
}

fn remove_component_store_path(path: &Path) -> io::Result<()> {
    match path.symlink_metadata() {
        Ok(metadata) if metadata.file_type().is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn path_size_bytes(path: &Path) -> Result<u64, String> {
    if path.is_file() {
        return path
            .metadata()
            .map(|metadata| metadata.len())
            .map_err(|error| format!("Could not read component file size: {error}"));
    }
    directory_size(path)
}

impl ComponentStore {
    pub fn from_manifest(manifest: OnDemandManifest) -> Result<Self, String> {
        let root = component_store_root()?;
        prune_other_versions(&legacy_asset_cache_root()?, &manifest.version)?;
        Ok(Self { manifest, root })
    }

    #[allow(dead_code)] // component-store query builder API kept complete
    pub fn with_root(manifest: OnDemandManifest, root: PathBuf) -> Self {
        Self { manifest, root }
    }

    pub fn component(&self, name: &str) -> Option<&ComponentDefinition> {
        self.manifest.components.get(name)
    }

    /// The folder names of `name`'s installs other than the version this release seals: what an app
    /// update left behind until the component installs again (Settings > Updates,
    /// app/updates/items.rs). On Windows code-server's folders are hashes, not versions.
    pub fn other_installed_versions(&self, name: &str) -> Vec<String> {
        let Some(component) = self.manifest.components.get(name) else {
            return Vec::new();
        };
        let current = component_version_dir_name(&component.name, &component.component_version);
        let Ok(entries) = fs::read_dir(self.root.join(&component.name)) else {
            return Vec::new();
        };
        let mut versions: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|folder| !folder.starts_with('.') && *folder != current)
            .collect();
        versions.sort();
        versions
    }

    /// An install of `name` for `platform` from a version other than the one this release seals:
    /// what the user installed before an app update brought a new version. One whose archive is
    /// the sealed one (same SHA-256) comes first, else the most recently installed.
    pub fn previous_install_for_platform(
        &self,
        name: &str,
        platform: &str,
    ) -> Option<PreviousInstall> {
        let component = self.manifest.components.get(name)?;
        let current = component_version_dir_name(&component.name, &component.component_version);
        let sealed_sha256 = component
            .platforms
            .get(platform)
            .map(|asset| asset.sha256.as_str());
        let mut found: Vec<(bool, std::time::SystemTime, PreviousInstall)> =
            fs::read_dir(self.root.join(&component.name))
                .ok()?
                .flatten()
                .filter(|entry| {
                    entry.path().is_dir()
                        && entry
                            .file_name()
                            .to_str()
                            .is_some_and(|folder| !folder.starts_with('.') && folder != current)
                })
                .filter_map(|entry| {
                    let path = entry.path().join(platform);
                    let marker = read_install_marker(&path)?;
                    (marker.name == component.name
                        && marker.platform == platform
                        && marker.version != component.component_version
                        && require_identifier(&marker.version, "component version").is_ok())
                    .then_some(())?;
                    let installed_at = path
                        .join(".ghostex-component.json")
                        .metadata()
                        .and_then(|metadata| metadata.modified())
                        .unwrap_or(std::time::UNIX_EPOCH);
                    Some((
                        sealed_sha256 == Some(marker.sha256.as_str()),
                        installed_at,
                        PreviousInstall {
                            version: marker.version,
                            sha256: marker.sha256,
                            path,
                        },
                    ))
                })
                .collect();
        found.sort_by_key(|(same_archive, installed_at, _)| (*same_archive, *installed_at));
        found.pop().map(|(_, _, previous)| previous)
    }

    /// CDXC:Release 2026-11-10 WHY:
    /// A component's version re-keys whenever any of its platforms' inputs change, so an app update can seal a new version whose archive for this platform is byte for byte the one already installed (GitHub #530: 11.0.0 re-keyed code-server for a Windows-only script change and every Linux archive stayed identical). The installed folder is moved under the new version instead of downloading the same bytes again. Returns `None` when there is nothing to adopt or the move failed; the caller then downloads as usual.
    fn adopt_previous_install(
        &self,
        component: &ComponentDefinition,
        platform: &str,
        asset: &ComponentPlatformAsset,
        version_root: &Path,
    ) -> Option<InstalledComponent> {
        let previous = self
            .previous_install_for_platform(&component.name, platform)
            .filter(|previous| previous.sha256 == asset.sha256)?;
        fs::create_dir_all(version_root).ok()?;
        let unique = unique_suffix();
        let staged = version_root.join(format!(".install-{}-{unique}", std::process::id()));
        let backup = version_root.join(format!(".previous-{}-{unique}", std::process::id()));
        let _cleanup = ScopedPathCleanup::new([staged.clone(), backup.clone()]);
        fs::rename(&previous.path, &staged).ok()?;
        if component.name == CODE_SERVER_COMPONENT_NAME && platform.starts_with("windows-") {
            verify_installed_windows_code_server(&staged, &component.component_version, platform)
                .ok()?;
        }
        write_install_marker(
            &staged,
            &component.name,
            &component.component_version,
            platform,
            &asset.sha256,
        )
        .ok()?;
        let mut replacement =
            AtomicPathReplacement::prepare(&version_root.join(platform), &backup).ok()?;
        replacement.install(&staged).ok()?;
        let installed = self
            .query_for_platform(&component.name, &component.component_version, platform)
            .ok()
            .filter(|installed| installed.installed)?;
        replacement.commit().ok()?;
        Some(installed)
    }

    #[allow(dead_code)] // component-store query builder API kept complete
    pub fn query(&self, name: &str, version: &str) -> Result<InstalledComponent, String> {
        let platform = current_platform()?;
        self.query_for_platform(name, version, &platform)
    }

    pub fn query_for_platform(
        &self,
        name: &str,
        version: &str,
        platform: &str,
    ) -> Result<InstalledComponent, String> {
        require_identifier(name, "component name")?;
        require_identifier(version, "component version")?;
        require_identifier(platform, "component platform")?;
        let path = self
            .root
            .join(name)
            .join(component_version_dir_name(name, version))
            .join(platform);
        let expected_sha256 = self
            .manifest
            .components
            .get(name)
            .filter(|component| component.component_version == version)
            .and_then(|component| component.platforms.get(platform))
            .map(|asset| asset.sha256.as_str());
        let installed = installed_marker_matches(&path, name, version, platform, expected_sha256)?;
        if installed && name == CODE_SERVER_COMPONENT_NAME && platform.starts_with("windows-") {
            verify_installed_windows_code_server(&path, version, platform)?;
        }
        let size_bytes = if installed { directory_size(&path)? } else { 0 };
        Ok(InstalledComponent {
            installed,
            name: name.to_string(),
            version: version.to_string(),
            platform: platform.to_string(),
            path,
            size_bytes,
        })
    }

    pub fn query_current(&self, name: &str) -> Result<InstalledComponent, String> {
        let platform = current_platform()?;
        self.query_current_for_platform(name, &platform)
    }

    pub fn query_current_for_platform(
        &self,
        name: &str,
        platform: &str,
    ) -> Result<InstalledComponent, String> {
        require_identifier(platform, "component platform")?;
        let component = self
            .manifest
            .components
            .get(name)
            .ok_or_else(|| format!("Sealed manifest does not define component {name}"))?;
        if !component.platforms.contains_key(platform) {
            return Err(format!(
                "Sealed manifest does not define {} {} for {platform}",
                component.name, component.component_version
            ));
        }
        self.query_for_platform(&component.name, &component.component_version, platform)
    }

    /// The current version's folder on this platform when its install marker matches, without
    /// verifying the payload again: for a caller that verifies the files before it unpacks them.
    pub fn current_marked_install(&self, name: &str) -> Result<Option<(String, PathBuf)>, String> {
        let platform = current_platform()?;
        let component = self
            .manifest
            .components
            .get(name)
            .ok_or_else(|| format!("Sealed manifest does not define component {name}"))?;
        let asset = component.platforms.get(&platform).ok_or_else(|| {
            format!(
                "Sealed manifest does not define {} {} for {platform}",
                component.name, component.component_version
            )
        })?;
        require_identifier(&component.name, "component name")?;
        require_identifier(&component.component_version, "component version")?;
        require_identifier(&platform, "component platform")?;
        let path = self
            .root
            .join(&component.name)
            .join(component_version_dir_name(
                &component.name,
                &component.component_version,
            ))
            .join(&platform);
        let installed = installed_marker_matches(
            &path,
            &component.name,
            &component.component_version,
            &platform,
            Some(asset.sha256.as_str()),
        )?;
        Ok(installed.then(|| (component.component_version.clone(), path)))
    }

    pub fn install(
        &self,
        name: &str,
        progress: &mut dyn FnMut(ComponentStoreProgress),
    ) -> Result<InstalledComponent, String> {
        let platform = current_platform()?;
        self.install_for_platform(name, &platform, progress)
    }

    pub fn install_for_platform(
        &self,
        name: &str,
        platform: &str,
        progress: &mut dyn FnMut(ComponentStoreProgress),
    ) -> Result<InstalledComponent, String> {
        require_identifier(platform, "component platform")?;
        let component = self
            .manifest
            .components
            .get(name)
            .ok_or_else(|| format!("Sealed manifest does not define component {name}"))?;
        let asset = component.platforms.get(platform).ok_or_else(|| {
            format!(
                "Sealed manifest does not define {} {} for {platform}",
                component.name, component.component_version
            )
        })?;
        let component_root = self.root.join(&component.name);
        let version_dir_name =
            component_version_dir_name(&component.name, &component.component_version);
        let version_root = component_root.join(&version_dir_name);
        emit(
            progress,
            component,
            platform,
            asset.size_bytes,
            ComponentStoreProgressPhase::Checking,
        );
        let current =
            self.query_for_platform(&component.name, &component.component_version, platform)?;
        if current.installed {
            prune_temporary_install_artifacts(&version_root);
            prune_other_versions(&component_root, &version_dir_name)?;
            emit(
                progress,
                component,
                platform,
                asset.size_bytes,
                ComponentStoreProgressPhase::Ready,
            );
            return Ok(current);
        }
        if let Some(adopted) =
            self.adopt_previous_install(component, platform, asset, &version_root)
        {
            prune_temporary_install_artifacts(&version_root);
            prune_other_versions(&component_root, &version_dir_name)?;
            emit(
                progress,
                component,
                platform,
                asset.size_bytes,
                ComponentStoreProgressPhase::Ready,
            );
            return Ok(adopted);
        }

        fs::create_dir_all(&version_root).map_err(|error| {
            format!(
                "Could not create component store directory {}: {error}",
                version_root.display()
            )
        })?;
        prune_temporary_install_artifacts(&version_root);
        let unique = unique_suffix();
        let archive_path = version_root.join(format!(".download-{}-{unique}", std::process::id()));
        let sidecar_path =
            version_root.join(format!(".download-{}-{unique}.sha256", std::process::id()));
        let install_path = version_root.join(format!(".install-{}-{unique}", std::process::id()));
        let destination = version_root.join(platform);
        let previous_path = version_root.join(format!(".previous-{}-{unique}", std::process::id()));
        let _cleanup = ScopedPathCleanup::new([
            archive_path.clone(),
            sidecar_path.clone(),
            install_path.clone(),
            previous_path.clone(),
        ]);
        let url = download_url(
            component.download_repo(&self.manifest.github_repo),
            &component.download_tag,
            &asset.asset_name,
        );

        emit(
            progress,
            component,
            platform,
            asset.size_bytes,
            ComponentStoreProgressPhase::Downloading,
        );
        download(
            &url,
            &archive_path,
            asset.size_bytes,
            &mut |downloaded_bytes| {
                emit_download_progress(
                    progress,
                    component,
                    platform,
                    asset.size_bytes,
                    downloaded_bytes,
                );
            },
        )?;
        emit(
            progress,
            component,
            platform,
            asset.size_bytes,
            ComponentStoreProgressPhase::Verifying,
        );
        verify_file(&archive_path, &asset.sha256, asset.size_bytes)?;
        if let Some(sidecar_name) = &asset.sha256_sidecar_name {
            let sidecar_url = download_url(
                component.download_repo(&self.manifest.github_repo),
                &component.download_tag,
                sidecar_name,
            );
            download(&sidecar_url, &sidecar_path, 0, &mut |_| {})?;
            let sidecar = fs::read_to_string(&sidecar_path).map_err(|error| {
                format!(
                    "Could not read downloaded component checksum sidecar {}: {error}",
                    sidecar_path.display()
                )
            })?;
            let sidecar_sha256 = parse_code_server_checksum_sidecar(&sidecar, &asset.asset_name)?;
            if sidecar_sha256 != asset.sha256 {
                return Err(format!(
                    "Component checksum sidecar digest mismatch for {}",
                    asset.asset_name
                ));
            }
        }
        // Ghostex's signature, before the quarantine flag goes and before anything is unpacked.
        super::signature::verify_component_signature(component, platform, asset)?;
        remove_macos_quarantine(&archive_path)?;

        emit(
            progress,
            component,
            platform,
            asset.size_bytes,
            ComponentStoreProgressPhase::Installing,
        );
        fs::create_dir_all(&install_path)
            .map_err(|error| format!("Could not prepare atomic component install: {error}"))?;
        unpack_tar_gz(&archive_path, &install_path)?;
        remove_macos_quarantine(&install_path)?;
        if component.name == CODE_SERVER_COMPONENT_NAME && platform.starts_with("windows-") {
            verify_installed_windows_code_server(
                &install_path,
                &component.component_version,
                platform,
            )?;
        }
        write_install_marker(
            &install_path,
            &component.name,
            &component.component_version,
            platform,
            &asset.sha256,
        )?;
        let mut replacement = AtomicPathReplacement::prepare(&destination, &previous_path)?;
        replacement.install(&install_path)?;

        let installed =
            self.query_for_platform(&component.name, &component.component_version, platform)?;
        replacement.commit()?;

        /*
        CDXC:Extensions 2026-08-09:
        A process terminated after downloading or unpacking cannot run its
        normal error cleanup. Once this version has been installed atomically,
        every remaining .download-* or .install-* sibling is obsolete; remove
        those artifacts so a killed first launch does not permanently retain a
        full component archive.
        */
        prune_temporary_install_artifacts(&version_root);

        emit(
            progress,
            component,
            platform,
            asset.size_bytes,
            ComponentStoreProgressPhase::Pruning,
        );
        prune_other_versions(&component_root, &version_dir_name)?;
        emit(
            progress,
            component,
            platform,
            asset.size_bytes,
            ComponentStoreProgressPhase::Ready,
        );
        Ok(installed)
    }

    pub fn uninstall(&self, name: &str, version: &str) -> Result<bool, String> {
        require_identifier(name, "component name")?;
        require_identifier(version, "component version")?;
        let version_path = self
            .root
            .join(name)
            .join(component_version_dir_name(name, version));
        if !version_path.exists() {
            return Ok(false);
        }
        fs::remove_dir_all(&version_path)
            .map_err(|error| format!("Could not uninstall component {name} {version}: {error}"))?;
        Ok(true)
    }

    pub fn download_release_asset(
        &self,
        asset_key: &str,
        progress: &mut dyn FnMut(ComponentStoreProgress),
    ) -> Result<PathBuf, String> {
        let asset =
            self.manifest.assets.get(asset_key).ok_or_else(|| {
                format!("Sealed manifest does not define release asset {asset_key}")
            })?;
        let cache_dir = legacy_asset_cache_root()?.join(&self.manifest.version);
        fs::create_dir_all(&cache_dir).map_err(|error| {
            format!(
                "Could not create release asset cache {}: {error}",
                cache_dir.display()
            )
        })?;
        prune_temporary_install_artifacts(&cache_dir);
        let destination = cache_dir.join(&asset.name);
        let compatibility_component = ComponentDefinition {
            name: asset_key.to_string(),
            component_version: self.manifest.version.clone(),
            download_tag: format!("v{}", self.manifest.version),
            github_repo: None,
            platforms: HashMap::new(),
        };
        let platform = current_platform()?;
        emit(
            progress,
            &compatibility_component,
            &platform,
            asset.bytes,
            ComponentStoreProgressPhase::Checking,
        );
        if destination.is_file() && verify_file(&destination, &asset.sha256, asset.bytes).is_ok() {
            emit(
                progress,
                &compatibility_component,
                &platform,
                asset.bytes,
                ComponentStoreProgressPhase::Ready,
            );
            return Ok(destination);
        }
        let temporary = cache_dir.join(format!(
            ".download-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        let previous = cache_dir.join(format!(
            ".previous-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        let _cleanup = ScopedPathCleanup::new([temporary.clone(), previous.clone()]);
        emit(
            progress,
            &compatibility_component,
            &platform,
            asset.bytes,
            ComponentStoreProgressPhase::Downloading,
        );
        let url = download_url(
            &self.manifest.github_repo,
            &compatibility_component.download_tag,
            &asset.name,
        );
        download(&url, &temporary, asset.bytes, &mut |downloaded_bytes| {
            emit_download_progress(
                progress,
                &compatibility_component,
                &platform,
                asset.bytes,
                downloaded_bytes,
            );
        })?;
        emit(
            progress,
            &compatibility_component,
            &platform,
            asset.bytes,
            ComponentStoreProgressPhase::Verifying,
        );
        verify_file(&temporary, &asset.sha256, asset.bytes)?;
        remove_macos_quarantine(&temporary)?;
        let mut replacement = AtomicPathReplacement::prepare(&destination, &previous)?;
        replacement.install(&temporary)?;
        replacement.commit()?;
        emit(
            progress,
            &compatibility_component,
            &platform,
            asset.bytes,
            ComponentStoreProgressPhase::Ready,
        );
        Ok(destination)
    }
}
