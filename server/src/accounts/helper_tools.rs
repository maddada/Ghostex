//! Settings > Accounts keeps Claude Swap (cswap) and Codex Swap (xswap) installed and current:
//! Install, installed and latest versions, and Update, Reinstall and Uninstall run here so every
//! client (desktop, web, a remote computer's Settings) gets the same answer.

use super::{claude_swap_switch, claude_swap_windows, helpers, model::Provider};
use crate::{domain::DomainStateError, managed_tools, server::AppState};
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeMap,
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Mutex,
    time::{Duration, Instant},
};

const CODEX_SWAP_REPOSITORY: &str = "https://github.com/maddada/codex-swap";
const CODEX_SWAP_FORMULA: &str = "maddada/tap/codex-swap";
const CODEX_SWAP_INSTALL_SH: &str =
    "https://github.com/maddada/codex-swap/releases/latest/download/install.sh";
const CODEX_SWAP_INSTALL_PS1: &str =
    "https://github.com/maddada/codex-swap/releases/latest/download/install.ps1";
/// Written by codex-swap's `install.sh` beside the binary it installed.
const XSWAP_RECEIPT: &str = ".xswap-install-receipt.json";
const LATEST_TTL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Install,
    Update,
    Reinstall,
    Uninstall,
}
impl Action {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "install" => Some(Self::Install),
            "update" => Some(Self::Update),
            "reinstall" => Some(Self::Reinstall),
            "uninstall" => Some(Self::Uninstall),
            _ => None,
        }
    }
    fn id(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Update => "update",
            Self::Reinstall => "reinstall",
            Self::Uninstall => "uninstall",
        }
    }
}

/// What a job runs: a program with arguments, or an official installer one-liner in the shell.
enum Work {
    Program(PathBuf, Vec<String>),
    Script(String, BTreeMap<String, String>),
    /// Remove these files (a script install has no uninstaller).
    Remove(Vec<PathBuf>),
    /// Ghostex installs uv first, then runs `uv tool install claude-swap`.
    UvInstall,
    /// Ghostex's Windows build of Claude Swap, checked and installed from its pinned wheel
    /// (claude_swap_windows.rs) by the tool that owns the install; an installed one is replaced
    /// (or uninstalled) only once nothing uses it (claude_swap_switch.rs).
    WindowsBuild(WheelInstaller, Action),
}

/// What installs the Windows build's wheel: the uv that owns the install (Ghostex installs uv
/// first when there is none) or pipx.
enum WheelInstaller {
    Uv(Option<PathBuf>),
    Pipx(PathBuf),
}

/// How the helper on this computer was installed, read from where its executable lives.
enum InstallMethod {
    Uv(PathBuf),
    Pipx(PathBuf),
    Homebrew(PathBuf),
    Cargo {
        cargo: PathBuf,
        root: PathBuf,
    },
    /// codex-swap's `install.sh`, recognised by its receipt beside the binary.
    Script {
        xswap: PathBuf,
        dir: PathBuf,
    },
    WindowsInstaller(PathBuf),
    Unknown,
}
impl InstallMethod {
    fn id(&self) -> &'static str {
        match self {
            Self::Uv(_) => "uv",
            Self::Pipx(_) => "pipx",
            Self::Homebrew(_) => "homebrew",
            Self::Cargo { .. } => "cargo",
            Self::Script { .. } => "script",
            Self::WindowsInstaller(_) => "windowsInstaller",
            Self::Unknown => "unknown",
        }
    }
    /// What performs `action` with the tool that owns the install.
    fn work(&self, action: Action) -> Option<Work> {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        let verb = |action: Action| match action {
            Action::Update => "upgrade",
            Action::Reinstall => "reinstall",
            _ => "uninstall",
        };
        match (self, action) {
            (_, Action::Install) => None,
            // PyPI's release would bring back a cswap that refuses to share history on Windows.
            // Running sessions hold the installed files open on Windows, so Uninstall waits too.
            (Self::Uv(uv), action) if claude_swap_windows::applies() => Some(Work::WindowsBuild(
                WheelInstaller::Uv(Some(uv.clone())),
                action,
            )),
            (Self::Pipx(pipx), action) if claude_swap_windows::applies() => Some(
                Work::WindowsBuild(WheelInstaller::Pipx(pipx.clone()), action),
            ),
            (Self::Uv(uv), Action::Update) => Some(Work::Program(
                uv.clone(),
                args(&["tool", "upgrade", "claude-swap"]),
            )),
            (Self::Uv(uv), Action::Reinstall) => Some(Work::Program(
                uv.clone(),
                args(&["tool", "install", "--force", "--reinstall", "claude-swap"]),
            )),
            (Self::Uv(uv), Action::Uninstall) => Some(Work::Program(
                uv.clone(),
                args(&["tool", "uninstall", "claude-swap"]),
            )),
            (Self::Pipx(pipx), action) => Some(Work::Program(
                pipx.clone(),
                args(&[verb(action), "claude-swap"]),
            )),
            (Self::Homebrew(brew), action) => Some(Work::Program(
                brew.clone(),
                args(&[verb(action), CODEX_SWAP_FORMULA]),
            )),
            (Self::Cargo { cargo, root }, Action::Update | Action::Reinstall) => {
                let mut list = args(&[
                    "install",
                    "--git",
                    CODEX_SWAP_REPOSITORY,
                    "--locked",
                    "--force",
                    "--root",
                ]);
                list.push(root.to_string_lossy().into_owned());
                Some(Work::Program(cargo.clone(), list))
            }
            (Self::Cargo { cargo, root }, Action::Uninstall) => {
                let mut list = args(&["uninstall", "codex-swap", "--root"]);
                list.push(root.to_string_lossy().into_owned());
                Some(Work::Program(cargo.clone(), list))
            }
            (Self::Script { dir, .. }, Action::Update | Action::Reinstall) => Some(Work::Script(
                format!("curl -fsSL {CODEX_SWAP_INSTALL_SH} | sh"),
                BTreeMap::from([(
                    "XSWAP_INSTALL_DIR".to_string(),
                    dir.to_string_lossy().into_owned(),
                )]),
            )),
            (Self::Script { xswap, dir }, Action::Uninstall) => {
                Some(Work::Remove(vec![xswap.clone(), dir.join(XSWAP_RECEIPT)]))
            }
            // xswap's own upgrade runs the official Windows installer into the same folder.
            (Self::WindowsInstaller(xswap), Action::Update | Action::Reinstall) => {
                Some(Work::Program(xswap.clone(), args(&["upgrade"])))
            }
            (Self::WindowsInstaller(_), Action::Uninstall) | (Self::Unknown, _) => None,
        }
    }
}

fn install_method(home: &Path, provider: Provider, executable: &Path) -> InstallMethod {
    let resolved = executable
        .canonicalize()
        .unwrap_or_else(|_| executable.to_path_buf());
    let normalized = resolved.to_string_lossy().replace('\\', "/").to_lowercase();
    match provider {
        Provider::Claude => {
            let uv = helpers::executable(home, "uv");
            if normalized.contains("/uv/tools/claude-swap/") {
                if let Some(uv) = uv {
                    return InstallMethod::Uv(uv);
                }
            }
            if normalized.contains("/pipx/venvs/claude-swap/") {
                if let Some(pipx) = helpers::executable(home, "pipx") {
                    return InstallMethod::Pipx(pipx);
                }
            }
            // uv copies its tool shims on Windows instead of linking them, so ask uv where its tools live.
            if let Some(uv) = uv {
                let tool_dir = crate::platform::process::background_command(&uv)
                    .args(["tool", "dir"])
                    .stdin(Stdio::null())
                    .output()
                    .ok()
                    .filter(|output| output.status.success())
                    .map(|output| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()));
                if tool_dir.is_some_and(|dir| dir.join("claude-swap").is_dir()) {
                    return InstallMethod::Uv(uv);
                }
            }
            InstallMethod::Unknown
        }
        Provider::Codex => {
            if let Some(dir) = script_install_dir(&resolved) {
                return InstallMethod::Script {
                    xswap: resolved,
                    dir,
                };
            }
            let Some(root) = resolved
                .parent()
                .filter(|bin| bin.ends_with("bin"))
                .and_then(Path::parent)
            else {
                return windows_installer(&resolved);
            };
            if root
                .parent()
                .is_some_and(|path| path.ends_with("Cellar/codex-swap"))
                && root.join("INSTALL_RECEIPT.json").is_file()
            {
                if let Some(brew) = helpers::executable(home, "brew") {
                    return InstallMethod::Homebrew(brew);
                }
            }
            let cargo_registered = std::fs::read_to_string(root.join(".crates.toml"))
                .ok()
                .and_then(|contents| contents.parse::<toml_edit::DocumentMut>().ok())
                .is_some_and(|receipt| {
                    receipt
                        .get("v1")
                        .and_then(toml_edit::Item::as_table)
                        .is_some_and(|packages| {
                            packages.iter().any(|(package, binaries)| {
                                package.starts_with("codex-swap ")
                                    && binaries.as_array().is_some_and(|binaries| {
                                        binaries
                                            .iter()
                                            .any(|binary| binary.as_str() == Some("xswap"))
                                    })
                            })
                        })
                });
            if cargo_registered {
                if let Some(cargo) = helpers::executable(home, "cargo") {
                    return InstallMethod::Cargo {
                        cargo,
                        root: root.to_path_buf(),
                    };
                }
            }
            windows_installer(&resolved)
        }
    }
}

/// The folder of a script-installed xswap: its receipt sits beside the binary and records the
/// binary's hash, so a copy another installer wrote there later is not mistaken for it (the same
/// rule `xswap upgrade` applies).
fn script_install_dir(xswap: &Path) -> Option<PathBuf> {
    let dir = xswap.parent()?;
    let receipt: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join(XSWAP_RECEIPT)).ok()?).ok()?;
    if receipt["method"] != "script" {
        return None;
    }
    let recorded = receipt["sha256"].as_str()?.to_lowercase();
    (managed_tools::download::sha256_file(xswap).ok()? == recorded).then(|| dir.to_path_buf())
}

fn windows_installer(xswap: &Path) -> InstallMethod {
    if cfg!(windows)
        && xswap.parent().is_some_and(|dir| {
            dir.ends_with("Programs/codex-swap") || dir.ends_with("Programs\\codex-swap")
        })
    {
        InstallMethod::WindowsInstaller(xswap.to_path_buf())
    } else {
        InstallMethod::Unknown
    }
}

/// How Install sets up the helper on this computer: tooltip text, and why it cannot when so.
fn install_plan(home: &Path, provider: Provider) -> (String, Option<String>) {
    match provider {
        Provider::Claude if claude_swap_windows::applies() => (
            format!("Downloads Ghostex's Windows build of Claude Swap ({}: the current release plus history sharing on Windows) from GitHub, checks its checksum and installs it with uv, which Ghostex installs first when needed. No password needed.", claude_swap_windows::VERSION),
            managed_tools::platform::arch()
                .is_none()
                .then(managed_tools::platform::unsupported_cpu_reason),
        ),
        Provider::Claude => {
            if helpers::executable(home, "uv").is_some() {
                ("Runs uv tool install claude-swap; uv downloads the Python it needs. No password needed.".into(), None)
            } else {
                (
                    "Ghostex first downloads uv from Astral into its tools folder, then runs uv tool install claude-swap; uv downloads the Python it needs. No password needed.".into(),
                    managed_tools::platform::arch()
                        .is_none()
                        .then(managed_tools::platform::unsupported_cpu_reason),
                )
            }
        }
        Provider::Codex if cfg!(windows) => (
            format!("Runs codex-swap's official Windows installer (irm {CODEX_SWAP_INSTALL_PS1} | iex), which puts xswap in your user folder and on your PATH. No password needed."),
            None,
        ),
        Provider::Codex => {
            let plan = format!("Runs codex-swap's official installer (curl -fsSL {CODEX_SWAP_INSTALL_SH} | sh), which downloads the build for this computer, checks its checksum and puts xswap in ~/.local/bin. No password needed.");
            let missing = managed_tools::system_tools::missing(home, &["curl", "ca-certificates"]);
            if missing.is_empty() || managed_tools::system_tools::can_install_in_background() {
                (plan, None)
            } else {
                (plan, Some("This installer needs curl and certificates. Use Install system tools in Settings > Integrations > Tools, then try again.".into()))
            }
        }
    }
}

fn installed_version(executable: &Path) -> Option<String> {
    let mut command = crate::platform::process::background_command(executable);
    command.arg("--version");
    let stdout = crate::agent_hooks::probing::run_command_stdout_with_timeout(
        command,
        managed_tools::run::PROBE_TIMEOUT,
    )?;
    managed_tools::tools::version_in(&stdout)
}

static LATEST: Mutex<Option<HashMap<&'static str, (Instant, Result<String, String>)>>> =
    Mutex::new(None);

/// The newest published release: PyPI for claude-swap, the codex-swap `releases/latest` redirect
/// for xswap. Cached for six hours; `fresh` is the Settings "check again" click.
fn latest_version(provider: Provider, fresh: bool) -> Result<String, String> {
    let key = provider.helper();
    if provider == Provider::Claude && claude_swap_windows::applies() {
        return Ok(claude_swap_windows::VERSION.to_string());
    }
    if !fresh {
        let cache = LATEST.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, answer)) = cache.as_ref().and_then(|cache| cache.get(key)) {
            if at.elapsed() < LATEST_TTL {
                return answer.clone();
            }
        }
    }
    let answer = match provider {
        Provider::Claude => {
            managed_tools::download::get_json("https://pypi.org/pypi/claude-swap/json")
                .ok()
                .and_then(|value| {
                    value
                        .pointer("/info/version")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
        }
        Provider::Codex => managed_tools::download::latest_release_tag(&format!(
            "{CODEX_SWAP_REPOSITORY}/releases/latest"
        ))
        .ok()
        .and_then(|tag| managed_tools::tools::version_in(&tag)),
    }
    .ok_or_else(|| {
        "Couldn't reach the release server. Check your connection and try again.".to_string()
    });
    LATEST
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(key, (Instant::now(), answer.clone()));
    answer
}

fn job_key(provider: Provider) -> String {
    format!("accountHelper:{}", provider.helper())
}

/// The job as the Accounts page reads it (`AccountHelperTool.job`).
fn job_view(provider: Provider) -> Value {
    managed_tools::jobs::get(&job_key(provider)).map_or(Value::Null, |job| {
        let status = match job.status {
            "succeeded" => "complete",
            "failed" => "failed",
            _ => "running",
        };
        json!({"action":job.operation,"status":status,"output":job.output,"error":job.error,"finishedAt":job.finished_at})
    })
}

fn tool_view(home: &Path, provider: Provider, fresh: bool) -> Value {
    let job = job_view(provider);
    let running = job.get("status").and_then(Value::as_str) == Some("running");
    let (plan, blocker) = install_plan(home, provider);
    let Some(executable) = helpers::executable(home, provider.helper()) else {
        let actions: Vec<&str> = if blocker.is_none() {
            vec!["install"]
        } else {
            Vec::new()
        };
        return json!({"provider":provider,"installed":false,"actions":actions,"installPlan":plan,"unavailableReason":blocker,"job":job});
    };
    let method = install_method(home, provider, &executable);
    let pending = (provider == Provider::Claude)
        .then(claude_swap_switch::pending)
        .flatten();
    // A waiting switch offers nothing that would replace the files running sessions use.
    let actions: Vec<&str> = [Action::Update, Action::Reinstall, Action::Uninstall]
        .into_iter()
        .filter(|action| pending.is_none() && method.work(*action).is_some())
        .map(Action::id)
        .collect();
    // A running update replaces the binary under us; its version is read again once it finishes.
    let version = (!running).then(|| installed_version(&executable)).flatten();
    let latest = latest_version(provider, fresh);
    let update_available = match (&version, &latest) {
        // Any other version than Ghostex's Windows build (PyPI's, which refuses to share history
        // there) is one to replace; `newer` reads only the numbers before `.post1+ghostex`.
        (Some(current), Ok(latest))
            if provider == Provider::Claude && claude_swap_windows::applies() =>
        {
            Some(current != latest)
        }
        (Some(current), Ok(latest)) => Some(managed_tools::tools::newer(latest, current)),
        _ => None,
    }
    .map(|available| available && pending.is_none());
    json!({
        "provider": provider,
        "installed": true,
        "path": executable,
        "installMethod": method.id(),
        "actions": actions,
        "installPlan": plan,
        "version": version,
        "latestVersion": latest.as_ref().ok(),
        "updateAvailable": update_available,
        "checkError": latest.err(),
        "job": job,
        "pendingSwitch": pending.map(|pending| json!({
            "action": pending.action,
            "message": claude_swap_switch::waiting_message(&pending),
        })),
    })
}

fn response(home: &Path, fresh: bool) -> Value {
    let tools: Vec<Value> = std::thread::scope(|scope| {
        let tasks: Vec<_> = [Provider::Claude, Provider::Codex]
            .into_iter()
            .map(|provider| scope.spawn(move || tool_view(home, provider, fresh)))
            .collect();
        tasks
            .into_iter()
            .filter_map(|task| task.join().ok())
            .collect()
    });
    json!({"accounts":[],"helpers":[],"defaults":{"claude":{},"codex":{}},"defaultAccounts":{},"newSessionAccounts":{},"helperTools":tools})
}

/// CDXC:AgentProviders 2026-09-28 DECISION:
/// User: Settings > Accounts updates, reinstalls and uninstalls Claude Swap and Codex Swap with icon buttons and tooltips, and checks for their updates the way the Trycua row does.
/// Each action runs through the tool that owns the install (uv or pipx for cswap; Homebrew, Cargo with its original `--root`, codex-swap's `install.sh` or the Windows installer for xswap), read from where the executable lives, so an update replaces the copy Ghostex actually runs instead of adding a second one. An install Ghostex cannot place offers no actions rather than guessing a command. Uninstall removes only the program: saved logins and shared conversations stay.
///
/// CDXC:ManagedTools 2026-09-29 DECISION:
/// User: one click installs a missing helper. cswap installs with uv (Ghostex installs uv first when missing, and uv fetches the Python it needs); xswap installs with codex-swap's official `install.sh` on macOS and Linux (user decision 4A) and its `install.ps1` on Windows. Supersedes the copyable install command the Accounts page showed.
pub(crate) fn dispatch(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let home = state.paths.home_dir.clone();
    let operation = params
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let fresh = params.get("fresh").and_then(Value::as_bool) == Some(true);
    match operation {
        "helperStatus" => Ok(response(&home, fresh)),
        "helperAction" => {
            let provider: Provider =
                serde_json::from_value(params.get("provider").cloned().unwrap_or_default())
                    .map_err(|_| DomainStateError::bad_request("Choose Claude or Codex."))?;
            let action = params
                .get("action")
                .and_then(Value::as_str)
                .and_then(Action::parse)
                .ok_or_else(|| {
                    DomainStateError::bad_request("Choose install, update, reinstall or uninstall.")
                })?;
            let work = match (action, helpers::executable(&home, provider.helper())) {
                (Action::Install, Some(_)) => {
                    return Err(DomainStateError::bad_request("It's already installed."))
                }
                (Action::Install, None) => {
                    if let (_, Some(reason)) = install_plan(&home, provider) {
                        return Err(DomainStateError::bad_request(reason));
                    }
                    match provider {
                        Provider::Claude if claude_swap_windows::applies() => {
                            Work::WindowsBuild(WheelInstaller::Uv(None), Action::Install)
                        }
                        Provider::Claude => Work::UvInstall,
                        Provider::Codex if cfg!(windows) => Work::Script(
                            format!("irm {CODEX_SWAP_INSTALL_PS1} | iex"),
                            BTreeMap::new(),
                        ),
                        Provider::Codex => Work::Script(
                            format!("curl -fsSL {CODEX_SWAP_INSTALL_SH} | sh"),
                            BTreeMap::new(),
                        ),
                    }
                }
                (_, None) => return Err(DomainStateError::bad_request("It isn't installed.")),
                (_, Some(_)) if provider == Provider::Claude && claude_swap_switch::pending().is_some() => {
                    let message = claude_swap_switch::pending()
                        .map(|pending| claude_swap_switch::waiting_message(&pending))
                        .unwrap_or_default();
                    return Err(DomainStateError::bad_request(format!("{message}.")));
                }
                (_, Some(executable)) => install_method(&home, provider, &executable)
                    .work(action)
                    .ok_or_else(|| {
                        DomainStateError::bad_request(
                            "Ghostex can't manage this install. Use the tool you installed it with.",
                        )
                    })?,
            };
            start_job(
                &tokio::runtime::Handle::current(),
                &home,
                provider,
                action,
                work,
            )
            .map_err(DomainStateError::bad_request)?;
            Ok(response(&home, false))
        }
        _ => Err(DomainStateError::bad_request(
            "Unknown account tool action.",
        )),
    }
}

fn start_job(
    runtime: &tokio::runtime::Handle,
    home: &Path,
    provider: Provider,
    action: Action,
    work: Work,
) -> Result<(), String> {
    let key = job_key(provider);
    managed_tools::jobs::begin(&key, action.id())?;
    let job_home = home.to_path_buf();
    runtime.spawn(async move {
        let _turn = managed_tools::INSTALL_LOCK.lock().await;
        managed_tools::jobs::set_status(&key, "running");
        let log = managed_tools::jobs::log_for(&key);
        let result = run_work(work, provider, &job_home, &log).await;
        if let Err(error) = &result {
            log.line(error);
        }
        managed_tools::jobs::finish(&key, &result);
    });
    Ok(())
}

/// Replaces a Windows cswap that refuses to share history with Ghostex's Windows build, in the
/// background, as the Update button in Settings > Accounts would (`Ok` while one already runs).
/// `Err` says why Ghostex cannot place it.
pub(super) fn install_windows_build_in_background(home: &Path) -> Result<(), String> {
    // A switch to this build already waits for the installed one to be free.
    if claude_swap_switch::pending().is_some() {
        return Ok(());
    }
    let work = match helpers::executable(home, Provider::Claude.helper()) {
        None => Work::WindowsBuild(WheelInstaller::Uv(None), Action::Install),
        Some(executable) => install_method(home, Provider::Claude, &executable)
            .work(Action::Update)
            .filter(|work| matches!(work, Work::WindowsBuild(..)))
            .ok_or_else(|| {
                format!(
                    "Ghostex can't replace the Claude Swap at {}. Uninstall it with the tool you installed it with, then click Install next to Claude Swap in Settings > Accounts.",
                    executable.display()
                )
            })?,
    };
    let runtime = tokio::runtime::Handle::try_current()
        .map_err(|_| "Ghostex could not start the install. Click Update next to Claude Swap in Settings > Accounts.".to_string())?;
    if managed_tools::jobs::get(&job_key(Provider::Claude)).is_some_and(|job| job.active()) {
        return Ok(());
    }
    start_job(&runtime, home, Provider::Claude, Action::Update, work)
}

/// Moves an installed Windows cswap that is not Ghostex's pinned build, or no longer starts, to
/// that build in the background (claude_swap_switch.rs asks once an hour).
pub(super) fn update_outdated(home: &Path) {
    if !claude_swap_windows::applies() || claude_swap_switch::pending().is_some() {
        return;
    }
    let Some(executable) = helpers::executable(home, Provider::Claude.helper()) else {
        return;
    };
    if installed_version(&executable).as_deref() != Some(claude_swap_windows::VERSION) {
        // An install Ghostex cannot place stays as it is; Settings > Accounts says so.
        let _ = install_windows_build_in_background(home);
    }
}

async fn run_work(
    work: Work,
    provider: Provider,
    home: &Path,
    log: &managed_tools::Log,
) -> Result<(), String> {
    let timeout = Duration::from_secs(20 * 60);
    let installs_into_local_bin = matches!(
        &work,
        Work::UvInstall
            | Work::WindowsBuild(
                WheelInstaller::Uv(_),
                Action::Install | Action::Update | Action::Reinstall
            )
    ) || matches!(&work, Work::Script(script, env) if script.contains("install.sh") && !env.contains_key("XSWAP_INSTALL_DIR"));
    match work {
        Work::UvInstall => {
            let uv = managed_tools::ensure_uv(home, log).await?;
            let owned_log = log.clone();
            tokio::task::spawn_blocking(move || {
                managed_tools::run::run(
                    &uv,
                    &["tool", "install", "claude-swap"],
                    &[],
                    &[],
                    &owned_log,
                    timeout,
                )
            })
            .await
            .map_err(|error| error.to_string())??;
        }
        Work::WindowsBuild(WheelInstaller::Uv(None), _) => {
            // Nothing is installed, so nothing can be in use: install straight into place.
            let uv = managed_tools::ensure_uv(home, log).await?;
            let owned_log = log.clone();
            tokio::task::spawn_blocking(move || {
                let wheel = claude_swap_windows::wheel(&owned_log)?;
                let wheel = wheel.to_string_lossy().into_owned();
                managed_tools::run::run(
                    &uv,
                    &["tool", "install", "--force", &wheel],
                    &[],
                    &[],
                    &owned_log,
                    timeout,
                )
            })
            .await
            .map_err(|error| error.to_string())??;
        }
        Work::WindowsBuild(installer, action) => {
            let (program, owner) = match installer {
                WheelInstaller::Uv(uv) => (uv.unwrap_or_default(), claude_swap_switch::Owner::Uv),
                WheelInstaller::Pipx(pipx) => (pipx, claude_swap_switch::Owner::Pipx),
            };
            // The side-by-side copy is installed with uv, also next to a pipx install.
            let uv = match (owner, action) {
                (claude_swap_switch::Owner::Uv, _) | (_, Action::Uninstall) => program.clone(),
                _ => managed_tools::ensure_uv(home, log).await?,
            };
            let owned_log = log.clone();
            let owned_home = home.to_path_buf();
            tokio::task::spawn_blocking(move || {
                // `--force` replaces a cswap installed from PyPI under the same name.
                claude_swap_switch::replace(
                    action.id(),
                    owner,
                    &program,
                    &uv,
                    &owned_home,
                    &owned_log,
                )
            })
            .await
            .map_err(|error| error.to_string())??;
        }
        Work::Program(program, args) => {
            let owned_log = log.clone();
            tokio::task::spawn_blocking(move || {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                managed_tools::run::run(
                    &program,
                    &args,
                    &[
                        ("HOMEBREW_NO_ENV_HINTS", "1".as_ref()),
                        ("NONINTERACTIVE", "1".as_ref()),
                    ],
                    &[],
                    &owned_log,
                    timeout,
                )
            })
            .await
            .map_err(|error| error.to_string())??;
        }
        Work::Script(script, env) => {
            if script.starts_with("curl ") {
                managed_tools::ensure_system_tools(home, &["curl", "ca-certificates"], log).await?;
            }
            crate::agent_cli::process::run(&script, home, &env, timeout, log.sink()).await?;
        }
        Work::Remove(files) => {
            for file in files {
                if file.exists() {
                    std::fs::remove_file(&file)
                        .map_err(|error| format!("Could not remove {}: {error}", file.display()))?;
                }
            }
            log.line(&format!("Removed {}.", provider.helper()));
        }
    }
    if installs_into_local_bin {
        let local_bin = home.join(".local").join("bin");
        if let Ok(Some(message)) = crate::agent_cli::path_setup::ensure_on_path(&local_bin, home) {
            log.line(&message);
        }
    }
    crate::agent_hooks::probing::refresh_cli_environment(home);
    Ok(())
}
