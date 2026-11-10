use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

const MANIFEST_SCHEMA_VERSION: u64 = 2;
pub(crate) const CODE_SERVER_COMPONENT_NAME: &str = "code-server";

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
    /// Ghostex's signature over this archive (base64 Ed25519, `component_store::signature`).
    pub signature: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentDefinition {
    pub name: String,
    pub component_version: String,
    pub download_tag: String,
    /// CDXC:Release 2026-09-16 WHY:
    /// Component tags are published to a separate repository (maddada/ghostex-components) while the manifest's top-level `githubRepo` still names the app release that serves the version-scoped `assets`.
    /// Manifests sealed before the move carry no per-component repository and keep downloading from the top-level one, so `None` must resolve to `OnDemandManifest::github_repo`.
    /// SEE-ALSO: tooling/release-gpui/components-repo.mjs, tooling/release-gpui/on-demand-manifest.mjs.
    pub github_repo: Option<String>,
    pub platforms: HashMap<String, ComponentPlatformAsset>,
}

impl ComponentDefinition {
    /// The repository this component downloads from.
    pub fn download_repo<'a>(&'a self, manifest_repo: &'a str) -> &'a str {
        self.github_repo.as_deref().unwrap_or(manifest_repo)
    }
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
    #[allow(dead_code)] // wire shape: the ComponentStoreProgress JSON projection, kept next to the struct it serialises
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

/// An install of a component's other version, found by `ComponentStore::previous_install_for_platform`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviousInstall {
    pub version: String,
    /// The SHA-256 of the archive it was unpacked from, as its install marker records it.
    pub sha256: String,
    /// `<store>/<name>/<version folder>/<platform>`.
    pub path: PathBuf,
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
            let component_github_repo = component
                .get("githubRepo")
                .map(|value| {
                    let repo =
                        nonempty_string(Some(value), &format!("components.{key}.githubRepo"))?;
                    if repo.matches('/').count() != 1
                        || repo.split('/').any(|part| !valid_identifier(part))
                    {
                        return Err(format!(
                            "Malformed sealed on-demand manifest: components.{key}.githubRepo must have owner/repository form"
                        ));
                    }
                    Ok(repo)
                })
                .transpose()?;
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
                        signature: platform_asset
                            .get("signature")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string),
                    },
                );
            }
            components.insert(
                key.clone(),
                ComponentDefinition {
                    name,
                    component_version,
                    download_tag,
                    github_repo: component_github_repo,
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

pub(crate) fn require_identifier(value: &str, label: &str) -> Result<(), String> {
    if valid_identifier(value) {
        Ok(())
    } else {
        Err(format!(
            "Malformed sealed on-demand manifest: {label} must be an identifier"
        ))
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub(crate) fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn missing(label: &str) -> String {
    format!("Malformed sealed on-demand manifest: missing {label}")
}
