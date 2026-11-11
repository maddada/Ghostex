//! `/api/*Coordinator*` endpoints: read, list, update, link and resolve.

use chrono::Utc;
use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::records::{
    acknowledge_thread_pending_message, drop_thread_pending_message, link_thread,
    list_coordinators, list_threads_for, now_iso, read_coordinator, read_thread,
    set_thread_observed_working, set_thread_pending_message, set_thread_resolved, unlink_thread,
    write_coordinator, CoordinatorMemoryNote, CoordinatorRecord, SessionKey, ThreadRecord,
    COORDINATOR_GOAL_MAX_CHARS, COORDINATOR_INSTRUCTIONS_MAX_CHARS, COORDINATOR_MEMORY_MAX_NOTES,
    COORDINATOR_MEMORY_NOTE_MAX_CHARS,
};
use super::state::{ThreadProgress, ThreadState};
use crate::domain::{DomainRepository, DomainStateError};
use crate::ids::create_global_session_ref;
use crate::presentation::{
    effective_lifecycle_state, presentation_activity, project_session_title,
};

/// What an endpoint answered, plus the sessions whose presentation changed.
pub struct CoordinatorEndpointOutput {
    pub result: Value,
    pub changed_sessions: Vec<SessionKey>,
}

pub const COORDINATOR_ENDPOINTS: &[&str] = &[
    "/api/readCoordinator",
    "/api/readCoordinatorThreads",
    "/api/listCoordinators",
    "/api/updateCoordinator",
    "/api/linkCoordinatorThread",
    "/api/unlinkCoordinatorThread",
    "/api/setCoordinatorThreadResolved",
    "/api/renameCoordinatorThread",
    "/api/coordinatorThreadTitleReminder",
    "/api/checkAgentSessionWrite",
    "/api/coordinatorBoard",
    "/api/coordinatorWork",
    "/api/setMainCoordinator",
];

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn required_ids(
    params: &Map<String, Value>,
    project_key: &str,
    session_key: &str,
) -> Result<SessionKey, DomainStateError> {
    match (text(params, project_key), text(params, session_key)) {
        (Some(project_id), Some(session_id)) => Ok((project_id, session_id)),
        _ => Err(DomainStateError::bad_request(format!(
            "This coordinator request needs {project_key} and {session_key}."
        ))),
    }
}

pub fn handle_coordinator_endpoint(
    endpoint_path: &str,
    db: &Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<CoordinatorEndpointOutput, DomainStateError> {
    match endpoint_path {
        "/api/readCoordinator" => {
            let key = required_ids(params, "projectId", "sessionId")?;
            let coordinator = resolve_coordinator(db, &key)?;
            Ok(CoordinatorEndpointOutput {
                result: coordinator_view(db, repository, &coordinator)?,
                changed_sessions: Vec::new(),
            })
        }
        // The board: read it, or add, move, note, mark done or drop an item (board.rs).
        "/api/coordinatorBoard" => {
            let key = required_ids(params, "projectId", "sessionId")?;
            resolve_coordinator(db, &key)?;
            // "Don't show again" on the coordinator tips, for every coordinator (intro.rs).
            if params.get("op").and_then(Value::as_str) == Some("hideIntro") {
                super::intro::hide_intro(db)?;
                return Ok(CoordinatorEndpointOutput {
                    result: json!({ "introHidden": true }),
                    changed_sessions: Vec::new(),
                });
            }
            let threads = super::panel::cached_thread_rows(&key);
            let (result, _) = super::board::handle_board_request(db, &key, &threads, params)?;
            Ok(CoordinatorEndpointOutput {
                result,
                changed_sessions: Vec::new(),
            })
        }
        // The work items: list them, or add, update, ask, wait on, finish or drop one (work.rs).
        "/api/coordinatorWork" => {
            let key = required_ids(params, "projectId", "sessionId")?;
            resolve_coordinator(db, &key)?;
            let threads = super::panel::cached_thread_rows(&key);
            let ask_floor =
                crate::session_chat_open_questions::highest_question_tag(&key.0, &key.1);
            let (result, _) =
                super::work::handle_work_request(db, &key, &threads, params, ask_floor)?;
            Ok(CoordinatorEndpointOutput {
                result,
                changed_sessions: Vec::new(),
            })
        }
        "/api/readCoordinatorThreads" => Ok(CoordinatorEndpointOutput {
            result: super::panel::read_coordinator_threads(&required_ids(
                params,
                "projectId",
                "sessionId",
            )?),
            changed_sessions: Vec::new(),
        }),
        "/api/listCoordinators" => {
            let project_id = text(params, "projectId");
            // The resolved threads from memory unless one changed (thread_reads.rs).
            let all_threads = super::read_all_threads(db)?;
            let threads = all_threads.ordered();
            let mains = super::main_coordinator::main_coordinators(db, repository);
            let mut rows = Vec::new();
            for coordinator in list_coordinators(db)? {
                if project_id
                    .as_ref()
                    .is_some_and(|project_id| *project_id != coordinator.project_id)
                {
                    continue;
                }
                let Some(session) =
                    repository.get_session(&coordinator.project_id, &coordinator.session_id)?
                else {
                    continue;
                };
                let mut counts = Map::new();
                for thread in threads
                    .iter()
                    .filter(|thread| thread.coordinator_key() == coordinator.key())
                {
                    let thread_session =
                        repository.get_session(&thread.project_id, &thread.session_id)?;
                    let state = super::live::classify_thread_live(
                        thread_session.as_ref(),
                        ThreadProgress::of(thread),
                        &now_iso(),
                    );
                    let count = counts.entry(state.as_str().to_string()).or_insert(json!(0));
                    *count = json!(count.as_u64().unwrap_or(0) + 1);
                }
                rows.push(json!({
                    "globalRef": create_global_session_ref(repository.server_id.as_str(), &coordinator.project_id, &coordinator.session_id),
                    "projectId": coordinator.project_id,
                    "sessionId": coordinator.session_id,
                    "title": session_title_of(&session),
                    "goal": coordinator.goal,
                    "lifecycleState": effective_lifecycle_state(&session),
                    "threadCounts": counts,
                    // Its project's main coordinator on this computer (main_coordinator.rs). An
                    // older gxserver leaves the field out, which the CLI reads as "needs an update".
                    "isMainCoordinator": mains.get(&coordinator.project_id) == Some(&coordinator.session_id),
                }));
            }
            Ok(CoordinatorEndpointOutput {
                result: json!({ "coordinators": rows }),
                changed_sessions: Vec::new(),
            })
        }
        // Right-click > Make main coordinator (desktop, web build, phone).
        "/api/setMainCoordinator" => {
            let key = required_ids(params, "projectId", "sessionId")?;
            let changed = super::main_coordinator::set_main_coordinator(db, repository, &key)?;
            Ok(CoordinatorEndpointOutput {
                result: json!({
                    "ok": true,
                    "projectId": key.0,
                    "sessionId": key.1,
                    "globalRef": create_global_session_ref(repository.server_id.as_str(), &key.0, &key.1),
                }),
                changed_sessions: changed,
            })
        }
        "/api/updateCoordinator" => {
            let key = required_ids(params, "projectId", "sessionId")?;
            let mut coordinator = resolve_coordinator(db, &key)?;
            apply_update(&mut coordinator, params)?;
            write_coordinator(db, &coordinator)?;
            Ok(CoordinatorEndpointOutput {
                result: coordinator_view(db, repository, &coordinator)?,
                changed_sessions: vec![coordinator.key()],
            })
        }
        "/api/linkCoordinatorThread" => {
            let coordinator_key =
                required_ids(params, "coordinatorProjectId", "coordinatorSessionId")?;
            let thread_key = required_ids(params, "projectId", "sessionId")?;
            let only_if_coordinator =
                params.get("onlyIfCoordinator").and_then(Value::as_bool) == Some(true);
            // A message from a coordinator to its own done thread reopens it, so the reply is
            // supervised and reported again; any other recipient is left alone. A reply the thread
            // finished while it was done goes back to the sender in this answer: the message has
            // just started a new turn, whose report would replace it. gxserver's send route already
            // reopens on the send itself (reopen.rs); this call is the `--queue` path's reopen and
            // an older CLI's, guarded by the time the CLI took before its send (`sentAtMs`), and a
            // confirmation (`messageDelivered`) only acknowledges the delivery.
            if params.get("reopenOnly").and_then(Value::as_bool) == Some(true) {
                let thread = read_thread(db, &thread_key.0, &thread_key.1)?
                    .filter(|thread| thread.coordinator_key() == coordinator_key);
                if params.get("messageDelivered").and_then(Value::as_bool) == Some(true) {
                    if let (Some(thread), Some(send_id)) =
                        (thread.as_ref(), text(params, "sendRequestId"))
                    {
                        acknowledge_thread_pending_message(
                            db,
                            &thread.project_id,
                            &thread.session_id,
                            &send_id,
                        )?;
                    }
                    return Ok(CoordinatorEndpointOutput {
                        result: json!({ "linked": false, "reopened": false, "heldReport": null, "watchingDelivery": false }),
                        changed_sessions: Vec::new(),
                    });
                }
                let send_ms = params
                    .get("sentAtMs")
                    .and_then(Value::as_i64)
                    .filter(|sent| *sent <= Utc::now().timestamp_millis());
                let plan = match (thread.as_ref(), send_ms) {
                    (Some(_), Some(send_ms)) => {
                        super::reopen::prepare_reopen(db, repository, &thread_key, send_ms, |_| {
                            true
                        })?
                    }
                    _ => None,
                };
                let (reopened, held_report) = match plan {
                    Some(plan) => {
                        if super::reopen::apply_reopen(db, repository, &plan, None)? {
                            (true, plan.held_report)
                        } else {
                            (false, None)
                        }
                    }
                    None => (false, None),
                };
                let watching = thread.is_some() && watch_pending_message(db, &thread_key, params)?;
                return Ok(CoordinatorEndpointOutput {
                    result: json!({ "linked": false, "reopened": reopened, "heldReport": held_report, "watchingDelivery": watching }),
                    changed_sessions: if reopened {
                        vec![thread_key]
                    } else {
                        Vec::new()
                    },
                });
            }
            if read_coordinator(db, &coordinator_key.0, &coordinator_key.1)?.is_none() {
                if only_if_coordinator {
                    return Ok(CoordinatorEndpointOutput {
                        result: json!({ "linked": false }),
                        changed_sessions: Vec::new(),
                    });
                }
                return Err(DomainStateError::not_found(
                    "That session is not a coordinator.",
                ));
            }
            if coordinator_key == thread_key {
                return Err(DomainStateError::bad_request(
                    "A coordinator cannot be its own thread.",
                ));
            }
            if repository
                .get_session(&thread_key.0, &thread_key.1)?
                .is_none()
            {
                return Err(DomainStateError::not_found(
                    "The thread session does not exist.",
                ));
            }
            let task = text(params, "task").unwrap_or_default();
            link_thread(
                db,
                &thread_key.0,
                &thread_key.1,
                &coordinator_key.0,
                &coordinator_key.1,
                &task,
            )?;
            let watching = watch_pending_message(db, &thread_key, params)?;
            Ok(CoordinatorEndpointOutput {
                result: json!({
                    "linked": true,
                    "watchingDelivery": watching,
                    "globalRef": create_global_session_ref(repository.server_id.as_str(), &thread_key.0, &thread_key.1),
                }),
                changed_sessions: vec![thread_key],
            })
        }
        "/api/unlinkCoordinatorThread" => {
            let coordinator_key =
                required_ids(params, "coordinatorProjectId", "coordinatorSessionId")?;
            let thread_key = required_ids(params, "projectId", "sessionId")?;
            if read_thread(db, &thread_key.0, &thread_key.1)?
                .filter(|thread| thread.coordinator_key() == coordinator_key)
                .is_none()
            {
                return Err(DomainStateError::not_found(
                    "That session is not a thread of this coordinator, so it was not unlinked.",
                ));
            }
            unlink_thread(db, &thread_key.0, &thread_key.1)?;
            Ok(CoordinatorEndpointOutput {
                result: json!({
                    "unlinked": true,
                    "globalRef": create_global_session_ref(repository.server_id.as_str(), &thread_key.0, &thread_key.1),
                }),
                changed_sessions: vec![thread_key],
            })
        }
        "/api/setCoordinatorThreadResolved" => {
            let thread_key = required_ids(params, "projectId", "sessionId")?;
            let resolved = params
                .get("resolved")
                .and_then(Value::as_bool)
                .ok_or_else(|| DomainStateError::bad_request("Say resolved: true or false."))?;
            let Some(thread) = read_thread(db, &thread_key.0, &thread_key.1)? else {
                return Err(DomainStateError::not_found(
                    "That session is not a coordinator thread.",
                ));
            };
            let close_session =
                resolved && params.get("closeSession").and_then(Value::as_bool) == Some(true);
            if close_session {
                refuse_closing_unfinished(repository, &thread)?;
            }
            let held = (!resolved && thread.is_resolved())
                .then(|| held_reply(repository, &thread))
                .flatten();
            let changed = set_thread_resolved(db, &thread_key.0, &thread_key.1, resolved)?;
            // Seen working, so the supervisor reports the held reply once the thread is idle.
            if held.is_some() {
                set_thread_observed_working(db, &thread_key.0, &thread_key.1)?;
            }
            if changed && !close_session {
                set_parked(repository, &thread_key, resolved)?;
            }
            let thread = read_thread(db, &thread_key.0, &thread_key.1)?
                .ok_or_else(|| DomainStateError::not_found("The thread disappeared."))?;
            let view = thread_view(repository, &thread)?;
            Ok(CoordinatorEndpointOutput {
                result: json!({ "changed": changed, "thread": view }),
                changed_sessions: vec![thread_key],
            })
        }
        "/api/renameCoordinatorThread" => {
            let (result, thread_key) = super::rename::rename_thread(repository, db, params)?;
            Ok(CoordinatorEndpointOutput {
                result,
                changed_sessions: vec![thread_key],
            })
        }
        "/api/coordinatorThreadTitleReminder" => Ok(CoordinatorEndpointOutput {
            result: super::rename::read_thread_title_reminder(repository, db, params)?,
            changed_sessions: Vec::new(),
        }),
        "/api/checkAgentSessionWrite" => Ok(CoordinatorEndpointOutput {
            result: super::thread_messages::read_agent_session_write_check(db, params)?,
            changed_sessions: Vec::new(),
        }),
        _ => Err(DomainStateError::not_found(format!(
            "{endpoint_path} is not a coordinator endpoint."
        ))),
    }
}

/// CDXC:Coordinators 2026-10-04 WHY:
/// A coordinator moved on from a follow-up the CLI reported as accepted while it sat unsent in the thread's input box, and the thread waited idle with nobody told. Every message a coordinator hands its thread (`pendingMessage` on the link call that `start-thread`, `agents create` and `agents send` already make) is watched by the supervisor until the thread's transcript records it; if the thread goes idle without it, the coordinator gets an "undelivered" thread report, so a coordinator that ignored the CLI's answer still learns. Ghostex reports instead of resending: a transcript it cannot see yet is not proof the message was lost, and a resent brief makes a thread do the work twice.
/// SEE-ALSO: server/src/server/coordinator_runtime.rs (the supervisor's check), server/src/coordinators/delivery.rs (the shared transcript match).
fn watch_pending_message(
    db: &Connection,
    thread_key: &SessionKey,
    params: &Map<String, Value>,
) -> Result<bool, DomainStateError> {
    // The sender read the message in the thread's transcript itself (`delivered`, `started`).
    if params.get("messageDelivered").and_then(Value::as_bool) == Some(true) {
        drop_thread_pending_message(db, &thread_key.0, &thread_key.1)?;
        return Ok(false);
    }
    let Some(excerpt) = text(params, "pendingMessage")
        .map(|message| super::delivery::pending_message_excerpt(&message))
        .filter(|excerpt| !super::delivery::delivery_needles(excerpt).is_empty())
    else {
        return Ok(false);
    };
    let now = Utc::now();
    let sent_at = params
        .get("sentAtMs")
        .and_then(Value::as_i64)
        .and_then(chrono::DateTime::from_timestamp_millis)
        .filter(|sent_at| *sent_at <= now)
        .unwrap_or(now)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    set_thread_pending_message(
        db,
        &thread_key.0,
        &thread_key.1,
        &excerpt,
        &sent_at,
        text(params, "sendRequestId").as_deref(),
    )?;
    Ok(true)
}

/// A thread whose session the resolve is about to close must have nothing left running: closing
/// stops its agent mid-turn or under an unanswered question.
fn refuse_closing_unfinished(
    repository: &DomainRepository<'_>,
    thread: &ThreadRecord,
) -> Result<(), DomainStateError> {
    let session = repository.get_session(&thread.project_id, &thread.session_id)?;
    let progress = ThreadProgress {
        resolved: false,
        ..ThreadProgress::of(thread)
    };
    match super::live::classify_thread_live(session.as_ref(), progress, &now_iso()) {
        ThreadState::Working => Err(DomainStateError::bad_request(
            "The thread is still working, so its session was not closed. Wait for its report, or pass --keep-open to only mark it done.",
        )),
        ThreadState::Waiting => Err(DomainStateError::bad_request(
            "The thread is waiting for an answer, so its session was not closed. Answer it first, or pass --keep-open to only mark it done.",
        )),
        _ => Ok(()),
    }
}

/// CDXC:Coordinators 2026-10-01 DECISION:
/// User: "If I close those threads in the sidebar, would we still be able to reopen them and send there later if needed? Can we start doing this to make the sidebar less cluttered? Instead of moving them to Parked." So `ghostex coordinator resolve` closes a finished thread's session (`closeSession`, which skips parking) and `reopen` or a coordinator's message resumes the same conversation. Only `resolve --keep-open` still parks a done thread, so the sidebar's Sessions list keeps only the work in flight, and reopening unparks it. Supersedes the 2026-09-30 rule that every done thread was parked.
pub(super) fn set_parked(
    repository: &DomainRepository<'_>,
    key: &SessionKey,
    parked: bool,
) -> Result<(), DomainStateError> {
    let Some(session) = repository.get_session(&key.0, &key.1)? else {
        return Ok(());
    };
    if session
        .get("isParked")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        == parked
    {
        return Ok(());
    }
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(key.0));
    update.insert("sessionId".to_string(), json!(key.1));
    update.insert("isParked".to_string(), Value::Bool(parked));
    repository.update_session(&update)?;
    Ok(())
}

/// CDXC:Coordinators 2026-11-10 WHY:
/// The supervisor skips a resolved thread, and resolving clears `observedWorking`, so a turn the thread finished while it was done was never reported after a reopen: the supervisor saw an idle thread it had not seen work (GitHub #541). The reply newer than the thread's last report is that held report; a reopen hands it back (the supervisor reports it, or `agents send` prints it).
pub(super) fn held_reply(
    repository: &DomainRepository<'_>,
    thread: &ThreadRecord,
) -> Option<String> {
    let session = repository
        .get_session(&thread.project_id, &thread.session_id)
        .ok()??;
    let (text, timestamp) = crate::notification_feed::body::last_assistant_message(&session)?;
    let reported = thread
        .reported_at
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|time| time.timestamp_millis());
    match (timestamp, reported) {
        (Some(timestamp), Some(reported)) if timestamp <= reported => None,
        (None, Some(_)) => None,
        _ => Some(text),
    }
}

/// The coordinator a request names: the coordinator itself, or the coordinator of a thread.
fn resolve_coordinator(
    db: &Connection,
    key: &SessionKey,
) -> Result<CoordinatorRecord, DomainStateError> {
    if let Some(record) = read_coordinator(db, &key.0, &key.1)? {
        return Ok(record);
    }
    if let Some(thread) = read_thread(db, &key.0, &key.1)? {
        if let Some(record) = read_coordinator(
            db,
            &thread.coordinator_project_id,
            &thread.coordinator_session_id,
        )? {
            return Ok(record);
        }
    }
    Err(DomainStateError::not_found(
        "That session is not a coordinator. Create one with ghostex coordinator create.",
    ))
}

fn apply_update(
    coordinator: &mut CoordinatorRecord,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    if let Some(goal) = params.get("goal").and_then(Value::as_str) {
        let goal = goal.trim();
        if goal.chars().count() > COORDINATOR_GOAL_MAX_CHARS {
            return Err(DomainStateError::bad_request(format!(
                "Keep the goal under {COORDINATOR_GOAL_MAX_CHARS} characters; put details in the instructions."
            )));
        }
        coordinator.goal = goal.to_string();
    }
    if let Some(instructions) = params.get("instructions").and_then(Value::as_str) {
        let instructions = instructions.trim();
        if instructions.chars().count() > COORDINATOR_INSTRUCTIONS_MAX_CHARS {
            return Err(DomainStateError::bad_request(format!(
                "Standing instructions are limited to {COORDINATOR_INSTRUCTIONS_MAX_CHARS} characters."
            )));
        }
        coordinator.instructions = instructions.to_string();
    }
    // The whole list at once, from a client's Memory editor: notes kept keep their date.
    if let Some(notes) = params.get("memory").and_then(Value::as_array) {
        let mut memory: Vec<CoordinatorMemoryNote> = Vec::new();
        for note in notes.iter().filter_map(Value::as_str) {
            let note = note.split_whitespace().collect::<Vec<_>>().join(" ");
            if note.is_empty()
                || memory
                    .iter()
                    .any(|kept| kept.text.eq_ignore_ascii_case(&note))
            {
                continue;
            }
            if note.chars().count() > COORDINATOR_MEMORY_NOTE_MAX_CHARS {
                return Err(DomainStateError::bad_request(format!(
                    "Keep a note to one line under {COORDINATOR_MEMORY_NOTE_MAX_CHARS} characters."
                )));
            }
            let created_at = coordinator
                .memory
                .iter()
                .find(|existing| existing.text == note)
                .map(|existing| existing.created_at.clone())
                .unwrap_or_else(now_iso);
            memory.push(CoordinatorMemoryNote {
                text: note,
                created_at,
            });
        }
        if memory.len() > COORDINATOR_MEMORY_MAX_NOTES {
            return Err(DomainStateError::bad_request(format!(
                "Memory holds {COORDINATOR_MEMORY_MAX_NOTES} notes. Forget or merge old ones first."
            )));
        }
        coordinator.memory = memory;
    }
    if let Some(note) = params.get("remember").and_then(Value::as_str) {
        let note = note.split_whitespace().collect::<Vec<_>>().join(" ");
        if note.is_empty() {
            return Err(DomainStateError::bad_request("The note is empty."));
        }
        if note.chars().count() > COORDINATOR_MEMORY_NOTE_MAX_CHARS {
            return Err(DomainStateError::bad_request(format!(
                "Keep a note to one line under {COORDINATOR_MEMORY_NOTE_MAX_CHARS} characters."
            )));
        }
        if coordinator.memory.len() >= COORDINATOR_MEMORY_MAX_NOTES {
            return Err(DomainStateError::bad_request(format!(
                "Memory holds {COORDINATOR_MEMORY_MAX_NOTES} notes. Forget or merge old ones first."
            )));
        }
        if !coordinator
            .memory
            .iter()
            .any(|existing| existing.text.eq_ignore_ascii_case(&note))
        {
            coordinator.memory.push(CoordinatorMemoryNote {
                text: note,
                created_at: now_iso(),
            });
        }
    }
    if let Some(number) = params.get("forget").and_then(Value::as_u64) {
        let index = (number as usize)
            .checked_sub(1)
            .filter(|index| *index < coordinator.memory.len());
        let Some(index) = index else {
            return Err(DomainStateError::bad_request(format!(
                "There is no note {number}. Run ghostex coordinator status to see the numbers."
            )));
        };
        coordinator.memory.remove(index);
    }
    Ok(())
}

pub(crate) fn session_title_of(session: &Value) -> String {
    project_session_title(session)
        .get("displayTitle")
        .or_else(|| session.get("title"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// One thread as the CLI and the clients read it.
pub fn thread_view(
    repository: &DomainRepository<'_>,
    thread: &ThreadRecord,
) -> Result<Value, DomainStateError> {
    let session = repository.get_session(&thread.project_id, &thread.session_id)?;
    let generated_at = now_iso();
    let state = super::live::classify_thread_live(
        session.as_ref(),
        ThreadProgress::of(thread),
        &generated_at,
    );
    let mut view = json!({
        "globalRef": create_global_session_ref(repository.server_id.as_str(), &thread.project_id, &thread.session_id),
        "projectId": thread.project_id,
        "sessionId": thread.session_id,
        "state": state.as_str(),
        "task": thread.task.chars().take(300).collect::<String>(),
        "createdAt": thread.created_at,
        "reportedAt": thread.reported_at,
        "resolvedAt": thread.resolved_at,
        "lastReport": thread.last_report,
    });
    if let Some(session) = session.as_ref() {
        view["title"] = json!(session_title_of(session));
        view["agentId"] = session.get("agentId").cloned().unwrap_or(Value::Null);
        view["lifecycleState"] = json!(effective_lifecycle_state(session));
        view["activity"] = json!(presentation_activity(session, &generated_at));
        if let Some(marker) = crate::worktree_sessions::read_worktree_session_marker(session) {
            view["branch"] = json!(marker.branch);
            view["worktreePath"] = json!(marker.path);
        }
        if state == ThreadState::Waiting {
            if let Some(prompt) = super::state::waiting_prompt(session) {
                view["waitingFor"] = json!(prompt.summary);
            }
        }
        // Times to judge a long step by (issue #427): when this working stint began, when its
        // transcript or screen last changed, and whether it has been quiet long enough to report.
        if state == ThreadState::Working {
            if let Some(started) =
                crate::presentation::session_effective_working_started_at(session, &generated_at)
            {
                view["workingStartedAt"] = json!(started);
            }
            if let Some(quiet) = super::stall::session_quiet(&thread.project_id, &thread.session_id)
            {
                if let Some(at) = quiet.last_output_at.and_then(super::stall::iso_from_ms) {
                    view["lastOutputAt"] = json!(at);
                }
                if quiet.stalled {
                    view["stalled"] = json!(true);
                    view["quietSince"] = json!(super::stall::iso_from_ms(quiet.quiet_since));
                }
                if quiet.stuck {
                    view["stuck"] = json!(true);
                }
            }
        }
    }
    Ok(view)
}

/// The coordinator, its memory and every thread, most urgent state first.
pub fn coordinator_view(
    db: &Connection,
    repository: &DomainRepository<'_>,
    coordinator: &CoordinatorRecord,
) -> Result<Value, DomainStateError> {
    let session = repository.get_session(&coordinator.project_id, &coordinator.session_id)?;
    let mut threads = list_threads_for(db, &coordinator.project_id, &coordinator.session_id)?
        .iter()
        .map(|thread| thread_view(repository, thread))
        .collect::<Result<Vec<_>, _>>()?;
    threads.extend(
        super::remote_threads::list_remote_threads_for(db, &coordinator.key())?
            .iter()
            .map(super::remote_threads::remote_thread_view),
    );
    let order = |state: &str| match state {
        "waiting" => ThreadState::Waiting.order(),
        "finished" => ThreadState::Finished.order(),
        "working" => ThreadState::Working.order(),
        "sleeping" | "unreachable" => ThreadState::Sleeping.order(),
        "closed" => ThreadState::Closed.order(),
        _ => ThreadState::Done.order(),
    };
    threads.sort_by_key(|thread| order(thread["state"].as_str().unwrap_or_default()));
    let playbook_notice =
        super::playbook::playbook_status_notice(db, repository, &coordinator.key())?;
    Ok(json!({
        "coordinator": {
            "globalRef": create_global_session_ref(repository.server_id.as_str(), &coordinator.project_id, &coordinator.session_id),
            "projectId": coordinator.project_id,
            "sessionId": coordinator.session_id,
            "title": session.as_ref().map(session_title_of).unwrap_or_default(),
            "agentId": session.as_ref().and_then(|session| session.get("agentId").cloned()).unwrap_or(Value::Null),
            "goal": coordinator.goal,
            "instructions": coordinator.instructions,
            "memory": coordinator.memory_value(),
            "createdAt": coordinator.created_at,
        },
        "threads": threads,
        "playbookNotice": playbook_notice,
        "networkNotice": network_notice(),
        "workNotice": super::work::idle_notice_for(db, &coordinator.key()),
        "generatedAt": Utc::now().to_rfc3339(),
    }))
}

/// What `coordinator status` says while this computer is offline (CDXC:OfflineHold in
/// network_status): Ghostex holds its nudges and reports, so silence from the threads is expected.
fn network_notice() -> Option<String> {
    crate::network_status::offline_since().map(|since| {
        format!(
            "{} (since {}). Ghostex holds nudges, not-moving reports and reports to you until it's back, then sends one line saying so.",
            crate::network_status::OFFLINE_LABEL,
            crate::network_status::local_clock(since)
        )
    })
}
