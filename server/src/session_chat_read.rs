use crate::domain::{DomainRepository, DomainStateError, read_domain_rpc_params};
use crate::protocol::rpc_success;
use crate::server::{
    AppState,
    RoutedResponse,
    domain_error_response,
    read_runtime_text,
    routed_json,
    session_observer_key,
};
use crate::session_chat_follower::{
    is_session_chat_followable_session,
    session_chat_agent_for_session,
    session_chat_hook_working,
};
use crate::session_chat_options::{
    SessionChatOptionDetector,
    cached_session_chat_screen_state,
    cached_session_chat_terminal_notice,
};
use crate::storage::open_gxserver_database;
use serde_json::{Map, Value, json};
use axum::http::StatusCode;

/*
CDXC:SessionChatMobileLongPoll 2026-07-31:
Session/transcript state resolved fresh from SQLite plus a change fingerprint
(transcript stat + prompt + lifecycle). The fingerprint lets SSH-only clients
(Ghostex mobile) long-poll readSessionChat instead of subscribing to
/api/events: pass the previous `fingerprint` with `waitMs` and the handler
holds the request until the fingerprint changes or the wait times out.
Transcript path re-resolution can scan agent home directories, so an
unchanged (agent, agentSessionId, agentSessionPath) triple reuses the cached
path while it still exists on disk.
*/
pub(crate) struct SessionChatReadResolution {
    agent: Option<String>,
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
    lifecycle_running: bool,
    /// Agent-hook activity: true while the agent is working on a turn.
    working: bool,
    stored_prompt: Option<String>,
    transcript_path: Option<std::path::PathBuf>,
    /*
    CDXC:SessionChatQueueCarriage 2026-08-21: the Ghostex prompt queue and the
    synced composer draft, resolved on the SAME connection the rest of this
    state comes from. Folded into the fingerprint below, without which a mobile
    client — which synthesizes its frames from long-polled reads — would never
    learn that the queue or the draft changed at all.
    */
    queue: crate::session_chat_queue::SessionChatQueueSnapshot,
    fingerprint: String,
}

pub(crate) fn resolve_session_chat_read_state(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    cached: Option<&SessionChatReadResolution>,
) -> Result<SessionChatReadResolution, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| DomainStateError {
            code: "notFound",
            message: "The session no longer exists.".to_string(),
        })?;
    let agent = session_chat_agent_for_session(&session);
    let agent_session_id = read_runtime_text(&session, "agentSessionId");
    let agent_session_path = read_runtime_text(&session, "agentSessionPath");
    let lifecycle_running = is_session_chat_followable_session(&session);
    let working = session_chat_hook_working(&session);
    let stored_prompt = crate::agents::session_chat_prompt_setting(&session);
    let queue = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
        &db, project_id, session_id,
    );
    drop(session);
    drop(repository);
    drop(db);

    let transcript_path =
        match crate::session_chat::resolve_session_chat_transcript_agent(agent.as_deref()) {
            None => None,
            Some(transcript_agent) => {
                let cached_path = cached
                    .filter(|previous| {
                        previous.agent == agent
                            && previous.agent_session_id == agent_session_id
                            && previous.agent_session_path == agent_session_path
                    })
                    .and_then(|previous| previous.transcript_path.clone())
                    .filter(|path| path.is_file());
                match cached_path {
                    Some(path) => Some(path),
                    None => crate::session_chat::resolve_session_chat_transcript_path(
                        transcript_agent,
                        agent_session_id.as_deref(),
                        agent_session_path.as_deref(),
                    ),
                }
            }
        };

    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    agent.hash(&mut hasher);
    agent_session_id.hash(&mut hasher);
    stored_prompt.hash(&mut hasher);
    lifecycle_running.hash(&mut hasher);
    // Long-pollers must wake on a working↔idle flip: it is the only way an
    // SSH-only client learns the spinner started.
    working.hash(&mut hasher);
    match transcript_path.as_deref() {
        Some(path) => {
            path.hash(&mut hasher);
            if let Ok(metadata) = std::fs::metadata(path) {
                metadata.len().hash(&mut hasher);
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = modified.duration_since(std::time::UNIX_EPOCH) {
                        elapsed.as_millis().hash(&mut hasher);
                    }
                }
            }
        }
        None => {
            0u8.hash(&mut hasher);
        }
    }
    /*
    CDXC:SessionChatTerminalNotices 2026-08-19:
    CACHED notice identity only — kind plus the human text, never `detectedAt`
    and never the raw screen (both churn every probe). This loop runs every
    500ms per long-poller, so it reads the detector cache and the watchdog map
    and NOTHING else: a detection here would spawn a process per tick. Cost of
    that discipline: an SSH-only client can learn about a new SCREEN notice one
    poll cycle (≤20s) late, when the next probe refreshes the cache.
    */
    match cached_session_chat_terminal_notice(state, project_id, session_id) {
        Some(notice) => notice.identity().hash(&mut hasher),
        None => 0u8.hash(&mut hasher),
    }
    /*
    CDXC:SessionChatTerminalActivity 2026-08-22:
    The progress row DOES hash its numbers, unlike the notice above. A notice
    that says the same thing must not churn the fingerprint; a progress bar that
    moved is the only reason to re-read at all, and an SSH-only client with no
    event socket would otherwise watch a frozen bar for the whole compaction.
    Still cache-only: no detection, no spawn, on this 500ms loop.
    */
    let screen = cached_session_chat_screen_state(state, project_id, session_id);
    match screen.activity {
        Some(activity) => {
            activity.kind.hash(&mut hasher);
            activity.percent.hash(&mut hasher);
            activity.elapsed_seconds.hash(&mut hasher);
        }
        None => 0u8.hash(&mut hasher),
    }
    /*
    CDXC:SessionChatAgentFleet 2026-08-23:
    The ROSTER only — names and tasks, never the clocks. Opposite choice from
    the progress row above and for the opposite reason: a fleet row's clock
    moves every second for as long as the agent runs, so hashing it would make
    this 500ms loop re-read the whole transcript forever. The client ticks those
    clocks itself from `detectedAt`.
    */
    match screen.fleet {
        Some(fleet) => {
            for agent in &fleet.agents {
                agent.name.hash(&mut hasher);
                agent.task.hash(&mut hasher);
            }
        }
        None => 0u8.hash(&mut hasher),
    }
    /*
    CDXC:SessionChatPromptQueue 2026-08-21:
    Queue revision + draft updatedAt. This is load-bearing for Ghostex mobile:
    it has no /api/events socket and rebuilds its frames from long-polled
    readSessionChat results, so a queue or draft change that does not move the
    fingerprint is a change the phone never sees. Already-materialised rows —
    no extra query, no extra connection.
    */
    queue.revision().hash(&mut hasher);
    let fingerprint = format!("{:016x}", hasher.finish());

    Ok(SessionChatReadResolution {
        agent,
        agent_session_id,
        agent_session_path,
        lifecycle_running,
        working,
        stored_prompt,
        transcript_path,
        queue,
        fingerprint,
    })
}

/*
CDXC:SessionChatCore 2026-07-31:
Read-path endpoint: reverse tail read of the resolved transcript. A missing
transcript on a RUNNING session reports status "starting" (never an error) —
the agent CLI can take seconds to minutes to flush its first JSONL line, and
the follower's resolve-poll keeps looking. epoch/seq mirror the live follower
stream when one exists so clients can order this read against frames.
*/
pub(crate) async fn handle_read_session_chat_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let project_id = params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if project_id.is_empty() || session_id.is_empty() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "readSessionChat requires projectId and sessionId.".to_string(),
            },
        );
    }
    let limit = params
        .get("limit")
        .and_then(Value::as_i64)
        .map(|value| value.clamp(0, crate::session_chat::SESSION_CHAT_MAX_LIMIT as i64) as usize)
        .unwrap_or(crate::session_chat::SESSION_CHAT_INITIAL_LIMIT);
    let before_offset = params.get("beforeOffset").and_then(Value::as_u64);
    let wait_ms = params
        .get("waitMs")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .clamp(0, 30_000) as u64;
    let last_fingerprint = params
        .get("fingerprint")
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut resolution =
        match resolve_session_chat_read_state(state, &project_id, &session_id, None) {
            Ok(resolution) => resolution,
            Err(error) => return domain_error_response(endpoint_path, request_id, error),
        };
    // Long-poll: hold while nothing observable changed, then fall through to
    // the normal read. A vanished session surfaces as the notFound error the
    // immediate read would have produced.
    if wait_ms > 0 {
        if let Some(last_fingerprint) = last_fingerprint.as_deref() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
            while resolution.fingerprint == last_fingerprint && std::time::Instant::now() < deadline
            {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                resolution = match resolve_session_chat_read_state(
                    state,
                    &project_id,
                    &session_id,
                    Some(&resolution),
                ) {
                    Ok(resolution) => resolution,
                    Err(error) => return domain_error_response(endpoint_path, request_id, error),
                };
            }
        }
    }
    let SessionChatReadResolution {
        agent,
        agent_session_id,
        agent_session_path: _,
        lifecycle_running,
        working,
        stored_prompt,
        transcript_path,
        queue,
        fingerprint,
    } = resolution;

    let stream_position = || {
        state
            .session_chat_followers
            .lock()
            .ok()
            .and_then(|followers| {
                followers
                    .get(&session_observer_key(&project_id, &session_id))
                    .map(|entry| entry.stream.current())
            })
            .unwrap_or((0, 0))
    };

    let mut result = Map::new();
    result.insert("fingerprint".to_string(), json!(fingerprint));
    result.insert("working".to_string(), json!(working));
    /*
    CDXC:SessionChatQueueCarriage 2026-08-21:
    `queue` is written on EVERY readSessionChat answer, including the early
    "unsupported"/"starting" returns below, because its presence — even as an
    empty array — is the capability probe a client uses to decide whether to
    show queue controls at all. `draft` rides along only when the server holds
    one: an omitted draft means unchanged, never cleared.
    */
    queue.insert_into(&mut result);
    if let Some(agent) = agent.as_deref() {
        result.insert("agent".to_string(), json!(agent));
    }
    if let Some(agent_session_id) = agent_session_id.as_deref() {
        result.insert("agentSessionId".to_string(), json!(agent_session_id));
    }
    /*
    The pills' "what is the agent ACTUALLY running" value. Structured
    transcript metadata fills values absent from the terminal footer. Read
    through the 5s cache, so a mobile long-poll loop or a paced follow-up read
    is free; a session that is not running (or whose agent has no table) never
    spawns anything and simply omits the field.
    */
    if lifecycle_running {
        let detection = SessionChatOptionDetector::new(state)
            .detect(&project_id, &session_id, agent.as_deref(), false)
            .await;
        if let Some(detected) = detection.options {
            result.insert("selectedOptions".to_string(), detected.to_value());
        }
        /*
        CDXC:SessionChatTerminalNotices 2026-08-19:
        Same capture, same 5s cache, same running-only gate: a stopped session
        has no live screen to classify. Followerless clients (mobile long-poll)
        get watchdog notices here, which is why the merge happens on the read
        path too and not only in the follower's reader.
        */
        if let Some(notice) = crate::session_chat_notice::resolve_session_chat_terminal_notice(
            &project_id,
            &session_id,
            detection.notice,
        ) {
            result.insert("terminalNotice".to_string(), notice.to_value());
        }
        /*
        CDXC:SessionChatTerminalActivity 2026-08-22: same capture, same cache,
        same running-only gate. Omitted means the client clears its progress row.
        */
        if working {
            if let Some(activity) = detection.activity.as_ref() {
                result.insert("terminalActivity".to_string(), activity.to_value());
            }
        }
        /*
        CDXC:SessionChatAgentFleet 2026-08-23: same capture, same cache — but no
        running gate. Sub-agents outlive the turn that spawned them, so
        an idle main agent is exactly when this is worth reading.
        */
        if let Some(fleet) = detection.fleet.as_ref() {
            result.insert("agentFleet".to_string(), fleet.to_value());
        }
        /*
        CDXC:SessionChatScreenProbed 2026-08-22: same capture again. Followerless
        clients need the "detection has run" bit for exactly the reason follower
        clients do — telling a pill that is still loading from one whose agent
        simply never names a model.
        */
        if detection.attempted {
            result.insert("screenProbed".to_string(), json!(true));
        }
    } else {
        /*
        CDXC:SessionChatScreenProbed 2026-08-22: a session that is not running
        has no screen, so detection is skipped entirely above — but the answer
        ("nothing to read") is settled, not pending. Saying so keeps a stopped
        session's pills from sitting under a loading skeleton forever.
        */
        result.insert("screenProbed".to_string(), json!(true));
    }
    let stored_prompt = stored_prompt
        .as_deref()
        .and_then(crate::session_chat::parse_stored_session_chat_prompt);

    let Some(transcript_agent) =
        crate::session_chat::resolve_session_chat_transcript_agent(agent.as_deref())
    else {
        let (epoch, seq) = stream_position();
        result.insert("epoch".to_string(), json!(epoch));
        result.insert("seq".to_string(), json!(seq));
        if let Some(value) = stored_prompt
            .as_ref()
            .and_then(|prompt| serde_json::to_value(prompt).ok())
        {
            result.insert("prompt".to_string(), value);
        }
        result.insert("messages".to_string(), json!([]));
        result.insert("hasMore".to_string(), json!(false));
        result.insert("beforeOffset".to_string(), json!(0));
        result.insert("status".to_string(), json!("unsupported"));
        return routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, Value::Object(result)),
        );
    };

    /*
    CDXC:SessionChatCore 2026-08-01:
    The reported (epoch, seq) must be COHERENT with the bytes in `messages`.
    Sampling the stream before the file read let a resyncing client land at a
    seq whose frames carried rows this read never saw: the client then believed
    it was caught up and sat missing the end of a turn until the next write.
    The position is now sampled around the read, and a follower that published
    while the read was in flight forces a bounded re-read; if it keeps racing we
    report the EARLIER position, which costs one extra client resync instead of
    losing messages.
    */
    const SESSION_CHAT_READ_COHERENCE_ATTEMPTS: usize = 3;
    let mut attempt = 0usize;
    let (read_outcome, epoch, seq) = loop {
        let before = stream_position();
        let path = transcript_path.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            let Some(path) = path else {
                return Ok(crate::session_chat::SessionChatTailPage::NotFound);
            };
            crate::session_chat::read_session_chat_tail_page(
                transcript_agent,
                &path,
                limit,
                before_offset,
            )
        })
        .await;
        let after = stream_position();
        attempt += 1;
        if after == before {
            break (outcome, after.0, after.1);
        }
        if attempt >= SESSION_CHAT_READ_COHERENCE_ATTEMPTS {
            break (outcome, before.0, before.1);
        }
    };
    result.insert("epoch".to_string(), json!(epoch));
    result.insert("seq".to_string(), json!(seq));

    match read_outcome {
        Ok(Ok(crate::session_chat::SessionChatTailPage::Page {
            messages,
            lifecycle,
            has_more,
            before_offset: page_before_offset,
        })) => {
            let status = if before_offset.is_none() && messages.is_empty() && !has_more {
                "empty"
            } else {
                "ready"
            };
            /*
            Pagination pages look at old history, so only a live tail read may
            retire or supply a question card.
            */
            let prompt = if before_offset.is_none() {
                let transcript_prompt =
                    crate::session_chat::scan_transcript_prompt_state(&messages);
                crate::session_chat::resolve_session_chat_prompt(stored_prompt, &transcript_prompt)
            } else {
                stored_prompt
            };
            if let Some(value) = prompt
                .as_ref()
                .and_then(|prompt| serde_json::to_value(prompt).ok())
            {
                result.insert("prompt".to_string(), value);
            }
            result.insert(
                "messages".to_string(),
                serde_json::to_value(&messages).unwrap_or(json!([])),
            );
            if let Some(lifecycle) = lifecycle.as_ref() {
                if let Ok(value) = serde_json::to_value(lifecycle) {
                    result.insert("lifecycle".to_string(), value);
                }
            }
            result.insert("hasMore".to_string(), json!(has_more));
            result.insert("beforeOffset".to_string(), json!(page_before_offset));
            result.insert("status".to_string(), json!(status));
        }
        Ok(Ok(crate::session_chat::SessionChatTailPage::NotFound)) => {
            // Not-yet-flushed transcript on a running session is "starting",
            // never an error: the follower's resolve-poll keeps looking.
            if let Some(value) = stored_prompt
                .as_ref()
                .and_then(|prompt| serde_json::to_value(prompt).ok())
            {
                result.insert("prompt".to_string(), value);
            }
            result.insert("messages".to_string(), json!([]));
            result.insert("hasMore".to_string(), json!(false));
            result.insert("beforeOffset".to_string(), json!(0));
            result.insert(
                "status".to_string(),
                json!(if lifecycle_running {
                    "starting"
                } else {
                    "empty"
                }),
            );
        }
        Ok(Err(_)) | Err(_) => {
            if let Some(value) = stored_prompt
                .as_ref()
                .and_then(|prompt| serde_json::to_value(prompt).ok())
            {
                result.insert("prompt".to_string(), value);
            }
            result.insert("messages".to_string(), json!([]));
            result.insert("hasMore".to_string(), json!(false));
            result.insert("beforeOffset".to_string(), json!(0));
            result.insert("status".to_string(), json!("error"));
            result.insert("error".to_string(), json!("Transcript unavailable"));
        }
    }
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, Value::Object(result)),
    )
}
