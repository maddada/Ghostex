//! `/api/draftRunLocation`: where a DRAFT thread will run, this computer or an agentbox box, read
//! and changed from the chat composer's Run on row before the thread's first message.
//!
//! A draft moved to a box runs nothing until its first message: the local CLI it was started with
//! is stopped, `runtimeSettings.agentbox` is written with `pending: true`, and the first chat send
//! creates the box with that message as the agent's initial prompt
//! ([`launch_pending_box_draft`]). Picking a location therefore never creates (or bills) a box,
//! and switching back is only a relaunch of the local CLI.
//!
//! CDXC:AgentBox 2026-10-01 WHY:
//! The box is created by the first message rather than when the chip is picked because a box takes
//! about a minute to come up and a Claude box may first ask for a sign-in in its terminal. A box
//! started at the pick would boot behind the chat view, where its sign-in prompt cannot be seen,
//! and the first send would wait on it; trying chips would also create and destroy cloud VMs.
//! Starting it with the first message lets the chat hand the thread to its terminal at the moment
//! the box starts, prompt included, the same launch `createAgentSession` builds for a box.
//! SEE-ALSO: packages/gx-chat-core/src/menus/run_location.rs (the Run on row),
//! server/src/session_chat_queue_runtime/box_first_send.rs (the first send),
//! packages/gx-core/src/agentbox.rs (`session_chat_view_unavailable`).

use std::path::Path;

use rusqlite::Connection;
use serde_json::{Map, Value, json};

use super::*;
use crate::domain::{DomainRepository, DomainStateError};
use crate::zmx::{ZmxServerContext, dispatch_zmx_lifecycle_endpoint};

/// Attribution for the bytes a location switch types into a draft's pane.
const DRAFT_RUN_LOCATION_SEND_SOURCE: &str = "draft-run-location";

/// What the old launch owned and the rebuild resolves again (the list `switch_draft_agent`
/// clears, plus the box record itself).
const REBUILT_RUNTIME_KEYS: &[&str] = &[
    "accountId",
    "accountName",
    "accountColor",
    "accountSlot",
    "accountProvider",
    "accountBaseCommand",
    "accountCommand",
    "accountSwitch",
    "accountRecovery",
    "accountRecoverySuppressed",
    "accountPolicyDefault",
    "accountPolicyOverride",
    "agentActivity",
    "agentSessionId",
    "agentSessionPath",
    "agentCommand",
    "launchAgentId",
    "agentName",
    "agentbox",
    crate::accounts::hub::launch::RUNTIME_KEY,
];
const REBUILT_LAUNCH_KEYS: &[&str] = &[
    "acceptAllMode",
    "agentCommand",
    "agentLaunchPlan",
    "agentResumePlan",
    "icon",
    "runtimeRelevant",
];

/// The agentbox agent a session's agent is, or `None` when a box cannot run it.
fn session_box_agent(project: &Value, session: &Value) -> Option<String> {
    let agent_id = read_text_value(session, "agentId")?;
    let launch_settings = object_field(session, "launchSettings");
    let agent_config = resolve_project_agent_config(project, &agent_id, Some(&launch_settings));
    let family = resume_agent_family_id(Some(agent_id), &agent_config, &launch_settings)?;
    crate::agentbox::AGENTBOX_AGENTS
        .contains(&family.as_str())
        .then_some(family)
}

fn run_location_of(provider: Option<&str>) -> String {
    provider
        .map(|provider| format!("agentbox:{provider}"))
        .unwrap_or_else(|| "local".to_string())
}

/// The locations `/api/agentbox status` reports ready, in its order, with its labels (the labels
/// the New Thread picker shows).
fn ready_locations(status: &Value) -> Vec<Value> {
    let usable = status.get("supported").and_then(Value::as_bool) == Some(true)
        && status.get("installed").and_then(Value::as_bool) == Some(true);
    if !usable {
        return Vec::new();
    }
    status
        .get("providers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|provider| provider.get("ready").and_then(Value::as_bool) == Some(true))
        .filter_map(|provider| {
            let id = provider.get("id").and_then(Value::as_str)?;
            // `remote-docker` is the setup entry, not a place a box can start.
            (id != "remote-docker").then(|| {
                json!({
                    "runLocation": run_location_of(Some(id)),
                    "provider": id,
                    "label": provider.get("label").cloned().unwrap_or(Value::Null),
                    "description": provider.get("description").cloned().unwrap_or(Value::Null),
                    "kind": provider.get("kind").cloned().unwrap_or(Value::Null),
                })
            })
        })
        .collect()
}

/// The answer of every call: where the draft runs now and where it could run.
fn answer(project: &Value, session: &Value, home_dir: &Path) -> Value {
    let draft = session_is_draft(session);
    let pending = crate::agentbox::pending_session_agentbox(session);
    let started_box = crate::agentbox::session_agentbox(session);
    let current = pending.as_ref().or(started_box.as_ref());
    let box_agent = session_box_agent(project, session);
    // Only a draft that could move reads the status: `agentbox doctor` is cached for 30 seconds,
    // but its first run takes a moment.
    let status = (draft && started_box.is_none() && box_agent.is_some())
        .then(|| crate::agentbox::read_status(home_dir, false));
    json!({
        "draft": draft,
        "runLocation": run_location_of(current.map(|agentbox| agentbox.provider.as_str())),
        "current": current.map(|agentbox| json!({
            "runLocation": run_location_of(Some(&agentbox.provider)),
            "provider": agentbox.provider,
            "label": crate::agentbox::provider_label(&agentbox.provider),
        })),
        "boxStarted": started_box.is_some(),
        "boxAgent": box_agent,
        // This machine's own name, for the Run on row's chip of it (never "This computer").
        "computer": crate::connected_computers::this_computer_name(),
        "agentbox": status.as_ref().map(|status| json!({
            "supported": status.get("supported").cloned().unwrap_or(Value::Bool(false)),
            "installed": status.get("installed").cloned().unwrap_or(Value::Bool(false)),
            "locations": ready_locations(status),
        })),
    })
}

/// The draft rebuilt for `provider` (a box) or this computer, through the same resolution
/// `/api/createAgentSession` uses: a box gets agentbox's launch and no account wrapping, this
/// computer gets the agent's normal launch and the account the sidebar button would pick.
pub(crate) fn rebuild_draft_for_location(
    db: &Connection,
    project: &Value,
    session: &Value,
    provider: Option<&str>,
    first_message: Option<&str>,
) -> Result<Map<String, Value>, DomainStateError> {
    let agent_id = read_text_value(session, "agentId")
        .ok_or_else(|| DomainStateError::bad_request("This thread has no agent to run."))?;
    let mut runtime_settings = object_field(session, "runtimeSettings");
    for key in REBUILT_RUNTIME_KEYS {
        runtime_settings.remove(*key);
    }
    let mut launch_settings = object_field(session, "launchSettings");
    for key in REBUILT_LAUNCH_KEYS {
        launch_settings.remove(*key);
    }
    let mut params = Map::new();
    params.insert(
        "projectId".to_string(),
        json!(read_text_value(session, "projectId").unwrap_or_default()),
    );
    params.insert("agentId".to_string(), json!(agent_id));
    params.insert("requireLaunchCommand".to_string(), json!(true));
    if let Some(cwd) = read_text_value(session, "cwd") {
        params.insert("cwd".to_string(), json!(cwd));
    }
    match first_message {
        // The first message promotes the draft: a create without `draft` strips the marker.
        Some(message) => {
            runtime_settings.insert("firstUserMessage".to_string(), json!(message));
        }
        None => {
            params.insert("draft".to_string(), json!(true));
        }
    }
    params.insert("launchSettings".to_string(), Value::Object(launch_settings));
    params.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    params.insert("runLocation".to_string(), json!(run_location_of(provider)));
    let mut resolved = create_agent_session_params_for_project(db, project, &params)?;
    // The rebuild starts every row idle: a draft is relaunched with nothing to do, and a box
    // session reports working once its agent does (agentbox/activity.rs).
    let mut runtime = resolved
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime.insert(
        "agentActivity".to_string(),
        default_activity(
            Some(&read_text_value(session, "agentId").unwrap_or_default()),
            None,
        ),
    );
    resolved.insert("runtimeSettings".to_string(), Value::Object(runtime));
    Ok(resolved)
}

/// Marks a rebuilt box draft's record pending and drops the launch it queued, so nothing starts
/// the box before the first message.
pub(crate) fn hold_box_launch(resolved: &mut Map<String, Value>) {
    if let Some(record) = resolved
        .get_mut("runtimeSettings")
        .and_then(Value::as_object_mut)
        .and_then(|runtime| runtime.get_mut("agentbox"))
        .and_then(Value::as_object_mut)
    {
        record.insert(crate::agentbox::PENDING_KEY.to_string(), json!(true));
    }
    if let Some(launch_settings) =
        crate::zmx::launch_settings_with_consumed_agent_launch_startup_text(&Value::Object(
            resolved.clone(),
        ))
    {
        resolved.insert("launchSettings".to_string(), Value::Object(launch_settings));
    }
}

fn write_rebuilt_row(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    resolved: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let mut update = lifecycle_update(lifecycle);
    for key in ["launchSettings", "runtimeSettings"] {
        if let Some(value) = resolved.get(key).cloned() {
            update.insert(key.to_string(), value);
        }
    }
    repository.update_session(&update)
}

fn provider_exists(
    repository: &DomainRepository<'_>,
    db: &Connection,
    lifecycle: &LifecycleParams,
    context: &ZmxServerContext,
) -> Result<bool, AgentEndpointError> {
    let settings = read_agent_settings(db)?;
    let probe = dispatch_zmx_lifecycle_endpoint(
        repository,
        "/api/probeSessionProvider",
        &lifecycle_update(lifecycle),
        context,
        &settings,
    )?;
    Ok(probe
        .result
        .get("providerState")
        .and_then(|state| state.get("lifecycleState"))
        .and_then(Value::as_str)
        == Some("exists"))
}

fn start_provider(
    repository: &DomainRepository<'_>,
    db: &Connection,
    lifecycle: &LifecycleParams,
    context: &ZmxServerContext,
) -> Result<(), AgentEndpointError> {
    let settings = read_agent_settings(db)?;
    dispatch_zmx_lifecycle_endpoint(
        repository,
        "/api/startSessionProvider",
        &lifecycle_update(lifecycle),
        context,
        &settings,
    )?;
    Ok(())
}

/// Types `steps` into the draft's live pane as one job, releasing `guard` when it ends.
fn run_in_pane(
    session: &Value,
    lifecycle: &LifecycleParams,
    steps: Vec<crate::session_chat_send::SessionChatSendStep>,
    guard: DraftAgentSwitch,
) -> Result<(), DomainStateError> {
    crate::session_chat_send::cancel_session_chat_sends(
        &lifecycle.project_id,
        &lifecycle.session_id,
    );
    let completion = crate::session_chat_send::enqueue_session_write_sequence_with_completion(
        session,
        &lifecycle.project_id,
        &lifecycle.session_id,
        DRAFT_RUN_LOCATION_SEND_SOURCE,
        steps,
        None,
    )?;
    guard.finish_after(completion);
    Ok(())
}

/// `/api/draftRunLocation {projectId, sessionId, runLocation?}`. Without `runLocation` it only
/// answers; with one it moves the draft there first. The answer is
/// `{draft, runLocation, current, boxStarted, boxAgent, agentbox: {supported, installed, locations}}`.
pub(crate) fn draft_run_location(
    repository: &DomainRepository<'_>,
    db: &Connection,
    home_dir: &Path,
    params: &Map<String, Value>,
    context: &ZmxServerContext,
) -> Result<AgentEndpointOutput, AgentEndpointError> {
    let lifecycle = read_lifecycle(params)?;
    let project = require_project(repository, &lifecycle.project_id)?;
    let session = require_session(repository, &lifecycle)?;
    if params.get("runLocation").is_none_or(Value::is_null) {
        return Ok(AgentEndpointOutput {
            presentation_session: None,
            result: answer(&project, &session, home_dir),
        });
    }
    let target = crate::agentbox::requested_agentbox_provider(params)?;
    if !session_is_draft(&session) {
        return Err(DomainStateError {
            code: "invalidState",
            message: "This thread has already started, so where it runs can no longer be changed."
                .to_string(),
        }
        .into());
    }
    if crate::agentbox::session_agentbox(&session).is_some() {
        return Err(DomainStateError {
            code: "invalidState",
            message:
                "This thread's box has already started, so where it runs can no longer be changed."
                    .to_string(),
        }
        .into());
    }
    let current =
        crate::agentbox::pending_session_agentbox(&session).map(|agentbox| agentbox.provider);
    if current == target {
        return Ok(AgentEndpointOutput {
            presentation_session: None,
            result: answer(&project, &session, home_dir),
        });
    }
    if target.is_some() && session_box_agent(&project, &session).is_none() {
        return Err(DomainStateError::bad_request(
            "AgentBox can run Claude, Codex, OpenCode and Pi.",
        )
        .into());
    }
    let alive = provider_exists(repository, db, &lifecycle, context)?;
    let session = require_session(repository, &lifecycle)?;
    let guard = DraftAgentSwitch::begin(&lifecycle.project_id, &lifecycle.session_id)?;
    let mut resolved = rebuild_draft_for_location(db, &project, &session, target.as_deref(), None)?;
    if target.is_some() {
        hold_box_launch(&mut resolved);
        let updated = write_rebuilt_row(repository, &lifecycle, &resolved)?;
        // From this computer: stop the CLI the draft was started with, so nothing runs until the
        // first message starts the box. From another box location nothing is running.
        if alive && current.is_none() {
            run_in_pane(&updated, &lifecycle, build_draft_agent_exit_steps(), guard)?;
        }
        return Ok(AgentEndpointOutput {
            presentation_session: Some((
                lifecycle.project_id.clone(),
                lifecycle.session_id.clone(),
            )),
            result: answer(&project, &updated, home_dir),
        });
    }
    // Back to this computer: the pane sits at its login shell, so the local launch is typed there
    // (or the provider is started when there is no pane).
    let command = alive
        .then(|| draft_switch_reuse_command(&resolved))
        .transpose()?;
    if alive {
        if let Some(launch_settings) =
            crate::zmx::launch_settings_with_consumed_agent_launch_startup_text(&Value::Object(
                resolved.clone(),
            ))
        {
            resolved.insert("launchSettings".to_string(), Value::Object(launch_settings));
        }
    }
    let updated = write_rebuilt_row(repository, &lifecycle, &resolved)?;
    match command {
        Some(command) => {
            let updated = arm_draft_launch_activity_suppression(repository, &updated)?;
            run_in_pane(
                &updated,
                &lifecycle,
                build_draft_agent_switch_steps(&command),
                guard,
            )?;
        }
        None => {
            drop(guard);
            start_provider(repository, db, &lifecycle, context)?;
        }
    }
    let updated = require_session(repository, &lifecycle)?;
    Ok(AgentEndpointOutput {
        presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
        result: answer(&project, &updated, home_dir),
    })
}

/// How a pending box draft's first message was handed on.
pub(crate) struct BoxFirstSend {
    /// The message rode the box's launch command; otherwise the caller types it once the box's
    /// agent shows its input box.
    pub(crate) prompt_in_launch: bool,
}

/// The first message of a draft whose Run on row picked a box: creates the box with that message
/// as the agent's first prompt and promotes the draft, which makes it an ordinary box session
/// (its chat hands over to the terminal). `None` for every other session.
pub(crate) fn launch_pending_box_draft(
    repository: &DomainRepository<'_>,
    db: &Connection,
    project_id: &str,
    session_id: &str,
    message: &str,
    context: &ZmxServerContext,
) -> Result<Option<BoxFirstSend>, AgentEndpointError> {
    let lifecycle = LifecycleParams {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    let session = require_session(repository, &lifecycle)?;
    let Some(pending) = crate::agentbox::pending_session_agentbox(&session) else {
        return Ok(None);
    };
    let project = require_project(repository, project_id)?;
    let alive = provider_exists(repository, db, &lifecycle, context)?;
    let session = require_session(repository, &lifecycle)?;
    let guard = DraftAgentSwitch::begin(project_id, session_id)?;
    let mut resolved = rebuild_draft_for_location(
        db,
        &project,
        &session,
        Some(&pending.provider),
        Some(message),
    )?;
    // The box launch carries a prompt up to its inline limit and marks it for the startup-send
    // claim (agentbox/first_prompt.rs); nothing queues that send here, so it is delivered.
    let record = resolved
        .get_mut("runtimeSettings")
        .and_then(Value::as_object_mut)
        .and_then(|runtime| runtime.get_mut("agentbox"))
        .and_then(Value::as_object_mut);
    let prompt_in_launch = match record {
        Some(record) if record.get("launchPrompt").and_then(Value::as_str) == Some("pending") => {
            record.insert("launchPrompt".to_string(), json!("delivered"));
            true
        }
        _ => false,
    };
    let command = draft_switch_reuse_command(&resolved)?;
    if alive {
        if let Some(launch_settings) =
            crate::zmx::launch_settings_with_consumed_agent_launch_startup_text(&Value::Object(
                resolved.clone(),
            ))
        {
            resolved.insert("launchSettings".to_string(), Value::Object(launch_settings));
        }
    }
    let updated = write_rebuilt_row(repository, &lifecycle, &resolved)?;
    if alive {
        run_in_pane(
            &updated,
            &lifecycle,
            build_draft_agent_switch_steps(&command),
            guard,
        )?;
    } else {
        drop(guard);
        start_provider(repository, db, &lifecycle, context)?;
    }
    crate::agentbox::wake_agentbox_activity_poller();
    Ok(Some(BoxFirstSend { prompt_in_launch }))
}
