//! Starting a cloud session on claude.ai, sending it follow-ups, and what its terminal client
//! reads: the `get`, `start` and `send` actions of `/api/claudeCloud`.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Map, Value};

use super::github::task_with_branch;
use super::record::{session_cloud_record, write_cloud_record, CloudRecord, CloudState};
use crate::cloud_runner::{claude_code_on_the_web, CloudStartRequest, MAX_CLOUD_PROMPT_CHARS};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// Sessions whose cloud start is running, so a second start of the same draft waits its turn.
static STARTING: Mutex<Option<HashSet<(String, String)>>> = Mutex::new(None);

/// Whether this gxserver is starting that session right now.
pub(super) fn is_starting(project_id: &str, session_id: &str) -> bool {
    STARTING
        .lock()
        .ok()
        .and_then(|starting| {
            starting
                .as_ref()
                .map(|set| set.contains(&(project_id.to_string(), session_id.to_string())))
        })
        .unwrap_or(false)
}

fn internal(message: String) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message,
    }
}

pub(super) fn open(state: &AppState) -> Result<crate::storage::GxserverDatabase, DomainStateError> {
    open_gxserver_database(&state.paths)
        .map_err(|error| internal(format!("SQLite gxserver state error: {error}")))
}

/// A cloud session and its record, or a refusal naming what is wrong.
pub(super) fn load(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<(Value, CloudRecord), DomainStateError> {
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| DomainStateError::not_found("That session does not exist."))?;
    let record = session_cloud_record(&session).ok_or_else(|| {
        DomainStateError::bad_request("That session is not a Claude cloud session.")
    })?;
    Ok((session, record))
}

/// Writes a record and tells every client.
pub(super) fn save(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    record: &CloudRecord,
) -> Result<(), DomainStateError> {
    write_cloud_record(db, project_id, session_id, record)?;
    let _ = crate::server::schedule_presentation_session_delta(
        state, db, repository, project_id, session_id,
    );
    Ok(())
}

/// The runner on the session's own Claude account, or on the current CLI login.
pub(super) fn runner_for(
    state: &AppState,
    db: &rusqlite::Connection,
    record: &CloudRecord,
) -> Result<Box<dyn crate::cloud_runner::CloudRunner + Send + Sync>, DomainStateError> {
    let account = match record.raw.get("accountId").and_then(Value::as_str) {
        Some(account_id) => Some(crate::accounts::launch::claude_account_argv(
            db, account_id,
        )?),
        None => None,
    };
    Ok(claude_code_on_the_web(&state.paths.home_dir, account))
}

/// What the terminal client and other callers read about a cloud session.
pub(super) fn answer(record: &CloudRecord) -> Value {
    record.to_value()
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The session's title from its task, while the title is still the launcher's placeholder.
fn title_from_task(
    repository: &DomainRepository<'_>,
    session: &Value,
    task: &str,
) -> Result<(), DomainStateError> {
    let source = session
        .pointer("/runtimeSettings/titleSource")
        .and_then(Value::as_str)
        .unwrap_or("placeholder");
    if !matches!(source, "placeholder" | "terminal-auto") {
        return Ok(());
    }
    let Some(line) = task.lines().map(str::trim).find(|line| !line.is_empty()) else {
        return Ok(());
    };
    let title: String = line.chars().take(60).collect();
    let mut update = Map::new();
    for key in ["projectId", "sessionId"] {
        update.insert(
            key.to_string(),
            session.get(key).cloned().unwrap_or_default(),
        );
    }
    update.insert("title".to_string(), json!(title));
    let mut runtime = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime.insert("titleSource".to_string(), json!("terminal-auto"));
    update.insert("runtimeSettings".to_string(), Value::Object(runtime));
    repository.update_session(&update)?;
    Ok(())
}

/// `start {projectId, sessionId, task?}`: starts the draft (or failed) session on claude.ai with
/// `task` (or the task it was created with). Blocking for as long as `claude --cloud` takes.
pub(crate) fn start_cloud_session(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project_id = text(params, "projectId").unwrap_or_default();
    let session_id = text(params, "sessionId").unwrap_or_default();
    let key = (project_id.clone(), session_id.clone());
    {
        let mut starting = STARTING
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let set = starting.get_or_insert_with(HashSet::new);
        if !set.insert(key.clone()) {
            return Err(DomainStateError::bad_request(
                "This cloud session is already starting.",
            ));
        }
    }
    let result = start_inner(state, params, &project_id, &session_id);
    if let Ok(mut starting) = STARTING.lock() {
        if let Some(set) = starting.as_mut() {
            set.remove(&key);
        }
    }
    result
}

fn start_inner(
    state: &AppState,
    params: &Map<String, Value>,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    let db = open(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let (session, mut record) = load(&repository, project_id, session_id)?;
    if !matches!(record.state, CloudState::Draft | CloudState::Failed) {
        return Ok(answer(&record));
    }
    let task = text(params, "task")
        .or_else(|| text(&record.raw, "task"))
        .ok_or_else(|| DomainStateError::bad_request("Write the task for Claude's cloud first."))?;
    let project = repository
        .get_project(project_id)?
        .ok_or_else(|| DomainStateError::not_found("The session's project does not exist."))?;
    let repo_dir = project
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| DomainStateError::bad_request("This project has no folder."))?
        .to_string();
    // A copy or a Work page ticket names its branch; otherwise claude.ai picks one and reports it
    // (remote.rs).
    let branch = record.branch.clone();
    let prompt = match &branch {
        Some(branch) => task_with_branch(&task, branch),
        None => task.clone(),
    };
    let length = prompt.chars().count();
    if length > MAX_CLOUD_PROMPT_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "The task is {length} characters; a cloud session starts with at most {MAX_CLOUD_PROMPT_CHARS}. Shorten it and try again."
        )));
    }
    let runner = runner_for(state, &db, &record)?;
    record.state = CloudState::Starting;
    record.error = None;
    record.raw.insert("task".to_string(), json!(task));
    record.raw.insert(
        "startRequestedAt".to_string(),
        json!(crate::domain::now_iso()),
    );
    title_from_task(&repository, &session, &task)?;
    // A launcher draft becomes an ordinary session once its task is sent, as a chat's first
    // message promotes a local draft (agents/drafts.rs).
    db.execute(
        "UPDATE sessions SET runtimeSettingsJson = json_remove(runtimeSettingsJson, '$.draftStatus') \
         WHERE projectId = ?1 AND sessionId = ?2",
        rusqlite::params![project_id, session_id],
    )
    .map_err(|error| internal(format!("SQLite gxserver state error: {error}")))?;
    save(state, &db, &repository, project_id, session_id, &record)?;
    // The start takes seconds to minutes; no database handle is held across it.
    drop(repository);
    drop(db);
    // A copy works on its original's branch, which GitHub already has (copy.rs).
    let on_branch = branch
        .clone()
        .filter(|_| record.raw.get("onBranch").and_then(Value::as_bool) == Some(true));
    let started = runner.start(&CloudStartRequest {
        repo_dir: Path::new(&repo_dir),
        prompt: &prompt,
        on_branch: on_branch.as_deref(),
    });
    let db = open(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let (_, mut record) = load(&repository, project_id, session_id)?;
    match started {
        Ok(cloud) => {
            record.state = CloudState::Running;
            record.url = super::record::normalized_session_url(&cloud.url).or(Some(cloud.url));
            record
                .raw
                .insert("startedAt".to_string(), json!(crate::domain::now_iso()));
        }
        Err(error) => {
            record.state = CloudState::Failed;
            record.error = Some(error);
        }
    }
    save(state, &db, &repository, project_id, session_id, &record)?;
    super::poller::wake_claude_cloud_poller();
    Ok(answer(&record))
}

/// `send {projectId, sessionId, text}`: one follow-up message to the running cloud session.
pub(crate) fn send_to_cloud_session(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project_id = text(params, "projectId").unwrap_or_default();
    let session_id = text(params, "sessionId").unwrap_or_default();
    let message = text(params, "text")
        .ok_or_else(|| DomainStateError::bad_request("There is no message to send."))?;
    let db = open(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let (_, record) = load(&repository, &project_id, &session_id)?;
    let url = record
        .url
        .clone()
        .filter(|_| record.state == CloudState::Running)
        .ok_or_else(|| {
            DomainStateError::bad_request("This cloud session is not running on claude.ai.")
        })?;
    let runner = runner_for(state, &db, &record)?;
    drop(repository);
    drop(db);
    runner
        .send(&url, &message)
        .map_err(DomainStateError::bad_request)?;
    // The row turns to working as soon as claude.ai takes the message.
    super::poller::wake_claude_cloud_poller();
    Ok(json!({ "ok": true }))
}

/// `get {projectId, sessionId}`: the record.
pub(crate) fn read_cloud_session(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project_id = text(params, "projectId").unwrap_or_default();
    let session_id = text(params, "sessionId").unwrap_or_default();
    let db = open(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let (_, record) = load(&repository, &project_id, &session_id)?;
    Ok(answer(&record))
}
