//! Keeping every mise install a running process uses until nothing runs from it.
//!
//! CDXC:AgentProviders 2026-11-08 DECISION:
//! User report: every grep in every running Claude session printed "mise ERROR ugrep is not a valid shim" after Claude Code went from 2.1.318 to 2.1.319. Ghostex's update kept 2.1.318 (`mise upgrade --no-prune`, mise.rs `command`), but `--bump` moved the global config to 2.1.319, so 2.1.318 became "prunable", and a later `mise prune --yes` (picked on a "Free up disk space" page, 2026-11-08 02:18) deleted it, and 2.1.312, under sessions still running from them. Claude Code's shell helpers re-run `$CLAUDE_CODE_EXECPATH` (its own install path), so those sessions broke.
//! WHY: mise has no notion of a running process; `mise prune` keeps only what a tracked config names. So Ghostex keeps its own tracked config (`<tools>/mise-keep/mise.toml`, trusted, never anyone's working folder) that names every mise tool version a running process runs from, refreshed right after each mise update and on every Updates pass (30 minutes). Any `mise prune`, by the user, an agent or mise's own `upgrade.prune_after`, then leaves those versions alone. A version Ghostex's update replaced is uninstalled by a later pass once no process runs from it, no config names it and an hour has passed (a shell activated before the update can still hold the old folder on PATH), so `--no-prune` no longer piles old versions up.
//! Tools whose mise backend carries options (Grok's `http:` url) are not named: a config entry without those options might not parse. Windows is left out: a file in use there cannot be deleted.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};

/// mise install folder name (`claude`, `npm-openai-codex`) to the versions in use.
pub(crate) type InUse = BTreeMap<String, BTreeSet<String>>;

/// The mise runtimes whose versions hold globally installed packages rather than one tool.
const RUNTIMES: [&str; 4] = ["node", "bun", "pnpm", "npm"];
/// How long a replaced version stays at least, for shells activated before the update.
const RETIRE_GRACE_SECS: u64 = 60 * 60;

/// A version a Ghostex update replaced, to uninstall once nothing uses it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Retired {
    /// The install folder name under `installs/`.
    pub dir: String,
    /// The tool as mise names it (`claude`, `npm:@openai/codex`).
    pub tool: String,
    pub version: String,
    /// Unix seconds when the update replaced it.
    pub since: u64,
}

fn safe_name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
}

/// Every `<root>/<tool>/<version>/…` path in `text` (process program files and command lines).
pub(crate) fn installs_in_text(text: &str, roots: &[String]) -> InUse {
    let mut found = InUse::new();
    for root in roots {
        let prefix = format!("{}/", root.trim_end_matches('/'));
        let mut rest = text;
        while let Some(at) = rest.find(&prefix) {
            rest = &rest[at + prefix.len()..];
            let token = rest.split(char::is_whitespace).next().unwrap_or_default();
            let mut parts = token.split('/');
            let (Some(tool), Some(version), Some(inside)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if safe_name(tool) && safe_name(version) && !inside.is_empty() {
                found
                    .entry(tool.to_string())
                    .or_default()
                    .insert(version.to_string());
            }
        }
    }
    found
}

/// The tool name in an install's `.mise.backend.toml`, or `None` when its backend carries
/// options (`[opts]`) a bare config entry would lose.
pub(crate) fn backend_short(toml: &str) -> Option<String> {
    let mut short = None;
    for line in toml.lines().map(str::trim) {
        if line.starts_with('[') {
            return None;
        }
        if let Some(value) = line.strip_prefix("short") {
            let value = value.trim_start().strip_prefix('=')?.trim();
            short = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .map(str::to_string);
        }
    }
    short.filter(|short| {
        !short.starts_with('-')
            && short
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"@:/._-".contains(&byte))
    })
}

/// The tracked config that names every version in use (tool to versions).
pub(crate) fn keep_config(tools: &BTreeMap<String, BTreeSet<String>>) -> String {
    let mut text = String::from(
        "# Written by Ghostex: the mise tool versions running programs use, so `mise prune` keeps them.\n# Ghostex rewrites this file; nothing runs from this folder.\n[tools]\n",
    );
    for (tool, versions) in tools {
        let versions = versions
            .iter()
            .map(|version| format!("\"{version}\""))
            .collect::<Vec<_>>()
            .join(", ");
        text.push_str(&format!("\"{tool}\" = [{versions}]\n"));
    }
    text
}

/// The `(tool, version)` pairs `mise prune --dry-run` calls prunable
/// ("mise jq@1.7 is prunable: jq is required at 1.8.2 by …").
pub(crate) fn prunable(output: &str) -> BTreeSet<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let spec = line.strip_prefix("mise ").unwrap_or(line);
            let (spec, _) = spec.split_once(" is prunable")?;
            let (tool, version) = spec.trim().rsplit_once('@')?;
            Some((tool.to_string(), version.to_string()))
        })
        .collect()
}

/// The replaced versions a pass may uninstall now: an hour old, no running process uses them,
/// and mise itself calls them prunable (no config names them, the keep config included).
pub(crate) fn removable(
    retired: &[Retired],
    in_use: &InUse,
    prunable: &BTreeSet<(String, String)>,
    now: u64,
) -> Vec<Retired> {
    retired
        .iter()
        .filter(|entry| now.saturating_sub(entry.since) >= RETIRE_GRACE_SECS)
        .filter(|entry| {
            !in_use
                .get(&entry.dir)
                .is_some_and(|versions| versions.contains(&entry.version))
        })
        .filter(|entry| prunable.contains(&(entry.tool.clone(), entry.version.clone())))
        .cloned()
        .collect()
}

fn dir() -> PathBuf {
    crate::managed_tools::paths::tools_root().join("mise-keep")
}

fn retired_path() -> PathBuf {
    dir().join("retired.json")
}

fn read_retired() -> Vec<Retired> {
    std::fs::read(retired_path())
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

fn write_retired(entries: &[Retired]) {
    if entries.is_empty() {
        let _ = std::fs::remove_file(retired_path());
        return;
    }
    if let Ok(raw) = serde_json::to_vec_pretty(entries) {
        let _ = std::fs::create_dir_all(dir());
        let _ = std::fs::write(retired_path(), raw);
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

fn installs_root(home: &Path) -> PathBuf {
    super::launch_path::mise_data_dir(home).join("installs")
}

/// Runs mise and returns whether it succeeded with stdout and stderr together.
fn run_mise(mise: &str, args: &[&str], cwd: &Path, home: &Path) -> Option<(bool, String)> {
    let mut command = crate::platform::process::background_command(mise);
    command
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("NO_COLOR", "1")
        .env("MISE_YES", "1");
    #[cfg(not(windows))]
    command.env(
        "PATH",
        crate::agent_hooks::probing::normalize_gxserver_process_path(
            std::env::var("PATH").ok().as_deref(),
            home,
        ),
    );
    let finished =
        crate::managed_tools::run::output_within(command, Duration::from_secs(60)).ok()?;
    Some((
        finished.status.success(),
        format!("{}\n{}", finished.stdout, finished.stderr),
    ))
}

/// The install folder and version mise runs for `binary` now (`claude`, `2.1.319`).
pub(crate) fn current_install(binary: &str, home: &Path) -> Option<(String, String)> {
    let mise = super::process::resolve("mise", home)?;
    let path = super::mise::which(&mise, binary, home)?;
    let root = installs_root(home);
    let roots = roots_of(&root);
    let found = installs_in_text(&path.replace('\\', "/"), &roots);
    let (dir, versions) = found.into_iter().next()?;
    let version = resolve_version(&root, &dir, versions.into_iter().next()?);
    Some((dir, version))
}

fn roots_of(root: &Path) -> Vec<String> {
    let mut roots = vec![root.to_string_lossy().replace('\\', "/")];
    if let Ok(real) = std::fs::canonicalize(root) {
        let real = real.to_string_lossy().replace('\\', "/");
        if !roots.contains(&real) {
            roots.push(real);
        }
    }
    roots
}

/// `latest` and `2.1` are links to a real version folder.
fn resolve_version(root: &Path, dir: &str, version: String) -> String {
    std::fs::read_link(root.join(dir).join(&version))
        .ok()
        .and_then(|target| {
            target
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| safe_name(name))
        .unwrap_or(version)
}

/// Every mise install a running process of this user runs from.
#[cfg(unix)]
fn running(root: &Path) -> Option<InUse> {
    // `comm` is the program file on macOS; `args` holds it (and a script's path) on Linux too.
    let mut command = crate::platform::process::background_command("ps");
    command.args(["-A", "-ww", "-o", "comm=", "-o", "args="]);
    let finished = crate::managed_tools::run::output_within(command, Duration::from_secs(10))
        .ok()
        .filter(|finished| finished.status.success())?;
    let found = installs_in_text(&finished.stdout, &roots_of(root));
    let mut resolved = InUse::new();
    for (dir, versions) in found {
        for version in versions {
            let version = resolve_version(root, &dir, version);
            resolved.entry(dir.clone()).or_default().insert(version);
        }
    }
    Some(resolved)
}

/// The tool name for an install folder, when Ghostex can name it in a config.
fn tool_of(root: &Path, dir: &str) -> Option<String> {
    let toml = std::fs::read_to_string(root.join(dir).join(".mise.backend.toml")).ok()?;
    backend_short(&toml)
}

/// Rewrites the keep config with the versions running processes use and makes mise track it.
/// Returns what went wrong, when something did. A config mise cannot read is removed again, so
/// it never breaks the user's mise.
#[cfg(unix)]
fn refresh_shield(mise: &str, home: &Path) -> (Option<InUse>, Option<String>) {
    let root = installs_root(home);
    // A home without mise installs (a test server's own home) leaves the real config alone.
    if !root.is_dir() {
        return (None, None);
    }
    let Some(in_use) = running(&root) else {
        return (None, Some("Could not list running processes.".to_string()));
    };
    let mut tools = BTreeMap::<String, BTreeSet<String>>::new();
    for (dir, versions) in &in_use {
        let Some(tool) = tool_of(&root, dir) else {
            continue;
        };
        let present: BTreeSet<String> = versions
            .iter()
            .filter(|version| root.join(dir).join(version).is_dir())
            .cloned()
            .collect();
        if !present.is_empty() {
            tools.entry(tool).or_default().extend(present);
        }
    }
    let folder = dir();
    let config = folder.join("mise.toml");
    let text = keep_config(&tools);
    if std::fs::create_dir_all(&folder).is_err() || std::fs::write(&config, &text).is_err() {
        return (
            Some(in_use),
            Some("Could not write the mise keep config.".to_string()),
        );
    }
    let config_arg = config.to_string_lossy().into_owned();
    let trusted = run_mise(mise, &["trust", &config_arg], &folder, home);
    // Loading the config is what puts it on mise's tracked list (`mise prune` reads that list).
    let loaded = run_mise(mise, &["ls", "--current"], &folder, home);
    match (trusted, loaded) {
        (Some((true, _)), Some((true, _))) => (Some(in_use), None),
        (_, loaded) => {
            let _ = std::fs::remove_file(&config);
            let detail = loaded.map(|(_, output)| output).unwrap_or_default();
            (
                Some(in_use),
                Some(format!(
                    "mise did not accept the keep config, so Ghostex removed it: {}",
                    detail.trim()
                )),
            )
        }
    }
}

/// After a mise update: remembers the version it replaced and keeps every version in use.
pub(crate) async fn after_update(binary: &str, before: Option<(String, String)>, home: &Path) {
    if cfg!(windows) {
        return;
    }
    let binary = binary.to_string();
    let home = home.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        let Some((dir, version)) = before else {
            return;
        };
        if RUNTIMES.contains(&dir.as_str())
            || current_install(&binary, &home).as_ref() == Some(&(dir.clone(), version.clone()))
        {
            return;
        }
        let root = installs_root(&home);
        if let Some(tool) = tool_of(&root, &dir) {
            let mut retired = read_retired();
            if !retired
                .iter()
                .any(|entry| entry.dir == dir && entry.version == version)
            {
                retired.push(Retired {
                    dir,
                    tool,
                    version,
                    since: now(),
                });
                write_retired(&retired);
            }
        }
        #[cfg(unix)]
        if let Some(mise) = super::process::resolve("mise", &home) {
            let _ = refresh_shield(&mise, &home);
        }
    })
    .await;
}

/// The Updates pass: keeps every version in use, then uninstalls replaced versions nothing uses.
/// Returns problems worth logging.
pub(crate) async fn pass(home: &Path) -> Vec<String> {
    if cfg!(windows) {
        return Vec::new();
    }
    // An install or update running now owns mise's folders; the next pass catches up.
    let Ok(_turn) = crate::managed_tools::INSTALL_LOCK.try_lock() else {
        return Vec::new();
    };
    let home = home.to_path_buf();
    tokio::task::spawn_blocking(move || pass_blocking(&home))
        .await
        .unwrap_or_default()
}

#[cfg(not(unix))]
fn pass_blocking(_home: &Path) -> Vec<String> {
    Vec::new()
}

#[cfg(unix)]
fn pass_blocking(home: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let Some(mise) = super::process::resolve("mise", home) else {
        return problems;
    };
    let (in_use, problem) = refresh_shield(&mise, home);
    problems.extend(problem);
    let root = installs_root(home);
    let mut retired: Vec<Retired> = read_retired()
        .into_iter()
        .filter(|entry| root.join(&entry.dir).join(&entry.version).is_dir())
        .collect();
    // Without a trustworthy process list nothing is uninstalled.
    let Some(in_use) = in_use else {
        write_retired(&retired);
        return problems;
    };
    let tools: BTreeSet<String> = retired.iter().map(|entry| entry.tool.clone()).collect();
    let mut can_prune = BTreeSet::new();
    for tool in &tools {
        if let Some((true, output)) = run_mise(&mise, &["prune", "--dry-run", tool], home, home) {
            can_prune.extend(prunable(&output));
        }
    }
    for entry in removable(&retired, &in_use, &can_prune, now()) {
        let spec = format!("{}@{}", entry.tool, entry.version);
        match run_mise(&mise, &["uninstall", &spec], home, home) {
            Some((true, _)) => {}
            Some((false, output)) => {
                problems.push(format!("mise uninstall {spec} failed: {}", output.trim()))
            }
            None => problems.push(format!("mise uninstall {spec} did not finish.")),
        }
    }
    retired.retain(|entry| root.join(&entry.dir).join(&entry.version).is_dir());
    write_retired(&retired);
    problems
}
