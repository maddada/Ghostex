use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use serde_json::{json, Map, Value};

use crate::{
    constants::GXSERVER_PROTOCOL_VERSION,
    domain::{DomainRepository, DomainStateError},
    session_status::compute_activity_update,
    toolchain::GxserverResolvedTool,
};

use super::*;

pub(crate) fn create_attach_session_metadata(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
    context: &ZmxServerContext,
    agent_settings: &Map<String, Value>,
) -> ZmxEndpointResult<Value> {
    create_attach_session_metadata_with_observed_state(
        repository,
        params,
        context,
        agent_settings,
        ObservedProviderState::Unobserved,
    )
}

/// `create_attach_session_metadata`, told what this same request already
/// observed about the provider. See `ObservedProviderState`.
pub(crate) fn create_attach_session_metadata_with_observed_state(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
    context: &ZmxServerContext,
    agent_settings: &Map<String, Value>,
    observed: ObservedProviderState,
) -> ZmxEndpointResult<Value> {
    let lifecycle = read_lifecycle_params(params)?;
    let project = repository
        .get_project(&lifecycle.project_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!("Project {} does not exist.", lifecycle.project_id))
        })?;
    let existing_session = require_session(repository, &lifecycle)?;
    let cwd = string_field(&existing_session, "cwd").or_else(|| string_field(&project, "path"));
    let (probe, probed_session, zmx, zmx_name) =
        probe_and_cache_session_provider_with_observed_state(repository, &lifecycle, observed)?;
    let explicit_startup_text = normalize_optional_startup_text(params.get("startupText"));
    let queued_launch_startup_text = if explicit_startup_text.is_none() {
        get_queued_agent_launch_startup_text_for_session(&probed_session)
    } else {
        None
    };
    let startup_text = explicit_startup_text
        .clone()
        .or(queued_launch_startup_text.clone())
        .or_else(|| {
            get_provider_restart_startup_text_for_session(&project, &probed_session, agent_settings)
        });
    let startup_text_disposition =
        decide_startup_text_disposition(&probe.lifecycle_state, startup_text.as_deref());
    let session_for_attach = if explicit_startup_text.is_none()
        && (queued_launch_startup_text.is_some() || probe.lifecycle_state == "exists")
    {
        consume_queued_agent_launch_startup_text(repository, &probed_session)?
    } else {
        probed_session.clone()
    };
    if probe.lifecycle_state == "missing" && !cwd.as_deref().map(cwd_exists).unwrap_or(false) {
        let mut restore_blocked = Map::new();
        if let Some(cwd) = cwd.clone() {
            restore_blocked.insert("cwd".to_string(), Value::String(cwd));
        }
        restore_blocked.insert("reason".to_string(), json!("missingCwd"));
        let mut attach = Map::new();
        attach.insert("provider".to_string(), json!("zmx"));
        attach.insert("providerState".to_string(), probe_to_value(&probe));
        attach.insert("restoreBlocked".to_string(), Value::Object(restore_blocked));
        attach.insert("session".to_string(), session_for_attach);
        maybe_insert_startup_text(
            &mut attach,
            &startup_text_disposition,
            startup_text.as_deref(),
        );
        attach.insert(
            "startupTextDisposition".to_string(),
            Value::String(startup_text_disposition),
        );
        attach.insert("zmxName".to_string(), Value::String(zmx_name));
        return Ok(Value::Object(attach));
    }

    let attach_command_input = ZmxAttachCommandInput {
        cwd: cwd.clone().unwrap_or_default(),
        global_session_ref: string_field(&probed_session, "globalRef"),
        gxserver_auth_token_file: Some(context.auth_token_file.clone()),
        gxserver_base_url: Some(context.base_url.clone()),
        gxserver_protocol_version: Some(GXSERVER_PROTOCOL_VERSION),
        prompt_editor: prompt_editor_mode_from_params(params)?,
        session_name: zmx_name.clone(),
        title: string_field(&session_for_attach, "title"),
        zmx_executable_path: zmx.executable_path,
    };
    /*
    This request just confirmed the provider is alive (real probe or a
    same-request observation), so the attach script's own `zmx list`
    existence pre-check is redundant: use the leaner started-variant, which
    is the normal script's exists-branch (title notice + require-existing
    attach) without the extra subprocess round trip inside the PTY. A probe
    that reported `missing` keeps the canonical script with its persistence
    notice and cwd fallback.
    */
    let attach_command = if probe.lifecycle_state == "exists" {
        build_started_zmx_attach_command(attach_command_input)
    } else {
        build_zmx_attach_command(attach_command_input)
    };
    let mut attach = Map::new();
    attach.insert("attachCommand".to_string(), Value::String(attach_command));
    if let Some(cwd) = cwd {
        attach.insert("cwd".to_string(), Value::String(cwd));
    }
    attach.insert(
        "persistenceSessionCreated".to_string(),
        Value::Bool(probe.lifecycle_state == "missing"),
    );
    attach.insert("provider".to_string(), json!("zmx"));
    attach.insert("providerState".to_string(), probe_to_value(&probe));
    attach.insert("session".to_string(), session_for_attach);
    maybe_insert_startup_text(
        &mut attach,
        &startup_text_disposition,
        startup_text.as_deref(),
    );
    attach.insert(
        "startupTextDisposition".to_string(),
        Value::String(startup_text_disposition),
    );
    attach.insert("zmxName".to_string(), Value::String(zmx_name));
    Ok(Value::Object(attach))
}

pub(crate) fn wake_requires_session_provider_start(attach: &Value) -> bool {
    attach.get("provider").and_then(Value::as_str) == Some("zmx")
        && attach
            .get("providerState")
            .and_then(|state| state.get("lifecycleState"))
            .and_then(Value::as_str)
            == Some("missing")
}

pub(crate) fn start_session_provider(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
    context: &ZmxServerContext,
    agent_settings: &Map<String, Value>,
) -> ZmxEndpointResult<Value> {
    start_session_provider_with_observed_state(
        repository,
        params,
        context,
        agent_settings,
        ObservedProviderState::Unobserved,
    )
    .map(|(result, _)| result)
}

/// `start_session_provider`, told what this same request already observed about
/// the provider, and reporting back what the start itself observed so the
/// caller can skip a redundant probe of its own. See `ObservedProviderState`.
pub(crate) fn start_session_provider_with_observed_state(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
    context: &ZmxServerContext,
    agent_settings: &Map<String, Value>,
    observed: ObservedProviderState,
) -> ZmxEndpointResult<(Value, ObservedProviderState)> {
    let lifecycle = read_lifecycle_params(params)?;
    let project = repository
        .get_project(&lifecycle.project_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!("Project {} does not exist.", lifecycle.project_id))
        })?;
    let (probe, probed_session, zmx, zmx_name) =
        probe_and_cache_session_provider_with_observed_state(repository, &lifecycle, observed)?;
    let explicit_startup_text = normalize_optional_startup_text(params.get("startupText"));
    let queued_launch_startup_text = if explicit_startup_text.is_none() {
        get_queued_agent_launch_startup_text_for_session(&probed_session)
    } else {
        None
    };
    let startup_text = explicit_startup_text
        .clone()
        .or(queued_launch_startup_text)
        .or_else(|| {
            get_provider_restart_startup_text_for_session(&project, &probed_session, agent_settings)
        });
    let startup_text_disposition =
        decide_startup_text_disposition(&probe.lifecycle_state, startup_text.as_deref());
    let should_start_with_startup_text =
        startup_text_disposition == "queueAfterTerminalReady" && startup_text.is_some();
    let should_start_plain_terminal = probe.lifecycle_state == "missing"
        && startup_text_disposition == "none"
        && string_field(&probed_session, "kind").as_deref() == Some("terminal");
    if !should_start_with_startup_text && !should_start_plain_terminal {
        /*
        Nothing was launched, so this call observed nothing beyond the probe it
        was handed: the caller keeps its own authoritative probe.
        */
        return Ok((
            json!({
                "provider": "zmx",
                "providerState": probe_to_value(&probe),
                "session": if explicit_startup_text.is_none() && has_queued_agent_launch_startup_text(&probed_session) {
                    consume_queued_agent_launch_startup_text(repository, &probed_session)?
                } else {
                    probed_session
                },
                "started": false,
                "startupTextDisposition": startup_text_disposition,
                "zmxName": zmx_name,
            }),
            ObservedProviderState::Unobserved,
        ));
    }
    let cwd = string_field(&probed_session, "cwd").or_else(|| string_field(&project, "path"));
    let Some(cwd) = cwd.filter(|path| cwd_exists(path)) else {
        return Err(ZmxEndpointError::DependencyUnavailable(
            "Cannot start session provider because the project directory is missing.".to_string(),
        ));
    };
    let command = if should_start_with_startup_text {
        build_zmx_run_command(ZmxRunCommandInput {
            cwd,
            global_session_ref: string_field(&probed_session, "globalRef"),
            gxserver_auth_token_file: Some(context.auth_token_file.clone()),
            gxserver_base_url: Some(context.base_url.clone()),
            gxserver_protocol_version: Some(GXSERVER_PROTOCOL_VERSION),
            prompt_editor: prompt_editor_mode_from_params(params)?,
            session_name: zmx_name.clone(),
            startup_text: startup_text.unwrap_or_default(),
            zmx_executable_path: zmx.executable_path.clone(),
        })
    } else {
        build_zmx_shell_provider_command(ZmxShellProviderCommandInput {
            cwd,
            global_session_ref: string_field(&probed_session, "globalRef"),
            gxserver_auth_token_file: Some(context.auth_token_file.clone()),
            gxserver_base_url: Some(context.base_url.clone()),
            gxserver_protocol_version: Some(GXSERVER_PROTOCOL_VERSION),
            prompt_editor: prompt_editor_mode_from_params(params)?,
            session_name: zmx_name.clone(),
            zmx_executable_path: zmx.executable_path.clone(),
        })
    };
    let start = run_zmx_start_command(&zmx_name, &zmx.executable_path, command)?;
    let provider_state = ProviderProbe {
        error: None,
        lifecycle_state: "exists".to_string(),
        probed_at: now_iso(),
        zmx_name: zmx_name.clone(),
    };
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(lifecycle.project_id));
    update.insert("sessionId".to_string(), json!(lifecycle.session_id));
    update.insert("lifecycleState".to_string(), json!("running"));
    update.insert(
        "providerState".to_string(),
        Value::Object(started_provider_state_patch(
            &probed_session,
            &provider_state,
            &zmx.executable_path,
        )?),
    );
    if explicit_startup_text.is_none() {
        if let Some(launch_settings) =
            launch_settings_with_consumed_agent_launch_startup_text(&probed_session)
        {
            update.insert("launchSettings".to_string(), Value::Object(launch_settings));
        }
    }
    let session = repository.update_session_for_lifecycle(&update)?;
    /*
    CDXC:Drafts 2026-08-28:
    An agent CLI has just been launched into this pane, so re-arm layer 1 of
    CDXC:AgentScreenDetection for a DRAFT. `/api/wakeSession` already does
    this for itself further up the lifecycle dispatch, but a cold desktop attach
    and the provider restart inside `/api/switchDraftAgent` both land HERE and
    never touch the wake path — which left the CLI's startup spinner free to
    paint a working title, stamp `lastActiveAt`, and PROMOTE a draft the user
    had not typed a single character into.

    Scoped to drafts: an ordinary session's activity is untouched, exactly as
    before.
    */
    let session = crate::agents::arm_draft_launch_activity_suppression(repository, &session)?;
    Ok((
        json!({
            "exitCode": start.result.exit_code,
            "provider": "zmx",
            "providerState": probe_to_value(&provider_state),
            "session": session,
            "started": true,
            "startupTextDisposition": startup_text_disposition,
            "zmxName": zmx_name,
        }),
        if start.observed_alive {
            ObservedProviderState::Exists
        } else {
            ObservedProviderState::Unobserved
        },
    ))
}

pub(crate) fn probe_and_cache_session_provider(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
) -> ZmxEndpointResult<(ProviderProbe, Value, GxserverResolvedTool, String)> {
    probe_and_cache_session_provider_with_observed_state(
        repository,
        lifecycle,
        ObservedProviderState::Unobserved,
    )
}

/// `probe_and_cache_session_provider`, reusing a provider state this same
/// request observed milliseconds ago instead of spawning another `zmx list`.
/// The cached row is written exactly as the equivalent real probe would have
/// written it. See `ObservedProviderState`.
pub(crate) fn probe_and_cache_session_provider_with_observed_state(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    observed: ObservedProviderState,
) -> ZmxEndpointResult<(ProviderProbe, Value, GxserverResolvedTool, String)> {
    let session = require_session(repository, lifecycle)?;
    let zmx = require_zmx()?;
    let zmx_name = provider_zmx_session_name(&session)?;
    let probe = match observed {
        ObservedProviderState::Unobserved => probe_zmx_session(&zmx_name, &zmx.executable_path),
        ObservedProviderState::Exists => observed_provider_probe(&zmx_name, "exists"),
        ObservedProviderState::Missing => observed_provider_probe(&zmx_name, "missing"),
    };
    let lifecycle_state = reconcile_domain_lifecycle_from_provider_probe(
        string_field(&session, "lifecycleState")
            .as_deref()
            .unwrap_or("unknown"),
        &probe.lifecycle_state,
    );
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(lifecycle.project_id));
    update.insert("sessionId".to_string(), json!(lifecycle.session_id));
    update.insert("lifecycleState".to_string(), json!(lifecycle_state));
    update.insert(
        "providerState".to_string(),
        Value::Object(provider_state_patch(&session, &probe)?),
    );
    let updated = repository.update_session_for_lifecycle(&update)?;
    Ok((probe, updated, zmx, zmx_name))
}

pub(crate) fn kill_and_cache_session_provider(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    lifecycle_state: &str,
) -> ZmxEndpointResult<(ProviderKill, Value)> {
    let session = require_session(repository, lifecycle)?;
    let zmx = require_zmx()?;
    let zmx_name = provider_zmx_session_name(&session)?;
    let kill = kill_zmx_session(&zmx_name, &zmx.executable_path);
    let timestamp = now_iso();
    let provider_state = if kill.killed {
        missing_provider_state_patch(&session, &timestamp)?
    } else {
        failed_kill_provider_state_patch(&session, &kill, &timestamp)?
    };
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(lifecycle.project_id));
    update.insert("sessionId".to_string(), json!(lifecycle.session_id));
    /*
    CDXC:SessionSleep 2026-09-01:
    A failed kill used to write lifecycleState "unknown", which no surface
    shows: the sidebar requires running/sleeping and the Sessions history
    requires "stopped", so one zmx hiccup made the session invisible
    everywhere. The transition did not happen, so the durable state keeps what
    it was; the failure itself stays recorded in providerState
    (killError/probeError via `failed_kill_provider_state_patch`).
    */
    let preserved_lifecycle_state =
        string_field(&session, "lifecycleState").unwrap_or_else(|| "unknown".to_string());
    update.insert(
        "lifecycleState".to_string(),
        json!(if kill.killed {
            lifecycle_state
        } else {
            preserved_lifecycle_state.as_str()
        }),
    );
    update.insert("providerState".to_string(), Value::Object(provider_state));
    let updated = repository.update_session_for_lifecycle(&update)?;
    Ok((kill, updated))
}

pub(crate) fn apply_wake_session_activity_suppression(
    repository: &DomainRepository<'_>,
    session: &Value,
) -> Result<Value, DomainStateError> {
    /*
    CDXC:Zmx 2026-06-22-07:16:
    Waking a session must clear stale working/attention state before title observation can replay an old zmx title. Keep Rust wake aligned with TypeScript by forcing the shared `wake` activity transition inside the lifecycle endpoint result rather than waiting for a later renderer or title event.
    */
    let params = Map::new();
    let update = compute_activity_update(session, &params, Some("wake"));
    if !should_persist_activity_update(session, &update.activity, update.last_active_at.as_deref())
    {
        return Ok(session.clone());
    }
    let mut runtime_settings = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_settings.insert("agentActivity".to_string(), update.activity);
    let mut session_update = Map::new();
    session_update.insert("projectId".to_string(), value_field(session, "projectId")?);
    session_update.insert("sessionId".to_string(), value_field(session, "sessionId")?);
    session_update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    if let Some(last_active_at) = update.last_active_at {
        session_update.insert("lastActiveAt".to_string(), json!(last_active_at));
    }
    repository.update_session(&session_update)
}

fn should_persist_activity_update(
    session: &Value,
    activity: &Value,
    last_active_at: Option<&str>,
) -> bool {
    string_field(session, "lastActiveAt").as_deref() != last_active_at
        || session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("agentActivity"))
            != Some(activity)
}

/// The exact `ProviderProbe` a real probe would have produced for a state this
/// request already observed: `exists` and `missing` are both clean outcomes, so
/// they carry no probe error, and `probedAt` is stamped now because that is when
/// the observation is being recorded.
fn observed_provider_probe(session_name: &str, lifecycle_state: &str) -> ProviderProbe {
    ProviderProbe {
        error: None,
        lifecycle_state: lifecycle_state.to_string(),
        probed_at: now_iso(),
        zmx_name: session_name.to_string(),
    }
}

fn probe_zmx_session(session_name: &str, zmx_executable_path: &str) -> ProviderProbe {
    let probed_at = now_iso();
    let result = run_zmx_probe_script(
        build_zmx_exists_command(session_name, zmx_executable_path),
        ZmxCommandOptions::default(),
    );
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            return ProviderProbe {
                error: Some(format!("zmx probe command failed: {error}")),
                lifecycle_state: "unknown".to_string(),
                probed_at,
                zmx_name: session_name.to_string(),
            };
        }
    };
    let lifecycle_state = if result.exit_code == 0 {
        "exists"
    } else if result.exit_code == 1 {
        "missing"
    } else {
        "unknown"
    };
    ProviderProbe {
        error: (lifecycle_state == "unknown").then(|| zmx_probe_exit_error_message(&result)),
        lifecycle_state: lifecycle_state.to_string(),
        probed_at,
        zmx_name: session_name.to_string(),
    }
}

pub(crate) fn kill_zmx_session(session_name: &str, zmx_executable_path: &str) -> ProviderKill {
    let result = run_zmx_probe_script(
        build_zmx_kill_command(session_name, zmx_executable_path),
        ZmxCommandOptions::default(),
    );
    #[cfg(target_os = "macos")]
    cleanup_macos_zmx_launchd_job(session_name);
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            let message = format!("zmx kill command failed: {error}");
            return ProviderKill {
                error: Some(message.clone()),
                exit_code: 1,
                killed: false,
                stderr: message,
                stdout: String::new(),
                zmx_name: session_name.to_string(),
            };
        }
    };
    let killed = result.exit_code == 0;
    ProviderKill {
        error: (!killed).then(|| {
            if result.stderr.is_empty() {
                format!("exit-{}", result.exit_code)
            } else {
                result.stderr.clone()
            }
        }),
        exit_code: result.exit_code,
        killed,
        stderr: result.stderr,
        stdout: result.stdout,
        zmx_name: session_name.to_string(),
    }
}

pub fn read_zmx_session_process_identities(
    session_names: &[String],
    home_dir: &Path,
) -> ZmxEndpointResult<HashMap<String, ZmxProcessIdentity>> {
    if session_names.is_empty() {
        return Ok(HashMap::new());
    }
    let zmx = require_zmx()?;
    let result = run_zmx_probe_script(
        build_zmx_process_snapshot_command(&zmx.executable_path),
        ZmxCommandOptions {
            stdout_limit_bytes: Some(GXSERVER_ZMX_PROCESS_SNAPSHOT_STDOUT_LIMIT_BYTES),
            ..ZmxCommandOptions::default()
        },
    )
    .map_err(ZmxEndpointError::DependencyUnavailable)?;
    if result.exit_code != 0 {
        return Ok(HashMap::new());
    }
    let (ps_output, zmx_list_output) = parse_zmx_process_snapshot_sections(&result.stdout);
    let mut identities =
        parse_zmx_session_process_identities(&ps_output, session_names, &zmx_list_output);
    for identity in identities.values_mut() {
        if identity.agent_id.as_deref() == Some("codex")
            && (identity.agent_session_id.is_none() || identity.agent_session_path.is_none())
        {
            /*
            CDXC:SessionTitles 2026-08-11:
            A live Codex process started as plain `codex` can resume or switch
            threads inside the TUI, so argv contains no conversation id. Codex
            keeps the exact active rollout open for append; resolve that
            process-owned file descriptor instead of guessing from cwd or
            transcript recency. This gives title reconciliation the canonical
            session_index identity after desktop restore as well as first run.
            */
            if let Some((agent_session_id, agent_session_path)) =
                read_codex_process_session_identity(identity.process_id)
            {
                identity.agent_session_id.get_or_insert(agent_session_id);
                identity
                    .agent_session_path
                    .get_or_insert(agent_session_path);
            }
        }
        if identity.agent_id.as_deref() != Some("omp")
            || (identity.agent_session_id.is_some() && identity.agent_session_path.is_some())
        {
            continue;
        }
        let Some((agent_session_id, agent_session_path)) =
            read_omp_terminal_session_identity(home_dir, identity.terminal_name.as_deref())
        else {
            continue;
        };
        identity.agent_session_id.get_or_insert(agent_session_id);
        identity
            .agent_session_path
            .get_or_insert(agent_session_path);
    }
    Ok(identities)
}

pub fn read_zmx_existing_session_names() -> Result<HashSet<String>, ZmxEndpointError> {
    let zmx = require_zmx()?;
    let result = run_zmx_probe_command(
        format!(
            "unset ZMX_SESSION ZMX_SESSION_PREFIX\nexec {} list --short",
            shell_quote(&zmx.executable_path),
        ),
        ZmxCommandOptions {
            stdout_limit_bytes: Some(GXSERVER_ZMX_PROCESS_SNAPSHOT_STDOUT_LIMIT_BYTES),
            timeout_ms: Some(ZMX_LIFECYCLE_COMMAND_TIMEOUT_MS),
            ..ZmxCommandOptions::default()
        },
    )?;
    Ok(result
        .stdout
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn parse_zmx_session_process_identities(
    ps_output: &str,
    session_names: &[String],
    zmx_list_output: &str,
) -> HashMap<String, ZmxProcessIdentity> {
    let root_pids_by_session_name = parse_zmx_root_pids(zmx_list_output, session_names);
    let processes = parse_process_rows(ps_output);
    let children_by_parent_pid = group_processes_by_parent_pid(&processes);
    let mut identities = HashMap::new();
    for session_name in session_names {
        let Some(root_pid) = root_pids_by_session_name.get(session_name) else {
            continue;
        };
        if let Some(identity) =
            resolve_process_tree_agent_identity(*root_pid, &processes, &children_by_parent_pid)
        {
            if identity.agent_id.is_some() {
                identities.insert(session_name.clone(), identity);
            }
        }
    }
    identities
}

fn build_zmx_process_snapshot_command(zmx_executable_path: &str) -> String {
    /*
    CDXC:SessionIdentity 2026-06-21-18:25:
    Rust must copy TypeScript gxserver's live zmx process identity scan so sidebar rows are repaired from actual agent executables after cutover. Capture only bounded process metadata in memory, never persistent logs, and keep parsing centralized in server instead of client fallbacks.
    */
    format!(
        r#"
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
printf '%s\n' '__GHOSTEX_ZMX_LIST__'
"$zmx_bin" list
printf '%s\n' '__GHOSTEX_PS__'
ps -axo pid=,ppid=,tty=,command=
"#,
        shell_quote(zmx_executable_path)
    )
    .trim()
    .to_string()
}

fn parse_zmx_process_snapshot_sections(stdout: &str) -> (String, String) {
    let zmx_marker = "__GHOSTEX_ZMX_LIST__";
    let ps_marker = "__GHOSTEX_PS__";
    let Some(zmx_index) = stdout.find(zmx_marker) else {
        return (String::new(), String::new());
    };
    let Some(ps_index) = stdout.find(ps_marker) else {
        return (String::new(), String::new());
    };
    if ps_index <= zmx_index {
        return (String::new(), String::new());
    }
    (
        stdout[ps_index + ps_marker.len()..].trim().to_string(),
        stdout[zmx_index + zmx_marker.len()..ps_index]
            .trim()
            .to_string(),
    )
}

fn parse_zmx_root_pids(zmx_list_output: &str, session_names: &[String]) -> HashMap<String, i64> {
    let wanted = session_names.iter().cloned().collect::<HashSet<_>>();
    let mut root_pids = HashMap::new();
    for line in zmx_list_output.lines() {
        let Some(name) = parse_zmx_list_name(line) else {
            continue;
        };
        if !wanted.contains(&name) {
            continue;
        }
        if let Some(pid) = parse_zmx_list_pid(line) {
            root_pids.insert(name, pid);
        }
    }
    root_pids
}

fn parse_zmx_list_name(line: &str) -> Option<String> {
    for part in line.split_whitespace() {
        let Some(value) = part
            .strip_prefix("name=")
            .or_else(|| part.strip_prefix("→name="))
        else {
            continue;
        };
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn parse_zmx_list_pid(line: &str) -> Option<i64> {
    for part in line.split_whitespace() {
        let Some(value) = part.strip_prefix("pid=") else {
            continue;
        };
        let pid = value.parse::<i64>().ok()?;
        if pid > 0 {
            return Some(pid);
        }
    }
    None
}
