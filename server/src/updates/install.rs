//! Running updates: one item, a few, or Update all. Each goes through the path its own Settings
//! row uses; tools go first (an npm-installed agent may need the new Node), then the rest a few at
//! a time, and the Ghostex app last, because it restarts the app (Q1364).
use std::time::{Duration, Instant};

use futures_util::StreamExt as _;
use serde_json::{json, Map, Value};

use super::items::{self, definition, Usage};
use super::store::{self, Outcome};
use crate::server::AppState;

/// How many updates run at once after the tools.
const PARALLEL: usize = 3;
/// How long one helper or tool update may run before it counts as failed.
const JOB_WAIT: Duration = Duration::from_secs(20 * 60);
/// How long an update that stops sessions waits for them to go idle.
const IDLE_WAIT: Duration = Duration::from_secs(30 * 60);

fn failed(message: impl Into<String>) -> Outcome {
    Outcome {
        state: "failed",
        message: message.into(),
    }
}

/// Starts the updates of `ids` in the background and returns the ones it started (an id already
/// updating is left to its run).
pub(super) fn start(state: &AppState, ids: Vec<String>, source: &'static str) -> Vec<String> {
    let mut started: Vec<String> = Vec::new();
    for id in ids {
        if !started.contains(&id) && store::begin(&id, "Waiting…") {
            started.push(id);
        }
    }
    if started.is_empty() {
        return started;
    }
    let (app, rest): (Vec<String>, Vec<String>) =
        started.iter().cloned().partition(|id| id == "app");
    let (tools, others): (Vec<String>, Vec<String>) =
        rest.into_iter().partition(|id| id.starts_with("tool:"));
    let state = state.clone();
    tokio::spawn(async move {
        for id in tools {
            run(&state, &id, source).await;
        }
        futures_util::stream::iter(others)
            .for_each_concurrent(PARALLEL, |id| {
                let state = state.clone();
                async move { run(&state, &id, source).await }
            })
            .await;
        for id in app {
            run(&state, &id, source).await;
        }
    });
    started
}

async fn run(state: &AppState, id: &str, source: &'static str) {
    store::set_phase(id, "Updating…");
    let result = if let Some(agent) = id.strip_prefix("agent:") {
        update_agent(state, id, agent).await
    } else if let Some(provider) = id.strip_prefix("helper:") {
        update_helper(state, provider).await
    } else if let Some(tool) = id.strip_prefix("tool:") {
        update_tool(state, tool).await
    } else if let Some(extension) = id.strip_prefix("extension:") {
        update_extension(state, extension).await
    } else if let Some(component) = id.strip_prefix(items::COMPONENT_ITEM_PREFIX) {
        update_component(state, component).await
    } else if id == "app" {
        crate::app_update::request_app_update_install(state, source)
            .await
            .map(|_| ())
            .map_err(failed)
    } else {
        Err(failed("Ghostex doesn't know how to update this."))
    };
    store::finish(id, result.err());
    items::invalidate();
}

async fn usage(state: &AppState) -> Usage {
    let state = state.clone();
    tokio::task::spawn_blocking(move || Usage::read(&state))
        .await
        .unwrap_or_default()
}

/// The agent CLI's own update job (the Agents page's Update), then every running session of it
/// restarts into the same chat once idle (Q1363).
async fn update_agent(state: &AppState, id: &str, agent: &str) -> Result<(), Outcome> {
    let definition = definition(agent).ok_or_else(|| failed("Unknown agent CLI."))?;
    let home = state.paths.agent_config_home_dir().to_path_buf();
    let current = crate::agent_cli::endpoint::read_agent(&home, agent)
        .await
        .map_err(|error| failed(error.message))?;
    // Not installed here (Install in Updates' Not installed part): Settings > Agents' install.
    if !current["executablePath"].is_string() {
        return crate::agent_cli::endpoint::install_default_and_wait(&state.paths, agent)
            .await
            .map(|_| ())
            .map_err(failed);
    }
    let name = current["name"]
        .as_str()
        .unwrap_or(&definition.binary)
        .to_string();
    let method = current["detectedMethodId"]
        .as_str()
        .map(str::to_string)
        .or_else(|| definition.self_updater().map(|_| "self".to_string()))
        .ok_or_else(|| {
            failed(format!(
                "Ghostex can't tell how {name} was installed. Choose its install method in Settings > Agents and update it there."
            ))
        })?;
    if definition.update_stops_sessions {
        store::set_phase(id, &format!("Waiting for {name} to finish…"));
        let started = Instant::now();
        while usage(state).await.working(agent) {
            if started.elapsed() > IDLE_WAIT {
                return Err(failed(format!(
                    "{name} stayed busy for 30 minutes, so its update did not run. Try again when it's idle."
                )));
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
        store::set_phase(id, "Updating…");
    }
    crate::agent_cli::endpoint::update_and_wait(&state.paths, agent, &method)
        .await
        .map_err(failed)?;
    for (project_id, session_id) in usage(state).await.running_sessions(agent) {
        let state = state.clone();
        let name = name.clone();
        tokio::spawn(async move {
            if let Err(reason) = crate::session_chat_agent_restart::restart_when_idle(
                &state,
                &project_id,
                &session_id,
                &name,
                "updatesRestart",
            )
            .await
            {
                eprintln!(
                    "gxserver: {name} was updated but a session stays on the old version: {reason}"
                );
            }
        });
    }
    Ok(())
}

fn helper_params(operation: &str) -> Map<String, Value> {
    let mut params = Map::new();
    params.insert("operation".into(), json!(operation));
    params
}

/// Settings > Accounts' Update of Claude Swap or Codex Swap (its Install when it isn't installed
/// yet), followed until its job ends.
async fn update_helper(state: &AppState, provider: &str) -> Result<(), Outcome> {
    let status_state = state.clone();
    let status = tokio::task::spawn_blocking(move || {
        crate::accounts::endpoint::dispatch(&status_state, &helper_params("helperStatus"))
    })
    .await
    .map_err(|error| failed(error.to_string()))?
    .map_err(|error| failed(error.message))?;
    let installed = status["helperTools"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|tool| tool["provider"] == provider)
        .is_none_or(|tool| tool["installed"] == true);
    let mut params = helper_params("helperAction");
    params.insert("provider".into(), json!(provider));
    params.insert(
        "action".into(),
        json!(if installed { "update" } else { "install" }),
    );
    let run_state = state.clone();
    tokio::task::spawn_blocking(move || crate::accounts::endpoint::dispatch(&run_state, &params))
        .await
        .map_err(|error| failed(error.to_string()))?
        .map_err(|error| failed(error.message))?;
    let started = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let read_state = state.clone();
        let answer = tokio::task::spawn_blocking(move || {
            crate::accounts::endpoint::dispatch(&read_state, &helper_params("helperStatus"))
        })
        .await
        .map_err(|error| failed(error.to_string()))?
        .map_err(|error| failed(error.message))?;
        let job = answer["helperTools"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|tool| tool["provider"] == provider)
            .map(|tool| tool["job"].clone())
            .unwrap_or(Value::Null);
        match job["status"].as_str() {
            Some("complete") => return Ok(()),
            Some("failed") => {
                return Err(failed(
                    job["error"].as_str().unwrap_or("The update failed."),
                ))
            }
            _ if started.elapsed() > JOB_WAIT => {
                return Err(failed("The update did not finish within 20 minutes."))
            }
            _ => {}
        }
    }
}

/// Settings > Integrations' Update of a tool (cua-driver, SpaceO, Node, uv...), followed until its
/// job ends. SpaceO's own update restarts its daemon with the operator restart that waits for agents.
async fn update_tool(state: &AppState, tool: &str) -> Result<(), Outcome> {
    // A tool the list shows before it is installed (Hex, when Dictate uses it) is installed.
    let mut read = Map::new();
    read.insert("action".into(), json!("read"));
    read.insert("tool".into(), json!(tool));
    let current = crate::managed_tools::endpoint::dispatch(&state.paths, &read)
        .await
        .map_err(|error| failed(error.message))?;
    let offers = |operation: &str| {
        current["actions"]
            .as_array()
            .is_some_and(|actions| actions.iter().any(|action| action == operation))
    };
    let operation = if !offers("update") && offers("install") {
        "install"
    } else {
        "update"
    };
    let mut params = Map::new();
    params.insert("action".into(), json!("start"));
    params.insert("tool".into(), json!(tool));
    params.insert("operation".into(), json!(operation));
    crate::managed_tools::endpoint::dispatch(&state.paths, &params)
        .await
        .map_err(|error| failed(error.message))?;
    let started = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let Some(job) = crate::managed_tools::jobs::get(tool) else {
            return Err(failed("The update job disappeared."));
        };
        if !job.active() {
            return match job.status {
                "succeeded" => Ok(()),
                _ => Err(failed(
                    job.error
                        .unwrap_or_else(|| "The update failed.".to_string()),
                )),
            };
        }
        if started.elapsed() > JOB_WAIT {
            return Err(failed("The update did not finish within 20 minutes."));
        }
    }
}

/// The extension's catalog install, as Settings > Extensions' Update runs it without a new
/// approval: an update that asks for more (a background process, a remote website) is refused by
/// the registry and left for the user to approve there.
/// How long a component download and install may take.
const COMPONENT_WAIT: Duration = Duration::from_secs(15 * 60);

/// Installs the newer component this release sealed (Collab, GBots), the way its own Settings
/// card does, and waits for it.
async fn update_component(state: &AppState, component: &str) -> Result<(), Outcome> {
    let started = Instant::now();
    match component {
        "collab" => {
            use crate::collab_shim::install;
            let Some(app_state) = crate::collab_shim::sidecar::app_state() else {
                return Err(failed("Collab isn't ready yet. Try again in a moment."));
            };
            install::start_update(app_state).map_err(failed)?;
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let item = install::item();
                match item["phase"].as_str() {
                    Some("downloading" | "installing") => {
                        if let Some(done) = item["doneBytes"].as_f64() {
                            let total = item["totalBytes"].as_f64().unwrap_or(0.0);
                            if total > 0.0 {
                                store::set_phase(
                                    &format!("{}collab", items::COMPONENT_ITEM_PREFIX),
                                    &format!(
                                        "Downloading\u{2026} {}%",
                                        ((done / total) * 100.0).floor()
                                    ),
                                );
                            }
                        }
                    }
                    Some("failed") => {
                        return Err(failed(
                            item["error"]
                                .as_str()
                                .unwrap_or("The Collab update failed."),
                        ))
                    }
                    _ if install::update_available().is_none() => return Ok(()),
                    _ => {}
                }
                if started.elapsed() > COMPONENT_WAIT {
                    return Err(failed("The Collab update took too long."));
                }
            }
        }
        "gbots" => {
            use crate::gbots::{download, runtime};
            let _ = state;
            download::retry_now();
            if !download::ensure_downloading() {
                return Err(failed("This Ghostex has no GBots runtime to install."));
            }
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if runtime::installed_runtime().is_some() {
                    return Ok(());
                }
                let status = download::status_value();
                if status["phase"] == "failed" {
                    return Err(failed(
                        status["error"]
                            .as_str()
                            .unwrap_or("The GBots update failed."),
                    ));
                }
                if started.elapsed() > COMPONENT_WAIT {
                    return Err(failed("The GBots update took too long."));
                }
            }
        }
        _ => Err(failed("Ghostex doesn't know how to update this.")),
    }
}

async fn update_extension(state: &AppState, extension: &str) -> Result<(), Outcome> {
    let registry = state.extension_registry.clone();
    let id = extension.to_string();
    let result = tokio::task::spawn_blocking(move || registry.install_from_catalog(&id, None))
        .await
        .map_err(|error| failed(error.to_string()))?;
    match result {
        Ok(_) => Ok(()),
        Err(error) if error.message.contains("approve") => Err(Outcome {
            state: "needsYou",
            message: error.message,
        }),
        Err(error) => Err(failed(error.message)),
    }
}
