//! Where the GBots runtime comes from, where it is installed, and the Node that runs it.
//!
//! The runtime is an on-demand component like CEF and the Code editor: the release seals its
//! version and per-platform archive (name, SHA-256, size) into the app's
//! `on-demand-resources.json` (`components.gbots`), and this computer's gxserver downloads it
//! into `<components>/gbots/<component version>/<platform>/` when GBots is turned on here
//! (download.rs). The folder holds `gbots-runtime.json`, `server/openmausbot.js` and the pinned
//! Node (`node/bin/node`). A build without a sealed GBots component (a development build) uses
//! `<components>/gbots/dev/<platform>/`, which `cargo xtask gbots-runtime --install` fills.
//!
//! CDXC:GBots 2026-11-07 DECISION:
//! User (Q1505): ship a pinned Node 24 inside the GBots component, signed and notarised like the Code editor's node, so a computer needs no Node of its own. The version and checksums are pinned in tooling/gbots/upstream.json.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The component's name in the sealed manifest and the component store.
pub(crate) const GBOTS_COMPONENT: &str = "gbots";

/// The oldest Node the runtime runs on (OpenMausBot's `engines.node`).
const MIN_NODE_MAJOR: u64 = 24;

/// One installed runtime.
#[derive(Clone, Debug)]
pub(crate) struct InstalledRuntime {
    pub(crate) root: PathBuf,
    pub(crate) entry: PathBuf,
    /// The Node the component ships; `None` for a development build, which uses the PATH's.
    pub(crate) node: Option<PathBuf>,
    pub(crate) version: String,
}

/// The sealed GBots component for this platform (crate::components).
pub(crate) use crate::components::SealedComponent;

/// The component store's GBots folder.
pub(crate) fn component_root() -> PathBuf {
    crate::components::component_root(GBOTS_COMPONENT)
}

/// `<os>-<arch>`, the platform folder and asset suffix the component store names.
pub(crate) fn platform_dir() -> String {
    crate::components::platform_dir()
}

/// The sealed GBots component for this platform, or `None` when the app has none (a
/// development build, or an app released before GBots).
pub(crate) fn sealed_component() -> Option<SealedComponent> {
    crate::components::sealed(GBOTS_COMPONENT)
}

/// Where this gxserver's runtime goes: the sealed component's version, else `dev`.
pub(crate) fn install_dir() -> PathBuf {
    let version = sealed_component()
        .map(|component| component.component_version)
        .unwrap_or_else(|| "dev".to_string());
    component_root().join(version).join(platform_dir())
}

/// The runtime when it is installed and readable.
pub(crate) fn installed_runtime() -> Option<InstalledRuntime> {
    read_runtime(&install_dir())
}

pub(crate) fn read_runtime(root: &Path) -> Option<InstalledRuntime> {
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("gbots-runtime.json")).ok()?)
            .ok()?;
    let entry = root.join(manifest.get("entry").and_then(Value::as_str)?);
    let node = manifest
        .get("node")
        .and_then(Value::as_str)
        .map(|node| root.join(node));
    if node.as_ref().is_some_and(|node| !node.is_file()) || !entry.is_file() {
        return None;
    }
    Some(InstalledRuntime {
        root: root.to_path_buf(),
        entry,
        node,
        version: manifest
            .get("componentVersion")
            .or_else(|| manifest.get("runtimeVersion"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

/// The Node the sidecar runs on: the component's own, else (a development build)
/// `GHOSTEX_GBOTS_NODE` or `node` on the PATH a terminal gets, when it is Node 24 or newer.
pub(crate) fn resolve_node(runtime: &InstalledRuntime) -> Result<PathBuf, String> {
    if let Some(node) = &runtime.node {
        return Ok(node.clone());
    }
    let node = match std::env::var_os("GHOSTEX_GBOTS_NODE") {
        Some(path) => PathBuf::from(path),
        None => std::env::split_paths(&sidecar_path(&[]))
            .map(|dir| dir.join(if cfg!(windows) { "node.exe" } else { "node" }))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| {
                format!("GBots needs Node {MIN_NODE_MAJOR} or newer, and no node was found.")
            })?,
    };
    let output = std::process::Command::new(&node)
        .arg("--version")
        .output()
        .map_err(|error| format!("Could not run {}: {error}", node.display()))?;
    let version = String::from_utf8_lossy(&output.stdout);
    let major = version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|major| major.parse::<u64>().ok())
        .unwrap_or(0);
    if major < MIN_NODE_MAJOR {
        return Err(format!(
            "GBots needs Node {MIN_NODE_MAJOR} or newer; {} is {}.",
            node.display(),
            version.trim()
        ));
    }
    Ok(node)
}

/// The PATH the sidecar and the bots' agents run with: `prefix` first, then the PATH a terminal
/// gets (so `claude`, `codex` and `ghostex` resolve as they do in a session).
pub(crate) fn sidecar_path(prefix: &[PathBuf]) -> std::ffi::OsString {
    crate::managed_tools::run::job_path(prefix)
}
