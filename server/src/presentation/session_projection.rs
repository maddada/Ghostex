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
    // The home project, whose header is Main Hub's row (chief/home.rs). Present only when true.
    if crate::chief::is_home_project(project) && crate::chief::chief_shown() {
        output.insert("isHome".to_string(), Value::Bool(true));
    }
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
    // CDXC:Bots 2026-09-26 WHY:
    // The snapshot's project row is what every client holds from its first frame; the domain row with `launchSettings` only reaches a client in a later add or update delta, so a bot flag read from there vanished on every reload.
    if let Some(profile) = crate::bot_projects::bot_profile(project) {
        output.insert("botProfile".to_string(), Value::String(profile.to_string()));
        output.insert(
            "botGatewayRunning".to_string(),
            Value::Bool(crate::bot_projects::published_bot_gateway_running(profile)),
        );
        output.insert(
            "botRunsToday".to_string(),
            Value::from(crate::bot_feed::published_bot_runs_today(profile)),
        );
    }
    if let Some(bot_id) = crate::gbots::gbots_bot_id(project) {
        output.insert("botProfile".to_string(), Value::String(bot_id.to_string()));
        output.insert("botKind".to_string(), Value::String("gbots".to_string()));
        output.insert(
            "botActivity".to_string(),
            Value::String(crate::gbots::published_bot_activity(bot_id).to_string()),
        );
    }
    if let Some(project_id) = project.get("projectId").and_then(Value::as_str) {
        insert_issues_workflow_fields(
            &mut output,
            crate::work_mode::issues_workflow_parts(project),
            crate::work_mode::cached_project_work_tracker(project_id),
        );
        if let Some(work_source) = crate::work_mode::presentation_work_source(project) {
            output.insert("workSource".to_string(), work_source);
        }
    }
    // Settings > Autopilot covers this project and its worktrees (the sidebar's Advanced row).
    if let Some(inside) = crate::autopilot::project_presentation(project) {
        output.insert("autopilot".to_string(), Value::Bool(inside));
    }
    if crate::workspaces::workspaces_feature_enabled() {
        if let Some(workspace_id) = crate::workspaces::stored_project_workspace_id(project) {
            output.insert("workspaceId".to_string(), json!(workspace_id));
        }
        if crate::workspaces::project_in_every_workspace(project) {
            output.insert("everyWorkspace".to_string(), Value::Bool(true));
        }
    }
    Value::Object(output)
}

/// `workMode`, `workLinear`, `workGithubOff` and `workTracker` on a project row
/// (`PresentationProject` in packages/gx-protocol/src/presentation.rs): Issues Workflow is on once
/// a Linear key applies, `gh` is signed in or the workspace has a ticket command (CDXC:WorkMode
/// 2026-10-24 in server/src/work_mode/project.rs). `workGithubOff` is sent only when false would mislead: a
/// daemon too old to send it had no such rule, so its absence means GitHub's parts are on.
///
/// CDXC:WorkMode 2026-11-04 WHY:
/// With a ticket command as the tracker, `workTracker` and `workLinear` are left out and `workSource.isTracker` says so. Clients that know only Linear and GitHub read `workTracker` as one of the two and `workLinear` as "offer Linear's Create and Link to", so either would offer the wrong tracker's tickets; without them they offer neither, and the PR rows stay (`workGithubOff` is about the code host, not the tracker).
pub(crate) fn insert_issues_workflow_fields(
    output: &mut Map<String, Value>,
    parts: crate::work_mode::IssuesWorkflowParts,
    tracker: crate::work_mode::WorkSourceId,
) {
    if !parts.any() {
        return;
    }
    output.insert("workMode".to_string(), Value::Bool(true));
    let command_tracker = matches!(tracker, crate::work_mode::WorkSourceId::Command(_));
    if parts.linear && !command_tracker {
        output.insert("workLinear".to_string(), Value::Bool(true));
        if parts.linear_create {
            output.insert("workLinearCreate".to_string(), Value::Bool(true));
        }
    }
    if !parts.github {
        output.insert("workGithubOff".to_string(), Value::Bool(true));
    }
    if !command_tracker {
        output.insert("workTracker".to_string(), json!(tracker.as_wire()));
    }
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
        .filter(|value| is_presentation_git_action(value))
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
    // The open ```questions forms and question pages count too, while the agent works as well
    // (CDXC:SessionStatus 2026-10-20 in server/src/session_chat_open_questions.rs). Only a running
    // session publishes them; `attention.kind` reads them in every lifecycle state.
    let unanswered_question_count =
        crate::session_chat_async_questions::pending_question_count(session)
            + crate::session_chat_open_questions::open_question_count(session);
    output.insert(
        "pendingQuestionCount".to_string(),
        Value::from(if lifecycle_state == "running" {
            unanswered_question_count
        } else {
            0
        }),
    );
    // Present only while true, like a daemon that predates it (CDXC:SessionStatus 2026-10-27 in
    // server/src/session_approval.rs).
    let awaiting_approval = crate::session_approval::session_awaits_approval(session, generated_at);
    if awaiting_approval {
        output.insert("awaitingApproval".to_string(), Value::Bool(true));
    }
    // Present only while the session is stuck: working with no output for long enough, or a
    // thread idle although a background job waits for it, so the sidebar draws the red
    // not-moving dot (server/src/coordinators/stall.rs, idle_wait.rs).
    if matches!(lifecycle_state.as_str(), "running" | "sleeping") {
        let project_id = string_field(session, "projectId").unwrap_or_default();
        let session_id = string_field(session, "sessionId").unwrap_or_default();
        if let Some(since) = crate::coordinators::session_stuck_since(&project_id, &session_id)
            .and_then(crate::coordinators::iso_from_ms)
        {
            output.insert("stuckSince".to_string(), Value::String(since));
            // CDXC:Notifications 2026-11-08: the red dot shows, but alerts only once nobody heals
            // it (packages/gx-core/src/alert_rule.rs).
            if let Some(by) = super::stuck_handled_by(&project_id, &session_id) {
                output.insert("stuckHandledBy".to_string(), Value::String(by.to_string()));
            }
        }
    }
    // Present only while blocked: every account out of usage, or an error recovery could not get
    // past (CDXC:Notifications 2026-11-08 in packages/gx-core/src/alert_rule.rs).
    if let Some((since, reason)) = super::blocked_state(session) {
        output.insert("blockedSince".to_string(), Value::String(since));
        output.insert("blockedReason".to_string(), Value::String(reason));
    }
    // Present only while true (CDXC:SessionStatus 2026-10-31 in server/src/session_approval.rs).
    if crate::session_approval::session_awaits_answer(session) {
        output.insert("awaitingAnswer".to_string(), Value::Bool(true));
    }
    insert_optional_string(
        &mut output,
        "agentName",
        read_runtime_text(session, "agentName")
            .or_else(|| custom_agent_display_name(Some(project), session))
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
    for key in ["accountId", "accountName", "accountSlot"] {
        insert_optional_string(&mut output, key, read_runtime_text(session, key));
    }
    if let Some(switchable_agents) =
        crate::agents::switchable_session_agents_value(project, session)
    {
        output.insert("switchableAgents".to_string(), switchable_agents);
    }
    // CDXC:AgentBox 2026-10-01 SEE-ALSO: `PresentationSession.agentbox` in packages/gx-protocol/src/presentation.rs and `GxserverPresentationSession.agentbox` in packages/shared/gxserver-protocol-presentation.ts; absent for a session that runs on this computer.
    if let Some(agentbox) = crate::agentbox::presentation_agentbox_value(session) {
        output.insert("agentbox".to_string(), agentbox);
    }
    if let Some(cloud) = crate::claude_cloud::presentation_cloud_value(session) {
        output.insert("claudeCloud".to_string(), cloud);
    }
    if let Some(cloud_box) = crate::cloud_boxes::presentation::presentation_cloud_box_value(session)
    {
        output.insert("cloudBox".to_string(), cloud_box);
    }
    // CDXC:GBots 2026-11-07 SEE-ALSO: the phone's Bots space groups a GBot's conversations under the bot with its status from this (apps/mobile/app/src/sessions/gbots.ts); mobile_summary.rs forwards it.
    if let Some(bot_id) = crate::gbots::gbots_bot_id(project) {
        output.insert(
            "gbot".to_string(),
            serde_json::json!({
                "botId": bot_id,
                "name": project.get("name").and_then(Value::as_str).unwrap_or_default(),
                "activity": crate::gbots::published_bot_activity(bot_id),
            }),
        );
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
    // A side chat names its main session; the session lists leave it out (CDXC:SideChats
    // 2026-11-02 in server/src/side_chats/mod.rs).
    insert_optional_string(
        &mut output,
        "sideChatParentSessionId",
        crate::side_chats::parent_of(session),
    );
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
    // CDXC:WorkMode 2026-10-09 SEE-ALSO: `PresentationSession.work` in packages/gx-protocol/src/presentation.rs; absent outside work mode, and built from caches only (server/src/work_mode/).
    insert_optional_value(
        &mut output,
        "work",
        crate::work_mode::presentation_session_work(project, session),
    );
    // CDXC:SessionLinks 2026-10-24 SEE-ALSO: `PresentationSession.links` in packages/gx-protocol/src/presentation.rs; every session, built from caches only (server/src/session_links/).
    insert_optional_value(
        &mut output,
        "links",
        crate::session_links::presentation_session_links(project, session),
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
    // A side chat is named by its question, not by the name its forked agent reports.
    if let Some(side_chat_title) = crate::side_chats::side_chat_title(session) {
        for key in ["title", "primaryTitle", "displayTitle"] {
            output.insert(key.to_string(), Value::String(side_chat_title.clone()));
        }
    }
    // In work mode a session the person never renamed is titled by its branch.
    if let Some(branch_title) = crate::work_mode::work_display_title(
        project,
        session,
        output.get("titleSource").and_then(Value::as_str),
    ) {
        output.insert("displayTitle".to_string(), Value::String(branch_title));
    }
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
    insert_optional_string(
        &mut output,
        "backgroundWorkDetectedAt",
        session_background_work_detected_at(session),
    );
    insert_optional_string(
        &mut output,
        "modelSelectionFailure",
        crate::session_chat_model_selection_alert::session_model_selection_failure(session),
    );
    insert_optional_string(
        &mut output,
        "sendFailure",
        crate::session_chat_send_failure::session_send_failure_dot(session),
    );
    if let (Some(project_id), Some(session_id)) = (
        string_field(session, "projectId"),
        string_field(session, "sessionId"),
    ) {
        if let Some(program_status) =
            crate::program_status::presentation_program_status(&project_id, &session_id)
        {
            output.insert("programStatus".to_string(), program_status);
        }
    }
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
            .then(|| {
                crate::session_chat_compacting::session_chat_compacting_detected_at(session)
                    .or_else(|| {
                        crate::session_chat_compacting::session_chat_fleet_detected_at(session)
                    })
            })
            .flatten()
    })
}

/// CDXC:SessionStatus 2026-09-22 DECISION:
/// User: a Claude session whose footer still lists a background shell (or monitor) after its turn shows a grey dot in the sidebar instead of working or idle. The value is the screen detection's stable `detectedAt`, present only while the session runs and the footer still shows the line.
pub(crate) fn session_background_work_detected_at(session: &Value) -> Option<String> {
    (effective_lifecycle_state(session) == "running")
        .then(|| crate::session_chat_compacting::session_chat_monitor_detected_at(session))
        .flatten()
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
