//! Threads that run on another computer this one is connected to (migration 0051): their link,
//! the supervisor's bookkeeping, the endpoints the CLI calls, and the coordinator field the
//! sidebar nests them by. The supervisor's clock is `server/coordinator_remote_runtime.rs`.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Map, Value};

use super::records::{now_iso, read_coordinator, sql_error, SessionKey};
use crate::domain::DomainStateError;

/// A thread's computer, project and session.
pub type RemoteThreadKey = (String, String, String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteThreadRecord {
    pub server_id: String,
    pub project_id: String,
    pub session_id: String,
    pub coordinator_project_id: String,
    pub coordinator_session_id: String,
    pub task: String,
    pub title: String,
    pub resolved_at: Option<String>,
    pub observed_working: bool,
    pub reported_at: Option<String>,
    pub reported_prompt_key: Option<String>,
    pub last_report: Option<String>,
    /// The state the supervisor last saw (`working`, `waiting`, `finished`, `sleeping`, `closed`,
    /// or `unreachable` while its computer is not connected).
    pub last_state: Option<String>,
    /// The assistant message the last finished report carried (migration 0052).
    pub reported_message_id: Option<String>,
    /// The other computer's own activity time (ms) the last finished check covered (migration
    /// 0053), compared only with that computer's later times, never with this computer's clock.
    pub checked_activity_ms: Option<i64>,
    /// When anything about the link last changed on this computer (its state, title or report),
    /// the activity time the coordinator's Workers list orders it by.
    pub updated_at: String,
}

impl RemoteThreadRecord {
    pub fn key(&self) -> RemoteThreadKey {
        (
            self.server_id.clone(),
            self.project_id.clone(),
            self.session_id.clone(),
        )
    }

    pub fn coordinator_key(&self) -> SessionKey {
        (
            self.coordinator_project_id.clone(),
            self.coordinator_session_id.clone(),
        )
    }

    pub fn global_ref(&self) -> String {
        crate::ids::create_global_session_ref(&self.server_id, &self.project_id, &self.session_id)
    }

    /// The state clients show: done once resolved, else what the supervisor last saw.
    pub fn state(&self) -> &str {
        if self.resolved_at.is_some() {
            return "done";
        }
        match self.last_state.as_deref() {
            Some(state) => state,
            None => "working",
        }
    }
}

const COLUMNS: &str = "serverId, projectId, sessionId, coordinatorProjectId, coordinatorSessionId, task, title, resolvedAt, observedWorking, reportedAt, reportedPromptKey, lastReport, lastState, reportedMessageId, checkedActivityMs, updatedAt";

fn from_row(row: &Row<'_>) -> rusqlite::Result<RemoteThreadRecord> {
    Ok(RemoteThreadRecord {
        server_id: row.get(0)?,
        project_id: row.get(1)?,
        session_id: row.get(2)?,
        coordinator_project_id: row.get(3)?,
        coordinator_session_id: row.get(4)?,
        task: row.get(5)?,
        title: row.get(6)?,
        resolved_at: row.get(7)?,
        observed_working: row.get::<_, i64>(8)? != 0,
        reported_at: row.get(9)?,
        reported_prompt_key: row.get(10)?,
        last_report: row.get(11)?,
        last_state: row.get(12)?,
        reported_message_id: row.get(13)?,
        checked_activity_ms: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

pub fn list_remote_threads(db: &Connection) -> Result<Vec<RemoteThreadRecord>, DomainStateError> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {COLUMNS} FROM coordinator_remote_threads ORDER BY createdAt"
        ))
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], from_row)
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(rows)
}

pub fn list_remote_threads_for(
    db: &Connection,
    coordinator: &SessionKey,
) -> Result<Vec<RemoteThreadRecord>, DomainStateError> {
    Ok(list_remote_threads(db)?
        .into_iter()
        .filter(|thread| thread.coordinator_key() == *coordinator)
        .collect())
}

pub fn read_remote_thread(
    db: &Connection,
    key: &RemoteThreadKey,
) -> Result<Option<RemoteThreadRecord>, DomainStateError> {
    db.query_row(
        &format!(
            "SELECT {COLUMNS} FROM coordinator_remote_threads WHERE serverId = ?1 AND projectId = ?2 AND sessionId = ?3"
        ),
        params![key.0, key.1, key.2],
        from_row,
    )
    .optional()
    .map_err(sql_error)
}

/// Links a session on another computer to a coordinator here. A thread linked again keeps its
/// history; linking it to another coordinator moves and reopens it.
pub fn link_remote_thread(
    db: &Connection,
    key: &RemoteThreadKey,
    coordinator: &SessionKey,
    task: &str,
    title: &str,
) -> Result<(), DomainStateError> {
    let timestamp = now_iso();
    db.execute(
        r#"
        INSERT INTO coordinator_remote_threads (
          serverId, projectId, sessionId, coordinatorProjectId, coordinatorSessionId, task, title,
          resolvedAt, observedWorking, reportedAt, reportedPromptKey, lastReport, lastState,
          createdAt, updatedAt
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, 0, CASE WHEN ?6 = '' THEN ?8 ELSE NULL END, NULL, NULL, NULL, ?8, ?8)
        ON CONFLICT(serverId, projectId, sessionId) DO UPDATE SET
          task = CASE WHEN coordinator_remote_threads.task = '' THEN excluded.task ELSE coordinator_remote_threads.task END,
          title = CASE WHEN excluded.title = '' THEN coordinator_remote_threads.title ELSE excluded.title END,
          resolvedAt = CASE
            WHEN coordinator_remote_threads.coordinatorProjectId = excluded.coordinatorProjectId
             AND coordinator_remote_threads.coordinatorSessionId = excluded.coordinatorSessionId
            THEN coordinator_remote_threads.resolvedAt ELSE NULL END,
          coordinatorProjectId = excluded.coordinatorProjectId,
          coordinatorSessionId = excluded.coordinatorSessionId,
          updatedAt = excluded.updatedAt
        "#,
        params![key.0, key.1, key.2, coordinator.0, coordinator.1, task, title, timestamp],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub fn unlink_remote_thread(
    db: &Connection,
    key: &RemoteThreadKey,
) -> Result<bool, DomainStateError> {
    db.execute(
        "DELETE FROM coordinator_remote_threads WHERE serverId = ?1 AND projectId = ?2 AND sessionId = ?3",
        params![key.0, key.1, key.2],
    )
    .map(|changed| changed > 0)
    .map_err(sql_error)
}

pub fn set_remote_thread_resolved(
    db: &Connection,
    key: &RemoteThreadKey,
    resolved: bool,
) -> Result<bool, DomainStateError> {
    let timestamp = now_iso();
    db.execute(
        "UPDATE coordinator_remote_threads SET resolvedAt = CASE WHEN ?4 THEN COALESCE(resolvedAt, ?5) ELSE NULL END, observedWorking = CASE WHEN ?4 THEN observedWorking ELSE 0 END, updatedAt = ?5 WHERE serverId = ?1 AND projectId = ?2 AND sessionId = ?3",
        params![key.0, key.1, key.2, resolved, timestamp],
    )
    .map(|changed| changed > 0)
    .map_err(sql_error)
}

/// What the supervisor saw this pass. Returns whether the state clients show changed.
pub fn record_remote_thread_progress(
    db: &Connection,
    key: &RemoteThreadKey,
    observed_working: bool,
    last_state: &str,
) -> Result<bool, DomainStateError> {
    let previous = read_remote_thread(db, key)?;
    let changed = previous
        .as_ref()
        .is_some_and(|thread| thread.last_state.as_deref() != Some(last_state));
    if previous
        .as_ref()
        .is_some_and(|thread| thread.observed_working == observed_working && !changed)
    {
        return Ok(false);
    }
    db.execute(
        "UPDATE coordinator_remote_threads SET observedWorking = ?4, lastState = ?5, updatedAt = ?6 WHERE serverId = ?1 AND projectId = ?2 AND sessionId = ?3",
        params![key.0, key.1, key.2, observed_working, last_state, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(changed)
}

/// Keeps the title the thread's own computer shows (a thread adopted with `link` has none, and the
/// agent's naming can change it). Returns whether it changed.
pub fn set_remote_thread_title(
    db: &Connection,
    key: &RemoteThreadKey,
    title: &str,
) -> Result<bool, DomainStateError> {
    db.execute(
        "UPDATE coordinator_remote_threads SET title = ?4, updatedAt = ?5 WHERE serverId = ?1 AND projectId = ?2 AND sessionId = ?3 AND title != ?4",
        params![key.0, key.1, key.2, title, now_iso()],
    )
    .map(|changed| changed > 0)
    .map_err(sql_error)
}

/// The other computer's activity time a finished check covered (whether it reported or not), so
/// that time is never read again.
pub fn record_remote_thread_checked(
    db: &Connection,
    key: &RemoteThreadKey,
    activity_ms: i64,
) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE coordinator_remote_threads SET checkedActivityMs = MAX(COALESCE(checkedActivityMs, 0), ?4) WHERE serverId = ?1 AND projectId = ?2 AND sessionId = ?3",
        params![key.0, key.1, key.2, activity_ms],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// A report that reached the coordinator. A finished turn clears the working mark; a question
/// remembers its prompt so it is reported once.
pub fn record_remote_thread_report(
    db: &Connection,
    key: &RemoteThreadKey,
    prompt_key: Option<&str>,
    message_id: Option<&str>,
    excerpt: &str,
) -> Result<(), DomainStateError> {
    let timestamp = now_iso();
    let excerpt: String = excerpt
        .chars()
        .take(super::records::COORDINATOR_THREAD_REPORT_EXCERPT_MAX_CHARS)
        .collect();
    match prompt_key {
        Some(prompt_key) => db.execute(
            "UPDATE coordinator_remote_threads SET reportedPromptKey = ?4, lastReport = ?5, updatedAt = ?6 WHERE serverId = ?1 AND projectId = ?2 AND sessionId = ?3",
            params![key.0, key.1, key.2, prompt_key, excerpt, timestamp],
        ),
        None => db.execute(
            "UPDATE coordinator_remote_threads SET observedWorking = 0, reportedAt = ?6, reportedPromptKey = NULL, lastReport = ?5, reportedMessageId = COALESCE(?4, reportedMessageId), updatedAt = ?6 WHERE serverId = ?1 AND projectId = ?2 AND sessionId = ?3",
            params![key.0, key.1, key.2, message_id, excerpt, timestamp],
        ),
    }
    .map_err(sql_error)?;
    Ok(())
}

/// The threads a coordinator has on other computers, for its presentation session: the sidebar
/// nests the rows of those computers under it by these.
///
/// CDXC:RemoteMachines 2026-11-03 WHY:
/// The coordinator's own computer is the only one that knows the link, and the other computer may run an older gxserver, so the coordinator row carries its threads elsewhere (`coordinatorRemoteThreads`) and the client nests each one under it from that computer's own rows, which are already in its store.
/// SEE-ALSO: packages/gx-protocol/src/presentation.rs (`coordinator_remote_threads`), packages/gx-core/src/sidebar_view/model.rs (the nesting), server/src/server/coordinator_remote_runtime.rs (the supervisor).
pub fn insert_coordinator_remote_threads(
    presentation_session: &mut Value,
    db: &Connection,
    coordinator: &SessionKey,
) {
    let Ok(threads) = list_remote_threads_for(db, coordinator) else {
        return;
    };
    let threads: Vec<Value> = threads
        .iter()
        .map(|thread| {
            json!({
                "serverId": thread.server_id,
                "projectId": thread.project_id,
                "sessionId": thread.session_id,
                // The sidebar knows no "away" state; a thread whose computer is not connected is
                // drawn asleep rather than as working, which it may no longer be.
                "threadState": match thread.state() {
                    "unreachable" => "sleeping",
                    state => state,
                },
            })
        })
        .collect();
    if threads.is_empty() {
        return;
    }
    if let Some(object) = presentation_session.as_object_mut() {
        object.insert(
            "coordinatorRemoteThreads".to_string(),
            Value::Array(threads),
        );
    }
}

pub const REMOTE_THREAD_ENDPOINTS: &[&str] = &[
    "/api/linkRemoteCoordinatorThread",
    "/api/updateRemoteCoordinatorThread",
    "/api/listRemoteCoordinatorThreads",
];

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn thread_key(params: &Map<String, Value>) -> Result<RemoteThreadKey, DomainStateError> {
    match (
        text(params, "serverId"),
        text(params, "projectId"),
        text(params, "sessionId"),
    ) {
        (Some(server_id), Some(project_id), Some(session_id)) => {
            Ok((server_id, project_id, session_id))
        }
        _ => Err(DomainStateError::bad_request(
            "A thread on another computer needs serverId, projectId and sessionId.",
        )),
    }
}

/// One thread on another computer, as `coordinator status` and the clients read it.
pub fn remote_thread_view(thread: &RemoteThreadRecord) -> Value {
    let computer = crate::connected_computers::find(&thread.server_id)
        .map(|computer| computer.name)
        .unwrap_or_else(|| thread.server_id.clone());
    let state = thread.state();
    json!({
        "globalRef": thread.global_ref(),
        "serverId": thread.server_id,
        "computer": computer,
        "projectId": thread.project_id,
        "sessionId": thread.session_id,
        "title": format!("{} (on {computer})", if thread.title.is_empty() { "(untitled)" } else { thread.title.as_str() }),
        "task": thread.task.chars().take(300).collect::<String>(),
        "state": state,
        "reachable": state != "unreachable",
        "lastReport": thread.last_report,
        "resolvedAt": thread.resolved_at,
    })
}

/// What an endpoint answered, plus the coordinator whose presentation changed.
pub struct RemoteThreadEndpointOutput {
    pub result: Value,
    pub changed_coordinator: Option<SessionKey>,
}

pub fn handle_remote_thread_endpoint(
    endpoint_path: &str,
    db: &Connection,
    local_server_id: &str,
    params: &Map<String, Value>,
) -> Result<RemoteThreadEndpointOutput, DomainStateError> {
    match endpoint_path {
        "/api/linkRemoteCoordinatorThread" => {
            let coordinator = match (
                text(params, "coordinatorProjectId"),
                text(params, "coordinatorSessionId"),
            ) {
                (Some(project_id), Some(session_id)) => (project_id, session_id),
                _ => {
                    return Err(DomainStateError::bad_request(
                        "Linking a thread needs coordinatorProjectId and coordinatorSessionId.",
                    ))
                }
            };
            let key = thread_key(params)?;
            if key.0 == local_server_id {
                return Err(DomainStateError::bad_request(
                    "That session is on this computer; link it with linkCoordinatorThread.",
                ));
            }
            if read_coordinator(db, &coordinator.0, &coordinator.1)?.is_none() {
                if params.get("onlyIfCoordinator").and_then(Value::as_bool) == Some(true) {
                    return Ok(RemoteThreadEndpointOutput {
                        result: json!({ "linked": false }),
                        changed_coordinator: None,
                    });
                }
                return Err(DomainStateError::not_found(
                    "That session is not a coordinator.",
                ));
            }
            link_remote_thread(
                db,
                &key,
                &coordinator,
                &text(params, "task").unwrap_or_default(),
                &text(params, "title").unwrap_or_default(),
            )?;
            Ok(RemoteThreadEndpointOutput {
                result: json!({
                    "linked": true,
                    "globalRef": crate::ids::create_global_session_ref(&key.0, &key.1, &key.2),
                }),
                changed_coordinator: Some(coordinator),
            })
        }
        "/api/updateRemoteCoordinatorThread" => {
            let key = thread_key(params)?;
            let Some(thread) = read_remote_thread(db, &key)? else {
                return Err(DomainStateError::not_found(
                    "That session is not a thread of a coordinator on this computer.",
                ));
            };
            let changed = match text(params, "action").as_deref() {
                Some("resolve") => set_remote_thread_resolved(db, &key, true)?,
                Some("reopen") => set_remote_thread_resolved(db, &key, false)?,
                Some("unlink") => unlink_remote_thread(db, &key)?,
                _ => {
                    return Err(DomainStateError::bad_request(
                        "action must be resolve, reopen or unlink.",
                    ))
                }
            };
            Ok(RemoteThreadEndpointOutput {
                result: json!({ "updated": changed, "globalRef": thread.global_ref() }),
                changed_coordinator: Some(thread.coordinator_key()),
            })
        }
        _ => {
            let threads = match (
                text(params, "coordinatorProjectId"),
                text(params, "coordinatorSessionId"),
            ) {
                (Some(project_id), Some(session_id)) => {
                    list_remote_threads_for(db, &(project_id, session_id))?
                }
                _ => list_remote_threads(db)?,
            };
            Ok(RemoteThreadEndpointOutput {
                result: json!({ "threads": threads.iter().map(remote_thread_view).collect::<Vec<_>>() }),
                changed_coordinator: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../storage/migrations/0041_coordinators.sql"))
            .unwrap();
        db.execute_batch(include_str!(
            "../storage/migrations/0051_coordinator_remote_threads.sql"
        ))
        .unwrap();
        db.execute_batch(include_str!(
            "../storage/migrations/0052_coordinator_remote_thread_message.sql"
        ))
        .unwrap();
        db.execute_batch(include_str!(
            "../storage/migrations/0053_coordinator_remote_thread_activity.sql"
        ))
        .unwrap();
        db.execute(
            "INSERT INTO coordinators (projectId, sessionId, createdAt, updatedAt) VALUES ('P1', 'G1', 'x', 'x')",
            [],
        )
        .unwrap();
        db
    }

    fn params(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn a_counted_question_form_in_a_waiting_thread_is_not_an_approval() {
        let session = json!({
            "lifecycleState": "running",
            "runtimeSettings": {
                "sessionChatOpenQuestions": { "count": 1 },
                "agentActivity": { "activity": "attention" },
            },
        });
        assert_eq!(
            crate::session_chat_async_questions::pending_question_count(&session),
            0
        );
        assert_eq!(
            crate::coordinators::classify_thread_session(
                Some(&session),
                crate::coordinators::ThreadProgress {
                    resolved: false,
                    has_run: true,
                },
                "2026-10-27T00:00:00.000Z",
                false,
            ),
            crate::coordinators::ThreadState::Waiting
        );
        assert!(!crate::coordinators::thread_needs_user_approval(&session));
    }

    #[test]
    fn a_thread_on_another_computer_is_linked_reported_and_resolved_by_its_computer_and_ids() {
        let db = db();
        let linked = handle_remote_thread_endpoint(
            "/api/linkRemoteCoordinatorThread",
            &db,
            "S9l",
            &params(json!({
                "coordinatorProjectId": "P1", "coordinatorSessionId": "G1",
                "serverId": "S4k", "projectId": "P7", "sessionId": "G2",
                "task": "Build it", "title": "Build it there",
            })),
        )
        .unwrap();
        assert_eq!(linked.result["globalRef"], "S4k:P7:G2");
        assert_eq!(linked.changed_coordinator, Some(("P1".into(), "G1".into())));
        let key: RemoteThreadKey = ("S4k".into(), "P7".into(), "G2".into());
        // The same ids on this computer are another session.
        assert!(
            read_remote_thread(&db, &("S9l".into(), "P7".into(), "G2".into()))
                .unwrap()
                .is_none()
        );
        assert!(record_remote_thread_progress(&db, &key, true, "working").unwrap());
        assert!(!record_remote_thread_progress(&db, &key, true, "working").unwrap());
        record_remote_thread_report(&db, &key, None, Some("m1"), "Done.").unwrap();
        let thread = read_remote_thread(&db, &key).unwrap().unwrap();
        assert!(!thread.observed_working);
        assert_eq!(thread.last_report.as_deref(), Some("Done."));
        record_remote_thread_report(&db, &key, Some("prompt:abc"), None, "Which one?").unwrap();
        assert_eq!(
            read_remote_thread(&db, &key)
                .unwrap()
                .unwrap()
                .reported_prompt_key
                .as_deref(),
            Some("prompt:abc")
        );

        let mut session = json!({ "projectId": "P1", "sessionId": "G1" });
        insert_coordinator_remote_threads(&mut session, &db, &("P1".into(), "G1".into()));
        assert_eq!(session["coordinatorRemoteThreads"][0]["serverId"], "S4k");
        assert_eq!(
            session["coordinatorRemoteThreads"][0]["threadState"],
            "working"
        );

        handle_remote_thread_endpoint(
            "/api/updateRemoteCoordinatorThread",
            &db,
            "S9l",
            &params(json!({ "serverId": "S4k", "projectId": "P7", "sessionId": "G2", "action": "resolve" })),
        )
        .unwrap();
        assert_eq!(
            read_remote_thread(&db, &key).unwrap().unwrap().state(),
            "done"
        );
        let listed = handle_remote_thread_endpoint(
            "/api/listRemoteCoordinatorThreads",
            &db,
            "S9l",
            &params(json!({ "coordinatorProjectId": "P1", "coordinatorSessionId": "G1" })),
        )
        .unwrap();
        assert_eq!(listed.result["threads"][0]["state"], "done");
    }

    #[test]
    fn only_a_coordinator_takes_a_thread_and_a_local_session_is_refused() {
        let db = db();
        let not_coordinator = handle_remote_thread_endpoint(
            "/api/linkRemoteCoordinatorThread",
            &db,
            "S9l",
            &params(json!({
                "coordinatorProjectId": "P1", "coordinatorSessionId": "G9",
                "serverId": "S4k", "projectId": "P7", "sessionId": "G2", "onlyIfCoordinator": true,
            })),
        )
        .unwrap();
        assert_eq!(not_coordinator.result["linked"], false);
        assert!(handle_remote_thread_endpoint(
            "/api/linkRemoteCoordinatorThread",
            &db,
            "S9l",
            &params(json!({
                "coordinatorProjectId": "P1", "coordinatorSessionId": "G1",
                "serverId": "S9l", "projectId": "P7", "sessionId": "G2",
            })),
        )
        .is_err());
    }
}
