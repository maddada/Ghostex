//! The user's own Executor CLI (`executor`): where it is, its version, what `executor whoami`
//! says about its sign-in, the apps that sign-in reaches, and the command line every run of it
//! gets.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::managed_tools::run::{output_within, Unfinished, PROBE_TIMEOUT};

/// Long enough for a sign-in's network round trip, short enough that Settings never hangs on it.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(20);
/// What `executor login` signs in to when no host is named.
pub(super) const HOSTED_HOST: &str = "https://api.executor.sh";
/// Where hosted Executor's apps and accounts are managed.
const HOSTED_DASHBOARD: &str = "https://app.executor.sh";

/// The folders Executor's own installers put `executor` in, for a gxserver whose PATH predates
/// the install: `install.sh` links `~/.local/bin/executor`, `install.ps1` adds
/// `%USERPROFILE%\.executor\v2\runtime\current` to the user's PATH.
fn install_dirs() -> Vec<PathBuf> {
    let home = crate::resume_lookup::home_dir();
    if cfg!(windows) {
        vec![home
            .join(".executor")
            .join("v2")
            .join("runtime")
            .join("current")]
    } else {
        vec![home.join(".local").join("bin")]
    }
}

/// An `executor` command with the environment every run gets: the live PATH (so a CLI installed
/// while gxserver runs is found) and no telemetry or update prompt. The user's own `EXECUTOR_*`
/// variables (a host, an access token) are kept, as their terminal would.
pub(super) fn command(executable: &Path) -> Command {
    let mut command = crate::platform::process::background_command(executable);
    command
        .env("PATH", crate::platform::live_path::value())
        .env("DO_NOT_TRACK", "1")
        .env("EXECUTOR_NO_UPDATE_CHECK", "1")
        .env("NO_COLOR", "1");
    command
}

/// The version an `executor` reports (`executor v2.0.0-beta.79` gives `2.0.0-beta.79`).
fn read_version(executable: &Path) -> Option<String> {
    let mut command = command(executable);
    command.arg("--version");
    let output = output_within(command, PROBE_TIMEOUT).ok()?;
    let line = output.stdout.trim();
    Some(
        line.strip_prefix("executor v")
            .unwrap_or(line)
            .trim()
            .to_string(),
    )
    .filter(|version| !version.is_empty())
}

/// Versions already read, by program and its modification time, so each Settings read doesn't
/// start the CLI again.
static VERSIONS: Mutex<Vec<(PathBuf, Option<SystemTime>, Option<String>)>> = Mutex::new(Vec::new());

fn version_of(executable: &Path) -> Option<String> {
    let modified = std::fs::metadata(executable)
        .and_then(|meta| meta.modified())
        .ok();
    let mut cache = VERSIONS.lock().unwrap_or_else(|error| error.into_inner());
    if let Some((_, _, version)) = cache
        .iter()
        .find(|(path, at, _)| path == executable && *at == modified)
    {
        return version.clone();
    }
    let version = read_version(executable);
    cache.retain(|(path, _, _)| path != executable);
    cache.push((executable.to_path_buf(), modified, version.clone()));
    version
}

/// The CLI Ghostex uses: the first `executor` on PATH or in the installers' folders that is
/// Executor 2, else the first one found (which status then reports as too old).
#[derive(Clone, Debug)]
pub(super) struct Found {
    pub(super) executable: PathBuf,
    pub(super) version: Option<String>,
}

impl Found {
    /// Executor 2, the one with `login`, `whoami` and the `mcp` relay. Executor 1, which plain
    /// `npm i -g executor` still installs, has an `executor mcp` that is a different program.
    pub(super) fn usable(&self) -> bool {
        self.version
            .as_deref()
            .is_some_and(|version| version.starts_with("2."))
    }
}

pub(super) fn find() -> Option<Found> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(on_path) = crate::platform::live_path::find("executor", &[]) {
        candidates.push(on_path);
    }
    if let Some(installed) = crate::platform::live_path::find_in("executor", &install_dirs()) {
        if !candidates.contains(&installed) {
            candidates.push(installed);
        }
    }
    let found: Vec<Found> = candidates
        .into_iter()
        .map(|executable| Found {
            version: version_of(&executable),
            executable,
        })
        .collect();
    found
        .iter()
        .find(|candidate| candidate.usable())
        .or_else(|| found.first())
        .cloned()
}

/// What the CLI's sign-in reaches, from `executor whoami`.
#[derive(Clone, Debug)]
pub(super) struct Whoami {
    pub(super) host: String,
    pub(super) organization: Option<String>,
    pub(super) access: String,
    pub(super) mode: String,
}

impl Whoami {
    /// The host as Settings names it: `executor.sh` for hosted Executor.
    pub(super) fn host_label(&self) -> String {
        if self.host == HOSTED_HOST {
            return "executor.sh".to_string();
        }
        self.host
            .split("://")
            .nth(1)
            .unwrap_or(&self.host)
            .trim_end_matches('/')
            .to_string()
    }

    /// Where its apps and accounts are managed in the browser.
    pub(super) fn manage_url(&self) -> String {
        if self.host == HOSTED_HOST {
            HOSTED_DASHBOARD.to_string()
        } else {
            self.host.clone()
        }
    }
}

/// `executor whoami`'s account description, in the CLI's own words (`describeAccess`).
fn describe_access(access: &Value) -> String {
    let apps = |count: u64| format!("{count} {}", if count == 1 { "app" } else { "apps" });
    let count = access.get("apps").and_then(Value::as_u64).unwrap_or(0);
    match access.get("kind").and_then(Value::as_str) {
        Some("all") => "all apps".to_string(),
        Some("connection") => format!(
            "connection {} ({})",
            access.get("name").and_then(Value::as_str).unwrap_or("?"),
            apps(count)
        ),
        Some("tools") => format!("selected tools of {}", apps(count)),
        _ => "some apps".to_string(),
    }
}

/// The `{ error: { code, message } }` the CLI prints on stdout when a command fails.
fn cli_error(stdout: &str) -> Option<(String, String)> {
    let start = stdout.find("{\n")?;
    let value: Value = serde_json::from_str(stdout[start..].trim()).ok()?;
    let error = value.get("error")?;
    Some((
        error.get("code")?.as_str()?.to_string(),
        error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    ))
}

fn run_json(executable: &Path, args: &[&str]) -> Result<Value, (Option<String>, String)> {
    let mut command = command(executable);
    command.args(args);
    let output = match output_within(command, NETWORK_TIMEOUT) {
        Ok(output) => output,
        Err(Unfinished::TimedOut(_)) => {
            return Err((
                None,
                format!(
                    "executor {} didn't answer within 20 seconds.",
                    args.join(" ")
                ),
            ));
        }
        Err(Unfinished::Start) => {
            return Err((None, "The Executor CLI couldn't be started.".to_string()));
        }
    };
    if output.status.success() {
        return serde_json::from_str(output.stdout.trim()).map_err(|_| {
            (
                None,
                format!(
                    "executor {} printed something Ghostex can't read.",
                    args.join(" ")
                ),
            )
        });
    }
    match cli_error(&output.stdout) {
        Some((code, message)) => Err((Some(code), message)),
        None => Err((
            None,
            format!("executor {} failed ({}).", args.join(" "), output.status),
        )),
    }
}

/// The CLI's sign-in: `Ok(None)` when it has none, an error when it couldn't be read.
pub(super) fn whoami(executable: &Path) -> Result<Option<Whoami>, String> {
    match run_json(executable, &["whoami"]) {
        Ok(value) => Ok(Some(Whoami {
            host: value
                .get("host")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            organization: value
                .pointer("/organization/slug")
                .and_then(Value::as_str)
                .map(str::to_string),
            access: describe_access(value.get("access").unwrap_or(&Value::Null)),
            mode: value
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })),
        Err((Some(code), _)) if code == "NotSignedIn" || code == "ToolsSignInRequired" => Ok(None),
        Err((_, message)) => Err(message),
    }
}

/// An app the sign-in reaches.
#[derive(Clone, Debug)]
pub(super) struct App {
    pub(super) id: String,
    pub(super) slug: String,
    pub(super) name: String,
}

/// Executor's own app-management app, which Ghostex neither lists nor lets agents call.
pub(super) const MANAGEMENT_SLUG: &str = "executor";

/// The apps `executor apps list` shows for the sign-in, without Executor's own.
pub(super) fn apps(executable: &Path, host: &str) -> Result<Vec<App>, String> {
    let value = run_json(executable, &["apps", "list", "--host", host]).map_err(|(_, m)| m)?;
    let list = match &value {
        Value::Array(items) => items.clone(),
        other => other
            .get("apps")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };
    Ok(list
        .iter()
        .filter_map(|app| {
            let text = |key: &str| app.get(key).and_then(Value::as_str).map(str::to_string);
            let slug = text("slug")?;
            (slug != MANAGEMENT_SLUG).then(|| App {
                id: text("id").unwrap_or_default(),
                name: text("name").unwrap_or_else(|| slug.clone()),
                slug,
            })
        })
        .collect())
}
