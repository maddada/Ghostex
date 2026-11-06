//! Where the GBots runtime is installed and the Node that runs it.
//!
//! The runtime is an on-demand component like CEF and the Code editor: `<components>/gbots/<version>/<platform>/`
//! holds `gbots-runtime.json` and `server/openmausbot.js`. A development build
//! (`cargo xtask gbots-runtime`) installs into the same place.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The runtime version this gxserver is built for (tooling/gbots/upstream.json `runtimeVersion`).
pub(crate) const GBOTS_RUNTIME_VERSION: &str = "0.1.143-gx1";

/// The oldest Node the runtime runs on (OpenMausBot's `engines.node`).
const MIN_NODE_MAJOR: u64 = 24;

/// One installed runtime.
#[derive(Clone, Debug)]
pub(crate) struct InstalledRuntime {
    pub(crate) root: PathBuf,
    pub(crate) entry: PathBuf,
    pub(crate) version: String,
}

/// The component store's GBots folder: `GHOSTEX_COMPONENT_STORE_DIR` when set (as the desktop's
/// component store reads it), else the data folder's `components/gbots`.
pub(crate) fn component_root() -> PathBuf {
    let root = std::env::var_os("GHOSTEX_COMPONENT_STORE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            ghostex_paths::GhostexPaths::resolve()
                .data_dir
                .join("components")
        });
    root.join("gbots")
}

/// `<os>-<arch>`, the platform folder the component store names.
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

/// Where this gxserver's pinned runtime goes.
pub(crate) fn install_dir() -> PathBuf {
    component_root()
        .join(GBOTS_RUNTIME_VERSION)
        .join(platform_dir())
}

/// The pinned runtime when it is installed and readable.
pub(crate) fn installed_runtime() -> Option<InstalledRuntime> {
    read_runtime(&install_dir())
}

fn read_runtime(root: &Path) -> Option<InstalledRuntime> {
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("gbots-runtime.json")).ok()?)
            .ok()?;
    let entry = root.join(manifest.get("entry").and_then(Value::as_str)?);
    let version = manifest
        .get("runtimeVersion")
        .and_then(Value::as_str)?
        .to_string();
    (entry.is_file() && version == GBOTS_RUNTIME_VERSION).then(|| InstalledRuntime {
        root: root.to_path_buf(),
        entry,
        version,
    })
}

/// The Node the sidecar runs on: `GHOSTEX_GBOTS_NODE`, else `node` on the PATH a terminal gets,
/// when it is Node 24 or newer. The error says what is missing.
pub(crate) fn resolve_node() -> Result<PathBuf, String> {
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
