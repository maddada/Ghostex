//! Replacing the installed Claude Swap on Windows only once no running cswap uses its files.
//!
//! CDXC:AgentProviders 2026-10-18 DECISION:
//! User: "Fix Ghostex to switch safely": Ghostex's Windows Claude Swap update installs side by side and switches the moment nothing holds cswap.exe, retrying on its own, so this computer switches itself and so does every Windows user. Since 2026-11-07 the switch installs whatever claude_swap_build.rs names (Ghostex's build on upstream 1.0.0 for now, PyPI's claude-swap once upstream has the two fixes), and it stays for every Windows update whatever the source. Every live Claude session holds the installed cswap open (uv's `cswap.exe` launcher, the tool venv's `python.exe`, and the base Python that opens the launcher as a zipapp), and `uv tool install --force` over it deleted the old venv's packages, then failed on `Scripts` with "Access is denied" and left every new cswap with "No module named 'claude_swap'" (2026-10-18). So on Windows, Update, Reinstall, Uninstall, the launch-time replacement (launch.rs) and the automatic move to the pinned build (`update_outdated` in helper_tools.rs) first install the new build into Ghostex's own `side-by-side/<version>` folder and check that it starts, then switch only when no process runs from the installed venv or launchers and no program holds a launcher open. The switch renames the old launchers, then the venv, aside before the owning tool (uv or pipx) installs into the real place, so nothing is deleted under a running cswap: a held launcher refuses the rename and leaves the install as it was, and a failed install puts the old one back. A waiting switch is retried every 30 seconds and when gxserver starts, and old copies are deleted once nothing uses them. New sessions keep launching the installed cswap until the switch, because a session saves that path and every later resume runs it. macOS and Linux replace files under a running process and keep PyPI's claude-swap.

use super::claude_swap_build;
use crate::{managed_tools, server::AppState};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const RETRY: Duration = Duration::from_secs(30);
/// How often an installed cswap older than the wanted one is looked for (every OS).
const OUTDATED_CHECK: Duration = Duration::from_secs(60 * 60);
const TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// The tool that owns the installed cswap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Owner {
    Uv,
    Pipx,
}

/// A switch that waits for the installed cswap to be free.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Pending {
    /// `update`, `reinstall` or `uninstall`.
    pub(crate) action: String,
    pub(crate) owner: Owner,
    /// The uv or pipx that installs the build into the real place.
    pub(crate) program: PathBuf,
    /// The installed venv and its launchers: what a running cswap holds open.
    pub(crate) holds: Vec<PathBuf>,
    pub(crate) since: String,
    /// Why the last attempt failed while nothing used the install; a later pass tries again.
    #[serde(default)]
    pub(crate) last_error: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct State {
    pending: Option<Pending>,
    /// Replaced installs and side-by-side copies, deleted once nothing uses them.
    #[serde(default)]
    retired: Vec<PathBuf>,
}

/// Held while a switch moves and installs files, so Ghostex hands out no cswap halfway through.
static SWITCHING: Mutex<()> = Mutex::new(());

fn dir() -> PathBuf {
    managed_tools::paths::tools_root().join("claude-swap")
}

fn state_path() -> PathBuf {
    dir().join("switch.json")
}

fn side_by_side_root() -> PathBuf {
    dir().join("side-by-side")
}

fn staged_dir() -> PathBuf {
    side_by_side_root().join(claude_swap_build::pinned().unwrap_or("latest"))
}

fn read_state() -> State {
    std::fs::read(state_path())
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

fn write_state(state: &State) -> Result<(), String> {
    let path = state_path();
    if state.pending.is_none() && state.retired.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(dir()).map_err(|error| error.to_string())?;
    let partial = path.with_extension("json.part");
    let raw = serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?;
    std::fs::write(&partial, raw)
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|error| format!("Could not write {}: {error}", path.display()))
}

/// The switch waiting on this computer, if any.
pub(crate) fn pending() -> Option<Pending> {
    cfg!(windows)
        .then(read_state)
        .and_then(|state| state.pending)
}

/// Returns once no switch is moving files (helpers::executable calls it before handing out cswap).
pub(crate) fn wait_for_switch() {
    if cfg!(windows) {
        drop(SWITCHING.lock().unwrap_or_else(|error| error.into_inner()));
    }
}

/// The installed venv and launchers of the cswap `owner` manages, read from the tool itself.
pub(crate) fn installed_paths(owner: Owner, program: &Path) -> Result<Vec<PathBuf>, String> {
    let ask = |args: &[&str]| {
        crate::platform::process::background_command(program)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| {
                let text = String::from_utf8_lossy(&output.stdout);
                text.lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .map(PathBuf::from)
            })
            .ok_or_else(|| {
                format!(
                    "Could not ask {} where Claude Swap is installed.",
                    program.display()
                )
            })
    };
    let launcher_names = ["cswap.exe", "claude-swap.exe"];
    match owner {
        Owner::Uv => {
            let venv = ask(&["tool", "dir"])?.join("claude-swap");
            // uv records every launcher it wrote in the tool's receipt.
            let mut launchers: Vec<PathBuf> = std::fs::read_to_string(venv.join("uv-receipt.toml"))
                .ok()
                .and_then(|raw| raw.parse::<toml_edit::DocumentMut>().ok())
                .and_then(|receipt| {
                    receipt["tool"]["entrypoints"].as_array().map(|entries| {
                        entries
                            .iter()
                            .filter_map(|entry| {
                                entry.as_inline_table()?.get("install-path")?.as_str()
                            })
                            .map(PathBuf::from)
                            .collect()
                    })
                })
                .unwrap_or_default();
            if launchers.is_empty() {
                let bin = ask(&["tool", "dir", "--bin"])?;
                launchers = launcher_names.iter().map(|name| bin.join(name)).collect();
            }
            Ok(std::iter::once(venv).chain(launchers).collect())
        }
        Owner::Pipx => {
            let venv = ask(&["environment", "--value", "PIPX_LOCAL_VENVS"])?.join("claude-swap");
            let bin = ask(&["environment", "--value", "PIPX_BIN_DIR"])?;
            Ok(std::iter::once(venv)
                .chain(launcher_names.iter().map(|name| bin.join(name)))
                .collect())
        }
    }
}

/// What still uses `paths` (a process running from them, or a file another program opened
/// without letting it be deleted), or `None` when they are free.
pub(crate) fn user_of(paths: &[PathBuf]) -> Option<String> {
    #[cfg(windows)]
    {
        let normalize = |path: &Path| {
            let text = path.to_string_lossy().replace('/', "\\").to_lowercase();
            text.strip_prefix(r"\\?\")
                .map(str::to_string)
                .unwrap_or(text)
                .trim_end_matches('\\')
                .to_string()
        };
        let roots: Vec<String> = paths.iter().map(|path| normalize(path)).collect();
        let own = std::process::id();
        for pid in crate::platform::process::process_ids() {
            if pid == own {
                continue;
            }
            let Some(image) = crate::platform::process::process_image_path(pid) else {
                continue;
            };
            let running = normalize(&image);
            if roots
                .iter()
                .any(|root| running == *root || running.starts_with(&format!("{root}\\")))
            {
                return Some(format!("{} (process {pid})", image.display()));
            }
        }
        // The base Python opens the launcher as a zipapp, so its image is elsewhere.
        if let Some(open) = paths
            .iter()
            .find(|path| path.is_file() && opened_elsewhere(path))
        {
            return Some(format!("{} is open in another program", open.display()));
        }
    }
    #[cfg(not(windows))]
    let _ = paths;
    None
}

/// Whether another program holds `file` open without letting it be deleted, asked by opening it
/// for deletion and closing it again at once.
#[cfg(windows)]
fn opened_elsewhere(file: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_SHARING_VIOLATION, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, DELETE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            OPEN_EXISTING,
        },
    };
    let wide: Vec<u16> = file.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let handle = CreateFileW(
            wide.as_ptr(),
            DELETE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return GetLastError() == ERROR_SHARING_VIOLATION;
        }
        CloseHandle(handle);
        false
    }
}

/// Installs `spec` (claude_swap_build.rs) into Ghostex's own side-by-side folder and checks that
/// it starts. The installed cswap is not touched.
fn stage(uv: &Path, spec: &str, log: &managed_tools::Log) -> Result<(), String> {
    let staged = staged_dir();
    if staged.exists() && user_of(std::slice::from_ref(&staged)).is_none() {
        let _ = std::fs::remove_dir_all(&staged);
    }
    let tools = staged.join("tools");
    let bin = staged.join("bin");
    log.line(&format!(
        "Installing {} side by side in {}; the installed Claude Swap stays as it is until nothing uses it.",
        target_label(),
        staged.display()
    ));
    managed_tools::run::run(
        uv,
        &claude_swap_build::uv_install_args(spec),
        &[
            ("UV_TOOL_DIR", tools.as_os_str()),
            ("UV_TOOL_BIN_DIR", bin.as_os_str()),
        ],
        &[],
        log,
        TIMEOUT,
    )?;
    let version = managed_tools::tools::version_of(&bin.join("cswap.exe"), &["--version"]);
    let started = match claude_swap_build::pinned() {
        Some(pinned) => version.as_deref() == Some(pinned),
        None => version.is_some(),
    };
    if !started {
        return Err(format!(
            "{} installed side by side but did not start (it reported {}).",
            target_label(),
            version.as_deref().unwrap_or("no version")
        ));
    }
    Ok(())
}

/// "Claude Swap 1.0.0.post1+ghostex", or "the latest Claude Swap" when Ghostex follows PyPI.
fn target_label() -> String {
    claude_swap_build::pinned().map_or_else(
        || "the latest Claude Swap".to_string(),
        |version| format!("Claude Swap {version}"),
    )
}

/// Update, Reinstall or Uninstall of the cswap `owner` installed: stages the new build (not for
/// Uninstall), records the switch and makes the first attempt. A switch that has to wait is not
/// an error: it is retried on its own.
pub(crate) fn replace(
    action: &str,
    owner: Owner,
    program: &Path,
    uv: &Path,
    home: &Path,
    log: &managed_tools::Log,
) -> Result<(), String> {
    let holds = installed_paths(owner, program)?;
    if action != "uninstall" {
        let spec = claude_swap_build::install_spec(log)?;
        stage(uv, &spec, log)?;
    }
    let pending = Pending {
        action: action.to_string(),
        owner,
        program: program.to_path_buf(),
        holds,
        since: chrono::Utc::now().to_rfc3339(),
        last_error: None,
    };
    {
        let _turn = SWITCHING.lock().unwrap_or_else(|error| error.into_inner());
        let mut state = read_state();
        state.pending = Some(pending.clone());
        write_state(&state)?;
    }
    match switch(home, log) {
        Ok(None) => {}
        Ok(Some(user)) => log.line(&format!(
            "{}. Waiting for: {user}.",
            waiting_message(&pending)
        )),
        Err(error) => log.line(&format!(
            "The switch did not finish ({error}). The installed Claude Swap was put back, and Ghostex tries again on its own."
        )),
    }
    Ok(())
}

/// The Accounts page's line for a waiting switch.
pub(crate) fn waiting_message(pending: &Pending) -> String {
    let what = match pending.action.as_str() {
        "uninstall" => "Uninstall is waiting".to_string(),
        "reinstall" => "Reinstall is ready".to_string(),
        _ => claude_swap_build::pinned().map_or_else(
            || "Update is ready".to_string(),
            |version| format!("Update to {version} is ready"),
        ),
    };
    if pending.last_error.is_some() {
        format!("{what}; Ghostex is retrying the switch on its own")
    } else {
        format!("{what}; applies when your running Claude sessions close")
    }
}

/// Switches to the waiting build when nothing uses the installed one, and deletes old copies
/// nothing uses any more. `Ok(Some(why))` while the switch waits, `Ok(None)` once nothing is
/// left to switch, `Err` when an attempt failed (the old install is back and a later pass retries).
pub(crate) fn switch(home: &Path, log: &managed_tools::Log) -> Result<Option<String>, String> {
    let _turn = SWITCHING.lock().unwrap_or_else(|error| error.into_inner());
    let mut state = read_state();
    let Some(pending) = state.pending.clone() else {
        // A side-by-side copy no switch waits for (its record lost) goes too.
        if side_by_side_root().exists() {
            state.retired.push(side_by_side_root());
        }
        cleanup(&mut state);
        write_state(&state)?;
        return Ok(None);
    };
    if let Some(user) = user_of(&pending.holds) {
        cleanup(&mut state);
        write_state(&state)?;
        return Ok(Some(user));
    }
    let spec = if pending.action == "uninstall" {
        None
    } else {
        Some(claude_swap_build::install_spec(log)?)
    };
    // A held launcher refuses the rename (Windows lets a venv folder with running files be
    // renamed), so the launchers go aside first and a refusal leaves the install as it was.
    let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();
    let mut order: Vec<&PathBuf> = pending.holds.iter().filter(|path| path.exists()).collect();
    order.sort_by_key(|path| path.is_dir());
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for hold in order {
        let mut name = hold.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".ghostex-old-{stamp}"));
        let aside = hold.with_file_name(name);
        if let Err(error) = std::fs::rename(hold, &aside) {
            state.retired.extend(put_back(&moved));
            write_state(&state)?;
            return Ok(Some(format!("{} ({error})", hold.display())));
        }
        moved.push((hold.clone(), aside));
    }
    let installed = match &spec {
        None => Ok(()),
        Some(spec) => {
            let args: Vec<&str> = match pending.owner {
                Owner::Uv => claude_swap_build::uv_install_args(spec),
                Owner::Pipx => vec!["install", "--force", spec],
            };
            managed_tools::run::run(&pending.program, &args, &[], &[], log, TIMEOUT)
        }
    };
    if let Err(error) = installed {
        for (hold, _) in &moved {
            remove(hold);
        }
        state.retired.extend(put_back(&moved));
        if let Some(pending) = state.pending.as_mut() {
            pending.last_error = Some(error.clone());
        }
        write_state(&state)?;
        return Err(error);
    }
    state
        .retired
        .extend(moved.into_iter().map(|(_, aside)| aside));
    state.retired.push(side_by_side_root());
    state.pending = None;
    cleanup(&mut state);
    write_state(&state)?;
    crate::agent_hooks::probing::refresh_cli_environment(home);
    log.line(&match spec {
        Some(_) => format!("Switched to {}.", target_label()),
        None => "Removed Claude Swap.".to_string(),
    });
    Ok(None)
}

/// Moves renamed files back; returns the ones that could not go back, for cleanup.
fn put_back(moved: &[(PathBuf, PathBuf)]) -> Vec<PathBuf> {
    moved
        .iter()
        .rev()
        .filter(|(hold, aside)| std::fs::rename(aside, hold).is_err())
        .map(|(_, aside)| aside.clone())
        .collect()
}

fn remove(path: &Path) {
    let _ = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
}

/// Deletes retired copies nothing uses any more; the rest wait for a later pass.
fn cleanup(state: &mut State) {
    let mut seen = Vec::new();
    state.retired.retain(|path| {
        if seen.contains(path) {
            return false;
        }
        seen.push(path.clone());
        if !path.exists() {
            return false;
        }
        if user_of(std::slice::from_ref(path)).is_some() {
            return true;
        }
        remove(path);
        path.exists()
    });
}

/// On Windows, retries a waiting switch every 30 seconds (the first pass runs as gxserver starts)
/// and deletes old copies once free; on every OS, once an hour moves an installed cswap that is
/// not the one Ghostex wants (claude_swap_build.rs) to it.
pub(crate) fn start(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    let home = state.paths.home_dir.clone();
    tokio::spawn(async move {
        let mut checked: Option<Instant> = None;
        loop {
            if cfg!(windows) && (state_path().exists() || side_by_side_root().exists()) {
                // A running install job makes its own attempt; this pass waits for the next tick.
                if let Ok(_turn) = managed_tools::INSTALL_LOCK.try_lock() {
                    let pass_home = home.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        switch(&pass_home, &managed_tools::Log::new(|_| {}))
                    })
                    .await;
                    if let Ok(Err(error)) = result {
                        let _ = state.logger.log(crate::logging::GxserverLogInput {
                            level: crate::logging::LogLevel::Warn,
                            event: "claudeSwapSwitchFailed".to_string(),
                            server_id: Some(state.metadata.server_id.clone()),
                            request_id: None,
                            client: None,
                            duration_ms: None,
                            error: Some(error),
                            details: None,
                        });
                    }
                }
            }
            if checked.is_none_or(|at| at.elapsed() >= OUTDATED_CHECK) {
                checked = Some(Instant::now());
                let check_home = home.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    super::helper_tools::update_outdated(&check_home)
                })
                .await;
            }
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(RETRY) => {}
            }
        }
    });
}
