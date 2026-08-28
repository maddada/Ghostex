use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use crate::{
    agents::apply_created_session_identity,
    constants::GXSERVER_PROTOCOL_VERSION,
    domain::{read_project_id, DomainRepository, DomainStateError},
    logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger, LogLevel},
    paths::get_gxserver_paths,
    toolchain::require_bundled_zmx,
};

use super::*;

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
    /*
    CDXC:SessionChatVerifyPaste 2026-08-24: chat's queue writes are recorded
    whatever they carry. A lost message's paste body and its Enter contain no
    listed control byte, so the control-byte filter used to drop exactly the
    two entries a delivery report needs — the sequence left no trace at all.
    Still metadata only (byte length and classifications, never the text), and
    still gated behind the diagnostic scenario like every other routine entry.
    */
    let session_chat_queue_write = operation == "sessionChatQueueWrite";
    if controls.is_empty() && !title_flow && !session_chat_queue_write {
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
    let provider_state_patch =
        match started_provider_state_patch(&session, &provider_state, &zmx.executable_path) {
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
            {
                if crate::session_keep_awake::is_held_awake(
                    &lifecycle.project_id,
                    &lifecycle.session_id,
                ) || crate::session_chat_queue::session_has_pending_session_chat_queue(
                    repository.connection(),
                    &lifecycle.project_id,
                    &lifecycle.session_id,
                ) {
                    let session = require_session(repository, &lifecycle)?;
                    return Ok(ZmxEndpointOutput {
                        created_workspace_terminal: None,
                        result: json!({ "declined": "keptAwake", "session": session }),
                        presentation_session: None,
                    });
                }
                /*
                CDXC:AutoSleepNeverActive 2026-08-22:
                A session that has never been active has never been prompted:
                `lastActiveAt` is written only when a session enters working or
                attention. An inactivity sweep has no idle time to measure on
                such a row, and sleeping it is destructive rather than thrifty —
                an agent publishes its session id at startup but writes no
                transcript until the first prompt, so waking one resumes a
                conversation that was never recorded and the terminal is lost.
                The decision lives here, next to the keep-awake decline, so every
                client's sweep gets the same answer. A user-triggered Sleep is
                never declined.

                CDXC:DraftSessions 2026-08-28:
                A DRAFT is the one never-active session that IS safe to sleep,
                and the one it matters most for: it is by definition unprompted,
                so it would otherwise be the only session class that can never
                auto-sleep at all, and a user who opens several drafts would
                leave that many agent CLIs running forever. There is nothing to
                lose by killing its provider because there is no conversation —
                waking one relaunches the agent fresh from its stored launch
                plan (see `get_provider_restart_startup_text_for_session`)
                rather than resuming anything.
                */
                let session = require_session(repository, &lifecycle)?;
                if !session_has_ever_been_active(&session)
                    && !crate::agents::session_is_draft(&session)
                {
                    return Ok(ZmxEndpointOutput {
                        created_workspace_terminal: None,
                        result: json!({ "declined": "neverActive", "session": session }),
                        presentation_session: None,
                    });
                }
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

/*
CDXC:SessionChatSerializedWriters 2026-08-24:
One `/api/sendSessionText` or `/api/sendSessionEnter` call, resolved against the
repository BEFORE anything is awaited — a rusqlite handle cannot be held across
an await point, and the awaited variant needs the queue's answer.
*/
pub struct ZmxQueuedSessionWrite {
    pub project_id: String,
    pub session_id: String,
    pub zmx_name: String,
    pub session: Value,
    /// `diagnosticInputSource`, carried through so the temporary input log keeps
    /// attributing the write to its caller instead of to the queue.
    pub source: String,
    pub text: String,
}

pub fn read_zmx_queued_session_write(
    repository: &DomainRepository<'_>,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> ZmxEndpointResult<ZmxQueuedSessionWrite> {
    require_zmx()?;
    let lifecycle = read_lifecycle_params(params)?;
    let session = require_session(repository, &lifecycle)?;
    let zmx_name = provider_zmx_session_name(&session)?;
    let text = if endpoint_path == "/api/sendSessionEnter" {
        "\r".to_string()
    } else {
        read_interaction_text(params.get("text"), "sendSessionText")?
    };
    Ok(ZmxQueuedSessionWrite {
        project_id: lifecycle.project_id,
        session_id: lifecycle.session_id,
        zmx_name,
        session,
        source: read_diagnostic_input_source(params).to_string(),
        text,
    })
}

impl ZmxQueuedSessionWrite {
    fn steps(&self) -> Vec<crate::session_chat_send::SessionChatSendStep> {
        vec![crate::session_chat_send::SessionChatSendStep::Write(
            self.text.clone(),
        )]
    }

    /// The unchanged `sendSession*` response shape. `exitCode` is 0 because the
    /// queue reports a refused burst as an error rather than as a zmx status.
    fn into_result(self) -> Value {
        send_result(0, self.session, &self.text, false, self.zmx_name)
    }

    /// Awaited delivery, used by the HTTP layer.
    pub async fn execute(self) -> ZmxEndpointResult<Value> {
        let steps = self.steps();
        crate::session_chat_send::execute_session_chat_send(
            &self.project_id,
            &self.session_id,
            &self.zmx_name,
            &self.source,
            steps,
        )
        .await
        .map_err(|error| ZmxEndpointError::DependencyUnavailable(error.message))?;
        Ok(self.into_result())
    }

    /// Fire-and-forget enqueue, used by the synchronous in-process dispatch.
    pub fn enqueue(self) -> Value {
        crate::session_chat_send::enqueue_session_chat_send(
            &self.project_id,
            &self.session_id,
            &self.zmx_name,
            &self.source,
            self.steps(),
        );
        self.into_result()
    }
}

/*
CDXC:SessionChatSerializedWriters 2026-08-26:
`/api/sendSessionMessage` — the automation prompt, the worktree first prompt,
the fork rename, the remote rename, and `gx sendMessage` — is the last writer of
the session's TUI input line that bypassed the per-session queue. It wrote its
text straight to the pty and slept a whole HTTP worker thread before the Enter,
so its bytes could land inside another sequence (and its own text/Enter pair
could be split by somebody else's job). It rides the queue now, as ONE job whose
settle is a queue step rather than a `thread::sleep`.

It also opens with the measured clear burst, as its own write plus the settle:
this endpoint types a whole message and submits it, so a draft on that line was
previously prepended to the message and submitted with it. The API shape is
unchanged — `submit`, `sendDelayMs` (same default and clamp), and the same
result object.
*/
pub struct ZmxQueuedSessionMessage {
    pub project_id: String,
    pub session_id: String,
    pub zmx_name: String,
    pub session: Value,
    /// `diagnosticInputSource`, carried through so every write of this job is
    /// still attributed to its caller instead of to the queue.
    pub source: String,
    pub text: String,
    pub submit: bool,
    pub send_delay_ms: u64,
}

/// Callers that widen the settle window past this are clamped, so a bad value
/// cannot park the session's queue.
const SEND_SESSION_MESSAGE_SUBMIT_DELAY_DEFAULT_MS: u64 = 150;
const SEND_SESSION_MESSAGE_SUBMIT_DELAY_MAX_MS: u64 = 2_000;

impl ZmxQueuedSessionMessage {
    fn read(
        lifecycle: &LifecycleParams,
        session: Value,
        zmx_name: String,
        source: &str,
        params: &Map<String, Value>,
    ) -> ZmxEndpointResult<Self> {
        Ok(Self {
            project_id: lifecycle.project_id.clone(),
            session_id: lifecycle.session_id.clone(),
            zmx_name,
            session,
            source: source.to_string(),
            text: read_interaction_text(params.get("text"), "sendSessionMessage")?,
            submit: params.get("submit").and_then(Value::as_bool) != Some(false),
            send_delay_ms: params
                .get("sendDelayMs")
                .and_then(Value::as_u64)
                .unwrap_or(SEND_SESSION_MESSAGE_SUBMIT_DELAY_DEFAULT_MS)
                .min(SEND_SESSION_MESSAGE_SUBMIT_DELAY_MAX_MS),
        })
    }

    /*
    CDXC:GxserverSendMessageSubmit 2026-07-04-17:02:
    Submit must be a separate zmx send after a short settle delay, never a
    trailing \r inside the same stdin burst. Bracketed-paste TUIs (Claude Code
    and similar composers) treat a \r that arrives in the same paste burst as
    newline text, leaving the message staged in the composer instead of
    submitted. The clear burst obeys the same rule for the same reason.
    */
    fn steps(&self) -> Vec<crate::session_chat_send::SessionChatSendStep> {
        let mut steps =
            crate::session_chat_send::build_agent_tui_clear_input_steps(None, &self.text);
        steps.push(crate::session_chat_send::SessionChatSendStep::Write(
            if self.submit {
                crate::session_chat_send::disambiguate_agent_tui_submit_text(&self.text)
            } else {
                self.text.clone()
            },
        ));
        if self.submit {
            if self.send_delay_ms > 0 {
                steps.push(crate::session_chat_send::SessionChatSendStep::SleepMs(
                    self.send_delay_ms,
                ));
            }
            steps.push(crate::session_chat_send::SessionChatSendStep::Write(
                crate::session_chat_send::SESSION_CHAT_SUBMIT.to_string(),
            ));
        }
        steps
    }

    /// The unchanged `sendSessionMessage` response shape. `exitCode` is 0
    /// because the queue reports a refused burst as an error rather than as a
    /// zmx status.
    fn into_result(self) -> Value {
        let submit = self.submit;
        let mut value = send_result(0, self.session, &self.text, false, self.zmx_name);
        value
            .as_object_mut()
            .expect("send result object")
            .insert("submit".to_string(), Value::Bool(submit));
        value
    }

    /// Awaited delivery, used by the HTTP layer so its callers still learn that
    /// the terminal refused the message.
    pub async fn execute(self) -> ZmxEndpointResult<Value> {
        let steps = self.steps();
        crate::session_chat_send::execute_session_chat_send(
            &self.project_id,
            &self.session_id,
            &self.zmx_name,
            &self.source,
            steps,
        )
        .await
        .map_err(|error| ZmxEndpointError::DependencyUnavailable(error.message))?;
        self.capture_prompt_sent();
        Ok(self.into_result())
    }

    /// Fire-and-forget enqueue, used by the synchronous in-process dispatch.
    pub fn enqueue(self) -> Value {
        crate::session_chat_send::enqueue_session_chat_send(
            &self.project_id,
            &self.session_id,
            &self.zmx_name,
            &self.source,
            self.steps(),
        );
        self.capture_prompt_sent();
        self.into_result()
    }

    /*
    CDXC:AnonymousAnalytics 2026-08-26:
    `/api/sendSessionMessage` carries two very different things: real user
    prompts (automation runs, the worktree first prompt, `gx sendMessage`) and
    gxserver's own non-prompt writes (auto-title generation, the fork rename,
    the remote rename). `diagnosticInputSource` already distinguishes them —
    every caller tags itself — so the mapping is an explicit ALLOW table:
    unrecognised tags emit nothing rather than being guessed at. Counting a
    rename as a prompt would inflate every per-user prompt metric.
    */
    fn capture_prompt_sent(&self) {
        let Some(prompt_source) =
            crate::telemetry::prompt_source_for_diagnostic_input_source(&self.source)
        else {
            return;
        };
        crate::telemetry::prompt_sent(
            self.session.get("agentId").and_then(Value::as_str),
            prompt_source,
        );
    }
}

pub fn read_zmx_queued_session_message(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> ZmxEndpointResult<ZmxQueuedSessionMessage> {
    require_zmx()?;
    let lifecycle = read_lifecycle_params(params)?;
    let session = require_session(repository, &lifecycle)?;
    let zmx_name = provider_zmx_session_name(&session)?;
    ZmxQueuedSessionMessage::read(
        &lifecycle,
        session,
        zmx_name,
        read_diagnostic_input_source(params),
        params,
    )
}

fn read_diagnostic_input_source(params: &Map<String, Value>) -> &str {
    params
        .get("diagnosticInputSource")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("external-api")
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
    let diagnostic_source = read_diagnostic_input_source(params);
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
        /*
        CDXC:SessionChatSerializedWriters 2026-08-24: these two write raw bytes
        into the session's TUI input line, which is the same line an in-flight
        Session Chat send owns from its clear burst until its Enter, so they go
        through that per-session queue instead of straight to the pty. This
        synchronous dispatch enqueues without waiting; the HTTP layer takes the
        awaited path below so its callers still learn that the terminal refused
        the bytes.
        */
        "/api/sendSessionText" | "/api/sendSessionEnter" => {
            let text = if endpoint_path == "/api/sendSessionEnter" {
                "\r".to_string()
            } else {
                read_interaction_text(params.get("text"), "sendSessionText")?
            };
            Ok(ZmxQueuedSessionWrite {
                project_id: lifecycle.project_id.clone(),
                session_id: lifecycle.session_id.clone(),
                zmx_name,
                session,
                source: diagnostic_source.to_string(),
                text,
            }
            .enqueue())
        }
        "/api/sendSessionMessage" => Ok(ZmxQueuedSessionMessage::read(
            &lifecycle,
            session,
            zmx_name,
            diagnostic_source,
            params,
        )?
        .enqueue()),
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

pub(crate) fn send_result(
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
