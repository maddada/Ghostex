//! Cloud sessions started outside New Thread (the Work page's Start in cloud, and the records it
//! kept on this computer before cloud sessions had rows) become ordinary cloud sessions in the
//! sidebar, linked to their ticket.
//!
//! CDXC:ClaudeCloud 2026-10-23 DECISION:
//! User: Work page cloud sessions get "the same cloud row, with the ticket chip". Every cloud session Ghostex starts is a sidebar session; the Work page keeps listing it on the ticket from `work-cloud-sessions.json`, whose records now also name their sidebar session.

use serde_json::{json, Map, Value};

use super::record::{normalized_session_url, session_cloud_record, CloudState, RECORD_KEY};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// A cloud session that already exists on claude.ai.
pub(crate) struct StartedCloudSession {
    pub(crate) project_id: String,
    pub(crate) url: String,
    pub(crate) branch: Option<String>,
    pub(crate) task: Option<String>,
    pub(crate) title: String,
    /// The Work page's ticket key and label (`SPX-1245`, `owner/repo#12` with `#12` or `PR #12`).
    pub(crate) ticket: Option<(String, String)>,
    pub(crate) account: Map<String, Value>,
    /// GitHub's current state is taken in without a notification (a migrated record).
    pub(crate) quiet: bool,
}

/// `runtimeSettings.workLinks` for a ticket key and its label: the chip a work-mode session shows.
pub(crate) fn work_links_for_ticket(ticket: &str, label: &str) -> Option<Map<String, Value>> {
    let mut links = Map::new();
    if let Some(number) = label
        .strip_prefix("PR #")
        .and_then(|n| n.parse::<u64>().ok())
    {
        links.insert("pullRequest".to_string(), json!(number));
    } else if let Some(number) = label.strip_prefix('#').and_then(|n| n.parse::<u64>().ok()) {
        links.insert("githubIssues".to_string(), json!([number]));
    } else if let Some(identifier) = crate::work_mode::normalize_linear_identifier(ticket) {
        links.insert("linearIssues".to_string(), json!([identifier]));
    } else {
        return None;
    }
    Some(links)
}

/// Adds a cloud session row for a session already running on claude.ai; returns its
/// `(projectId, sessionId)`. The row goes to the main checkout's project.
pub(crate) fn add_started_cloud_session(
    state: &AppState,
    started: StartedCloudSession,
) -> Result<(String, String), DomainStateError> {
    let url = normalized_session_url(&started.url).ok_or_else(|| {
        DomainStateError::bad_request("That is not a claude.ai/code session link.")
    })?;
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    // One row per cloud session. Only a row whose `claudeCloud` is an object carries a cloud
    // record, so SQLite picks those rows instead of every row being hydrated; this runs for each
    // unlinked Work page record at every start.
    if let Some(existing) = repository
        .list_sessions_with_runtime_object(RECORD_KEY)?
        .into_iter()
        .find(|session| {
            session_cloud_record(session).is_some_and(|record| record.url.as_deref() == Some(&url))
        })
    {
        let text = |key: &str| {
            existing
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        return Ok((text("projectId"), text("sessionId")));
    }
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(started.project_id));
    params.insert("agentId".to_string(), json!(super::CLAUDE_CLOUD_AGENT_ID));
    params.insert("title".to_string(), json!(started.title));
    let params = super::cloud_create_params(&repository, &params)?.unwrap_or(params);
    let project = repository.resolve_create_session_project(&params)?;
    let mut create =
        crate::agents::create_agent_session_params_for_project(&db, &project, &params)?;
    let mut runtime = create
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut record = runtime
        .get(RECORD_KEY)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    // The account the session really started on, not the one a new session would pick now.
    record.remove("accountId");
    record.remove("accountName");
    for (key, value) in started.account {
        record.insert(key, value);
    }
    record.insert("state".to_string(), json!(CloudState::Running.as_str()));
    record.insert("url".to_string(), json!(url));
    record.insert("startedAt".to_string(), json!(crate::domain::now_iso()));
    if let Some(branch) = started.branch {
        record.insert("branch".to_string(), json!(branch));
    }
    if let Some(task) = started.task {
        record.insert("task".to_string(), json!(task));
    }
    if let Some((ticket, label)) = &started.ticket {
        record.insert("ticket".to_string(), json!(ticket));
        record.insert("ticketLabel".to_string(), json!(label));
        if let Some(links) = work_links_for_ticket(ticket, label) {
            runtime.insert("workLinks".to_string(), Value::Object(links));
        }
    }
    if started.quiet {
        record.insert("quietFirstLook".to_string(), json!(true));
    }
    runtime.insert(RECORD_KEY.to_string(), Value::Object(record));
    runtime.insert("titleSource".to_string(), json!("user"));
    create.insert("runtimeSettings".to_string(), Value::Object(runtime));
    let session = repository.create_session(&create, false)?;
    let text = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text("projectId"), text("sessionId"));
    let _ = crate::server::schedule_presentation_session_delta(
        state,
        &db,
        &repository,
        &project_id,
        &session_id,
    );
    super::wake_claude_cloud_poller();
    Ok((project_id, session_id))
}

/// Gives every Work page cloud record kept before cloud sessions had rows its sidebar row, once.
pub(crate) fn migrate_work_cloud_records(state: &AppState) {
    for record in crate::work_mode::unlinked_cloud_records(&state.paths) {
        let text = |key: &str| {
            record
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let (Some(url), Some(project_id)) = (text("sessionUrl"), text("projectId")) else {
            continue;
        };
        let ticket = text("ticket").zip(text("ticketLabel"));
        let title = match &ticket {
            Some((_, label)) => format!("{label} in Claude's cloud"),
            None => "Claude cloud session".to_string(),
        };
        let added = add_started_cloud_session(
            state,
            StartedCloudSession {
                project_id,
                url: url.clone(),
                branch: text("branch"),
                task: None,
                title,
                ticket,
                account: Map::new(),
                quiet: true,
            },
        );
        // A record whose project is gone stays unlinked and is tried again next start.
        if let Ok((project_id, session_id)) = added {
            let _ = crate::work_mode::link_cloud_record_to_session(
                &state.paths,
                &url,
                &project_id,
                &session_id,
            );
        }
    }
}
