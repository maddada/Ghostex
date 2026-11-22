use serde_json::{json, Map, Value};

use super::*;
use crate::domain::DomainStateError;

/// Resolves the identity a session is allowed to adopt.
///
/// `project_sessions` is only hydrated inside the passive-Codex ownership
/// branch: every other source (and every passive observation that carries no
/// new Codex conversation id) returns before the list is ever read. When it IS
/// read it is the project's non-stopped rows, which is every row
/// `is_active_identity_owner` can accept.
pub(crate) fn resolve_allowed_session_identity(
    current_identity: &ResolvedIdentity,
    current_session: &Value,
    observed_identity: &ResolvedIdentity,
    resolved_identity: &ResolvedIdentity,
    project_sessions: &mut LazyProjectSessions<'_, '_>,
    source: SessionIdentityUpdateSource,
) -> Result<(ResolvedIdentity, Option<SessionIdentityConflict>), DomainStateError> {
    let observed_agent_id = normalize_agent_id(observed_identity.agent_id.as_deref());
    let current_agent_id = normalize_agent_id(current_identity.agent_id.as_deref());
    let resolved_agent_id = normalize_agent_id(resolved_identity.agent_id.as_deref());
    let incoming_agent_session_id = observed_identity
        .agent_session_id
        .as_deref()
        .and_then(normalize_codex_session_id);
    let current_codex_session_id = if current_agent_id.as_deref() == Some("codex") {
        current_identity
            .agent_session_id
            .as_deref()
            .and_then(normalize_codex_session_id)
    } else {
        None
    };
    if let Some(conflict) = unwritten_conversation_conflict(
        current_identity,
        current_session,
        observed_identity,
        observed_agent_id.as_deref(),
        current_agent_id.as_deref(),
        resolved_agent_id.as_deref(),
        source,
    ) {
        return Ok((
            keep_current_session_identity(resolved_identity, current_identity),
            Some(conflict),
        ));
    }
    let is_passive_codex_observation = source == SessionIdentityUpdateSource::Passive
        && incoming_agent_session_id.is_some()
        && (observed_agent_id.as_deref() == Some("codex")
            || (observed_agent_id.is_none()
                && current_agent_id.as_deref() == Some("codex")
                && resolved_agent_id.as_deref() == Some("codex")));
    if !is_passive_codex_observation {
        return Ok((resolved_identity.clone(), None));
    }
    let incoming_agent_session_id = incoming_agent_session_id.expect("checked above");
    if let Some(current_codex_session_id) = current_codex_session_id {
        if current_codex_session_id != incoming_agent_session_id {
            let conflict = SessionIdentityConflict {
                agent_id: "codex".to_string(),
                current_agent_session_id: Some(current_codex_session_id),
                incoming_agent_session_id,
                owner_project_id: None,
                owner_session_id: None,
                reason: "passive-agent-session-id-replacement",
                source,
            };
            return Ok((
                keep_current_session_identity(resolved_identity, current_identity),
                Some(conflict),
            ));
        }
        return Ok((resolved_identity.clone(), None));
    }
    if let Some(owner) = find_active_codex_identity_owner(
        project_sessions.get()?,
        current_session,
        &incoming_agent_session_id,
    ) {
        let conflict = SessionIdentityConflict {
            agent_id: "codex".to_string(),
            current_agent_session_id: None,
            incoming_agent_session_id,
            owner_project_id: read_text_value(&owner, "projectId"),
            owner_session_id: read_text_value(&owner, "sessionId"),
            reason: "active-agent-session-id-owned",
            source,
        };
        return Ok((
            keep_current_session_identity(resolved_identity, current_identity),
            Some(conflict),
        ));
    }
    Ok((resolved_identity.clone(), None))
}

/// CDXC:SessionIdentity 2026-09-20 WHY:
/// Claude reports a brand-new conversation id through its hooks before it writes a single transcript line, and it writes none at all until the first turn. A Claude that started and died within a second therefore replaced a working conversation with an id no CLI can resume; the next wake failed its exact resume and the title lookup opened another project's chat (observed 2026-09-19, session S60-P7369-G6gmp). An observation that names a transcript which is not on disk is not evidence yet, so keep the conversation that is: the same id arrives again on the next hook event once the file exists.
/// `align_observed_identity_with_launch_profile` has already renamed a Claude observation to its `custom-…` profile by the time this runs, so the family is read from the session's launch icon for those ids; checking for the literal `claude` let every custom Claude profile skip the guard.
/// CDXC:SessionIdentity 2026-09-24 WHY:
/// The guard also covers live process scans, and an observation without a transcript path is checked against `<id>.jsonl` beside the current transcript instead of passing unchecked. A fresh Claude that never received a prompt replaced conversation 083f9352 in session G3aa6 on 2026-09-21, and after a reboot the sidebar click resumed an id Claude had never written.
/// CDXC:SessionIdentity 2026-10-17 WHY:
/// Copilot CLI gets the same guard (its transcript is `session-state/<id>/events.jsonl`). `copilot --resume <id>` also opens a scratch session it never writes, and its hooks reported that id, so a session woken once stored it and the next wake ran `copilot --resume <scratch id>`: Copilot printed its usage, the pane fell back to the shell and the conversation was gone from the row (seen live 2026-10-17, session G8nen).
fn unwritten_conversation_conflict(
    current_identity: &ResolvedIdentity,
    current_session: &Value,
    observed_identity: &ResolvedIdentity,
    observed_agent_id: Option<&str>,
    current_agent_id: Option<&str>,
    resolved_agent_id: Option<&str>,
    source: SessionIdentityUpdateSource,
) -> Option<SessionIdentityConflict> {
    if !matches!(
        source,
        SessionIdentityUpdateSource::Passive | SessionIdentityUpdateSource::LiveProcess
    ) {
        return None;
    }
    let is_family = |agent_id: Option<&str>, family: &str| {
        agent_id == Some(family)
            || (agent_id.is_some_and(|agent_id| agent_id.starts_with("custom-"))
                && session_launch_agent_provider_id(current_session).as_deref() == Some(family))
    };
    let family = ["claude", "copilot"].into_iter().find(|family| {
        is_family(observed_agent_id, family)
            || (observed_agent_id.is_none()
                && is_family(current_agent_id, family)
                && is_family(resolved_agent_id, family))
    })?;
    let incoming = trimmed_identity_value(observed_identity.agent_session_id.as_deref())?;
    let current = trimmed_identity_value(current_identity.agent_session_id.as_deref())?;
    if incoming == current {
        return None;
    }
    let current_path = trimmed_identity_value(current_identity.agent_session_path.as_deref())
        .map(|path| crate::resume_lookup::expand_home(&path))
        .filter(|path| path.is_file())?;
    let incoming_path =
        match trimmed_identity_value(observed_identity.agent_session_path.as_deref()) {
            Some(path) => crate::resume_lookup::expand_home(&path),
            None if family == "copilot" => current_path
                .parent()?
                .with_file_name(&incoming)
                .join("events.jsonl"),
            None => current_path.with_file_name(format!("{incoming}.jsonl")),
        };
    if incoming_path.is_file() {
        return None;
    }
    Some(SessionIdentityConflict {
        agent_id: family.to_string(),
        current_agent_session_id: Some(current),
        incoming_agent_session_id: incoming,
        owner_project_id: None,
        owner_session_id: None,
        reason: "agent-session-id-unwritten",
        source,
    })
}

fn trimmed_identity_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn keep_current_session_identity(
    resolved_identity: &ResolvedIdentity,
    current_identity: &ResolvedIdentity,
) -> ResolvedIdentity {
    ResolvedIdentity {
        agent_id: resolved_identity.agent_id.clone(),
        agent_session_id: current_identity.agent_session_id.clone(),
        agent_session_path: current_identity.agent_session_path.clone(),
    }
}

pub(crate) fn merge_observed_session_identity(
    observed_identity: &ResolvedIdentity,
    current_identity: &ResolvedIdentity,
) -> ResolvedIdentity {
    let observed_agent_id = normalize_agent_id(observed_identity.agent_id.as_deref());
    let current_agent_id = normalize_agent_id(current_identity.agent_id.as_deref());
    let agent_changed = observed_agent_id.is_some()
        && current_agent_id.is_some()
        && observed_agent_id != current_agent_id;
    // CDXC:SessionIdentity 2026-09-07 WHY:
    // Rewinding Codex before its first prompt creates a new UUID without a rollout yet. A new conversation must not inherit the previous conversation's transcript path.
    let session_changed = observed_identity.agent_session_id.is_some()
        && observed_identity.agent_session_id != current_identity.agent_session_id;
    ResolvedIdentity {
        agent_id: observed_agent_id.or(current_agent_id),
        agent_session_id: observed_identity.agent_session_id.clone().or_else(|| {
            (!agent_changed)
                .then(|| current_identity.agent_session_id.clone())
                .flatten()
        }),
        agent_session_path: observed_identity.agent_session_path.clone().or_else(|| {
            (!agent_changed && !session_changed)
                .then(|| current_identity.agent_session_path.clone())
                .flatten()
        }),
    }
}

pub(crate) fn resolve_stored_session_identity(session: &Value) -> ResolvedIdentity {
    let runtime_settings = object_field(session, "runtimeSettings");
    let stored_identity = resolve_session_identity(&IdentityInput {
        agent_id: read_text_value(session, "agentId"),
        agent_name: read_text_from_map(&runtime_settings, "agentName"),
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
        runtime_settings: runtime_settings.clone(),
        startup_text: None,
    });
    /*
    CDXC:SessionIdentity 2026-09-03:
    The transcript path only names the CLI FAMILY (`~/.claude/…jsonl` →
    "claude"), while the stored id may be a sidebar CONFIGURATION of that
    family (`custom-claude-…`). Merging the raw family over the stored id made
    the row's own identity disagree with every later observation, which
    `align_observed_identity_with_launch_profile` maps back onto the custom
    id: each hook and live-process pass then saw an agent change, dropped
    agentActivity (and the transcript path), the next hook re-created it, and
    custom Claude sessions flapped working→idle on every tool call (observed
    live 2026-09-03). Align the path-derived identity with the launch profile
    first, exactly as hook and process observations already are.
    */
    let transcript_path_identity = align_observed_identity_with_launch_profile(
        session,
        resolve_session_identity(&IdentityInput {
            agent_id: None,
            agent_name: None,
            agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
            agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
            runtime_settings: Map::new(),
            startup_text: None,
        }),
    );
    merge_observed_session_identity(&transcript_path_identity, &stored_identity)
}

pub(crate) fn apply_session_identity_runtime_settings(
    current_identity: &ResolvedIdentity,
    identity: &ResolvedIdentity,
    mut runtime_settings: Map<String, Value>,
    source: SessionIdentityUpdateSource,
    launch_agent_provider_id: Option<String>,
) -> Map<String, Value> {
    let current_agent_id = normalize_agent_id(current_identity.agent_id.as_deref());
    let next_agent_id = normalize_agent_id(identity.agent_id.as_deref());
    let agent_changed =
        current_agent_id.is_some() && next_agent_id.is_some() && current_agent_id != next_agent_id;
    let activity_agent_id = read_agent_activity_agent_id(runtime_settings.get("agentActivity"));
    /*
    CDXC:SessionStatus 2026-08-29:
    agentActivity.agentName always stores the canonical CLI family ("claude",
    "codex", …), while a `custom-…` agent id names a sidebar CONFIGURATION of
    that family, declared by launchSettings.icon — the same contract
    launch_agent_mismatch reads. Comparing the family against the raw
    configuration id made every hook/title identity pass wipe a custom
    agent's activity, so continuously working custom Claude sessions flapped
    working→idle once per hook event (observed live 2026-08-29).
    */
    let activity_owner_changed = next_agent_id.is_some()
        && activity_agent_id.is_some()
        && activity_agent_id != next_agent_id
        && activity_agent_id != launch_agent_provider_id;
    if let Some(agent_id) = identity.agent_id.clone() {
        runtime_settings.insert("agentName".to_string(), json!(agent_id));
    }
    if source == SessionIdentityUpdateSource::LiveProcess {
        if let Some(agent_id) = next_agent_id.clone() {
            runtime_settings.insert("launchAgentId".to_string(), json!(agent_id));
        }
    }
    if let Some(agent_session_id) = identity.agent_session_id.clone() {
        runtime_settings.insert("agentSessionId".to_string(), json!(agent_session_id));
    } else if agent_changed {
        runtime_settings.remove("agentSessionId");
    }
    if let Some(agent_session_path) = identity.agent_session_path.clone() {
        runtime_settings.insert("agentSessionPath".to_string(), json!(agent_session_path));
    } else if agent_changed
        || (identity.agent_session_id.is_some()
            && identity.agent_session_id != current_identity.agent_session_id)
    {
        runtime_settings.remove("agentSessionPath");
    }
    if agent_changed {
        runtime_settings.remove("agentId");
    }
    if agent_changed || activity_owner_changed {
        runtime_settings.remove("agentActivity");
    }
    runtime_settings
}

static IDENTITY_LOGGER: std::sync::OnceLock<crate::logging::GxserverLogger> =
    std::sync::OnceLock::new();

/*
Unconditional (not scenario-gated): dropping a live activity record is the one
identity side effect a user can see (a working session shows idle) and it left
no trace at all when it misfired on 2026-09-03, so it had to be found by
sampling the SQLite row. Records ids and activity state only — never a
transcript path.
*/
pub(super) fn log_identity_dropped_agent_activity(
    lifecycle: &LifecycleParams,
    source: SessionIdentityUpdateSource,
    current_identity: &ResolvedIdentity,
    next_identity: &ResolvedIdentity,
    dropped_activity: &Value,
) {
    let logger = IDENTITY_LOGGER.get_or_init(|| {
        crate::logging::GxserverLogger::new(crate::paths::get_gxserver_paths(None))
    });
    let source = match source {
        SessionIdentityUpdateSource::Lifecycle => "lifecycle",
        SessionIdentityUpdateSource::LiveProcess => "liveProcess",
        SessionIdentityUpdateSource::Passive => "passive",
        SessionIdentityUpdateSource::VerifiedRewind => "verifiedRewind",
        SessionIdentityUpdateSource::TerminalTitle => "terminalTitle",
    };
    let _ = logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: "sessionIdentity.agentActivityDropped".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(
            "An identity update dropped this session's activity record because the agent appeared to change."
                .to_string(),
        ),
        details: Some(json!({
            "activity": dropped_activity.get("activity").and_then(Value::as_str),
            "activityAgentName": dropped_activity.get("agentName").and_then(Value::as_str),
            "currentAgentId": current_identity.agent_id,
            "nextAgentId": next_identity.agent_id,
            "projectId": lifecycle.project_id,
            "sessionId": lifecycle.session_id,
            "source": source,
            "workingSource": dropped_activity.get("workingSource").and_then(Value::as_str),
        })),
    });
}

pub(crate) fn read_agent_activity_agent_id(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("agentName"))
        .and_then(Value::as_str)
        .and_then(|value| normalize_agent_id(Some(value)))
}

pub(crate) fn find_active_codex_identity_owner(
    sessions: &[Value],
    current_session: &Value,
    incoming_agent_session_id: &str,
) -> Option<Value> {
    sessions.iter().find_map(|session| {
        if read_text_value(session, "sessionId") == read_text_value(current_session, "sessionId")
            && read_text_value(session, "projectId")
                == read_text_value(current_session, "projectId")
        {
            return None;
        }
        if !is_active_identity_owner(session) {
            return None;
        }
        let runtime_settings = object_field(session, "runtimeSettings");
        let identity = resolve_session_identity(&IdentityInput {
            agent_id: read_text_value(session, "agentId"),
            agent_name: None,
            agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
            agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
            runtime_settings,
            startup_text: None,
        });
        let is_match = normalize_agent_id(identity.agent_id.as_deref()).as_deref() == Some("codex")
            && identity
                .agent_session_id
                .as_deref()
                .and_then(normalize_codex_session_id)
                .as_deref()
                == Some(incoming_agent_session_id);
        is_match.then(|| session.clone())
    })
}

/// A session that could still be tailing the provider conversation it is bound
/// to. Stopped history rows are NOT owners — the registry keeps every session
/// ever created, so treating them as owners blocks legitimate re-binding.
pub(crate) fn is_active_identity_owner(session: &Value) -> bool {
    session.get("lifecycleState").and_then(Value::as_str) == Some("running")
        || session.get("lifecycleState").and_then(Value::as_str) == Some("sleeping")
        || (session.get("lifecycleState").and_then(Value::as_str) != Some("stopped")
            && object_field(session, "providerState")
                .get("lifecycleState")
                .and_then(Value::as_str)
                == Some("exists"))
}

pub(crate) fn session_identity_conflict_value(conflict: &SessionIdentityConflict) -> Value {
    let mut output = Map::new();
    output.insert("agentId".to_string(), json!(conflict.agent_id));
    insert_optional_string(
        &mut output,
        "currentAgentSessionId",
        conflict.current_agent_session_id.clone(),
    );
    output.insert(
        "incomingAgentSessionId".to_string(),
        json!(conflict.incoming_agent_session_id),
    );
    insert_optional_string(
        &mut output,
        "ownerProjectId",
        conflict.owner_project_id.clone(),
    );
    insert_optional_string(
        &mut output,
        "ownerSessionId",
        conflict.owner_session_id.clone(),
    );
    output.insert("reason".to_string(), json!(conflict.reason));
    output.insert(
        "source".to_string(),
        json!(identity_update_source_name(conflict.source)),
    );
    Value::Object(output)
}

pub(crate) fn identity_update_source_name(source: SessionIdentityUpdateSource) -> &'static str {
    match source {
        SessionIdentityUpdateSource::Lifecycle => "lifecycle",
        SessionIdentityUpdateSource::LiveProcess => "live-process",
        SessionIdentityUpdateSource::Passive => "passive",
        SessionIdentityUpdateSource::VerifiedRewind => "verified-rewind",
        SessionIdentityUpdateSource::TerminalTitle => "terminal-title",
    }
}
