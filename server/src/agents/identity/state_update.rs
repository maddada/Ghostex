use serde_json::{json, Map, Value};

use super::*;
use crate::domain::{DomainRepository, DomainStateError};
use crate::presentation::project_session_title_projection;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionIdentityUpdateSource {
    Lifecycle,
    LiveProcess,
    Passive,
    VerifiedRewind,
    TerminalTitle,
}

/*
CDXC:SessionIdentity 2026-09-01:
`apply_session_state_update` used to hydrate the project's WHOLE session list up
front, for every identity observation. On a registry with a few thousand rows in
one project that is ~70ms of JSON hydration, and the live zmx process scan calls
this once per running agent session on every listSessions /
readPresentationSnapshot / WS-subscribe request — the list
alone was the bulk of a ~900ms response.

Only two places actually read the list, and neither runs on the steady-state
path: the passive-Codex conflict resolver (`resolve_allowed_session_identity`,
which needs it only when a passive observation brings a NEW Codex conversation
id) and the trusted-title search (`select_trusted_title_for_identity`, which
runs only while the row still has no trusted title of its own). This handle
hydrates on first use and caches, so a call that reaches neither pays nothing
while a call that reaches either still sees exactly the same full project list —
every lifecycle included, because stopped rows can still own an identity.
*/
pub(crate) struct LazyProjectSessions<'a, 'db> {
    project_id: &'a str,
    repository: &'a DomainRepository<'db>,
    sessions: Option<Vec<Value>>,
}

impl<'a, 'db> LazyProjectSessions<'a, 'db> {
    pub(crate) fn new(repository: &'a DomainRepository<'db>, project_id: &'a str) -> Self {
        Self {
            project_id,
            repository,
            sessions: None,
        }
    }

    /// The project's non-stopped rows. The only reader is the active Codex
    /// owner lookup, and `is_active_identity_owner` never accepts a stopped
    /// row, so leaving stopped history out of the read changes nothing.
    pub(crate) fn get(&mut self) -> Result<&[Value], DomainStateError> {
        if self.sessions.is_none() {
            self.sessions = Some(
                self.repository
                    .list_sessions_excluding_stopped(Some(self.project_id))?,
            );
        }
        Ok(self.sessions.as_deref().unwrap_or_default())
    }

    /// The project's rows that can match `identity` (same agent session id or
    /// path), read narrowly from SQLite. See
    /// `DomainRepository::list_sessions_matching_identity`.
    pub(crate) fn matching_identity(
        &self,
        identity: &ResolvedIdentity,
    ) -> Result<Vec<Value>, DomainStateError> {
        self.repository.list_sessions_matching_identity(
            self.project_id,
            identity.agent_session_id.as_deref(),
            identity.agent_session_path.as_deref(),
        )
    }
}

pub(crate) struct SessionIdentityConflict {
    pub(super) agent_id: String,
    pub(super) current_agent_session_id: Option<String>,
    pub(super) incoming_agent_session_id: String,
    pub(super) owner_project_id: Option<String>,
    pub(super) owner_session_id: Option<String>,
    pub(super) reason: &'static str,
    pub(super) source: SessionIdentityUpdateSource,
}

pub(crate) fn apply_session_state_update(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
    identity_update_source: SessionIdentityUpdateSource,
) -> Result<(Map<String, Value>, Value), DomainStateError> {
    let session = require_session(repository, lifecycle)?;
    if draft_agent_switch_in_progress(&lifecycle.project_id, &lifecycle.session_id) {
        return Ok((
            object_from_value(json!({
                "changed": false,
                "projection": project_session_title_projection(&session),
                "reason": "draft-agent-switch-in-progress",
                "session": session.clone(),
            })),
            session,
        ));
    }
    let project = require_project(repository, &lifecycle.project_id)?;
    let mut project_sessions = LazyProjectSessions::new(repository, &lifecycle.project_id);
    let observed_identity = align_observed_identity_with_launch_profile(
        &session,
        resolve_session_identity(&IdentityInput {
            agent_id: None,
            agent_name: read_text(params, "agentName"),
            agent_session_id: read_text(params, "agentSessionId"),
            agent_session_path: read_text(params, "agentSessionPath"),
            runtime_settings: Map::new(),
            startup_text: read_text(params, "startupText"),
        }),
    );
    if identity_update_source != SessionIdentityUpdateSource::LiveProcess
        && launch_agent_mismatch(&session, observed_identity.agent_id.as_deref())
    {
        let result = json!({
            "changed": false,
            "projection": project_session_title_projection(&session),
            "reason": "launch-agent-mismatch",
            "session": session.clone(),
        });
        return Ok((object_from_value(result), session));
    }
    let current_identity = resolve_stored_session_identity(&session);
    let resolved_identity = merge_observed_session_identity(&observed_identity, &current_identity);
    let (identity, identity_conflict) = resolve_allowed_session_identity(
        &current_identity,
        &session,
        &observed_identity,
        &resolved_identity,
        &mut project_sessions,
        identity_update_source,
    )?;
    if identity_conflict.is_some() && identity_update_source == SessionIdentityUpdateSource::Passive
    {
        let mut result = object_from_value(json!({
            "changed": false,
            "projection": project_session_title_projection(&session),
            "reason": "passive-session-identity-conflict",
            "session": session.clone(),
        }));
        if let Some(conflict) = identity_conflict {
            result.insert(
                "identityConflict".to_string(),
                session_identity_conflict_value(&conflict),
            );
        }
        return Ok((result, session));
    }

    let next_agent = identity
        .agent_id
        .clone()
        .or_else(|| read_text_value(&session, "agentId"));
    let stored_runtime_settings = object_field(&session, "runtimeSettings");
    let mut runtime_settings = apply_session_identity_runtime_settings(
        &current_identity,
        &identity,
        stored_runtime_settings.clone(),
        identity_update_source,
        session_launch_agent_provider_id(&session),
    );
    if identity_update_source == SessionIdentityUpdateSource::VerifiedRewind
        && read_text(params, "agentSessionPath").is_none()
    {
        runtime_settings.remove("agentSessionPath");
    }
    if identity_update_source == SessionIdentityUpdateSource::VerifiedRewind {
        insert_optional_from_params(&mut runtime_settings, params, "codexRewindProcessId");
    }
    // CDXC:AgentProviders 2026-09-16 WHY:
    // Live identity adoption can replace the CLI without a Switch Agent flow, leaving the previous binary paired with the new family's resume grammar and conversation id.
    // Clear its launch and account metadata together so account validation and automatic recovery cannot reuse the previous provider's login.
    // SEE-ALSO: server/src/agents/resume_plan.rs, server/src/agents/switch_account.rs.
    // CDXC:AgentProviders 2026-09-19 WHY:
    // Compare CLI families, not agent ids. A custom agent built on Claude that the live-process scan re-detects as `claude` (restored sessions have no launch icon to align with) is the same CLI and login; treating it as a change removed the session's account and its Customize continuation settings.
    let family_changed = {
        let launch_settings = object_field(&session, "launchSettings");
        let family = |agent_id: Option<&str>| {
            let agent_id = normalize_agent_id(agent_id)?;
            let config = resolve_project_agent_config(&project, &agent_id, Some(&launch_settings));
            resume_agent_family_id(Some(agent_id), &config, &launch_settings)
                .and_then(|family| normalize_agent_id(Some(&family)))
        };
        let previous = family(current_identity.agent_id.as_deref());
        let next = family(identity.agent_id.as_deref());
        previous.is_some() && next.is_some() && previous != next
    };
    let mut stale_launch_metadata_cleared = 0;
    let mut launch_settings = object_field(&session, "launchSettings");
    if family_changed {
        crate::session_approval::clear_approval_wait(&mut runtime_settings);
        for key in [
            "agentCommand",
            "accountId",
            "accountName",
            "accountColor",
            "accountSlot",
            "accountProvider",
            "accountBaseCommand",
            "accountCommand",
            "accountSwitch",
            "accountRecovery",
            "accountRecoverySuppressed",
            "accountPolicyOverride",
            "accountPolicyDefault",
        ] {
            runtime_settings.remove(key);
        }
        for key in [
            "acceptAllMode",
            "agentCommand",
            "agentLaunchPlan",
            "agentResumePlan",
            "icon",
        ] {
            if launch_settings.remove(key).is_some() {
                stale_launch_metadata_cleared += 1;
            }
        }
    }
    if let Some(dropped_activity) = stored_runtime_settings
        .get("agentActivity")
        .filter(|_| runtime_settings.get("agentActivity").is_none())
    {
        log_identity_dropped_agent_activity(
            lifecycle,
            identity_update_source,
            &current_identity,
            &identity,
            dropped_activity,
        );
    }
    insert_truthy_from_params(
        &mut runtime_settings,
        params,
        "firstPromptTitleGenerationAgent",
    );
    insert_optional_from_params(
        &mut runtime_settings,
        params,
        "firstPromptTitleGenerationCommand",
    );
    insert_truthy_from_params(&mut runtime_settings, params, "firstUserMessage");

    let should_promote_agent = next_agent.is_some()
        || identity.agent_session_id.is_some()
        || identity.agent_session_path.is_some();
    let mut title =
        read_text_value(&session, "title").unwrap_or_else(|| "Terminal Session".to_string());
    let mut reason = "identity-updated".to_string();
    let mut current_with_identity = session.clone();
    if let Some(object) = current_with_identity.as_object_mut() {
        if let Some(agent_id) = next_agent.clone() {
            object.insert("agentId".to_string(), json!(agent_id));
        }
        if should_promote_agent {
            object.insert("kind".to_string(), json!("agent"));
        }
        object.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings.clone()),
        );
    }

    if provisional_fork_title(&current_with_identity).is_some() {
        reason = "fork-provisional-title-preserved".to_string();
    } else if trusted_resume_title(&current_with_identity).is_none() {
        if let Some(candidate) = select_trusted_title_for_identity(
            &project,
            &mut project_sessions,
            &current_with_identity,
            params.get("title"),
            params.get("titleSource"),
            read_text(params, "agentSessionId").as_deref(),
            &identity,
        )? {
            title = candidate.title;
            /*
            CDXC:SessionTitles 2026-09-17 DECISION:
            User: save donated titles with a non-user source. A candidate that only
            matched by store path (never by conversation id) must not land as
            "user", because a user-sourced title outranks terminal-auto and the
            agent's own later names could never replace it. Weak matches are
            recorded as terminal-auto, so the conversation's real title replaces
            them; id-confirmed candidates keep the donor's source.
            */
            let title_source = if candidate.title_source == "user" && !candidate.same_conversation {
                "terminal-auto".to_string()
            } else {
                candidate.title_source
            };
            runtime_settings.insert("titleSource".to_string(), json!(title_source));
            reason = candidate.reason;
        } else if next_agent.is_some() {
            /*
            Plain terminals promoted by a live WSL agent process or its first
            hook should immediately gain the same neutral agent-aware title as
            sessions created from the agent launcher. Keep it a placeholder so
            first-prompt auto-title generation remains eligible to replace it.
            */
            title = project_agent_session_default_title(&project, &current_with_identity);
            runtime_settings.insert("titleSource".to_string(), json!("placeholder"));
            reason = "agent-default-title-applied".to_string();
        }
    } else {
        reason = "current-title-already-trusted".to_string();
    }

    let mut update = lifecycle_update(lifecycle);
    if let Some(agent_id) = next_agent.clone() {
        update.insert("agentId".to_string(), json!(agent_id));
    }
    if should_promote_agent {
        update.insert("kind".to_string(), json!("agent"));
    }
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings.clone()),
    );
    update.insert("title".to_string(), json!(title));
    if stale_launch_metadata_cleared > 0 {
        update.insert(
            "launchSettings".to_string(),
            Value::Object(launch_settings.clone()),
        );
    }
    let needs_update = update.get("title") != session.get("title")
        || stale_launch_metadata_cleared > 0
        || next_agent != read_text_value(&session, "agentId")
        || (should_promote_agent && session.get("kind").and_then(Value::as_str) != Some("agent"))
        || runtime_settings.get("agentName")
            != object_field(&session, "runtimeSettings").get("agentName")
        || runtime_settings.get("agentId")
            != object_field(&session, "runtimeSettings").get("agentId")
        || runtime_settings.get("agentSessionId")
            != object_field(&session, "runtimeSettings").get("agentSessionId")
        || runtime_settings.get("agentSessionPath")
            != object_field(&session, "runtimeSettings").get("agentSessionPath")
        || runtime_settings.get("launchAgentId")
            != object_field(&session, "runtimeSettings").get("launchAgentId")
        || runtime_settings.get("firstPromptTitleGenerationAgent")
            != object_field(&session, "runtimeSettings").get("firstPromptTitleGenerationAgent")
        || runtime_settings.get("firstPromptTitleGenerationCommand")
            != object_field(&session, "runtimeSettings").get("firstPromptTitleGenerationCommand")
        || runtime_settings.get("firstUserMessage")
            != object_field(&session, "runtimeSettings").get("firstUserMessage")
        || runtime_settings.get("agentActivity")
            != object_field(&session, "runtimeSettings").get("agentActivity")
        || runtime_settings.get("titleSource")
            != object_field(&session, "runtimeSettings").get("titleSource");
    let updated = if needs_update {
        repository.update_session(&update)?
    } else {
        current_with_identity
    };
    /*
    CDXC:SessionNotes 2026-08-24:
    Session notes are keyed by the agent session id, so whenever the stored id
    transitions old→new the note must follow or it strands on the dead id and
    silently disappears from every surface. EVERY identity source funnels
    through this function — agent-hook ingest, the live-process scan, and the
    transcript-successor repair — so this is the single choke point; re-keying
    only in the successor path missed the common hooks-installed configuration
    where the hook lands the new Claude conversation id first. The helper
    no-ops on identical ids and never overwrites a note already written
    against the new id. An agent change removes the stored id instead of
    replacing it, so no re-key fires and the old conversation keeps its note.

    CDXC:SavedPrompts 2026-08-24:
    Stashed prompts are keyed by the same conversation id (0026) and ride the
    same choke point for the same reason: a compaction would otherwise strand
    every prompt stashed from this thread on the dead id, dropping them out of
    the composer count and the "This session" scope.
    */
    if needs_update {
        let previous_agent_session_id =
            read_text_from_map(&object_field(&session, "runtimeSettings"), "agentSessionId");
        let next_agent_session_id = runtime_settings
            .get("agentSessionId")
            .and_then(Value::as_str);
        if let (Some(previous), Some(next)) =
            (previous_agent_session_id.as_deref(), next_agent_session_id)
        {
            repository.rekey_session_agent_note(previous, next)?;
            repository.rekey_stashed_prompt_agent_sessions(previous, next)?;
        }
    }
    let mut result = object_from_value(json!({
        "changed": needs_update,
        "projection": project_session_title_projection(&updated),
        "reason": if needs_update { reason } else { "unchanged".to_string() },
        "session": updated.clone(),
    }));
    if let Some(conflict) = identity_conflict {
        result.insert(
            "identityConflict".to_string(),
            session_identity_conflict_value(&conflict),
        );
    }
    Ok((result, updated))
}

/*
CDXC:SessionIdentity 2026-09-01:
The live zmx process scan re-observes the SAME identity on every poll, and a
poll happens on every listSessions / readPresentationSnapshot / WS-subscribe
request. Steady state is therefore "everything this observation
carries is already stored", which `apply_session_state_update` answers with
`changed: false` after re-deriving the identity, re-deriving runtime settings,
and — until the lazy handle above — hydrating the project's whole session list.

This predicate answers the same question from the row that is already in hand,
with no database read at all. It replays the update path's own helpers rather
than re-stating their rules, and returns true ONLY when every term of that
path's `needs_update` is provably false:

  1. `apply_session_identity_runtime_settings` produced a runtime settings map
     byte-identical to the stored one. The LiveProcess params carry no
     `firstPromptTitleGeneration*` / `firstUserMessage` keys, so the
     `insert_*_from_params` calls that follow it are no-ops, and whole-map
     equality covers every runtime key `needs_update` compares (agentName,
     agentId, agentSessionId, agentSessionPath, launchAgentId, agentActivity,
     titleSource).
  2. The promoted `agentId` equals the stored one.
  3. Nothing would promote `kind` that is not already `agent`.
  4. The row's title is already trusted, so the update path takes the
     `current-title-already-trusted` branch: it neither consults sibling
     sessions nor rewrites the title, which is also why this is the one
     condition that keeps the sibling-title adoption behaviour intact for rows
     that still carry a placeholder title.
  5. The stored title is already trimmed, so the trimmed title the update path
     writes back equals the stored value verbatim.

Under 1-3 the `current_with_identity` the update path builds is the stored row
itself, so 4 and 5 may be evaluated against the stored row directly.

With `needs_update` false the update path writes nothing: the session-note and
stashed-prompt re-keys are gated on it, `require_project` cannot fail for a row
whose project is enforced by an ON DELETE CASCADE foreign key, and the only
thing the caller reads back is `changed`. Skipping the call is therefore
observationally identical, not an approximation. Any future write that
`apply_session_state_update` performs unconditionally must be reflected here.
*/
pub(crate) fn live_process_identity_update_is_noop(
    session: &Value,
    params: &Map<String, Value>,
) -> bool {
    let observed_identity = align_observed_identity_with_launch_profile(
        session,
        resolve_session_identity(&IdentityInput {
            agent_id: None,
            agent_name: read_text(params, "agentName"),
            agent_session_id: read_text(params, "agentSessionId"),
            agent_session_path: read_text(params, "agentSessionPath"),
            runtime_settings: Map::new(),
            startup_text: read_text(params, "startupText"),
        }),
    );
    let current_identity = resolve_stored_session_identity(session);
    /*
    `resolve_allowed_session_identity` hands every non-passive source its
    resolved identity back unchanged, so for a LiveProcess observation the
    identity that would be applied is exactly the merge of observed over stored.
    */
    let identity = merge_observed_session_identity(&observed_identity, &current_identity);
    let stored_runtime_settings = object_field(session, "runtimeSettings");
    let runtime_settings = apply_session_identity_runtime_settings(
        &current_identity,
        &identity,
        stored_runtime_settings.clone(),
        SessionIdentityUpdateSource::LiveProcess,
        session_launch_agent_provider_id(session),
    );
    if runtime_settings != stored_runtime_settings {
        return false;
    }
    let stored_agent_id = read_text_value(session, "agentId");
    let next_agent = identity
        .agent_id
        .clone()
        .or_else(|| stored_agent_id.clone());
    if next_agent != stored_agent_id {
        return false;
    }
    let should_promote_agent = next_agent.is_some()
        || identity.agent_session_id.is_some()
        || identity.agent_session_path.is_some();
    if should_promote_agent && session.get("kind").and_then(Value::as_str) != Some("agent") {
        return false;
    }
    if trusted_resume_title(session).is_none() {
        return false;
    }
    read_text_value(session, "title").as_deref() == session.get("title").and_then(Value::as_str)
}
