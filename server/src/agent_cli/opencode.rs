//! CDXC:AgentProviders 2026-10-25 WHY:
//! User: "im using opencode ... it's saying a different model" and then, on a session showing only
//! "OpenCode chat is not connected": "fix please". The `opencode` on PATH was not OpenCode 2: first
//! mise's `npm:@opencode/cli`, installed under `ignore-scripts=true`, so its postinstall never fetched
//! the binary and every start failed; then OpenCode 1.19.4 (`aqua:anomalyco/opencode`), which has no
//! service or plugin for the chat to reach. Ghostex supports OpenCode 2 only, so a new terminal runs
//! the first working OpenCode 2 it can find (OpenCode 2 ships an `opencode2` command beside `opencode`
//! so both majors can coexist), and a chat that cannot connect says which OpenCode it found and offers
//! the catalog's official installer instead of a generic "not connected".
//! SEE-ALSO: server/src/session_chat_opencode/mirror.rs (the notice), server/src/session_chat_agent_restart.rs (its actions).

use super::{catalog::CATALOG, mise, process};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant, SystemTime},
};

pub(crate) const ACTION_INSTALL: &str = "installOpenCode2";
pub(crate) const ACTION_RESTART: &str = "restartOnOpenCode2";

/// What one OpenCode executable turned out to be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Release {
    Two,
    /// An OpenCode before 2, with the version it printed.
    Older(String),
    /// It exists but does not start (`--version` failed or printed no version).
    Unusable,
}

/// Why an OpenCode session's chat cannot connect, when Ghostex can tell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Problem {
    /// The session runs an OpenCode before 2.
    Older { version: String, path: String },
    /// The `opencode` a new terminal runs is installed but cannot start.
    Unusable { path: String },
    /// No OpenCode is installed.
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InstallState {
    Running,
    Failed(String),
}

static INSTALL: Mutex<Option<InstallState>> = Mutex::new(None);

/// Reads the version a binary prints: `opencode v2.0.29` (OpenCode 2) or `1.19.4` (OpenCode 1).
pub(crate) fn parse_release(output: &str) -> Release {
    let version = output
        .split_whitespace()
        .map(|word| word.trim_start_matches(['v', 'V']))
        .find(|word| {
            word.split('.').count() >= 2 && word.split('.').all(|part| part.parse::<u32>().is_ok())
        });
    let Some(version) = version else {
        return Release::Unusable;
    };
    match version
        .split('.')
        .next()
        .and_then(|major| major.parse::<u32>().ok())
    {
        Some(major) if major >= 2 => Release::Two,
        _ => Release::Older(version.to_string()),
    }
}

type ReleaseKey = (PathBuf, Option<SystemTime>, u64);
static RELEASES: LazyLock<Mutex<HashMap<ReleaseKey, Release>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The release of one executable, asked once per file version (an update replaces the file).
pub(crate) fn release(path: &Path, home: &Path) -> Release {
    let metadata = std::fs::metadata(path).ok();
    let key = (
        path.to_path_buf(),
        metadata.as_ref().and_then(|m| m.modified().ok()),
        metadata.as_ref().map_or(0, |m| m.len()),
    );
    if let Some(release) = RELEASES
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
    {
        return release;
    }
    let mut command = crate::platform::process::background_command(path);
    command.arg("--version").current_dir(home).env("HOME", home);
    let release = crate::agent_hooks::probing::run_command_stdout_with_timeout(
        command,
        Duration::from_secs(10),
    )
    .map_or(Release::Unusable, |output| parse_release(&output));
    if let Ok(mut cache) = RELEASES.lock() {
        cache.insert(key, release.clone());
    }
    release
}

fn install_dirs(home: &Path) -> Vec<PathBuf> {
    CATALOG
        .iter()
        .find(|definition| definition.agent_id == "opencode")
        .map(|definition| definition.install_dirs(home))
        .unwrap_or_default()
}

/// The real program behind a command found on PATH: a mise shim is asked which install it runs, and
/// the official installer's `opencode2` (a script that runs the `opencode` beside it) is followed to
/// that `opencode`. The process then keeps the name `opencode`, which session identity, hooks and
/// resume all match on.
fn real_executable(found: &str, home: &Path) -> Option<PathBuf> {
    let name = Path::new(found).file_name()?.to_string_lossy().to_string();
    let name = name.trim_end_matches(".exe").trim_end_matches(".cmd");
    let path = mise::shim_target(found, name, home).unwrap_or_else(|| found.to_string());
    let real = std::fs::canonicalize(&path).ok()?;
    let stem = real.file_stem()?.to_string_lossy().to_string();
    if stem == "opencode" {
        return Some(real);
    }
    let binary = if cfg!(windows) {
        "opencode.exe"
    } else {
        "opencode"
    };
    let sibling = real.parent()?.join(binary);
    sibling.is_file().then_some(sibling)
}

/// The OpenCode 2 a new terminal should run. `Ok(None)`: the plain `opencode` on PATH already is
/// OpenCode 2. `Err(())`: no OpenCode 2 was found.
/// An install-on-demand wrapper is never asked for its version, which would install OpenCode
/// (`agent_hooks/on_demand.rs`); the terminal then runs the plain `opencode`, wrapper included.
fn find_launch(home: &Path) -> Result<Option<PathBuf>, ()> {
    if let Some(path) = process::resolve("opencode", home)
        .and_then(|found| crate::agent_hooks::on_demand::probe_target(&found, "opencode", home))
        .and_then(|found| real_executable(&found, home))
    {
        if release(&path, home) == Release::Two {
            return Ok(None);
        }
    }
    let dirs = install_dirs(home);
    let candidates = process::resolve_agent("opencode2", &dirs, home)
        .and_then(|found| real_executable(&found, home))
        .into_iter()
        .chain(dirs.iter().map(|dir| {
            dir.join(if cfg!(windows) {
                "opencode.exe"
            } else {
                "opencode"
            })
        }));
    for candidate in candidates {
        if candidate.is_file()
            && crate::agent_hooks::on_demand::classify(&candidate, home)
                == crate::agent_hooks::on_demand::Kind::Real
            && release(&candidate, home) == Release::Two
        {
            return Ok(Some(candidate));
        }
    }
    Err(())
}

static LAUNCH: LazyLock<Mutex<Option<(Instant, Result<Option<PathBuf>, ()>)>>> =
    LazyLock::new(|| Mutex::new(None));

/// `find_launch`, read again at most every 15 seconds and right after an install.
pub(crate) fn launch_executable(home: &Path) -> Result<Option<PathBuf>, ()> {
    if let Some((at, found)) = LAUNCH.lock().ok().and_then(|cached| cached.clone()) {
        if at.elapsed() < Duration::from_secs(15) {
            return found;
        }
    }
    let found = find_launch(home);
    if let Ok(mut cached) = LAUNCH.lock() {
        *cached = Some((Instant::now(), found.clone()));
    }
    found
}

/// The OpenCode 2 program itself, for what gxserver asks it (its model lineup); `None` when no
/// OpenCode 2 is installed. An install-on-demand wrapper is never followed, as in `find_launch`.
pub(crate) fn opencode2_program(home: &Path) -> Option<PathBuf> {
    match launch_executable(home) {
        Ok(Some(path)) => Some(path),
        Ok(None) => process::resolve("opencode", home)
            .and_then(|found| crate::agent_hooks::on_demand::probe_target(&found, "opencode", home))
            .and_then(|found| real_executable(&found, home)),
        Err(()) => None,
    }
}

fn forget_launch() {
    if let Ok(mut cached) = LAUNCH.lock() {
        *cached = None;
    }
}

/// Replaces a bare `opencode` command with the OpenCode 2 executable when the `opencode` on PATH is
/// not OpenCode 2. Custom commands (a wrapper, an absolute path) are left as written.
pub(crate) fn with_opencode2(agent_id: &str, icon: Option<&str>, command: &str) -> String {
    // Unit tests assert exact commands, which must not depend on the CLIs installed on the machine.
    if cfg!(test) || (agent_id != "opencode" && icon != Some("opencode")) {
        return command.to_string();
    }
    let trimmed = command.trim_start();
    let rest = trimmed.strip_prefix("opencode").unwrap_or("x");
    if !(rest.is_empty() || rest.starts_with(char::is_whitespace)) {
        return command.to_string();
    }
    let home = crate::resume_lookup::home_dir();
    let Ok(Some(path)) = launch_executable(&home) else {
        return command.to_string();
    };
    let quoted = crate::agents::quote_shell_arg(&path.to_string_lossy());
    // PowerShell runs a quoted path only through the call operator.
    let program = if cfg!(windows) {
        format!("& {quoted}")
    } else {
        quoted
    };
    format!("{program}{rest}")
}

/// The executable of the session's running agent, if it is an OpenCode process.
#[cfg(unix)]
fn running_executable(zmx_name: &str, home: &Path) -> Option<PathBuf> {
    let identities =
        crate::zmx::read_zmx_session_process_identities(&[zmx_name.to_string()], home).ok()?;
    let identity = identities.get(zmx_name)?;
    if identity.agent_id.as_deref() != Some("opencode") {
        return None;
    }
    let pid = identity.process_id.filter(|pid| *pid > 0)?;
    let mut command = crate::platform::process::background_command("ps");
    command.args(["-o", "command=", "-p", &pid.to_string()]);
    let line = crate::agent_hooks::probing::run_command_stdout_with_timeout(
        command,
        Duration::from_secs(3),
    )?;
    let program = line.split_whitespace().next()?;
    let path = if program.contains('/') {
        PathBuf::from(program)
    } else {
        PathBuf::from(process::resolve(program, home)?)
    };
    std::fs::canonicalize(path).ok()
}

#[cfg(not(unix))]
fn running_executable(_zmx_name: &str, _home: &Path) -> Option<PathBuf> {
    None
}

/// Why an OpenCode chat cannot connect: the release the session's own process runs, else what a new
/// terminal would start. `None` when an OpenCode 2 is running or available, so the problem is elsewhere.
pub(crate) fn diagnose(zmx_name: Option<&str>, home: &Path) -> Option<Problem> {
    if let Some(path) = zmx_name.and_then(|name| running_executable(name, home)) {
        return match release(&path, home) {
            Release::Older(version) => Some(Problem::Older {
                version,
                path: path.to_string_lossy().into_owned(),
            }),
            _ => None,
        };
    }
    if launch_executable(home).is_ok() {
        return None;
    }
    let Some(found) = process::resolve("opencode", home) else {
        return Some(Problem::Missing);
    };
    let path = real_executable(&found, home).unwrap_or_else(|| PathBuf::from(&found));
    Some(match release(&path, home) {
        Release::Older(version) => Problem::Older {
            version,
            path: path.to_string_lossy().into_owned(),
        },
        _ => Problem::Unusable {
            path: path.to_string_lossy().into_owned(),
        },
    })
}

static DIAGNOSES: LazyLock<Mutex<HashMap<String, (Instant, Option<Problem>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// `diagnose` for a chat that polls: read again at most every 10 seconds per session.
pub(crate) fn diagnose_cached(zmx_name: Option<&str>, home: &Path) -> Option<Problem> {
    let key = zmx_name.unwrap_or_default().to_string();
    if let Some((at, problem)) = DIAGNOSES
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
    {
        if at.elapsed() < Duration::from_secs(10) {
            return problem;
        }
    }
    let problem = diagnose(zmx_name, home);
    if let Ok(mut cache) = DIAGNOSES.lock() {
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(key, (Instant::now(), problem.clone()));
    }
    problem
}

pub(crate) fn install_state() -> Option<InstallState> {
    INSTALL.lock().ok().and_then(|state| state.clone())
}

/// The catalog's official OpenCode 2 installer, told to leave shell profiles alone: it installs into
/// `~/.opencode/bin` (one of the catalog's install folders, so Ghostex finds it without PATH), and the
/// user's own `opencode` keeps running whatever it ran before.
pub(crate) fn install_command() -> Option<String> {
    let definition = CATALOG
        .iter()
        .find(|definition| definition.agent_id == "opencode")?;
    let install = definition.native_install()?;
    if cfg!(windows) {
        return None;
    }
    Some(if install.trim_end().ends_with("| bash") {
        format!("{} -s -- --no-modify-path", install.trim_end())
    } else {
        install.to_string()
    })
}

/// Marks the install as running, so every waiting chat says so at once. Refused while one runs.
pub(crate) fn begin_install() -> Result<(), String> {
    let mut state = INSTALL.lock().map_err(|e| e.to_string())?;
    if *state == Some(InstallState::Running) {
        return Err("OpenCode 2 is already being installed.".into());
    }
    *state = Some(InstallState::Running);
    if let Ok(mut cache) = DIAGNOSES.lock() {
        cache.clear();
    }
    Ok(())
}

/// After `begin_install`: runs the official installer (installs share the managed-tools queue with the
/// Agents page) and checks that an OpenCode 2 is now found.
pub(crate) async fn install(home: &Path) -> Result<(), String> {
    let result = run_install(home).await;
    forget_launch();
    if let Ok(mut cache) = DIAGNOSES.lock() {
        cache.clear();
    }
    if let Ok(mut state) = INSTALL.lock() {
        *state = result
            .as_ref()
            .err()
            .map(|error| InstallState::Failed(error.clone()));
    }
    result
}

async fn run_install(home: &Path) -> Result<(), String> {
    let script = install_command()
        .ok_or_else(|| "Install OpenCode 2 from Settings > Agents on this computer.".to_string())?;
    let _turn = crate::managed_tools::INSTALL_LOCK.lock().await;
    let output = std::sync::Arc::new(Mutex::new(String::new()));
    let sink = output.clone();
    let ran = process::run(
        &script,
        home,
        &Default::default(),
        Duration::from_secs(600),
        move |chunk| {
            if let Ok(mut output) = sink.lock() {
                output.push_str(&chunk);
            }
        },
    )
    .await;
    if let Err(error) = ran {
        let output = output.lock().map(|text| text.clone()).unwrap_or_default();
        let tail: Vec<&str> = output.trim().lines().rev().take(4).collect();
        let tail = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(if tail.is_empty() {
            error
        } else {
            format!("{error}\n{tail}")
        });
    }
    let owned_home = home.to_path_buf();
    let found = tokio::task::spawn_blocking(move || {
        crate::agent_hooks::probing::refresh_cli_environment(&owned_home);
        forget_launch();
        launch_executable(&owned_home)
    })
    .await
    .map_err(|e| e.to_string())?;
    found.map(|_| ()).map_err(|_| {
        "The installer finished, but OpenCode 2 still was not found in ~/.opencode/bin.".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_version_formats() {
        assert_eq!(parse_release("opencode v2.0.29\n"), Release::Two);
        assert_eq!(parse_release("2.1.4"), Release::Two);
        assert_eq!(parse_release("10.0.1"), Release::Two);
        assert_eq!(parse_release("1.19.4\n"), Release::Older("1.19.4".into()));
        assert_eq!(parse_release("0.15.20"), Release::Older("0.15.20".into()));
        assert_eq!(
            parse_release("Error: @opencode/cli's postinstall script was not run."),
            Release::Unusable
        );
    }

    #[test]
    fn launch_rewrite_leaves_custom_commands_alone_in_tests() {
        assert_eq!(
            with_opencode2("opencode", None, "opencode --auto"),
            "opencode --auto"
        );
    }
}
