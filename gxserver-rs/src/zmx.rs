use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use crate::{
    agents::{apply_created_session_identity, get_agent_startup_text_for_session},
    constants::GXSERVER_PROTOCOL_VERSION,
    domain::{read_project_id, read_session_id, DomainRepository, DomainStateError},
    logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger, LogLevel},
    paths::get_gxserver_paths,
    platform::shell::{command_shell, user_login_shell_exec_command},
    session_status::compute_activity_update,
    toolchain::{require_bundled_zmx, GxserverResolvedTool},
};

const ZMX_LIFECYCLE_COMMAND_TIMEOUT_MS: u64 = 5_000;
pub const GXSERVER_ZMX_COMMAND_STDOUT_LIMIT_BYTES: usize = 512 * 1024;
pub const GXSERVER_ZMX_COMMAND_STDERR_LIMIT_BYTES: usize = 64 * 1024;
pub const GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES: usize = 256 * 1024;
pub const GXSERVER_ZMX_PROCESS_SNAPSHOT_STDOUT_LIMIT_BYTES: usize = 1024 * 1024;
pub const GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES: usize = 512 * 1024;

static TEMPORARY_TERMINAL_INPUT_LOGGER: OnceLock<GxserverLogger> = OnceLock::new();

/*
Temporary cross-session interruption diagnosis (2026-08-07): record only
control-byte classifications and stable session identities at the last
gxserver boundary before `zmx send`. Never persist terminal text. Title-flow
writes are included even when they contain no control bytes so a future report
can prove which exact session every command/submit step targeted.
*/
pub(crate) fn log_temporary_zmx_input_write(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    operation: &str,
    source: &str,
    payload: &str,
) {
    let bytes = payload.as_bytes();
    let mut controls = Vec::new();
    if payload.contains("\u{1b}[27u") {
        controls.push("kitty-escape");
    } else if bytes == [0x1b] {
        controls.push("raw-escape");
    }
    for (byte, label) in [
        (0x03, "ctrl-c"),
        (0x04, "ctrl-d"),
        (0x15, "ctrl-u"),
        (0x19, "ctrl-y"),
        (0x1a, "ctrl-z"),
        (0x1c, "ctrl-backslash"),
    ] {
        if bytes.contains(&byte) {
            controls.push(label);
        }
    }
    let title_flow = source.contains("title");
    if controls.is_empty() && !title_flow {
        return;
    }
    let paths = get_gxserver_paths(None);
    let logger = TEMPORARY_TERMINAL_INPUT_LOGGER.get_or_init(|| GxserverLogger::new(paths));
    let _ = logger.log_routine(
        DiagnosticLogScenario::TerminalFocus,
        GxserverLogInput {
            level: LogLevel::Debug,
            event: "temporaryTerminalInputWrite".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(json!({
                "byteLength": bytes.len(),
                "controls": controls,
                "operation": operation.trim_start_matches("/api/"),
                "projectId": project_id,
                "providerSessionId": zmx_name,
                "sessionId": session_id,
                "source": source,
            })),
        },
    );
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ZmxProcessIdentity {
    pub agent_id: Option<String>,
    pub agent_session_id: Option<String>,
    pub agent_session_path: Option<String>,
    process_id: Option<i64>,
    pub(crate) terminal_name: Option<String>,
}

#[derive(Clone)]
pub struct ZmxServerContext {
    pub auth_token_file: String,
    pub base_url: String,
}

pub struct ZmxEndpointOutput {
    pub result: Value,
    pub presentation_session: Option<(String, String)>,
    pub(crate) created_workspace_terminal: Option<CreatedWorkspaceTerminalIdentity>,
}

#[derive(Clone, Debug)]
pub(crate) struct CreatedWorkspaceTerminalIdentity {
    project_id: String,
    session_id: String,
    zmx_executable_path: String,
    zmx_name: String,
}

pub(crate) struct CreatedWorkspaceTerminalError {
    pub(crate) error: ZmxEndpointError,
    pub(crate) presentation_session: Option<(String, String)>,
}

#[derive(Debug)]
pub enum ZmxEndpointError {
    DependencyUnavailable(String),
    Domain(DomainStateError),
}

impl From<DomainStateError> for ZmxEndpointError {
    fn from(error: DomainStateError) -> Self {
        Self::Domain(error)
    }
}

impl From<ZmxEndpointError> for CreatedWorkspaceTerminalError {
    fn from(error: ZmxEndpointError) -> Self {
        Self {
            error,
            presentation_session: None,
        }
    }
}

impl From<DomainStateError> for CreatedWorkspaceTerminalError {
    fn from(error: DomainStateError) -> Self {
        ZmxEndpointError::from(error).into()
    }
}

type ZmxEndpointResult<T> = Result<T, ZmxEndpointError>;

#[derive(Clone, Debug, Default)]
struct ZmxCommandOptions {
    allow_stdout_truncation: bool,
    stderr_limit_bytes: Option<usize>,
    stdin: Option<String>,
    stdout_limit_bytes: Option<usize>,
    timeout_ms: Option<u64>,
}

#[derive(Clone, Debug)]
struct ZmxCommandResult {
    exit_code: i32,
    stderr: String,
    stdout: String,
    stdout_truncated: bool,
}

#[derive(Clone, Debug)]
struct ProviderProbe {
    error: Option<String>,
    lifecycle_state: String,
    probed_at: String,
    zmx_name: String,
}

#[derive(Clone, Debug)]
struct ProviderKill {
    error: Option<String>,
    exit_code: i32,
    killed: bool,
    stderr: String,
    stdout: String,
    zmx_name: String,
}

struct LifecycleParams {
    project_id: String,
    session_id: String,
}

/*
CDXC:GPUIWindowsTerminalStartup 2026-07-26:
Windows GPUI quick terminals use one daemon-owned create/start/attach-plan
operation. A newly allocated gxserver session id is never reused, so its zmx
name cannot already own a provider and probing it before and after startup only
adds serial process launches. Create the row, start that exact provider once,
persist the successful provider state, and return the ordinary require-existing
attach command. Other create/attach/wake paths keep their existing probe-based
race handling.
*/
pub(crate) fn create_started_workspace_terminal(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
    context: &ZmxServerContext,
) -> Result<ZmxEndpointOutput, CreatedWorkspaceTerminalError> {
    if params
        .keys()
        .any(|key| key != "projectId" && key != "promptEditor")
    {
        return Err(DomainStateError::bad_request(
            "createWorkspaceTerminal accepts only projectId and promptEditor.",
        )
        .into());
    }
    let prompt_editor = prompt_editor_mode_from_params(params)?;
    let project_id = read_project_id(params)?;
    let project = repository.get_project(&project_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("Project {project_id} does not exist."))
    })?;
    let cwd = string_field(&project, "path")
        .filter(|path| cwd_exists(path))
        .ok_or_else(|| {
            ZmxEndpointError::DependencyUnavailable(
                "Cannot start terminal because the project directory is missing.".to_string(),
            )
        })?;
    let zmx = require_zmx()?;
    let mut create_params = Map::new();
    create_params.insert("kind".to_string(), json!("terminal"));
    create_params.insert("lifecycleState".to_string(), json!("running"));
    create_params.insert("projectId".to_string(), json!(project_id));
    create_params.insert("surface".to_string(), json!("workspace"));
    create_params.insert("title".to_string(), json!("Terminal Session"));

    let created = repository.create_session_transactional(&create_params, false)?;
    let created_project_id = match string_field(&created, "projectId") {
        Some(project_id) => project_id,
        None => {
            return Err(
                DomainStateError::corrupt_state("Created terminal missing projectId.").into(),
            )
        }
    };
    let created_session_id = match string_field(&created, "sessionId") {
        Some(session_id) => session_id,
        None => {
            return Err(
                DomainStateError::corrupt_state("Created terminal missing sessionId.").into(),
            )
        }
    };
    let session = match apply_created_session_identity(repository, &created, &create_params) {
        Ok(session) => session,
        Err(error) => {
            return Err(workspace_terminal_failure_with_cleanup(
                error.into(),
                remove_created_workspace_terminal(
                    repository,
                    &created_project_id,
                    &created_session_id,
                )
                .map_err(|error| error.message),
                &created_project_id,
                &created_session_id,
            ));
        }
    };
    let project_id = created_project_id;
    let session_id = created_session_id;
    let zmx_name = match provider_zmx_session_name(&session) {
        Ok(zmx_name) => zmx_name,
        Err(error) => {
            return Err(workspace_terminal_failure_with_cleanup(
                error.into(),
                remove_created_workspace_terminal(repository, &project_id, &session_id)
                    .map_err(|error| error.message),
                &project_id,
                &session_id,
            ));
        }
    };
    let created_identity = CreatedWorkspaceTerminalIdentity {
        project_id: project_id.clone(),
        session_id: session_id.clone(),
        zmx_executable_path: zmx.executable_path.clone(),
        zmx_name: zmx_name.clone(),
    };
    let global_session_ref = string_field(&session, "globalRef");
    let start_command = build_zmx_shell_provider_command(ZmxShellProviderCommandInput {
        cwd: cwd.clone(),
        global_session_ref: global_session_ref.clone(),
        gxserver_auth_token_file: Some(context.auth_token_file.clone()),
        gxserver_base_url: Some(context.base_url.clone()),
        gxserver_protocol_version: Some(GXSERVER_PROTOCOL_VERSION),
        prompt_editor: prompt_editor.clone(),
        session_name: zmx_name.clone(),
        zmx_executable_path: zmx.executable_path.clone(),
    });
    if let Err(error) = run_zmx_start_command(&zmx_name, &zmx.executable_path, start_command) {
        return Err(workspace_terminal_failure_with_cleanup(
            error,
            compensate_created_workspace_terminal(repository, &created_identity),
            &project_id,
            &session_id,
        ));
    }

    let provider_state = ProviderProbe {
        error: None,
        lifecycle_state: "exists".to_string(),
        probed_at: now_iso(),
        zmx_name: zmx_name.clone(),
    };
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert("lifecycleState".to_string(), json!("running"));
    let provider_state_patch = match provider_state_patch(&session, &provider_state) {
        Ok(provider_state_patch) => provider_state_patch,
        Err(error) => {
            return Err(workspace_terminal_failure_with_cleanup(
                error.into(),
                compensate_created_workspace_terminal(repository, &created_identity),
                &project_id,
                &session_id,
            ));
        }
    };
    update.insert(
        "providerState".to_string(),
        Value::Object(provider_state_patch),
    );
    let session = match repository.update_session_for_lifecycle(&update) {
        Ok(session) => session,
        Err(error) => {
            return Err(workspace_terminal_failure_with_cleanup(
                error.into(),
                compensate_created_workspace_terminal(repository, &created_identity),
                &project_id,
                &session_id,
            ));
        }
    };
    let attach_command = build_started_zmx_attach_command(ZmxAttachCommandInput {
        cwd: cwd.clone(),
        global_session_ref,
        gxserver_auth_token_file: Some(context.auth_token_file.clone()),
        gxserver_base_url: Some(context.base_url.clone()),
        gxserver_protocol_version: Some(GXSERVER_PROTOCOL_VERSION),
        prompt_editor,
        session_name: zmx_name.clone(),
        title: string_field(&session, "title"),
        zmx_executable_path: zmx.executable_path,
    });
    let attach = json!({
        "attachCommand": attach_command,
        "cwd": cwd,
        "persistenceSessionCreated": false,
        "provider": "zmx",
        "providerState": probe_to_value(&provider_state),
        "session": session,
        "startupTextDisposition": "none",
        "zmxName": zmx_name,
    });
    Ok(ZmxEndpointOutput {
        created_workspace_terminal: Some(created_identity),
        presentation_session: Some((project_id, session_id)),
        result: json!({
            "attach": attach,
            "session": session,
        }),
    })
}

pub(crate) fn compensate_created_workspace_terminal(
    repository: &DomainRepository<'_>,
    identity: &CreatedWorkspaceTerminalIdentity,
) -> Result<(), String> {
    /*
    A detached `zmx run -d` may have created the provider even when the wrapper
    command times out or reports an error. The allocated session identity is
    unique, so terminate exactly that known zmx name without a list/exists
    probe, then remove exactly its durable row. Attempt both cleanup steps and
    report every cleanup failure to the caller.
    */
    let mut cleanup_errors = Vec::new();
    let kill = kill_zmx_session(&identity.zmx_name, &identity.zmx_executable_path);
    if !kill.killed {
        cleanup_errors.push(
            kill.error
                .clone()
                .or_else(|| (!kill.stderr.is_empty()).then_some(kill.stderr.clone()))
                .unwrap_or_else(|| format!("zmx kill exited {}", kill.exit_code)),
        );
    }
    if let Err(error) =
        remove_created_workspace_terminal(repository, &identity.project_id, &identity.session_id)
    {
        cleanup_errors.push(format!("durable session removal failed: {}", error.message));
    }
    if cleanup_errors.is_empty() {
        Ok(())
    } else {
        Err(cleanup_errors.join("; "))
    }
}

fn remove_created_workspace_terminal(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<(), DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    repository.remove_session(&params).map(|_| ())
}

fn workspace_terminal_failure_with_cleanup(
    mut error: ZmxEndpointError,
    cleanup: Result<(), String>,
    project_id: &str,
    session_id: &str,
) -> CreatedWorkspaceTerminalError {
    if let Err(cleanup_error) = cleanup {
        append_zmx_endpoint_error_context(
            &mut error,
            &format!(" Compensating cleanup also failed for the new terminal: {cleanup_error}"),
        );
    }
    CreatedWorkspaceTerminalError {
        error,
        presentation_session: Some((project_id.to_string(), session_id.to_string())),
    }
}

pub(crate) fn append_zmx_endpoint_error_context(error: &mut ZmxEndpointError, context: &str) {
    match error {
        ZmxEndpointError::DependencyUnavailable(message) => {
            message.push_str(context);
        }
        ZmxEndpointError::Domain(error) => {
            error.message.push_str(context);
        }
    }
}

/*
CDXC:GxserverRustPort 2026-06-15-18:06:
Phase 5 Rust must own zmx-backed lifecycle and session I/O through Ghostex-managed zmx artifacts only. Keep command builders explicit, pass user send text through stdin, cap subprocess output, and never add PATH fallback or automatic listener-port fallback.
*/
pub fn dispatch_zmx_lifecycle_endpoint(
    repository: &DomainRepository<'_>,
    endpoint_path: &str,
    params: &Map<String, Value>,
    context: &ZmxServerContext,
    agent_settings: &Map<String, Value>,
) -> ZmxEndpointResult<ZmxEndpointOutput> {
    let result = match endpoint_path {
        "/api/probeSessionProvider" => {
            let lifecycle = read_lifecycle_params(params)?;
            let (probe, session, _, _) = probe_and_cache_session_provider(repository, &lifecycle)?;
            json!({
                "provider": "zmx",
                "providerState": probe_to_value(&probe),
                "session": session,
            })
        }
        "/api/attachSessionMetadata" | "/api/wakeSession" => {
            let mut attach =
                create_attach_session_metadata(repository, params, context, agent_settings)?;
            let restore_blocked = attach.get("restoreBlocked").is_some();
            if endpoint_path == "/api/wakeSession"
                && !restore_blocked
                && wake_requires_session_provider_start(&attach)
            {
                /*
                CDXC:GxserverZmxLifecycle 2026-07-12-15:10:
                Wake must revive the provider itself, not just flip lifecycleState to
                "running". Headless wakers (ghostex CLI and React Native Android) never follow up
                with /api/startSessionProvider, so a wake that only writes the DB leaves
                a dead daemon behind a "running" row, and their zmx attach fast paths
                then auto-create a plain shell with no agent restore. Spawn synchronously
                before returning so the wake response already reflects a live provider;
                desktop clients that still call startSessionProvider afterwards probe
                "exists" and skip.
                */
                let mut provider_params = params.clone();
                if let Some(startup_text) = attach.get("startupText").and_then(Value::as_str) {
                    provider_params.insert("startupText".to_string(), json!(startup_text));
                }
                start_session_provider(repository, &provider_params, context, agent_settings)?;
                attach =
                    create_attach_session_metadata(repository, params, context, agent_settings)?;
            }
            if endpoint_path == "/api/wakeSession" && !restore_blocked {
                let attach_session = attach
                    .get("session")
                    .cloned()
                    .ok_or_else(|| DomainStateError::corrupt_state("Attach session missing."))?;
                let provider_state = attach_session
                    .get("providerState")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let mut update = Map::new();
                update.insert(
                    "projectId".to_string(),
                    value_field(&attach_session, "projectId")?,
                );
                update.insert(
                    "sessionId".to_string(),
                    value_field(&attach_session, "sessionId")?,
                );
                update.insert("lifecycleState".to_string(), json!("running"));
                update.insert("providerState".to_string(), provider_state);
                let lifecycle_session = repository.update_session_for_lifecycle(&update)?;
                let session =
                    apply_wake_session_activity_suppression(repository, &lifecycle_session)?;
                attach
                    .as_object_mut()
                    .expect("attach object")
                    .insert("session".to_string(), session.clone());
                json!({ "attach": attach, "session": session })
            } else if endpoint_path == "/api/wakeSession" {
                let session = attach
                    .get("session")
                    .cloned()
                    .ok_or_else(|| DomainStateError::corrupt_state("Attach session missing."))?;
                json!({ "attach": attach, "session": session })
            } else {
                json!({ "attach": attach })
            }
        }
        "/api/startSessionProvider" => {
            start_session_provider(repository, params, context, agent_settings)?
        }
        "/api/transitionSession" => {
            let lifecycle = read_lifecycle_params(params)?;
            let action = match params.get("action").and_then(Value::as_str) {
                Some("close") => "close",
                Some("sleep") => "sleep",
                _ => {
                    return Err(DomainStateError::bad_request(format!(
                        "Invalid session transition action: {}.",
                        js_string(params.get("action"))
                    ))
                    .into())
                }
            };
            let target_lifecycle = if action == "sleep" {
                "sleeping"
            } else {
                "stopped"
            };
            let (kill, session) =
                kill_and_cache_session_provider(repository, &lifecycle, target_lifecycle)?;
            json!({
                "action": action,
                "session": session,
                "transition": {
                    "kill": kill_to_value(&kill),
                    "session": session,
                },
            })
        }
        "/api/sleepSession" | "/api/killSession" => {
            let lifecycle = read_lifecycle_params(params)?;
            /*
            CDXC:MobileKeepAwake 2026-08-19:
            An AUTOMATIC sleep (a client's "Sleep inactive agents" sweep) loses to
            a live keep-awake lease, because the sweeping client cannot see that a
            phone is attached to this terminal. The decision belongs here rather
            than in each client's sweep: gxserver owns the lease, so every client's
            sweep gets the same answer without learning a new presentation field.

            A user-triggered Sleep is never declined — the caller asked for it.

            CDXC:SessionChatPromptQueue 2026-08-21: a session holding queued chat
            prompts declines the same way. The scheduler can only deliver into a
            RUNNING session, so letting an inactivity sweep retire one would park
            the user's queued text until they happened to wake the session again.
            The queue is emptied by delivering it, not by sleeping through it.
            Rows that already failed do not count: they are waiting on the user,
            not on the agent.
            */
            if endpoint_path == "/api/sleepSession"
                && crate::session_keep_awake::sleep_trigger_is_automatic(
                    params.get("sleepTrigger").and_then(Value::as_str),
                )
                && (crate::session_keep_awake::is_held_awake(
                    &lifecycle.project_id,
                    &lifecycle.session_id,
                ) || crate::session_chat_queue::session_has_pending_session_chat_queue(
                    repository.connection(),
                    &lifecycle.project_id,
                    &lifecycle.session_id,
                ))
            {
                let session = require_session(repository, &lifecycle)?;
                return Ok(ZmxEndpointOutput {
                    created_workspace_terminal: None,
                    result: json!({ "declined": "keptAwake", "session": session }),
                    presentation_session: None,
                });
            }
            let target_lifecycle = if endpoint_path == "/api/sleepSession" {
                "sleeping"
            } else {
                "stopped"
            };
            let (kill, session) =
                kill_and_cache_session_provider(repository, &lifecycle, target_lifecycle)?;
            json!({ "kill": kill_to_value(&kill), "session": session })
        }
        _ => {
            return Err(DomainStateError::not_found(format!(
                "{endpoint_path} is not a gxserver zmx lifecycle endpoint."
            ))
            .into())
        }
    };
    let presentation_session = session_target_from_lifecycle_result(&result);
    Ok(ZmxEndpointOutput {
        created_workspace_terminal: None,
        result,
        presentation_session,
    })
}

/// Screen-state read for one session, by repository identity.
///
/// CDXC:SessionChatTerminalNotices 2026-08-19: the truncation flag travels with
/// the text now. A capture that hit the cap lost its TAIL — the live screen —
/// so screen-state readers must not conclude anything from it.
pub(crate) fn read_zmx_session_history_capture(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> ZmxEndpointResult<ZmxHistoryCapture> {
    let lifecycle = LifecycleParams {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    let session = require_session(repository, &lifecycle)?;
    let zmx_name = provider_zmx_session_name(&session)?;
    read_zmx_session_screen_capture(&zmx_name).map_err(ZmxEndpointError::DependencyUnavailable)
}

pub fn dispatch_zmx_session_interaction_endpoint(
    repository: &DomainRepository<'_>,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> ZmxEndpointResult<Value> {
    let zmx = require_zmx()?;
    let lifecycle = read_lifecycle_params(params)?;
    let session = require_session(repository, &lifecycle)?;
    let zmx_name = provider_zmx_session_name(&session)?;
    let diagnostic_source = params
        .get("diagnosticInputSource")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("external-api");
    match endpoint_path {
        "/api/readSessionText" => {
            let result = run_zmx_interaction_command(
                build_zmx_history_command(&zmx_name, &zmx.executable_path),
                ZmxCommandOptions {
                    allow_stdout_truncation: true,
                    stdout_limit_bytes: Some(GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES),
                    ..ZmxCommandOptions::default()
                },
            )?;
            let mut output = Map::new();
            output.insert("capturedBytes".to_string(), json!(result.stdout.len()));
            output.insert(
                "limitBytes".to_string(),
                json!(GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES),
            );
            output.insert("provider".to_string(), json!("zmx"));
            output.insert("session".to_string(), session);
            output.insert("source".to_string(), json!("history"));
            output.insert("text".to_string(), Value::String(result.stdout));
            output.insert(
                "truncated".to_string(),
                Value::Bool(result.stdout_truncated),
            );
            if result.stdout_truncated {
                output.insert(
                    "truncatedReason".to_string(),
                    json!("historyOutputLimitExceeded"),
                );
            }
            output.insert("zmxName".to_string(), Value::String(zmx_name));
            Ok(Value::Object(output))
        }
        "/api/sendSessionText" => {
            let text = read_interaction_text(params.get("text"), "sendSessionText")?;
            log_temporary_zmx_input_write(
                &lifecycle.project_id,
                &lifecycle.session_id,
                &zmx_name,
                endpoint_path,
                diagnostic_source,
                &text,
            );
            let result = run_zmx_interaction_command(
                build_zmx_send_command(&zmx_name, &zmx.executable_path),
                ZmxCommandOptions {
                    stdin: Some(text.clone()),
                    ..ZmxCommandOptions::default()
                },
            )?;
            Ok(send_result(
                result.exit_code,
                session,
                &text,
                false,
                zmx_name,
            ))
        }
        "/api/sendSessionEnter" => {
            log_temporary_zmx_input_write(
                &lifecycle.project_id,
                &lifecycle.session_id,
                &zmx_name,
                endpoint_path,
                diagnostic_source,
                "\r",
            );
            let result = run_zmx_interaction_command(
                build_zmx_send_command(&zmx_name, &zmx.executable_path),
                ZmxCommandOptions {
                    stdin: Some("\r".to_string()),
                    ..ZmxCommandOptions::default()
                },
            )?;
            Ok(send_result(
                result.exit_code,
                session,
                "\r",
                false,
                zmx_name,
            ))
        }
        "/api/sendSessionMessage" => {
            let text = read_interaction_text(params.get("text"), "sendSessionMessage")?;
            let submit = params.get("submit").and_then(Value::as_bool) != Some(false);
            /*
            CDXC:GxserverSendMessageSubmit 2026-07-04-17:02:
            Submit must be a separate zmx send after a short settle delay, never a
            trailing \r inside the same stdin burst. Bracketed-paste TUIs (Claude
            Code and similar composers) treat a \r that arrives in the same paste
            burst as newline text, leaving the message staged in the composer
            instead of submitted. `sendDelayMs` lets callers widen the settle
            window; the clamp keeps a bad value from stalling the endpoint.
            */
            log_temporary_zmx_input_write(
                &lifecycle.project_id,
                &lifecycle.session_id,
                &zmx_name,
                endpoint_path,
                diagnostic_source,
                &text,
            );
            let result = run_zmx_interaction_command(
                build_zmx_send_command(&zmx_name, &zmx.executable_path),
                ZmxCommandOptions {
                    stdin: Some(text.clone()),
                    ..ZmxCommandOptions::default()
                },
            )?;
            if submit {
                let send_delay_ms = params
                    .get("sendDelayMs")
                    .and_then(Value::as_u64)
                    .unwrap_or(150)
                    .min(2000);
                if send_delay_ms > 0 {
                    thread::sleep(Duration::from_millis(send_delay_ms));
                }
                log_temporary_zmx_input_write(
                    &lifecycle.project_id,
                    &lifecycle.session_id,
                    &zmx_name,
                    "sendSessionMessageSubmit",
                    diagnostic_source,
                    "\r",
                );
                run_zmx_interaction_command(
                    build_zmx_send_command(&zmx_name, &zmx.executable_path),
                    ZmxCommandOptions {
                        stdin: Some("\r".to_string()),
                        ..ZmxCommandOptions::default()
                    },
                )?;
            }
            let mut value = send_result(result.exit_code, session, &text, false, zmx_name);
            value
                .as_object_mut()
                .expect("send result object")
                .insert("submit".to_string(), Value::Bool(submit));
            Ok(value)
        }
        _ => Err(DomainStateError::not_found(format!(
            "{endpoint_path} is not a gxserver zmx session interaction endpoint."
        ))
        .into()),
    }
}

pub fn prepare_focus_session_renderer_command(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> ZmxEndpointResult<(Value, Map<String, Value>)> {
    let lifecycle = read_lifecycle_params(params)?;
    let session = require_session(repository, &lifecycle)?;
    let mut payload = params.clone();
    payload.insert("projectId".to_string(), json!(lifecycle.project_id));
    payload.insert("sessionId".to_string(), json!(lifecycle.session_id));
    Ok((session, payload))
}

pub fn merge_session_with_renderer_result(session: Value, result: Value) -> Value {
    let mut output = result.as_object().cloned().unwrap_or_default();
    output.insert("session".to_string(), session);
    Value::Object(output)
}

fn send_result(
    exit_code: i32,
    session: Value,
    text: &str,
    _submit: bool,
    zmx_name: String,
) -> Value {
    json!({
        "exitCode": exit_code,
        "provider": "zmx",
        "session": session,
        "textBytes": text.len(),
        "textLength": js_string_length(text),
        "zmxName": zmx_name,
    })
}

/*
CDXC:GxserverSessionIO 2026-06-22-07:09:
sendSessionText, sendSessionMessage, and sendSessionEnter report `textLength` through the TypeScript API contract. Count UTF-16 code units like JavaScript `string.length` while keeping send limits and `textBytes` byte-based.
*/
fn js_string_length(text: &str) -> usize {
    text.encode_utf16().count()
}

/*
CDXC:SessionChatSend 2026-07-31:
Session Chat's server-side send queue reuses the exact zmx stdin path the
sendSession* endpoints use (`zmx send <session>` reading raw payload bytes
from stdin) instead of growing a second pty-write mechanism. One call = one
stdin burst; the queue owns all pacing (clear burst, bracketed-paste body,
separate delayed Enter) between bursts.
*/
pub(crate) fn session_chat_zmx_write(zmx_name: &str, payload: &str) -> Result<i32, String> {
    let zmx = require_bundled_zmx()?;
    let result = run_zmx_interaction_command(
        build_zmx_send_command(zmx_name, &zmx.executable_path),
        ZmxCommandOptions {
            stdin: Some(payload.to_string()),
            ..ZmxCommandOptions::default()
        },
    )
    .map_err(|error| match error {
        ZmxEndpointError::DependencyUnavailable(message) => message,
        ZmxEndpointError::Domain(error) => error.message,
    })?;
    Ok(result.exit_code)
}

/// Terminal text for one zmx session, read by name.
pub(crate) struct ZmxHistoryCapture {
    pub text: String,
    /// The capture lost its TAIL — the live screen — so screen-state readers
    /// must not draw conclusions from it. The in-process screen capture keeps
    /// the tail by construction, so only the spawned `/api/readSessionText`
    /// path can still set this.
    pub truncated: bool,
}

/*
CDXC:SessionChatScreenCapture 2026-08-22:
Screen-state readers (chat model/effort pills, terminal notices, the compaction
activity row, the send-delivery watchdog) talk the zmx IPC socket directly
instead of spawning `zsh -lc … zmx history`. Measured on this machine against
live daemons, p50 per capture:

    session scrollback   zsh -lc + zmx    direct socket
    871 B                       5.67 ms          0.10 ms
    193 KB                     13.74 ms          1.67 ms
    686 KB                     11.25 ms          6.20 ms

The spawn was the whole cost at small sizes: `command_shell()` runs `zsh -lc`,
so every capture paid for a LOGIN shell sourcing the user's profile, plus an
exec of the zmx binary, just to copy bytes off a socket the daemon was already
listening on. At large sizes the remaining time is the daemon's own
`serializeTerminal` (5.5 ms of the 6.2 ms above is time-to-first-byte), which
no client-side change can touch — cutting that needs a tail-scoped IPC tag in
zmx itself.

Two behavior notes, both improvements over the spawn:

  - `/api/readSessionText` capped stdout at 256 KiB and kept the HEAD, so any
    session with more than 256 KiB of scrollback reported `truncated` and every
    screen-state reader correctly refused to conclude anything — those sessions
    never showed model/effort pills at all. This path keeps the TAIL instead,
    which is the only part any reader looks at (the widest window is 60
    non-blank lines), so scrollback size stops deciding whether detection works.
  - the public `/api/readSessionText` endpoint, the CLI, and automations still
    go through the spawned path unchanged: they are whole-history consumers,
    not screen-state readers.

The wire format is frozen (see zmx/src/ipc.zig): 8-byte header of a packed
`struct { tag: u8, len: u32 }` — one tag byte, four little-endian length bytes,
three bytes of backing-integer padding — followed by `len` payload bytes.
*/

/// `ipc.Tag.History`.
#[cfg(unix)]
const ZMX_IPC_TAG_HISTORY: u8 = 8;
/// `@sizeOf(ipc.Header)`: a packed `struct { u8, u32 }` backs to `u40`, which
/// rounds up to 8 bytes. The top three bytes are padding on both ends.
#[cfg(unix)]
const ZMX_IPC_HEADER_BYTES: usize = 8;
/// `util.HistoryFormat.plain`.
#[cfg(unix)]
const ZMX_IPC_HISTORY_FORMAT_PLAIN: u8 = 0;
/// Matches the 5s poll `zmx history` uses before it gives up on the daemon.
#[cfg(unix)]
const ZMX_SCREEN_CAPTURE_TIMEOUT: Duration = Duration::from_millis(5_000);
/// Scrollback TAIL retained by a screen capture. The widest consumer window is
/// 60 non-blank lines, so this is orders of magnitude more than any reader
/// needs; it exists to bound memory against a 10 MB daemon scrollback.
#[cfg(unix)]
const ZMX_SCREEN_CAPTURE_TAIL_BYTES: usize = 256 * 1024;

/// Directory the zmx daemon binds its session sockets in, resolved exactly as
/// `Cfg.socketDir` does in zmx/src/main.zig. Both ends agree: gxserver exports
/// this same environment into every daemon it launches, and the macOS launchd
/// supervisor already watches the resulting path as its liveness signal.
#[cfg(unix)]
fn zmx_socket_directory() -> PathBuf {
    if let Some(zmx_dir) = std::env::var_os("ZMX_DIR") {
        return PathBuf::from(zmx_dir);
    }
    if let Some(xdg_runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(xdg_runtime_dir).join("zmx");
    }
    let temporary_directory = std::env::var("TMPDIR")
        .unwrap_or_else(|_| "/tmp".to_string())
        .trim_end_matches('/')
        .to_string();
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("{temporary_directory}/zmx-{uid}"))
}

#[cfg(unix)]
fn zmx_session_socket_path(session_name: &str) -> PathBuf {
    zmx_socket_directory().join(session_name)
}

/// The live screen plus the tail of the scrollback, read straight off the
/// daemon's IPC socket. See `CDXC:SessionChatScreenCapture`.
#[cfg(unix)]
pub(crate) fn read_zmx_session_screen_capture(
    zmx_name: &str,
) -> Result<ZmxHistoryCapture, String> {
    let socket_path = zmx_session_socket_path(zmx_name);
    let mut stream = std::os::unix::net::UnixStream::connect(&socket_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::ConnectionRefused {
            // Same hygiene as `zmx history`: a refused connect means the daemon
            // is gone and only its socket file is left behind.
            let _ = fs::remove_file(&socket_path);
        }
        format!("zmx session screen capture could not reach the session: {error}")
    })?;
    stream
        .set_read_timeout(Some(ZMX_SCREEN_CAPTURE_TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(ZMX_SCREEN_CAPTURE_TIMEOUT)))
        .map_err(|error| format!("zmx session screen capture could not arm timeouts: {error}"))?;

    let mut request = [0_u8; ZMX_IPC_HEADER_BYTES + 1];
    request[0] = ZMX_IPC_TAG_HISTORY;
    request[1..5].copy_from_slice(&1_u32.to_le_bytes());
    request[ZMX_IPC_HEADER_BYTES] = ZMX_IPC_HISTORY_FORMAT_PLAIN;
    stream
        .write_all(&request)
        .map_err(|error| format!("zmx session screen capture could not be requested: {error}"))?;

    read_zmx_screen_capture_reply(&mut stream)
}

/// Drains one `History` reply, retaining only the last
/// `ZMX_SCREEN_CAPTURE_TAIL_BYTES` of payload so a huge scrollback costs
/// bounded memory here regardless of what the daemon serialized.
#[cfg(unix)]
fn read_zmx_screen_capture_reply(
    stream: &mut std::os::unix::net::UnixStream,
) -> Result<ZmxHistoryCapture, String> {
    let mut header = [0_u8; ZMX_IPC_HEADER_BYTES];
    let mut chunk = vec![0_u8; 64 * 1024];
    loop {
        stream.read_exact(&mut header).map_err(|error| {
            format!("zmx session screen capture reply header was unreadable: {error}")
        })?;
        let tag = header[0];
        let payload_len =
            u32::from_le_bytes([header[1], header[2], header[3], header[4]]) as usize;

        let mut remaining = payload_len;
        let mut tail: Vec<u8> = Vec::new();
        let keep = tag == ZMX_IPC_TAG_HISTORY;
        while remaining > 0 {
            let want = remaining.min(chunk.len());
            let read = stream.read(&mut chunk[..want]).map_err(|error| {
                format!("zmx session screen capture reply body was unreadable: {error}")
            })?;
            if read == 0 {
                return Err(
                    "zmx session screen capture reply ended before the payload did".to_string(),
                );
            }
            remaining -= read;
            if keep {
                tail.extend_from_slice(&chunk[..read]);
                if tail.len() > ZMX_SCREEN_CAPTURE_TAIL_BYTES {
                    let drop_to = tail.len() - ZMX_SCREEN_CAPTURE_TAIL_BYTES;
                    tail.drain(..drop_to);
                }
            }
        }
        if keep {
            return Ok(ZmxHistoryCapture {
                text: zmx_screen_capture_tail_text(tail, payload_len),
                truncated: false,
            });
        }
        // Any other tag on this connection is a message we did not ask for
        // (or one a newer daemon volunteers); skip it and keep reading.
    }
}

/// Decodes a retained tail into text, starting at the first clean line
/// boundary so a reader never sees half of a dropped line.
#[cfg(unix)]
fn zmx_screen_capture_tail_text(tail: Vec<u8>, payload_len: usize) -> String {
    let clipped = tail.len() < payload_len;
    let mut text = String::from_utf8_lossy(&tail).into_owned();
    if clipped {
        if let Some(first_newline) = text.find('\n') {
            text.drain(..=first_newline);
        }
    }
    text
}

/*
CDXC:SessionChatScreenCapture 2026-08-22:
On Windows every zmx daemon lives inside WSL, so its session socket sits in the
WSL filesystem namespace and a Windows process has no AF_UNIX path that reaches
it. The direct read above is therefore Unix-only, and Windows keeps running
`zmx history` through the same WSL command wrapper every other zmx interaction
uses. That path caps stdout and keeps the HEAD, so a session with more
scrollback than the cap reports `truncated` and screen-state readers correctly
decline to conclude anything from it — the behaviour Windows already had.
*/
#[cfg(not(unix))]
pub(crate) fn read_zmx_session_screen_capture(
    zmx_name: &str,
) -> Result<ZmxHistoryCapture, String> {
    let zmx = require_bundled_zmx()?;
    let result = run_zmx_interaction_command(
        build_zmx_history_command(zmx_name, &zmx.executable_path),
        ZmxCommandOptions {
            allow_stdout_truncation: true,
            stdout_limit_bytes: Some(GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES),
            ..ZmxCommandOptions::default()
        },
    )
    .map_err(|error| match error {
        ZmxEndpointError::DependencyUnavailable(message) => message,
        ZmxEndpointError::Domain(error) => error.message,
    })?;
    Ok(ZmxHistoryCapture {
        truncated: result.stdout_truncated,
        text: result.stdout,
    })
}

fn create_attach_session_metadata(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
    context: &ZmxServerContext,
    agent_settings: &Map<String, Value>,
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
        probe_and_cache_session_provider(repository, &lifecycle)?;
    let explicit_startup_text = normalize_optional_startup_text(params.get("startupText"));
    let queued_launch_startup_text = if explicit_startup_text.is_none() {
        get_queued_agent_launch_startup_text_for_session(&probed_session)
    } else {
        None
    };
    let startup_text = explicit_startup_text
        .clone()
        .or(queued_launch_startup_text.clone())
        .or_else(|| get_agent_startup_text_for_session(&project, &probed_session, agent_settings));
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

    let attach_command = build_zmx_attach_command(ZmxAttachCommandInput {
        cwd: cwd.clone().unwrap_or_default(),
        global_session_ref: string_field(&probed_session, "globalRef"),
        gxserver_auth_token_file: Some(context.auth_token_file.clone()),
        gxserver_base_url: Some(context.base_url.clone()),
        gxserver_protocol_version: Some(GXSERVER_PROTOCOL_VERSION),
        prompt_editor: prompt_editor_mode_from_params(params)?,
        session_name: zmx_name.clone(),
        title: string_field(&session_for_attach, "title"),
        zmx_executable_path: zmx.executable_path,
    });
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

fn wake_requires_session_provider_start(attach: &Value) -> bool {
    attach.get("provider").and_then(Value::as_str) == Some("zmx")
        && attach
            .get("providerState")
            .and_then(|state| state.get("lifecycleState"))
            .and_then(Value::as_str)
            == Some("missing")
}

fn start_session_provider(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
    context: &ZmxServerContext,
    agent_settings: &Map<String, Value>,
) -> ZmxEndpointResult<Value> {
    let lifecycle = read_lifecycle_params(params)?;
    let project = repository
        .get_project(&lifecycle.project_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!("Project {} does not exist.", lifecycle.project_id))
        })?;
    let (probe, probed_session, zmx, zmx_name) =
        probe_and_cache_session_provider(repository, &lifecycle)?;
    let explicit_startup_text = normalize_optional_startup_text(params.get("startupText"));
    let queued_launch_startup_text = if explicit_startup_text.is_none() {
        get_queued_agent_launch_startup_text_for_session(&probed_session)
    } else {
        None
    };
    let startup_text = explicit_startup_text
        .clone()
        .or(queued_launch_startup_text)
        .or_else(|| get_agent_startup_text_for_session(&project, &probed_session, agent_settings));
    let startup_text_disposition =
        decide_startup_text_disposition(&probe.lifecycle_state, startup_text.as_deref());
    let should_start_with_startup_text =
        startup_text_disposition == "queueAfterTerminalReady" && startup_text.is_some();
    let should_start_plain_terminal = probe.lifecycle_state == "missing"
        && startup_text_disposition == "none"
        && string_field(&probed_session, "kind").as_deref() == Some("terminal");
    if !should_start_with_startup_text && !should_start_plain_terminal {
        return Ok(json!({
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
        }));
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
    let result = run_zmx_start_command(&zmx_name, &zmx.executable_path, command)?;
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
        Value::Object(provider_state_patch(&probed_session, &provider_state)?),
    );
    if explicit_startup_text.is_none() {
        if let Some(launch_settings) =
            launch_settings_with_consumed_agent_launch_startup_text(&probed_session)
        {
            update.insert("launchSettings".to_string(), Value::Object(launch_settings));
        }
    }
    let session = repository.update_session_for_lifecycle(&update)?;
    Ok(json!({
        "exitCode": result.exit_code,
        "provider": "zmx",
        "providerState": probe_to_value(&provider_state),
        "session": session,
        "started": true,
        "startupTextDisposition": startup_text_disposition,
        "zmxName": zmx_name,
    }))
}

fn probe_and_cache_session_provider(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
) -> ZmxEndpointResult<(ProviderProbe, Value, GxserverResolvedTool, String)> {
    let session = require_session(repository, lifecycle)?;
    let zmx = require_zmx()?;
    let zmx_name = provider_zmx_session_name(&session)?;
    let probe = probe_zmx_session(&zmx_name, &zmx.executable_path);
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

fn kill_and_cache_session_provider(
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
    update.insert(
        "lifecycleState".to_string(),
        json!(if kill.killed {
            lifecycle_state
        } else {
            "unknown"
        }),
    );
    update.insert("providerState".to_string(), Value::Object(provider_state));
    let updated = repository.update_session_for_lifecycle(&update)?;
    Ok((kill, updated))
}

fn apply_wake_session_activity_suppression(
    repository: &DomainRepository<'_>,
    session: &Value,
) -> Result<Value, DomainStateError> {
    /*
    CDXC:GxserverZmxLifecycle 2026-06-22-07:16:
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

fn probe_zmx_session(session_name: &str, zmx_executable_path: &str) -> ProviderProbe {
    let probed_at = now_iso();
    let result = run_zsh_script(
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
            }
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

fn kill_zmx_session(session_name: &str, zmx_executable_path: &str) -> ProviderKill {
    let result = run_zsh_script(
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
    let result = run_zsh_script(
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
            CDXC:GxserverAgentTitles 2026-08-11:
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
    let result = run_zmx_interaction_command(
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
    CDXC:GxserverSessionIdentity 2026-06-21-18:25:
    Rust must copy TypeScript gxserver's live zmx process identity scan so sidebar rows are repaired from actual agent executables after cutover. Capture only bounded process metadata in memory, never persistent logs, and keep parsing centralized in gxserver-rs instead of client fallbacks.
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

#[derive(Clone, Debug)]
struct ProcessRow {
    command: String,
    pid: i64,
    ppid: i64,
    terminal_name: Option<String>,
}

#[derive(Clone, Debug)]
struct ProcessIdentityCandidate {
    confidence: i64,
    depth: i64,
    identity: ZmxProcessIdentity,
}

#[derive(Clone, Debug)]
struct ProcessIdentityObservation {
    confidence: i64,
    identity: ZmxProcessIdentity,
}

const GXSERVER_PROCESS_COMMAND_PREFIX_TOKEN_LIMIT: usize = 12;
const GXSERVER_DIRECT_AGENT_PROCESS_CONFIDENCE: i64 = 300;
const GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE: i64 = 275;
const GXSERVER_AGENT_SESSION_ID_CONFIDENCE_BONUS: i64 = 25;

fn parse_process_rows(ps_output: &str) -> Vec<ProcessRow> {
    let mut rows = Vec::new();
    for line in ps_output.lines() {
        let mut parts = line.split_whitespace();
        let Some(pid) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        let Some(ppid) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        let remaining = parts.collect::<Vec<_>>();
        let has_terminal_column = remaining
            .first()
            .copied()
            .is_some_and(looks_like_process_terminal_name);
        let terminal_name = remaining
            .first()
            .copied()
            .filter(|_| has_terminal_column)
            .and_then(normalize_process_terminal_name);
        let command_start = usize::from(has_terminal_column);
        rows.push(ProcessRow {
            command: remaining[command_start..].join(" "),
            pid,
            ppid,
            terminal_name,
        });
    }
    rows
}

fn looks_like_process_terminal_name(value: &str) -> bool {
    value == "??"
        || value == "-"
        || value.starts_with("tty")
        || value.starts_with("pts/")
        || value.starts_with("/dev/")
}

fn normalize_process_terminal_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || matches!(trimmed, "??" | "-") {
        return None;
    }
    let name = Path::new(trimmed).file_name()?.to_str()?;
    if name.is_empty()
        || name.len() > 128
        || !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return None;
    }
    Some(name.to_string())
}

fn group_processes_by_parent_pid(processes: &[ProcessRow]) -> HashMap<i64, Vec<ProcessRow>> {
    let mut grouped = HashMap::<i64, Vec<ProcessRow>>::new();
    for process_row in processes {
        grouped
            .entry(process_row.ppid)
            .or_default()
            .push(process_row.clone());
    }
    grouped
}

fn resolve_process_tree_agent_identity(
    root_pid: i64,
    processes: &[ProcessRow],
    children_by_parent_pid: &HashMap<i64, Vec<ProcessRow>>,
) -> Option<ZmxProcessIdentity> {
    let rows_by_pid = processes
        .iter()
        .map(|process_row| (process_row.pid, process_row))
        .collect::<HashMap<_, _>>();
    let mut candidates = Vec::<ProcessIdentityCandidate>::new();
    let mut queue = VecDeque::from([(0_i64, root_pid)]);
    let mut seen = HashSet::<i64>::new();
    while let Some((depth, pid)) = queue.pop_front() {
        if !seen.insert(pid) {
            continue;
        }
        if let Some(row) = rows_by_pid.get(&pid) {
            if let Some(mut observation) = resolve_process_command_agent_identity(&row.command) {
                if observation.identity.agent_id.is_some() {
                    observation.identity.process_id = Some(row.pid);
                    observation.identity.terminal_name = row.terminal_name.clone();
                    candidates.push(ProcessIdentityCandidate {
                        confidence: observation.confidence,
                        depth,
                        identity: observation.identity,
                    });
                }
            }
        }
        if let Some(children) = children_by_parent_pid.get(&pid) {
            for child in children {
                queue.push_back((depth + 1, child.pid));
            }
        }
    }
    candidates.sort_by(|left, right| {
        let confidence =
            score_process_identity_candidate(right).cmp(&score_process_identity_candidate(left));
        if confidence != std::cmp::Ordering::Equal {
            return confidence;
        }
        let id_score = right
            .identity
            .agent_session_id
            .is_some()
            .cmp(&left.identity.agent_session_id.is_some());
        if id_score != std::cmp::Ordering::Equal {
            return id_score;
        }
        right.depth.cmp(&left.depth)
    });
    candidates
        .into_iter()
        .next()
        .map(|candidate| candidate.identity)
}

fn score_process_identity_candidate(candidate: &ProcessIdentityCandidate) -> i64 {
    candidate.confidence
        + if candidate.identity.agent_session_id.is_some() {
            GXSERVER_AGENT_SESSION_ID_CONFIDENCE_BONUS
        } else {
            0
        }
}

fn resolve_process_command_agent_identity(command: &str) -> Option<ProcessIdentityObservation> {
    let tokens = split_process_command_prefix(command, GXSERVER_PROCESS_COMMAND_PREFIX_TOKEN_LIMIT);
    if tokens.is_empty() || should_ignore_process_command_for_agent_identity(command, &tokens) {
        return None;
    }
    resolve_agent_process_invocation(&tokens, 0, GXSERVER_DIRECT_AGENT_PROCESS_CONFIDENCE)
        .or_else(|| resolve_wrapped_agent_process_invocation(&tokens))
}

fn should_ignore_process_command_for_agent_identity(command: &str, tokens: &[String]) -> bool {
    let executable_name = normalize_process_executable_name(tokens.first().map(String::as_str));
    if executable_name.as_deref().is_some_and(|name| {
        matches!(
            name,
            "gte" | "node_repl" | "prompt-editor" | "skycomputeruseclient"
        )
    }) {
        return true;
    }
    let lower_command = command.to_ascii_lowercase();
    ["skycomputeruseclient", "/node_repl", " node_repl"]
        .iter()
        .any(|marker| lower_command.contains(marker))
}

fn resolve_wrapped_agent_process_invocation(
    tokens: &[String],
) -> Option<ProcessIdentityObservation> {
    let executable_name = normalize_process_executable_name(tokens.first().map(String::as_str));
    if executable_name.as_deref() == Some("env") {
        if let Some(invocation_index) = find_env_wrapped_command_index(tokens) {
            return resolve_agent_process_invocation(
                tokens,
                invocation_index,
                GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE,
            );
        }
    }
    if !executable_name
        .as_deref()
        .is_some_and(|name| matches!(name, "bun" | "node"))
    {
        return None;
    }
    for index in 1..tokens.len().min(8) {
        if let Some(observation) = resolve_agent_process_invocation(
            tokens,
            index,
            GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE,
        ) {
            return Some(observation);
        }
    }
    None
}

fn resolve_agent_process_invocation(
    tokens: &[String],
    executable_index: usize,
    confidence: i64,
) -> Option<ProcessIdentityObservation> {
    let agent_id = infer_agent_id_from_process_executable(tokens, executable_index)?;
    let agent_session_id = extract_agent_process_session_id(&agent_id, tokens, executable_index);
    Some(ProcessIdentityObservation {
        confidence,
        identity: ZmxProcessIdentity {
            agent_id: Some(agent_id),
            agent_session_id,
            agent_session_path: None,
            process_id: None,
            terminal_name: None,
        },
    })
}

fn read_codex_process_session_identity(process_id: Option<i64>) -> Option<(String, String)> {
    let process_id = process_id.filter(|process_id| *process_id > 0)?;
    let fd_dir = PathBuf::from(format!("/proc/{process_id}/fd"));
    let mut identities = HashMap::<String, PathBuf>::new();
    for entry in fs::read_dir(fd_dir).ok()?.flatten() {
        let Ok(target) = fs::read_link(entry.path()) else {
            continue;
        };
        let Some(agent_session_id) = codex_session_id_from_transcript_path(&target) else {
            continue;
        };
        identities.entry(agent_session_id).or_insert(target);
    }
    if identities.len() != 1 {
        return None;
    }
    identities
        .into_iter()
        .next()
        .map(|(agent_session_id, path)| (agent_session_id, path.to_string_lossy().into_owned()))
}

fn codex_session_id_from_transcript_path(path: &Path) -> Option<String> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl")
        || !path
            .ancestors()
            .filter_map(Path::file_name)
            .filter_map(|name| name.to_str())
            .any(|name| name == "sessions")
    {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    if !stem.starts_with("rollout-") || stem.len() < 36 {
        return None;
    }
    normalize_codex_session_id(&stem[stem.len() - 36..])
}

/*
CDXC:OmpSessionChatIdentity 2026-08-06:
OMP owns an exact terminal-to-transcript record under
`agent/terminal-sessions/<tty>`. A fresh OMP process has no `--session` argv,
so its TTY record is the authoritative provider identity source for the live
zmx process scan. Read only that exact record and require its existing JSONL
target; never guess from transcript recency or another session in the cwd.
*/
fn read_omp_terminal_session_identity(
    home_dir: &Path,
    terminal_name: Option<&str>,
) -> Option<(String, String)> {
    let terminal_name = normalize_process_terminal_name(terminal_name?)?;
    let agent_dir = omp_agent_directory(home_dir);
    read_omp_terminal_session_identity_from_agent_dir(&agent_dir, &terminal_name, home_dir)
}

fn read_omp_terminal_session_identity_from_agent_dir(
    agent_dir: &Path,
    terminal_name: &str,
    home_dir: &Path,
) -> Option<(String, String)> {
    let record_path = agent_dir.join("terminal-sessions").join(terminal_name);
    let metadata = fs::metadata(&record_path).ok()?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 {
        return None;
    }
    let record = fs::read_to_string(record_path).ok()?;
    let transcript_path = record
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| expand_home_path(line, home_dir))
        .find(|path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
                && path.is_file()
        })?;
    let stem = transcript_path.file_stem()?.to_str()?.trim();
    let agent_session_id = stem.rsplit_once('_').map(|(_, id)| id).unwrap_or(stem);
    if agent_session_id.is_empty() || agent_session_id.chars().count() > 256 {
        return None;
    }
    Some((
        agent_session_id.to_string(),
        transcript_path.to_string_lossy().into_owned(),
    ))
}

fn omp_agent_directory(home_dir: &Path) -> PathBuf {
    if let Some(agent_dir) = std::env::var("PI_CODING_AGENT_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return expand_home_path(&agent_dir, home_dir);
    }
    let config_dir = std::env::var("PI_CONFIG_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| expand_home_path(&value, home_dir))
        .unwrap_or_else(|| home_dir.join(".omp"));
    config_dir.join("agent")
}

fn expand_home_path(value: &str, home_dir: &Path) -> PathBuf {
    let trimmed = value.trim();
    if trimmed == "~" {
        return home_dir.to_path_buf();
    }
    if let Some(relative) = trimmed.strip_prefix("~/") {
        return home_dir.join(relative);
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        path
    } else {
        home_dir.join(path)
    }
}

fn infer_agent_id_from_process_executable(
    tokens: &[String],
    executable_index: usize,
) -> Option<String> {
    let executable_name =
        normalize_process_executable_name(tokens.get(executable_index).map(String::as_str))?;
    if executable_name == "acli"
        && normalize_process_token(tokens.get(executable_index + 1).map(String::as_str)).as_deref()
            == Some("rovodev")
        && normalize_process_token(tokens.get(executable_index + 2).map(String::as_str)).as_deref()
            == Some("run")
    {
        return Some("rovodev".to_string());
    }
    Some(
        match executable_name.as_str() {
            "agy" => "antigravity",
            "amp" => "amp",
            "claude" => "claude",
            "codebuddy" => "codebuddy",
            "codex" => "codex",
            "copilot" => "copilot",
            "cursor-agent" => "cursor",
            "droid" => "droid",
            "gemini" => "gemini",
            "grok" => "grok",
            "hermes" => "hermes-agent",
            "kiro-cli" => "kiro",
            "omp" => "omp",
            "opencode" => "opencode",
            "pi" => "pi",
            "qodercli" => "qoder",
            "rovodev" => "rovodev",
            _ => return None,
        }
        .to_string(),
    )
}

fn extract_agent_process_session_id(
    agent_id: &str,
    tokens: &[String],
    executable_index: usize,
) -> Option<String> {
    let args = tokens.get(executable_index + 1..).unwrap_or(&[]);
    if agent_id == "codex" {
        for index in 0..args.len().saturating_sub(1) {
            let token = normalize_process_token(args.get(index).map(String::as_str));
            if matches!(token.as_deref(), Some("resume" | "fork")) {
                return normalize_agent_process_session_id(
                    agent_id,
                    args.get(index + 1).map(String::as_str),
                );
            }
        }
        return None;
    }
    if matches!(agent_id, "claude" | "cursor") {
        return read_agent_process_flag_value(agent_id, args, "--resume");
    }
    if agent_id == "opencode" {
        return read_agent_process_flag_value(agent_id, args, "--session")
            .or_else(|| read_agent_process_flag_value(agent_id, args, "-s"));
    }
    if matches!(agent_id, "pi" | "omp") {
        return read_agent_process_flag_value(agent_id, args, "--session");
    }
    if agent_id == "kiro" {
        return read_agent_process_flag_value(agent_id, args, "--resume-id");
    }
    None
}

fn read_agent_process_flag_value(agent_id: &str, args: &[String], flag: &str) -> Option<String> {
    for index in 0..args.len() {
        let arg = args.get(index)?;
        if arg == flag {
            return normalize_agent_process_session_id(
                agent_id,
                args.get(index + 1).map(String::as_str),
            );
        }
        if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
            return normalize_agent_process_session_id(agent_id, Some(value));
        }
    }
    None
}

fn normalize_agent_process_session_id(agent_id: &str, value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().trim_end_matches([';', '&', '|']).to_string();
    if normalized.is_empty() {
        return None;
    }
    if agent_id == "codex" {
        normalize_codex_session_id(&normalized).or(Some(normalized))
    } else {
        Some(normalized)
    }
}

fn find_env_wrapped_command_index(tokens: &[String]) -> Option<usize> {
    for index in 1..tokens.len() {
        let token = tokens.get(index)?;
        if token.starts_with('-') || is_environment_assignment(token) {
            continue;
        }
        return Some(index);
    }
    None
}

fn split_process_command_prefix(command: &str, max_tokens: usize) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for char in command.chars() {
        if escaped {
            current.push(char);
            escaped = false;
            continue;
        }
        if char == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if char == active_quote {
                quote = None;
            } else {
                current.push(char);
            }
            continue;
        }
        if matches!(char, '"' | '\'') {
            quote = Some(char);
            continue;
        }
        if char.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
                if tokens.len() >= max_tokens {
                    return tokens;
                }
            }
            continue;
        }
        current.push(char);
    }
    if !current.is_empty() && tokens.len() < max_tokens {
        tokens.push(current);
    }
    tokens
}

fn normalize_process_executable_name(token: Option<&str>) -> Option<String> {
    let normalized = normalize_process_token(token)?;
    let basename = normalized
        .rsplit('/')
        .next()
        .unwrap_or(normalized.as_str())
        .to_string();
    for extension in [".cjs", ".cmd", ".exe", ".js", ".mjs", ".ts"] {
        if let Some(stripped) = basename.strip_suffix(extension) {
            return (!stripped.is_empty()).then(|| stripped.to_string());
        }
    }
    Some(basename)
}

fn normalize_process_token(token: Option<&str>) -> Option<String> {
    let text = token?.trim().to_ascii_lowercase();
    let text = text.trim_matches(['"', '\'', '`']).to_string();
    (!text.is_empty()).then_some(text)
}

fn is_environment_assignment(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    let mut seen_equals = false;
    for char in chars {
        if char == '=' {
            seen_equals = true;
            break;
        }
        if !(char.is_ascii_alphanumeric() || char == '_') {
            return false;
        }
    }
    seen_equals
}

fn normalize_codex_session_id(value: &str) -> Option<String> {
    is_uuid(value).then(|| value.to_ascii_lowercase())
}

fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn run_zmx_interaction_command(
    script: String,
    options: ZmxCommandOptions,
) -> ZmxEndpointResult<ZmxCommandResult> {
    let allow_stdout_truncation = options.allow_stdout_truncation;
    let result =
        run_zsh_script(script, options).map_err(ZmxEndpointError::DependencyUnavailable)?;
    if result.exit_code != 0 && !(allow_stdout_truncation && result.stdout_truncated) {
        let message = if !result.stderr.is_empty() {
            result.stderr.clone()
        } else if !result.stdout.is_empty() {
            result.stdout.clone()
        } else {
            format!(
                "zmx session interaction command exited {}",
                result.exit_code
            )
        };
        return Err(ZmxEndpointError::DependencyUnavailable(message));
    }
    Ok(result)
}

#[cfg(not(target_os = "macos"))]
fn run_zmx_start_command(
    _session_name: &str,
    _zmx_executable_path: &str,
    script: String,
) -> ZmxEndpointResult<ZmxCommandResult> {
    run_zmx_interaction_command(script, ZmxCommandOptions::default())
}

/*
CDXC:ZmxLaunchdPersistence 2026-08-07:
The macOS app and gxserver each have launchd/Background Task Management
lifecycles that may be replaced during a development rebuild. A detached zmx
daemon otherwise inherits gxserver's resource coalition, so macOS later kills
the terminal session while cleaning up the old gxserver job. Give every zmx
session its own launchd job whose long-lived shell supervisor remains active
until the exact zmx session disappears. The job runs /bin/zsh rather than an
executable inside Ghostex.app so an app-bundle replacement cannot re-associate
the session with the retiring app coalition.

The generated launch script is private and deletes itself as soon as launchd
starts it because startup text can contain user-authored terminal input and
the inherited terminal environment can contain secrets. The plist contains
only hashed identity and runtime paths.
*/
#[cfg(target_os = "macos")]
fn run_zmx_start_command(
    session_name: &str,
    zmx_executable_path: &str,
    script: String,
) -> ZmxEndpointResult<ZmxCommandResult> {
    let job = MacosZmxLaunchdJob::new(session_name)?;
    job.prepare(&script)?;

    let bootstrap = job.launchctl(&["bootstrap", &job.domain_target, &job.plist_path_string]);
    if !bootstrap.success {
        if macos_zmx_session_exists(session_name, zmx_executable_path) {
            job.remove_private_launch_script();
            return Ok(zmx_start_success_result());
        }
        let _ = job.launchctl(&["bootout", &job.job_target]);
        let retry = job.launchctl(&["bootstrap", &job.domain_target, &job.plist_path_string]);
        if !retry.success {
            job.remove_private_launch_script();
            return Err(ZmxEndpointError::DependencyUnavailable(format!(
                "zmx launchd bootstrap failed: {}",
                retry.message()
            )));
        }
    }

    let kickstart = job.launchctl(&["kickstart", &job.job_target]);
    if !kickstart.success && !macos_zmx_session_exists(session_name, zmx_executable_path) {
        job.cleanup();
        return Err(ZmxEndpointError::DependencyUnavailable(format!(
            "zmx launchd kickstart failed: {}",
            kickstart.message()
        )));
    }

    let deadline = Instant::now() + Duration::from_millis(ZMX_LIFECYCLE_COMMAND_TIMEOUT_MS);
    while Instant::now() < deadline {
        if macos_zmx_session_exists(session_name, zmx_executable_path) {
            return Ok(zmx_start_success_result());
        }
        thread::sleep(Duration::from_millis(100));
    }

    let launch_log = fs::read_to_string(&job.log_path)
        .ok()
        .map(|text| text.trim().chars().rev().take(4_096).collect::<String>())
        .map(|text| text.chars().rev().collect::<String>())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "the per-session launch job produced no error output".to_string());
    job.cleanup();
    Err(ZmxEndpointError::DependencyUnavailable(format!(
        "zmx session did not become ready: {launch_log}"
    )))
}

#[cfg(target_os = "macos")]
fn macos_zmx_session_exists(session_name: &str, zmx_executable_path: &str) -> bool {
    run_zsh_script(
        build_zmx_exists_command(session_name, zmx_executable_path),
        ZmxCommandOptions {
            timeout_ms: Some(1_000),
            ..ZmxCommandOptions::default()
        },
    )
    .map(|result| result.exit_code == 0)
    .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn zmx_start_success_result() -> ZmxCommandResult {
    ZmxCommandResult {
        exit_code: 0,
        stderr: String::new(),
        stdout: String::new(),
        stdout_truncated: false,
    }
}

#[cfg(target_os = "macos")]
struct MacosLaunchctlResult {
    success: bool,
    stderr: String,
    stdout: String,
}

#[cfg(target_os = "macos")]
impl MacosLaunchctlResult {
    fn message(&self) -> String {
        if !self.stderr.trim().is_empty() {
            self.stderr.trim().to_string()
        } else if !self.stdout.trim().is_empty() {
            self.stdout.trim().to_string()
        } else {
            "launchctl exited without an error message".to_string()
        }
    }
}

#[cfg(target_os = "macos")]
struct MacosZmxLaunchdJob {
    domain_target: String,
    job_target: String,
    label: String,
    launch_script_path: PathBuf,
    log_path: PathBuf,
    plist_path: PathBuf,
    plist_path_string: String,
    socket_path: PathBuf,
}

#[cfg(target_os = "macos")]
impl MacosZmxLaunchdJob {
    fn new(session_name: &str) -> ZmxEndpointResult<Self> {
        let uid = unsafe { libc::getuid() };
        let key = macos_zmx_launchd_session_key(session_name);
        let label = format!("com.madda.ghostex.zmx.{key}");
        let runtime_dir = get_gxserver_paths(None).runtime_dir.join("zmx-launchd");
        fs::create_dir_all(&runtime_dir).map_err(|error| {
            ZmxEndpointError::DependencyUnavailable(format!(
                "create zmx launchd runtime directory failed: {error}"
            ))
        })?;
        let plist_path = runtime_dir.join(format!("{key}.plist"));
        let plist_path_string = plist_path.to_string_lossy().to_string();
        Ok(Self {
            domain_target: format!("gui/{uid}"),
            job_target: format!("gui/{uid}/{label}"),
            label,
            launch_script_path: runtime_dir.join(format!("{key}.sh")),
            log_path: runtime_dir.join(format!("{key}.log")),
            plist_path,
            plist_path_string,
            socket_path: macos_zmx_socket_path(session_name),
        })
    }

    fn prepare(&self, start_script: &str) -> ZmxEndpointResult<()> {
        let _ = fs::remove_file(&self.log_path);
        let environment_exports = macos_zmx_launchd_environment_exports();
        let supervisor_script = format!(
            r#"#!/bin/zsh
/bin/rm -f {}
{}
(
{}
)
zmx_start_status=$?
if [ "$zmx_start_status" -ne 0 ]; then
  exit "$zmx_start_status"
fi
zmx_socket={}
zmx_absent_checks=0
zmodload zsh/zselect
while [ "$zmx_absent_checks" -lt 3 ]; do
  if [ ! -S "$zmx_socket" ]; then
    zmx_absent_checks=$((zmx_absent_checks + 1))
  else
    zmx_absent_checks=0
  fi
  zselect -t 200
done
"#,
            shell_quote(&self.launch_script_path.to_string_lossy()),
            environment_exports,
            start_script,
            shell_quote(&self.socket_path.to_string_lossy()),
        );
        write_private_macos_launchd_file(&self.launch_script_path, &supervisor_script)?;

        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/zsh</string>
    <string>{launch_script}</string>
  </array>
  <key>RunAtLoad</key>
  <false/>
  <key>KeepAlive</key>
  <false/>
  <key>ProcessType</key>
  <string>Interactive</string>
  <key>StandardOutPath</key>
  <string>{launch_log}</string>
  <key>StandardErrorPath</key>
  <string>{launch_log}</string>
</dict>
</plist>
"#,
            label = macos_launchd_xml_escape(&self.label),
            launch_script = macos_launchd_xml_escape(&self.launch_script_path.to_string_lossy()),
            launch_log = macos_launchd_xml_escape(&self.log_path.to_string_lossy()),
        );
        write_private_macos_launchd_file(&self.plist_path, &plist)
    }

    fn launchctl(&self, arguments: &[&str]) -> MacosLaunchctlResult {
        match Command::new("/bin/launchctl")
            .args(arguments)
            .stdin(Stdio::null())
            .output()
        {
            Ok(output) => MacosLaunchctlResult {
                success: output.status.success(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            },
            Err(error) => MacosLaunchctlResult {
                success: false,
                stderr: error.to_string(),
                stdout: String::new(),
            },
        }
    }

    fn remove_private_launch_script(&self) {
        let _ = fs::remove_file(&self.launch_script_path);
    }

    fn cleanup(&self) {
        let _ = self.launchctl(&["bootout", &self.job_target]);
        self.remove_private_launch_script();
        let _ = fs::remove_file(&self.plist_path);
        let _ = fs::remove_file(&self.log_path);
    }
}

#[cfg(target_os = "macos")]
fn macos_zmx_launchd_environment_exports() -> String {
    let mut environment = build_gxserver_zmx_child_environment()
        .into_iter()
        .filter(|(key, _)| is_shell_environment_name(key))
        .collect::<Vec<_>>();
    environment.sort_by(|left, right| left.0.cmp(&right.0));
    environment
        .into_iter()
        .map(|(key, value)| format!("export {key}={}\n", shell_quote(&value)))
        .collect()
}

#[cfg(target_os = "macos")]
fn macos_zmx_socket_path(session_name: &str) -> PathBuf {
    zmx_session_socket_path(session_name)
}

#[cfg(target_os = "macos")]
fn is_shell_environment_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|char| char.is_ascii_alphanumeric() || char == '_')
}

#[cfg(target_os = "macos")]
fn macos_zmx_launchd_session_key(session_name: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in session_name.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(target_os = "macos")]
fn macos_launchd_xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(target_os = "macos")]
fn write_private_macos_launchd_file(path: &Path, contents: &str) -> ZmxEndpointResult<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| {
            ZmxEndpointError::DependencyUnavailable(format!(
                "write {} failed: {error}",
                path.display()
            ))
        })?;
    file.write_all(contents.as_bytes()).map_err(|error| {
        ZmxEndpointError::DependencyUnavailable(format!("write {} failed: {error}", path.display()))
    })
}

#[cfg(target_os = "macos")]
fn cleanup_macos_zmx_launchd_job(session_name: &str) {
    if let Ok(job) = MacosZmxLaunchdJob::new(session_name) {
        job.cleanup();
    }
}

fn run_zsh_script(script: String, options: ZmxCommandOptions) -> Result<ZmxCommandResult, String> {
    run_zsh_script_blocking(&script, options)
}

fn run_zsh_script_blocking(
    script: &str,
    options: ZmxCommandOptions,
) -> Result<ZmxCommandResult, String> {
    let shell = command_shell();
    let mut child = Command::new(&shell.executable)
        .args(shell.script_args(script))
        /*
        CDXC:GxserverTerminalColorEnvironment 2026-07-18:
        Command::envs does not remove inherited variables that are absent from
        the supplied map. Clear first so environment_keys_to_strip actually
        removes NO_COLOR and other host-process suppression/session values from
        interactive zmx terminals, then install the complete sanitized copy.
        */
        .env_clear()
        .envs(build_gxserver_zmx_child_environment())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let input = options.stdin.clone().unwrap_or_default();
        let _ = stdin.write_all(input.as_bytes());
    }
    let terminate = Arc::new(AtomicBool::new(false));
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "missing zmx stdout pipe".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "missing zmx stderr pipe".to_string())?;
    let stdout_limit = options
        .stdout_limit_bytes
        .unwrap_or(GXSERVER_ZMX_COMMAND_STDOUT_LIMIT_BYTES);
    let stderr_limit = options
        .stderr_limit_bytes
        .unwrap_or(GXSERVER_ZMX_COMMAND_STDERR_LIMIT_BYTES);
    let stdout_terminate = terminate.clone();
    let stderr_terminate = terminate.clone();
    let stdout_thread = thread::spawn(move || read_capped(stdout, stdout_limit, stdout_terminate));
    let stderr_thread = thread::spawn(move || read_capped(stderr, stderr_limit, stderr_terminate));

    let timeout = Duration::from_millis(
        options
            .timeout_ms
            .unwrap_or(ZMX_LIFECYCLE_COMMAND_TIMEOUT_MS),
    );
    let started = Instant::now();
    let mut timed_out = false;
    let mut terminate_started: Option<Instant> = None;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            let (stdout, stdout_truncated) = stdout_thread
                .join()
                .map_err(|_| "zmx stdout reader panicked".to_string())?;
            let (stderr, stderr_truncated) = stderr_thread
                .join()
                .map_err(|_| "zmx stderr reader panicked".to_string())?;
            let mut exit_code = status.code().unwrap_or(1);
            if timed_out {
                exit_code = 124;
            } else if stdout_truncated || stderr_truncated {
                exit_code = 125;
            }
            let stderr_text = String::from_utf8_lossy(&stderr).trim().to_string();
            let stdout_text = String::from_utf8_lossy(&stdout).trim().to_string();
            let mut stderr_lines = Vec::new();
            if !stderr_text.is_empty() {
                stderr_lines.push(stderr_text);
            }
            if timed_out {
                stderr_lines.push(format!(
                    "zmx lifecycle command timed out after {}ms",
                    timeout.as_millis()
                ));
            }
            if stdout_truncated {
                stderr_lines.push(format!("zmx command stdout exceeded {stdout_limit} bytes"));
            }
            if stderr_truncated {
                stderr_lines.push(format!("zmx command stderr exceeded {stderr_limit} bytes"));
            }
            return Ok(ZmxCommandResult {
                exit_code,
                stderr: stderr_lines.join("\n"),
                stdout: stdout_text,
                stdout_truncated,
            });
        }
        let should_terminate = terminate.load(Ordering::SeqCst) || started.elapsed() >= timeout;
        if should_terminate {
            timed_out = timed_out || started.elapsed() >= timeout;
            if terminate_started.is_none() {
                terminate_started = Some(Instant::now());
                send_sigterm(&mut child);
            } else if terminate_started
                .map(|instant| instant.elapsed() >= Duration::from_millis(1_000))
                .unwrap_or(false)
            {
                let _ = child.kill();
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn read_capped<R: Read>(
    mut reader: R,
    limit: usize,
    terminate: Arc<AtomicBool>,
) -> (Vec<u8>, bool) {
    let mut output = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let Ok(read) = reader.read(&mut buffer) else {
            break;
        };
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        if read > remaining {
            if remaining > 0 {
                output.extend_from_slice(&buffer[..remaining]);
            }
            truncated = true;
            terminate.store(true, Ordering::SeqCst);
            break;
        }
        output.extend_from_slice(&buffer[..read]);
    }
    (output, truncated)
}

fn send_sigterm(child: &mut std::process::Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

struct ZmxAttachCommandInput {
    cwd: String,
    global_session_ref: Option<String>,
    gxserver_auth_token_file: Option<String>,
    gxserver_base_url: Option<String>,
    gxserver_protocol_version: Option<u64>,
    prompt_editor: Option<String>,
    session_name: String,
    title: Option<String>,
    zmx_executable_path: String,
}

struct ZmxRunCommandInput {
    cwd: String,
    global_session_ref: Option<String>,
    gxserver_auth_token_file: Option<String>,
    gxserver_base_url: Option<String>,
    gxserver_protocol_version: Option<u64>,
    prompt_editor: Option<String>,
    session_name: String,
    startup_text: String,
    zmx_executable_path: String,
}

struct ZmxShellProviderCommandInput {
    cwd: String,
    global_session_ref: Option<String>,
    gxserver_auth_token_file: Option<String>,
    gxserver_base_url: Option<String>,
    gxserver_protocol_version: Option<u64>,
    prompt_editor: Option<String>,
    session_name: String,
    zmx_executable_path: String,
}

fn build_zmx_attach_command(input: ZmxAttachCommandInput) -> String {
    /*
    CDXC:GhostexZmxProviderOwnership 2026-07-15:
    gxserver-generated attach commands may only connect to providers already
    initialized through startSessionProvider. --require-existing closes the
    probe/attach race without changing direct zmx CLI create-if-missing
    behavior or the per-client prompt-editor capability decision.
    */
    let shell = command_shell();
    let prompt_editor_attach_args = zmx_prompt_editor_attach_args(input.prompt_editor.as_deref());
    let script = format!(
        r#"
zmx_session={}
zmx_cwd={}
zmx_global_session_ref={}
zmx_gxserver_auth_token_file={}
zmx_gxserver_base_url={}
zmx_gxserver_protocol_version={}
zmx_persistence_notice_command={}
zmx_title_notice_command={}
zmx_bin={}
zmx_prompt_editor_attach_args={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.'
  exit 127
fi
export GHOSTEX_ZMX_BIN="$zmx_bin"
{}
if [ -n "$zmx_global_session_ref" ]; then
  export GHOSTEX_GLOBAL_SESSION_REF="$zmx_global_session_ref"
fi
if [ -n "$zmx_session" ]; then
  export GHOSTEX_SESSION_ID="$zmx_session"
fi
if [ -n "$zmx_gxserver_auth_token_file" ]; then
  export GHOSTEX_GXSERVER_AUTH_TOKEN_FILE="$zmx_gxserver_auth_token_file"
fi
if [ -n "$zmx_gxserver_base_url" ]; then
  export GHOSTEX_GXSERVER_BASE_URL="$zmx_gxserver_base_url"
fi
if [ -n "$zmx_gxserver_protocol_version" ]; then
  export GHOSTEX_GXSERVER_PROTOCOL_VERSION="$zmx_gxserver_protocol_version"
fi
if "$zmx_bin" list --short 2>/dev/null | grep -F -x -- "$zmx_session" >/dev/null 2>&1; then
  if [ -n "$zmx_title_notice_command" ]; then
    {} {} "$zmx_title_notice_command"
  fi
  exec "$zmx_bin" attach --require-existing $zmx_prompt_editor_attach_args "$zmx_session"
fi
if [ -n "$zmx_persistence_notice_command" ]; then
  {} {} "$zmx_persistence_notice_command"
fi
cd "$zmx_cwd" || exit
exec "$zmx_bin" attach --require-existing $zmx_prompt_editor_attach_args "$zmx_session"
"#,
        shell_quote(&input.session_name),
        shell_quote(&input.cwd),
        shell_quote(input.global_session_ref.as_deref().unwrap_or("")),
        shell_quote(input.gxserver_auth_token_file.as_deref().unwrap_or("")),
        shell_quote(input.gxserver_base_url.as_deref().unwrap_or("")),
        shell_quote(
            &input
                .gxserver_protocol_version
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ),
        shell_quote(&persistence_notice_shell_command(&input.session_name)),
        shell_quote(&session_title_shell_command(input.title.as_deref())),
        shell_quote(&input.zmx_executable_path),
        shell_quote(prompt_editor_attach_args),
        zmx_session_identity_reset_shell_command(),
        &shell.executable,
        shell.command_flag(false),
        &shell.executable,
        shell.command_flag(false),
    )
    .trim()
    .to_string();
    shell.command_string(&script, false)
}

fn build_started_zmx_attach_command(input: ZmxAttachCommandInput) -> String {
    /*
    createWorkspaceTerminal has just started this exact provider under a
    never-reused session identity. Keep the normal executable/env validation
    and zmx's require-existing failure contract, but do not launch a redundant
    `zmx list` process before attaching. Other lifecycle callers continue
    through build_zmx_attach_command and retain their canonical probe path.
    */
    let shell = command_shell();
    let prompt_editor_attach_args = zmx_prompt_editor_attach_args(input.prompt_editor.as_deref());
    let script = format!(
        r#"
zmx_session={}
zmx_global_session_ref={}
zmx_gxserver_auth_token_file={}
zmx_gxserver_base_url={}
zmx_gxserver_protocol_version={}
zmx_title_notice_command={}
zmx_bin={}
zmx_prompt_editor_attach_args={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.'
  exit 127
fi
export GHOSTEX_ZMX_BIN="$zmx_bin"
{}
if [ -n "$zmx_global_session_ref" ]; then
  export GHOSTEX_GLOBAL_SESSION_REF="$zmx_global_session_ref"
fi
if [ -n "$zmx_session" ]; then
  export GHOSTEX_SESSION_ID="$zmx_session"
fi
if [ -n "$zmx_gxserver_auth_token_file" ]; then
  export GHOSTEX_GXSERVER_AUTH_TOKEN_FILE="$zmx_gxserver_auth_token_file"
fi
if [ -n "$zmx_gxserver_base_url" ]; then
  export GHOSTEX_GXSERVER_BASE_URL="$zmx_gxserver_base_url"
fi
if [ -n "$zmx_gxserver_protocol_version" ]; then
  export GHOSTEX_GXSERVER_PROTOCOL_VERSION="$zmx_gxserver_protocol_version"
fi
if [ -n "$zmx_title_notice_command" ]; then
  {} {} "$zmx_title_notice_command"
fi
exec "$zmx_bin" attach --require-existing $zmx_prompt_editor_attach_args "$zmx_session"
"#,
        shell_quote(&input.session_name),
        shell_quote(input.global_session_ref.as_deref().unwrap_or("")),
        shell_quote(input.gxserver_auth_token_file.as_deref().unwrap_or("")),
        shell_quote(input.gxserver_base_url.as_deref().unwrap_or("")),
        shell_quote(
            &input
                .gxserver_protocol_version
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ),
        shell_quote(&session_title_shell_command(input.title.as_deref())),
        shell_quote(&input.zmx_executable_path),
        shell_quote(prompt_editor_attach_args),
        zmx_session_identity_reset_shell_command(),
        &shell.executable,
        shell.command_flag(false),
    )
    .trim()
    .to_string();
    shell.command_string(&script, false)
}

fn build_zmx_kill_command(session_name: &str, zmx_executable_path: &str) -> String {
    format!(
        r#"
zmx_session={}
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.'
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
exec "$zmx_bin" kill "$zmx_session" --force
"#,
        shell_quote(session_name),
        shell_quote(zmx_executable_path),
    )
    .trim()
    .to_string()
}

fn build_zmx_history_command(session_name: &str, zmx_executable_path: &str) -> String {
    format!(
        r#"
zmx_session={}
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
exec "$zmx_bin" history "$zmx_session"
"#,
        shell_quote(session_name),
        shell_quote(zmx_executable_path),
    )
    .trim()
    .to_string()
}

fn build_zmx_send_command(session_name: &str, zmx_executable_path: &str) -> String {
    format!(
        r#"
zmx_session={}
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
exec "$zmx_bin" send "$zmx_session"
"#,
        shell_quote(session_name),
        shell_quote(zmx_executable_path),
    )
    .trim()
    .to_string()
}

fn build_zmx_run_command(input: ZmxRunCommandInput) -> String {
    let startup_command =
        with_atuin_ignored_shell_history_prefix(input.startup_text.trim_end_matches(['\r', '\n']));
    let provider_shell_command = format!(
        "{}\n{}\n{}",
        zmx_provider_prompt_editor_setup_shell_command(input.prompt_editor.as_deref()),
        startup_command,
        user_login_shell_exec_command()
    );
    format_zmx_provider_run_script(
        &input.session_name,
        &input.cwd,
        input.global_session_ref.as_deref(),
        input.gxserver_auth_token_file.as_deref(),
        input.gxserver_base_url.as_deref(),
        input.gxserver_protocol_version,
        Some(&startup_command),
        &provider_shell_command,
        "zmx_startup_command",
        &input.zmx_executable_path,
    )
}

fn build_zmx_shell_provider_command(input: ZmxShellProviderCommandInput) -> String {
    let provider_shell_command = format!(
        "{}\n{}",
        zmx_provider_prompt_editor_setup_shell_command(input.prompt_editor.as_deref()),
        user_login_shell_exec_command()
    );
    format_zmx_provider_run_script(
        &input.session_name,
        &input.cwd,
        input.global_session_ref.as_deref(),
        input.gxserver_auth_token_file.as_deref(),
        input.gxserver_base_url.as_deref(),
        input.gxserver_protocol_version,
        None,
        &provider_shell_command,
        "zmx_shell_command",
        &input.zmx_executable_path,
    )
}

#[allow(clippy::too_many_arguments)]
fn format_zmx_provider_run_script(
    session_name: &str,
    cwd: &str,
    global_session_ref: Option<&str>,
    gxserver_auth_token_file: Option<&str>,
    gxserver_base_url: Option<&str>,
    gxserver_protocol_version: Option<u64>,
    startup_text: Option<&str>,
    provider_shell_command: &str,
    command_variable: &str,
    zmx_executable_path: &str,
) -> String {
    let shell = command_shell();
    let startup_text_assignment = startup_text
        .map(|text| format!("zmx_startup_text={}\n", shell_quote(text)))
        .unwrap_or_default();
    let startup_text_guard = if startup_text.is_some() {
        "if [ -z \"$zmx_startup_text\" ]; then\n  printf '%s\\n' 'gxserver startSessionProvider requires startup text.' >&2\n  exit 64\nfi\n"
    } else {
        ""
    };
    let command_arg = format!("${command_variable}");
    format!(
        r#"
zmx_session={}
zmx_cwd={}
zmx_global_session_ref={}
zmx_gxserver_auth_token_file={}
zmx_gxserver_base_url={}
zmx_gxserver_protocol_version={}
{}{}={}
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
export GHOSTEX_ZMX_BIN="$zmx_bin"
{}{}
if [ -n "$zmx_global_session_ref" ]; then
  export GHOSTEX_GLOBAL_SESSION_REF="$zmx_global_session_ref"
fi
if [ -n "$zmx_session" ]; then
  export GHOSTEX_SESSION_ID="$zmx_session"
fi
if [ -n "$zmx_gxserver_auth_token_file" ]; then
  export GHOSTEX_GXSERVER_AUTH_TOKEN_FILE="$zmx_gxserver_auth_token_file"
fi
if [ -n "$zmx_gxserver_base_url" ]; then
  export GHOSTEX_GXSERVER_BASE_URL="$zmx_gxserver_base_url"
fi
if [ -n "$zmx_gxserver_protocol_version" ]; then
  export GHOSTEX_GXSERVER_PROTOCOL_VERSION="$zmx_gxserver_protocol_version"
fi
cd "$zmx_cwd" || exit
exec "$zmx_bin" run "$zmx_session" -d --initial-command {} {} "{}"
"#,
        shell_quote(session_name),
        shell_quote(cwd),
        shell_quote(global_session_ref.unwrap_or("")),
        shell_quote(gxserver_auth_token_file.unwrap_or("")),
        shell_quote(gxserver_base_url.unwrap_or("")),
        shell_quote(
            &gxserver_protocol_version
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ),
        startup_text_assignment,
        command_variable,
        shell_quote(provider_shell_command),
        shell_quote(zmx_executable_path),
        startup_text_guard,
        zmx_session_identity_reset_shell_command(),
        &shell.executable,
        shell.command_flag(true),
        command_arg,
    )
    .trim()
    .to_string()
}

fn build_zmx_exists_command(session_name: &str, zmx_executable_path: &str) -> String {
    format!(
        r#"
zmx_session={}
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
zmx_sessions=$("$zmx_bin" list --short)
zmx_list_status=$?
if [ "$zmx_list_status" -ne 0 ]; then
  printf '%s\n' "zmx list --short failed with exit $zmx_list_status" >&2
  exit 2
fi
printf '%s\n' "$zmx_sessions" | grep -F -x -- "$zmx_session" >/dev/null 2>&1
"#,
        shell_quote(session_name),
        shell_quote(zmx_executable_path),
    )
    .trim()
    .to_string()
}

fn zmx_provider_prompt_editor_setup_shell_command(prompt_editor: Option<&str>) -> String {
    /*
    CDXC:PromptEditorBackend 2026-06-30-03:11:
    zmx providers still need a stable EDITOR wrapper so Ctrl+G can block through
    Ghostex when Monaco is available, but the non-Monaco path must run the
    remote shell's own editor instead of gte. Capture VISUAL/EDITOR before
    installing the wrapper, and export Monaco only for attach clients that
    explicitly advertised it.
    */
    let mut script = r#"
case "${GHOSTEX_HOME:-}" in
  /*) ghostex_prompt_editor_state_dir="$GHOSTEX_HOME/state" ;;
  *) case "${XDG_STATE_HOME:-}" in
       /*) ghostex_prompt_editor_state_dir="${XDG_STATE_HOME%/}/ghostex" ;;
       *) ghostex_prompt_editor_state_dir="$HOME/.local/state/ghostex" ;;
     esac ;;
esac
ghostex_prompt_editor_wrapper="$ghostex_prompt_editor_state_dir/prompt-editor"
ghostex_prompt_editor_machine_visual="${VISUAL:-}"
ghostex_prompt_editor_machine_editor="${EDITOR:-}"
mkdir -p "$(dirname "$ghostex_prompt_editor_wrapper")" 2>/dev/null || true
cat > "$ghostex_prompt_editor_wrapper" <<'__GHOSTEX_PROMPT_EDITOR_WRAPPER__'
#!/bin/sh
if [ -n "${GHOSTEX_ZMX_BIN:-}" ] && [ -x "${GHOSTEX_ZMX_BIN:-}" ]; then
  export GHOSTEX_ZMX_BIN
fi
if [ -n "${GHOSTEX_CLI_EXECUTABLE:-}" ] && [ -x "${GHOSTEX_CLI_EXECUTABLE:-}" ]; then
  exec "$GHOSTEX_CLI_EXECUTABLE" prompt-editor "$@"
fi
if command -v ghostex >/dev/null 2>&1; then
  exec ghostex prompt-editor "$@"
fi
exec /bin/sh -lc 'exec ${GHOSTEX_PROMPT_EDITOR_MACHINE_VISUAL:-${GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR:-vi}} "$@"' ghostex-prompt-editor "$@"
__GHOSTEX_PROMPT_EDITOR_WRAPPER__
chmod 755 "$ghostex_prompt_editor_wrapper" 2>/dev/null || true
export GHOSTEX_PROMPT_EDITOR_MACHINE_VISUAL="$ghostex_prompt_editor_machine_visual"
export GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR="$ghostex_prompt_editor_machine_editor"
export EDITOR="$ghostex_prompt_editor_wrapper"
export VISUAL="$ghostex_prompt_editor_wrapper"
"#
    .trim()
    .to_string();
    if prompt_editor == Some("monaco") {
        script.push_str("\nexport GHOSTEX_PROMPT_EDITOR_BACKEND=monaco");
    }
    script.push_str("\nexport GHOSTEX_PROMPT_EDITING_ENABLED=1");
    script
}

fn zmx_prompt_editor_attach_args(prompt_editor: Option<&str>) -> &'static str {
    match prompt_editor {
        Some("monaco") => "--prompt-editor=monaco",
        Some("code-server") => "--prompt-editor=code-server",
        _ => "",
    }
}

fn zmx_session_identity_reset_shell_command() -> String {
    format!("unset {}", session_identity_environment_keys().join(" "))
}

fn persistence_notice_shell_command(session_name: &str) -> String {
    format!(
        "printf '%s\\n' {}",
        shell_quote(&format!(
            "This session is using zmx persistence: {session_name}"
        ))
    )
}

fn session_title_shell_command(title: Option<&str>) -> String {
    let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) else {
        return String::new();
    };
    format!("printf '%s\\n' {}", shell_quote(title))
}

fn with_atuin_ignored_shell_history_prefix(text: &str) -> String {
    let trimmed_right = text.trim_end();
    if trimmed_right.trim().is_empty() {
        return String::new();
    }
    if trimmed_right.starts_with(' ') {
        trimmed_right.to_string()
    } else {
        format!(" {}", trimmed_right.trim_start())
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn read_lifecycle_params(params: &Map<String, Value>) -> Result<LifecycleParams, DomainStateError> {
    Ok(LifecycleParams {
        project_id: read_project_id(params)?,
        session_id: read_session_id(params)?,
    })
}

fn prompt_editor_mode_from_params(
    params: &Map<String, Value>,
) -> Result<Option<String>, DomainStateError> {
    match params.get("promptEditor") {
        None => Ok(None),
        Some(Value::String(value)) if matches!(value.as_str(), "monaco" | "code-server") => {
            Ok(Some(value.clone()))
        }
        Some(_) => Err(DomainStateError::bad_request(
            "promptEditor must be monaco or code-server when provided.",
        )),
    }
}

fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| js_string(Some(value)))
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".to_string(),
        Some(value) => value.to_string(),
    }
}

fn require_session(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
) -> Result<Value, DomainStateError> {
    repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session {}/{} does not exist.",
                lifecycle.project_id, lifecycle.session_id
            ))
        })
}

fn require_zmx() -> ZmxEndpointResult<GxserverResolvedTool> {
    require_bundled_zmx().map_err(ZmxEndpointError::DependencyUnavailable)
}

pub(crate) fn provider_zmx_session_name(session: &Value) -> Result<String, DomainStateError> {
    /*
    CDXC:GxserverUnsupportedSessionParity 2026-06-22-07:30:
    TypeScript gxserver does not have an unsupported-session branch for zmx lifecycle or session-I/O endpoints. Route every persisted session through the canonical top-level zmxName, ignoring providerState.provider, providerState.zmxName, and runtimeSettings.sessionPersistenceProvider so provider-off, missing-provider, and migrated rows keep the same error/order behavior.
    */
    string_field(session, "zmxName").ok_or_else(|| {
        DomainStateError::corrupt_state("zmxName missing from session domain state.")
    })
}

fn provider_state_patch(
    session: &Value,
    probe: &ProviderProbe,
) -> Result<Map<String, Value>, DomainStateError> {
    let mut provider_state = session
        .get("providerState")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    provider_state.remove("killError");
    provider_state.insert(
        "lifecycleState".to_string(),
        Value::String(probe.lifecycle_state.clone()),
    );
    if let Some(error) = &probe.error {
        provider_state.insert("probeError".to_string(), Value::String(error.clone()));
    } else {
        provider_state.remove("probeError");
    }
    provider_state.insert(
        "probedAt".to_string(),
        Value::String(probe.probed_at.clone()),
    );
    provider_state.insert("zmxName".to_string(), Value::String(probe.zmx_name.clone()));
    Ok(provider_state)
}

fn missing_provider_state_patch(
    session: &Value,
    timestamp: &str,
) -> Result<Map<String, Value>, DomainStateError> {
    let mut provider_state = session
        .get("providerState")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    provider_state.remove("killError");
    provider_state.remove("probeError");
    provider_state.insert("lifecycleState".to_string(), json!("missing"));
    provider_state.insert("probedAt".to_string(), json!(timestamp));
    provider_state.insert(
        "zmxName".to_string(),
        json!(provider_zmx_session_name(session)?),
    );
    Ok(provider_state)
}

fn failed_kill_provider_state_patch(
    session: &Value,
    kill: &ProviderKill,
    timestamp: &str,
) -> Result<Map<String, Value>, DomainStateError> {
    let error = kill
        .error
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| (!kill.stderr.trim().is_empty()).then(|| kill.stderr.clone()))
        .unwrap_or_else(|| format!("zmx kill command exited {}", kill.exit_code));
    let mut provider_state = session
        .get("providerState")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    provider_state.insert("killError".to_string(), Value::String(error.clone()));
    provider_state.insert("lifecycleState".to_string(), json!("unknown"));
    provider_state.insert("probeError".to_string(), Value::String(error));
    provider_state.insert("probedAt".to_string(), json!(timestamp));
    provider_state.insert(
        "zmxName".to_string(),
        json!(provider_zmx_session_name(session)?),
    );
    Ok(provider_state)
}

fn reconcile_domain_lifecycle_from_provider_probe(
    current_lifecycle_state: &str,
    provider_lifecycle_state: &str,
) -> String {
    if provider_lifecycle_state == "exists" && current_lifecycle_state != "stopped" {
        "running".to_string()
    } else {
        current_lifecycle_state.to_string()
    }
}

fn probe_to_value(probe: &ProviderProbe) -> Value {
    let mut value = Map::new();
    if let Some(error) = &probe.error {
        value.insert("error".to_string(), Value::String(error.clone()));
    }
    value.insert(
        "lifecycleState".to_string(),
        Value::String(probe.lifecycle_state.clone()),
    );
    value.insert(
        "probedAt".to_string(),
        Value::String(probe.probed_at.clone()),
    );
    value.insert("zmxName".to_string(), Value::String(probe.zmx_name.clone()));
    Value::Object(value)
}

fn kill_to_value(kill: &ProviderKill) -> Value {
    let mut value = Map::new();
    if let Some(error) = &kill.error {
        value.insert("error".to_string(), Value::String(error.clone()));
    }
    value.insert("exitCode".to_string(), json!(kill.exit_code));
    value.insert("killed".to_string(), Value::Bool(kill.killed));
    value.insert("stderr".to_string(), Value::String(kill.stderr.clone()));
    value.insert("stdout".to_string(), Value::String(kill.stdout.clone()));
    value.insert("zmxName".to_string(), Value::String(kill.zmx_name.clone()));
    Value::Object(value)
}

fn decide_startup_text_disposition(provider_state: &str, startup_text: Option<&str>) -> String {
    if startup_text
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return "none".to_string();
    }
    if provider_state == "exists" {
        "discardExistingProvider".to_string()
    } else if provider_state == "unknown" {
        "discardUnknownProvider".to_string()
    } else {
        "queueAfterTerminalReady".to_string()
    }
}

fn maybe_insert_startup_text(
    output: &mut Map<String, Value>,
    disposition: &str,
    startup_text: Option<&str>,
) {
    if disposition == "queueAfterTerminalReady" {
        if let Some(startup_text) = startup_text.filter(|value| !value.trim().is_empty()) {
            output.insert(
                "startupText".to_string(),
                Value::String(startup_text.to_string()),
            );
        }
    }
}

fn normalize_optional_startup_text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

fn get_agent_launch_startup_text_for_session(session: &Value) -> Option<String> {
    session
        .get("launchSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentLaunchPlan"))
        .and_then(Value::as_object)
        .and_then(|plan| plan.get("startupText"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

fn get_queued_agent_launch_startup_text_for_session(session: &Value) -> Option<String> {
    if !has_queued_agent_launch_startup_text(session) {
        return None;
    }
    get_agent_launch_startup_text_for_session(session)
}

pub(crate) fn get_persisted_provider_startup_text_for_session(
    project: &Value,
    session: &Value,
    agent_settings: &Map<String, Value>,
) -> Option<String> {
    get_queued_agent_launch_startup_text_for_session(session)
        .or_else(|| get_agent_startup_text_for_session(project, session, agent_settings))
}

fn has_queued_agent_launch_startup_text(session: &Value) -> bool {
    session
        .get("launchSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("runtimeRelevant"))
        .and_then(Value::as_object)
        .and_then(|runtime| runtime.get("queueProviderStartupText"))
        .and_then(Value::as_bool)
        == Some(true)
}

fn launch_settings_with_consumed_agent_launch_startup_text(
    session: &Value,
) -> Option<Map<String, Value>> {
    if !has_queued_agent_launch_startup_text(session) {
        return None;
    }
    let mut launch_settings = session
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut runtime_relevant = launch_settings
        .get("runtimeRelevant")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_relevant.insert("queueProviderStartupText".to_string(), Value::Bool(false));
    launch_settings.insert(
        "runtimeRelevant".to_string(),
        Value::Object(runtime_relevant),
    );
    Some(launch_settings)
}

fn consume_queued_agent_launch_startup_text(
    repository: &DomainRepository<'_>,
    session: &Value,
) -> ZmxEndpointResult<Value> {
    let Some(launch_settings) = launch_settings_with_consumed_agent_launch_startup_text(session)
    else {
        return Ok(session.clone());
    };
    let mut update = Map::new();
    update.insert("projectId".to_string(), value_field(session, "projectId")?);
    update.insert("sessionId".to_string(), value_field(session, "sessionId")?);
    update.insert("launchSettings".to_string(), Value::Object(launch_settings));
    repository
        .update_session_for_lifecycle(&update)
        .map_err(ZmxEndpointError::Domain)
}

fn read_interaction_text(
    value: Option<&Value>,
    command_name: &str,
) -> Result<String, DomainStateError> {
    let Some(text) = value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return Err(DomainStateError::bad_request(format!(
            "{command_name} requires non-empty text."
        )));
    };
    if text.len() > GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES {
        return Err(DomainStateError::bad_request(format!(
            "{command_name} text exceeds the {GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES}-byte zmx send limit."
        )));
    }
    Ok(text.to_string())
}

fn zmx_probe_exit_error_message(result: &ZmxCommandResult) -> String {
    if !result.stderr.trim().is_empty() {
        return result.stderr.trim().to_string();
    }
    if !result.stdout.trim().is_empty() {
        return result.stdout.trim().to_string();
    }
    format!("zmx probe command exited {}", result.exit_code)
}

fn session_target_from_lifecycle_result(result: &Value) -> Option<(String, String)> {
    let session = result.get("session").or_else(|| {
        result
            .get("attach")
            .and_then(|attach| attach.get("session"))
    })?;
    Some((
        string_field(session, "projectId")?,
        string_field(session, "sessionId")?,
    ))
}

fn value_field(value: &Value, key: &str) -> Result<Value, DomainStateError> {
    value.get(key).cloned().ok_or_else(|| {
        DomainStateError::corrupt_state(format!("{key} missing from gxserver response state."))
    })
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn cwd_exists(cwd: &str) -> bool {
    let trimmed = cwd.trim();
    !trimmed.is_empty() && Path::new(trimmed).is_dir()
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn build_gxserver_zmx_child_environment() -> HashMap<String, String> {
    let mut environment = std::env::vars().collect::<HashMap<_, _>>();
    for key in environment_keys_to_strip() {
        environment.remove(key);
    }
    remove_gxserver_zmx_color_disabling_environment_values(&mut environment);
    environment.insert("COLORTERM".to_string(), "truecolor".to_string());
    environment.insert("TERM_PROGRAM".to_string(), "ghostty".to_string());
    if let Some(resources_dir) = environment
        .get("GHOSTTY_RESOURCES_DIR")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    {
        environment.insert("TERM".to_string(), "xterm-ghostty".to_string());
        if let Some(parent) = Path::new(&resources_dir).parent() {
            environment.insert(
                "TERMINFO".to_string(),
                parent.join("terminfo").to_string_lossy().to_string(),
            );
        }
    } else {
        environment.insert("TERM".to_string(), "xterm-256color".to_string());
    }
    environment
}

fn remove_gxserver_zmx_color_disabling_environment_values(
    environment: &mut HashMap<String, String>,
) {
    /*
    CDXC:FactoryDroidColorEnv 2026-06-30-22:56:
    Factory-created Droid sessions run inside gxserver-owned zmx provider children and may honor FORCE_COLOR=0 from the Ghostex launch environment. Strip only disabling FORCE_COLOR values here so interactive Ghostty sessions keep color while positive FORCE_COLOR overrides remain intact.
    */
    if environment
        .get("FORCE_COLOR")
        .is_some_and(|value| environment_value_disables_color(value))
    {
        environment.remove("FORCE_COLOR");
    }
}

fn environment_value_disables_color(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "0" | "false")
}

fn environment_keys_to_strip() -> Vec<&'static str> {
    let mut keys = Vec::new();
    keys.extend([
        "ANSI_COLORS_DISABLED",
        "NO_COLOR",
        "NODE_DISABLE_COLORS",
        "COLORTERM",
        "TERM",
        "TERMINFO",
        "TERM_PROGRAM",
        "TERM_PROGRAM_VERSION",
        "LaunchInstanceID",
        "XPC_FLAGS",
        "XPC_SERVICE_NAME",
        "__CFBundleIdentifier",
    ]);
    keys.extend(session_identity_environment_keys());
    keys
}

fn session_identity_environment_keys() -> Vec<&'static str> {
    vec![
        "GHOSTEX_AGENT",
        "GHOSTEX_GLOBAL_SESSION_REF",
        "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
        "GHOSTEX_GXSERVER_BASE_URL",
        "GHOSTEX_GXSERVER_PROTOCOL_VERSION",
        "GHOSTEX_NATIVE_SESSION_ID",
        "GHOSTEX_SESSION_ID",
        "GHOSTEX_SESSION_STATE_FILE",
        "GHOSTEX_WORKSPACE_ID",
        "GHOSTEX_WORKSPACE_ROOT",
        "VSMUX_AGENT",
        "VSMUX_SESSION_ID",
        "VSMUX_SESSION_STATE_FILE",
        "VSMUX_WORKSPACE_ID",
        "VSMUX_WORKSPACE_ROOT",
        "ZMX_SESSION",
        "ZMX_SESSION_PREFIX",
        "ghostex_AGENT",
        "ghostex_SESSION_ID",
        "ghostex_SESSION_STATE_FILE",
        "ghostex_WORKSPACE_ID",
        "ghostex_WORKSPACE_ROOT",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        paths::get_gxserver_paths,
        storage::{initialize_gxserver_storage, open_gxserver_database},
    };

    fn open_test_database() -> (tempfile::TempDir, rusqlite::Connection) {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        (temp, db)
    }

    #[test]
    fn wake_requires_session_provider_start_only_for_missing_zmx_provider() {
        assert!(wake_requires_session_provider_start(&json!({
            "provider": "zmx",
            "providerState": { "lifecycleState": "missing" },
        })));
        assert!(!wake_requires_session_provider_start(&json!({
            "provider": "zmx",
            "providerState": { "lifecycleState": "exists" },
        })));
        assert!(!wake_requires_session_provider_start(&json!({
            "provider": "zmx",
            "providerState": { "lifecycleState": "unknown" },
        })));
        assert!(!wake_requires_session_provider_start(&json!({
            "provider": "local",
            "providerState": { "lifecycleState": "missing" },
        })));
    }

    #[test]
    fn ghostex_attach_command_requires_a_prestarted_provider() {
        let command = build_zmx_attach_command(ZmxAttachCommandInput {
            cwd: "/tmp/project".to_string(),
            global_session_ref: Some("S7k:P100:G100".to_string()),
            gxserver_auth_token_file: Some("/tmp/home/.ghostex/gxserver/auth/token".to_string()),
            gxserver_base_url: Some("http://127.0.0.1:58746".to_string()),
            gxserver_protocol_version: Some(1),
            prompt_editor: Some("monaco".to_string()),
            session_name: "S7k-P100-G100".to_string(),
            title: Some("Terminal".to_string()),
            zmx_executable_path: "/repo/zmx/zig-out/bin/zmx".to_string(),
        });
        assert_eq!(command.matches("attach --require-existing").count(), 2);
        assert!(command.contains("--prompt-editor=monaco"));

        let remote_command = build_zmx_attach_command(ZmxAttachCommandInput {
            prompt_editor: Some("code-server".to_string()),
            ..ZmxAttachCommandInput {
                cwd: "/srv/project".to_string(),
                global_session_ref: Some("S7k:P100:G100".to_string()),
                gxserver_auth_token_file: Some("/tmp/ghostex/token".to_string()),
                gxserver_base_url: Some("http://127.0.0.1:58744".to_string()),
                gxserver_protocol_version: Some(1),
                prompt_editor: None,
                session_name: "S7k-P100-G100".to_string(),
                title: Some("Remote terminal".to_string()),
                zmx_executable_path: "/srv/ghostex/zmx".to_string(),
            }
        });
        assert!(remote_command.contains("--prompt-editor=code-server"));
    }

    #[test]
    fn zmx_run_command_uses_initial_command_and_ghostex_identity() {
        let command = build_zmx_run_command(ZmxRunCommandInput {
            cwd: "/tmp/project".to_string(),
            global_session_ref: Some("S7k:P100:G100".to_string()),
            gxserver_auth_token_file: Some("/tmp/home/.ghostex/gxserver/auth/token".to_string()),
            gxserver_base_url: Some("http://127.0.0.1:58746".to_string()),
            gxserver_protocol_version: Some(1),
            prompt_editor: None,
            session_name: "S7k-P100-G100".to_string(),
            startup_text: "codex --yolo\r".to_string(),
            zmx_executable_path: "/repo/zmx/zig-out/bin/zmx".to_string(),
        });
        assert!(command.contains(
            "run \"$zmx_session\" -d --initial-command /bin/zsh -lic \"$zmx_startup_command\""
        ));
        assert!(command.contains("zmx_startup_text=' codex --yolo'"));
        assert!(command.contains("export GHOSTEX_GLOBAL_SESSION_REF=\"$zmx_global_session_ref\""));
        assert!(command.contains(
            "ghostex_prompt_editor_wrapper=\"$ghostex_prompt_editor_home/state/prompt-editor\""
        ));
        assert!(command.contains("GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR"));
        assert!(!command.contains("export GHOSTEX_PROMPT_EDITOR_BACKEND=monaco"));
        assert!(!command.contains("PATH zmx"));
    }

    #[test]
    fn zmx_child_environment_strips_only_disabling_force_color_values() {
        let mut environment = HashMap::from([
            ("FORCE_COLOR".to_string(), "0".to_string()),
            ("OTHER_KEY".to_string(), "kept".to_string()),
        ]);
        remove_gxserver_zmx_color_disabling_environment_values(&mut environment);
        assert!(!environment.contains_key("FORCE_COLOR"));
        assert_eq!(
            environment.get("OTHER_KEY").map(String::as_str),
            Some("kept")
        );

        environment.insert("FORCE_COLOR".to_string(), "2".to_string());
        remove_gxserver_zmx_color_disabling_environment_values(&mut environment);
        assert_eq!(
            environment.get("FORCE_COLOR").map(String::as_str),
            Some("2")
        );

        environment.insert("FORCE_COLOR".to_string(), " false ".to_string());
        remove_gxserver_zmx_color_disabling_environment_values(&mut environment);
        assert!(!environment.contains_key("FORCE_COLOR"));
    }

    #[test]
    fn provider_state_patches_preserve_unknown_failed_kill_route() {
        let session = json!({
            "providerState": { "lifecycleState": "exists", "provider": "zmx" },
            "zmxName": "S7k-P100-G100",
        });
        let kill = ProviderKill {
            error: Some("zmx kill failed".to_string()),
            exit_code: 42,
            killed: false,
            stderr: "zmx kill failed".to_string(),
            stdout: String::new(),
            zmx_name: "S7k-P100-G100".to_string(),
        };
        let patch = failed_kill_provider_state_patch(&session, &kill, "2026-06-15T18:06:00.000Z")
            .expect("patch");
        assert_eq!(patch.get("lifecycleState"), Some(&json!("unknown")));
        assert_eq!(patch.get("zmxName"), Some(&json!("S7k-P100-G100")));
        assert_eq!(patch.get("killError"), Some(&json!("zmx kill failed")));
    }

    #[test]
    fn provider_metadata_does_not_create_unsupported_session_route() {
        for (session, expected_zmx_name, expected_provider) in [
            (
                json!({
                    "providerState": {
                        "lifecycleState": "exists",
                        "provider": "off",
                        "zmxName": "legacy-provider-name"
                    },
                    "runtimeSettings": { "sessionPersistenceProvider": "off" },
                    "zmxName": "S7k-P100-G100"
                }),
                "S7k-P100-G100",
                Some("off"),
            ),
            (
                json!({
                    "providerState": {
                        "lifecycleState": "missing",
                        "provider": "tmux",
                        "zmxName": "legacy-tmux-name"
                    },
                    "runtimeSettings": { "sessionPersistenceProvider": "tmux" },
                    "zmxName": "S7k-P100-G101"
                }),
                "S7k-P100-G101",
                Some("tmux"),
            ),
            (
                json!({
                    "runtimeSettings": {},
                    "zmxName": "S7k-P100-G102"
                }),
                "S7k-P100-G102",
                None,
            ),
        ] {
            assert_eq!(
                provider_zmx_session_name(&session).expect("zmx name"),
                expected_zmx_name
            );
            let probe = ProviderProbe {
                error: None,
                lifecycle_state: "exists".to_string(),
                probed_at: "2026-06-22T07:30:00.000Z".to_string(),
                zmx_name: provider_zmx_session_name(&session).expect("zmx name"),
            };
            let patch = provider_state_patch(&session, &probe).expect("provider patch");
            assert_eq!(patch.get("zmxName"), Some(&json!(probe.zmx_name)));
            assert_eq!(patch.get("lifecycleState"), Some(&json!("exists")));
            assert_eq!(
                patch.get("provider").and_then(Value::as_str),
                expected_provider
            );
        }
    }

    #[test]
    fn wake_activity_suppression_resets_stale_working_state() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "Wake Activity" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "agentId": "codex",
                    "kind": "agent",
                    "lifecycleState": "running",
                    "projectId": project_id,
                    "providerState": { "lifecycleState": "missing", "provider": "zmx" },
                    "runtimeSettings": {
                        "agentActivity": {
                            "activity": "working",
                            "agentName": "codex",
                            "hasSeenWorking": true,
                            "isAcknowledged": false,
                            "lastChangedAt": "2026-06-10T06:50:00.000Z",
                            "workingStartedAt": "2026-06-10T06:50:00.000Z"
                        },
                        "titleSource": "user"
                    },
                    "title": "Sleeping private session"
                })
                .as_object()
                .expect("session params"),
                true,
            )
            .expect("session");
        let before_wake_ms = chrono::Utc::now().timestamp_millis();

        let updated =
            apply_wake_session_activity_suppression(&repository, &session).expect("suppression");
        let activity = updated
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("agentActivity"))
            .and_then(Value::as_object)
            .expect("agent activity");

        assert_eq!(activity.get("activity"), Some(&json!("idle")));
        assert_eq!(activity.get("hasSeenWorking"), Some(&json!(false)));
        assert_eq!(activity.get("isAcknowledged"), Some(&json!(true)));
        let suppressed_until = activity
            .get("suppressedUntil")
            .and_then(Value::as_str)
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .expect("suppressedUntil")
            .timestamp_millis();
        assert!(suppressed_until - before_wake_ms > 10_000);
    }

    #[test]
    fn transition_action_errors_use_javascript_stringification() {
        assert_eq!(js_string(Some(&json!("pause"))), "pause");
        assert_eq!(
            js_string(Some(&json!({ "action": "pause" }))),
            "[object Object]"
        );
        assert_eq!(js_string(None), "undefined");
    }

    #[test]
    fn zmx_process_identity_parser_prefers_live_codex_child() {
        let session_name = "S90-P3lv0-G0p1k".to_string();
        let identities = parse_zmx_session_process_identities(
            r#"
81395     1 /bundle/zmx run S90-P3lv0-G0p1k -d --initial-command /bin/zsh -lic gx f
81396 81395 /bin/zsh -lic gx f
81557 81396 node /Applications/Ghostex.app/Contents/Resources/CLI/ghostex-cli.mjs f
81582 81557 /Applications/Ghostex.app/Contents/Resources/Web/bin/zehn --accept-all
82148 81582 node /Users/madda/.local/bin/codex --yolo resume 019EB8D0-D27B-7F30-B6D7-7A04AB8FAE78
82149 82148 /Users/madda/.local/lib/codex --yolo resume 019eb8d0-d27b-7f30-b6d7-7a04ab8fae78
94784 93944 /Users/madda/.local/bin/claude --resume 303d77cf-4871-48da-871f-47782e834307
"#
            .trim(),
            std::slice::from_ref(&session_name),
            &format!(
                "  name={session_name}\tpid=81396\tclients=1\tcreated=1781219985\tstart_dir=/repo"
            ),
        );
        let identity = identities.get(&session_name).expect("identity");
        assert_eq!(identity.agent_id.as_deref(), Some("codex"));
        assert_eq!(
            identity.agent_session_id.as_deref(),
            Some("019eb8d0-d27b-7f30-b6d7-7a04ab8fae78")
        );
        assert_eq!(identity.agent_session_path, None);
    }

    #[test]
    fn codex_rollout_path_resolves_exact_session_identity() {
        let path = Path::new(
            "/home/person/.codex/sessions/2026/08/07/rollout-2026-08-07T08-15-34-019fda6e-fdbe-7570-a4fd-347e9e0bfb40.jsonl",
        );
        assert_eq!(
            codex_session_id_from_transcript_path(path).as_deref(),
            Some("019fda6e-fdbe-7570-a4fd-347e9e0bfb40")
        );
        assert_eq!(
            codex_session_id_from_transcript_path(Path::new(
                "/home/person/.codex/archive/rollout-2026-08-07T08-15-34-019fda6e-fdbe-7570-a4fd-347e9e0bfb40.jsonl"
            )),
            None
        );
    }

    #[test]
    fn codex_process_open_rollout_resolves_exact_session_identity() {
        let temp = tempfile::tempdir().expect("tempdir");
        let sessions = temp
            .path()
            .join("sessions")
            .join("2026")
            .join("08")
            .join("11");
        fs::create_dir_all(&sessions).expect("sessions directory");
        let rollout =
            sessions.join("rollout-2026-08-11T20-15-34-019fda6e-fdbe-7570-a4fd-347e9e0bfb40.jsonl");
        let _open_rollout = fs::File::create(&rollout).expect("open rollout");
        let identity = read_codex_process_session_identity(Some(i64::from(std::process::id())))
            .expect("process rollout identity");
        assert_eq!(identity.0, "019fda6e-fdbe-7570-a4fd-347e9e0bfb40");
        assert_eq!(Path::new(&identity.1), rollout);
    }

    #[test]
    fn zmx_process_identity_parser_keeps_omp_terminal_name() {
        let session_name = "S90-P3lv0-G0omp".to_string();
        let identities = parse_zmx_session_process_identities(
            r#"
100     1 ttys006 /bundle/zmx run S90-P3lv0-G0omp -d --initial-command /bin/zsh -lic omp
101   100 ttys006 /bin/zsh -lic omp
102   101 ttys006 bun /Users/person/.bun/bin/omp
"#
            .trim(),
            std::slice::from_ref(&session_name),
            &format!(
                "  name={session_name}\tpid=100\tclients=1\tcreated=1781219985\tstart_dir=/repo"
            ),
        );
        let identity = identities.get(&session_name).expect("identity");
        assert_eq!(identity.agent_id.as_deref(), Some("omp"));
        assert_eq!(identity.agent_session_id, None);
        assert_eq!(identity.terminal_name.as_deref(), Some("ttys006"));
    }

    #[test]
    fn omp_terminal_record_resolves_exact_transcript_identity() {
        let temp = tempfile::tempdir().expect("tempdir");
        let agent_dir = temp.path().join(".omp").join("agent");
        let transcript_path = agent_dir
            .join("sessions")
            .join("repo")
            .join("2026-08-05T20-29-13-027Z_019fd39d-a9c3-7000-b87f-5ac5c34a9778.jsonl");
        fs::create_dir_all(transcript_path.parent().expect("transcript parent"))
            .expect("transcript directory");
        fs::write(&transcript_path, "{}\n").expect("transcript");
        let terminal_records = agent_dir.join("terminal-sessions");
        fs::create_dir_all(&terminal_records).expect("terminal records");
        fs::write(
            terminal_records.join("ttys006"),
            format!("/repo\n{}\nfresh\n", transcript_path.display()),
        )
        .expect("terminal record");

        let identity =
            read_omp_terminal_session_identity_from_agent_dir(&agent_dir, "ttys006", temp.path())
                .expect("OMP transcript identity");
        assert_eq!(identity.0, "019fd39d-a9c3-7000-b87f-5ac5c34a9778");
        assert_eq!(identity.1, transcript_path.to_string_lossy());
    }

    #[test]
    fn zmx_process_identity_parser_recognizes_attached_codex_resume_without_thread_id() {
        let session_name = "S2o-P7n77-G8wrt".to_string();
        let identities = parse_zmx_session_process_identities(
            r#"
52961     1 /home/ghostex/.ghostex/gxserver/package/bin/zmx attach --prompt-editor=monaco S2o-P7n77-G8wrt
52962 52961 -bash
52966 52962 codex --yolo resume
"#
            .trim(),
            std::slice::from_ref(&session_name),
            &format!(
                "  name={session_name}\tpid=52962\tclients=1\tcreated=1782781010\tstart_dir=/home/ghostex/ghostex"
            ),
        );
        /*
        CDXC:GxserverSessionIdentity 2026-06-30-11:15:
        Attached remote zmx terminals can run Codex as a shell child without a
        resume thread id argument. The live-process scanner still needs to
        identify the agent so gxserver can project the sidebar agent icon.
        */
        let identity = identities.get(&session_name).expect("identity");
        assert_eq!(identity.agent_id.as_deref(), Some("codex"));
        assert_eq!(identity.agent_session_id, None);
        assert_eq!(identity.agent_session_path, None);
    }

    #[test]
    fn zmx_process_identity_parser_ignores_helper_payload_agent_mentions() {
        let session_name = "S90-P3lv0-G8cl2".to_string();
        let identities = parse_zmx_session_process_identities(
            r#"
23572     1 -zsh
23754 23572 node /Users/person/.local/share/mise/installs/node/24.14.1/bin/codex --yolo
23755 23754 /Users/person/.local/share/mise/installs/node/24.14.1/lib/node_modules/@openai/codex/node_modules/@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/bin/codex --yolo
34225 23755 /Users/person/.codex/computer-use/Codex Computer Use.app/Contents/SharedSupport/SkyComputerUseClient.app/Contents/MacOS/SkyComputerUseClient turn-ended {"input-messages":["compare codex hotkeys with claude code"]}
"#
            .trim(),
            std::slice::from_ref(&session_name),
            &format!("  name={session_name}\tpid=23572\tclients=1\tcreated=1781317239\tstart_dir=/repo"),
        );
        /*
        CDXC:GxserverSessionIdentity 2026-06-21-18:25:
        Rust process identity parsing must copy TypeScript's helper-process guard. Agent labels come from actual executable tokens, not serialized helper payloads that may mention other CLIs in user-owned text.
        */
        let identity = identities.get(&session_name).expect("identity");
        assert_eq!(identity.agent_id.as_deref(), Some("codex"));
        assert_eq!(identity.agent_session_id, None);
        assert_eq!(identity.agent_session_path, None);
    }

    #[test]
    fn send_payload_validation_uses_utf8_byte_cap() {
        assert!(read_interaction_text(Some(&json!("hello")), "sendSessionText").is_ok());
        let error = read_interaction_text(Some(&json!("")), "sendSessionText")
            .expect_err("empty text rejected");
        assert_eq!(error.code, "badRequest");
        let rocket = "\u{1F680}";
        assert!(read_interaction_text(
            Some(&json!(rocket.repeat(GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES / 4))),
            "sendSessionText",
        )
        .is_ok());
        let oversized = "x".repeat(GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES + 1);
        let error = read_interaction_text(Some(&json!(oversized)), "sendSessionText")
            .expect_err("oversized text rejected");
        assert!(error.message.contains("zmx send limit"));
    }

    #[test]
    fn send_result_text_length_matches_javascript_string_length() {
        let result = send_result(
            0,
            json!({ "projectId": "P7abc", "sessionId": "G8def" }),
            "go \u{1F680}",
            false,
            "S7k-P7abc-G8def".to_string(),
        );
        assert_eq!(result["textBytes"], json!(7));
        assert_eq!(result["textLength"], json!(5));
    }

    #[test]
    fn queued_launch_startup_text_is_explicit_and_consumable() {
        let session = json!({
            "launchSettings": {
                "agentLaunchPlan": {
                    "startupText": " cursor-agent --yolo\r"
                },
                "runtimeRelevant": {
                    "queueProviderStartupText": true
                }
            }
        });
        assert_eq!(
            get_queued_agent_launch_startup_text_for_session(&session),
            Some(" cursor-agent --yolo\r".to_string())
        );
        let consumed =
            launch_settings_with_consumed_agent_launch_startup_text(&session).expect("consumed");
        assert_eq!(
            consumed
                .get("runtimeRelevant")
                .and_then(Value::as_object)
                .and_then(|runtime| runtime.get("queueProviderStartupText")),
            Some(&Value::Bool(false))
        );
    }

    #[test]
    fn startup_text_disposition_never_replays_live_provider_text() {
        assert_eq!(
            decide_startup_text_disposition("exists", Some(" codex --yolo")),
            "discardExistingProvider"
        );
        assert_eq!(
            decide_startup_text_disposition("unknown", Some(" codex --yolo")),
            "discardUnknownProvider"
        );
        assert_eq!(
            decide_startup_text_disposition("missing", Some(" codex --yolo")),
            "queueAfterTerminalReady"
        );
        assert_eq!(decide_startup_text_disposition("missing", None), "none");
    }
}
