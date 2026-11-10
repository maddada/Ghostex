use super::*;

fn session_chat_frame(
    config: &SessionChatFollowerConfig,
    frame_type: &str,
    epoch: i64,
    seq: i64,
) -> Map<String, Value> {
    let mut frame = Map::new();
    frame.insert("type".to_string(), json!(frame_type));
    frame.insert("projectId".to_string(), json!(config.project_id));
    frame.insert("sessionId".to_string(), json!(config.session_id));
    frame.insert("epoch".to_string(), json!(epoch));
    frame.insert("seq".to_string(), json!(seq));
    frame.insert(
        "protocolVersion".to_string(),
        json!(config.protocol_version),
    );
    frame.insert("serverId".to_string(), json!(config.server_id));
    frame
}

fn insert_optional_lifecycle(
    frame: &mut Map<String, Value>,
    lifecycle: Option<&SessionChatTurnLifecycle>,
) {
    if let Some(lifecycle) = lifecycle {
        if let Ok(value) = serde_json::to_value(lifecycle) {
            frame.insert("lifecycle".to_string(), value);
        }
    }
}

fn insert_optional_agent_session_id(
    frame: &mut Map<String, Value>,
    config: &SessionChatFollowerConfig,
) {
    if let Some(agent_session_id) = config
        .agent_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        frame.insert("agentSessionId".to_string(), json!(agent_session_id));
    }
}

fn insert_optional_prompt(
    frame: &mut Map<String, Value>,
    prompt: Option<&SessionChatInteractivePrompt>,
) {
    if let Some(prompt) = prompt {
        if let Ok(value) = serde_json::to_value(prompt) {
            frame.insert("prompt".to_string(), value);
        }
    }
}

/// Detected model/effort. Absent ⇒ the field is omitted ⇒ clients keep their
/// own truth (older daemons behave the same way).
pub(crate) fn insert_optional_selected_options(
    frame: &mut Map<String, Value>,
    selected_options: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
) {
    if let Some(selected_options) = selected_options {
        frame.insert("selectedOptions".to_string(), selected_options.to_value());
    }
}

/*
CDXC:AgentScreenDetection 2026-08-19:
Terminal-state notice (login expired, trust dialog, usage limit, undelivered
send). Absent ⇒ the field is omitted ⇒ clients CLEAR the card, exactly like
`prompt` and unlike `selectedOptions`. Every frame that can carry it must
therefore carry the CURRENT value, never `None` as a shorthand for "unchanged".
*/
fn insert_optional_terminal_notice(
    frame: &mut Map<String, Value>,
    terminal_notice: Option<&crate::session_chat_notice::SessionChatTerminalNotice>,
) {
    if let Some(terminal_notice) = terminal_notice {
        frame.insert("terminalNotice".to_string(), terminal_notice.to_value());
    }
}

/*
CDXC:AgentScreenDetection 2026-08-22:
Everything one terminal capture tells a client, travelling as ONE value. The
notice card and the transcript's activity row are read from the same screen and
are always restated together — carrying them as two parallel parameters through
four frame builders is how they would eventually drift out of step, with a
stale progress row surviving a frame that cleared its notice (or the reverse).
Both halves keep `prompt` semantics: absent ⇒ the field is omitted ⇒ the client
CLEARS it, so every producer restates the CURRENT value.
*/
#[derive(Clone, Copy, Debug, Default)]
pub struct SessionChatScreenState<'a> {
    pub prompt: Option<&'a SessionChatInteractivePrompt>,
    pub notice: Option<&'a crate::session_chat_notice::SessionChatTerminalNotice>,
    pub activity: Option<&'a crate::session_chat_terminal_activity::SessionChatTerminalActivity>,
    /*
    CDXC:AgentScreenDetection 2026-08-23: the sub-agents the screen is
    painting right now. Rides here for the same reason the activity row does —
    one capture, one value, so the fleet strip can never survive a frame that
    cleared the progress row it was read beside.
    */
    pub fleet: Option<&'a crate::session_chat_agent_fleet::SessionChatAgentFleet>,
    /*
    CDXC:SessionChat 2026-09-03: Claude's task list from its on-disk
    store. Not a screen reading, but it travels with them because every
    producer of a state frame restates this whole value, and the panel needs
    the same omitted ⇒ CLEARED rule (the store is deleted with the session).
    */
    pub tasks: Option<&'a crate::session_chat_agent_tasks::SessionChatAgentTasks>,
    /// CDXC:SessionChat 2026-10-22: Claude's Workflow runs, disk-backed like `tasks` and restated
    /// by every producer the same way (omitted means cleared).
    pub workflows: Option<&'a crate::session_chat_claude_workflow_cards::SessionChatWorkflows>,
    /*
    CDXC:AgentScreenDetection 2026-08-22:
    True once a WHOLE screen capture has actually been read for this session.

    Every other screen-derived field omits itself when it has nothing to say,
    which leaves a client unable to tell "the model is still being detected"
    from "detection ran and this agent's screen names no model". The composer
    needs exactly that distinction to decide between a loading skeleton and a
    plain unset pill — a stopped or sleeping session has no screen at all and
    must never sit under a spinner waiting for a value that is not coming.

    Same rule as `captured` in SessionChatTerminalDetection, which is where
    this comes from: only a capture that succeeded whole counts.
    */
    pub probed: bool,
    /// The agent is gone from the session with no notice saying so
    /// (`session_chat_start_failure::agent_left_conversation`). Written only when true, so an
    /// omitted `agentGone` means it runs, like the rest of this value.
    pub agent_gone: bool,
}

pub(crate) fn insert_screen_state(
    frame: &mut Map<String, Value>,
    screen: SessionChatScreenState<'_>,
) {
    insert_optional_terminal_notice(frame, screen.notice);
    if let Some(activity) = screen.activity {
        frame.insert("terminalActivity".to_string(), activity.to_value());
    }
    if let Some(fleet) = screen.fleet {
        frame.insert("agentFleet".to_string(), fleet.to_value());
    }
    if let Some(tasks) = screen.tasks {
        frame.insert("agentTasks".to_string(), tasks.to_value());
    }
    if let Some(workflows) = screen.workflows {
        frame.insert("workflows".to_string(), workflows.to_value());
    }
    if screen.probed {
        frame.insert("screenProbed".to_string(), Value::Bool(true));
    }
    if screen.agent_gone {
        frame.insert("agentGone".to_string(), Value::Bool(true));
    }
}

/*
CDXC:SessionChat 2026-08-21:
Queue + draft ride snapshot / replaced / state frames only. `queue` is written
even when empty — present is the daemon capability probe — while `draft` is
written only when the server actually holds one, because an omitted draft means
UNCHANGED and never "cleared". Read only when a frame that carries it is
actually being emitted, never on the reconcile tick.

CDXC:AgentScreenDetection 2026-08-24: the READ moved out of the frame
builders. `emit_sequenced`'s build closure runs under the stream's emit-order
mutex and must not block, but the reader opens the state database — SQLite busy
contention there held the mutex every other publisher (hook ingest, options
detection) has to take. The snapshot is taken first and only inserted here.
*/
fn insert_optional_queue(
    frame: &mut Map<String, Value>,
    queue: Option<&crate::session_chat_queue::SessionChatQueueSnapshot>,
) {
    let Some(queue) = queue else {
        return;
    };
    queue.insert_into(frame);
}

/// Reads the queue snapshot a state/snapshot frame will carry, outside the
/// emit lock. Absent reader ⇒ absent fields (the client's "no queue" probe).
fn read_optional_queue(
    config: &SessionChatFollowerConfig,
) -> Option<crate::session_chat_queue::SessionChatQueueSnapshot> {
    config.queue_reader.as_ref().map(|reader| reader())
}

/*
CDXC:SessionChat 2026-08-23:
Rides on `config` rather than on a new parameter through four frame builders:
the rows live in a process-global store keyed by exactly the ids the config
already carries, and unlike the screen state they are not read from a capture
this frame took, so bundling them into SessionChatScreenState would tie a
value with its own lifetime to one that must never survive a frame that
cleared it.
*/
fn insert_optional_app_commands(
    frame: &mut Map<String, Value>,
    config: &SessionChatFollowerConfig,
) {
    crate::session_chat_app_command::insert_session_chat_app_commands(
        frame,
        &config.project_id,
        &config.session_id,
    );
}

/// The prompt Claude handed back to its composer, for the client to put back
/// into its own. Same process-global store shape as the app commands above.
fn insert_optional_returned_prompt(
    frame: &mut Map<String, Value>,
    config: &SessionChatFollowerConfig,
) {
    crate::session_chat_returned_prompt::insert_session_chat_returned_prompt(
        frame,
        &config.project_id,
        &config.session_id,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_state_frame(
    emit: &SessionChatFrameEmitter,
    config: &SessionChatFollowerConfig,
    stream: &SessionChatStream,
    epoch: i64,
    status: SessionChatStatus,
    prompt: Option<&SessionChatInteractivePrompt>,
    working: Option<bool>,
    selected_options: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
    screen: SessionChatScreenState<'_>,
) {
    let queue = read_optional_queue(config);
    stream.emit_sequenced(
        |seq| {
            let mut frame = session_chat_frame(config, "sessionChatState", epoch, seq);
            frame.insert("status".to_string(), json!(status.as_str()));
            insert_optional_prompt(&mut frame, prompt.or(screen.prompt));
            if let Some(working) = working {
                frame.insert("working".to_string(), json!(working));
            }
            insert_optional_selected_options(&mut frame, selected_options);
            insert_screen_state(&mut frame, screen);
            insert_optional_queue(&mut frame, queue.as_ref());
            insert_optional_app_commands(&mut frame, config);
            insert_optional_returned_prompt(&mut frame, config);
            crate::coordinators::insert_coordinator_threads(
                &mut frame,
                &config.project_id,
                &config.session_id,
            );
            crate::side_chats::insert_side_chats(
                &mut frame,
                &config.project_id,
                &config.session_id,
            );
            crate::secret_requests::insert_secret_requests(
                &mut frame,
                &config.project_id,
                &config.session_id,
            );
            crate::coordinators::insert_relayed_answers(
                &mut frame,
                &config.project_id,
                &config.session_id,
            );
            crate::autopilot::insert_chat_autopilot(
                &mut frame,
                &config.project_id,
                &config.session_id,
            );
            crate::collab_shim::insert_chat_collab(
                &mut frame,
                &config.project_id,
                &config.session_id,
            );
            crate::agent_message_deliveries::insert_agent_message_deliveries(
                &mut frame,
                &config.project_id,
                &config.session_id,
                crate::agent_message_deliveries::DeliveriesFrame::State,
            );
            insert_optional_agent_session_id(&mut frame, config);
            Value::Object(frame)
        },
        |frame| emit(frame),
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_snapshot_frame(
    emit: &SessionChatFrameEmitter,
    config: &SessionChatFollowerConfig,
    stream: &SessionChatStream,
    epoch: i64,
    frame_type: &str,
    tail: &SessionChatTailFileResult,
    prompt: Option<&SessionChatInteractivePrompt>,
    working: bool,
    selected_options: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
    screen: SessionChatScreenState<'_>,
) {
    let queue = read_optional_queue(config);
    /*
    CDXC:SessionChat 2026-09-27 WHY:
    The slash commands the user sent from chat, and the side answers stored as `/btw` outputs, are replayed from gxserver's archive into every read, but the socket's snapshot carried only the transcript, so the desktop and web chats lost them once the live row expired and never got them back on reload. The snapshot now merges the archive exactly as the live-tail read does.
    */
    let archived = crate::session_chat_local_command::load_session_chat_local_commands(
        &config.project_id,
        &config.session_id,
    );
    let local_commands = crate::session_chat_local_command::select_session_chat_local_commands(
        archived,
        &tail.messages,
        true,
        tail.has_more,
    );
    let messages = crate::session_chat_local_command::merge_session_chat_local_commands(
        tail.messages.clone(),
        &local_commands,
    );
    let mut rows =
        crate::session_chat_history::collapse_snapshot_messages(&messages, tail.before_offset);
    crate::session_chat_mention_paths::annotate_messages(
        &mut rows,
        &config.project_id,
        &config.session_id,
    );
    stream.emit_sequenced(
        |seq| {
            let mut frame = session_chat_frame(config, frame_type, epoch, seq);
            frame.insert("messages".to_string(), Value::Array(rows));
            insert_optional_lifecycle(&mut frame, tail.lifecycle.as_ref());
            frame.insert("hasMore".to_string(), json!(tail.has_more));
            frame.insert("hasMoreExact".to_string(), json!(true));
            frame.insert("beforeOffset".to_string(), json!(tail.before_offset));
            let status = if messages.is_empty() {
                SessionChatStatus::Empty
            } else {
                SessionChatStatus::Ready
            };
            frame.insert("status".to_string(), json!(status.as_str()));
            frame.insert("working".to_string(), json!(working));
            // CDXC:AgentProviders 2026-09-07 WHY:
            // The account submenu needs the provider even when the live snapshot arrives before the initial read, or that read fails. An authoritative snapshot must carry its own agent family.
            if let Some(agent) = config.agent.as_deref() {
                frame.insert("agent".to_string(), json!(agent));
            }
            insert_optional_prompt(&mut frame, prompt.or(screen.prompt));
            insert_optional_selected_options(&mut frame, selected_options);
            insert_screen_state(&mut frame, screen);
            insert_optional_queue(&mut frame, queue.as_ref());
            insert_optional_app_commands(&mut frame, config);
            insert_optional_returned_prompt(&mut frame, config);
            crate::coordinators::insert_coordinator_threads(
                &mut frame,
                &config.project_id,
                &config.session_id,
            );
            crate::side_chats::insert_side_chats(
                &mut frame,
                &config.project_id,
                &config.session_id,
            );
            crate::secret_requests::insert_secret_requests(
                &mut frame,
                &config.project_id,
                &config.session_id,
            );
            crate::coordinators::insert_relayed_answers(
                &mut frame,
                &config.project_id,
                &config.session_id,
            );
            crate::autopilot::insert_chat_autopilot(
                &mut frame,
                &config.project_id,
                &config.session_id,
            );
            crate::collab_shim::insert_chat_collab(
                &mut frame,
                &config.project_id,
                &config.session_id,
            );
            crate::agent_message_deliveries::insert_agent_message_deliveries(
                &mut frame,
                &config.project_id,
                &config.session_id,
                crate::agent_message_deliveries::DeliveriesFrame::Whole,
            );
            insert_optional_agent_session_id(&mut frame, config);
            Value::Object(frame)
        },
        |frame| emit(frame),
    );
}

pub(super) fn emit_appended_frame(
    emit: &SessionChatFrameEmitter,
    config: &SessionChatFollowerConfig,
    stream: &SessionChatStream,
    epoch: i64,
    messages: &[SessionChatMessage],
    lifecycle: Option<&SessionChatTurnLifecycle>,
    superseded_message_ids: &[String],
) {
    let mut rows = match serde_json::to_value(messages) {
        Ok(Value::Array(rows)) => rows,
        _ => Vec::new(),
    };
    crate::session_chat_mention_paths::annotate_messages(
        &mut rows,
        &config.project_id,
        &config.session_id,
    );
    stream.emit_sequenced(
        |seq| {
            let mut frame = session_chat_frame(config, "sessionChatAppended", epoch, seq);
            frame.insert("messages".to_string(), Value::Array(rows));
            // Omitted when empty: daemons and clients that predate the field
            // then behave exactly as before.
            if !superseded_message_ids.is_empty() {
                frame.insert(
                    "supersededMessageIds".to_string(),
                    json!(superseded_message_ids),
                );
            }
            insert_optional_lifecycle(&mut frame, lifecycle);
            Value::Object(frame)
        },
        |frame| emit(frame),
    );
}
