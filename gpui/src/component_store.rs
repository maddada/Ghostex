use std::{
    collections::{HashMap, HashSet},
    env, fs,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use flate2::read::GzDecoder;
use sha2::{Digest as _, Sha256};

#[cfg(target_os = "linux")]
use std::io::Write as _;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

const MANIFEST_SCHEMA_VERSION: u64 = 2;
const CODE_SERVER_COMPONENT_NAME: &str = "code-server";
const CODE_SERVER_ARCHIVE_CONTRACT_JSON: &str =
    include_str!("../../shared/code-server-archive-contract.json");

struct CodeServerArchiveContract {
    schema_version: u64,
    required_entries: Vec<String>,
    required_entries_by_platform: HashMap<String, Vec<String>>,
    executable_entries: Vec<String>,
    executable_entries_by_platform: HashMap<String, Vec<String>>,
    readiness_entry: String,
    readiness_signal: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VerifiedCodeServerArchive {
    pub component_version: String,
    pub platform: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseAsset {
    pub bytes: u64,
    pub name: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentPlatformAsset {
    pub asset_name: String,
    pub sha256_sidecar_name: Option<String>,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentDefinition {
    pub name: String,
    pub component_version: String,
    pub download_tag: String,
    pub platforms: HashMap<String, ComponentPlatformAsset>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnDemandManifest {
    pub version: String,
    pub github_repo: String,
    pub assets: HashMap<String, ReleaseAsset>,
    pub components: HashMap<String, ComponentDefinition>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentStoreProgressPhase {
    Checking,
    Downloading,
    Verifying,
    Installing,
    Pruning,
    Ready,
}

impl ComponentStoreProgressPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Checking => "checking",
            Self::Downloading => "downloading",
            Self::Verifying => "verifying",
            Self::Installing => "installing",
            Self::Pruning => "pruning",
            Self::Ready => "ready",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentStoreProgress {
    pub component: String,
    pub component_version: String,
    pub downloaded_bytes: u64,
    pub platform: String,
    pub phase: ComponentStoreProgressPhase,
    pub size_bytes: u64,
}

impl ComponentStoreProgress {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "component": self.component,
            "componentVersion": self.component_version,
            "downloadedBytes": self.downloaded_bytes,
            "platform": self.platform,
            "phase": self.phase.as_str(),
            "sizeBytes": self.size_bytes,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledComponent {
    pub installed: bool,
    pub name: String,
    pub version: String,
    pub platform: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAssetCachePayload<'a> {
    DownloadArchive,
    ExtractedExecutable(&'a str),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedReleaseAsset {
    pub asset_key: String,
    pub cached: bool,
    pub download_size_bytes: u64,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub version: String,
}

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

impl OnDemandManifest {
    pub fn load(path: &Path) -> Result<Self, String> {
        let data = fs::read_to_string(path).map_err(|error| {
            format!(
                "Could not read sealed on-demand manifest {}: {error}",
                path.display()
            )
        })?;
        let payload = serde_json::from_str::<serde_json::Value>(&data).map_err(|error| {
            format!(
                "Malformed sealed on-demand manifest {}: {error}",
                path.display()
            )
        })?;
        Self::parse(&payload)
    }

    fn parse(payload: &serde_json::Value) -> Result<Self, String> {
        let root = object(payload, "root")?;
        if unsigned(root.get("schemaVersion"), "schemaVersion")? != MANIFEST_SCHEMA_VERSION {
            return Err(
                "Malformed sealed on-demand manifest: schemaVersion must equal 2".to_string(),
            );
        }
        let version = nonempty_string(root.get("version"), "version")?;
        let github_repo = nonempty_string(root.get("githubRepo"), "githubRepo")?;
        if github_repo.matches('/').count() != 1
            || github_repo.split('/').any(|part| !valid_identifier(part))
        {
            return Err(
                "Malformed sealed on-demand manifest: githubRepo must have owner/repository form"
                    .to_string(),
            );
        }

        let mut assets = HashMap::new();
        for (key, raw_asset) in object(
            root.get("assets").ok_or_else(|| missing("assets"))?,
            "assets",
        )? {
            require_identifier(key, "release asset key")?;
            let asset = object(raw_asset, &format!("assets.{key}"))?;
            assets.insert(
                key.clone(),
                ReleaseAsset {
                    bytes: unsigned(asset.get("bytes"), &format!("assets.{key}.bytes"))?,
                    name: asset_name(asset.get("name"), &format!("assets.{key}.name"))?,
                    sha256: sha256(asset.get("sha256"), &format!("assets.{key}.sha256"))?,
                },
            );
        }

        let mut components = HashMap::new();
        for (key, raw_component) in object(
            root.get("components")
                .ok_or_else(|| missing("components"))?,
            "components",
        )? {
            require_identifier(key, "component key")?;
            let component = object(raw_component, &format!("components.{key}"))?;
            let name = identifier(component.get("name"), &format!("components.{key}.name"))?;
            if name != key.as_str() {
                return Err(format!(
                    "Malformed sealed on-demand manifest: components.{key}.name must equal its map key"
                ));
            }
            let component_version = identifier(
                component.get("componentVersion"),
                &format!("components.{key}.componentVersion"),
            )?;
            let download_tag = identifier(
                component.get("downloadTag"),
                &format!("components.{key}.downloadTag"),
            )?;
            let raw_platforms = object(
                component
                    .get("platforms")
                    .ok_or_else(|| missing(&format!("components.{key}.platforms")))?,
                &format!("components.{key}.platforms"),
            )?;
            if raw_platforms.is_empty() {
                return Err(format!(
                    "Malformed sealed on-demand manifest: components.{key}.platforms must not be empty"
                ));
            }
            let mut platforms = HashMap::new();
            for (platform, raw_platform_asset) in raw_platforms {
                require_identifier(platform, "component platform")?;
                let platform_asset = object(
                    raw_platform_asset,
                    &format!("components.{key}.platforms.{platform}"),
                )?;
                let platform_asset_name = asset_name(
                    platform_asset.get("assetName"),
                    &format!("components.{key}.platforms.{platform}.assetName"),
                )?;
                let sha256_sidecar_name = platform_asset
                    .get("sha256SidecarName")
                    .map(|value| {
                        asset_name(
                            Some(value),
                            &format!("components.{key}.platforms.{platform}.sha256SidecarName"),
                        )
                    })
                    .transpose()?;
                let expected_sidecar_name = format!("{platform_asset_name}.sha256");
                if key == CODE_SERVER_COMPONENT_NAME
                    && sha256_sidecar_name.as_deref() != Some(expected_sidecar_name.as_str())
                {
                    return Err(format!(
                        "Malformed sealed on-demand manifest: components.{key}.platforms.{platform}.sha256SidecarName must equal {expected_sidecar_name}"
                    ));
                }
                if sha256_sidecar_name
                    .as_deref()
                    .is_some_and(|name| name != expected_sidecar_name)
                {
                    return Err(format!(
                        "Malformed sealed on-demand manifest: components.{key}.platforms.{platform}.sha256SidecarName must equal {expected_sidecar_name}"
                    ));
                }
                platforms.insert(
                    platform.clone(),
                    ComponentPlatformAsset {
                        asset_name: platform_asset_name,
                        sha256_sidecar_name,
                        sha256: sha256(
                            platform_asset.get("sha256"),
                            &format!("components.{key}.platforms.{platform}.sha256"),
                        )?,
                        size_bytes: unsigned(
                            platform_asset.get("sizeBytes"),
                            &format!("components.{key}.platforms.{platform}.sizeBytes"),
                        )?,
                    },
                );
            }
            components.insert(
                key.clone(),
                ComponentDefinition {
                    name,
                    component_version,
                    download_tag,
                    platforms,
                },
            );
        }
        Ok(Self {
            version,
            github_repo,
            assets,
            components,
        })
    }
}

impl ComponentStore {
    pub fn from_manifest(manifest: OnDemandManifest) -> Result<Self, String> {
        let root = component_store_root()?;
        prune_other_versions(&legacy_asset_cache_root()?, &manifest.version)?;
        Ok(Self { manifest, root })
    }

    pub fn with_root(manifest: OnDemandManifest, root: PathBuf) -> Self {
        Self { manifest, root }
    }

    pub fn release_version(&self) -> &str {
        &self.manifest.version
    }

    pub fn component(&self, name: &str) -> Option<&ComponentDefinition> {
        self.manifest.components.get(name)
    }

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
        let path = self.root.join(name).join(version).join(platform);
        let expected_sha256 = self
            .manifest
            .components
            .get(name)
            .filter(|component| component.component_version == version)
            .and_then(|component| component.platforms.get(platform))
            .map(|asset| asset.sha256.as_str());
        let installed = installed_marker_matches(&path, name, version, platform, expected_sha256)?;
        if installed && name == CODE_SERVER_COMPONENT_NAME && platform.starts_with("windows-") {
            verify_installed_windows_code_server_component(&path, version, platform)?;
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
        let version_root = component_root.join(&component.component_version);
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
            prune_other_versions(&component_root, &component.component_version)?;
            emit(
                progress,
                component,
                platform,
                asset.size_bytes,
                ComponentStoreProgressPhase::Ready,
            );
            return Ok(current);
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
            &self.manifest.github_repo,
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
                &self.manifest.github_repo,
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
            verify_installed_windows_code_server_component(
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
        CDXC:ComponentStoreInterruptedInstallCleanup 2026-08-09:
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
        prune_other_versions(&component_root, &component.component_version)?;
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
        let version_path = self.root.join(name).join(version);
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

    pub fn has_release_asset(&self, asset_key: &str) -> bool {
        self.manifest.assets.contains_key(asset_key)
    }

    pub fn query_release_asset_cache(
        &self,
        asset_key: &str,
        payload: ReleaseAssetCachePayload<'_>,
    ) -> Result<CachedReleaseAsset, String> {
        let asset =
            self.manifest.assets.get(asset_key).ok_or_else(|| {
                format!("Sealed manifest does not define release asset {asset_key}")
            })?;
        let cache_dir = legacy_asset_cache_root()?.join(&self.manifest.version);
        let path = match payload {
            ReleaseAssetCachePayload::DownloadArchive => cache_dir.join(&asset.name),
            ReleaseAssetCachePayload::ExtractedExecutable(name) => {
                require_cache_file_name(name)?;
                cache_dir.join(name)
            }
        };
        let size_bytes = path
            .metadata()
            .ok()
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let cached = match payload {
            ReleaseAssetCachePayload::DownloadArchive => size_bytes == asset.bytes,
            ReleaseAssetCachePayload::ExtractedExecutable(_) => {
                cached_executable_is_ready(&path, size_bytes)
            }
        };
        Ok(CachedReleaseAsset {
            asset_key: asset_key.to_string(),
            cached,
            download_size_bytes: asset.bytes,
            path,
            size_bytes,
            version: self.manifest.version.clone(),
        })
    }

    pub fn remove_release_asset_cache(
        &self,
        asset_key: &str,
        payload: ReleaseAssetCachePayload<'_>,
    ) -> Result<bool, String> {
        let cached = self.query_release_asset_cache(asset_key, payload)?;
        if !cached.path.exists() {
            return Ok(false);
        }
        fs::remove_file(&cached.path).map_err(|error| {
            format!(
                "Could not remove cached release asset {}: {error}",
                cached.path.display()
            )
        })?;
        Ok(true)
    }
}

fn emit(
    progress: &mut dyn FnMut(ComponentStoreProgress),
    component: &ComponentDefinition,
    platform: &str,
    size_bytes: u64,
    phase: ComponentStoreProgressPhase,
) {
    progress(ComponentStoreProgress {
        component: component.name.clone(),
        component_version: component.component_version.clone(),
        downloaded_bytes: 0,
        platform: platform.to_string(),
        phase,
        size_bytes,
    });
}

fn emit_download_progress(
    progress: &mut dyn FnMut(ComponentStoreProgress),
    component: &ComponentDefinition,
    platform: &str,
    size_bytes: u64,
    downloaded_bytes: u64,
) {
    progress(ComponentStoreProgress {
        component: component.name.clone(),
        component_version: component.component_version.clone(),
        downloaded_bytes: downloaded_bytes.min(size_bytes),
        platform: platform.to_string(),
        phase: ComponentStoreProgressPhase::Downloading,
        size_bytes,
    });
}

fn component_store_root() -> Result<PathBuf, String> {
    if let Some(override_root) = env::var_os("GHOSTEX_COMPONENT_STORE_DIR") {
        return Ok(PathBuf::from(override_root));
    }
    #[cfg(target_os = "windows")]
    /*
    CDXC:WindowsComponentPersistence 2026-08-16:
    Velopack owns %LOCALAPPDATA%\Ghostex and transactionally replaces that
    entire directory during reinstall/update. Keep large on-demand runtimes in
    a sibling data root so the installer cannot discard a verified CEF payload
    and force every downloaded upgrade to fetch Chromium again.
    */
    return env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| root.join("GhostexData/components"))
        .ok_or_else(|| {
            "LOCALAPPDATA is unavailable; cannot locate the Ghostex component store".to_string()
        });
    #[cfg(not(target_os = "windows"))]
    return ghostex_paths::GhostexPaths::resolve_and_migrate()
        .map(|paths| paths.data_dir.join("components"))
        .map_err(|error| format!("Could not migrate Ghostex component storage: {error}"));
}

fn legacy_asset_cache_root() -> Result<PathBuf, String> {
    if let Some(override_root) = env::var_os("GHOSTEX_ON_DEMAND_CACHE_DIR") {
        return Ok(PathBuf::from(override_root));
    }
    #[cfg(target_os = "windows")]
    return env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| root.join("GhostexData/on-demand"))
        .ok_or_else(|| {
            "LOCALAPPDATA is unavailable; cannot locate the Ghostex release asset cache".to_string()
        });
    #[cfg(not(target_os = "windows"))]
    return ghostex_paths::GhostexPaths::resolve_and_migrate()
        .map(|paths| paths.data_dir.join("on-demand"))
        .map_err(|error| format!("Could not migrate Ghostex release asset storage: {error}"));
}

fn current_platform() -> Result<String, String> {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return Err(
            "This operating system is unsupported by the Ghostex component store".to_string(),
        );
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        return Err(
            "This CPU architecture is unsupported by the Ghostex component store".to_string(),
        );
    };
    Ok(format!("{os}-{arch}"))
}

fn download_url(repo: &str, tag: &str, asset_name: &str) -> String {
    let base = env::var("GHOSTEX_ON_DEMAND_BASE_URL")
        .unwrap_or_else(|_| format!("https://github.com/{repo}/releases/download"));
    format!("{}/{tag}/{asset_name}", base.trim_end_matches('/'))
}

fn download(
    url: &str,
    destination: &Path,
    expected_size: u64,
    progress: &mut dyn FnMut(u64),
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        /*
        CDXC:WindowsComponentDownloader 2026-08-16:
        Do not resolve and launch an arbitrary curl.exe from the customer's
        PATH. Windows installations already carry Velopack's in-process HTTPS
        stack for signed application updates; use that same deterministic
        downloader for sealed CEF/code-server assets so a broken or shadowed
        system curl cannot strand Ghostex during first launch. The component
        store still owns exact-size and SHA-256 verification after download.
        */
        let mut last_error = None;
        for attempt in 1..=3 {
            let result = velopack::download::download_url_to_file(url, destination, |percent| {
                if expected_size > 0 {
                    let downloaded =
                        expected_size.saturating_mul(percent.clamp(0, 100) as u64) / 100;
                    progress(downloaded);
                }
            });
            match result {
                Ok(()) => {
                    if expected_size > 0 {
                        progress(expected_size);
                    }
                    return Ok(());
                }
                Err(error) => {
                    last_error = Some(error.to_string());
                    if attempt < 3 {
                        thread::sleep(Duration::from_millis(500 * attempt));
                    }
                }
            }
        }
        return Err(format!(
            "Could not download component asset from {url}: {}",
            last_error.unwrap_or_else(|| "the HTTPS request failed".to_string())
        ));
    }

    #[cfg(target_os = "linux")]
    {
        /*
        CDXC:LinuxComponentDownloader 2026-08-16:
        Installed Ghostex must not depend on a PATH-resolved curl process for
        its first-launch CEF download. Stream through an in-process HTTPS
        client using the Linux system certificate verifier, keep the existing
        bounded retries, and surface the actual request/read/write error. The
        component store still verifies the exact byte count and sealed SHA-256
        before the archive can become an installed runtime.
        */
        let tls_config = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(900)))
            .tls_config(tls_config)
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let mut last_error = None;

        for attempt in 1..=3 {
            let result = (|| -> Result<(), String> {
                let mut response = agent
                    .get(url)
                    .call()
                    .map_err(|error| format!("HTTPS request failed: {error}"))?;
                let mut file = File::create(destination).map_err(|error| {
                    format!(
                        "Could not create component download {}: {error}",
                        destination.display()
                    )
                })?;
                let mut reader = response.body_mut().as_reader();
                let mut buffer = [0_u8; 64 * 1024];
                let mut downloaded_bytes = 0_u64;

                loop {
                    let count = reader
                        .read(&mut buffer)
                        .map_err(|error| format!("HTTPS response read failed: {error}"))?;
                    if count == 0 {
                        break;
                    }
                    file.write_all(&buffer[..count])
                        .map_err(|error| format!("Component download write failed: {error}"))?;
                    downloaded_bytes = downloaded_bytes.saturating_add(count as u64);
                    progress(downloaded_bytes);
                }
                file.sync_all()
                    .map_err(|error| format!("Could not finish component download: {error}"))?;
                Ok(())
            })();

            match result {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_error = Some(error);
                    if attempt < 3 {
                        thread::sleep(Duration::from_millis(500 * attempt));
                    }
                }
            }
        }

        return Err(format!(
            "Could not download component asset from {url}: {}",
            last_error.unwrap_or_else(|| "the HTTPS request failed".to_string())
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("/usr/bin/curl");

        command
            .args([
                "--fail",
                "--location",
                "--retry",
                "2",
                "--max-time",
                "900",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(destination)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let mut child = command
            .spawn()
            .map_err(|error| format!("Could not launch component downloader for {url}: {error}"))?;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if expected_size > 0 {
                        let downloaded_bytes = fs::metadata(destination)
                            .map(|metadata| metadata.len())
                            .unwrap_or(0);
                        progress(downloaded_bytes);
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "Could not monitor component downloader for {url}: {error}"
                    ));
                }
            }
        };
        if expected_size > 0 {
            let downloaded_bytes = fs::metadata(destination)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            progress(downloaded_bytes);
        }
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "Could not download component asset from {url}: downloader exited with {status}"
            ))
        }
    }
}

fn verify_file(path: &Path, expected_sha256: &str, expected_size: u64) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "Could not inspect downloaded component asset {}: {error}",
            path.display()
        )
    })?;
    if metadata.len() != expected_size {
        return Err(format!(
            "Downloaded component asset size mismatch: expected {expected_size} bytes, received {} bytes",
            metadata.len()
        ));
    }
    let mut file = File::open(path).map_err(|error| {
        format!(
            "Could not open downloaded component asset {}: {error}",
            path.display()
        )
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| {
            format!(
                "Could not hash downloaded component asset {}: {error}",
                path.display()
            )
        })?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let actual = format!("{:x}", digest.finalize());
    if actual != expected_sha256 {
        return Err("Downloaded component asset failed SHA-256 verification against the app's sealed manifest".to_string());
    }
    Ok(())
}

fn code_server_archive_contract() -> Result<CodeServerArchiveContract, String> {
    let value = serde_json::from_str::<serde_json::Value>(CODE_SERVER_ARCHIVE_CONTRACT_JSON)
        .map_err(|error| format!("Invalid bundled code-server archive contract: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Invalid bundled code-server archive contract".to_string())?;
    let expected_keys = [
        "schemaVersion",
        "requiredEntries",
        "requiredEntriesByPlatform",
        "executableEntries",
        "executableEntriesByPlatform",
        "readinessEntry",
        "readinessSignal",
    ]
    .into_iter()
    .collect::<HashSet<_>>();
    if object.keys().map(String::as_str).collect::<HashSet<_>>() != expected_keys {
        return Err("Invalid bundled code-server archive contract fields".to_string());
    }
    let string_array = |key: &str| -> Result<Vec<String>, String> {
        object
            .get(key)
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("Invalid bundled code-server archive contract {key}"))?
            .iter()
            .map(|value| {
                value.as_str().map(str::to_string).ok_or_else(|| {
                    format!("Invalid bundled code-server archive contract {key} entry")
                })
            })
            .collect()
    };
    let string = |key: &str| -> Result<String, String> {
        object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("Invalid bundled code-server archive contract {key}"))
    };
    let string_array_map = |key: &str| -> Result<HashMap<String, Vec<String>>, String> {
        object
            .get(key)
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| format!("Invalid bundled code-server archive contract {key}"))?
            .iter()
            .map(|(platform, values)| {
                let entries = values
                    .as_array()
                    .ok_or_else(|| {
                        format!("Invalid bundled code-server archive contract {key}.{platform}")
                    })?
                    .iter()
                    .map(|value| {
                        value.as_str().map(str::to_string).ok_or_else(|| {
                            format!(
                                "Invalid bundled code-server archive contract {key}.{platform} entry"
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((platform.clone(), entries))
            })
            .collect()
    };
    let contract = CodeServerArchiveContract {
        schema_version: object
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                "Invalid bundled code-server archive contract schemaVersion".to_string()
            })?,
        required_entries: string_array("requiredEntries")?,
        required_entries_by_platform: string_array_map("requiredEntriesByPlatform")?,
        executable_entries: string_array("executableEntries")?,
        executable_entries_by_platform: string_array_map("executableEntriesByPlatform")?,
        readiness_entry: string("readinessEntry")?,
        readiness_signal: string("readinessSignal")?,
    };
    let supported_platforms = HashSet::from([
        "darwin-arm64".to_string(),
        "linux-arm64".to_string(),
        "linux-x64".to_string(),
    ]);
    if contract.schema_version != 2
        || contract.required_entries.is_empty()
        || contract.executable_entries.is_empty()
        || contract
            .required_entries_by_platform
            .keys()
            .cloned()
            .collect::<HashSet<_>>()
            != supported_platforms
        || contract
            .executable_entries_by_platform
            .keys()
            .cloned()
            .collect::<HashSet<_>>()
            != supported_platforms
        || !contract
            .required_entries
            .contains(&contract.readiness_entry)
    {
        return Err("Invalid bundled code-server archive contract".to_string());
    }
    for platform in &supported_platforms {
        let mut required_entries = contract.required_entries.clone();
        required_entries.extend(
            contract.required_entries_by_platform[platform]
                .iter()
                .cloned(),
        );
        let mut executable_entries = contract.executable_entries.clone();
        executable_entries.extend(
            contract.executable_entries_by_platform[platform]
                .iter()
                .cloned(),
        );
        if executable_entries
            .iter()
            .any(|entry| !required_entries.contains(entry))
        {
            return Err("Invalid bundled code-server archive contract".to_string());
        }
    }
    for entry in contract
        .required_entries
        .iter()
        .chain(contract.executable_entries.iter())
        .chain(contract.required_entries_by_platform.values().flatten())
        .chain(contract.executable_entries_by_platform.values().flatten())
    {
        if entry.is_empty()
            || entry.starts_with('/')
            || entry.split('/').any(|segment| {
                segment.is_empty()
                    || segment == "."
                    || segment == ".."
                    || !segment.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'@')
                    })
            })
        {
            return Err("Invalid bundled code-server archive contract path".to_string());
        }
    }
    if contract.readiness_signal.is_empty()
        || !contract
            .readiness_signal
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("Invalid bundled code-server archive readiness signal".to_string());
    }
    Ok(contract)
}

fn code_server_archive_contract_entries(
    contract: &CodeServerArchiveContract,
    platform: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    let platform_required = contract
        .required_entries_by_platform
        .get(platform)
        .ok_or_else(|| format!("Unsupported code-server archive platform: {platform}"))?;
    let platform_executable = contract
        .executable_entries_by_platform
        .get(platform)
        .ok_or_else(|| format!("Unsupported code-server archive platform: {platform}"))?;
    let mut required = contract.required_entries.clone();
    required.extend(platform_required.iter().cloned());
    let mut executable = contract.executable_entries.clone();
    executable.extend(platform_executable.iter().cloned());
    Ok((required, executable))
}

fn valid_code_server_component_version(version: &str) -> bool {
    let Some((revision, fingerprint)) = version.split_once("-p2-") else {
        return false;
    };
    revision.len() == 12
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        && fingerprint.len() == 64
        && fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn code_server_archive_name(component_version: &str, platform: &str) -> Result<String, String> {
    if !valid_code_server_component_version(component_version) {
        return Err("Invalid code-server p2 component identity".to_string());
    }
    if !matches!(platform, "linux-x64" | "linux-arm64") {
        return Err(format!(
            "Unsupported code-server archive platform: {platform}"
        ));
    }
    Ok(format!("code-server-{component_version}-{platform}.tar.gz"))
}

fn code_server_archive_sha256(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| {
        format!(
            "Could not open code-server archive {}: {error}",
            path.display()
        )
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| {
            format!(
                "Could not hash code-server archive {}: {error}",
                path.display()
            )
        })?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn parse_code_server_checksum_sidecar(
    contents: &str,
    expected_archive_name: &str,
) -> Result<String, String> {
    let record = if let Some(record) = contents.strip_suffix("\r\n") {
        record
    } else if let Some(record) = contents.strip_suffix('\n') {
        record
    } else {
        contents
    };
    let Some((sha256, archive_name)) = record.split_once("  ") else {
        return Err("Malformed code-server checksum sidecar".to_string());
    };
    if sha256.len() != 64
        || !sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || archive_name.contains('\r')
        || archive_name.contains('\n')
    {
        return Err("Malformed code-server checksum sidecar".to_string());
    }
    if archive_name != expected_archive_name {
        return Err(format!(
            "Code-server checksum filename mismatch: expected {expected_archive_name}, got {archive_name}"
        ));
    }
    Ok(sha256.to_string())
}

const CODE_SERVER_TAR_BLOCK_SIZE: usize = 512;
const CODE_SERVER_TAR_METADATA_LIMIT: u64 = 1024 * 1024;

fn raw_code_server_tar_string<'a>(field: &'a [u8], label: &str) -> Result<&'a str, String> {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    if field[end..].iter().any(|byte| *byte != 0) {
        return Err(format!("Malformed code-server tar {label}"));
    }
    std::str::from_utf8(&field[..end])
        .map_err(|_| format!("Invalid UTF-8 in code-server tar {label}"))
}

fn raw_code_server_tar_octal(field: &[u8], label: &str) -> Result<u64, String> {
    let start = field
        .iter()
        .position(|byte| !matches!(byte, 0 | b' '))
        .unwrap_or(field.len());
    let end = field[start..]
        .iter()
        .position(|byte| matches!(byte, 0 | b' '))
        .map(|offset| start + offset)
        .unwrap_or(field.len());
    if field[end..].iter().any(|byte| !matches!(byte, 0 | b' ')) {
        return Err(format!("Invalid code-server tar {label}"));
    }
    let value = &field[start..end];
    if value.is_empty() || !value.iter().all(|byte| matches!(byte, b'0'..=b'7')) {
        return Err(format!("Invalid code-server tar {label}"));
    }
    let value =
        std::str::from_utf8(value).map_err(|_| format!("Invalid code-server tar {label}"))?;
    u64::from_str_radix(value, 8).map_err(|_| format!("Invalid code-server tar {label}"))
}

fn raw_code_server_tar_header_name(
    header: &[u8; CODE_SERVER_TAR_BLOCK_SIZE],
) -> Result<String, String> {
    let name = raw_code_server_tar_string(&header[..100], "entry name")?;
    let prefix = raw_code_server_tar_string(&header[345..500], "entry prefix")?;
    Ok(if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    })
}

fn validate_raw_code_server_tar_checksum(
    header: &[u8; CODE_SERVER_TAR_BLOCK_SIZE],
) -> Result<(), String> {
    let stored = raw_code_server_tar_octal(&header[148..156], "header checksum")?;
    let actual = header
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            if (148..156).contains(&index) {
                u64::from(b' ')
            } else {
                u64::from(*byte)
            }
        })
        .sum::<u64>();
    if stored != actual {
        return Err("Invalid code-server tar header checksum".to_string());
    }
    Ok(())
}

fn parse_raw_code_server_pax_metadata(contents: &[u8]) -> Result<HashMap<String, String>, String> {
    let mut values = HashMap::new();
    let mut offset = 0_usize;
    while offset < contents.len() {
        let relative_space = contents[offset..]
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or_else(|| "Malformed code-server PAX header".to_string())?;
        let space = offset + relative_space;
        let length_text = std::str::from_utf8(&contents[offset..space])
            .map_err(|_| "Malformed code-server PAX header".to_string())?;
        if length_text.is_empty()
            || length_text.starts_with('0')
            || !length_text.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err("Malformed code-server PAX header".to_string());
        }
        let length = length_text
            .parse::<usize>()
            .map_err(|_| "Malformed code-server PAX header".to_string())?;
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= contents.len())
            .ok_or_else(|| "Malformed code-server PAX header".to_string())?;
        if end <= space + 1 || contents[end - 1] != b'\n' {
            return Err("Malformed code-server PAX header".to_string());
        }
        let record = std::str::from_utf8(&contents[space + 1..end - 1])
            .map_err(|_| "Invalid UTF-8 in code-server PAX record".to_string())?;
        let (key, value) = record
            .split_once('=')
            .filter(|(key, value)| !key.is_empty() && !value.is_empty())
            .ok_or_else(|| "Malformed code-server PAX header".to_string())?;
        if !matches!(key, "path" | "linkpath") {
            return Err(format!("Unsupported code-server PAX field: {key}"));
        }
        if values.insert(key.to_string(), value.to_string()).is_some() {
            return Err(format!("Duplicate code-server PAX field: {key}"));
        }
        offset = end;
    }
    if values.is_empty() {
        return Err("Malformed empty code-server PAX header".to_string());
    }
    Ok(values)
}

fn raw_code_server_gnu_metadata(contents: &[u8], label: &str) -> Result<String, String> {
    let Some((&0, value)) = contents.split_last() else {
        return Err(format!("Malformed GNU tar {label}"));
    };
    if value.is_empty() || value.contains(&0) {
        return Err(format!("Malformed GNU tar {label}"));
    }
    std::str::from_utf8(value)
        .map(str::to_string)
        .map_err(|_| format!("Invalid UTF-8 in GNU tar {label}"))
}

fn read_raw_code_server_tar_payload(
    decoder: &mut GzDecoder<File>,
    size: u64,
    retain: bool,
) -> Result<Vec<u8>, String> {
    let padded_size = size
        .checked_add((CODE_SERVER_TAR_BLOCK_SIZE - 1) as u64)
        .map(|value| value / CODE_SERVER_TAR_BLOCK_SIZE as u64 * CODE_SERVER_TAR_BLOCK_SIZE as u64)
        .ok_or_else(|| "Invalid code-server tar entry size".to_string())?;
    if retain {
        let padded_size = usize::try_from(padded_size)
            .map_err(|_| "Invalid code-server tar metadata size".to_string())?;
        let size = usize::try_from(size)
            .map_err(|_| "Invalid code-server tar metadata size".to_string())?;
        let mut payload = vec![0_u8; padded_size];
        decoder
            .read_exact(&mut payload)
            .map_err(|error| format!("Truncated code-server tar metadata: {error}"))?;
        payload.truncate(size);
        return Ok(payload);
    }
    let copied = io::copy(&mut decoder.take(padded_size), &mut io::sink())
        .map_err(|error| format!("Could not inspect code-server tar payload: {error}"))?;
    if copied != padded_size {
        return Err("Truncated code-server tar payload".to_string());
    }
    Ok(Vec::new())
}

fn preflight_code_server_tar_metadata(archive_path: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|error| {
        format!(
            "Could not open verified code-server archive {}: {error}",
            archive_path.display()
        )
    })?;
    let mut decoder = GzDecoder::new(file);
    let mut pending_long_name = None::<String>;
    let mut pending_long_link = None::<String>;
    let mut pending_pax = None::<HashMap<String, String>>;
    loop {
        let mut header = [0_u8; CODE_SERVER_TAR_BLOCK_SIZE];
        decoder
            .read_exact(&mut header)
            .map_err(|error| format!("Truncated code-server tar header: {error}"))?;
        if header.iter().all(|byte| *byte == 0) {
            if pending_long_name.is_some() || pending_long_link.is_some() || pending_pax.is_some() {
                return Err("Dangling code-server tar metadata header".to_string());
            }
            let mut trailing = Vec::new();
            decoder
                .read_to_end(&mut trailing)
                .map_err(|error| format!("Invalid code-server tar terminator: {error}"))?;
            if trailing.len() < CODE_SERVER_TAR_BLOCK_SIZE || trailing.iter().any(|byte| *byte != 0)
            {
                return Err("Malformed code-server tar terminator".to_string());
            }
            return Ok(());
        }

        validate_raw_code_server_tar_checksum(&header)?;
        let archive_name = raw_code_server_tar_header_name(&header)?;
        let entry_type = match header[156] {
            0 => b'0',
            value => value,
        };
        let size = raw_code_server_tar_octal(&header[124..136], "entry size")?;
        let normalized_archive_name =
            normalized_code_server_tar_path_text(&archive_name, entry_type == b'5')?;

        if matches!(entry_type, b'L' | b'K' | b'x') {
            if size == 0 || size > CODE_SERVER_TAR_METADATA_LIMIT {
                return Err(format!(
                    "Malformed code-server tar metadata size for {}",
                    normalized_archive_name.as_deref().unwrap_or(".")
                ));
            }
            let contents = read_raw_code_server_tar_payload(&mut decoder, size, true)?;
            match entry_type {
                b'L' => {
                    if pending_long_name.is_some() {
                        return Err("Duplicate GNU tar long-name header".to_string());
                    }
                    if pending_pax
                        .as_ref()
                        .is_some_and(|pax| pax.contains_key("path"))
                    {
                        return Err("Conflicting code-server tar path metadata".to_string());
                    }
                    if archive_name != "././@LongLink" {
                        return Err("Malformed GNU tar long-name header".to_string());
                    }
                    pending_long_name = Some(raw_code_server_gnu_metadata(&contents, "long name")?);
                }
                b'K' => {
                    if pending_long_link.is_some() {
                        return Err("Duplicate GNU tar long-link header".to_string());
                    }
                    if pending_pax
                        .as_ref()
                        .is_some_and(|pax| pax.contains_key("linkpath"))
                    {
                        return Err("Conflicting code-server tar link metadata".to_string());
                    }
                    if archive_name != "././@LongLink" {
                        return Err("Malformed GNU tar long-link header".to_string());
                    }
                    pending_long_link = Some(raw_code_server_gnu_metadata(&contents, "long link")?);
                }
                b'x' => {
                    if pending_pax.is_some() {
                        return Err("Duplicate local PAX tar header".to_string());
                    }
                    let metadata_path = normalized_archive_name.as_deref().ok_or_else(|| {
                        "Malformed local PAX code-server archive header".to_string()
                    })?;
                    if !metadata_path
                        .split('/')
                        .any(|segment| segment == "PaxHeader")
                    {
                        return Err("Malformed local PAX code-server archive header".to_string());
                    }
                    let pax = parse_raw_code_server_pax_metadata(&contents)?;
                    if pax.contains_key("path") && pending_long_name.is_some() {
                        return Err("Conflicting code-server tar path metadata".to_string());
                    }
                    if pax.contains_key("linkpath") && pending_long_link.is_some() {
                        return Err("Conflicting code-server tar link metadata".to_string());
                    }
                    pending_pax = Some(pax);
                }
                _ => unreachable!(),
            }
            continue;
        }
        if entry_type == b'g' {
            return Err("Unsupported global PAX code-server archive header".to_string());
        }
        if !matches!(entry_type, b'0' | b'1' | b'2' | b'5') {
            return Err(format!(
                "Unsupported code-server archive entry type {entry_type:#04x} for {}",
                normalized_archive_name.as_deref().unwrap_or(".")
            ));
        }
        if (pending_long_link.is_some()
            || pending_pax
                .as_ref()
                .is_some_and(|pax| pax.contains_key("linkpath")))
            && !matches!(entry_type, b'1' | b'2')
        {
            return Err("Code-server tar link metadata does not describe a link entry".to_string());
        }
        let effective_name = pending_pax
            .as_ref()
            .and_then(|pax| pax.get("path"))
            .or(pending_long_name.as_ref())
            .map(String::as_str)
            .unwrap_or(&archive_name);
        let effective_path =
            normalized_code_server_tar_path_text(effective_name, entry_type == b'5')?;
        if matches!(entry_type, b'1' | b'2') {
            let effective_path = effective_path
                .as_deref()
                .ok_or_else(|| "Malformed code-server archive link entry".to_string())?;
            let header_link = raw_code_server_tar_string(&header[157..257], "link target")?;
            let effective_link = pending_pax
                .as_ref()
                .and_then(|pax| pax.get("linkpath"))
                .or(pending_long_link.as_ref())
                .map(String::as_str)
                .unwrap_or(header_link);
            normalized_code_server_link_target(
                effective_path,
                Path::new(effective_link),
                entry_type == b'1',
            )?;
        }
        pending_long_name = None;
        pending_long_link = None;
        pending_pax = None;
        read_raw_code_server_tar_payload(&mut decoder, size, false)?;
    }
}

fn normalized_code_server_tar_path_text(
    path: &str,
    allow_root: bool,
) -> Result<Option<String>, String> {
    let mut normalized = path;
    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped;
    }
    while let Some(stripped) = normalized.strip_suffix('/') {
        normalized = stripped;
    }
    if normalized.is_empty() && allow_root {
        return Ok(None);
    }
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.as_bytes().get(1) == Some(&b':')
        || normalized.contains('\\')
        || normalized
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || normalized.chars().any(|character| character.is_control())
    {
        return Err("Code-server archive contains an unsafe entry".to_string());
    }
    Ok(Some(normalized.to_string()))
}

fn normalized_code_server_tar_path(
    path: &Path,
    allow_root: bool,
) -> Result<Option<String>, String> {
    normalized_code_server_tar_path_text(
        path.to_str()
            .ok_or_else(|| "Code-server archive contains a non-UTF-8 entry".to_string())?,
        allow_root,
    )
}

fn register_code_server_archive_entry(
    seen: &mut HashMap<String, &'static str>,
    path: &str,
    kind: &'static str,
) -> Result<(), String> {
    if seen.contains_key(path) {
        return Err(format!("Duplicate code-server archive entry: {path}"));
    }
    let segments = path.split('/').collect::<Vec<_>>();
    for index in 1..segments.len() {
        let parent = segments[..index].join("/");
        if seen
            .get(&parent)
            .is_some_and(|parent_kind| *parent_kind != "directory")
        {
            return Err(format!(
                "Conflicting code-server archive entries: {parent} and {path}"
            ));
        }
    }
    if kind != "directory"
        && seen
            .keys()
            .any(|entry| entry.starts_with(&format!("{path}/")))
    {
        return Err(format!(
            "Conflicting code-server archive entries beneath {path}"
        ));
    }
    seen.insert(path.to_string(), kind);
    Ok(())
}

fn normalized_code_server_link_target(
    entry_path: &str,
    link_path: &Path,
    hardlink: bool,
) -> Result<String, String> {
    if link_path.as_os_str().is_empty() {
        return Err("Code-server archive contains an unsafe empty link target".to_string());
    }
    if hardlink {
        return normalized_code_server_tar_path(link_path, false)?
            .ok_or_else(|| "Code-server archive contains an unsafe hardlink target".to_string());
    }
    let mut segments = entry_path
        .split('/')
        .take(entry_path.split('/').count().saturating_sub(1))
        .map(str::to_string)
        .collect::<Vec<_>>();
    for component in link_path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if segments.pop().is_none() {
                    return Err("Code-server archive contains an unsafe symlink target".to_string());
                }
            }
            std::path::Component::Normal(segment) => segments.push(
                segment
                    .to_str()
                    .ok_or_else(|| {
                        "Code-server archive contains a non-UTF-8 symlink target".to_string()
                    })?
                    .to_string(),
            ),
            _ => {
                return Err("Code-server archive contains an unsafe symlink target".to_string());
            }
        }
    }
    if segments.is_empty() {
        return Err("Code-server archive contains an unsafe symlink target".to_string());
    }
    Ok(segments.join("/"))
}

pub(crate) fn verify_code_server_archive(
    archive_path: &Path,
    component_version: &str,
    platform: &str,
) -> Result<VerifiedCodeServerArchive, String> {
    let expected_archive_name = code_server_archive_name(component_version, platform)?;
    if archive_path.file_name().and_then(|name| name.to_str()) != Some(&expected_archive_name) {
        return Err(format!(
            "Code-server archive identity mismatch: expected {expected_archive_name}"
        ));
    }
    let mut sidecar_name = archive_path.as_os_str().to_owned();
    sidecar_name.push(".sha256");
    let sidecar_path = PathBuf::from(sidecar_name);
    let sidecar = fs::read_to_string(&sidecar_path).map_err(|error| {
        format!(
            "Could not read filename-bound code-server checksum sidecar {}: {error}",
            sidecar_path.display()
        )
    })?;
    let expected_sha256 = parse_code_server_checksum_sidecar(&sidecar, &expected_archive_name)?;
    let actual_sha256 = code_server_archive_sha256(archive_path)?;
    if actual_sha256 != expected_sha256 {
        return Err(format!(
            "Code-server archive checksum mismatch for {expected_archive_name}"
        ));
    }

    preflight_code_server_tar_metadata(archive_path)?;

    let contract = code_server_archive_contract()?;
    let (required_entries, executable_entries) =
        code_server_archive_contract_entries(&contract, platform)?;
    let required = required_entries
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let executable = executable_entries
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let file = File::open(archive_path).map_err(|error| {
        format!(
            "Could not open verified code-server archive {}: {error}",
            archive_path.display()
        )
    })?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    let entries = archive.entries().map_err(|error| {
        format!(
            "Invalid code-server archive {}: {error}",
            archive_path.display()
        )
    })?;
    let mut found = HashSet::new();
    let mut seen = HashMap::new();
    let mut links = HashMap::new();
    let mut saw_root_directory = false;
    let mut readiness_found = false;
    for entry in entries {
        let mut entry = entry.map_err(|error| format!("Invalid code-server archive: {error}"))?;
        let entry_type = entry.header().entry_type().as_byte();
        let path = normalized_code_server_tar_path(
            entry
                .path()
                .map_err(|error| format!("Invalid code-server archive entry: {error}"))?
                .as_ref(),
            entry_type == b'5',
        )?;
        let Some(path) = path else {
            if entry_type != b'5' || entry.header().size().unwrap_or(u64::MAX) != 0 {
                return Err("Malformed code-server archive root entry".to_string());
            }
            if saw_root_directory {
                return Err("Duplicate code-server archive root directory".to_string());
            }
            saw_root_directory = true;
            continue;
        };
        match entry_type {
            b'0' => {
                register_code_server_archive_entry(&mut seen, &path, "file")?;
                found.insert(path.clone());
                if !required.contains(path.as_str()) {
                    continue;
                }
                if entry.header().size().unwrap_or(0) == 0 {
                    return Err(format!(
                        "Code-server archive is missing required payload: {path}"
                    ));
                }
                if executable.contains(path.as_str())
                    && entry.header().mode().unwrap_or(0) & 0o111 == 0
                {
                    return Err(format!(
                        "Code-server archive payload is not executable: {path}"
                    ));
                }
                if path == contract.readiness_entry {
                    let mut contents = String::new();
                    entry
                        .take(8 * 1024 * 1024)
                        .read_to_string(&mut contents)
                        .map_err(|error| {
                            format!("Could not read code-server readiness payload: {error}")
                        })?;
                    readiness_found = contents.contains(&contract.readiness_signal);
                }
            }
            b'5' => {
                if entry.header().size().unwrap_or(u64::MAX) != 0 {
                    return Err(format!("Malformed code-server directory entry: {path}"));
                }
                register_code_server_archive_entry(&mut seen, &path, "directory")?;
            }
            b'1' | b'2' => {
                if entry.header().size().unwrap_or(u64::MAX) != 0 {
                    return Err(format!("Malformed code-server link entry: {path}"));
                }
                let link_name = entry
                    .link_name()
                    .map_err(|error| format!("Invalid code-server archive link: {error}"))?
                    .ok_or_else(|| format!("Missing code-server archive link target: {path}"))?;
                let kind = if entry_type == b'1' {
                    "hardlink"
                } else {
                    "symlink"
                };
                let target = normalized_code_server_link_target(
                    &path,
                    link_name.as_ref(),
                    entry_type == b'1',
                )?;
                register_code_server_archive_entry(&mut seen, &path, kind)?;
                links.insert(path, (kind, target));
            }
            _ => {
                return Err(format!(
                    "Unsupported code-server archive entry type {entry_type:#04x} for {path}"
                ));
            }
        }
    }
    for (path, (kind, original_target)) in &links {
        let mut target = original_target.as_str();
        let mut visited = HashSet::from([path.as_str()]);
        while let Some((_, next_target)) = links.get(target) {
            if !visited.insert(target) {
                return Err(format!("Cyclic code-server archive link: {path}"));
            }
            target = next_target;
        }
        let target_kind = seen.get(target);
        if target_kind.is_none() || (*kind == "hardlink" && target_kind != Some(&"file")) {
            return Err(format!(
                "Unsafe or dangling code-server archive link: {path} -> {original_target}"
            ));
        }
    }
    for required_entry in &required_entries {
        if !found.contains(required_entry) {
            return Err(format!(
                "Code-server archive is missing required payload: {required_entry}"
            ));
        }
    }
    if !readiness_found {
        return Err(format!(
            "Code-server archive lacks compiled {} readiness signal",
            contract.readiness_signal
        ));
    }
    Ok(VerifiedCodeServerArchive {
        component_version: component_version.to_string(),
        platform: platform.to_string(),
        sha256: actual_sha256,
    })
}

pub(crate) fn verify_installed_windows_code_server_component(
    component_path: &Path,
    component_version: &str,
    windows_platform: &str,
) -> Result<VerifiedCodeServerArchive, String> {
    let linux_platform = match windows_platform {
        "windows-x64" => "linux-x64",
        "windows-arm64" => "linux-arm64",
        _ => {
            return Err(format!(
                "Unsupported installed Windows code-server platform: {windows_platform}"
            ));
        }
    };
    let archive_name = code_server_archive_name(component_version, linux_platform)?;
    verify_code_server_archive(
        &component_path.join(archive_name),
        component_version,
        linux_platform,
    )
}

pub(crate) fn code_server_payload_shell_validation_script() -> Result<String, String> {
    let contract = code_server_archive_contract()?;
    let (required_entries, executable_entries) =
        code_server_archive_contract_entries(&contract, "linux-x64")?;
    let mut checks = Vec::new();
    for entry in &required_entries {
        checks.push(format!("test -s \"$code_server_root/{entry}\""));
    }
    for entry in &executable_entries {
        checks.push(format!("test -x \"$code_server_root/{entry}\""));
    }
    checks.push(format!(
        "grep -Fq -- '{}' \"$code_server_root/{}\"",
        contract.readiness_signal, contract.readiness_entry
    ));
    Ok(checks.join("; "))
}

fn unpack_tar_gz(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|error| {
        format!(
            "Could not open verified component archive {}: {error}",
            archive_path.display()
        )
    })?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(destination).map_err(|error| {
        format!(
            "Could not unpack verified component archive {}: {error}",
            archive_path.display()
        )
    })
}

#[cfg(target_os = "macos")]
fn remove_macos_quarantine(path: &Path) -> Result<(), String> {
    let status = Command::new("/usr/bin/xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(path)
        .status()
        .map_err(|error| {
            format!(
                "Could not strip macOS quarantine from verified component {}: {error}",
                path.display()
            )
        })?;
    if !status.success() {
        return Err(format!(
            "Could not strip macOS quarantine from verified component {}: xattr exited with {status}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn remove_macos_quarantine(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn write_install_marker(
    path: &Path,
    name: &str,
    version: &str,
    platform: &str,
    sha256: &str,
) -> Result<(), String> {
    let marker = serde_json::json!({
        "name": name,
        "version": version,
        "platform": platform,
        "sha256": sha256,
    });
    fs::write(
        path.join(".ghostex-component.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&marker).map_err(|error| error.to_string())?
        ),
    )
    .map_err(|error| format!("Could not write component install marker: {error}"))
}

fn installed_marker_matches(
    path: &Path,
    name: &str,
    version: &str,
    platform: &str,
    expected_sha256: Option<&str>,
) -> Result<bool, String> {
    if !path.is_dir() {
        return Ok(false);
    }
    let marker_path = path.join(".ghostex-component.json");
    let data = match fs::read_to_string(&marker_path) {
        Ok(data) => data,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "Could not read component marker {}: {error}",
                marker_path.display()
            ));
        }
    };
    let marker = serde_json::from_str::<serde_json::Value>(&data).map_err(|error| {
        format!(
            "Malformed component marker {}: {error}",
            marker_path.display()
        )
    })?;
    Ok(
        marker.get("name").and_then(serde_json::Value::as_str) == Some(name)
            && marker.get("version").and_then(serde_json::Value::as_str) == Some(version)
            && marker.get("platform").and_then(serde_json::Value::as_str) == Some(platform)
            && marker
                .get("sha256")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|sha256| {
                    valid_sha256(sha256)
                        && expected_sha256.is_none_or(|expected| sha256 == expected)
                }),
    )
}

fn directory_size(path: &Path) -> Result<u64, String> {
    let mut total = 0_u64;
    for entry in fs::read_dir(path)
        .map_err(|error| format!("Could not measure {}: {error}", path.display()))?
    {
        let entry =
            entry.map_err(|error| format!("Could not measure {}: {error}", path.display()))?;
        let metadata = entry
            .path()
            .symlink_metadata()
            .map_err(|error| format!("Could not measure {}: {error}", entry.path().display()))?;
        if metadata.is_dir() {
            total = total.saturating_add(directory_size(&entry.path())?);
        } else {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

fn prune_temporary_install_artifacts(version_root: &Path) {
    let Ok(entries) = fs::read_dir(version_root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(".download-")
            && !name.starts_with(".install-")
            && !name.starts_with(".previous-")
            && !name.starts_with(".bd-download-")
            && !name.starts_with(".bd-extract-")
        {
            continue;
        }
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => {
                let _ = fs::remove_dir_all(path);
            }
            Ok(_) => {
                let _ = fs::remove_file(path);
            }
            Err(_) => {}
        }
    }
}

fn prune_other_versions(component_root: &Path, retained_version: &str) -> Result<(), String> {
    let entries = match fs::read_dir(component_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Could not prune component versions under {}: {error}",
                component_root.display()
            ));
        }
    };
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Could not inspect component version: {error}"))?;
        let name = entry.file_name();
        if name.to_string_lossy() == retained_version
            || !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
        {
            continue;
        }
        fs::remove_dir_all(entry.path()).map_err(|error| {
            format!(
                "Could not prune old component version {}: {error}",
                entry.path().display()
            )
        })?;
    }
    Ok(())
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn object<'a>(
    value: &'a serde_json::Value,
    label: &str,
) -> Result<&'a serde_json::Map<String, serde_json::Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("Malformed sealed on-demand manifest: {label} must be an object"))
}

fn nonempty_string(value: Option<&serde_json::Value>, label: &str) -> Result<String, String> {
    value
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            format!("Malformed sealed on-demand manifest: {label} must be a non-empty string")
        })
}

fn identifier(value: Option<&serde_json::Value>, label: &str) -> Result<String, String> {
    let value = nonempty_string(value, label)?;
    require_identifier(&value, label)?;
    Ok(value)
}

fn asset_name(value: Option<&serde_json::Value>, label: &str) -> Result<String, String> {
    let value = nonempty_string(value, label)?;
    if value.contains('/') || value.contains('\\') || value.contains("..") {
        return Err(format!(
            "Malformed sealed on-demand manifest: {label} must be a plain file name"
        ));
    }
    Ok(value)
}

fn sha256(value: Option<&serde_json::Value>, label: &str) -> Result<String, String> {
    let value = nonempty_string(value, label)?;
    if !valid_sha256(&value) {
        return Err(format!(
            "Malformed sealed on-demand manifest: {label} must be 64 lowercase hex characters"
        ));
    }
    Ok(value)
}

fn unsigned(value: Option<&serde_json::Value>, label: &str) -> Result<u64, String> {
    value.and_then(serde_json::Value::as_u64).ok_or_else(|| {
        format!("Malformed sealed on-demand manifest: {label} must be a non-negative integer")
    })
}

fn require_identifier(value: &str, label: &str) -> Result<(), String> {
    if valid_identifier(value) {
        Ok(())
    } else {
        Err(format!(
            "Malformed sealed on-demand manifest: {label} must be an identifier"
        ))
    }
}

fn require_cache_file_name(value: &str) -> Result<(), String> {
    if value.is_empty() || Path::new(value).components().count() != 1 || matches!(value, "." | "..")
    {
        return Err("Release asset cache file name must be a single file name".to_string());
    }
    Ok(())
}

fn cached_executable_is_ready(path: &Path, size_bytes: u64) -> bool {
    if size_bytes == 0 {
        return false;
    }
    #[cfg(unix)]
    return path
        .metadata()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false);
    #[cfg(not(unix))]
    return true;
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn missing(label: &str) -> String {
    format!("Malformed sealed on-demand manifest: missing {label}")
}
