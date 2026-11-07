//! Where Claude Swap comes from: Ghostex's own build of it while upstream lacks two fixes, else PyPI.

use std::path::PathBuf;

use crate::managed_tools::{self, Log};

/// CDXC:AgentProviders 2026-11-07 DECISION:
/// User (Q1680, Q1681): "Our build on top of 1.0.0" for Windows, and for macOS and Linux "use the one we are doing for windows above also for mac/linux for now"; "please send those 2 fixes to cswap saying why we need them and issues we found without them. i dont want to fork cswap." Upstream claude-swap 1.0.0 shares history on Windows itself (realiti4/claude-swap#559), which ends the Windows-only build of 2026-10-16 (#429 and #430 are dropped: Ghostex trusts session folders itself, agent_folder_trust/stores.rs). Two things Ghostex relies on are still missing upstream, so on every OS Ghostex installs, updates and reinstalls this wheel: upstream 1.0.0 plus realiti4/claude-swap#448 (no suspended git.exe left by the auth-status probe on Windows, about 11 in 20 runs) and realiti4/claude-swap#563 (`cswap run N` uses the slot's profile when the default login names N but holds no usable token; since 0.30.0 a preset CLAUDE_CONFIG_DIR no longer gets past cswap's default-login shortcut, so default_login.rs's route stopped working and every new session on that account hit "Login expired"). Built from the `ghostex` branch of maddada/claude-swap (`.dependencies/claude-swap`) and published on that fork's GitHub releases. TEMPORARY: once an upstream release carries both PRs, set `GHOSTEX_BUILD` to false (Ghostex then installs PyPI's claude-swap everywhere and moves this build's installs to it), then remove the constants below, the submodule and its .gitmodules entry.
/// WHY: `uv tool install` ignores a `#sha256=` fragment on a wheel URL (a wrong hash installed fine on 2026-10-16), so Ghostex downloads the wheel itself, checks it against `SHA256` and installs the checked file.
const GHOSTEX_BUILD: bool = true;
const VERSION: &str = "1.0.0.post1+ghostex";
const WHEEL: &str = "claude_swap-1.0.0.post1+ghostex-py3-none-any.whl";
const URL: &str = "https://github.com/maddada/claude-swap/releases/download/v1.0.0.post1%2Bghostex/claude_swap-1.0.0.post1%2Bghostex-py3-none-any.whl";
const SHA256: &str = "4718451876ee96e18b8295c427358683997ddef1840e58008d9280ad634f4596";

/// The version Ghostex keeps installed, or `None` when it follows PyPI's latest release.
pub(crate) fn pinned() -> Option<&'static str> {
    GHOSTEX_BUILD.then_some(VERSION)
}

/// What `uv tool install` / `pipx install` installs, for the Accounts page's copyable command.
pub(crate) fn install_command() -> String {
    match pinned() {
        Some(_) => format!("uv tool install --force {URL}"),
        None => "uv tool install claude-swap".to_string(),
    }
}

/// The argument that installs the wanted Claude Swap: the checked wheel (downloaded first when
/// missing or changed), or PyPI's `claude-swap`.
pub(crate) fn install_spec(log: &Log) -> Result<String, String> {
    if pinned().is_none() {
        return Ok("claude-swap".to_string());
    }
    Ok(wheel(log)?.to_string_lossy().into_owned())
}

/// `uv` arguments that install `spec` over whatever Claude Swap is installed. Following PyPI,
/// `--upgrade` takes its latest release even over a tool uv first installed from this wheel.
pub(crate) fn uv_install_args(spec: &str) -> Vec<&str> {
    let mut args = vec!["tool", "install", "--force"];
    if pinned().is_none() {
        args.push("--upgrade");
    }
    args.push(spec);
    args
}

/// Whether Settings offers an update for the installed `version`: any version but the pinned
/// build (they differ only in their `.postN+ghostex` part), else an older one than PyPI's or one
/// of Ghostex's builds.
pub(crate) fn outdated(installed: &str, latest: &str) -> bool {
    match pinned() {
        Some(version) => installed != version,
        None => installed.contains("+ghostex") || managed_tools::tools::newer(latest, installed),
    }
}

/// Whether Ghostex replaces the installed `version` on its own (the hourly check in
/// claude_swap_switch.rs): any version but the pinned build, or, following PyPI, one of Ghostex's
/// builds. A PyPI install that is merely behind waits for the user's Update, as before.
pub(crate) fn must_replace(installed: &str) -> bool {
    match pinned() {
        Some(version) => installed != version,
        None => installed.contains("+ghostex"),
    }
}

fn wheel(log: &Log) -> Result<PathBuf, String> {
    let dir = managed_tools::paths::tools_root().join("claude-swap");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not create {}: {error}", dir.display()))?;
    let path = dir.join(WHEEL);
    if managed_tools::download::sha256_file(&path).is_ok_and(|hash| hash == SHA256) {
        return Ok(path);
    }
    let partial = dir.join(format!("{WHEEL}.part"));
    managed_tools::download::download(URL, &partial, "Ghostex's build of Claude Swap", log)?;
    managed_tools::download::verify(&partial, SHA256, "Claude Swap", log)?;
    std::fs::rename(&partial, &path)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pinned_build_replaces_every_other_version() {
        assert_eq!(pinned(), Some("1.0.0.post1+ghostex"));
        // PyPI's release, Ghostex's earlier Windows build and a newer upstream all move to it.
        assert!(outdated("1.0.0", "1.0.0"));
        assert!(outdated("0.29.0.post1+ghostex", "1.0.0"));
        assert!(outdated("1.0.1", "1.0.1"));
        assert!(!outdated("1.0.0.post1+ghostex", "1.0.0"));
        // An existing Windows install of the 0.29.0 build, or PyPI's 1.0.0 on macOS and Linux,
        // is moved by the hourly check without a click.
        assert!(must_replace("0.29.0.post1+ghostex"));
        assert!(must_replace("1.0.0"));
        assert!(!must_replace("1.0.0.post1+ghostex"));
        assert_eq!(
            uv_install_args("w.whl"),
            ["tool", "install", "--force", "w.whl"]
        );
    }

    #[test]
    fn the_wheel_constants_name_one_release() {
        assert!(WHEEL.contains(VERSION));
        assert!(URL.ends_with(&WHEEL.replace('+', "%2B")));
        assert!(URL.contains(&format!("/v{}/", VERSION.replace('+', "%2B"))));
        assert_eq!(SHA256.len(), 64);
        assert!(install_command().ends_with(URL));
    }
}
