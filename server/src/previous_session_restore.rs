//! `/api/restorePreviousSession`: bring a closed session back as a new session that carries the
//! closed one's agent, conversation id, persistence name, tag, sidebar place, title and saved
//! launch (launch settings, account, model and commands), then drop the closed history row. The
//! agent itself comes back when a client attaches: the attach plan is built from those fields, for
//! every agent the same way.
//!
//! CDXC:Sessions 2026-10-14 DECISION:
//! User: "add ONE gxserver restore for closed sessions ... that restores every agent the way the desktop does today (not just Claude/Codex/Pi via /api/openConversation), and make the desktop, the 'ghostex restore-previous-session' CLI and the phone all use it, so there is one restore rule for every client. Move the desktop's restore logic ... into gxserver rather than duplicating it." This is the desktop's former `gpui_restore_previous_session_from_history_id` and its metadata and create-params helpers (apps/desktop/src/app/helpers/remote/previous_sessions.rs), unchanged except that a session with no closed row is refused instead of restored as an empty "Terminal Session", and that the saved launch comes along (`RESTORED_RUNTIME_KEYS`).
//!
//! SEE-ALSO: server/src/server/previous_session_restore_http.rs, server/src/ghostex_cli/previous_sessions.rs, apps/mobile/app/src/sessions/closedSessions.ts.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};

/// The title a restored session gets when its closed row has none.
const DEFAULT_TITLE: &str = "Terminal Session";

/// CDXC:Sessions 2026-10-16 DECISION:
/// User chose: "restore moves the coordinator record and its threads to the new session, and keeps the closed session's account, model and launch commands." A restore copied only the conversation id, so the session resumed on the default account and model without the flags it ran with (a coordinator lost its role prompt), and a custom agent no longer in the project's agent list resumed nothing at all: its CLI family lives only in the closed row's launch settings, so wake had no command and the pane showed "gxserver did not confirm the zmx provider exists". The closed row's launch settings come along whole, and these runtime keys with them; activity, recovery and switch state stay behind because they describe the old process.
/// SEE-ALSO: server/src/coordinators/restore.rs (the coordinator records), server/src/agent_model_pins.rs (`carry_session_choice`), server/src/domain/repository/session.rs (`create_session`, which carries a box or detected session's whole row itself).
const RESTORED_RUNTIME_KEYS: &[&str] = &[
    "agentCommand",
    "accountBaseCommand",
    "accountCommand",
    "accountId",
    "accountName",
    "accountColor",
    "accountSlot",
    "accountProvider",
    "accountPolicyOverride",
    "agentModelPin",
    "agentName",
    "agentSessionPath",
    "firstUserMessage",
    "launchAgentId",
    // CDXC:SessionLinks 2026-10-24 DECISION: User: "when i close a session in the sidebar then i restore it it should keep the links". The restored session is a new row, so its links come along with it: `sessionLinks` (the links that joined by themselves or by hand, the pins, the removed ones, which stay removed, and what was last read about each) and work mode's hand-set `workLinks`. Before this the restored session started with none, and only what its branch or transcript named came back.
    "sessionLinks",
    "workLinks",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestorePreviousSessionRequest {
    pub project_id: String,
    pub session_id: String,
    /// Stamped on `/api/removeSession` for the closed row.
    pub reason: String,
}

impl RestorePreviousSessionRequest {
    pub fn read(params: &Map<String, Value>) -> Result<Self, DomainStateError> {
        let text = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId")) else {
            return Err(DomainStateError::bad_request(
                "restorePreviousSession needs projectId and sessionId.",
            ));
        };
        Ok(Self {
            project_id,
            session_id,
            reason: text("reason").unwrap_or_else(|| "restorePreviousSession".to_string()),
        })
    }
}

/// Restores the session. The closed session's own row is read from `repository` (a legacy
/// history entry has none), and its coordinator records and model choice move to the new session
/// in `db`. `call` performs one of this daemon's own endpoints and answers its `result`. Answers
/// `{ outcome: "restored", projectId, sessionId, zmxName? }` naming the new session.
pub fn restore_previous_session(
    request: &RestorePreviousSessionRequest,
    repository: &DomainRepository<'_>,
    db: &Connection,
    call: &dyn Fn(&str, Value) -> Result<Value, DomainStateError>,
) -> Result<Value, DomainStateError> {
    let (project_id, session_id) = (request.project_id.as_str(), request.session_id.as_str());
    let stored = repository.get_session(project_id, session_id)?;
    let previous = call(
        "/api/listPreviousSessions",
        json!({
            "includeActive": false,
            "includePrevious": true,
            "limit": 20,
            "projectId": project_id,
            "query": session_id,
        }),
    )?;
    let Some(row) = closed_row(&previous, project_id, session_id) else {
        return Err(DomainStateError::not_found(
            "This closed session is no longer in Previous Sessions.",
        ));
    };
    let created = call(
        "/api/createSession",
        create_params(project_id, session_id, row, stored.as_ref()),
    )?;
    let session = created.get("session");
    let Some(restored_id) = non_empty(session, "sessionId") else {
        return Err(DomainStateError::bad_request(
            "The closed session could not be restored.",
        ));
    };
    let restored_project = non_empty(session, "projectId").unwrap_or(project_id.to_string());
    crate::coordinators::carry_coordinator_to_restored_session(
        db,
        &(project_id.to_string(), session_id.to_string()),
        &(restored_project.clone(), restored_id.clone()),
    )?;
    crate::agent_model_pins::carry_session_choice(
        (project_id, session_id),
        (&restored_project, &restored_id),
    );
    // A failure here leaves an extra history row and changes nothing else.
    let _ = call(
        "/api/removeSession",
        json!({
            "projectId": project_id,
            "reason": request.reason,
            "sessionId": session_id,
        }),
    );
    let mut answer =
        json!({ "outcome": "restored", "projectId": restored_project, "sessionId": restored_id });
    if let Some(zmx_name) = non_empty(session, "zmxName") {
        answer["zmxName"] = Value::String(zmx_name);
    }
    let (restored_threads, threads_not_restored) = restore_threads_closed_with_coordinator(
        &(restored_project, restored_id),
        repository,
        db,
        call,
    );
    if !restored_threads.is_empty() {
        answer["restoredThreads"] = json!(restored_threads);
    }
    if threads_not_restored > 0 {
        answer["threadsNotRestored"] = json!(threads_not_restored);
    }
    Ok(answer)
}

/// Restores the threads that closing this coordinator closed with it (see the CDXC:Coordinators
/// 2026-11-02 DECISION on `close_threads_with_coordinator` in
/// server/src/server/coordinator_close.rs). Each comes back through this same restore, which moves
/// its thread link onto its new session, so it sits under the coordinator again on its own
/// conversation. A thread that was brought back by hand meanwhile is left as it is. Answers the new
/// sessions' ids and how many could not come back.
fn restore_threads_closed_with_coordinator(
    coordinator: &crate::coordinators::SessionKey,
    repository: &DomainRepository<'_>,
    db: &Connection,
    call: &dyn Fn(&str, Value) -> Result<Value, DomainStateError>,
) -> (Vec<String>, usize) {
    let Ok(threads) = crate::coordinators::threads_closed_with_coordinator(db, coordinator) else {
        return (Vec::new(), 0);
    };
    let mut restored = Vec::new();
    let mut not_restored = 0;
    for thread in threads {
        let still_closed = repository
            .get_session(&thread.project_id, &thread.session_id)
            .ok()
            .flatten()
            .is_some_and(|session| {
                crate::presentation::effective_lifecycle_state(&session) == "stopped"
            });
        if !still_closed {
            continue;
        }
        let request = RestorePreviousSessionRequest {
            project_id: thread.project_id,
            session_id: thread.session_id,
            reason: "coordinatorRestored".to_string(),
        };
        match restore_previous_session(&request, repository, db, call) {
            Ok(answer) => restored.extend(non_empty(Some(&answer), "sessionId")),
            Err(_) => not_restored += 1,
        }
    }
    let _ = crate::coordinators::clear_threads_closed_with_coordinator(db, coordinator);
    (restored, not_restored)
}

/// The closed row for the session; a row still marked running is taken only when no stopped one
/// matches.
fn closed_row<'a>(
    previous: &'a Value,
    project_id: &str,
    session_id: &str,
) -> Option<&'a Map<String, Value>> {
    let mut running_match = None;
    for row in previous
        .get("results")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_object)
    {
        if text(row, "projectId") != Some(project_id) || text(row, "sessionId") != Some(session_id)
        {
            continue;
        }
        let running = row.get("isRunning").and_then(Value::as_bool) == Some(true)
            || row.get("lifecycleState").and_then(Value::as_str) == Some("running")
            || row.get("providerSessionState").and_then(Value::as_str) == Some("running");
        if !running {
            return Some(row);
        }
        running_match.get_or_insert(row);
    }
    running_match
}

fn create_params(
    project_id: &str,
    session_id: &str,
    row: &Map<String, Value>,
    stored: Option<&Value>,
) -> Value {
    let title = ["displayTitle", "primaryTitle", "title"]
        .iter()
        .find_map(|key| text(row, key))
        .unwrap_or(DEFAULT_TITLE);
    let mut create = Map::new();
    create.insert("kind".into(), json!("terminal"));
    if let Some(agent_id) = text(row, "agentId") {
        create.insert("agentId".into(), json!(agent_id));
    }
    create.insert("lifecycleState".into(), json!("running"));
    create.insert("projectId".into(), json!(project_id));
    create.insert("restoredFromSessionId".into(), json!(session_id));
    create.insert("surface".into(), json!("workspace"));
    create.insert("title".into(), json!(title));
    if let Some(launch_settings) = stored
        .and_then(|session| session.get("launchSettings"))
        .and_then(Value::as_object)
        .filter(|settings| !settings.is_empty())
    {
        create.insert("launchSettings".into(), json!(launch_settings));
    }
    let mut runtime_settings: Map<String, Value> = stored
        .and_then(|session| session.get("runtimeSettings"))
        .and_then(Value::as_object)
        .map(|stored_runtime| {
            RESTORED_RUNTIME_KEYS
                .iter()
                .filter_map(|key| {
                    let value = stored_runtime.get(*key)?;
                    (!value.is_null()).then(|| ((*key).to_string(), value.clone()))
                })
                .collect()
        })
        .unwrap_or_default();
    for key in [
        "agentSessionId",
        "sessionPersistenceName",
        "sessionPersistenceProvider",
    ] {
        if let Some(value) = text(row, key) {
            runtime_settings.insert(key.to_string(), json!(value));
        }
    }
    if !runtime_settings.is_empty() {
        create.insert("runtimeSettings".into(), Value::Object(runtime_settings));
    }
    if let Some(tag) = text(row, "sessionTag") {
        create.insert("sessionTag".into(), json!(tag));
    }
    if let Some(order) = row.get("sidebarOrder").filter(|order| order.is_number()) {
        create.insert("sidebarOrder".into(), order.clone());
    }
    Value::Object(create)
}

fn text<'a>(row: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn non_empty(value: Option<&Value>, key: &str) -> Option<String> {
    value?
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;
    use crate::storage::{initialize_gxserver_storage, open_gxserver_database};

    fn params(value: Value) -> Map<String, Value> {
        value.as_object().cloned().expect("params object")
    }

    #[test]
    fn a_closed_session_comes_back_with_its_links() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        let repository = DomainRepository::new(&db, "S1");
        let project = repository
            .create_project(&params(
                json!({ "name": "Links", "path": std::env::temp_dir() }),
            ))
            .expect("create project");
        let project_id = project["projectId"]
            .as_str()
            .expect("projectId")
            .to_string();
        let links = json!({
            "items": [
                { "key": "pr:maddada/ghostex#286", "source": "user" },
                { "key": "issue:maddada/ghostex#283", "source": "closes", "via": "#284" },
                { "key": "linear:GHX-7", "source": "mentioned" },
            ],
            "removed": ["pr:maddada/ghostex#279"],
            "removedItems": [{ "key": "pr:maddada/ghostex#279", "source": "agentOpened" }],
            "pinned": ["issue:maddada/ghostex#283"],
            "read": { "pr:maddada/ghostex#286": { "state": "merged", "title": "feat", "readAt": 1 } },
        });
        let work_links = json!({ "pullRequest": "286" });
        let session = repository
            .create_session(
                &params(json!({
                    "projectId": project_id,
                    "kind": "agent",
                    "agentId": "codex",
                    "title": "Linked session",
                    "lifecycleState": "running",
                    "runtimeSettings": { "sessionLinks": links, "workLinks": work_links },
                })),
                false,
            )
            .expect("create session");
        let session_id = session["sessionId"]
            .as_str()
            .expect("sessionId")
            .to_string();

        // Closed the way the sidebar's Close leaves it: the row stays, stopped.
        repository
            .update_session_for_lifecycle(&params(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "lifecycleState": "stopped",
            })))
            .expect("close session");

        let call = |path: &str, body: Value| -> Result<Value, DomainStateError> {
            match path {
                "/api/listPreviousSessions" => Ok(json!({ "results": [{
                    "projectId": project_id,
                    "sessionId": session_id,
                    "title": "Linked session",
                    "agentId": "codex",
                    "lifecycleState": "stopped",
                }] })),
                "/api/createSession" => Ok(json!({
                    "session": repository.create_session(&params(body), false)?,
                })),
                "/api/removeSession" => Ok(json!({})),
                other => panic!("unexpected call {other}"),
            }
        };
        let request = RestorePreviousSessionRequest {
            project_id: project_id.clone(),
            session_id: session_id.clone(),
            reason: "test".to_string(),
        };
        let answer = restore_previous_session(&request, &repository, &db, &call).expect("restore");
        let restored_id = answer["sessionId"].as_str().expect("restored id");
        assert_ne!(restored_id, session_id);
        let restored = repository
            .get_session(&project_id, restored_id)
            .expect("read restored")
            .expect("restored row");

        assert_eq!(
            restored.pointer("/runtimeSettings/sessionLinks"),
            Some(&links)
        );
        assert_eq!(
            restored.pointer("/runtimeSettings/workLinks"),
            Some(&work_links)
        );
        let stored = crate::session_links::stored_session_links(&restored);
        assert_eq!(stored.items.len(), 3);
        assert!(stored.is_pinned("issue:maddada/ghostex#283"));
        // A link the person removed stays removed.
        assert!(stored.is_removed("pr:maddada/ghostex#279"));
        assert!(stored.read.contains_key("pr:maddada/ghostex#286"));
    }

    #[test]
    fn restoring_a_coordinator_brings_back_the_threads_its_close_took() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        let repository = DomainRepository::new(&db, "S1");
        let project = repository
            .create_project(&params(
                json!({ "name": "Coordinated", "path": std::env::temp_dir() }),
            ))
            .expect("create project");
        let project_id = project["projectId"]
            .as_str()
            .expect("projectId")
            .to_string();
        let create = |title: &str, lifecycle: &str| -> String {
            repository
                .create_session(
                    &params(json!({
                        "projectId": project_id,
                        "kind": "agent",
                        "agentId": "claude",
                        "title": title,
                        "lifecycleState": lifecycle,
                    })),
                    false,
                )
                .expect("create session")["sessionId"]
                .as_str()
                .expect("sessionId")
                .to_string()
        };
        let close = |session_id: &str| {
            repository
                .update_session_for_lifecycle(&params(json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "lifecycleState": "stopped",
                })))
                .expect("close session");
        };
        let coordinator = create("Main Coordinator", "running");
        let working = create("Working thread", "running");
        let asleep = create("Sleeping thread", "sleeping");
        let done = create("Done thread", "running");
        crate::coordinators::insert_coordinator(&db, &project_id, &coordinator, "", "")
            .expect("coordinator");
        for thread in [&working, &asleep, &done] {
            crate::coordinators::link_thread(
                &db,
                &project_id,
                thread,
                &project_id,
                &coordinator,
                "task",
            )
            .expect("link thread");
        }
        crate::coordinators::set_thread_resolved(&db, &project_id, &done, true).expect("resolve");
        let coordinator_key = (project_id.clone(), coordinator.clone());

        // Closing the coordinator takes its two open threads, not the done one.
        let open = crate::coordinators::threads_open_under(&db, &repository, &coordinator_key)
            .expect("open threads");
        let mut open_ids: Vec<String> = open
            .iter()
            .map(|thread| thread.session_id.clone())
            .collect();
        open_ids.sort();
        let mut expected = vec![working.clone(), asleep.clone()];
        expected.sort();
        assert_eq!(open_ids, expected);
        close(&coordinator);
        for thread in &open {
            close(&thread.session_id);
            crate::coordinators::mark_thread_closed_with_coordinator(&db, &thread.key())
                .expect("mark");
        }

        let call = |path: &str, body: Value| -> Result<Value, DomainStateError> {
            match path {
                "/api/listPreviousSessions" => {
                    let wanted = body["query"].as_str().unwrap_or_default().to_string();
                    let row = repository
                        .get_session(&project_id, &wanted)?
                        .expect("closed row");
                    Ok(json!({ "results": [{
                        "projectId": project_id,
                        "sessionId": wanted,
                        "title": row["title"],
                        "agentId": "claude",
                        "lifecycleState": "stopped",
                    }] }))
                }
                "/api/createSession" => Ok(json!({
                    "session": repository.create_session(&params(body), false)?,
                })),
                "/api/removeSession" => Ok(json!({})),
                other => panic!("unexpected call {other}"),
            }
        };
        let request = RestorePreviousSessionRequest {
            project_id: project_id.clone(),
            session_id: coordinator.clone(),
            reason: "test".to_string(),
        };
        let answer = restore_previous_session(&request, &repository, &db, &call).expect("restore");
        let restored = (
            project_id.clone(),
            answer["sessionId"]
                .as_str()
                .expect("restored id")
                .to_string(),
        );
        let restored_threads = answer["restoredThreads"]
            .as_array()
            .expect("restored threads");
        assert_eq!(restored_threads.len(), 2);

        // Both come back under the restored coordinator, unmarked and not done; the done one stays done.
        let threads =
            crate::coordinators::list_threads_for(&db, &restored.0, &restored.1).expect("threads");
        let open_now: Vec<_> = threads
            .iter()
            .filter(|thread| !thread.is_resolved())
            .collect();
        assert_eq!(open_now.len(), 2);
        for thread in open_now {
            assert!(thread.closed_with_coordinator_at.is_none());
            assert!(restored_threads
                .iter()
                .any(|id| id.as_str() == Some(thread.session_id.as_str())));
        }
        assert!(threads
            .iter()
            .any(|thread| thread.session_id == done && thread.is_resolved()));
    }
}
