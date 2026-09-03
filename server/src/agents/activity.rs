use std::path::Path;

use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::*;
use crate::domain::{DomainRepository, DomainStateError};
use crate::presentation::project_session_title_projection;
use crate::session_status::{
    compute_activity_update, is_stale_activity_event, normalize_agent_activity_value, parse_iso_ms,
    ActivityUpdate,
};

pub(crate) const FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY: &str =
    "gxserverFirstPromptAutoTitleAttemptId";
pub(crate) fn cancel_first_prompt_auto_title(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let session = require_session(repository, lifecycle)?;
    let runtime_settings = object_field(&session, "runtimeSettings");
    let previous_status =
        read_text_from_map(&runtime_settings, "gxserverFirstPromptAutoTitleStatus");
    if previous_status.as_deref() != Some("running") {
        let mut result = Map::new();
        result.insert("changed".to_string(), Value::Bool(false));
        if let Some(status) = previous_status.clone() {
            result.insert("previousStatus".to_string(), Value::String(status.clone()));
            result.insert(
                "reason".to_string(),
                Value::String(format!("already-{status}")),
            );
        } else {
            result.insert(
                "reason".to_string(),
                Value::String("not-running".to_string()),
            );
        }
        result.insert("session".to_string(), session);
        return Ok(Value::Object(result));
    }
    let mut next_runtime = runtime_settings;
    next_runtime.insert(
        "gxserverFirstPromptAutoTitleCancelledAt".to_string(),
        json!(now_iso()),
    );
    if let Some(prompt) = read_text_from_map(&next_runtime, "firstUserMessage") {
        next_runtime.insert(
            "gxserverFirstPromptAutoTitleCancelledPrompt".to_string(),
            json!(prompt),
        );
    }
    next_runtime.insert(
        "gxserverFirstPromptAutoTitleReason".to_string(),
        json!(read_text(params, "reason").unwrap_or_else(|| "userCancelled".to_string())),
    );
    next_runtime.insert(
        "gxserverFirstPromptAutoTitleStatus".to_string(),
        json!("cancelled"),
    );
    let mut update = lifecycle_update(lifecycle);
    update.insert("runtimeSettings".to_string(), Value::Object(next_runtime));
    let updated = repository.update_session(&update)?;
    Ok(json!({
        "changed": true,
        "previousStatus": "running",
        "reason": "cancelled",
        "session": updated,
    }))
}

pub(crate) fn update_agent_activity_endpoint(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let current = require_session(repository, lifecycle)?;
    if launch_agent_mismatch(&current, read_text(params, "agentName").as_deref()) {
        let previous = normalize_agent_activity_value(
            object_field(&current, "runtimeSettings").get("agentActivity"),
            "idle",
        );
        return Ok(json!({
            "activity": previous,
            "enteredAttention": false,
            "previousActivity": previous.get("activity").and_then(Value::as_str).unwrap_or("idle"),
            "session": current,
        }));
    }
    let mut update = compute_activity_update(&current, params, None);
    // Explicit activity RPCs (bell, escape, acknowledge, …) must not erase a
    // pending Session Chat card; hook ingest and the transcript retire it.
    carry_session_chat_prompt(&current, &mut update.activity);
    let mut runtime_settings = object_field(&current, "runtimeSettings");
    runtime_settings.insert("agentActivity".to_string(), update.activity.clone());
    promote_draft_on_first_activity(
        &current,
        &mut runtime_settings,
        update.last_active_at.as_deref(),
    );
    let mut session_update = lifecycle_update(lifecycle);
    session_update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    if let Some(last_active_at) = update.last_active_at.clone() {
        session_update.insert("lastActiveAt".to_string(), json!(last_active_at));
    }
    let session = repository.update_session(&session_update)?;
    Ok(json!({
        "activity": update.activity,
        "enteredAttention": update.entered_attention,
        "previousActivity": update.previous_activity,
        "session": session,
    }))
}

pub(crate) fn ingest_agent_hook_event(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
    home_dir: &Path,
) -> Result<Value, DomainStateError> {
    let current = require_session(repository, lifecycle)?;
    let mut hook_activity = normalize_agent_hook_activity(
        params.get("status"),
        params
            .get("eventName")
            .or_else(|| params.get("rawEventName")),
        params.get("agentName"),
    );
    /*
    CDXC:SessionChat 2026-08-24:
    The notify hook tags Claude's 60s "waiting for your input" reminder with
    notificationKind=idleInput. That reminder is proof the CLI is idle at its
    prompt, not a completion or request for user action, so it must never enter
    attention. Genuine permission notifications are not tagged idleInput and
    retain their attention transition. Older notify binaries omit the tag and
    keep their existing behavior.
    */
    if hook_activity.as_deref() == Some("attention")
        && params.get("notificationKind").and_then(Value::as_str) == Some("idleInput")
    {
        hook_activity = Some("idle".to_string());
    }
    let observed_identity = resolve_session_identity(&IdentityInput {
        agent_id: None,
        agent_name: read_text(params, "agentName"),
        agent_session_id: read_text(params, "agentSessionId"),
        agent_session_path: read_text(params, "agentSessionPath"),
        runtime_settings: Map::new(),
        startup_text: None,
    });
    if launch_agent_mismatch(&current, observed_identity.agent_id.as_deref()) {
        let previous = normalize_agent_activity_value(
            object_field(&current, "runtimeSettings").get("agentActivity"),
            "idle",
        );
        return Ok(json!({
            "activity": previous,
            "changed": false,
            "enteredAttention": false,
            "previousActivity": previous.get("activity").and_then(Value::as_str).unwrap_or("idle"),
            "projection": project_session_title_projection(&current),
            "reason": "agent-hook-agent-mismatch",
            "session": current,
        }));
    }
    let (metadata_result, mut session) = apply_session_state_update(
        repository,
        lifecycle,
        params,
        SessionIdentityUpdateSource::Passive,
    )?;
    let mut changed = metadata_result
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut reason = metadata_result
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("unchanged")
        .to_string();
    if reason == "passive-session-identity-conflict" {
        let previous = normalize_agent_activity_value(
            object_field(&current, "runtimeSettings").get("agentActivity"),
            "idle",
        );
        let mut result = object_from_value(json!({
            "activity": previous,
            "changed": false,
            "enteredAttention": false,
            "previousActivity": previous.get("activity").and_then(Value::as_str).unwrap_or("idle"),
            "projection": project_session_title_projection(&current),
            "reason": "passive-session-identity-conflict",
            "session": current,
        }));
        if let Some(conflict) = metadata_result.get("identityConflict").cloned() {
            result.insert("identityConflict".to_string(), conflict);
        }
        return Ok(Value::Object(result));
    }
    let mut activity_update: Option<ActivityUpdate> = None;
    let mut session_chat_prompt_changed = false;
    let mut session_chat_activity_changed = false;
    let mut activity_reason = if hook_activity.is_some() {
        "activity-unchanged".to_string()
    } else {
        "metadata-only".to_string()
    };
    if let Some(activity) = hook_activity {
        let now_ms = params
            .get("statusUpdatedAt")
            .and_then(Value::as_str)
            .and_then(parse_iso_ms)
            .unwrap_or_else(now_ms);
        let mut activity_params = params.clone();
        activity_params.insert("activity".to_string(), json!(activity));
        activity_params.insert(
            "nowMs".to_string(),
            Value::Number(serde_json::Number::from(now_ms)),
        );
        let mut update = compute_activity_update(&session, &activity_params, None);
        /*
        CDXC:SessionChat 2026-07-31:
        Session Chat interactive-prompt capture. compute_activity_update
        rebuilds agentActivity from a fixed struct, so the stored
        sessionChatPrompt must be explicitly re-attached (kept, replaced, or
        dropped) here or every activity write would erase it. The key is also
        in the persistable_agent_activity_snapshot whitelist so a prompt-only
        change forces a persist.
        */
        let session_chat_prompt_before = session_chat_prompt_setting(&session);
        let session_chat_prompt_after = next_session_chat_prompt_setting(
            session_chat_prompt_before.as_deref(),
            params,
            &activity,
        );
        session_chat_prompt_changed =
            session_chat_prompt_before.as_deref() != session_chat_prompt_after.as_deref();
        if let Some(prompt_json) = session_chat_prompt_after.as_deref() {
            if let Some(activity_object) = update.activity.as_object_mut() {
                activity_object.insert("sessionChatPrompt".to_string(), json!(prompt_json));
            }
        }
        if !is_stale_activity_event(&session, now_ms) {
            let next_activity_name = update
                .activity
                .get("activity")
                .and_then(Value::as_str)
                .unwrap_or("idle");
            session_chat_activity_changed = is_session_chat_working_activity(next_activity_name)
                != is_session_chat_working_activity(&update.previous_activity);
            let activity_changed = should_persist_activity_update(&session, &update);
            if activity_changed {
                let mut runtime_settings = object_field(&session, "runtimeSettings");
                runtime_settings.insert("agentActivity".to_string(), update.activity.clone());
                promote_draft_on_first_activity(
                    &session,
                    &mut runtime_settings,
                    update.last_active_at.as_deref(),
                );
                let mut session_update = lifecycle_update(lifecycle);
                session_update.insert(
                    "runtimeSettings".to_string(),
                    Value::Object(runtime_settings),
                );
                if let Some(last_active_at) = update.last_active_at.clone() {
                    session_update.insert("lastActiveAt".to_string(), json!(last_active_at));
                }
                session = repository.update_session(&session_update)?;
                changed = true;
            }
            activity_reason = if activity_changed {
                "activity-updated".to_string()
            } else {
                "activity-unchanged".to_string()
            };
            activity_update = Some(update);
        } else {
            activity_reason = "stale-activity-event".to_string();
            session_chat_activity_changed = false;
            /*
            CDXC:AgentScreenDetection 2026-09-03:
            Staleness orders ACTIVITY transitions: an event older than the
            last transition must not move the activity backwards. The card
            is not ordered by that clock — it is scoped to one tool call by
            next_session_chat_prompt_setting — so a question that arrives
            "stale" (its PreToolUse hook raced a subagent's hook or the
            question's own permission Notification, both of which bump
            lastChangedAt) is still the question the terminal is showing.
            Dropping it here left the chat with no card at all, because the
            hook is the only live source for a pending Claude question.
            Persist the prompt alone onto the stored activity object.
            */
            if session_chat_prompt_changed {
                let mut runtime_settings = object_field(&session, "runtimeSettings");
                let mut stored_activity = runtime_settings
                    .get("agentActivity")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if let Some(activity_object) = stored_activity.as_object_mut() {
                    match session_chat_prompt_after.as_deref() {
                        Some(prompt_json) => {
                            activity_object
                                .insert("sessionChatPrompt".to_string(), json!(prompt_json));
                        }
                        None => {
                            activity_object.remove("sessionChatPrompt");
                        }
                    }
                }
                runtime_settings.insert("agentActivity".to_string(), stored_activity);
                let mut session_update = lifecycle_update(lifecycle);
                session_update.insert(
                    "runtimeSettings".to_string(),
                    Value::Object(runtime_settings),
                );
                session = repository.update_session(&session_update)?;
                changed = true;
            }
        }
    }
    /*
    CDXC:Drafts 2026-08-28:
    The PRECISE half of draft promotion, and the reason the activity-based half
    above can stay conservative. `UserPromptSubmit` — or any hook carrying the
    prompt text itself — is positive evidence that the USER prompted this agent,
    which nothing a CLI does while starting up can produce. It runs outside the
    activity block on purpose: a prompt-submit hook often carries no status at
    all, so `hook_activity` is `None` and that block never executes.
    */
    if session_is_draft(&session) {
        let mut runtime_settings = object_field(&session, "runtimeSettings");
        if promote_draft_on_prompt_evidence(params, &mut runtime_settings) {
            let mut session_update = lifecycle_update(lifecycle);
            session_update.insert(
                "runtimeSettings".to_string(),
                Value::Object(runtime_settings),
            );
            session = repository.update_session(&session_update)?;
            changed = true;
        }
    }
    /*
    CDXC:AgentHooks 2026-06-22-08:31:
    Hook ingestion must mirror TypeScript's accepted-event reduction order: passive identity metadata, explicit activity, forced metadata-title reconciliation, then first-prompt auto-title claiming. Rejected passive conflicts stop before status, title, prompt, or presentation side effects can mutate the wrong session.
    */
    let reconciled = reconcile_agent_metadata_title(repository, lifecycle, home_dir, "pending")?;
    if let Some(reconciled_session) = reconciled.session {
        session = reconciled_session;
    }
    if reconciled.changed {
        changed = true;
    }
    let mut auto_title_claimed = false;
    if let Some(claimed_session) = claim_first_prompt_auto_title(
        repository,
        &session,
        read_text(params, "firstUserMessage"),
        is_explicit_user_prompt_submit_event(params),
    )? {
        session = claimed_session;
        changed = true;
        auto_title_claimed = true;
    }
    if reconciled.changed {
        reason = reconciled.reason;
    } else if auto_title_claimed {
        reason = "first-prompt-auto-title-claimed".to_string();
    } else if activity_reason != "metadata-only" {
        reason = activity_reason;
    }
    let mut result = Map::new();
    if let Some(update) = activity_update {
        result.insert("activity".to_string(), update.activity);
        result.insert(
            "enteredAttention".to_string(),
            Value::Bool(update.entered_attention),
        );
        result.insert(
            "previousActivity".to_string(),
            Value::String(update.previous_activity),
        );
    } else {
        result.insert("enteredAttention".to_string(), Value::Bool(false));
    }
    result.insert("changed".to_string(), Value::Bool(changed));
    result.insert(
        "projection".to_string(),
        project_session_title_projection(&session),
    );
    result.insert("reason".to_string(), Value::String(reason));
    result.insert(
        "sessionChatPromptChanged".to_string(),
        Value::Bool(session_chat_prompt_changed),
    );
    /*
    CDXC:SessionChat 2026-08-01:
    Session Chat's working indicator has no other source: the transcript can
    only ever SETTLE a spinner (a completed assistant row), never start one,
    because the first transcript row of a turn lands seconds after the agent
    starts. Reporting the working↔idle transition here lets the server push a
    sessionChatState frame on the chat channel, so every host gets the spinner
    and the Stop button without wiring its own activity prop. Only real
    transitions are reported — steady-state working events must not spam a
    frame every hook tick.
    */
    result.insert(
        "sessionChatActivityChanged".to_string(),
        Value::Bool(session_chat_activity_changed),
    );
    result.insert("session".to_string(), session);
    Ok(Value::Object(result))
}

/// A working↔not-working flip is the only activity change the chat channel
/// cares about (attention/idle both read as "not working").
pub(crate) fn is_session_chat_working_activity(activity: &str) -> bool {
    activity == "working"
}

/// The stored Session Chat interactive prompt: a JSON string in the shared
/// `SessionChatInteractivePrompt` wire shape, kept under
/// `runtimeSettings.agentActivity.sessionChatPrompt`.
pub(crate) fn session_chat_prompt_setting(session: &Value) -> Option<String> {
    object_field(session, "runtimeSettings")
        .get("agentActivity")
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("sessionChatPrompt"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/*
CDXC:SessionChat 2026-08-01:
Re-attach the stored Session Chat prompt to a freshly computed agentActivity
object. compute_activity_update rebuilds the object from the fixed
ActivityState struct, which does not know the key, so every non-hook activity
writer (terminal-title observation, explicit activity RPCs) must carry the
stored card forward — otherwise the very next title tick erases a
still-pending question card seconds after the PreToolUse hook stored it
(observed live 2026-08-01: an AskUserQuestion card never survived to a read).
Hook ingest must NOT use this: it re-derives the prompt per event
(replace / keep / clear via next_session_chat_prompt_setting). Lifecycle
resets (wake) also deliberately skip it — a woken session's card is stale.
*/
pub(crate) fn carry_session_chat_prompt(session: &Value, activity: &mut Value) {
    let Some(stored) = session_chat_prompt_setting(session) else {
        return;
    };
    if let Some(object) = activity.as_object_mut() {
        object
            .entry("sessionChatPrompt".to_string())
            .or_insert_with(|| json!(stored));
    }
}

/// Prompt disposition for one hook event: derive (AskUserQuestion-ish tool
/// input on a non-post-tool event, or PermissionRequest) → replace; the asking
/// call's own post-tool event / Stop / SessionEnd / idle transition → clear;
/// anything else — including post-tool events of OTHER tool calls, which a
/// background subagent keeps producing under the lead session's id — → keep.
/// See CDXC:AgentScreenDetection in session_chat_interactive.rs.
pub(crate) fn next_session_chat_prompt_setting(
    previous: Option<&str>,
    params: &Map<String, Value>,
    next_activity: &str,
) -> Option<String> {
    let event_name = params
        .get("eventName")
        .or_else(|| params.get("rawEventName"))
        .and_then(Value::as_str);
    let tool_name = params
        .get("toolName")
        .or_else(|| params.get("tool_name"))
        .and_then(Value::as_str);
    let tool_use_id = params
        .get("toolUseId")
        .or_else(|| params.get("tool_use_id"))
        .and_then(Value::as_str);
    let tool_input = params.get("toolInput").or_else(|| params.get("tool_input"));
    if let Some(prompt) =
        crate::session_chat::derive_session_chat_prompt(tool_name, tool_input, event_name)
    {
        return serde_json::to_string(&prompt.with_tool_use_id(tool_use_id.map(str::to_string)))
            .ok()
            .or_else(|| previous.map(str::to_string));
    }
    let stored = previous.and_then(crate::session_chat::parse_stored_session_chat_prompt);
    let clear = match stored.as_ref() {
        Some(stored) => crate::session_chat::session_chat_prompt_clear_decision(
            Some(stored),
            crate::session_chat::SessionChatPromptClearEvent {
                event_name,
                next_activity: Some(next_activity),
                tool_name,
                tool_use_id,
                idle_input_notification: params.get("notificationKind").and_then(Value::as_str)
                    == Some("idleInput"),
            },
        ),
        // Unparsable stored text has no tool identity to scope on: the
        // tool-blind rule decides, so it cannot linger forever.
        None => {
            crate::session_chat::should_clear_session_chat_prompt(event_name, Some(next_activity))
        }
    };
    if clear {
        return None;
    }
    previous.map(str::to_string)
}

pub(crate) fn is_explicit_user_prompt_submit_event(params: &Map<String, Value>) -> bool {
    params
        .get("eventName")
        .or_else(|| params.get("rawEventName"))
        .and_then(Value::as_str)
        .is_some_and(|event_name| event_name.trim().eq_ignore_ascii_case("UserPromptSubmit"))
}

pub(crate) fn claim_first_prompt_auto_title(
    repository: &DomainRepository<'_>,
    session: &Value,
    prompt: Option<String>,
    is_explicit_user_prompt_submit: bool,
) -> Result<Option<Value>, DomainStateError> {
    let decision = decide_first_prompt_auto_title_claim(
        session,
        prompt.as_deref(),
        false,
        is_explicit_user_prompt_submit,
    );
    if !decision.should_run {
        return Ok(None);
    };
    let Some(prompt) = prompt else {
        return Ok(None);
    };
    let mut runtime_settings = object_field(session, "runtimeSettings");
    /*
    CDXC:SessionFork 2026-07-11:
    A fork's initial `Fork: …` CLI rename is provisional, not the first-prompt
    generated name. Remove the defensive auto-title bit when the fork's first
    real prompt is claimed, while keeping the fork marker through the async
    job so its non-generic provisional title remains eligible.
    */
    runtime_settings.remove("autoTitleFromFirstPrompt");
    runtime_settings.remove("gxserverFirstPromptAutoTitleCancelledAt");
    runtime_settings.remove("gxserverFirstPromptAutoTitleCancelledPrompt");
    runtime_settings.remove("gxserverFirstPromptAutoTitleReason");
    runtime_settings.insert("firstUserMessage".to_string(), json!(prompt));
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleStatus".to_string(),
        json!("running"),
    );
    runtime_settings.insert(
        FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY.to_string(),
        json!(Uuid::new_v4().to_string()),
    );
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleStartedAt".to_string(),
        json!(now_iso()),
    );
    let lifecycle = LifecycleParams {
        project_id: read_text_value(session, "projectId")
            .ok_or_else(|| DomainStateError::corrupt_state("Session missing projectId."))?,
        session_id: read_text_value(session, "sessionId")
            .ok_or_else(|| DomainStateError::corrupt_state("Session missing sessionId."))?,
    };
    let mut update = lifecycle_update(&lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&update).map(Some)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FirstPromptAutoTitleClaimDecision {
    pub(crate) normalized_prompt: Option<String>,
    pub(crate) reason: String,
    pub(crate) should_run: bool,
    pub(crate) strategy: Option<&'static str>,
}

/*
CDXC:SessionTitles 2026-06-22-08:12:
Rust must match TypeScript gxserver's first-prompt claim boundary, not only the later background job. Claim only supported providers with generic titles and real user prompts, and skip meta and slash-command prompts without setting `running`.

CDXC:SessionTitles 2026-08-03:
Escape cancels one title-generation attempt, not every later submission of the
same prompt. A fresh explicit UserPromptSubmit may therefore re-arm identical
cancelled text, while passive sidecar, lifecycle, and later hook replays remain
blocked so cancellation cannot restart itself.
*/
pub(crate) fn decide_first_prompt_auto_title_claim(
    session: &Value,
    prompt: Option<&str>,
    allow_running: bool,
    is_explicit_user_prompt_submit: bool,
) -> FirstPromptAutoTitleClaimDecision {
    let runtime_settings = object_field(session, "runtimeSettings");
    let fork_first_prompt_rearmed = runtime_settings
        .get("forkFirstPromptAutoTitlePending")
        .and_then(Value::as_bool)
        == Some(true);
    let status = read_text_from_map(&runtime_settings, "gxserverFirstPromptAutoTitleStatus");
    let normalized_prompt = normalize_first_prompt_title_claim_prompt(prompt);
    let cancelled_prompt = normalize_first_prompt_title_claim_prompt(
        read_text_from_map(
            &runtime_settings,
            "gxserverFirstPromptAutoTitleCancelledPrompt",
        )
        .as_deref(),
    )
    .or_else(|| {
        normalize_first_prompt_title_claim_prompt(
            read_text_from_map(&runtime_settings, "firstUserMessage").as_deref(),
        )
    });
    let is_cancelled_retry_prompt = status.as_deref() == Some("cancelled")
        && normalized_prompt.is_some()
        && (normalized_prompt != cancelled_prompt || is_explicit_user_prompt_submit);
    if (status.as_deref() == Some("running") && !allow_running)
        || matches!(status.as_deref(), Some("applied" | "failed" | "skipped"))
        || (status.as_deref() == Some("cancelled") && !is_cancelled_retry_prompt)
    {
        return first_prompt_claim_decision(
            normalized_prompt,
            &format!("already-{}", status.unwrap_or_default()),
            false,
            None,
        );
    }
    if !fork_first_prompt_rearmed
        && runtime_settings
            .get("autoTitleFromFirstPrompt")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return first_prompt_claim_decision(normalized_prompt, "alreadyAutoNamed", false, None);
    }
    let agent_name = first_prompt_claim_agent_name(session, &runtime_settings);
    let strategy = first_prompt_claim_strategy(agent_name.as_deref());
    if strategy.is_none() {
        return first_prompt_claim_decision(normalized_prompt, "unsupportedAgent", false, None);
    }
    let Some(normalized) = normalized_prompt.clone() else {
        return first_prompt_claim_decision(normalized_prompt, "emptyPrompt", false, strategy);
    };
    if is_first_prompt_claim_meta_prompt(&normalized) {
        return first_prompt_claim_decision(Some(normalized), "metaPrompt", false, strategy);
    }
    if is_first_prompt_claim_slash_command(prompt, &normalized) {
        return first_prompt_claim_decision(Some(normalized), "slashCommand", false, strategy);
    }
    if strategy == Some("agentAutoTitle") {
        // These agents own first-turn naming; metadata reconciliation adopts it.
        return first_prompt_claim_decision(Some(normalized), "agentAutoTitle", false, strategy);
    }
    // CDXC:SessionTitles 2026-09-03: Codex's provisional first-words
    // name may already have been adopted as the title by the time the claim
    // runs; it is not a real title and must not block the job.
    let current_title = read_text_value(session, "title");
    let is_codex_provisional_title = strategy == Some("awaitAgentAutoTitle")
        && current_title
            .as_deref()
            .is_some_and(|title| is_codex_provisional_thread_name(prompt, title));
    // CDXC:SessionTitles 2026-09-03: a placeholder title is the
    // launcher's default, whatever its spelling, and never blocks the claim.
    let is_placeholder_title =
        read_text_from_map(&runtime_settings, "titleSource").as_deref() == Some("placeholder");
    if !fork_first_prompt_rearmed
        && !is_codex_provisional_title
        && !is_placeholder_title
        && !is_first_prompt_claim_generic_title(agent_name.as_deref(), current_title.as_deref())
    {
        return first_prompt_claim_decision(
            Some(normalized),
            "nonGenericCurrentTitle",
            false,
            strategy,
        );
    }
    first_prompt_claim_decision(Some(normalized), "eligible", true, strategy)
}

pub(crate) fn first_prompt_claim_decision(
    normalized_prompt: Option<String>,
    reason: &str,
    should_run: bool,
    strategy: Option<&'static str>,
) -> FirstPromptAutoTitleClaimDecision {
    FirstPromptAutoTitleClaimDecision {
        normalized_prompt,
        reason: reason.to_string(),
        should_run,
        strategy,
    }
}

pub(crate) fn first_prompt_claim_agent_name(
    session: &Value,
    runtime_settings: &Map<String, Value>,
) -> Option<String> {
    read_text_value(session, "agentId")
        .or_else(|| read_text_from_map(runtime_settings, "agentName"))
}

pub(crate) fn first_prompt_claim_strategy(agent_name: Option<&str>) -> Option<&'static str> {
    match normalize_first_prompt_claim_agent_name(agent_name).as_deref() {
        Some("claude") => Some("sendBareRenameCommand"),
        // See first_prompt_auto_title_strategy: Codex names the thread itself
        // and Ghostex only steps in when its generated title never lands.
        Some("codex") => Some("awaitAgentAutoTitle"),
        // See first_prompt_auto_title_strategy: this agent names its own
        // sessions and the metadata sync adopts those names.
        Some("hermes-agent") => Some("agentAutoTitle"),
        // See first_prompt_auto_title_strategy: this agent names its own
        // conversations and the metadata sync adopts those names.
        Some("antigravity") => Some("agentAutoTitle"),
        Some("pi") => Some("generateTitleAndName"),
        Some("omp") => Some("generateTitleAndName"),
        _ => None,
    }
}

pub(crate) fn normalize_first_prompt_claim_agent_name(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" => None,
        "openai codex" | "codex cli" => Some("codex".to_string()),
        "claude code" => Some("claude".to_string()),
        "hermes" | "hermes agent" | "hermes-agent" => Some("hermes-agent".to_string()),
        "π" => Some("pi".to_string()),
        other => Some(other.to_string()),
    }
}

pub(crate) fn is_first_prompt_claim_generic_title(
    agent_name: Option<&str>,
    title: Option<&str>,
) -> bool {
    let normalized_title = title
        .map(|value| {
            value
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    if normalized_title.is_empty() {
        return true;
    }
    let normalized_agent = normalize_first_prompt_claim_agent_name(agent_name);
    let generic = [
        "terminal",
        "terminal session",
        "agent",
        "agent session",
        "antigravity cli",
        "antigravity cli session",
        "claude",
        "claude code",
        "claude session",
        "codex",
        "codex cli",
        "codex session",
        "openai codex",
        "openai codex session",
        "pi",
        "\u{03c0}",
        "pi session",
    ];
    if generic.contains(&normalized_title.as_str()) {
        return true;
    }
    let Some(agent) = normalized_agent else {
        return false;
    };
    normalized_title == agent
        || normalized_title == format!("{agent} session")
        || normalized_title == format!("{agent} agent session")
}

pub(crate) fn normalize_first_prompt_title_claim_prompt(prompt: Option<&str>) -> Option<String> {
    let normalized = prompt?.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }
    let stripped = strip_first_prompt_title_claim_prefixes(normalized);
    let cleaned = stripped
        .trim()
        .trim_end_matches(['.', '?', '!', ':', ';', ','])
        .trim();
    Some(
        if cleaned.is_empty() {
            normalized
        } else {
            cleaned
        }
        .to_string(),
    )
}

pub(crate) fn strip_first_prompt_title_claim_prefixes(value: &str) -> &str {
    let mut stripped = value;
    loop {
        let lower = stripped.to_lowercase();
        let prefix = [
            "please ",
            "kindly ",
            "hey ",
            "hi ",
            "hello ",
            "can you ",
            "could you ",
            "would you ",
            "will you ",
            "can we ",
            "could we ",
            "would we ",
            "help me ",
            "i need you to ",
            "i need to ",
            "i need ",
            "how do i ",
            "how does ",
            "is there any way to ",
            "is there way to ",
        ]
        .into_iter()
        .find(|prefix| lower.starts_with(prefix));
        let Some(prefix) = prefix else {
            return stripped;
        };
        stripped = &stripped[prefix.len()..];
    }
}

pub(crate) fn is_first_prompt_claim_meta_prompt(prompt: &str) -> bool {
    prompt.starts_with("# AGENTS")
        || prompt.contains("tool_use_id")
        || [
            "<command",
            "<environment_context",
            "<permissions instructions>",
            "<user_instructions>",
            "<INSTRUCTIONS>",
            "<collaboration_mode>",
            "<app-context>",
            "<turn_aborted>",
            "<ide_opened_file>",
            "<local-",
            "[Tool Result]",
            "Caveat:",
        ]
        .iter()
        .any(|prefix| prompt.starts_with(prefix))
}

pub(crate) fn is_first_prompt_claim_slash_command(
    raw_prompt: Option<&str>,
    normalized_prompt: &str,
) -> bool {
    if normalized_prompt.encode_utf16().count() > 50 {
        return false;
    }
    let Some(raw_prompt) = raw_prompt else {
        return false;
    };
    raw_prompt
        .split('\n')
        .any(is_first_prompt_claim_slash_command_line)
}

pub(crate) fn is_first_prompt_claim_slash_command_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let Some(rest) = trimmed.strip_prefix('/') else {
        return false;
    };
    let mut chars = rest.char_indices();
    let Some((_, first)) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let mut consumed_bytes = first.len_utf8();
    for (index, ch) in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            consumed_bytes = index + ch.len_utf8();
            continue;
        }
        consumed_bytes = index;
        break;
    }
    let suffix = &rest[consumed_bytes..];
    suffix
        .chars()
        .next()
        .map(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    ')' | '.' | ',' | ':' | ';' | '!' | '?' | '\'' | '"' | '`'
                )
        })
        .unwrap_or(true)
}

pub(crate) fn should_persist_activity_update(session: &Value, update: &ActivityUpdate) -> bool {
    read_text_value(session, "lastActiveAt") != update.last_active_at
        || persistable_agent_activity_snapshot(
            object_field(session, "runtimeSettings").get("agentActivity"),
        ) != persistable_agent_activity_snapshot(Some(&update.activity))
}

pub(crate) fn persistable_agent_activity_snapshot(value: Option<&Value>) -> Value {
    let Some(activity) = value.and_then(Value::as_object) else {
        return json!({});
    };
    let mut snapshot = Map::new();
    for key in [
        "activity",
        "agentName",
        "attentionEventId",
        "attentionSuppressedUntil",
        "hasSeenWorking",
        "isAcknowledged",
        "lastMeaningfulActivityAt",
        "lastTitle",
        "lastTitleChangeAt",
        // Session Chat interactive prompt (question/approval card JSON).
        // REQUIRED here: a key missing from this whitelist is invisible to
        // change detection, so should_persist_activity_update would return
        // false and the prompt would never reach disk.
        "sessionChatPrompt",
        "suppressedUntil",
        "workingSource",
        "workingStartedAt",
    ] {
        if let Some(value) = activity.get(key) {
            snapshot.insert(key.to_string(), value.clone());
        }
    }
    if activity.get("activity").and_then(Value::as_str) != Some("idle") {
        if let Some(value) = activity.get("lastChangedAt") {
            snapshot.insert("lastChangedAt".to_string(), value.clone());
        }
    }
    Value::Object(snapshot)
}

pub(crate) fn normalize_agent_hook_activity(
    status: Option<&Value>,
    event_name: Option<&Value>,
    agent_name: Option<&Value>,
) -> Option<String> {
    let normalized_agent = normalize_agent_id(agent_name.and_then(Value::as_str));
    let event = event_name
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let lower = event.to_ascii_lowercase();
    /*
    CDXC:AgentHooks 2026-08-27:
    Separator-stripped event name, so this table reads the same event under
    every provider's spelling (`post_tool_use`, `post-tool-use`,
    `message.part`) exactly like the notify hook's mapping does.
    */
    let compact = lower.replace(['_', '-', '.'], "");
    /*
    CDXC:AgentHooks 2026-08-27:
    Subagent and teammate lifecycle events describe a CHILD of the session, and
    PreCompact fires before the compaction is validated (an aborted compact
    emits it alone). Neither carries an activity signal for the lead pane, so
    they must not fall through to the sidecar status either — a stale posted
    status would move the session on a roster-only event.
    */
    if matches!(
        compact.as_str(),
        "subagentstart" | "subagentstop" | "teammateidle" | "precompact"
    ) {
        return None;
    }
    /*
    CDXC:AgentHooks 2026-06-22-08:31:
    Server-side hook ingestion must use provider event semantics before trusting
    sidecar status. Codex Stop is an authoritative completed-turn boundary, so
    it enters attention; SessionEnd still clears the session to idle. Keeping
    the event mapping here aligned with the hook helper prevents a later
    sidecar sync from erasing the attention transition.
    */
    if normalized_agent.as_deref() == Some("codex") {
        if lower == "stop" {
            return Some("attention".to_string());
        }
        if matches!(lower.as_str(), "sessionend" | "session-end") {
            return Some("idle".to_string());
        }
    }
    // OpenClaude ships Claude's hook contract verbatim, so it shares every
    // Claude rule here exactly as it does in the notify hook's mapping.
    if matches!(normalized_agent.as_deref(), Some("claude" | "openclaude")) {
        if matches!(lower.as_str(), "stop" | "idle") {
            return Some("idle".to_string());
        }
        /*
        CDXC:SessionChat 2026-08-24:
        SessionStart is the only hook Claude Code fires when /compact or /clear
        finishes; the UserPromptSubmit that submitted the command set "working"
        and no Stop follows, which left the session — and the prompt-queue
        scheduler gating on it — stuck working after every manual compaction.
        Every SessionStart source means the CLI is at its input prompt, so it
        settles to idle (mirrors the notify hook's mapping).
        */
        if matches!(lower.as_str(), "sessionstart" | "session-start") {
            return Some("idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "notification" | "notify" | "permissionrequest"
        ) {
            return Some("attention".to_string());
        }
        if matches!(
            lower.as_str(),
            "userpromptsubmit" | "prompt-submit" | "pretooluse" | "pre-tool-use"
        ) {
            return Some("working".to_string());
        }
        if matches!(lower.as_str(), "sessionend" | "session-end") {
            return Some("idle".to_string());
        }
    }
    if matches!(
        normalized_agent.as_deref(),
        Some("copilot" | "codebuddy" | "droid" | "qoder")
    ) {
        if matches!(
            lower.as_str(),
            "stop" | "notification" | "sessionend" | "session-end"
        ) {
            return Some("idle".to_string());
        }
        if matches!(lower.as_str(), "pretooluse" | "pre-tool-use") {
            return Some("working".to_string());
        }
    }
    if normalized_agent.as_deref() == Some("antigravity") {
        if matches!(
            lower.as_str(),
            "stop" | "turn-completion" | "sessionend" | "session-end"
        ) {
            return Some("idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "preinvocation" | "pretooluse" | "posttooluse"
        ) {
            return Some("working".to_string());
        }
    }
    if matches!(
        lower.as_str(),
        "stop"
            | "agent-response"
            | "afteragent"
            | "afteragentresponse"
            | "agent.end"
            | "agent_end"
            | "on_complete"
            | "on_error"
            | "post_llm_call"
            | "turn-completion"
    ) {
        return Some("idle".to_string());
    }
    if matches!(
        lower.as_str(),
        "on_tool_permission"
            | "post_approval_response"
            | "pretooluse"
            | "posttooluse"
            | "pre_tool_call"
            | "beforeagent"
            | "preinvocation"
            | "userpromptsubmit"
            | "agent.start"
            | "agent_start"
            | "beforeshellexecution"
            | "beforesubmitprompt"
    ) {
        return Some("working".to_string());
    }
    /*
    CDXC:AgentHooks 2026-08-27:
    The event names the hook installer registers today, mirrored from the notify
    hook's table so both layers move a session the same way. They sit ahead of
    the posted-status fallback because gxserver's own event semantics outrank a
    sidecar status.

    Two of that table's rules are deliberately NOT mirrored here: Copilot's
    ErrorOccurred (idle unless the payload says `recoverable`) and Claude's
    PostCompact (idle only when `trigger` is manual) both need the hook payload,
    which this function never receives. Leaving them unmapped lets the posted
    status — already derived from the full payload by the notify hook — decide.
    */
    if matches!(
        compact.as_str(),
        "aftertool"
            | "beforemcpexecution"
            | "beforetool"
            | "messagepart"
            | "postcompaction"
            | "postinvocation"
            | "posttoolusefailure"
            | "sessionbusy"
    ) {
        return Some("working".to_string());
    }
    if compact == "askuserquestion" {
        return Some("attention".to_string());
    }
    if matches!(compact.as_str(), "sessionidle" | "stopfailure") {
        return Some("idle".to_string());
    }
    if let Some(status) = status
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "idle" | "working" | "attention"))
    {
        return Some(status.to_string());
    }
    if event.is_empty() {
        return None;
    }
    if agent_hook_event_matches(
        &event,
        &lower,
        &[
            "BeforeAgent",
            "PreInvocation",
            "PreToolUse",
            "UserPromptSubmit",
            "agent.start",
            "agent_start",
            "agentSpawn",
            "beforeShellExecution",
            "beforeSubmitPrompt",
            "on_session_reset",
            "on_session_start",
            "on_tool_permission",
            "post_approval_response",
            "postToolUse",
            "pre_llm_call",
            "pre_tool_call",
            "preToolUse",
            "userPromptSubmit",
        ],
    ) {
        return Some("working".to_string());
    }
    if agent_hook_event_matches(
        &event,
        &lower,
        &[
            "Notification",
            "PermissionRequest",
            "message.updated",
            "permission.updated",
            "pre_approval_request",
            "session.updated",
        ],
    ) {
        return Some("attention".to_string());
    }
    if agent_hook_event_matches(
        &event,
        &lower,
        &[
            "AfterAgent",
            "SessionEnd",
            "Stop",
            "afterAgentResponse",
            "agent.end",
            "agent_end",
            "on_complete",
            "on_error",
            "on_session_end",
            "on_session_finalize",
            "release",
            "session.end",
            "session_shutdown",
            "turn-completion",
        ],
    ) {
        return Some("idle".to_string());
    }
    None
}

pub(crate) fn agent_hook_event_matches(
    event: &str,
    lower_event: &str,
    candidates: &[&str],
) -> bool {
    candidates
        .iter()
        .any(|candidate| event == *candidate || lower_event == *candidate)
}

pub(crate) fn default_activity(agent_id: Option<&str>, override_activity: Option<&str>) -> Value {
    let timestamp = now_iso();
    let initial_activity = override_activity.unwrap_or("idle");
    let mut activity = Map::new();
    activity.insert(
        "activity".to_string(),
        Value::String(initial_activity.to_string()),
    );
    if let Some(agent_id) = agent_id.and_then(|value| normalize_status_agent_name(Some(value))) {
        activity.insert("agentName".to_string(), Value::String(agent_id));
    }
    activity.insert("hasSeenWorking".to_string(), Value::Bool(false));
    activity.insert("isAcknowledged".to_string(), Value::Bool(true));
    activity.insert(
        "lastChangedAt".to_string(),
        Value::String(timestamp.clone()),
    );
    /*
    Creation IS a launch, so an idle-born session arms the same initial
    passive-signal suppression window the launch/resume/wake transitions do
    (ActivitySuppressionPolicy layer 1). Without it, Codex's startup spinner in
    the terminal title flipped a brand-new session to "working" before anything
    had happened, and the chat surface — empty transcript, known agent session
    — read that as "turn in flight, transcript not flushed" and replaced its
    welcome with the blank "Loading conversation…" hold until the spinner
    settled. A creation that starts "working" (launch startup text) keeps the
    expired stamp: its own titles must stay able to settle it back to idle.
    */
    activity.insert(
        "suppressedUntil".to_string(),
        Value::String(if initial_activity == "idle" {
            crate::session_status::iso_from_ms(
                now_ms() + crate::session_status::INITIAL_ACTIVITY_SUPPRESSION_MS,
            )
        } else {
            timestamp
        }),
    );
    Value::Object(activity)
}
