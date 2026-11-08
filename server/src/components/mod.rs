//! The on-demand components gxserver downloads itself (GBots, Collab): where the release sealed
//! them, where they are installed, and the checked download.
//!
//! The release seals each component's version and per-platform archive (name, SHA-256, size)
//! into the app's `on-demand-resources.json` (`components.<name>`, written by
//! tooling/release-gpui/on-demand-manifest.mjs) and publishes the archive to the components
//! repository. A component is downloaded only when the owner turns it on here, into
//! `<components>/<name>/<component version>/<platform>/`. The archive is checked against the size
//! and SHA-256 the release sealed before anything is unpacked, unpacked into a staging folder,
//! checked by its caller to hold what it should, and only then moved into place, so a broken
//! download never leaves a half-installed component.
//!
//! The desktop's component store (apps/desktop/src/component_store/) does the same for the Code
//! editor's code-server and CEF, which the desktop app runs.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest as _, Sha256};

/// One sealed component for this platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SealedComponent {
    pub(crate) name: String,
    pub(crate) component_version: String,
    pub(crate) url: String,
    pub(crate) asset_name: String,
    pub(crate) sha256: String,
    pub(crate) size_bytes: u64,
}

/// The component store: `GHOSTEX_COMPONENT_STORE_DIR` when set (as the desktop's component store
/// reads it), else the data folder's `components`.
pub(crate) fn store_root() -> PathBuf {
    std::env::var_os("GHOSTEX_COMPONENT_STORE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            ghostex_paths::GhostexPaths::resolve()
                .data_dir
                .join("components")
        })
}

/// One component's folder in the store.
pub(crate) fn component_root(name: &str) -> PathBuf {
    store_root().join(name)
}

/// `<os>-<arch>`, the platform folder and asset suffix the component store names.
pub(crate) fn platform_dir() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    format!("{os}-{arch}")
}

/// The app's sealed `on-demand-resources.json`: `GHOSTEX_ON_DEMAND_MANIFEST`, else the first one
/// beside or above gxserver's own executable (macOS keeps it in `Contents/Resources/Web/`, above
/// `Web/gxserver/GhostexServer.app`; Linux and Windows beside the app under `resources/`).
pub(crate) fn manifest_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GHOSTEX_ON_DEMAND_MANIFEST") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let executable = std::env::current_exe().ok()?;
    let executable = std::fs::canonicalize(&executable).unwrap_or(executable);
    executable.ancestors().skip(1).take(7).find_map(|dir| {
        [
            "on-demand-resources.json",
            "Web/on-demand-resources.json",
            "resources/on-demand-resources.json",
            "resources/Web/on-demand-resources.json",
        ]
        .iter()
        .map(|candidate| dir.join(candidate))
        .find(|candidate| candidate.is_file())
    })
}

/// The sealed component `name` for this platform, or `None` when the app has none (a
/// development build, or an app released before it).
pub(crate) fn sealed(name: &str) -> Option<SealedComponent> {
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(manifest_path()?).ok()?).ok()?;
    sealed_in(&manifest, name, &platform_dir())
}

pub(crate) fn sealed_in(manifest: &Value, name: &str, platform: &str) -> Option<SealedComponent> {
    let component = manifest.pointer(&format!("/components/{name}"))?;
    let component_version = component.get("componentVersion")?.as_str()?.to_string();
    let tag = component.get("downloadTag")?.as_str()?.to_string();
    let repo = component
        .get("githubRepo")
        .or_else(|| manifest.get("githubRepo"))?
        .as_str()?
        .to_string();
    let platform = component.pointer(&format!("/platforms/{platform}"))?;
    let asset_name = platform.get("assetName")?.as_str()?.to_string();
    let sha256 = platform.get("sha256")?.as_str()?.to_ascii_lowercase();
    let size_bytes = platform.get("sizeBytes")?.as_u64()?;
    let safe = |text: &str| !text.is_empty() && !text.contains(['/', '\\']) && !text.contains("..");
    if !safe(&component_version) || !safe(&asset_name) || !safe(&tag) || sha256.len() != 64 {
        return None;
    }
    let base = std::env::var("GHOSTEX_ON_DEMAND_BASE_URL")
        .unwrap_or_else(|_| format!("https://github.com/{repo}/releases/download"));
    Some(SealedComponent {
        name: name.to_string(),
        url: format!("{}/{tag}/{asset_name}", base.trim_end_matches('/')),
        component_version,
        asset_name,
        sha256,
        size_bytes,
    })
}

pub(crate) fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default()
}

/// Streams the archive to `path`, hashing as it goes, and checks its size and SHA-256 against
/// what the release sealed. `progress(done, total)` follows the download. `title` names the
/// component in messages ("GBots", "Collab").
pub(crate) fn fetch_verified(
    component: &SealedComponent,
    title: &str,
    path: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<(), String> {
    let response = crate::http_agent::agent()
        .get(&component.url)
        .config()
        .timeout_global(None)
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .build()
        .call()
        .map_err(|error| {
            if crate::http_agent::never_connected(&error) {
                format!("Could not reach GitHub to download {title}. Try again in a moment.")
            } else {
                format!("The {title} download failed: {error}")
            }
        })?;
    let mut reader = response.into_body().into_reader();
    let mut file = std::fs::File::create(path)
        .map_err(|error| format!("Could not write the {title} download: {error}"))?;
    let mut hasher = Sha256::new();
    let mut done = 0_u64;
    let mut buffer = vec![0_u8; 256 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("The {title} download stopped: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|error| format!("Could not write the {title} download: {error}"))?;
        done += read as u64;
        if done > component.size_bytes {
            return Err(format!(
                "The {title} download is larger than this release expects."
            ));
        }
        progress(done, component.size_bytes);
    }
    file.flush()
        .map_err(|error| format!("Could not write the {title} download: {error}"))?;
    if done != component.size_bytes {
        return Err(format!(
            "The {title} download ended early ({done} of {} bytes).",
            component.size_bytes
        ));
    }
    let digest = hex::encode(hasher.finalize());
    if digest != component.sha256 {
        return Err(format!(
            "The {title} download does not match this release's checksum."
        ));
    }
    Ok(())
}

/// Unpacks `archive` into `staging`, lets `check` look at it, and moves it to `destination`
/// (replacing what was there).
pub(crate) fn unpack_into_place(
    archive: &Path,
    staging: &Path,
    destination: &Path,
    title: &str,
    check: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<(), String> {
    std::fs::create_dir_all(staging).map_err(|error| error.to_string())?;
    let output = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(staging)
        .output()
        .map_err(|error| format!("Could not unpack {title}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Could not unpack {title}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    check(staging)?;
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if destination.exists() {
        std::fs::remove_dir_all(destination).map_err(|error| error.to_string())?;
    }
    std::fs::rename(staging, destination)
        .map_err(|error| format!("Could not install {title}: {error}"))
}

/// Downloads the sealed `component` and installs it at `destination`, through a download and a
/// staging folder beside it in the component's folder, removed whatever happens.
pub(crate) fn download_and_install(
    component: &SealedComponent,
    title: &str,
    destination: &Path,
    progress: &dyn Fn(u64, u64),
    installing: &dyn Fn(),
    check: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let root = component_root(&component.name);
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("Could not make {}: {error}", root.display()))?;
    let stamp = format!("{}-{}", std::process::id(), unique());
    let archive = root.join(format!(".download-{stamp}.tar.gz"));
    let staging = root.join(format!(".install-{stamp}"));
    let result = fetch_verified(component, title, &archive, progress).and_then(|()| {
        installing();
        unpack_into_place(&archive, &staging, destination, title, check)
    });
    let _ = std::fs::remove_file(&archive);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

/// Removes the component's other versions once `installed` (`<root>/<version>/<platform>`) is in
/// place (never `dev`).
pub(crate) fn prune_other_versions(name: &str, installed: &Path) {
    let keep: Option<PathBuf> = installed.parent().map(Path::to_path_buf);
    let Ok(entries) = std::fs::read_dir(component_root(name)) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !path.is_dir()
            || file_name.starts_with('.')
            || file_name == "dev"
            || keep.as_deref() == Some(path.as_path())
        {
            continue;
        }
        let _ = std::fs::remove_dir_all(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_sealed_component_needs_safe_names_and_a_full_checksum() {
        let manifest = |asset: &str, sha: &str| {
            json!({
                "githubRepo": "maddada/ghostex-components",
                "components": {"collab": {
                    "componentVersion": "1.0.0-abc",
                    "downloadTag": "collab-1.0.0-abc",
                    "platforms": {"darwin-arm64": {"assetName": asset, "sha256": sha, "sizeBytes": 10}}
                }}
            })
        };
        let good = sealed_in(
            &manifest("collab-1.0.0-abc-darwin-arm64.tar.gz", &"a".repeat(64)),
            "collab",
            "darwin-arm64",
        )
        .expect("sealed");
        assert!(good
            .url
            .ends_with("/collab-1.0.0-abc/collab-1.0.0-abc-darwin-arm64.tar.gz"));
        assert!(sealed_in(
            &manifest("../x.tar.gz", &"a".repeat(64)),
            "collab",
            "darwin-arm64"
        )
        .is_none());
        assert!(sealed_in(&manifest("x.tar.gz", "abc"), "collab", "darwin-arm64").is_none());
        assert!(sealed_in(
            &manifest("x.tar.gz", &"a".repeat(64)),
            "collab",
            "linux-x64"
        )
        .is_none());
        assert!(sealed_in(
            &manifest("x.tar.gz", &"a".repeat(64)),
            "gbots",
            "darwin-arm64"
        )
        .is_none());
    }
}
