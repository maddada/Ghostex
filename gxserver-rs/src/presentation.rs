use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    delayed_sends::{
        insert_delayed_send_presentation_payload, insert_delayed_send_session_projection,
    },
    domain::{DomainRepository, DomainStateError},
    ids::is_gxserver_session_id,
    portless::read_portless_presentation_payload,
    session_status::{
        effective_agent_activity_value, effective_working_started_at, meaningful_activity_at,
    },
};

/*
CDXC:GxserverRustPort 2026-06-14-22:12:
Phase 3 presentation is a read-only projection over the durable project/session repository. Keep it metadata-only and camelCase so sidebar inventory can compare Rust and TypeScript without moving pane layout, terminal text, prompts, or other client-local/private state into gxserver.
*/
/*
CDXC:SidebarV2LogicalProjects 2026-07-29-00:00:
`auto_settle_after_days` is the inactivity window THIS daemon's auto-settle sweep
resolves right now, in days, or `None` when the sweep settles nothing (Sidebar V1
is selected, or the user switched auto-settle off). It is threaded in rather than
read here so the snapshot stays a pure projection and so the caller can do the
settings read OFF the presentation sequencer lock; callers must resolve it with
`session_lifecycle::read_sweep_auto_settle_after_days`, the same function the
sweep itself calls, so the published window and the applied window can never be
two different rules.
*/
/*
CDXC:SidebarV2DataGate 2026-07-29:
`sidebar_v2_selected` is the same `sidebarVersion` gate the git-status and
`origin`-remote passes run under, threaded in for the same
reason and from the same place as the window above (callers must resolve it with
`session_lifecycle::read_sidebar_v2_selected`, off the presentation sequencer).
It decides ONE thing in the snapshot: the `sessionGitStatus` capability, so a
daemon that has switched its probes off cannot advertise git data it will never
produce. See `presentation_capabilities`.
*/
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

fn project_snapshot(
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

fn insert_workspace_groups_presentation_payload(
    snapshot: &mut Value,
    db: &Connection,
) -> Result<(), DomainStateError> {
    /*
    CDXC:WorkspaceSessionGroups 2026-07-12-00:00:
    Mobile and CLI consumers read the GPUI-authored named-group overlay from the
    same presentation snapshot they already poll, so grouped ordering needs no
    extra round trip.
    */
    let groups = crate::workspace_groups::read_workspace_session_groups(db)?;
    if let Some(snapshot) = snapshot.as_object_mut() {
        snapshot.insert("workspaceGroups".to_string(), groups);
    }
    Ok(())
}

fn insert_sidebar_project_collections_presentation_payload(
    snapshot: &mut Value,
    db: &Connection,
) -> Result<(), DomainStateError> {
    /*
    CDXC:SidebarProjectCollections 2026-07-18-00:00:
    Mobile and CLI consumers read the colored project-collection overlay from
    the same presentation snapshot they already poll, so grouped project
    rendering needs no extra round trip.
    */
    let collections = crate::sidebar_project_collections::read_sidebar_project_collections(db)?;
    if let Some(snapshot) = snapshot.as_object_mut() {
        snapshot.insert("sidebarProjectCollections".to_string(), collections);
    }
    Ok(())
}

fn insert_auto_settle_window_presentation_payload(
    snapshot: &mut Value,
    auto_settle_after_days: Option<f64>,
) {
    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29-00:00:
    One sidebar renders rows from several daemons and each daemon reads its OWN
    `sidebarAutoSettleAfterDays`, so a client that applied the local window to
    every machine would park remote sessions the remote daemon still considers
    active (the recorded P2 minor). The key is therefore ALWAYS published — an
    explicit `null` says "this daemon settles nothing", while an ABSENT key can
    only mean a daemon too old to state its window.
    */
    if let Some(snapshot) = snapshot.as_object_mut() {
        snapshot.insert(
            "autoSettleAfterDays".to_string(),
            match auto_settle_after_days {
                Some(days) => auto_settle_window_value(days),
                None => Value::Null,
            },
        );
    }
}

/*
The window is carried as an f64 because the sweep computes with one, but the
setting users actually write is a whole number of days. Publishing `3` rather
than `3.0` round-trips their value byte for byte, which keeps the wire readable
and comparable; a fractional window (a test or a power user's `1.5`) publishes as
the float it is.
*/
fn auto_settle_window_value(days: f64) -> Value {
    if days.fract() == 0.0 && days.abs() < 9_007_199_254_740_992.0 {
        return json!(days as i64);
    }
    json!(days)
}

fn insert_portless_presentation_payload(snapshot: &mut Value, db: &Connection) {
    if let Some(snapshot) = snapshot.as_object_mut() {
        snapshot.insert(
            "portless".to_string(),
            serde_json::to_value(read_portless_presentation_payload(db))
                .expect("Portless presentation payload serializes"),
        );
    }
}

fn project_presentation_project(project: &Value) -> Value {
    let project_id = string_field(project, "projectId").unwrap_or_default();
    let mut output = Map::new();
    output.insert("createdAt".to_string(), value_field(project, "createdAt"));
    output.insert(
        "groupIds".to_string(),
        json!([default_group_id(&project_id)]),
    );
    if let Some(git_config) = project_presentation_git_config(project) {
        output.insert("gitConfig".to_string(), git_config);
    }
    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29-00:00:
    Sidebar V2 merges the same repository across machines by its `origin` remote.
    This is a READ of the background probe cache keyed by the project's family
    root path (`project_git_remote_key`) — never a probe — so building a snapshot
    stays a pure in-memory projection. `insert_present_value` is deliberate: a
    repository with no `origin` publishes an explicit `null`, while a path the
    pass has not reached, a non-git folder, and an older remote daemon all
    publish no key at all.
    */
    let git_remote_key = crate::project_git_remote::project_git_remote_key(project);
    insert_present_value(
        &mut output,
        "gitRemoteOriginUrl",
        git_remote_key.as_deref().and_then(|path| {
            crate::project_git_remote::published_project_git_remote_origin_url(path)
        }),
    );
    /*
    The repository ROOT from the same cache entry. Sidebar V2's "Repository +
    path" mode measures each project's path against this to tell two
    sub-projects of one monorepo apart; with no root published the mode has
    nothing to measure and silently degrades to plain repository merging.

    There is no `null` state here: a probe that could not resolve a root simply
    publishes no key, exactly like an unprobed or non-git path.
    */
    insert_present_value(
        &mut output,
        "gitRepositoryRootPath",
        git_remote_key.as_deref().and_then(|path| {
            crate::project_git_remote::published_project_git_repository_root_path(path)
        }),
    );
    /*
    CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
    The icon the project itself ships through a favicon, app icon, or
    the icon its HTML entry point declares — discovered server-side and published
    as a data URL. Another pure cache READ (`project_icon`), keyed on the same
    family root as the remote probe so a worktree inherits its parent checkout's
    icon.

    Two states only: a data URL, or an absent key for a project that has not been
    probed yet, has no discoverable icon, or is served by an older daemon. It is
    deliberately SEPARATE from the client-side `iconDataUrl` overlay (the icon a
    user attached by hand) so the client can RANK the two: an uploaded image
    outranks this, and this outranks a typed Tabler glyph.
    */
    insert_present_value(
        &mut output,
        "discoveredIconDataUrl",
        git_remote_key
            .as_deref()
            .and_then(crate::project_icon::published_project_icon_data_url),
    );
    output.insert("isFavorite".to_string(), value_field(project, "isFavorite"));
    output.insert("isPinned".to_string(), value_field(project, "isPinned"));
    insert_optional_value(&mut output, "path", project.get("path").cloned());
    output.insert(
        "pathState".to_string(),
        Value::String(
            crate::domain::project_path_state(project)
                .as_str()
                .to_string(),
        ),
    );
    output.insert("projectId".to_string(), Value::String(project_id.clone()));
    output.insert(
        "sortKey".to_string(),
        Value::String(project_sort_key(project)),
    );
    output.insert("title".to_string(), value_field(project, "name"));
    output.insert("updatedAt".to_string(), value_field(project, "updatedAt"));
    insert_optional_value(&mut output, "worktree", project.get("worktree").cloned());
    Value::Object(output)
}

fn project_presentation_git_config(project: &Value) -> Option<Value> {
    /*
    CDXC:GPUIRemoteGit 2026-06-24-18:22:
    Presentation may expose only Git preference keys needed by reused sidebar controls. Do not forward arbitrary project gitConfig values, command text, paths, URLs, branch names, tokens, or daemon output through remote sidebar presentation.
    */
    let source = project.get("gitConfig")?.as_object()?;
    let mut output = Map::new();
    if let Some(confirm_commit) = source.get("confirmCommit").and_then(Value::as_bool) {
        output.insert("confirmCommit".to_string(), Value::Bool(confirm_commit));
    }
    if let Some(generate_commit_body) = source.get("generateCommitBody").and_then(Value::as_bool) {
        output.insert(
            "generateCommitBody".to_string(),
            Value::Bool(generate_commit_body),
        );
    }
    if let Some(primary_action) = source
        .get("primaryAction")
        .and_then(Value::as_str)
        .filter(|value| is_presentation_git_action(*value))
    {
        output.insert(
            "primaryAction".to_string(),
            Value::String(primary_action.to_string()),
        );
    }
    (!output.is_empty()).then(|| Value::Object(output))
}

fn is_presentation_git_action(value: &str) -> bool {
    matches!(
        value,
        "commit" | "push" | "pr" | "syncRemote" | "syncMain" | "multiRelease" | "release"
    )
}

fn project_presentation_session(
    project: &Value,
    group_id: &str,
    session: &Value,
    generated_at: &str,
) -> Value {
    let title = project_session_title(session);
    let activity = presentation_activity(session, generated_at);
    let lifecycle_state = effective_lifecycle_state(session);
    let subtitle = snapshot_subtitle(project, session);
    let mut output = Map::new();
    output.insert(
        "actions".to_string(),
        presentation_actions(session, &activity),
    );
    output.insert("activity".to_string(), Value::String(activity.clone()));
    insert_optional_string(
        &mut output,
        "agentName",
        read_runtime_text(session, "agentName")
            .or_else(|| string_field(session, "agentId"))
            .filter(|value| !value.is_empty()),
    );
    if let Some(agent_id) = string_field(session, "agentId").filter(|value| !value.is_empty()) {
        output.insert("agentId".to_string(), Value::String(agent_id.clone()));
        output.insert("agentIcon".to_string(), Value::String(agent_id));
    }
    insert_optional_string(
        &mut output,
        "agentSessionId",
        read_runtime_text(session, "agentSessionId"),
    );
    insert_optional_string(
        &mut output,
        "agentSessionPath",
        read_runtime_text(session, "agentSessionPath"),
    );
    if activity == "attention" {
        output.insert(
            "attention".to_string(),
            attention_state(session, generated_at),
        );
    }
    /*
    CDXC:WebCommandPaneActions 2026-08-08:
    Command-pane clients need the stable saved Action id to find the daemon
    session that already owns that Action. Publish only that identifier;
    command text and launch settings remain outside presentation snapshots.
    */
    insert_optional_string(&mut output, "commandId", string_field(session, "commandId"));
    output.insert("createdAt".to_string(), value_field(session, "createdAt"));
    insert_optional_js_truthy_value(&mut output, "cwd", session.get("cwd").cloned());
    /*
    CDXC:SidebarV2GitStatus 2026-07-29-00:00:
    Sidebar V2's card row reads branch / +n −n / PR badge from server-owned state.
    This is a READ of the background probe cache keyed by the session cwd — never
    a probe — so building a snapshot stays a pure in-memory projection. A cwd the
    background pass has not reached yet, a cwd outside any repository, and an
    older remote daemon all publish the same thing: no `gitStatus` key at all.

    CDXC:SidebarV2GitStatus 2026-07-30 (effective cwd):
    The lookup key is the session's EFFECTIVE cwd — its own `cwd`, else the
    project's path — because agent sessions carry no cwd by design and run in the
    project root. The published `cwd` field above stays raw on purpose: V2 uses it
    to tell a managed worktree checkout apart from a project-root session.
    */
    insert_optional_value(
        &mut output,
        "gitStatus",
        crate::session_git_status::effective_session_git_cwd(session, Some(project))
            .and_then(|cwd| crate::session_git_status::published_session_git_status(&cwd)),
    );
    output.insert("groupId".to_string(), Value::String(group_id.to_string()));
    output.insert("isFavorite".to_string(), Value::Bool(is_favorite(session)));
    /*
    CDXC:GxserverSessionTitle 2026-07-02-15:10:
    gxserver stages and submits first-prompt title commands itself through zmx, so presentation no longer carries a client Enter-submit flag. `isGeneratingFirstPromptTitle` stays published for client loading chrome only.
    */
    output.insert(
        "isGeneratingFirstPromptTitle".to_string(),
        Value::Bool(
            read_runtime_text(session, "gxserverFirstPromptAutoTitleStatus").as_deref()
                == Some("running"),
        ),
    );
    output.insert("isPinned".to_string(), value_field(session, "isPinned"));
    merge_object(&mut output, title);
    output.insert("kind".to_string(), value_field(session, "kind"));
    output.insert(
        "lastActiveAt".to_string(),
        Value::String(last_active_at(session)),
    );
    output.insert("lifecycleState".to_string(), Value::String(lifecycle_state));
    /*
    CDXC:ActivitySuppressionPolicy 2026-07-29-12:00:
    `meaningfulActivityAt` is the recency clients sort by: it ignores working
    blips shorter than the meaningful threshold and advances live while a
    session is meaningfully working. `workingStartedAt` lets sort layers tell
    whether the current working stint has qualified yet. `lastActiveAt` stays
    raw for auto-sleep and Last Active labels.
    */
    output.insert(
        "meaningfulActivityAt".to_string(),
        Value::String(session_meaningful_activity_at(session, generated_at)),
    );
    output.insert(
        "providerSessionState".to_string(),
        Value::String(provider_session_state(session)),
    );
    output.insert("projectId".to_string(), value_field(session, "projectId"));
    output.insert("sessionId".to_string(), value_field(session, "sessionId"));
    if let Some(provider) = search_session_persistence_provider(session) {
        output.insert(
            "sessionPersistenceProvider".to_string(),
            Value::String(provider),
        );
    }
    insert_optional_js_truthy_value(
        &mut output,
        "sessionTag",
        session.get("sessionTag").cloned(),
    );
    /*
    CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
    Sidebar V2's settled/snoozed shelves read server-owned lifecycle state, so
    presentation publishes it verbatim. Absent keys mean "never settled / never
    snoozed" — the same shape a pre-migration state.db and an older remote
    daemon produce. `settledOverrideAt` stays server-internal: it only exists so
    the sweep can decide when real activity has outrun an override.
    */
    insert_optional_string(&mut output, "settledAt", string_field(session, "settledAt"));
    insert_optional_string(
        &mut output,
        "settledOverride",
        string_field(session, "settledOverride"),
    );
    insert_present_value(
        &mut output,
        "sidebarOrder",
        session.get("sidebarOrder").cloned(),
    );
    insert_optional_string(&mut output, "snoozedAt", string_field(session, "snoozedAt"));
    insert_optional_string(
        &mut output,
        "snoozedUntil",
        string_field(session, "snoozedUntil"),
    );
    output.insert(
        "sortKey".to_string(),
        Value::String(session_sort_key(session)),
    );
    insert_optional_string(&mut output, "subtitle", subtitle);
    output.insert("surface".to_string(), value_field(session, "surface"));
    insert_optional_value(
        &mut output,
        "titleObservation",
        title_observation_state(session),
    );
    output.insert(
        "tooltip".to_string(),
        Value::String(build_session_tooltip(
            project,
            session,
            output
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )),
    );
    output.insert("updatedAt".to_string(), value_field(session, "updatedAt"));
    output.insert(
        "visibleInSidebarByDefault".to_string(),
        Value::Bool(
            string_field(session, "surface").as_deref() == Some("workspace") && is_active(session),
        ),
    );
    insert_optional_string(
        &mut output,
        "workingStartedAt",
        session_effective_working_started_at(session, generated_at),
    );
    output.insert("zmxName".to_string(), value_field(session, "zmxName"));
    Value::Object(output)
}

pub(crate) fn session_meaningful_activity_at(session: &Value, generated_at: &str) -> String {
    let generated_at_ms = parse_iso_ms(generated_at).unwrap_or_else(now_ms);
    meaningful_activity_at(session_agent_activity(session), generated_at_ms)
        .unwrap_or_else(|| last_active_at(session))
}

pub(crate) fn session_effective_working_started_at(
    session: &Value,
    generated_at: &str,
) -> Option<String> {
    let generated_at_ms = parse_iso_ms(generated_at).unwrap_or_else(now_ms);
    effective_working_started_at(session_agent_activity(session), generated_at_ms)
}

fn session_agent_activity(session: &Value) -> Option<&Value> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"))
}

fn search_session_persistence_provider(session: &Value) -> Option<String> {
    let value = read_session_persistence_provider(session)?;
    matches!(value.as_str(), "tmux" | "zmx" | "zellij").then_some(value)
}

fn search_session_persistence_name(session: &Value, provider: &str) -> Option<String> {
    if provider == "zmx" {
        return read_provider_trimmed_text(session, "zmxName")
            .or_else(|| string_field(session, "zmxName"));
    }
    read_provider_trimmed_text(session, "providerName")
        .or_else(|| read_runtime_text(session, "sessionPersistenceName"))
}

fn read_session_persistence_provider(session: &Value) -> Option<String> {
    read_runtime_text(session, "sessionPersistenceProvider")
        .or_else(|| read_provider_trimmed_text(session, "provider"))
}

/*
CDXC:GxserverPresentation 2026-06-22-06:36:
Presentation snapshots are active-focused state, not full stopped history. Match TypeScript by keeping all active sessions, capping explicitly pinned/favorite/tagged stopped rows to the first 20 per project by presentation sort key, and treating null or empty tags as absent.
*/
fn select_presentation_sessions(sessions: Vec<Value>) -> Vec<Value> {
    const RECENT_STOPPED_LIMIT_PER_PROJECT: usize = 20;
    let mut active = Vec::new();
    let mut pinned_stopped = Vec::new();
    for session in sessions {
        if is_active(&session) {
            active.push(session);
        } else if should_include_presentation_session(&session) {
            pinned_stopped.push(session);
        }
    }
    pinned_stopped.sort_by_key(session_sort_key);
    active.extend(
        pinned_stopped
            .into_iter()
            .take(RECENT_STOPPED_LIMIT_PER_PROJECT),
    );
    active
}

/*
CDXC:GxserverPresentationSearch 2026-06-22-06:27:
Search parity with TypeScript depends on JavaScript-like parameter truthiness, Unicode lowercasing, and title trust filters because Previous Sessions uses these metadata rows as its restore surface. Keep malformed or non-restorable titles out of search history instead of letting generic paths, commands, or G-session IDs become previous-session results.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
enum SearchProjectIdFilter {
    Any,
    Matches(String),
    MatchesNothing,
}

impl SearchProjectIdFilter {
    fn matches(&self, session: &Value) -> bool {
        match self {
            Self::Any => true,
            Self::Matches(project_id) => {
                string_field(session, "projectId").as_deref() == Some(project_id.as_str())
            }
            Self::MatchesNothing => false,
        }
    }
}

fn normalize_search_query(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_lowercase()
}

fn normalize_project_id_filter(value: Option<&Value>) -> SearchProjectIdFilter {
    match value {
        None | Some(Value::Null) => SearchProjectIdFilter::Any,
        Some(Value::String(project_id)) if project_id.is_empty() => SearchProjectIdFilter::Any,
        Some(Value::String(project_id)) => SearchProjectIdFilter::Matches(project_id.clone()),
        Some(Value::Bool(false)) => SearchProjectIdFilter::Any,
        Some(Value::Bool(true)) => SearchProjectIdFilter::MatchesNothing,
        Some(Value::Number(number)) => match number.as_f64() {
            Some(0.0) => SearchProjectIdFilter::Any,
            Some(_) => SearchProjectIdFilter::MatchesNothing,
            None => SearchProjectIdFilter::MatchesNothing,
        },
        Some(Value::Array(_) | Value::Object(_)) => SearchProjectIdFilter::MatchesNothing,
    }
}

fn normalize_session_tags(value: Option<&Value>) -> Result<Vec<&str>, DomainStateError> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => {
            let mut tags = Vec::new();
            for item in items {
                let Some(tag) = item.as_str() else {
                    continue;
                };
                if !tags.contains(&tag) {
                    tags.push(tag);
                }
            }
            Ok(tags)
        }
        Some(_) => Err(DomainStateError {
            code: "internalError",
            message: "values?.filter is not a function".to_string(),
        }),
    }
}

fn search_sessions(
    projects: Vec<Value>,
    sessions: Vec<Value>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let limit = normalize_limit(params.get("limit"));
    let offset = normalize_cursor(params.get("cursor"));
    let include_active = params.get("includeActive").and_then(Value::as_bool) != Some(false);
    let include_previous = params.get("includePrevious").and_then(Value::as_bool) != Some(false);
    let query = normalize_search_query(params.get("query"));
    let project_id_filter = normalize_project_id_filter(params.get("projectId"));
    let tags = normalize_session_tags(params.get("sessionTags"))?;
    let mut candidates = sessions
        .into_iter()
        .filter(|session| project_id_filter.matches(session))
        .filter(|session| session_matches_tag_filters(session, &tags))
        .filter(|session| {
            let active = is_active(session);
            (active && include_active) || (!active && include_previous)
        })
        .filter_map(|session| {
            let project = projects.iter().find(|project| {
                string_field(project, "projectId") == string_field(&session, "projectId")
            });
            match_session(project, &session, &query).map(|matched| (session, matched))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|(left, _), (right, _)| {
        last_active_at(right)
            .cmp(&last_active_at(left))
            .then_with(|| string_field(left, "sessionId").cmp(&string_field(right, "sessionId")))
    });
    let total = candidates.len();
    let page = candidates
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(session, matched)| {
            let project = projects.iter().find(|project| {
                string_field(project, "projectId") == string_field(&session, "projectId")
            });
            search_result(project, &session, matched)
        })
        .collect::<Vec<_>>();
    let mut output = Map::new();
    if offset + limit < total {
        output.insert(
            "cursor".to_string(),
            Value::String((offset + limit).to_string()),
        );
    }
    output.insert("results".to_string(), Value::Array(page));
    Ok(Value::Object(output))
}

fn search_previous_sessions(
    projects: Vec<Value>,
    sessions: Vec<Value>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    /*
    CDXC:PreviousSessions 2026-06-19-14:30:
    Rust listPreviousSessions must be the same previous-only restore surface as TypeScript: exclude active rows and command-pane sessions, keep pinned/favorite/tagged history, return closedAt, and rank by provider close time instead of last activity or metadata edits.
    */
    let limit = normalize_limit(params.get("limit"));
    let offset = normalize_cursor(params.get("cursor"));
    let include_active = params.get("includeActive").and_then(Value::as_bool) != Some(false);
    let include_previous = params.get("includePrevious").and_then(Value::as_bool) != Some(false);
    let query = normalize_search_query(params.get("query"));
    let project_id_filter = normalize_project_id_filter(params.get("projectId"));
    let tags = normalize_session_tags(params.get("sessionTags"))?;
    let mut candidates = sessions
        .into_iter()
        .filter(is_previous_session_history_candidate)
        .filter(|session| project_id_filter.matches(session))
        .filter(|session| session_matches_tag_filters(session, &tags))
        .filter(|session| {
            let active = is_active(session);
            (active && include_active) || (!active && include_previous)
        })
        .filter_map(|session| {
            let project = projects.iter().find(|project| {
                string_field(project, "projectId") == string_field(&session, "projectId")
            });
            match_session(project, &session, &query).map(|matched| (session, matched))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|(left, _), (right, _)| {
        previous_session_closed_at(right)
            .cmp(&previous_session_closed_at(left))
            .then_with(|| string_field(left, "sessionId").cmp(&string_field(right, "sessionId")))
    });
    let total = candidates.len();
    let page = candidates
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(session, matched)| {
            let project = projects.iter().find(|project| {
                string_field(project, "projectId") == string_field(&session, "projectId")
            });
            let mut result = search_result(project, &session, matched);
            if let Some(output) = result.as_object_mut() {
                output.insert(
                    "closedAt".to_string(),
                    Value::String(previous_session_closed_at(&session)),
                );
            }
            result
        })
        .collect::<Vec<_>>();
    let mut output = Map::new();
    if offset + limit < total {
        output.insert(
            "cursor".to_string(),
            Value::String((offset + limit).to_string()),
        );
    }
    output.insert("results".to_string(), Value::Array(page));
    Ok(Value::Object(output))
}

fn is_previous_session_history_candidate(session: &Value) -> bool {
    if string_field(session, "surface").as_deref() != Some("workspace") {
        return false;
    }
    if is_active(session) {
        return false;
    }
    if session.get("isPinned").and_then(Value::as_bool) == Some(true)
        || is_favorite(session)
        || session_tag_is_truthy(session)
    {
        return true;
    }
    if string_field(session, "lifecycleState").as_deref() != Some("stopped") {
        return false;
    }
    project_session_title(session)
        .get("trustedResumeTitle")
        .is_some()
}

fn previous_session_closed_at(session: &Value) -> String {
    let provider_closed_at = if string_field(session, "lifecycleState").as_deref()
        == Some("stopped")
        && read_provider_trimmed_text(session, "lifecycleState").as_deref() == Some("missing")
    {
        read_provider_trimmed_text(session, "probedAt")
    } else {
        None
    };
    provider_closed_at
        .or_else(|| string_field(session, "updatedAt"))
        .or_else(|| string_field(session, "createdAt"))
        .unwrap_or_default()
}

fn search_result(project: Option<&Value>, session: &Value, matched: Value) -> Value {
    let mut output = Map::new();
    if let Some(agent_id) = string_field(session, "agentId").filter(|value| !value.is_empty()) {
        output.insert("agentIcon".to_string(), Value::String(agent_id.clone()));
        output.insert("agentId".to_string(), Value::String(agent_id));
    }
    insert_optional_string(
        &mut output,
        "agentName",
        read_runtime_text(session, "agentName")
            .or_else(|| string_field(session, "agentId"))
            .filter(|value| !value.is_empty()),
    );
    insert_optional_string(
        &mut output,
        "agentSessionId",
        read_runtime_text(session, "agentSessionId"),
    );
    insert_optional_string(
        &mut output,
        "agentSessionPath",
        read_runtime_text(session, "agentSessionPath"),
    );
    output.insert("createdAt".to_string(), value_field(session, "createdAt"));
    insert_optional_js_truthy_value(&mut output, "cwd", session.get("cwd").cloned());
    merge_object(&mut output, project_session_title(session));
    output.insert("isFavorite".to_string(), Value::Bool(is_favorite(session)));
    output.insert("isPinned".to_string(), value_field(session, "isPinned"));
    output.insert(
        "lastActiveAt".to_string(),
        Value::String(last_active_at(session)),
    );
    output.insert(
        "lifecycleState".to_string(),
        value_field(session, "lifecycleState"),
    );
    output.insert("match".to_string(), matched);
    output.insert("projectId".to_string(), value_field(session, "projectId"));
    output.insert(
        "projectTitle".to_string(),
        project
            .and_then(|project| string_field(project, "name"))
            .or_else(|| string_field(session, "projectId"))
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    output.insert("sessionId".to_string(), value_field(session, "sessionId"));
    if let Some(provider) = search_session_persistence_provider(session) {
        output.insert(
            "sessionPersistenceProvider".to_string(),
            Value::String(provider.clone()),
        );
        if let Some(name) = search_session_persistence_name(session, &provider) {
            output.insert("sessionPersistenceName".to_string(), Value::String(name));
        }
    }
    insert_optional_js_truthy_value(
        &mut output,
        "sessionTag",
        session.get("sessionTag").cloned(),
    );
    insert_present_value(
        &mut output,
        "sidebarOrder",
        session.get("sidebarOrder").cloned(),
    );
    insert_optional_string(
        &mut output,
        "subtitle",
        string_field(session, "cwd")
            .or_else(|| project.and_then(|project| string_field(project, "path"))),
    );
    output.insert("surface".to_string(), value_field(session, "surface"));
    output.insert("updatedAt".to_string(), value_field(session, "updatedAt"));
    insert_optional_value(&mut output, "zmxName", session.get("zmxName").cloned());
    Value::Object(output)
}

fn match_session(project: Option<&Value>, session: &Value, query: &str) -> Option<Value> {
    if query.is_empty() {
        return Some(json!({ "field": "title" }));
    }
    let title = project_session_title(session);
    let mut fields: Vec<(&str, String)> = Vec::new();
    push_field(
        &mut fields,
        "title",
        title.get("title").and_then(Value::as_str),
    );
    push_field(
        &mut fields,
        "title",
        title.get("primaryTitle").and_then(Value::as_str),
    );
    push_field(
        &mut fields,
        "title",
        title.get("terminalTitle").and_then(Value::as_str),
    );
    push_owned_field(&mut fields, "agent", string_field(session, "agentId"));
    push_owned_field(
        &mut fields,
        "agent",
        read_runtime_text(session, "agentName"),
    );
    push_owned_field(
        &mut fields,
        "project",
        project.and_then(|project| string_field(project, "name")),
    );
    push_owned_field(
        &mut fields,
        "project",
        project.and_then(|project| string_field(project, "path")),
    );
    push_owned_field(&mut fields, "cwd", string_field(session, "cwd"));
    push_owned_field(&mut fields, "command", string_field(session, "commandId"));
    push_owned_field(&mut fields, "id", string_field(session, "sessionId"));
    push_owned_field(&mut fields, "id", string_field(session, "globalRef"));
    push_owned_field(&mut fields, "timestamp", string_field(session, "createdAt"));
    push_owned_field(&mut fields, "timestamp", string_field(session, "updatedAt"));
    push_owned_field(&mut fields, "timestamp", Some(last_active_at(session)));
    for (field, value) in fields {
        if value.to_lowercase().contains(query) {
            return Some(json!({ "field": field, "snippet": value }));
        }
    }
    None
}

fn push_field(fields: &mut Vec<(&'static str, String)>, field: &'static str, value: Option<&str>) {
    if let Some(value) = value {
        fields.push((field, value.to_string()));
    }
}

fn push_owned_field(
    fields: &mut Vec<(&'static str, String)>,
    field: &'static str,
    value: Option<String>,
) {
    if let Some(value) = value {
        fields.push((field, value));
    }
}

const DEFAULT_TERMINAL_SESSION_TITLE: &str = "Terminal Session";
const TERMINAL_TITLE_MARKER: &str = "\u{2217}";
const UNSYNCED_TITLE_LABEL: &str = "(Unsynced title)";

fn project_session_title(session: &Value) -> Map<String, Value> {
    let title = string_field(session, "title")
        .unwrap_or_else(|| DEFAULT_TERMINAL_SESSION_TITLE.to_string());
    let title_source = session_title_source(session, &title);
    let agent_id = string_field(session, "agentId");
    let primary_candidate = session_card_primary_title(&title, agent_id.as_deref());
    let trusted_resume_title = trusted_resume_title(&title, &title_source);
    let primary_title = primary_candidate;
    let terminal_title: Option<String> = None;
    let is_primary_terminal = trusted_resume_title.is_some();
    let display_title = format_display_session_title(
        is_primary_terminal,
        primary_title.as_deref(),
        terminal_title.as_deref(),
        &title,
        false,
    );
    let display_title_tooltip = format_display_session_title(
        is_primary_terminal,
        primary_title.as_deref(),
        terminal_title.as_deref(),
        &title,
        true,
    );
    let mut output = Map::new();
    output.insert("displayTitle".to_string(), Value::String(display_title));
    output.insert(
        "displayTitleTooltip".to_string(),
        Value::String(display_title_tooltip),
    );
    output.insert(
        "isPrimaryTitleTerminalTitle".to_string(),
        Value::Bool(is_primary_terminal),
    );
    output.insert(
        "isTemporaryTitle".to_string(),
        Value::Bool(title_source == "placeholder" || is_temporary_session_title(&title)),
    );
    insert_optional_string(&mut output, "primaryTitle", primary_title);
    insert_optional_string(&mut output, "terminalTitle", terminal_title);
    output.insert("title".to_string(), Value::String(title));
    output.insert("titleSource".to_string(), Value::String(title_source));
    insert_optional_string(&mut output, "trustedResumeTitle", trusted_resume_title);
    output
}

pub fn project_session_title_projection(session: &Value) -> Value {
    Value::Object(project_session_title(session))
}

fn presentation_actions(session: &Value, activity: &str) -> Value {
    /*
    CDXC:GxserverRustPort 2026-06-15-18:06:
    Phase 5 adds real zmx session I/O endpoints, so sidebar read/send/focus/sleep actions must require a confirmed provider route. A running domain row with providerState=unknown stays attachable but must not advertise live I/O until probe/start proves zmx exists.
    */
    let lifecycle = effective_lifecycle_state(session);
    let provider_session_state = provider_session_state(session);
    let provider_exists = provider_session_state == "exists";
    let is_running = lifecycle == "running";
    let is_sleeping = lifecycle == "sleeping";
    let is_stopped = lifecycle == "stopped";
    let can_attach =
        provider_exists || (is_running && provider_session_state == "unknown") || is_sleeping;
    let can_interact = provider_exists && !is_sleeping && !is_stopped;
    json!({
        "acknowledgeAttention": activity == "attention",
        "attach": can_attach,
        "focus": can_interact,
        "kill": !is_stopped,
        "readText": can_interact,
        "sendMessage": can_interact,
        "sendText": can_interact,
        "sleep": can_interact,
        "wake": is_sleeping,
    })
}

pub(crate) fn presentation_activity(session: &Value, generated_at: &str) -> String {
    let generated_at_ms = parse_iso_ms(generated_at).unwrap_or_else(now_ms);
    let raw_activity = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"));
    let effective = effective_agent_activity_value(raw_activity, "idle", generated_at_ms);
    let activity = effective
        .as_object()
        .and_then(|activity| activity.get("activity"))
        .and_then(Value::as_str);
    match activity {
        Some("attention" | "working") => activity.unwrap().to_string(),
        _ => "idle".to_string(),
    }
}

fn attention_state(session: &Value, generated_at: &str) -> Value {
    let generated_at_ms = parse_iso_ms(generated_at).unwrap_or_else(now_ms);
    let raw_activity = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"));
    let activity = effective_agent_activity_value(raw_activity, "idle", generated_at_ms)
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut output = Map::new();
    output.insert(
        "acknowledged".to_string(),
        Value::Bool(activity.get("isAcknowledged").and_then(Value::as_bool) == Some(true)),
    );
    insert_optional_value(
        &mut output,
        "enteredAt",
        activity.get("lastChangedAt").cloned(),
    );
    insert_optional_value(
        &mut output,
        "eventId",
        activity
            .get("attentionEventId")
            .cloned()
            .or_else(|| activity.get("lastChangedAt").cloned()),
    );
    Value::Object(output)
}

fn should_include_presentation_session(session: &Value) -> bool {
    is_active(session)
        || session.get("isPinned").and_then(Value::as_bool) == Some(true)
        || is_favorite(session)
        || session_tag_is_truthy(session)
}

pub(crate) fn is_active(session: &Value) -> bool {
    matches!(
        effective_lifecycle_state(session).as_str(),
        "running" | "sleeping"
    )
}

fn effective_lifecycle_state(session: &Value) -> String {
    if provider_exists(session)
        && string_field(session, "lifecycleState").as_deref() != Some("stopped")
    {
        return "running".to_string();
    }
    string_field(session, "lifecycleState").unwrap_or_else(|| "unknown".to_string())
}

fn provider_exists(session: &Value) -> bool {
    provider_session_state(session) == "exists"
}

fn provider_session_state(session: &Value) -> String {
    if read_session_persistence_provider(session).as_deref() == Some("off") {
        return "persistence-disabled".to_string();
    }
    match session
        .get("providerState")
        .and_then(Value::as_object)
        .and_then(|provider| provider.get("lifecycleState"))
        .and_then(Value::as_str)
    {
        Some("exists") => "exists".to_string(),
        Some("missing") => "missing".to_string(),
        Some("unknown") => "unknown".to_string(),
        _ => "unknown".to_string(),
    }
}

fn title_observation_state(session: &Value) -> Option<Value> {
    let observation = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("zmxTitleObservation"))
        .and_then(Value::as_object)?;
    let status = match observation.get("status").and_then(Value::as_str) {
        Some("active" | "failed" | "retrying" | "starting") => {
            observation.get("status").cloned().unwrap_or(Value::Null)
        }
        _ => return None,
    };
    let mut output = Map::new();
    if let Some(failure_count) = observation
        .get("failureCount")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value.floor() as i64)
    {
        output.insert(
            "failureCount".to_string(),
            Value::Number(serde_json::Number::from(failure_count)),
        );
    }
    insert_optional_observation_text(&mut output, observation, "lastFailedAt");
    insert_optional_observation_text(&mut output, observation, "lastObservedAt");
    insert_optional_observation_text(&mut output, observation, "lastStartedAt");
    insert_optional_observation_text(&mut output, observation, "nextRetryAt");
    output.insert("status".to_string(), status);
    Some(Value::Object(output))
}

fn insert_optional_observation_text(
    output: &mut Map<String, Value>,
    observation: &Map<String, Value>,
    key: &str,
) {
    if let Some(value) = observation
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        output.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn build_session_tooltip(project: &Value, session: &Value, title: &str) -> String {
    let mut parts = Vec::new();
    if !title.is_empty() {
        parts.push(title.to_string());
    }
    parts.extend(
        [
            string_field(project, "name"),
            string_field(session, "cwd"),
            string_field(session, "agentId"),
            string_field(session, "commandId"),
        ]
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty()),
    );
    parts.join(" - ")
}

fn snapshot_subtitle(project: &Value, session: &Value) -> Option<String> {
    let value = match session.get("cwd") {
        Some(value) if !value.is_null() => Some(value),
        _ => project.get("path"),
    }?;
    value
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn session_tag_is_truthy(session: &Value) -> bool {
    session.get("sessionTag").map(js_truthy).unwrap_or(false)
}

fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().map(|value| value != 0.0).unwrap_or(true),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn insert_optional_js_truthy_value(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value.filter(js_truthy) {
        map.insert(key.to_string(), value);
    }
}

fn is_favorite(session: &Value) -> bool {
    string_field(session, "sessionTag").as_deref() == Some("favorite")
        || session.get("isFavorite").and_then(Value::as_bool) == Some(true)
}

fn effective_session_tag_for_filter(session: &Value) -> Option<String> {
    if let Some(tag) = string_field(session, "sessionTag").filter(|tag| !tag.is_empty()) {
        return Some(tag);
    }
    if session.get("isFavorite").and_then(Value::as_bool) == Some(true) {
        return Some("favorite".to_string());
    }
    None
}

fn session_matches_tag_filters(session: &Value, tags: &[&str]) -> bool {
    if tags.is_empty() {
        return true;
    }
    match effective_session_tag_for_filter(session) {
        Some(tag) => tags.iter().any(|expected| *expected == tag),
        None => tags.iter().any(|expected| *expected == "untagged"),
    }
}

fn project_sort_key(project: &Value) -> String {
    let pin_rank = if project.get("isPinned").and_then(Value::as_bool) == Some(true) {
        "0"
    } else if project.get("isFavorite").and_then(Value::as_bool) == Some(true) {
        "1"
    } else {
        "2"
    };
    format!(
        "{}:{}:{}",
        pin_rank,
        string_field(project, "name")
            .unwrap_or_default()
            .to_lowercase(),
        string_field(project, "projectId").unwrap_or_default()
    )
}

fn session_sort_key(session: &Value) -> String {
    let active_rank = if is_active(session) { "0" } else { "1" };
    let pin_rank = if session.get("isPinned").and_then(Value::as_bool) == Some(true) {
        "0"
    } else if is_favorite(session) {
        "1"
    } else {
        "2"
    };
    let sidebar_order = session
        .get("sidebarOrder")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| format!("{:012}", value.floor() as i64))
        .unwrap_or_else(|| "z".to_string());
    format!(
        "{}:{}:{}:{}:{}",
        sidebar_order,
        active_rank,
        pin_rank,
        last_active_at(session),
        string_field(session, "sessionId").unwrap_or_default()
    )
}

fn last_active_at(session: &Value) -> String {
    string_field(session, "lastActiveAt")
        .or_else(|| string_field(session, "createdAt"))
        .unwrap_or_default()
}

fn session_title_source(session: &Value, title: &str) -> String {
    read_runtime_text(session, "titleSource")
        .or_else(|| read_runtime_text(session, "restoreTitleSource"))
        .filter(|value| {
            matches!(
                value.as_str(),
                "browser-auto" | "generated" | "placeholder" | "terminal-auto" | "user"
            )
        })
        .unwrap_or_else(|| {
            if is_temporary_session_title(title) {
                "placeholder".to_string()
            } else {
                "user".to_string()
            }
        })
}

fn trusted_resume_title(title: &str, title_source: &str) -> Option<String> {
    if title_source == "placeholder" {
        return None;
    }
    let resume_title = visible_terminal_title(Some(title))?.trim().to_string();
    if resume_title.is_empty() || is_rejected_resume_title(&resume_title) {
        return None;
    }
    Some(resume_title)
}

fn session_card_primary_title(title: &str, agent_id: Option<&str>) -> Option<String> {
    let normalized = normalize_terminal_title(Some(title))
        .map(|title| normalize_spaces(title.trim()))
        .unwrap_or_else(|| normalize_spaces(title.trim()));
    if normalized.is_empty()
        || is_session_number_title(&normalized)
        || is_ignored_generic_agent_terminal_title(&normalized)
        || is_path_like_terminal_title(&normalized)
        || is_shell_location_terminal_title(&normalized)
    {
        return Some(agent_default_title(agent_id));
    }
    Some(normalized)
}

fn format_display_session_title(
    is_primary_title_terminal_title: bool,
    primary_title: Option<&str>,
    terminal_title: Option<&str>,
    title: &str,
    include_unsynced_title_label: bool,
) -> String {
    let normalized_primary_title = normalize_display_title(primary_title);
    let normalized_terminal_title = normalize_display_title(terminal_title);
    let normalized_title = normalize_display_title(Some(title));
    let base_title = normalized_primary_title
        .clone()
        .or(normalized_title)
        .unwrap_or_else(|| DEFAULT_TERMINAL_SESSION_TITLE.to_string());
    if is_primary_title_terminal_title
        || normalized_primary_title.is_none()
        || normalized_primary_title == normalized_terminal_title
    {
        return base_title;
    }
    if include_unsynced_title_label {
        format!("{TERMINAL_TITLE_MARKER} {base_title} {UNSYNCED_TITLE_LABEL}")
    } else {
        format!("{TERMINAL_TITLE_MARKER} {base_title}")
    }
}

fn normalize_display_title(title: Option<&str>) -> Option<String> {
    let normalized = normalize_spaces(title?.trim());
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn visible_terminal_title(title: Option<&str>) -> Option<String> {
    let normalized = normalize_terminal_title(title)?;
    if is_path_like_terminal_title(&normalized)
        || is_shell_location_terminal_title(&normalized)
        || is_ignored_placeholder_session_title(&normalized)
        || is_ignored_generic_agent_terminal_title(&normalized)
        || is_agent_status_word_title(&normalized)
        || is_windows_default_powershell_title(&normalized)
    {
        return None;
    }
    Some(normalized)
}

fn normalize_terminal_title(title: Option<&str>) -> Option<String> {
    let normalized = title?.trim();
    if normalized.is_empty() {
        return None;
    }
    let without_markers = normalized
        .trim_start_matches(is_leading_terminal_title_status_marker)
        .trim();
    let sanitized = strip_oc_prefixes(without_markers).trim().to_string();
    if let Some(cursor_title) = normalize_cursor_terminal_title(&sanitized) {
        return cursor_title;
    }
    if let Some(antigravity_title) = normalize_antigravity_terminal_title(&sanitized) {
        return antigravity_title;
    }
    if let Some(pi_title) = normalize_pi_terminal_title(&sanitized) {
        return Some(pi_title);
    }
    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

fn is_leading_terminal_title_status_marker(ch: char) -> bool {
    /*
    CDXC:GxserverSessionTitles 2026-06-29-01:21:
    Factory Droid terminal titles can prefix visible session names with the U+26EC status marker.
    Presentation must strip the marker for existing stored rows too, because sidebar copy/details reads displayTitle before the raw durable title.
    */
    ch.is_whitespace()
        || ('\u{2800}'..='\u{28ff}').contains(&ch)
        || matches!(
            ch,
            '\u{00b7}'
                | '\u{2022}'
                | '\u{22c5}'
                | '\u{25e6}'
                | '\u{2733}'
                | '*'
                | '\u{2217}'
                | '\u{2736}'
                | '\u{273b}'
                | '\u{273d}'
                | '\u{2738}'
                | '\u{2739}'
                | '\u{273a}'
                | '\u{2737}'
                | '\u{2734}'
                | '\u{25d0}'
                | '\u{25d1}'
                | '\u{25d2}'
                | '\u{25d3}'
                | '\u{26ec}'
                | '\u{2726}'
                | '\u{25c7}'
                | '\u{1f916}'
                | '\u{1f514}'
        )
}

fn strip_oc_prefixes(title: &str) -> String {
    let mut rest = title;
    loop {
        let lower = rest.to_lowercase();
        if !lower.starts_with("oc") {
            break;
        }
        let after_oc = &rest[2..];
        let after_spaces = after_oc.trim_start();
        let Some(after_pipe) = after_spaces.strip_prefix('|') else {
            break;
        };
        rest = after_pipe.trim_start();
    }
    rest.to_string()
}

fn normalize_cursor_terminal_title(title: &str) -> Option<Option<String>> {
    let normalized = normalize_spaces(title.trim());
    if is_cursor_cli_placeholder_terminal_title(&normalized) {
        return Some(None);
    }
    if normalized.ends_with("\u{2705} Ready") {
        let stripped = strip_cursor_status_suffix(&normalized, "\u{2705} Ready");
        return Some(cursor_status_title(stripped));
    }
    let working_marker = "\u{23f3} Working ";
    if let Some(index) = normalized.rfind(working_marker) {
        let trailing = &normalized[index + working_marker.len()..];
        if !trailing.is_empty() && trailing.chars().all(|ch| ch == '.' || ch == '\u{00b7}') {
            let stripped = strip_cursor_working_suffix(&normalized, index);
            return Some(cursor_status_title(stripped));
        }
    }
    None
}

fn cursor_status_title(stripped: String) -> Option<String> {
    if is_cursor_cli_placeholder_terminal_title(&stripped) {
        return None;
    }
    if stripped.is_empty() {
        None
    } else {
        Some(stripped)
    }
}

fn strip_cursor_status_suffix(title: &str, suffix: &str) -> String {
    let Some(prefix) = title.strip_suffix(suffix) else {
        return title.trim().to_string();
    };
    prefix
        .trim_end()
        .strip_suffix('-')
        .map(str::trim)
        .unwrap_or(title)
        .trim()
        .to_string()
}

fn strip_cursor_working_suffix(title: &str, status_index: usize) -> String {
    let prefix = &title[..status_index];
    prefix
        .trim_end()
        .strip_suffix('-')
        .map(str::trim)
        .unwrap_or(title)
        .trim()
        .to_string()
}

fn is_cursor_cli_placeholder_terminal_title(title: &str) -> bool {
    let normalized = normalize_spaces(title.trim());
    let lower = normalized.to_lowercase();
    lower == "cursor"
        || lower == "cursor agent"
        || lower == "cursor cli"
        || lower == "cursor-agent"
        || lower == "cursor agent - \u{2705} ready"
}

fn normalize_antigravity_terminal_title(title: &str) -> Option<Option<String>> {
    let normalized = normalize_spaces(title.trim());
    let lower = normalized.to_lowercase();
    if lower == "agy" {
        return Some(Some("agy".to_string()));
    }
    if let Some(rest) = normalized.strip_prefix('\u{1f514}') {
        if rest.trim().eq_ignore_ascii_case("agy") {
            return Some(Some("agy".to_string()));
        }
    }
    None
}

pub(crate) fn normalize_pi_terminal_title(title: &str) -> Option<String> {
    let trimmed = title.trim();
    let rest = trimmed.strip_prefix('\u{03c0}')?.trim_start();
    if let Some(status_marker) = rest.chars().next() {
        if status_marker == '>' || ('\u{2800}'..='\u{28ff}').contains(&status_marker) {
            let title = rest[status_marker.len_utf8()..].trim();
            return Some(if title.is_empty() {
                "\u{03c0}".to_string()
            } else {
                title.to_string()
            });
        }
    }
    let rest = rest.strip_prefix('-')?.trim();
    if rest.is_empty() {
        return None;
    }
    let parts = rest
        .split(" - ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        Some("\u{03c0}".to_string())
    } else {
        Some(parts[..parts.len() - 1].join(" - "))
    }
}

fn is_ignored_placeholder_session_title(title: &str) -> bool {
    let normalized = normalize_spaces(title.trim());
    let lower = normalized.to_lowercase();
    is_session_number_title(&normalized)
        || codex_session_id_from_title(&normalized).is_some()
        || is_ghost_placeholder_session_title(&normalized)
        || is_agent_status_word_title(&normalized)
        || is_ignored_placeholder_session_title_text(&lower)
        || is_path_like_terminal_title(&normalized)
}

fn is_ignored_placeholder_session_title_text(lower: &str) -> bool {
    matches!(
        lower,
        "terminal session"
            | "amp cli session"
            | "amp session"
            | "antigravity cli session"
            | "antigravity session"
            | "claude session"
            | "claude code session"
            | "codebuddy session"
            | "code buddy session"
            | "codex session"
            | "codex cli session"
            | "copilot session"
            | "cursor agent session"
            | "cursor cli session"
            | "cursor session"
            | "droid session"
            | "factory droid session"
            | "gemini session"
            | "grok session"
            | "grok build session"
            | "hermes session"
            | "hermes agent session"
            | "kiro session"
            | "kiro cli session"
            | "omp session"
            | "opencode session"
            | "open code session"
            | "openai codex session"
            | "pi session"
            | "qoder session"
            | "qodercli session"
            | "rovo session"
            | "rovo dev session"
            | "rovodev session"
    )
}

fn is_ignored_generic_agent_terminal_title(title: &str) -> bool {
    let lower = normalize_spaces(title.trim()).to_lowercase();
    matches!(
        lower.as_str(),
        "amp"
            | "amp cli"
            | "agy"
            | "antigravity"
            | "antigravity cli"
            | "claude"
            | "claude code"
            | "codex"
            | "codex cli"
            | "cursor"
            | "cursor agent"
            | "cursor cli"
            | "cursor-agent"
            | "droid"
            | "factory droid"
            | "grok"
            | "grok build"
            | "kiro"
            | "kiro cli"
            | "kiro-cli"
            | "omp"
            | "openai codex"
            | "pi"
            | "\u{03c0}"
            | "ghostex"
    )
}

fn is_rejected_resume_title(title: &str) -> bool {
    let normalized = title.trim();
    let lower = normalized.to_lowercase();
    normalized == "\u{00f0}^\u{00df}^\u{00d1}\u{00bb}"
        || is_temporary_session_title(normalized)
        || is_ghost_placeholder_session_title(normalized)
        || is_gxserver_session_id(normalized)
        || normalized
            .chars()
            .any(|ch| (ch as u32) <= 0x1f || (ch as u32) == 0x7f)
        || (normalized.starts_with('\u{00f0}') && normalized.ends_with('\u{00bb}'))
        || is_agent_command_noise_title(&lower)
}

fn is_agent_command_noise_title(title: &str) -> bool {
    let Some(executable_name) = command_executable_name(title) else {
        return false;
    };
    if !is_agent_command_executable_name(&executable_name) {
        return false;
    }
    if title == executable_name {
        return true;
    }
    let rest = title[executable_name.len()..].trim();
    if rest.is_empty() || rest.starts_with('-') {
        return true;
    }
    let first_arg = rest.split_whitespace().next().unwrap_or_default();
    is_agent_command_subcommand_name(first_arg)
}

fn command_executable_name(command: &str) -> Option<String> {
    let first = command.split_whitespace().next()?.trim();
    let first = first.trim_matches(|ch| ch == '\'' || ch == '"');
    if first.is_empty() {
        None
    } else {
        Some(first.to_lowercase())
    }
}

fn is_agent_command_executable_name(value: &str) -> bool {
    matches!(
        value,
        "acli"
            | "agy"
            | "amp"
            | "claude"
            | "codebuddy"
            | "codex"
            | "copilot"
            | "cursor-agent"
            | "droid"
            | "gemini"
            | "grok"
            | "hermes"
            | "kiro-cli"
            | "omp"
            | "opencode"
            | "pi"
            | "qodercli"
    )
}

fn is_agent_command_subcommand_name(value: &str) -> bool {
    matches!(
        value,
        "auth"
            | "completion"
            | "debug"
            | "exec"
            | "help"
            | "login"
            | "logout"
            | "mcp"
            | "resume"
            | "run"
            | "sandbox"
            | "session"
            | "sessions"
    )
}

fn codex_session_id_from_title(title: &str) -> Option<String> {
    let normalized = normalize_terminal_title(Some(title))?;
    if is_uuid_like(&normalized) {
        Some(normalized.to_lowercase())
    } else {
        None
    }
}

fn is_uuid_like(value: &str) -> bool {
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

fn is_ghost_placeholder_session_title(title: &str) -> bool {
    let normalized = normalize_spaces(title.trim());
    normalized == "\u{1f47b}" || normalized == "\u{1f47b} Terminal Session"
}

fn is_temporary_session_title(title: &str) -> bool {
    normalize_spaces(title.trim()).to_lowercase() == "search by text"
}

fn is_session_number_title(title: &str) -> bool {
    let lower = normalize_spaces(title.trim()).to_lowercase();
    let Some(rest) = lower.strip_prefix("session ") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit())
}

fn is_path_like_terminal_title(title: &str) -> bool {
    let trimmed = title.trim();
    trimmed.starts_with('~')
        || trimmed.starts_with('/')
        || trimmed.starts_with("\u{2026}/")
        || trimmed.starts_with("\u{2026}\\")
        || trimmed.starts_with(".../")
        || trimmed.starts_with("...\\")
}

fn is_shell_location_terminal_title(title: &str) -> bool {
    let Some((user_host, location)) = title.split_once(':') else {
        return false;
    };
    let Some((user, host)) = user_host.split_once('@') else {
        return false;
    };
    if user.trim().is_empty()
        || host.trim().is_empty()
        || user.chars().any(char::is_whitespace)
        || host.chars().any(char::is_whitespace)
    {
        return false;
    }
    let location = location.trim_start();
    is_path_like_terminal_title(location) || is_windows_absolute_terminal_path(location)
}

fn is_windows_absolute_terminal_path(title: &str) -> bool {
    let bytes = title.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/'))
        || title.starts_with("\\\\")
}

fn is_agent_status_word_title(title: &str) -> bool {
    let core = title
        .trim_matches(is_agent_status_boundary_char)
        .to_lowercase();
    matches!(
        core.as_str(),
        "done" | "error" | "idle" | "thinking" | "working"
    )
}

fn is_agent_status_boundary_char(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '.' | ':' | '[' | ']' | '(' | ')' | '{' | '}' | '!' | '|' | '/' | '\\' | '_' | '-'
        )
}

fn is_windows_default_powershell_title(title: &str) -> bool {
    let lower = title.to_lowercase();
    let bytes = lower.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let rest = &lower[1..];
    let prefix = ":\\windows\\system32\\windowspowershell\\v1.0\\powershell.exe";
    let Some(suffix) = rest.strip_prefix(prefix) else {
        return false;
    };
    suffix.is_empty() || (suffix.starts_with(char::is_whitespace) && suffix.trim() == ".")
}

fn agent_default_title(agent_id: Option<&str>) -> String {
    let Some(agent_id) = agent_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return DEFAULT_TERMINAL_SESSION_TITLE.to_string();
    };
    let normalized = agent_id.to_lowercase().replace(['-', '_'], " ");
    let title = normalized
        .split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let word = if first.is_ascii_alphabetic() {
                format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
            } else {
                format!("{first}{}", chars.as_str())
            };
            if word == "Cli" {
                "CLI".to_string()
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("{title} Session")
}

fn normalize_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_limit(value: Option<&Value>) -> usize {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| value.trunc().clamp(1.0, 100.0) as usize)
        .unwrap_or(40)
}

fn normalize_cursor(value: Option<&Value>) -> usize {
    value
        .and_then(Value::as_str)
        .filter(|value| value.chars().all(|ch| ch.is_ascii_digit()))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
}

fn default_group_id(project_id: &str) -> String {
    format!("{project_id}:active")
}

fn read_runtime_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read_provider_trimmed_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("providerState")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn value_field(value: &Value, key: &str) -> Value {
    value.get(key).cloned().unwrap_or(Value::Null)
}

fn insert_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    }
}

fn insert_optional_value(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value.filter(|value| !value.is_null()) {
        map.insert(key.to_string(), value);
    }
}

fn insert_present_value(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        map.insert(key.to_string(), value);
    }
}

fn merge_object(target: &mut Map<String, Value>, values: Map<String, Value>) {
    target.extend(values);
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn parse_iso_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::DomainRepository,
        paths::get_gxserver_paths,
        storage::{initialize_gxserver_storage, open_gxserver_database},
    };

    #[test]
    fn snapshot_sorts_projects_and_sessions_for_sidebar_projection() {
        let projects = vec![
            project("P100", "Zulu", false, false),
            project("P101", "Alpha", true, false),
        ];
        let sessions = vec![
            session("P101", "G100", "Later", "running", 2000.0),
            session("P101", "G101", "Earlier", "running", 1000.0),
            session("P101", "G102", "Hidden stopped", "stopped", 0.0),
        ];
        let snapshot = project_snapshot(projects, sessions, 7, true);
        let projects = snapshot
            .get("projects")
            .and_then(Value::as_array)
            .expect("projects");
        assert_eq!(
            projects[0].get("projectId").and_then(Value::as_str),
            Some("P101")
        );
        let groups = snapshot
            .get("groups")
            .and_then(Value::as_array)
            .expect("groups");
        assert_eq!(groups[0].get("sessionIds"), Some(&json!(["G101", "G100"])));
    }

    #[test]
    fn snapshot_publishes_one_cached_git_status_to_every_session_sharing_a_cwd() {
        /*
        CDXC:SidebarV2GitStatus 2026-07-29-00:00:
        The probe is per unique cwd, so a project's terminal and its agent — two
        rows, one checkout — must render the identical card row from the single
        cached answer. Sessions whose cwd was never probed publish no key at all.
        */
        let cwd = "/tmp/ghostex-presentation-git-status/shared-checkout";
        crate::session_git_status::set_cached_session_git_status_for_test(
            cwd,
            Some(crate::session_git_status::SessionGitStatus {
                branch: Some("ghostex/9f8e7d6c".to_string()),
                additions: 41,
                deletions: 3,
                pull_request: Some(crate::session_git_status::SessionPullRequest {
                    number: 118,
                    state: crate::session_git_status::PullRequestState::Open,
                    url: Some("https://github.com/o/r/pull/118".to_string()),
                }),
                updated_at: "2026-07-29T12:00:00.000Z".to_string(),
            }),
        );

        let mut first = session("P400", "G400", "Terminal", "running", 1.0);
        first
            .as_object_mut()
            .expect("session object")
            .insert("cwd".to_string(), json!(cwd));
        let mut second = session("P400", "G401", "Agent", "running", 2.0);
        second
            .as_object_mut()
            .expect("session object")
            .insert("cwd".to_string(), json!(cwd));
        let mut elsewhere = session("P400", "G402", "Elsewhere", "running", 3.0);
        elsewhere.as_object_mut().expect("session object").insert(
            "cwd".to_string(),
            json!("/tmp/ghostex-presentation-git-status/never-probed"),
        );

        let snapshot = project_snapshot(
            vec![project("P400", "Git", false, false)],
            vec![first, second, elsewhere],
            3,
            true,
        );
        let sessions = snapshot
            .get("sessions")
            .and_then(Value::as_array)
            .expect("sessions");
        let expected = json!({
            "additions": 41,
            "branch": "ghostex/9f8e7d6c",
            "deletions": 3,
            "prNumber": 118,
            "prState": "open",
            "prUrl": "https://github.com/o/r/pull/118",
            "updatedAt": "2026-07-29T12:00:00.000Z",
        });
        assert_eq!(sessions[0].get("gitStatus"), Some(&expected));
        assert_eq!(sessions[1].get("gitStatus"), Some(&expected));
        assert!(
            sessions[2].get("gitStatus").is_none(),
            "an unprobed cwd publishes no gitStatus key"
        );
    }

    #[test]
    fn snapshot_publishes_git_status_for_sessions_running_in_the_project_root() {
        /*
        CDXC:SidebarV2GitStatus 2026-07-30 (effective cwd):
        Agent sessions are created WITHOUT a cwd — they run in the project's path,
        which is why every launcher resolves `session.cwd` else `project.path`.
        Presentation must resolve the git-status key the same way or no agent card
        on the machine ever shows a branch. The published `cwd` field stays raw
        (absent here), because V2's worktree logic reads it to tell a managed
        worktree checkout apart from a project-root session.
        */
        let project_path = "/tmp/ghostex-presentation-git-status/project-root";
        crate::session_git_status::set_cached_session_git_status_for_test(
            project_path,
            Some(crate::session_git_status::SessionGitStatus {
                branch: Some("main".to_string()),
                additions: 7,
                deletions: 2,
                pull_request: None,
                updated_at: "2026-07-30T12:00:00.000Z".to_string(),
            }),
        );

        let mut project = project("P401", "Root", false, false);
        project
            .as_object_mut()
            .expect("project object")
            .insert("path".to_string(), json!(project_path));
        // No `cwd` key at all: exactly how createAgentSession stores an agent row.
        let agent = session("P401", "G410", "Agent", "running", 1.0);

        let snapshot = project_snapshot(vec![project], vec![agent], 4, true);
        let sessions = snapshot
            .get("sessions")
            .and_then(Value::as_array)
            .expect("sessions");
        assert_eq!(
            sessions[0].get("gitStatus"),
            Some(&json!({
                "additions": 7,
                "branch": "main",
                "deletions": 2,
                "updatedAt": "2026-07-30T12:00:00.000Z",
            }))
        );
        assert!(
            sessions[0].get("cwd").is_none(),
            "the fallback must not invent a published cwd"
        );
    }

    #[test]
    fn snapshot_publishes_session_lifecycle_state_and_capability_flags() {
        /*
        CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
        Sidebar V2 reads settle/snooze from presentation and hides its
        affordances for machines whose snapshot carries no capability object.
        Sessions with no lifecycle state must publish absent keys, not nulls.
        */
        let mut settled = session("P100", "G100", "Settled", "running", 1.0);
        let settled_object = settled.as_object_mut().expect("settled session object");
        settled_object.insert("settledAt".to_string(), json!("2026-07-20T09:00:00.000Z"));
        settled_object.insert("settledOverride".to_string(), json!("settled"));
        settled_object.insert(
            "settledOverrideAt".to_string(),
            json!("2026-07-20T09:00:00.000Z"),
        );
        settled_object.insert("snoozedAt".to_string(), json!("2026-07-21T09:00:00.000Z"));
        settled_object.insert(
            "snoozedUntil".to_string(),
            json!("2026-07-22T09:00:00.000Z"),
        );

        let snapshot = project_snapshot(
            vec![project("P100", "Active", false, false)],
            vec![settled, session("P100", "G101", "Plain", "running", 2.0)],
            7,
            true,
        );

        assert_eq!(
            snapshot.get("capabilities"),
            Some(&json!({
                "sessionGitStatus": true,
                "sessionSettlement": true,
                "sessionSnooze": true,
                "worktreeSessions": true,
            }))
        );
        let sessions = snapshot
            .get("sessions")
            .and_then(Value::as_array)
            .expect("sessions");
        let published = &sessions[0];
        assert_eq!(
            published.get("settledAt"),
            Some(&json!("2026-07-20T09:00:00.000Z"))
        );
        assert_eq!(published.get("settledOverride"), Some(&json!("settled")));
        assert_eq!(
            published.get("snoozedAt"),
            Some(&json!("2026-07-21T09:00:00.000Z"))
        );
        assert_eq!(
            published.get("snoozedUntil"),
            Some(&json!("2026-07-22T09:00:00.000Z"))
        );
        assert!(
            published.get("settledOverrideAt").is_none(),
            "the override stamp stays server-internal"
        );
        for key in ["settledAt", "settledOverride", "snoozedAt", "snoozedUntil"] {
            assert!(
                sessions[1].get(key).is_none(),
                "{key} must be absent on a session with no lifecycle state"
            );
        }
    }

    #[test]
    fn snapshot_publishes_the_cached_origin_remote_for_a_project_and_its_worktree_family() {
        /*
        CDXC:SidebarV2LogicalProjects 2026-07-29-00:00:
        Sidebar V2 merges the same repository across machines by its `origin`
        remote. A registered worktree project must carry its FAMILY ROOT's
        remote, so the parent and its worktrees land in one logical project; a
        repository with no origin publishes an explicit null; a non-git or
        unprobed path publishes no key at all.
        */
        let root_path = "/tmp/ghostex-presentation-git-remote/repo";
        let plain_path = "/tmp/ghostex-presentation-git-remote/plain";
        crate::project_git_remote::set_cached_project_git_remote_for_test(
            root_path,
            Some(crate::project_git_remote::ProjectGitRemote {
                origin_url: Some("git@github.com:Owner/Repo.git".to_string()),
                repository_root_path: Some(root_path.to_string()),
            }),
        );
        crate::project_git_remote::set_cached_project_git_remote_for_test(
            plain_path,
            Some(crate::project_git_remote::ProjectGitRemote {
                origin_url: None,
                repository_root_path: None,
            }),
        );

        let mut root = project("P500", "Repo", false, false);
        root.as_object_mut()
            .expect("project object")
            .insert("path".to_string(), json!(root_path));
        let mut worktree = project("P501", "Repo worktree", false, false);
        let worktree_object = worktree.as_object_mut().expect("project object");
        worktree_object.insert(
            "path".to_string(),
            json!("/tmp/ghostex-presentation-git-remote/repo-a1b2c3d4"),
        );
        worktree_object.insert(
            "worktree".to_string(),
            json!({
                "branch": "ghostex/a1b2c3d4",
                "parentProjectId": "P500",
                "parentProjectPath": root_path,
            }),
        );
        let mut plain = project("P502", "Notes", false, false);
        plain
            .as_object_mut()
            .expect("project object")
            .insert("path".to_string(), json!(plain_path));
        let mut unprobed = project("P503", "Fresh", false, false);
        unprobed.as_object_mut().expect("project object").insert(
            "path".to_string(),
            json!("/tmp/ghostex-presentation-git-remote/never-probed"),
        );

        let snapshot = project_snapshot(vec![root, worktree, plain, unprobed], Vec::new(), 3, true);
        let projects = snapshot
            .get("projects")
            .and_then(Value::as_array)
            .expect("projects");
        let published = |project_id: &str, key: &str| -> Option<Value> {
            projects
                .iter()
                .find(|project| {
                    project.get("projectId").and_then(Value::as_str) == Some(project_id)
                })
                .expect("published project")
                .get(key)
                .cloned()
        };
        assert_eq!(
            published("P500", "gitRemoteOriginUrl"),
            Some(json!("git@github.com:Owner/Repo.git"))
        );
        assert_eq!(
            published("P501", "gitRemoteOriginUrl"),
            Some(json!("git@github.com:Owner/Repo.git")),
            "a registered worktree publishes its family root's remote"
        );
        assert_eq!(
            published("P502", "gitRemoteOriginUrl"),
            Some(Value::Null),
            "a repository with no origin publishes an explicit null"
        );
        assert_eq!(
            published("P503", "gitRemoteOriginUrl"),
            None,
            "an unprobed path publishes no gitRemoteOriginUrl key"
        );

        /*
        CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
        The repository root rides the same cache entry, so it must follow the
        same family rule as the remote and be omitted — never null — wherever
        the probe has no answer.
        */
        assert_eq!(
            published("P500", "gitRepositoryRootPath"),
            Some(json!(root_path))
        );
        assert_eq!(
            published("P501", "gitRepositoryRootPath"),
            Some(json!(root_path)),
            "a registered worktree publishes its family root's repository root"
        );
        assert_eq!(
            published("P502", "gitRepositoryRootPath"),
            None,
            "a probe with no resolved root publishes no key, not a null"
        );
        assert_eq!(published("P503", "gitRepositoryRootPath"), None);
    }

    #[test]
    fn snapshot_publishes_the_auto_settle_window_this_daemon_sweeps_with() {
        /*
        CDXC:SidebarV2LogicalProjects 2026-07-29-00:00:
        One sidebar renders rows from several daemons, so each snapshot states
        the window THAT daemon applies. The published value comes from
        `read_sweep_auto_settle_after_days` — the same function the auto-settle
        sweep calls — so the advertised window and the applied window are one
        rule, not two.
        */
        let (temp, db) = open_test_database();
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let settings_dir = paths.app_state_dir.clone();
        let settings_file = settings_dir.join("native-sidebar-settings.json");

        let published_window = |db: &Connection| -> Value {
            let snapshot = read_presentation_snapshot(
                db,
                "S7k",
                crate::session_lifecycle::read_sweep_auto_settle_after_days(&paths),
                crate::session_lifecycle::read_sidebar_v2_selected(&paths),
            )
            .expect("snapshot");
            snapshot
                .get("autoSettleAfterDays")
                .cloned()
                .expect("autoSettleAfterDays is always published")
        };

        assert_eq!(
            published_window(&db),
            Value::Null,
            "a machine with no settings file is a V1 machine and settles nothing"
        );

        std::fs::create_dir_all(&settings_dir).expect("settings dir");
        for (settings, expected) in [
            (json!({ "sidebarVersion": "v1" }), Value::Null),
            (
                json!({ "sidebarVersion": "v1", "sidebarAutoSettleAfterDays": 7 }),
                Value::Null,
            ),
            (json!({ "sidebarVersion": "v2" }), json!(3)),
            (
                json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": 7 }),
                json!(7),
            ),
            (
                json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": 1.5 }),
                json!(1.5),
            ),
            (
                json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": Value::Null }),
                Value::Null,
            ),
            (
                json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": 0 }),
                Value::Null,
            ),
        ] {
            std::fs::write(&settings_file, settings.to_string()).expect("settings file");
            assert_eq!(
                published_window(&db),
                expected,
                "settings {settings} must publish {expected}"
            );
        }
    }

    /*
    CDXC:SidebarV2DataGate 2026-07-29:
    The capability tells the truth about what this daemon will actually produce.
    With Sidebar V1 selected the git/`gh` probes do not run, so `sessionGitStatus`
    is false and a V2 client (local or remote) renders those cards exactly as it
    does for a daemon too old to probe — instead of waiting forever on data that
    is not coming. Settle/snooze and the worktree endpoints are unaffected: those
    RPCs are served regardless of which sidebar this machine renders.
    */
    #[test]
    fn the_git_status_capability_follows_the_sidebar_version_gate() {
        let (temp, db) = open_test_database();
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let settings_dir = paths.app_state_dir.clone();
        let settings_file = settings_dir.join("native-sidebar-settings.json");
        std::fs::create_dir_all(&settings_dir).expect("settings dir");

        let published_capabilities = |db: &Connection| -> Value {
            read_presentation_snapshot(
                db,
                "S7m",
                crate::session_lifecycle::read_sweep_auto_settle_after_days(&paths),
                crate::session_lifecycle::read_sidebar_v2_selected(&paths),
            )
            .expect("snapshot")
            .get("capabilities")
            .cloned()
            .expect("capabilities are always published")
        };

        for (settings, expected_git_status) in [
            (json!({ "sidebarVersion": "v1" }), false),
            (json!({ "sidebarVersion": "v2" }), true),
        ] {
            std::fs::write(&settings_file, settings.to_string()).expect("settings file");
            assert_eq!(
                published_capabilities(&db),
                json!({
                    "sessionGitStatus": expected_git_status,
                    "sessionSettlement": true,
                    "sessionSnooze": true,
                    "worktreeSessions": true,
                }),
                "settings {settings} must advertise sessionGitStatus {expected_git_status} \
                 and leave the lifecycle/worktree capabilities alone"
            );
        }
    }

    #[test]
    fn snapshot_omits_recent_and_hidden_system_projects() {
        /*
        CDXC:ProjectVisibility 2026-06-30-21:23:
        Project presentation is the shared active inventory contract. Recent Projects and Remote Attach carrier projects must stay out of presentation snapshots so React Native Android does not show closed workspaces or remote-attach containers as selectable projects.
        */
        let mut recent = project("P200", "Closed", false, false);
        recent
            .as_object_mut()
            .expect("recent project object")
            .insert("isRecentProject".to_string(), Value::Bool(true));
        let mut carrier = project("P201", "Remote Attach", false, false);
        let carrier_object = carrier.as_object_mut().expect("carrier project object");
        carrier_object.insert("systemKind".to_string(), json!("remoteAttachCarrier"));
        carrier_object.insert("visibility".to_string(), json!("hidden"));
        let snapshot = project_snapshot(
            vec![project("P100", "Active", false, false), recent, carrier],
            vec![
                session("P100", "G100", "Visible", "running", 1.0),
                session("P200", "G200", "Recent hidden", "running", 1.0),
                session("P201", "G201", "Carrier hidden", "running", 1.0),
            ],
            7,
            true,
        );
        let projects = snapshot
            .get("projects")
            .and_then(Value::as_array)
            .expect("projects");
        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0].get("projectId").and_then(Value::as_str),
            Some("P100")
        );
        let sessions = snapshot
            .get("sessions")
            .and_then(Value::as_array)
            .expect("sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].get("sessionId").and_then(Value::as_str),
            Some("G100")
        );
    }

    #[test]
    fn snapshot_sorts_sessions_with_sidebar_order_before_absent_order() {
        let projects = vec![project("P100", "Manual Order", false, false)];
        let ordered_later = session("P100", "G200", "Saved later", "running", 1000.0);
        let ordered_new = session("P100", "G100", "New default", "running", 0.0);
        let mut absent = session("P100", "G300", "No saved order", "running", 500.0);
        absent
            .as_object_mut()
            .expect("session object")
            .remove("sidebarOrder");

        let snapshot =
            project_snapshot(projects, vec![ordered_later, absent, ordered_new], 7, true);
        let groups = snapshot
            .get("groups")
            .and_then(Value::as_array)
            .expect("groups");
        assert_eq!(
            groups[0].get("sessionIds"),
            Some(&json!(["G100", "G200", "G300"]))
        );
        let sessions = snapshot
            .get("sessions")
            .and_then(Value::as_array)
            .expect("sessions");
        assert_eq!(sessions[0].get("sidebarOrder"), Some(&json!(0.0)));
        assert_eq!(sessions[1].get("sidebarOrder"), Some(&json!(1000.0)));
        assert!(sessions[2].get("sidebarOrder").is_none());
    }

    #[test]
    fn snapshot_projects_provider_actions_observation_and_tooltip_like_typescript() {
        let mut project = project("P100", "Projection", false, false);
        project
            .as_object_mut()
            .expect("project object")
            .insert("path".to_string(), json!("/workspace/projection"));
        let mut provider_missing = session("P100", "G100", "Missing Provider", "running", 0.0);
        provider_missing
            .as_object_mut()
            .expect("session object")
            .insert("agentId".to_string(), json!("codex"));
        provider_missing
            .as_object_mut()
            .expect("session object")
            .insert("commandId".to_string(), json!("build"));
        provider_missing
            .as_object_mut()
            .expect("session object")
            .insert("cwd".to_string(), json!("/workspace/projection"));
        provider_missing
            .as_object_mut()
            .expect("session object")
            .insert(
                "runtimeSettings".to_string(),
                json!({
                    "zmxTitleObservation": {
                        "failureCount": 2.9,
                        "lastFailedAt": "2026-06-07T00:29:59.000Z",
                        "lastObservedAt": "2026-06-07T00:29:40.000Z",
                        "lastStartedAt": "2026-06-07T00:29:58.000Z",
                        "nextRetryAt": "2026-06-07T00:30:00.000Z",
                        "rawTitle": "private terminal title",
                        "status": "retrying"
                    }
                }),
            );
        let mut provider_unknown = session("P100", "G101", "Unknown Provider", "running", 1000.0);
        provider_unknown
            .as_object_mut()
            .expect("session object")
            .remove("providerState");
        let mut provider_off = session("P100", "G102", "Provider Off", "running", 2000.0);
        provider_off
            .as_object_mut()
            .expect("session object")
            .insert(
                "runtimeSettings".to_string(),
                json!({ "sessionPersistenceProvider": "off" }),
            );

        let snapshot = project_snapshot(
            vec![project],
            vec![provider_missing, provider_unknown, provider_off],
            7,
            true,
        );
        let sessions = snapshot
            .get("sessions")
            .and_then(Value::as_array)
            .expect("sessions");
        let missing = sessions
            .iter()
            .find(|session| session.get("sessionId").and_then(Value::as_str) == Some("G100"))
            .expect("missing provider row");
        assert_eq!(
            missing.get("providerSessionState").and_then(Value::as_str),
            Some("missing")
        );
        assert_eq!(missing.get("commandId").and_then(Value::as_str), Some("build"));
        assert_eq!(
            missing
                .get("sessionPersistenceProvider")
                .and_then(Value::as_str),
            Some("zmx")
        );
        assert_eq!(
            missing
                .get("actions")
                .and_then(Value::as_object)
                .and_then(|actions| actions.get("attach"))
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            missing.get("titleObservation"),
            Some(&json!({
                "failureCount": 2,
                "lastFailedAt": "2026-06-07T00:29:59.000Z",
                "lastObservedAt": "2026-06-07T00:29:40.000Z",
                "lastStartedAt": "2026-06-07T00:29:58.000Z",
                "nextRetryAt": "2026-06-07T00:30:00.000Z",
                "status": "retrying"
            }))
        );
        assert_eq!(
            missing.get("tooltip").and_then(Value::as_str),
            Some("Missing Provider - Projection - /workspace/projection - codex - build")
        );
        assert!(!serde_json::to_string(missing)
            .expect("serialize row")
            .contains("private terminal title"));

        let unknown = sessions
            .iter()
            .find(|session| session.get("sessionId").and_then(Value::as_str) == Some("G101"))
            .expect("unknown provider row");
        assert_eq!(
            unknown.get("providerSessionState").and_then(Value::as_str),
            Some("unknown")
        );
        assert_eq!(
            unknown
                .get("actions")
                .and_then(Value::as_object)
                .and_then(|actions| actions.get("attach"))
                .and_then(Value::as_bool),
            Some(true)
        );

        let off = sessions
            .iter()
            .find(|session| session.get("sessionId").and_then(Value::as_str) == Some("G102"))
            .expect("provider off row");
        assert_eq!(
            off.get("providerSessionState").and_then(Value::as_str),
            Some("persistence-disabled")
        );
        assert_eq!(
            off.get("sessionPersistenceProvider")
                .and_then(Value::as_str),
            None
        );
        assert_eq!(
            off.get("actions")
                .and_then(Value::as_object)
                .and_then(|actions| actions.get("attach"))
                .and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn snapshot_caps_stopped_kept_sessions_and_uses_truthy_session_tags() {
        let projects = vec![project("P100", "Stopped Cap", false, false)];
        let active = session("P100", "Gactive", "Active", "running", 1000.0);
        let mut sessions = vec![active];
        for index in 0..25 {
            let mut stopped = session(
                "P100",
                &format!("G{index:02}"),
                &format!("Stopped {index:02}"),
                "stopped",
                index as f64,
            );
            stopped
                .as_object_mut()
                .expect("stopped session")
                .insert("isPinned".to_string(), Value::Bool(true));
            sessions.push(stopped);
        }
        let mut null_tag = session("P100", "Gnull", "Null Tag", "stopped", 40.0);
        null_tag
            .as_object_mut()
            .expect("null tag")
            .insert("sessionTag".to_string(), Value::Null);
        sessions.push(null_tag);
        let mut empty_tag = session("P100", "Gempty", "Empty Tag", "running", 2000.0);
        empty_tag
            .as_object_mut()
            .expect("empty tag")
            .insert("sessionTag".to_string(), Value::String(String::new()));
        empty_tag
            .as_object_mut()
            .expect("empty tag")
            .insert("agentId".to_string(), Value::String(String::new()));
        empty_tag
            .as_object_mut()
            .expect("empty tag")
            .insert("cwd".to_string(), Value::String(String::new()));
        empty_tag
            .as_object_mut()
            .expect("empty tag")
            .insert("sidebarOrder".to_string(), Value::Null);
        sessions.push(empty_tag);

        let snapshot = project_snapshot(projects, sessions, 7, true);
        let projected = snapshot
            .get("sessions")
            .and_then(Value::as_array)
            .expect("sessions");
        let ids = projected
            .iter()
            .filter_map(|session| session.get("sessionId").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 22);
        assert!(ids.contains(&"Gactive"));
        assert!(ids.contains(&"Gempty"));
        assert!(ids.contains(&"G00"));
        assert!(ids.contains(&"G19"));
        assert!(!ids.contains(&"G20"));
        assert!(!ids.contains(&"Gnull"));
        let empty_projected = projected
            .iter()
            .find(|session| session.get("sessionId").and_then(Value::as_str) == Some("Gempty"))
            .expect("empty tag active row");
        assert!(empty_projected.get("sessionTag").is_none());
        assert!(empty_projected.get("agentId").is_none());
        assert!(empty_projected.get("agentIcon").is_none());
        assert!(empty_projected.get("cwd").is_none());
        assert!(empty_projected.get("subtitle").is_none());
        assert_eq!(empty_projected.get("sidebarOrder"), Some(&Value::Null));
    }

    #[test]
    fn search_matches_case_insensitive_project_text_and_paginates() {
        let projects = vec![project("P100", "Search Project", false, false)];
        let sessions = vec![
            session("P100", "G100", "First", "running", 1000.0),
            session("P100", "G101", "Second", "running", 2000.0),
        ];
        let params = json!({
            "limit": 1,
            "query": "search project",
        });
        let result = search_sessions(
            projects,
            sessions,
            params.as_object().expect("params object"),
        )
        .expect("search sessions");
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .expect("results");
        assert_eq!(results.len(), 1);
        assert_eq!(result.get("cursor").and_then(Value::as_str), Some("1"));
        assert_eq!(
            results[0]
                .get("match")
                .and_then(Value::as_object)
                .and_then(|matched| matched.get("field"))
                .and_then(Value::as_str),
            Some("project")
        );
    }

    #[test]
    fn list_previous_sessions_reads_domain_rows_with_closed_at() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({
                    "name": "History",
                })
                .as_object()
                .expect("project params"),
            )
            .expect("project created");
        let project_id = string_field(&project, "projectId").expect("project id");
        let session = repository
            .create_session(
                json!({
                    "agentId": "codex",
                    "kind": "agent",
                    "lifecycleState": "stopped",
                    "projectId": project_id,
                    "providerState": {
                        "lifecycleState": "missing",
                        "probedAt": "2026-06-06T12:00:00.000Z",
                        "provider": "zmx",
                    },
                    "runtimeSettings": {
                        "titleSource": "terminal-auto",
                    },
                    "title": "Restorable session",
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("session created");

        let result = list_previous_sessions(&db, "S7k", &Map::new()).expect("previous sessions");
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .expect("results");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].get("sessionId"), session.get("sessionId"));
        assert_eq!(
            results[0].get("closedAt").and_then(Value::as_str),
            Some("2026-06-06T12:00:00.000Z")
        );
    }

    #[test]
    fn previous_sessions_filter_candidates_and_return_closed_at() {
        let projects = vec![project("P100", "History", false, false)];
        let trusted = previous_session(
            "G100",
            "Trusted title",
            "stopped",
            "workspace",
            Some("2026-06-06T12:00:00.000Z"),
            "2026-06-06T12:30:00.000Z",
            "2026-06-01T09:00:00.000Z",
        );
        let placeholder = previous_session(
            "G101",
            "Search by Text",
            "stopped",
            "workspace",
            Some("2026-06-07T12:00:00.000Z"),
            "2026-06-07T12:30:00.000Z",
            "2026-06-07T09:00:00.000Z",
        );
        let mut favorite_placeholder = previous_session(
            "G102",
            "Search by Text",
            "stopped",
            "workspace",
            None,
            "2026-06-05T12:30:00.000Z",
            "2026-06-05T09:00:00.000Z",
        );
        favorite_placeholder
            .as_object_mut()
            .expect("favorite object")
            .insert(
                "sessionTag".to_string(),
                Value::String("favorite".to_string()),
            );
        let mut command_pinned = previous_session(
            "G103",
            "Pinned command",
            "stopped",
            "commands",
            Some("2026-06-08T12:00:00.000Z"),
            "2026-06-08T12:30:00.000Z",
            "2026-06-08T09:00:00.000Z",
        );
        command_pinned
            .as_object_mut()
            .expect("command object")
            .insert("isPinned".to_string(), Value::Bool(true));
        let running = previous_session(
            "G104",
            "Running",
            "running",
            "workspace",
            Some("2026-06-09T12:00:00.000Z"),
            "2026-06-09T12:30:00.000Z",
            "2026-06-09T09:00:00.000Z",
        );

        let result = search_previous_sessions(
            projects,
            vec![
                trusted,
                placeholder,
                favorite_placeholder,
                command_pinned,
                running,
            ],
            &Map::new(),
        )
        .expect("previous sessions");
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .expect("results");

        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.get("sessionId").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec!["G100", "G102"]
        );
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.get("closedAt").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec!["2026-06-06T12:00:00.000Z", "2026-06-05T12:30:00.000Z"]
        );

        let query_params = json!({ "query": "09:00:00.000Z" });
        let query_result = search_previous_sessions(
            vec![project("P100", "History", false, false)],
            vec![
                previous_session(
                    "G100",
                    "Trusted title",
                    "stopped",
                    "workspace",
                    Some("2026-06-06T12:00:00.000Z"),
                    "2026-06-06T12:30:00.000Z",
                    "2026-06-01T09:00:00.000Z",
                ),
                previous_session(
                    "G102",
                    "Favorite title",
                    "stopped",
                    "workspace",
                    None,
                    "2026-06-05T12:30:00.000Z",
                    "2026-06-05T10:00:00.000Z",
                ),
            ],
            query_params.as_object().expect("query params"),
        )
        .expect("previous session query");
        let query_results = query_result
            .get("results")
            .and_then(Value::as_array)
            .expect("query results");
        assert_eq!(query_results.len(), 1);
        assert_eq!(
            query_results[0].get("sessionId").and_then(Value::as_str),
            Some("G100")
        );
        assert_eq!(
            query_results[0]
                .get("match")
                .and_then(Value::as_object)
                .and_then(|matched| matched.get("field"))
                .and_then(Value::as_str),
            Some("timestamp")
        );
    }

    #[test]
    fn previous_sessions_rank_by_close_time_then_session_id() {
        let projects = vec![project("P100", "History", false, false)];
        let closed_recent = previous_session(
            "G1close",
            "Closed recently",
            "stopped",
            "workspace",
            Some("2026-06-06T12:00:00.000Z"),
            "2026-06-06T12:00:00.000Z",
            "2026-06-01T09:00:00.000Z",
        );
        let active_before_close = previous_session(
            "G2active",
            "Active before close",
            "stopped",
            "workspace",
            Some("2026-06-05T12:00:00.000Z"),
            "2026-06-05T12:00:00.000Z",
            "2026-06-07T09:00:00.000Z",
        );
        let metadata_edited = previous_session(
            "G3meta",
            "Metadata edited after close",
            "stopped",
            "workspace",
            Some("2026-06-04T12:00:00.000Z"),
            "2026-06-08T12:00:00.000Z",
            "2026-06-04T09:00:00.000Z",
        );
        let same_time_later_id = previous_session(
            "G9same",
            "Same close later id",
            "stopped",
            "workspace",
            Some("2026-06-06T12:00:00.000Z"),
            "2026-06-06T12:00:00.000Z",
            "2026-06-06T09:00:00.000Z",
        );
        let same_time_earlier_id = previous_session(
            "G0same",
            "Same close earlier id",
            "stopped",
            "workspace",
            Some("2026-06-06T12:00:00.000Z"),
            "2026-06-06T12:00:00.000Z",
            "2026-06-06T09:00:00.000Z",
        );

        let result = search_previous_sessions(
            projects,
            vec![
                active_before_close,
                metadata_edited,
                same_time_later_id,
                closed_recent,
                same_time_earlier_id,
            ],
            &Map::new(),
        )
        .expect("previous sessions");
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .expect("results");

        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.get("sessionId").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec!["G0same", "G1close", "G9same", "G2active", "G3meta"]
        );
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.get("closedAt").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec![
                "2026-06-06T12:00:00.000Z",
                "2026-06-06T12:00:00.000Z",
                "2026-06-06T12:00:00.000Z",
                "2026-06-05T12:00:00.000Z",
                "2026-06-04T12:00:00.000Z"
            ]
        );
    }

    #[test]
    fn search_normalizes_unicode_query_project_id_and_bad_tags_like_typescript() {
        let projects = vec![
            project("P100", "Unicode", false, false),
            project("P200", "Other", false, false),
        ];
        let sessions = vec![
            session("P100", "G100", "\u{00dc}ber Build", "running", 1000.0),
            session("P200", "G200", "Other Build", "running", 2000.0),
        ];

        let params = json!({
            "projectId": "",
            "query": "\u{00fc}ber",
        });
        let result = search_sessions(
            projects.clone(),
            sessions.clone(),
            params.as_object().expect("params object"),
        )
        .expect("unicode search");
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .expect("results");
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].get("sessionId").and_then(Value::as_str),
            Some("G100")
        );

        let truthy_project_id = json!({ "projectId": true });
        let result = search_sessions(
            projects.clone(),
            sessions.clone(),
            truthy_project_id.as_object().expect("params object"),
        )
        .expect("truthy project id search");
        assert_eq!(
            result
                .get("results")
                .and_then(Value::as_array)
                .expect("results")
                .len(),
            0
        );

        let bad_tags = json!({ "sessionTags": "favorite" });
        let error = search_sessions(
            projects,
            sessions,
            bad_tags.as_object().expect("params object"),
        )
        .expect_err("bad sessionTags should match TypeScript internal error");
        assert_eq!(error.code, "internalError");
        assert_eq!(error.message, "values?.filter is not a function");
    }

    #[test]
    fn search_includes_untagged_sessions_when_untagged_filter_is_selected() {
        let projects = vec![project("P100", "Tags", false, false)];
        let mut tagged = session("P100", "G-tagged", "Tagged", "running", 1000.0);
        tagged
            .as_object_mut()
            .expect("session object")
            .insert("sessionTag".to_string(), json!("in-progress"));
        let untagged = session("P100", "G-untagged", "Untagged", "running", 2000.0);
        let mut favorite = session("P100", "G-favorite", "Favorite", "running", 3000.0);
        favorite
            .as_object_mut()
            .expect("session object")
            .insert("isFavorite".to_string(), json!(true));
        let sessions = vec![tagged, untagged, favorite];

        let result = search_sessions(
            projects.clone(),
            sessions.clone(),
            json!({ "sessionTags": ["untagged"] })
                .as_object()
                .expect("params object"),
        )
        .expect("untagged search");
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .expect("results");
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].get("sessionId").and_then(Value::as_str),
            Some("G-untagged")
        );

        let mixed_result = search_sessions(
            projects,
            sessions,
            json!({ "sessionTags": ["untagged", "in-progress"] })
                .as_object()
                .expect("params object"),
        )
        .expect("mixed tag search");
        let mixed_ids: Vec<&str> = mixed_result
            .get("results")
            .and_then(Value::as_array)
            .expect("results")
            .iter()
            .filter_map(|row| row.get("sessionId").and_then(Value::as_str))
            .collect();
        assert_eq!(mixed_ids.len(), 2);
        assert!(mixed_ids.contains(&"G-untagged"));
        assert!(mixed_ids.contains(&"G-tagged"));
        assert!(!mixed_ids.contains(&"G-favorite"));
    }

    #[test]
    fn search_does_not_treat_provider_off_rows_as_active() {
        let projects = vec![project("P100", "Provider Off", false, false)];
        let mut provider_off = session("P100", "G100", "Provider off", "unknown", 1000.0);
        provider_off
            .as_object_mut()
            .expect("session object")
            .insert(
                "providerState".to_string(),
                json!({ "lifecycleState": "exists", "provider": "zmx" }),
            );
        provider_off
            .as_object_mut()
            .expect("session object")
            .insert(
                "runtimeSettings".to_string(),
                json!({ "sessionPersistenceProvider": "off" }),
            );
        let params = json!({
            "includeActive": true,
            "includePrevious": false,
        });

        let result = search_sessions(
            projects,
            vec![provider_off],
            params.as_object().expect("params object"),
        )
        .expect("provider off search");
        assert_eq!(
            result
                .get("results")
                .and_then(Value::as_array)
                .expect("results")
                .len(),
            0
        );
    }

    #[test]
    fn previous_sessions_reject_non_restorable_title_noise() {
        let projects = vec![project("P100", "History", false, false)];
        let trusted = previous_session(
            "G100",
            "Trusted title",
            "stopped",
            "workspace",
            Some("2026-06-06T12:00:00.000Z"),
            "2026-06-06T12:30:00.000Z",
            "2026-06-01T09:00:00.000Z",
        );
        let path_title = previous_session(
            "G101",
            "/Users/madda/private",
            "stopped",
            "workspace",
            Some("2026-06-07T12:00:00.000Z"),
            "2026-06-07T12:30:00.000Z",
            "2026-06-07T09:00:00.000Z",
        );
        let command_title = previous_session(
            "G102",
            "codex resume 019e7f01-8243-7aa1-88db-dd84ebcf6aa4",
            "stopped",
            "workspace",
            Some("2026-06-08T12:00:00.000Z"),
            "2026-06-08T12:30:00.000Z",
            "2026-06-08T09:00:00.000Z",
        );
        let generic_title = previous_session(
            "G103",
            "Terminal Session",
            "stopped",
            "workspace",
            Some("2026-06-09T12:00:00.000Z"),
            "2026-06-09T12:30:00.000Z",
            "2026-06-09T09:00:00.000Z",
        );
        let gx_id_title = previous_session(
            "G104",
            "G1abc",
            "stopped",
            "workspace",
            Some("2026-06-10T12:00:00.000Z"),
            "2026-06-10T12:30:00.000Z",
            "2026-06-10T09:00:00.000Z",
        );

        let result = search_previous_sessions(
            projects,
            vec![
                trusted,
                path_title,
                command_title,
                generic_title,
                gx_id_title,
            ],
            &Map::new(),
        )
        .expect("previous sessions");
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .expect("results");
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.get("sessionId").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec!["G100"]
        );
    }

    #[test]
    fn search_result_title_projection_matches_generic_type_script_rows() {
        let projects = vec![project("P100", "Titles", false, false)];
        let sessions = vec![session(
            "P100",
            "G100",
            "Terminal Session",
            "running",
            1000.0,
        )];

        let result = search_sessions(projects, sessions, &Map::new()).expect("search sessions");
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .expect("results");
        let row = results.first().expect("search row");
        assert_eq!(row.get("titleSource").and_then(Value::as_str), Some("user"));
        assert_eq!(
            row.get("isTemporaryTitle").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(row.get("trustedResumeTitle"), None);
        assert_eq!(
            row.get("displayTitle").and_then(Value::as_str),
            Some("\u{2217} Terminal Session")
        );
        assert_eq!(
            row.get("displayTitleTooltip").and_then(Value::as_str),
            Some("\u{2217} Terminal Session (Unsynced title)")
        );
    }

    #[test]
    fn title_projection_strips_factory_droid_status_marker_from_stored_title() {
        let mut session = session("P100", "G100", "\u{26ec} New Session", "running", 1000.0);
        let session_object = session.as_object_mut().expect("session object");
        session_object.insert("agentId".to_string(), json!("droid"));
        session_object.insert(
            "runtimeSettings".to_string(),
            json!({
                "agentName": "factory droid",
                "titleSource": "terminal-auto"
            }),
        );

        let projection = project_session_title_projection(&session);

        assert_eq!(
            projection.get("displayTitle").and_then(Value::as_str),
            Some("New Session")
        );
        assert_eq!(
            projection
                .get("displayTitleTooltip")
                .and_then(Value::as_str),
            Some("New Session")
        );
        assert_eq!(
            projection.get("primaryTitle").and_then(Value::as_str),
            Some("New Session")
        );
        assert_eq!(
            projection.get("trustedResumeTitle").and_then(Value::as_str),
            Some("New Session")
        );
        assert_eq!(
            projection.get("title").and_then(Value::as_str),
            Some("\u{26ec} New Session")
        );
    }

    #[test]
    fn title_projection_strips_omp_idle_and_spinner_prefixes() {
        for raw_title in [
            "  \u{03c0} >   Delete marketplace skill and inventory skills  ",
            "  \u{03c0} \u{2827}   Delete marketplace skill and inventory skills  ",
        ] {
            let mut session = session("P100", "G100", raw_title, "running", 1000.0);
            let session_object = session.as_object_mut().expect("session object");
            session_object.insert("agentId".to_string(), json!("omp"));
            session_object.insert(
                "runtimeSettings".to_string(),
                json!({ "agentName": "omp", "titleSource": "terminal-auto" }),
            );

            let projection = project_session_title_projection(&session);

            assert_eq!(
                projection.get("displayTitle").and_then(Value::as_str),
                Some("Delete marketplace skill and inventory skills")
            );
            assert_eq!(
                projection
                    .get("displayTitleTooltip")
                    .and_then(Value::as_str),
                Some("Delete marketplace skill and inventory skills")
            );
            assert_eq!(
                projection.get("primaryTitle").and_then(Value::as_str),
                Some("Delete marketplace skill and inventory skills")
            );
        }
    }

    #[test]
    fn title_projection_replaces_wsl_shell_location_with_terminal_default_title() {
        let mut session = session(
            "P100",
            "G100",
            "madda@M7-Desktop: /mnt/c/dev/Ghostex",
            "running",
            1000.0,
        );
        let session_object = session.as_object_mut().expect("session object");
        session_object.insert(
            "runtimeSettings".to_string(),
            json!({ "titleSource": "terminal-auto" }),
        );

        let projection = project_session_title_projection(&session);

        assert_eq!(
            projection.get("primaryTitle").and_then(Value::as_str),
            Some("Terminal Session")
        );
        assert_eq!(projection.get("trustedResumeTitle"), None);
        assert_eq!(
            projection.get("displayTitle").and_then(Value::as_str),
            Some("\u{2217} Terminal Session")
        );
    }

    fn project(project_id: &str, name: &str, is_pinned: bool, is_favorite: bool) -> Value {
        json!({
            "createdAt": "2026-06-15T09:55:00.000Z",
            "isFavorite": is_favorite,
            "isPinned": is_pinned,
            "name": name,
            "projectId": project_id,
            "updatedAt": "2026-06-15T09:55:00.000Z",
        })
    }

    fn session(
        project_id: &str,
        session_id: &str,
        title: &str,
        lifecycle_state: &str,
        sidebar_order: f64,
    ) -> Value {
        json!({
            "createdAt": "2026-06-15T09:55:00.000Z",
            "isFavorite": false,
            "isPinned": false,
            "kind": "terminal",
            "lifecycleState": lifecycle_state,
            "projectId": project_id,
            "providerState": { "lifecycleState": "missing", "provider": "zmx" },
            "runtimeSettings": {},
            "sessionId": session_id,
            "sidebarOrder": sidebar_order,
            "surface": "workspace",
            "title": title,
            "updatedAt": "2026-06-15T09:55:00.000Z",
            "zmxName": format!("S7k-{project_id}-{session_id}"),
        })
    }

    fn previous_session(
        session_id: &str,
        title: &str,
        lifecycle_state: &str,
        surface: &str,
        probed_at: Option<&str>,
        updated_at: &str,
        last_active_at: &str,
    ) -> Value {
        let provider_state = match probed_at {
            Some(probed_at) => json!({
                "lifecycleState": "missing",
                "probedAt": probed_at,
                "provider": "zmx",
                "zmxName": format!("S7k-P100-{session_id}"),
            }),
            None => json!({
                "lifecycleState": "missing",
                "provider": "zmx",
                "zmxName": format!("S7k-P100-{session_id}"),
            }),
        };
        json!({
            "agentId": "codex",
            "createdAt": "2026-06-01T08:00:00.000Z",
            "isFavorite": false,
            "isPinned": false,
            "kind": "agent",
            "lastActiveAt": last_active_at,
            "lifecycleState": lifecycle_state,
            "projectId": "P100",
            "providerState": provider_state,
            "runtimeSettings": {
                "titleSource": if title == "Search by Text" { "placeholder" } else { "terminal-auto" },
            },
            "sessionId": session_id,
            "surface": surface,
            "title": title,
            "updatedAt": updated_at,
            "zmxName": format!("S7k-P100-{session_id}"),
        })
    }

    fn open_test_database() -> (tempfile::TempDir, Connection) {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        (temp, db)
    }
}
