//! Which agent binary a background probe may run.
//!
//! CDXC:AgentProviders 2026-10-28 WHY:
//! On a fresh Omarchy machine Grok installed itself a minute after Ghostex's first launch, without
//! the user picking Grok: gxserver's startup model probe ran `grok models`, and Omarchy's `grok` is a
//! wrapper that runs `mise use -g "npm:@xai-official/grok"` and then `mise x … -- grok`. Omarchy
//! ships one for most agents (cursor-agent, gemini, pi, claude, codex, hermes…); npx/bunx shims and
//! mise shims of a tool with no installed version do the same. A probe Ghostex runs on its own (model
//! lineups, versions, sign-in state, hook bootstrap) must never install anything, so it runs the
//! binary only when that cannot install: a real binary or script as today, the install a wrapper
//! would run once mise has it (`mise which` installs nothing), and nothing at all ("not installed
//! yet") otherwise. Starting an agent the user picked still types the wrapper's name, so Omarchy
//! installs it then, and the next probe finds the real install.

use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

/// The most a launcher wrapper weighs; anything larger is a real program.
const WRAPPER_MAX_BYTES: u64 = 16 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Running it cannot install anything.
    Real,
    /// A mise shim: mise installs a missing tool version when it runs one.
    MiseShim,
    /// An asdf shim whose tool versions are all missing.
    AsdfShimMissing,
    /// A small script that installs or fetches its tool (`mise use`, `mise x`, `npx`, `bunx`…).
    Wrapper,
}

/// The binary a background probe for `binary` may run in place of `path` (what PATH resolved),
/// or `None` while the agent is only an install-on-demand wrapper.
pub(crate) fn probe_target(path: &str, binary: &str, home: &Path) -> Option<String> {
    probe_target_with(path, home, || mise_installed(binary, home))
}

/// `probe_target` with the `mise which` lookup passed in.
fn probe_target_with(
    path: &str,
    home: &Path,
    mise_installed: impl FnOnce() -> Option<String>,
) -> Option<String> {
    match classify(Path::new(path), home) {
        Kind::Real => Some(path.to_string()),
        // Installed: keep running the shim, which gives the tool the environment mise sets up.
        Kind::MiseShim => mise_installed().map(|_| path.to_string()),
        Kind::AsdfShimMissing => None,
        Kind::Wrapper => mise_installed()
            .filter(|installed| classify(Path::new(installed), home) == Kind::Real)
            .filter(|installed| !same_file(Path::new(installed), Path::new(path))),
    }
}

pub(crate) fn probe_target_path(path: PathBuf, binary: &str, home: &Path) -> Option<PathBuf> {
    probe_target(&path.to_string_lossy(), binary, home).map(PathBuf::from)
}

pub(crate) fn classify(path: &Path, home: &Path) -> Kind {
    if is_mise_shim(path) {
        return Kind::MiseShim;
    }
    let Some(text) = small_script(path) else {
        return Kind::Real;
    };
    if let Some(versions) = asdf_shim_versions(&text) {
        return if versions.iter().any(|(plugin, version)| {
            asdf_data_dirs(home)
                .iter()
                .any(|dir| dir.join("installs").join(plugin).join(version).is_dir())
        }) {
            Kind::Real
        } else {
            Kind::AsdfShimMissing
        };
    }
    if installs_on_demand(&text) {
        Kind::Wrapper
    } else {
        Kind::Real
    }
}

fn is_mise_shim(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    if normalized.contains("/mise/shims/") {
        return true;
    }
    std::fs::canonicalize(path).is_ok_and(|real| {
        matches!(
            real.file_name().and_then(|name| name.to_str()),
            Some("mise" | "mise.exe")
        )
    })
}

/// The text of a launcher-sized shell, cmd or PowerShell script; `None` for binaries and programs.
fn small_script(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > WRAPPER_MAX_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    let windows_script = matches!(extension.as_deref(), Some("cmd" | "bat" | "ps1"));
    (windows_script || is_shell_shebang(text.lines().next().unwrap_or_default())).then_some(text)
}

/// `#!/bin/bash`, `#!/usr/bin/env -S zsh -f` and the like; node, python and other programs are not wrappers.
fn is_shell_shebang(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("#!") else {
        return false;
    };
    rest.split_whitespace()
        .filter(|word| !word.starts_with('-'))
        .map(|word| word.rsplit('/').next().unwrap_or(word))
        .find(|word| *word != "env")
        .is_some_and(|program| matches!(program, "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish"))
}

/// The `(plugin, version)` pairs an asdf shim names, or `None` when the script is not an asdf shim.
fn asdf_shim_versions(text: &str) -> Option<Vec<(String, String)>> {
    let versions = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("# asdf-plugin:"))
        .filter_map(|rest| {
            let mut words = rest.split_whitespace();
            Some((words.next()?.to_string(), words.next()?.to_string()))
        })
        .collect::<Vec<_>>();
    (!versions.is_empty() || text.contains("asdf exec")).then_some(versions)
}

fn asdf_data_dirs(home: &Path) -> Vec<PathBuf> {
    std::env::var_os("ASDF_DATA_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .into_iter()
        .chain([home.join(".asdf")])
        .collect()
}

/// Whether a script's commands install or fetch a tool before running it.
pub(crate) fn installs_on_demand(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .filter(|line| {
            !line.starts_with('#')
                && !line.starts_with("::")
                && !line.to_ascii_lowercase().starts_with("rem ")
        })
        .any(|line| {
            let words = line
                .split(|ch: char| ch.is_whitespace() || "\"'`;|&()$={}".contains(ch))
                .filter(|word| !word.is_empty())
                .map(command_name)
                .collect::<Vec<_>>();
            words.iter().enumerate().any(|(index, word)| {
                let next = || {
                    words[index + 1..]
                        .iter()
                        .find(|word| !word.starts_with('-'))
                        .map(String::as_str)
                };
                match word.as_str() {
                    "npx" | "bunx" | "pnpx" | "uvx" => true,
                    "mise" | "rtx" => {
                        matches!(next(), Some("use" | "u" | "install" | "i" | "x" | "exec"))
                    }
                    "asdf" => matches!(next(), Some("install")),
                    "bun" | "npm" => matches!(next(), Some("x" | "exec")),
                    "pnpm" | "yarn" => matches!(next(), Some("dlx")),
                    "pipx" => matches!(next(), Some("run")),
                    "uv" => {
                        matches!(next(), Some("tool"))
                            && words[index + 1..].iter().any(|word| word == "run")
                    }
                    _ => false,
                }
            })
        })
}

/// `/usr/bin/npx`, `npx.cmd` and `mise.exe` all name the command before the folder and extension.
fn command_name(word: &str) -> String {
    let name = word
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(word)
        .to_ascii_lowercase();
    for extension in [".exe", ".cmd", ".bat", ".ps1"] {
        if let Some(stem) = name.strip_suffix(extension) {
            return stem.to_string();
        }
    }
    name
}

/// The installed binary mise runs for `binary`; `mise which` only reads, it never installs.
fn mise_installed(binary: &str, home: &Path) -> Option<String> {
    let mise = super::probing::resolve_cli_command("mise", home)?;
    let mut command = crate::platform::process::background_command(&mise);
    command
        .args(["which", binary])
        .current_dir(home)
        .env("HOME", home);
    #[cfg(not(windows))]
    command.env(
        "PATH",
        super::probing::normalize_gxserver_process_path(
            std::env::var("PATH").ok().as_deref(),
            home,
        ),
    );
    super::probing::run_command_stdout_with_timeout(command, Duration::from_secs(3))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !value.contains(['\r', '\n']))
}

fn same_file(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

#[cfg(test)]
#[path = "on_demand_tests.rs"]
mod tests;
