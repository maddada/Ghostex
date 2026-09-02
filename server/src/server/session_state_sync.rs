use super::*;
use crate::agents::{repair_session_working_directory_title, LifecycleParams};

/*
CDXC:GxserverSessionSyncOneList 2026-09-01:
The presentation sync passes used to each run their own `list_sessions`, so a
single `listSessions` or `readPresentationSnapshot` request built the full JSON
row set roughly five times over. They now operate on the one list their caller
already fetched and report whether they changed any row, so the caller re-reads
only when a (rare) mutation actually happened.
*/
pub(crate) fn sync_zmx_provider_existence(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    sessions: &[Value],
) -> std::result::Result<bool, DomainStateError> {
    let mut changed_any = false;
    let candidates = sessions
        .iter()
        .filter(|session| {
            session.get("lifecycleState").and_then(Value::as_str) == Some("running")
                && session
                    .get("providerState")
                    .and_then(Value::as_object)
                    .and_then(|provider| provider.get("lifecycleState"))
                    .and_then(Value::as_str)
                    == Some("exists")
                && read_session_persistence_provider(session).as_deref() == Some("zmx")
                && session
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .and_then(|created_at| chrono::DateTime::parse_from_rfc3339(created_at).ok())
                    .is_some_and(|created_at| {
                        chrono::Utc::now()
                            .signed_duration_since(created_at.with_timezone(&chrono::Utc))
                            >= chrono::Duration::seconds(30)
                    })
        })
        .filter_map(|session| {
            Some((
                read_session_text(session, "projectId")?,
                read_session_text(session, "sessionId")?,
                read_session_text(session, "zmxName")?,
            ))
        })
        .collect::<Vec<_>>();
    let mut sessions_updated_here: HashSet<(String, String)> = HashSet::new();
    if !candidates.is_empty() {
        if let Ok(existing_names) = read_cached_zmx_existing_session_names() {
            for (candidate_project_id, candidate_session_id, zmx_name) in candidates {
                if existing_names.contains(&zmx_name) {
                    continue;
                }
                let Some(current) =
                    repository.get_session(&candidate_project_id, &candidate_session_id)?
                else {
                    continue;
                };
                let mut provider_state = current
                    .get("providerState")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                if provider_state.get("lifecycleState").and_then(Value::as_str) != Some("exists") {
                    continue;
                }
                provider_state.remove("killError");
                provider_state.remove("probeError");
                provider_state.insert("lifecycleState".to_string(), json!("missing"));
                provider_state.insert("probedAt".to_string(), json!(now_iso()));
                provider_state.insert("zmxName".to_string(), json!(zmx_name));
                let mut update = Map::new();
                update.insert("projectId".to_string(), json!(candidate_project_id.clone()));
                update.insert("sessionId".to_string(), json!(candidate_session_id.clone()));
                update.insert(
                    "lifecycleState".to_string(),
                    json!(
                        if current.get("surface").and_then(Value::as_str) == Some("commands") {
                            "stopped"
                        } else {
                            current
                                .get("lifecycleState")
                                .and_then(Value::as_str)
                                .unwrap_or("running")
                        }
                    ),
                );
                update.insert("providerState".to_string(), Value::Object(provider_state));
                repository.update_session_for_lifecycle(&update)?;
                changed_any = true;
                sessions_updated_here
                    .insert((candidate_project_id.clone(), candidate_session_id.clone()));
                schedule_presentation_session_delta(
                    state,
                    db,
                    repository,
                    &candidate_project_id,
                    &candidate_session_id,
                )?;
            }
        }
    }

    /*
    CDXC:GxserverUnrestorableAgentSessions 2026-08-04:
    A workspace agent whose zmx provider disappeared is recoverable only when
    durable state can produce the same queued launch/resume startup text used
    by startSessionProvider. Keeping an agent with neither a provider nor that
    startup text as `running` creates an impossible active row: desktop clients
    retain or reveal an empty terminal tab, while gxserver correctly advertises
    attach/focus as unavailable. Mark only those unrestorable agent rows
    during the existing authoritative provider reconciliation. Plain terminal
    rows still restart as shells, and agents with queued or resume startup text
    remain running so the ordinary restore path can revive them.

    CDXC:UnrestorableAgentsSleepNotStop 2026-09-01:
    They are marked "sleeping", not "stopped". "stopped" is the user's close
    action; writing it here made sessions silently vanish from the sidebar
    whenever a daemon died outside Ghostex's control (crash, reboot, external
    kill). Sleeping avoids the impossible active row just as well — a sleeping
    session holds no terminal tab — while the row stays visible and its chat
    history reachable; waking one relaunches a plain shell in the session's
    cwd. Only an explicit user close writes "stopped".
    */
    /*
    The pass above flips providerState `exists` -> `missing`, and this filter
    looks for `missing`, so it must see those writes. Re-read only the rows the
    pass actually updated (normally none) instead of re-listing every session.
    */
    let mut missing_agent_candidates = sessions
        .iter()
        .filter(|session| {
            let key = (
                read_session_text(session, "projectId").unwrap_or_default(),
                read_session_text(session, "sessionId").unwrap_or_default(),
            );
            !sessions_updated_here.contains(&key)
        })
        .filter(|session| is_running_missing_zmx_agent(session))
        .cloned()
        .collect::<Vec<_>>();
    for (updated_project_id, updated_session_id) in &sessions_updated_here {
        let Some(refreshed) = repository.get_session(updated_project_id, updated_session_id)?
        else {
            continue;
        };
        if is_running_missing_zmx_agent(&refreshed) {
            missing_agent_candidates.push(refreshed);
        }
    }
    if missing_agent_candidates.is_empty() {
        return Ok(changed_any);
    }
    let agent_settings = read_agent_settings(db)?;
    for session in missing_agent_candidates {
        let Some(candidate_project_id) = read_session_text(&session, "projectId") else {
            continue;
        };
        let Some(candidate_session_id) = read_session_text(&session, "sessionId") else {
            continue;
        };
        let Some(project) = repository.get_project(&candidate_project_id)? else {
            continue;
        };
        if !should_sleep_unrestorable_missing_zmx_agent(&project, &session, &agent_settings) {
            continue;
        }
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(candidate_project_id.clone()));
        update.insert("sessionId".to_string(), json!(candidate_session_id.clone()));
        update.insert("lifecycleState".to_string(), json!("sleeping"));
        repository.update_session_for_lifecycle(&update)?;
        changed_any = true;
        schedule_presentation_session_delta(
            state,
            db,
            repository,
            &candidate_project_id,
            &candidate_session_id,
        )?;
    }
    Ok(changed_any)
}

pub(crate) fn should_sleep_unrestorable_missing_zmx_agent(
    project: &Value,
    session: &Value,
    agent_settings: &Map<String, Value>,
) -> bool {
    is_running_missing_zmx_agent(session)
        && get_persisted_provider_startup_text_for_session(project, session, agent_settings)
            .is_none()
}

pub(crate) fn is_running_missing_zmx_agent(session: &Value) -> bool {
    session.get("lifecycleState").and_then(Value::as_str) == Some("running")
        && session.get("kind").and_then(Value::as_str) == Some("agent")
        && read_session_persistence_provider(session).as_deref() == Some("zmx")
        && session
            .get("providerState")
            .and_then(Value::as_object)
            .and_then(|provider| provider.get("lifecycleState"))
            .and_then(Value::as_str)
            == Some("missing")
}

pub(crate) fn sync_live_zmx_process_identities(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    sessions: &[Value],
    target_session_id: Option<&str>,
    _reason: &str,
) -> std::result::Result<bool, DomainStateError> {
    let candidates = sessions
        .iter()
        .filter(|session| should_sync_live_zmx_process_identity(session))
        .filter(|session| match target_session_id {
            None => true,
            Some(target_session_id) => {
                should_probe_title_signaled_zmx_process_identity(session)
                    && read_session_text(session, "sessionId").as_deref() == Some(target_session_id)
            }
        })
        .filter_map(|session| {
            Some((
                read_session_text(session, "projectId")?,
                read_session_text(session, "sessionId")?,
                read_session_text(session, "zmxName")?,
            ))
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(false);
    }
    let session_names = candidates
        .iter()
        .map(|(_, _, zmx_name)| zmx_name.clone())
        .collect::<Vec<_>>();
    let Ok(identities) =
        read_cached_zmx_session_process_identities(&session_names, &state.paths.home_dir)
    else {
        return Ok(false);
    };
    let mut changed_any = false;
    let codex_hook_identities = read_codex_hook_session_identities(&state.paths);
    for (candidate_project_id, candidate_session_id, _) in candidates {
        let Some(current) = repository.get_session(&candidate_project_id, &candidate_session_id)?
        else {
            continue;
        };
        if !should_sync_live_zmx_process_identity(&current) {
            continue;
        }
        let Some(zmx_name) = read_session_text(&current, "zmxName") else {
            continue;
        };
        let Some(identity) = identities.get(&zmx_name) else {
            continue;
        };
        if identity.agent_id.is_none() {
            continue;
        }
        /*
        CDXC:GxserverAgentTitles 2026-08-04:
        Codex forks and in-TUI resumes do not put the current conversation id
        in the process argv that this live zmx repair scans. The installed hook
        records that id against the exact GHOSTEX_SESSION_ID/zmx surface. Fold
        the newest persisted mapping into this existing authoritative live
        repair so restart/status reads correct stale identities automatically;
        title reconciliation below then consumes the same session_index.jsonl
        metadata as terminal-title notifications.
        */
        let hook_identity = (identity.agent_id.as_deref() == Some("codex"))
            .then(|| codex_hook_identities.get(&zmx_name))
            .flatten();
        let agent_session_id = hook_identity
            .map(|hook| hook.agent_session_id.clone())
            .or_else(|| identity.agent_session_id.clone());
        let agent_session_path = hook_identity
            .and_then(|hook| hook.agent_session_path.clone())
            .or_else(|| identity.agent_session_path.clone());
        /*
        CDXC:GxserverSessionIdentity 2026-06-21-18:25:
        Rust must copy TypeScript gxserver's live zmx process repair before sidebar list/snapshot responses. A running zmx terminal whose foreground process is Codex/Claude/etc. must be promoted to the matching agent row in durable state so macOS shows the same session identity after the server cutover.
        */
        let changed = apply_live_process_session_identity(
            repository,
            &current,
            &candidate_project_id,
            &candidate_session_id,
            identity.agent_id.clone(),
            agent_session_id,
            agent_session_path,
        )?;
        let reconciled = if changed {
            reconcile_agent_metadata_title_for_session(
                repository,
                &candidate_project_id,
                &candidate_session_id,
                &state.paths.home_dir,
                "pending",
            )
            .unwrap_or(false)
        } else {
            false
        };
        if changed || reconciled {
            changed_any = true;
            schedule_presentation_session_delta(
                state,
                db,
                repository,
                &candidate_project_id,
                &candidate_session_id,
            )?;
        }
    }
    Ok(changed_any)
}

pub(crate) fn sync_title_signaled_zmx_process_identity(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> std::result::Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let sessions = repository.list_sessions(Some(project_id))?;
    sync_live_zmx_process_identities(
        state,
        &db,
        &repository,
        &sessions,
        Some(session_id),
        "terminal-title-agent-signal",
    )?;
    Ok(())
}

/*
CDXC:MobileSessionStatus 2026-06-22-00:47:
Rust gxserver must read the same hook sidecar files as TypeScript before list, snapshot, and subscription projections. Hooks can write provider session id and working/idle state even when an HTTP hook POST is missed, so presentation state must ingest those sidecars at the daemon boundary instead of leaving Codex rows idle or unresumable.
*/
pub(crate) fn sync_session_state_sidecars(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    sessions: &[Value],
    _reason: &str,
) -> std::result::Result<bool, DomainStateError> {
    let mut changed_any = false;
    for session in sessions.iter().cloned() {
        let Some(session_project_id) = read_session_text(&session, "projectId") else {
            continue;
        };
        let Some(session_id) = read_session_text(&session, "sessionId") else {
            continue;
        };
        let lifecycle = LifecycleParams {
            project_id: session_project_id.clone(),
            session_id: session_id.clone(),
        };
        let (session, repaired_working_directory_title) =
            repair_session_working_directory_title(repository, &lifecycle, session)?;
        if repaired_working_directory_title {
            changed_any = true;
            schedule_presentation_session_delta(
                state,
                db,
                repository,
                &session_project_id,
                &session_id,
            )?;
        }
        if !should_sync_session_state_sidecar(&session) {
            continue;
        }
        let Some(sidecar) =
            read_session_state_sidecar(&state.paths, &session_project_id, &session_id)
        else {
            continue;
        };
        if !has_session_state_sidecar_payload(&sidecar) {
            continue;
        }
        let changed = apply_session_state_sidecar(
            state,
            db,
            repository,
            &session_project_id,
            &session_id,
            &sidecar,
        )?;
        if changed {
            changed_any = true;
            schedule_presentation_session_delta(
                state,
                db,
                repository,
                &session_project_id,
                &session_id,
            )?;
        }
    }
    Ok(changed_any)
}

pub(crate) fn should_sync_session_state_sidecar(session: &Value) -> bool {
    session.get("lifecycleState").and_then(Value::as_str) == Some("running")
        && session.get("surface").and_then(Value::as_str) != Some("commands")
}

#[derive(Default)]
pub(crate) struct SessionStateSidecar {
    pub(crate) agent_name: Option<String>,
    pub(crate) agent_session_id: Option<String>,
    pub(crate) agent_session_path: Option<String>,
    pub(crate) first_user_message: Option<String>,
    pub(crate) last_activity_at: Option<String>,
    pub(crate) pending_first_prompt_auto_rename_prompt: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) status_updated_at: Option<String>,
    pub(crate) title: Option<String>,
}

pub(crate) fn read_session_state_sidecar(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
) -> Option<SessionStateSidecar> {
    let path = build_session_state_sidecar_path(paths, project_id, session_id);
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > GXSERVER_SESSION_STATE_SIDECAR_MAX_BYTES {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    parse_session_state_sidecar(&raw)
}

pub(crate) fn build_session_state_sidecar_path(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
) -> PathBuf {
    paths
        .root_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("session-state")
        .join(sanitize_session_state_sidecar_path_part(project_id))
        .join(format!(
            "{}.env",
            sanitize_session_state_sidecar_path_part(session_id)
        ))
}

pub(crate) fn sanitize_session_state_sidecar_path_part(value: &str) -> String {
    let mut output = String::new();
    let mut last_was_dash = false;
    for ch in value.chars() {
        let keep = ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-');
        if keep {
            output.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            output.push('-');
            last_was_dash = true;
        }
    }
    let trimmed = output.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "session".to_string()
    } else {
        trimmed
    }
}

pub(crate) fn parse_session_state_sidecar(raw: &str) -> Option<SessionStateSidecar> {
    let mut sidecar = SessionStateSidecar::default();
    for line in raw.lines() {
        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let value = if matches!(key, "firstUserMessageBase64" | "agentSessionPath") {
            raw_value.trim().to_string()
        } else {
            raw_value.split_whitespace().collect::<Vec<_>>().join(" ")
        };
        if value.is_empty() {
            continue;
        }
        match key {
            "agent" => sidecar.agent_name = Some(value),
            "agentSessionId" => sidecar.agent_session_id = Some(value),
            "agentSessionPath" => sidecar.agent_session_path = Some(value),
            "firstUserMessageBase64" => {
                sidecar.first_user_message = decode_session_state_sidecar_base64(&value);
            }
            "lastActivityAt" => sidecar.last_activity_at = normalize_sidecar_iso_timestamp(&value),
            "pendingFirstPromptAutoRenamePrompt" => {
                sidecar.pending_first_prompt_auto_rename_prompt = Some(value);
            }
            "status" if matches!(value.as_str(), "attention" | "idle" | "working") => {
                sidecar.status = Some(value);
            }
            "statusUpdatedAt" => {
                sidecar.status_updated_at = normalize_sidecar_iso_timestamp(&value);
            }
            "title" => sidecar.title = get_visible_terminal_title(&value),
            _ => {}
        }
    }
    if sidecar.first_user_message.is_none() {
        sidecar.first_user_message = sidecar.pending_first_prompt_auto_rename_prompt.clone();
    }
    Some(sidecar)
}

pub(crate) fn has_session_state_sidecar_payload(sidecar: &SessionStateSidecar) -> bool {
    sidecar.agent_name.is_some()
        || sidecar.agent_session_id.is_some()
        || sidecar.agent_session_path.is_some()
        || sidecar.first_user_message.is_some()
        || sidecar.status.is_some()
        || sidecar.title.is_some()
}

pub(crate) fn decode_session_state_sidecar_base64(value: &str) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let decoded = STANDARD.decode(value).ok()?;
    String::from_utf8(decoded)
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

pub(crate) fn normalize_sidecar_iso_timestamp(value: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| {
            timestamp
                .with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
}

pub(crate) fn apply_session_state_sidecar(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    sidecar: &SessionStateSidecar,
) -> std::result::Result<bool, DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    insert_optional_json_string(&mut params, "agentName", sidecar.agent_name.as_deref());
    insert_optional_json_string(
        &mut params,
        "agentSessionId",
        sidecar.agent_session_id.as_deref(),
    );
    insert_optional_json_string(
        &mut params,
        "agentSessionPath",
        sidecar.agent_session_path.as_deref(),
    );
    insert_optional_json_string(
        &mut params,
        "firstUserMessage",
        sidecar.first_user_message.as_deref(),
    );
    insert_optional_json_string(&mut params, "status", sidecar.status.as_deref());
    insert_optional_json_string(
        &mut params,
        "statusUpdatedAt",
        sidecar
            .status_updated_at
            .as_deref()
            .or(sidecar.last_activity_at.as_deref()),
    );
    insert_optional_json_string(&mut params, "title", sidecar.title.as_deref());
    params.insert(
        "eventName".to_string(),
        Value::String("legacy-session-state".to_string()),
    );
    let output = dispatch_agent_endpoint(
        repository,
        db,
        &state.paths.home_dir,
        "/api/ingestAgentHookEvent",
        &params,
        None,
    )
    .map_err(|error| match error {
        AgentEndpointError::Domain(error) => error,
        AgentEndpointError::DependencyUnavailable(message) => DomainStateError {
            code: "dependencyUnavailable",
            message,
        },
    })?;
    let changed = output
        .result
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if changed {
        if let Some(claimed) = output
            .result
            .get("reason")
            .and_then(Value::as_str)
            .filter(|reason| *reason == "first-prompt-auto-title-claimed")
        {
            let _ = claimed;
            if let Some(attempt_id) = output.result.get("session").and_then(|session| {
                read_runtime_text(session, FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY)
            }) {
                schedule_first_prompt_auto_title_job(
                    state.clone(),
                    project_id.to_string(),
                    session_id.to_string(),
                    attempt_id,
                );
            }
        }
        if let Ok(Some(session)) = repository.get_session(project_id, session_id) {
            schedule_stale_activity_presentation_refresh(
                state,
                &session,
                "session-state-sidecar-stale-activity",
            );
        }
    }
    Ok(changed)
}

pub(crate) fn insert_optional_json_string(
    map: &mut Map<String, Value>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

pub(crate) fn should_sync_live_zmx_process_identity(session: &Value) -> bool {
    session.get("lifecycleState").and_then(Value::as_str) == Some("running")
        && session.get("surface").and_then(Value::as_str) != Some("commands")
        && read_session_persistence_provider(session).as_deref() == Some("zmx")
}

pub(crate) fn should_probe_title_signaled_zmx_process_identity(session: &Value) -> bool {
    should_sync_live_zmx_process_identity(session)
        && matches!(
            read_session_text(session, "kind").as_deref(),
            Some("terminal" | "agent")
        )
        && read_runtime_text(session, "agentSessionId").is_none()
}

/*
CDXC:GxserverSessionIdentity 2026-06-30-11:15:
Live process identity repair must use the same persistence-provider source as
presentation rows. Remote/attached zmx sessions can have providerState.provider
without runtimeSettings.sessionPersistenceProvider, and those rows still need
server-side Codex/Claude/etc. promotion so sidebar agent icons come from
canonical gxserver metadata instead of client guesses.
*/
pub(crate) fn read_session_persistence_provider(session: &Value) -> Option<String> {
    read_runtime_text(session, "sessionPersistenceProvider").or_else(|| {
        session
            .get("providerState")
            .and_then(Value::as_object)
            .and_then(|state| state.get("provider"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

pub(crate) fn read_runtime_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn read_session_text(session: &Value, key: &str) -> Option<String> {
    session
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn result_activity(result: &Value) -> Option<&str> {
    result
        .get("activity")
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("activity"))
        .and_then(Value::as_str)
}
