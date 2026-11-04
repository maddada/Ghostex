use super::{
    catalog::{self, Definition, CATALOG},
    latest, mise, path_setup, process, update_check,
};
use crate::{domain::DomainStateError, paths::GxserverPaths};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

#[derive(Clone)]
struct Job {
    state: Value,
    progress: Value,
}

static JOBS: LazyLock<Mutex<HashMap<(PathBuf, String), Job>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static PROBES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

pub(crate) async fn dispatch(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let home = paths.agent_config_home_dir().to_path_buf();
    let action = params
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("read");
    if action == "list" {
        return list(&home).await;
    }
    let agent_id = params
        .get("agentId")
        .and_then(Value::as_str)
        .ok_or_else(|| refused("Missing agent id."))?;
    let definition = CATALOG
        .iter()
        .find(|entry| entry.agent_id == agent_id)
        .ok_or_else(|| refused("Unknown agent CLI."))?;
    match action {
        "read" => read(definition, &home).await,
        "start" => start(definition, paths, params).await,
        "addToPath" => add_to_path(definition, &home).await,
        _ => Err(refused("Unknown CLI action.")),
    }
}

/// One agent's state as the Agents page reads it: the copy a new terminal runs, its version, how it was
/// installed and the update command for it.
pub(crate) async fn read_agent(home: &Path, agent_id: &str) -> Result<Value, DomainStateError> {
    let definition = CATALOG
        .iter()
        .find(|entry| entry.agent_id == agent_id)
        .ok_or_else(|| refused("Unknown agent CLI."))?;
    read(definition, home).await
}

/// Whether an install or update of `agent_id` is queued or running (the Agents page, the CLI, or another
/// session's Codex "Update now").
pub(crate) fn job_active(home: &Path, agent_id: &str) -> bool {
    JOBS.lock().ok().is_some_and(|jobs| {
        jobs.get(&(home.to_path_buf(), agent_id.to_string()))
            .is_some_and(|job| is_active(&job.progress))
    })
}

/// Updates an installed agent CLI with `method_id` (the method that installed it), the way `ghostex
/// agent-cli update` does, and waits for the job. `Err` carries the job's error and the end of its output.
pub(crate) async fn update_and_wait(
    paths: &GxserverPaths,
    agent_id: &str,
    method_id: &str,
) -> Result<Value, String> {
    run_and_wait(paths, agent_id, "update", method_id).await
}

/// Installs a missing agent CLI with the method Settings > Agents would pick (the first one this
/// computer can run) and waits for the job, for Install on a failed start's card.
pub(crate) async fn install_default_and_wait(
    paths: &GxserverPaths,
    agent_id: &str,
) -> Result<Value, String> {
    let state = read_agent(paths.agent_config_home_dir(), agent_id)
        .await
        .map_err(|error| error.message)?;
    if state["executablePath"].is_string() {
        return Ok(state);
    }
    let methods = state["methods"].as_array().cloned().unwrap_or_default();
    let method = methods
        .iter()
        .find(|method| method["unavailableReason"].is_null())
        .ok_or_else(|| {
            methods
                .first()
                .and_then(|method| method["unavailableReason"].as_str())
                .unwrap_or("Ghostex has no way to install this CLI on this computer.")
                .to_string()
        })?;
    let method_id = method["id"].as_str().unwrap_or_default().to_string();
    run_and_wait(paths, agent_id, "install", &method_id).await
}

async fn run_and_wait(
    paths: &GxserverPaths,
    agent_id: &str,
    operation: &str,
    method_id: &str,
) -> Result<Value, String> {
    let mut params = Map::new();
    params.insert("action".into(), json!("start"));
    params.insert("agentId".into(), json!(agent_id));
    params.insert("operation".into(), json!(operation));
    params.insert("methodId".into(), json!(method_id));
    let started = dispatch(paths, &params)
        .await
        .map_err(|error| error.message)?;
    let job_id = started["job"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let deadline = std::time::Instant::now() + Duration::from_secs(20 * 60);
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let state = read_agent(paths.agent_config_home_dir(), agent_id)
            .await
            .map_err(|error| error.message)?;
        let job = &state["job"];
        if job["id"].as_str() != Some(job_id.as_str()) {
            return Err(format!("The {operation} job disappeared."));
        }
        match job["status"].as_str() {
            Some("succeeded") => return Ok(state),
            Some("failed") => {
                let output = job["output"].as_str().unwrap_or_default().trim();
                let tail: Vec<&str> = output.lines().rev().take(12).collect();
                let tail = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
                let failed = format!("The {operation} failed.");
                let reason = job["error"].as_str().unwrap_or(&failed);
                return Err(if tail.is_empty() {
                    reason.to_string()
                } else {
                    format!("{reason}\n{tail}")
                });
            }
            _ if std::time::Instant::now() > deadline => {
                return Err(format!("The {operation} did not finish within 20 minutes."))
            }
            _ => {}
        }
    }
}

/// Every catalog agent's state in one reply, for the Agents page and `ghostex agent-cli status`.
async fn list(home: &Path) -> Result<Value, DomainStateError> {
    let states =
        futures_util::future::join_all(CATALOG.iter().map(|definition| read(definition, home)))
            .await;
    let agents = states.into_iter().collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "agents": agents }))
}

fn is_active(progress: &Value) -> bool {
    matches!(progress["status"].as_str(), Some("queued" | "running"))
}

async fn read(definition: &'static Definition, home: &Path) -> Result<Value, DomainStateError> {
    let key = (home.to_path_buf(), definition.agent_id.clone());
    let previous = JOBS.lock().map_err(error)?.get(&key).cloned();
    if let Some(job) = &previous {
        if is_active(&job.progress) {
            let mut state = job.state.clone();
            state["job"] = job.progress.clone();
            if let Some(name) = crate::sidebar_hud::default_agent_name(&definition.agent_id) {
                state["name"] = json!(name);
            }
            return Ok(state);
        }
    }
    let mut state = read_state(definition, home).await?;
    if let Some(job) = previous {
        state["job"] = job.progress;
    }
    // CDXC:RemoteMachines 2026-10-26 WHY: the phone's machine screen lists agents by this answer
    // alone, so it carries the built-in agent's display name.
    if let Some(name) = crate::sidebar_hud::default_agent_name(&definition.agent_id) {
        state["name"] = json!(name);
    }
    Ok(state)
}

async fn add_to_path(
    definition: &'static Definition,
    home: &Path,
) -> Result<Value, DomainStateError> {
    let state = read_state(definition, home).await?;
    let Some(directory) = state["pathDirectory"].as_str().map(PathBuf::from) else {
        return Ok(state);
    };
    let owned_home = home.to_path_buf();
    tokio::task::spawn_blocking(move || path_setup::ensure_on_path(&directory, &owned_home))
        .await
        .map_err(error)?
        .map_err(error)?;
    read(definition, home).await
}

async fn start(
    definition: &'static Definition,
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let home = paths.agent_config_home_dir();
    let key = (home.to_path_buf(), definition.agent_id.clone());
    if JOBS
        .lock()
        .map_err(error)?
        .get(&key)
        .is_some_and(|job| is_active(&job.progress))
    {
        return Err(refused("This CLI is already being installed or updated."));
    }
    let mut state = read_state(definition, home).await?;
    let operation = params
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(|| refused("Missing install/update operation."))?;
    let installed = state["executablePath"].is_string();
    if operation != if installed { "update" } else { "install" } {
        return Err(refused(
            "CLI installation changed. Refresh its status before continuing.",
        ));
    }
    let method_id = params
        .get("methodId")
        .and_then(Value::as_str)
        .ok_or_else(|| refused("Choose an installation method."))?;
    if method_id == "manual" {
        return Err(refused(format!(
            "Another tool (mise, Scoop or Nix) manages {} at {}, so Ghostex does not update it. Update it with that tool.",
            definition.binary,
            state["executablePath"].as_str().unwrap_or_default()
        )));
    }
    let method = state["methods"]
        .as_array()
        .and_then(|methods| methods.iter().find(|method| method["id"] == method_id))
        .ok_or_else(|| refused("Unsupported installation method."))?;
    if let Some(reason) = method["unavailableReason"].as_str() {
        return Err(refused(reason));
    }
    if installed {
        if let Some(detected) = state["detectedMethodId"].as_str() {
            if method_id != detected {
                return Err(refused(
                    "Use the detected installation method to update this CLI.",
                ));
            }
        }
    }
    let script = method["command"]
        .as_str()
        .ok_or_else(|| error("Missing CLI command."))?
        .to_string();
    let mut env = if method_id == "native" || method_id == "self" {
        definition.native_env()
    } else {
        BTreeMap::new()
    };
    let prerequisite = method["prerequisite"].as_str().map(str::to_string);
    let system_tools: Vec<String> = method["systemTools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    // An npm CLI Ghostex installed with its own Node.js updates through that same npm.
    let managed_node = state["executablePath"]
        .as_str()
        .is_some_and(|path| Path::new(path).starts_with(crate::managed_tools::paths::node_dir()));
    let id = uuid::Uuid::new_v4().to_string();
    let progress =
        json!({"id":id,"operation":operation,"command":script,"status":"queued","output":""});
    {
        let mut jobs = JOBS.lock().map_err(error)?;
        jobs.insert(
            key.clone(),
            Job {
                state: state.clone(),
                progress: progress.clone(),
            },
        );
    }
    state["job"] = progress;
    let home = home.to_path_buf();
    let paths = paths.clone();
    let installing = operation == "install";
    tokio::spawn(async move {
        /*
        CDXC:AgentProviders 2026-09-28 WHY:
        Installs and updates run one at a time (installers can share an npm prefix, the user PATH and the shell profile), but a second request waits in the queue instead of being refused, so onboarding can install Claude, Codex, Cursor and Grok from one pass. The queue is shared with the tools Ghostex installs itself (managed_tools::INSTALL_LOCK).
        */
        let _turn = crate::managed_tools::INSTALL_LOCK.lock().await;
        set_job_status(&key, "running");
        let log = crate::managed_tools::Log::new({
            let key = key.clone();
            move |chunk| append_output(&key, &chunk)
        });
        let prepared = prepare_prerequisite(
            prerequisite.as_deref(),
            &system_tools,
            managed_node,
            &home,
            &log,
        )
        .await;
        let result = match prepared {
            Ok(prefix) => {
                if !prefix.is_empty() {
                    env.insert(
                        "PATH".into(),
                        crate::managed_tools::run::job_path(&prefix)
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
                process::run(&script, &home, &env, Duration::from_secs(900), {
                    let key = key.clone();
                    move |chunk| append_output(&key, &chunk)
                })
                .await
            }
            Err(error) => Err(error),
        };
        let result = match result {
            Ok(()) => finish(definition, &home, &key).await,
            Err(error) => Err(error),
        };
        let notice = match &result {
            Ok(verified) if !installing => update_notice(definition, verified, &key),
            _ => None,
        };
        if result.is_ok() && installing {
            install_chat_hooks(definition, &paths, &key).await;
        }
        if let Ok(mut jobs) = JOBS.lock() {
            if let Some(job) = jobs.get_mut(&key) {
                job.progress["status"] = json!(if result.is_ok() {
                    "succeeded"
                } else {
                    "failed"
                });
                if let Some(notice) = notice {
                    job.progress["notice"] = json!(notice);
                }
                if let Err(error) = result {
                    job.progress["error"] = json!(error);
                }
            }
        }
    });
    Ok(state)
}

/// Installs what the chosen method needs first and returns the folders to put first on its PATH.
async fn prepare_prerequisite(
    prerequisite: Option<&str>,
    system_tools: &[String],
    managed_node: bool,
    home: &Path,
    log: &crate::managed_tools::Log,
) -> Result<Vec<PathBuf>, String> {
    if managed_node {
        return Ok(vec![crate::managed_tools::paths::node_bin_dir()]);
    }
    match prerequisite {
        Some("node") => crate::managed_tools::ensure_npm(home, log).await,
        Some("mise") => crate::managed_tools::ensure_mise(home, log).await,
        Some("homebrew") => crate::managed_tools::ensure_homebrew(home, log)
            .await
            .map(|brew| brew.parent().map(Path::to_path_buf).into_iter().collect()),
        Some("systemTools") => {
            let wanted: &'static [&'static str] = if system_tools.is_empty() {
                &["curl", "ca-certificates"]
            } else {
                &["curl", "ca-certificates", "unzip", "git"]
            };
            crate::managed_tools::ensure_system_tools(home, wanted, log)
                .await
                .map(|()| Vec::new())
        }
        _ => Ok(Vec::new()),
    }
}

/// Re-checks the CLI after its installer exited successfully, and puts its folder on PATH when the
/// installer did not (see `path_setup::ensure_on_path`).
async fn finish(
    definition: &'static Definition,
    home: &Path,
    key: &(PathBuf, String),
) -> Result<Value, String> {
    let mut verified = read_state_with(definition, home, VERSION_TIMEOUT_AFTER_INSTALL)
        .await
        .map_err(|error| error.message)?;
    if !verified["executablePath"].is_string() {
        return Err("The installer exited successfully, but the CLI is still missing. Check the installation docs, then refresh.".to_string());
    }
    if let Some(directory) = verified["pathDirectory"].as_str().map(PathBuf::from) {
        let owned_home = home.to_path_buf();
        let outcome = tokio::task::spawn_blocking(move || {
            path_setup::ensure_on_path(&directory, &owned_home)
        })
        .await
        .map_err(|error| error.to_string())?;
        match outcome {
            Ok(Some(message)) => append_output(key, &format!("\n{message}\n")),
            Ok(None) => {}
            Err(message) => append_output(key, &format!("\n{message}\n")),
        }
        verified = read_state_with(definition, home, VERSION_TIMEOUT_AFTER_INSTALL)
            .await
            .map_err(|error| error.message)?;
    }
    if let Some(reason) = verified["versionError"].as_str() {
        return Err(format!(
            "The installer finished, but the CLI version check failed: {reason}"
        ));
    }
    Ok(verified)
}

/// CDXC:AgentProviders 2026-10-17 WHY:
/// An updater that exits 0 without installing anything newer (mise holding a release back, or a mise plugin that pins Hermes to its commit) used to end in "CLI command completed", which read as up to date. A finished update that still leaves a newer release says so, and what to do next.
fn update_notice(
    definition: &Definition,
    verified: &Value,
    key: &(PathBuf, String),
) -> Option<String> {
    let output = JOBS
        .lock()
        .ok()
        .and_then(|jobs| jobs.get(key).map(|job| job.progress["output"].clone()))
        .and_then(|output| output.as_str().map(str::to_string))
        .unwrap_or_default();
    if verified["updateAvailable"] != true {
        return None;
    }
    if let Some(message) = verified["updateHeldBack"]
        .as_str()
        .map(str::to_string)
        .or_else(|| mise::held_back_message(&output, None))
    {
        return Some(message);
    }
    let newer = match (
        verified["latestVersion"].as_str(),
        verified["updateSummary"].as_str(),
    ) {
        (Some(latest), _) => format!(" ({latest})"),
        (None, Some(summary)) => format!(" ({})", summary.trim_end_matches('.')),
        (None, None) => String::new(),
    };
    let next = if verified["detectedMethodId"] == "mise" {
        format!(
            " mise decides which version it installs here, so check how your mise installs {} (its plugin or settings).",
            definition.binary
        )
    } else {
        " Check the command output for what stopped it.".to_string()
    };
    Some(format!(
        "The update finished, but {} still reports a newer version{newer}.{next}",
        definition.binary
    ))
}

/// CDXC:AgentHooks 2026-10-07 DECISION:
/// User: "if user has chat as the default interface and he installs a cli through ghostex then please install the hooks automatically also at the same time". The view is the agent's own Default view when it is Chat or Terminal, else the Default Agent View, which means Chat when unset (the 2026-09-30 rule in gx-core's `PreferredInterfaceSettings::resolve`). A hook install that fails is reported in the job output; the CLI install still succeeded and the Agents page keeps offering the hook.
async fn install_chat_hooks(
    definition: &'static Definition,
    paths: &GxserverPaths,
    key: &(PathBuf, String),
) {
    let agent_id = definition.agent_id.clone();
    let paths = paths.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let settings = crate::session_lifecycle::read_sidebar_settings(&paths);
        let interface = |value: Option<&Value>| {
            value
                .and_then(Value::as_str)
                .filter(|value| matches!(*value, "chat" | "terminal"))
                .map(str::to_string)
        };
        let chat = interface(
            settings
                .as_ref()
                .and_then(|settings| settings.pointer("/preferredAgentInterfaceOverrides"))
                .and_then(|overrides| overrides.get(&agent_id)),
        )
        .or_else(|| {
            interface(
                settings
                    .as_ref()
                    .and_then(|s| s.get("preferredAgentInterface")),
            )
        })
        .is_none_or(|view| view == "chat");
        if !chat {
            return Ok(false);
        }
        let mut params = Map::new();
        params.insert("agentIds".into(), json!([agent_id]));
        crate::agent_hooks::install_agent_hooks(&paths, &params).map(|_| true)
    })
    .await;
    match outcome {
        Ok(Ok(true)) => append_output(
            key,
            "\nInstalled Ghostex's hooks so this agent works in Chat View.\n",
        ),
        Ok(Ok(false)) => {}
        Ok(Err(error)) => append_output(
            key,
            &format!(
                "\nCould not install Ghostex's hooks for Chat View: {}\n",
                error.message
            ),
        ),
        Err(error) => append_output(
            key,
            &format!("\nCould not install Ghostex's hooks for Chat View: {error}\n"),
        ),
    }
}

fn set_job_status(key: &(PathBuf, String), status: &str) {
    if let Ok(mut jobs) = JOBS.lock() {
        if let Some(job) = jobs.get_mut(key) {
            job.progress["status"] = json!(status);
        }
    }
}

fn append_output(key: &(PathBuf, String), chunk: &str) {
    if let Ok(mut jobs) = JOBS.lock() {
        if let Some(job) = jobs.get_mut(key) {
            let mut output = job.progress["output"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            output.push_str(chunk);
            if output.len() > 64 * 1024 {
                let mut start = output.len() - 64 * 1024;
                while !output.is_char_boundary(start) {
                    start += 1;
                }
                output.drain(..start);
            }
            job.progress["output"] = json!(output);
        }
    }
}

async fn read_state(
    definition: &'static Definition,
    home: &Path,
) -> Result<Value, DomainStateError> {
    read_state_with(definition, home, VERSION_TIMEOUT).await
}

/// A version check normally answers in well under a second; the first start of a freshly installed binary on
/// Windows waits for the antivirus scan (Cursor's Node launcher took longer than 5 s in a clean VM).
const VERSION_TIMEOUT: Duration = Duration::from_secs(15);
const VERSION_TIMEOUT_AFTER_INSTALL: Duration = Duration::from_secs(60);

async fn read_state_with(
    definition: &'static Definition,
    home: &Path,
    version_timeout: Duration,
) -> Result<Value, DomainStateError> {
    let _permit = PROBES.acquire().await.map_err(error)?;
    let owned_home = home.to_path_buf();
    let (executable, methods, detected_method, path_directory, mise_tool) =
        tokio::task::spawn_blocking(move || {
            crate::agent_hooks::probing::refresh_cli_environment(&owned_home);
            let install_dirs = definition.install_dirs(&owned_home);
            let executable = process::resolve_agent(&definition.binary, &install_dirs, &owned_home);
            let mise = mise::installation(definition, executable.as_deref(), &owned_home);
            let executable = mise
                .as_ref()
                .map(|installation| installation.executable.clone())
                .or(executable);
            let mut methods = catalog::methods(
                definition,
                executable.as_deref(),
                &owned_home,
                mise.as_ref(),
            );
            let detected = if mise
                .as_ref()
                .is_some_and(|installation| installation.tool.is_some())
            {
                Some("mise".to_string())
            } else {
                executable
                    .as_deref()
                    .and_then(|path| catalog::detected_method(path, definition))
            };
            // CDXC:AgentProviders 2026-10-28 WHY:
            // DeepSeek Harness offers only mise, so a copy someone installed with a global npm install was detected as npm with no npm method to update it, and the update was refused. A detected method Ghostex does not offer updates through the CLI's own updater when it has one (`dsh-tui update`).
            let detected = match detected {
                Some(method)
                    if method != "manual"
                        && definition.self_updater().is_some()
                        && !methods.iter().any(|offered| offered.id == method) =>
                {
                    None
                }
                detected => detected,
            };
            // An installation Ghostex cannot place updates through the CLI's own updater.
            let detected = detected.or_else(|| {
                let updater = definition.self_updater().filter(|_| executable.is_some())?;
                if methods
                    .iter()
                    .any(|method| method.id == "native" && method.command == updater)
                {
                    return Some("native".to_string());
                }
                methods.push(catalog::self_method(definition)?);
                Some("self".to_string())
            });
            let mise_tool = mise
                .as_ref()
                .and_then(|installation| installation.tool.clone());
            // Only a folder the agent's own installer uses is offered for PATH, never an arbitrary one.
            let path_directory = executable
                .as_deref()
                .and_then(|path| Path::new(path).parent())
                .filter(|directory| {
                    install_dirs
                        .iter()
                        .any(|known| same_directory(known, directory))
                })
                .filter(|directory| !path_setup::on_path(directory, &owned_home))
                .map(Path::to_path_buf);
            (executable, methods, detected, path_directory, mise_tool)
        })
        .await
        .map_err(error)?;
    let mut state =
        json!({"agentId":definition.agent_id,"platform":std::env::consts::OS,"methods":methods});
    let Some(path) = executable else {
        return Ok(state);
    };
    state["executablePath"] = json!(path);
    if let Some(method) = &detected_method {
        state["detectedMethodId"] = json!(method);
    }
    if let Some(directory) = path_directory {
        state["pathDirectory"] = json!(directory.to_string_lossy());
    }
    let output = Arc::new(Mutex::new(String::new()));
    let capture = output.clone();
    let args = definition
        .version_args
        .clone()
        .unwrap_or_else(|| vec!["--version".into()]);
    let (version, latest_version) = tokio::join!(
        process::run_executable(&path, &args, home, version_timeout, move |chunk| {
            if let Ok(mut output) = capture.lock() {
                if output.len() < 4096 {
                    output.push_str(&chunk);
                }
            }
        }),
        latest::latest(definition),
    );
    match version {
        Ok(()) => {
            let text = output.lock().map_err(error)?.clone();
            if let Some(line) = text
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty() && line.chars().any(|ch| ch.is_ascii_digit()))
            {
                state["version"] = json!(line.chars().take(160).collect::<String>());
            }
        }
        Err(reason) => state["versionError"] = json!(reason),
    }
    if let Some(latest_version) = latest_version {
        let current = state["version"].as_str().and_then(latest::version_in);
        state["updateAvailable"] = json!(current
            .as_deref()
            .is_some_and(|current| latest::is_newer(&latest_version, current)));
        state["latestVersion"] = json!(latest_version);
    }
    if let Some(version) = state["version"].as_str().map(str::to_string) {
        match update_check::status(definition, &path, &version, home) {
            Some(update_check::Status::Ready(outcome)) => {
                if let Some(available) = outcome.available {
                    state["updateAvailable"] = json!(available);
                }
                if let Some(summary) = outcome.summary {
                    state["updateSummary"] = json!(summary);
                }
            }
            Some(update_check::Status::Checking) => state["updateChecking"] = json!(true),
            None => {}
        }
    }
    // mise's minimum_release_age only holds back releases with a publish date, the ones `latestVersion` reads.
    if let (true, Some("mise"), Some(tool), Some(latest)) = (
        state["updateAvailable"] == true,
        detected_method.as_deref(),
        &mise_tool,
        state["latestVersion"].as_str().map(str::to_string),
    ) {
        match mise::held_back(tool, &latest, home).await {
            Some(mise::HeldBack {
                message,
                installable: None,
            }) => state["updateHeldBack"] = json!(message),
            Some(mise::HeldBack {
                message,
                installable: Some(version),
            }) => {
                state["updateNote"] = json!(message);
                state["updateInstallable"] = json!(version);
            }
            None => {}
        }
    }
    Ok(state)
}

fn same_directory(left: &Path, right: &Path) -> bool {
    let key = |path: &Path| {
        let text = path.to_string_lossy();
        let text = text.trim_end_matches(['\\', '/']).to_string();
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    key(left) == key(right)
}

/// CDXC:AgentProviders 2026-10-16 WHY:
/// Every answer here used the 500-mapped `invalidParams` code, so a refused request (a busy job, an unavailable method, a stale page) logged as a server failure and a real one could not be told apart (GitHub #243). A request the user can correct is `badRequest` (400); only Ghostex's own failures stay 500.
fn refused(message: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "badRequest",
        message: message.to_string(),
    }
}

fn error(message: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: message.to_string(),
    }
}
