//! Claude Swap on Windows comes from Ghostex's own build of it, pinned here.

use std::path::PathBuf;

use crate::managed_tools::{self, Log};

/// CDXC:AgentProviders 2026-10-16 DECISION:
/// User: "f it let's ship our own fork of claude swap that's just used for windows until this is fixed, add it in the dependencies folder in this repo and upload it somewhere so on windows we download that version that includes windows support but on other oses you can dl the regular one not forked one". No released Claude Swap runs `--share-history` on Windows (`share_history_support` in launch.rs), so on Windows Ghostex installs, updates and reinstalls Claude Swap from this wheel: upstream 0.28.0 plus realiti4/claude-swap#429 (sharing through a junction and a hard link), #430 (folder trust carried into session profiles) and #448 (no suspended git.exe left by the auth-status probe), built from the `ghostex` branch of maddada/claude-swap (`.dependencies/claude-swap`) and published on that fork's GitHub releases. macOS and Linux keep PyPI's claude-swap. A Windows cswap that refuses sharing is replaced by this build the moment a launch finds it (launch.rs). Remove this file, the submodule and the Windows branches that call it once an upstream release supports `--share-history` on Windows.
/// WHY: `uv tool install` ignores a `#sha256=` fragment on a wheel URL (a wrong hash installed fine on 2026-10-16), so Ghostex downloads the wheel itself, checks it against `SHA256` and installs the checked file.
pub(crate) const VERSION: &str = "0.28.0.post1+ghostex";
const WHEEL: &str = "claude_swap-0.28.0.post1+ghostex-py3-none-any.whl";
pub(crate) const URL: &str = "https://github.com/maddada/claude-swap/releases/download/v0.28.0.post1%2Bghostex/claude_swap-0.28.0.post1%2Bghostex-py3-none-any.whl";
const SHA256: &str = "86edfab5622adbdba906d80cbc3fe4dd142c6ac9269925172a837bbeabdf0d5f";

/// Whether this computer installs Ghostex's build of Claude Swap instead of PyPI's.
pub(crate) fn applies() -> bool {
    cfg!(windows)
}

/// The checked wheel in Ghostex's tools folder, downloaded first when it is missing or changed.
pub(crate) fn wheel(log: &Log) -> Result<PathBuf, String> {
    let dir = managed_tools::paths::tools_root().join("claude-swap");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not create {}: {error}", dir.display()))?;
    let path = dir.join(WHEEL);
    if managed_tools::download::sha256_file(&path).is_ok_and(|hash| hash == SHA256) {
        return Ok(path);
    }
    let partial = dir.join(format!("{WHEEL}.part"));
    managed_tools::download::download(
        URL,
        &partial,
        "Ghostex's Windows build of Claude Swap",
        log,
    )?;
    managed_tools::download::verify(&partial, SHA256, "Claude Swap", log)?;
    std::fs::rename(&partial, &path)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    Ok(path)
}
