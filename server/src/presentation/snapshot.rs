use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::delayed_sends::{
    insert_delayed_send_presentation_payload, insert_delayed_send_session_projection,
};
use crate::domain::{DomainRepository, DomainStateError};

use super::*;

pub fn read_presentation_snapshot(
    db: &Connection,
    server_id: &str,
    auto_settle_after_days: Option<f64>,
    sidebar_v2_selected: bool,
) -> Result<Value, DomainStateError> {
    let repository = DomainRepository::new(db, server_id);
    let mut snapshot = project_snapshot(
        repository.list_projects()?,
        repository.list_sessions(None)?,
        read_presentation_revision(db)?,
        sidebar_v2_selected,
    );
    insert_delayed_send_presentation_payload(db, &mut snapshot)?;
    insert_session_chat_queue_presentation_payload(&mut snapshot, db);
    insert_auto_settle_window_presentation_payload(&mut snapshot, auto_settle_after_days);
    insert_portless_presentation_payload(&mut snapshot, db);
    insert_workspace_groups_presentation_payload(&mut snapshot, db)?;
    insert_sidebar_project_collections_presentation_payload(&mut snapshot, db)?;
    Ok(snapshot)
}

pub fn search_presentation_sessions(
    db: &Connection,
    server_id: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let repository = DomainRepository::new(db, server_id);
    let projects = repository.list_projects()?;
    let sessions = repository.list_sessions(None)?;
    search_sessions(projects, sessions, params)
}

pub fn list_previous_sessions(
    db: &Connection,
    server_id: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let repository = DomainRepository::new(db, server_id);
    let projects = repository.list_projects()?;
    let sessions = repository.list_sessions(None)?;
    let mut previous_params = params.clone();
    previous_params.insert("includeActive".to_string(), Value::Bool(false));
    previous_params.insert("includePrevious".to_string(), Value::Bool(true));
    search_previous_sessions(projects, sessions, &previous_params)
}

pub fn build_presentation_project_delta(
    repository: &DomainRepository<'_>,
    project_id: &str,
    delta_type: &str,
) -> Result<Value, DomainStateError> {
    let Some(project) = repository.get_project(project_id)? else {
        return Ok(json!({
            "projectId": project_id,
            "type": "projectRemoved",
        }));
    };
    if !should_include_presentation_project(&project) {
        return Ok(json!({
            "projectId": project_id,
            "type": "projectRemoved",
        }));
    }
    Ok(json!({
        "domainProject": project,
        "project": project_presentation_project(&project),
        "type": delta_type,
    }))
}

pub fn build_presentation_session_delta(
    db: &Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    let project = repository.get_project(project_id)?;
    let session = repository.get_session(project_id, session_id)?;
    let (Some(project), Some(session)) = (project, session) else {
        return Ok(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "type": "sessionRemoved",
        }));
    };
    if !should_include_presentation_project(&project)
        || !should_include_presentation_session(&session)
    {
        return Ok(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "type": "sessionRemoved",
        }));
    }
    let mut presentation_session = project_presentation_session(
        &project,
        &default_group_id(project_id),
        &session,
        &now_iso(),
    );
    insert_delayed_send_session_projection(db, &mut presentation_session)?;
    insert_session_chat_queue_session_projection(
        &mut presentation_session,
        db,
        project_id,
        session_id,
    );
    Ok(json!({
        "session": presentation_session,
        "type": "sessionPresentationChanged",
    }))
}

pub fn increment_presentation_revision(db: &Connection) -> Result<i64, DomainStateError> {
    /*
    Independent request connections must allocate distinct revisions. A single
    UPSERT statement holds SQLite's writer serialization through the increment
    and returns that statement's value, avoiding the old read/then-upsert race.
    Missing, invalid, or non-positive legacy values retain the prior effective
    "revision 1, then increment to 2" behavior.
    */
    db.query_row(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, '2', ?2)
        ON CONFLICT(key) DO UPDATE SET
          value = CASE
            WHEN CAST(metadata.value AS INTEGER) > 0
              THEN CAST(metadata.value AS INTEGER) + 1
            ELSE 2
          END,
          updatedAt = excluded.updatedAt
        RETURNING CAST(value AS INTEGER)
        "#,
        rusqlite::params!["presentationRevision", now_iso()],
        |row| row.get::<_, i64>(0),
    )
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite presentation error: {error}"),
    })
}

pub fn read_presentation_revision(db: &Connection) -> Result<i64, DomainStateError> {
    let value = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            ["presentationRevision"],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite presentation error: {error}"),
        })?;
    Ok(value
        .and_then(|text| text.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1))
}

pub(crate) fn project_snapshot(
    projects: Vec<Value>,
    sessions: Vec<Value>,
    revision: i64,
    sidebar_v2_selected: bool,
) -> Value {
    let generated_at = now_iso();
    let mut projects_sorted = projects;
    projects_sorted.sort_by_key(project_sort_key);
    let mut presentation_projects = Vec::new();
    let mut groups = Vec::new();
    let mut presentation_sessions = Vec::new();
    for project in projects_sorted {
        /*
        CDXC:GPUIRecentProjects 2026-06-24-12:27:
        Parked Recent Projects remain durable gxserver projects but are not
        active sidebar presentation groups. The only sidebar drawer source for
        them is `/api/listRecentProjects`, which returns explicit path-bearing
        rows instead of deriving recency from inactive sessions or labels.
        */
        if !should_include_presentation_project(&project) {
            continue;
        }
        let project_id = string_field(&project, "projectId").unwrap_or_default();
        let group_id = default_group_id(&project_id);
        let mut project_sessions = sessions
            .iter()
            .filter(|session| {
                string_field(session, "projectId").as_deref() == Some(project_id.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        project_sessions = select_presentation_sessions(project_sessions);
        project_sessions.sort_by_key(session_sort_key);
        let project_presentation_sessions = project_sessions
            .into_iter()
            .map(|session| {
                project_presentation_session(&project, &group_id, &session, &generated_at)
            })
            .collect::<Vec<_>>();
        groups.push(json!({
            "groupId": group_id,
            "projectId": project_id,
            "sessionIds": project_presentation_sessions
                .iter()
                .filter_map(|session| string_field(session, "sessionId"))
                .collect::<Vec<_>>(),
            "sortKey": format!("{}:active", project_sort_key(&project)),
            "title": "Active",
        }));
        presentation_projects.push(project_presentation_project(&project));
        presentation_sessions.extend(project_presentation_sessions);
    }
    json!({
        "capabilities": presentation_capabilities(sidebar_v2_selected),
        "generatedAt": generated_at,
        "groups": groups,
        "projects": presentation_projects,
        "revision": revision,
        "sessions": presentation_sessions,
    })
}

/*
CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
Capabilities are machine-scoped: a GPUI sidebar merges snapshots from several
gxservers, and an older remote daemon simply omits this object. Sidebar V2 hides
settle/snooze affordances and classifies nothing as settled for those machines
instead of inventing lifecycle out of derived data.
*/
pub fn presentation_capabilities(sidebar_v2_selected: bool) -> Value {
    json!({
        /*
        CDXC:SidebarV2GitStatus 2026-07-29-00:00:
        `sessionGitStatus` promises the `gitStatus` FIELD exists on this
        machine's sessions when their cwd is a git checkout, not that any
        particular session has one. Sidebar V2 uses it to decide whether an
        empty card row means "no git state" or "this daemon is too old to know".

        CDXC:SidebarV2DataGate 2026-07-29:
        That promise is exactly what the version gate takes away, so the flag
        follows the gate rather than the build: a daemon configured for Sidebar
        V1 runs no git/`gh` probe, so it has no git data to give and says so.
        The alternative — advertising `true` from a daemon that will never probe
        — turns the flag into a lie a remote V2 client cannot detect, and its
        cards would wait forever on branch/± /PR data that is not coming.
        Answering `false` instead lands in the path V2 already has (and tests)
        for a daemon too old to probe: the row renders byte-identically to a
        session with no git state, and the client also stops rendering any stale
        `gitStatus` this daemon still carries in its process cache from an
        earlier V2 stretch — which is why those cached values are left published
        rather than stripped session by session.

        NOTE for remote machines: a headless gxserver has no sidebar and so no
        `native-sidebar-settings.json`, which reads as V1. Such a daemon now
        publishes no git data and advertises none; giving remote daemons a way to
        opt in is deliberately left as its own decision, not smuggled in here as
        a "remote means V2" exception.
        */
        "sessionGitStatus": sidebar_v2_selected,
        "sessionSettlement": true,
        "sessionSnooze": true,
        /*
        CDXC:SidebarV2Worktrees 2026-07-29-00:00:
        `worktreeSessions` promises `/api/createWorktreeSession` and
        `/api/removeSessionWorktree` exist on this machine, so Sidebar V2 can
        offer "New worktree session…" and the worktree cleanup prompt for its
        projects instead of failing the call on an older daemon.
        */
        "worktreeSessions": true,
    })
}

pub fn should_include_presentation_project(project: &Value) -> bool {
    /*
    CDXC:ProjectVisibility 2026-06-30-21:23:
    Active sidebar/project inventory is gxserver-owned. Parked Recent Projects and hidden system carrier projects stay durable for domain/session ownership, but presentation snapshots and deltas must remove them so macOS, GPUI, CLI, and React Native Android do not independently invent visibility filters.
    */
    project.get("isRecentProject").and_then(Value::as_bool) != Some(true)
        && string_field(project, "visibility").as_deref() != Some("hidden")
        && string_field(project, "systemKind").as_deref() != Some("remoteAttachCarrier")
}

