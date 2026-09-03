use serde_json::{json, Map, Value};

use crate::agents::session_is_draft;
use crate::session_status::{effective_working_started_at, meaningful_activity_at};

use super::*;

pub(crate) fn project_presentation_project(project: &Value) -> Value {
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
    CDXC:StateSync 2026-07-29-00:00:
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
    CDXC:Icons 2026-07-29 (discovered icons):
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

pub(crate) fn project_presentation_git_config(project: &Value) -> Option<Value> {
    /*
    CDXC:Git 2026-06-24-18:22:
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

pub(crate) fn is_presentation_git_action(value: &str) -> bool {
    matches!(
        value,
        "commit" | "push" | "pr" | "syncRemote" | "syncMain" | "multiRelease" | "release"
    )
}

pub(crate) fn project_presentation_session(
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
        output.insert(
            "agentIcon".to_string(),
            Value::String(session_agent_icon(Some(project), session).unwrap_or(agent_id)),
        );
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
    /*
    CDXC:AgentProviders 2026-09-03:
    The accounts this session can be resumed under, resolved by the daemon that
    owns the project so every surface (sidebar context menu, terminal action
    bar, chat composer) renders the same rows. ABSENT when there is nothing to
    switch to, which is also what a daemon predating the feature publishes.
    */
    if let Some(switchable_agents) =
        crate::agents::switchable_session_agents_value(project, session)
    {
        output.insert("switchableAgents".to_string(), switchable_agents);
    }
    if activity == "attention" {
        output.insert(
            "attention".to_string(),
            attention_state(session, generated_at),
        );
    }
    /*
    CDXC:CommandPane 2026-08-08:
    Command-pane clients need the stable saved Action id to find the daemon
    session that already owns that Action. Publish only that identifier;
    command text and launch settings remain outside presentation snapshots.
    */
    insert_optional_string(&mut output, "commandId", string_field(session, "commandId"));
    output.insert("createdAt".to_string(), value_field(session, "createdAt"));
    insert_optional_js_truthy_value(&mut output, "cwd", session.get("cwd").cloned());
    /*
    CDXC:Git 2026-07-29-00:00:
    Sidebar V2's card row reads branch / +n −n / PR badge from server-owned state.
    This is a READ of the background probe cache keyed by the session cwd — never
    a probe — so building a snapshot stays a pure in-memory projection. A cwd the
    background pass has not reached yet, a cwd outside any repository, and an
    older remote daemon all publish the same thing: no `gitStatus` key at all.

    CDXC:Git 2026-07-30 (effective cwd):
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
    /*
    CDXC:SessionSleep 2026-08-22:
    `lastActiveAt` below falls back to `createdAt` so sorting and Last Active
    labels always have a timestamp, which leaves a session that has NEVER been
    active indistinguishable from one last active when it was created. Auto
    Sleep needs exactly that difference: an agent terminal nobody has prompted
    yet has no conversation to resume, because the agent publishes its session
    id at startup but writes no transcript until the first prompt. Sleeping one
    kills a provider whose stored resume reference points at a conversation
    that never existed, so the row can never be woken back into that agent.
    Publish the raw fact instead of asking clients to infer it from a timestamp
    this projection already rewrote.
    */
    output.insert(
        "hasEverBeenActive".to_string(),
        Value::Bool(
            string_field(session, "lastActiveAt").is_some_and(|value| !value.trim().is_empty()),
        ),
    );
    /*
    CDXC:Drafts 2026-08-28:
    A session created from the sidebar that has never received a user prompt.
    Published as a PRESENT-ONLY key — never `false` — so it reads identically to
    what a daemon predating drafts sends, and so promotion clears it through the
    same whole-object session upsert every other field uses. The sidebar swaps
    the agent logo for a pencil and dims the row on the strength of this alone.
    */
    if session_is_draft(session) {
        output.insert("isDraft".to_string(), Value::Bool(true));
    }
    output.insert("isFavorite".to_string(), Value::Bool(is_favorite(session)));
    /*
    CDXC:SessionTitles 2026-07-02-15:10:
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
    output.insert("isParked".to_string(), value_field(session, "isParked"));
    merge_object(&mut output, title);
    output.insert("kind".to_string(), value_field(session, "kind"));
    output.insert(
        "lastActiveAt".to_string(),
        Value::String(last_active_at(session)),
    );
    output.insert("lifecycleState".to_string(), Value::String(lifecycle_state));
    /*
    CDXC:AgentScreenDetection 2026-07-29-12:00:
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
    CDXC:StateSync 2026-07-29-00:00:
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
    effective_working_started_at(session_agent_activity(session), generated_at_ms).or_else(|| {
        (presentation_activity(session, generated_at) == "working")
            .then(|| crate::session_chat_compacting::session_chat_compacting_detected_at(session))
            .flatten()
    })
}

pub(crate) fn session_agent_activity(session: &Value) -> Option<&Value> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"))
}

pub(crate) fn search_session_persistence_provider(session: &Value) -> Option<String> {
    let value = read_session_persistence_provider(session)?;
    matches!(value.as_str(), "tmux" | "zmx" | "zellij").then_some(value)
}

pub(crate) fn search_session_persistence_name(session: &Value, provider: &str) -> Option<String> {
    if provider == "zmx" {
        return read_provider_trimmed_text(session, "zmxName")
            .or_else(|| string_field(session, "zmxName"));
    }
    read_provider_trimmed_text(session, "providerName")
        .or_else(|| read_runtime_text(session, "sessionPersistenceName"))
}

pub(crate) fn read_session_persistence_provider(session: &Value) -> Option<String> {
    read_runtime_text(session, "sessionPersistenceProvider")
        .or_else(|| read_provider_trimmed_text(session, "provider"))
}

/*
CDXC:StateSync 2026-06-22-06:36:
Presentation snapshots are active-focused state, not full stopped history. Match TypeScript by keeping all active sessions, capping explicitly pinned/favorite/tagged stopped rows to the first 20 per project by presentation sort key, and treating null or empty tags as absent.
*/
pub(crate) fn select_presentation_sessions(sessions: Vec<Value>) -> Vec<Value> {
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
    /*
    CDXC:Sessions 2026-09-01:
    The cap picks by recency, newest first. It used to take the first 20 by
    presentation sort key, which orders lastActiveAt ascending — so in a
    project with more than 20 pinned/tagged stopped rows the OLDEST ones held
    every slot and a freshly stopped pinned session never appeared. The caller
    re-sorts the returned rows for display (snapshot.rs), so selection order
    here does not leak.
    */
    pinned_stopped.sort_by_key(|session| std::cmp::Reverse(last_active_at(session)));
    active.extend(
        pinned_stopped
            .into_iter()
            .take(RECENT_STOPPED_LIMIT_PER_PROJECT),
    );
    active
}
