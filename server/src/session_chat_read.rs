use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, read_runtime_text, routed_json, session_observer_key, AppState,
    RoutedResponse,
};
use crate::session_chat_follower::{
    is_session_chat_followable_session, session_chat_agent_for_session, session_chat_hook_working,
};
use crate::session_chat_options::{
    cached_session_chat_screen_state, cached_session_chat_terminal_notice,
    SessionChatOptionDetector,
};
use crate::storage::open_gxserver_database;
use axum::http::StatusCode;
use serde_json::{json, Map, Value};

/*
CDXC:Mobile 2026-07-31:
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
    /// CDXC:Drafts 2026-08-28: the session's own `agentId`, not the
    /// transcript family `agent` above.
    session_agent_id: Option<String>,
    terminal_agent: Option<String>,
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
    lifecycle_running: bool,
    /// Agent-hook activity: true while the agent is working on a turn.
    working: bool,
    stored_prompt: Option<String>,
    transcript_path: Option<std::path::PathBuf>,
    /*
    CDXC:SessionChat 2026-08-21: the Ghostex prompt queue and the
    synced composer draft, resolved on the SAME connection the rest of this
    state comes from. Folded into the fingerprint below, without which a mobile
    client — which synthesizes its frames from long-polled reads — would never
    learn that the queue or the draft changed at all.
    */
    queue: crate::session_chat_queue::SessionChatQueueSnapshot,
    /*
    CDXC:Drafts 2026-08-28: the agents this session may still be switched
    to, resolved from the project's agent configuration. `Some` ONLY while the
    session is a draft — its absence is what tells the composer to hide the
    "Agents" section — and folded into the fingerprint below so a long-polling
    client learns about a promotion (or a project agent edit) without a reload.
    */
    available_agents: Option<Value>,
    /*
    CDXC:AgentProviders 2026-09-03: the same-family accounts a PROMPTED session
    can be resumed under, for the composer's "Switch Account" submenu. `None`
    on drafts (they have the agent switcher) and when nothing is compatible.
    In the fingerprint for the same reason `available_agents` is: an SSH-only
    client learns about a project agent edit through the long-polled read.
    */
    switchable_agents: Option<Value>,
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
    /*
    CDXC:Drafts 2026-08-28:
    The session's OWN launch agent id, which `agent` above is not: that one is
    the transcript family, so a project custom agent built on Claude reports
    `claude` there and is indistinguishable from Claude itself. The composer's
    agent switcher needs the concrete id to tick the right row and to name the
    agent it is switching away from, and it is the id `/api/switchDraftAgent`
    takes.
    */
    let session_agent_id = session
        .get("agentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let terminal_agent = crate::session_chat_composer::session_chat_composer_agent_id(&session)
        .or_else(|| agent.clone());
    let agent_session_id = read_runtime_text(&session, "agentSessionId");
    let agent_session_path = read_runtime_text(&session, "agentSessionPath");
    let lifecycle_running = is_session_chat_followable_session(&session);
    let working = session_chat_hook_working(&session);
    let stored_prompt = crate::agents::session_chat_prompt_setting(&session);
    let queue = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
        &db, project_id, session_id,
    );
    let available_agents = if crate::agents::session_is_draft(&session) {
        repository
            .get_project(project_id)?
            .as_ref()
            .map(crate::agents::available_draft_agents)
    } else {
        None
    };
    let switchable_agents = repository
        .get_project(project_id)?
        .as_ref()
        .and_then(|project| crate::agents::switchable_session_agents_value(project, &session));
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
    // CDXC:Drafts 2026-08-28: a switch between two agents of the SAME
    // transcript family moves only this value, and the composer has to follow it.
    session_agent_id.hash(&mut hasher);
    terminal_agent.hash(&mut hasher);
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
    CDXC:AgentScreenDetection 2026-08-19:
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
    CDXC:SessionChat 2026-08-23:
    Ids only, and in-memory only — this term must wake an SSH-only long-poller
    when Ghostex renames its session out from under it, without costing the
    500ms loop a spawn or a query.
    */
    crate::session_chat_app_command::session_chat_app_commands_identity(project_id, session_id)
        .hash(&mut hasher);
    /*
    CDXC:AgentScreenDetection 2026-08-22:
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
    CDXC:AgentScreenDetection 2026-08-23:
    Everything a fleet row shows EXCEPT its clock. Split differently from the
    progress row above, which hashes all of its numbers: a fleet clock moves
    every second for as long as the agent runs, so hashing it would make this
    500ms loop re-read the whole transcript forever, while a token counter moves
    only when an agent did something and is worth waking a poller for. The
    client ticks the clocks itself from `detectedAt`.
    */
    match screen.fleet {
        Some(fleet) => {
            for agent in &fleet.agents {
                agent.name.hash(&mut hasher);
                agent.task.hash(&mut hasher);
                agent.tokens.hash(&mut hasher);
                agent.nested.hash(&mut hasher);
            }
        }
        None => 0u8.hash(&mut hasher),
    }
    // CDXC:SessionChat 2026-09-03: every task's id, subject and
    // status. A status flip is what wakes a long poller; there are no clocks.
    match screen.tasks {
        Some(tasks) => {
            for task in &tasks.tasks {
                task.id.hash(&mut hasher);
                task.subject.hash(&mut hasher);
                task.status.hash(&mut hasher);
                task.blocked_by.hash(&mut hasher);
            }
        }
        None => 0u8.hash(&mut hasher),
    }
    screen
        .prompt
        .as_ref()
        .and_then(|prompt| serde_json::to_string(prompt).ok())
        .hash(&mut hasher);
    /*
    CDXC:SessionChat 2026-08-21:
    Queue revision + draft updatedAt. This is load-bearing for Ghostex mobile:
    it has no /api/events socket and rebuilds its frames from long-polled
    readSessionChat results, so a queue or draft change that does not move the
    fingerprint is a change the phone never sees. Already-materialised rows —
    no extra query, no extra connection.
    */
    queue.revision().hash(&mut hasher);
    /*
    CDXC:Drafts 2026-08-28:
    Already-materialised value, no extra query. It has to be in the fingerprint
    because an SSH-only client rebuilds its frames from long-polled reads: the
    field going away is how it learns the draft was promoted and the "Agents"
    section must disappear.
    */
    available_agents
        .as_ref()
        .map(Value::to_string)
        .hash(&mut hasher);
    switchable_agents
        .as_ref()
        .map(Value::to_string)
        .hash(&mut hasher);
    let fingerprint = format!("{:016x}", hasher.finish());

    Ok(SessionChatReadResolution {
        agent,
        session_agent_id,
        terminal_agent,
        agent_session_id,
        agent_session_path,
        lifecycle_running,
        working,
        stored_prompt,
        transcript_path,
        queue,
        available_agents,
        switchable_agents,
        fingerprint,
    })
}

/*
CDXC:SessionChat 2026-07-31:
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
        session_agent_id,
        terminal_agent,
        agent_session_id,
        agent_session_path: _,
        lifecycle_running,
        working,
        stored_prompt,
        transcript_path,
        queue,
        available_agents,
        switchable_agents,
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
    // Capability probe for clients that must also speak to older daemons whose
    // tail reader could compute `hasMore` before released queue rows were
    // removed. Current readers guarantee the flag describes the filtered page.
    result.insert("hasMoreExact".to_string(), json!(true));
    /*
    CDXC:SessionChat 2026-08-21:
    `queue` is written on EVERY readSessionChat answer, including the early
    "unsupported"/"starting" returns below, because its presence — even as an
    empty array — is the capability probe a client uses to decide whether to
    show queue controls at all. `draft` rides along only when the server holds
    one: an omitted draft means unchanged, never cleared.
    */
    queue.insert_into(&mut result);
    /*
    CDXC:Drafts 2026-08-28:
    Written on EVERY answer for a draft, including the early "unsupported" /
    "starting" returns below — a draft's CLI is usually still booting, which is
    exactly when the user reaches for the agent switcher. Absent on a promoted
    session: the agent is fixed once its first prompt has landed.
    */
    if let Some(available_agents) = available_agents.clone() {
        result.insert("availableAgents".to_string(), available_agents);
    }
    if let Some(switchable_agents) = switchable_agents.clone() {
        result.insert("switchableAgents".to_string(), switchable_agents);
    }
    if let Some(session_agent_id) = session_agent_id.as_deref() {
        result.insert("sessionAgentId".to_string(), json!(session_agent_id));
    }
    if let Some(agent) = agent.as_deref() {
        result.insert("agent".to_string(), json!(agent));
    }
    if let Some(agent_session_id) = agent_session_id.as_deref() {
        result.insert("agentSessionId".to_string(), json!(agent_session_id));
    }
    let mut screen_prompt = None;
    /*
    The pills' "what is the agent ACTUALLY running" value. Structured
    transcript metadata fills values absent from the terminal footer. Read
    through the 5s cache, so a mobile long-poll loop or a paced follow-up read
    is free; a session that is not running (or whose agent has no table) never
    spawns anything and simply omits the field.
    */
    if lifecycle_running {
        /*
        CDXC:AgentScreenDetection 2026-08-24:
        Bounded exactly like the follower's seed probe
        (CDXC:AgentScreenDetection), with the same value, for the same
        reason: the capture's own socket read timeout is 5s, so a daemon that
        accepts the connection and never answers would hold the transcript
        answer hostage for it. The transcript is the product of this endpoint;
        the screen-derived fields below are decoration, and the client contract
        already treats every one of them as omissible.

        Missing the deadline is not an error and nothing is cancelled or left
        half-written: `detect` does its work in `spawn_blocking`, taking the
        shared cache lock only inside that closure, so abandoning the join
        handle simply lets the probe finish in the background and warm the 5s
        cache for the next read. A timed-out detection reads as the empty one,
        which omits `selectedOptions`, `terminalActivity`, `agentFleet` and
        `screenProbed` alike — the last one deliberately, since detection did
        not settle and "still pending" is the honest answer. The watchdog
        notice merge below still runs on its own cached inputs.
        */
        let detection = tokio::time::timeout(
            crate::session_chat::SEED_OPTION_DETECTION_DEADLINE,
            SessionChatOptionDetector::new(state).detect(
                &project_id,
                &session_id,
                terminal_agent.as_deref(),
                false,
            ),
        )
        .await
        .unwrap_or_default();
        screen_prompt = detection.prompt.clone();
        if let Some(detected) = detection.options {
            result.insert("selectedOptions".to_string(), detected.to_value());
        }
        /*
        CDXC:AgentScreenDetection 2026-08-19:
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
        CDXC:SessionChat 2026-08-23: commands Ghostex itself typed
        into this session. Inside the running gate with the screen-derived
        fields because a stopped session is not one anything is renaming, and
        on the read path as well as the frames so a followerless mobile client
        sees them too.
        */
        crate::session_chat_app_command::insert_session_chat_app_commands(
            &mut result,
            &project_id,
            &session_id,
        );
        /*
        CDXC:AgentScreenDetection 2026-08-22: same capture, same cache.
        Ordinary activity is live only during the main turn, but Claude's
        background-shell footer deliberately remains live after that turn is
        ready. Omitted means the client clears its progress row.
        */
        if let Some(activity) = detection
            .activity
            .as_ref()
            .filter(|activity| working || activity.remains_live_when_ready())
        {
            result.insert("terminalActivity".to_string(), activity.to_value());
        }
        /*
        CDXC:AgentScreenDetection 2026-08-23: same capture, same cache — but no
        running gate. Sub-agents outlive the turn that spawned them, so
        an idle main agent is exactly when this is worth reading.
        */
        if let Some(fleet) = detection.fleet.as_ref() {
            result.insert("agentFleet".to_string(), fleet.to_value());
        }
        // CDXC:SessionChat 2026-09-03: same detection, no gate of
        // any kind: the store on disk is the list, working or idle.
        if let Some(tasks) = detection.tasks.as_ref() {
            result.insert("agentTasks".to_string(), tasks.to_value());
        }
        /*
        CDXC:AgentScreenDetection 2026-08-22: same capture again. Followerless
        clients need the "detection has run" bit for exactly the reason follower
        clients do — telling a pill that is still loading from one whose agent
        simply never names a model.
        */
        if detection.attempted {
            result.insert("screenProbed".to_string(), json!(true));
        }
    } else {
        /*
        CDXC:AgentScreenDetection 2026-08-22: a session that is not running
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
    CDXC:SessionChat 2026-08-01:
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
                return Ok(crate::session_chat_fork_stitch::SessionChatStitchedPage {
                    page: crate::session_chat::SessionChatTailPage::NotFound,
                    fork_info: None,
                });
            };
            /*
            CDXC:SessionFork 2026-08-28:
            Scroll-back follows a `codex fork` lineage across rollout files. Any
            other agent, and any Codex rollout without a `forked_from_id`, takes
            the untouched single-file read inside this helper.
            */
            crate::session_chat_fork_stitch::read_session_chat_tail_page_stitched(
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

    let read_outcome = match read_outcome {
        Ok(Ok(stitched)) => {
            /*
            CDXC:SessionFork 2026-08-28: lineage of the rollout this
            chat is reading, present only when it was opened by `codex fork`.
            Emitted next to the page so a client can label the boundary rows the
            stitched page carries.
            */
            if let Some(fork_info) = stitched.fork_info.as_ref() {
                result.insert("forkInfo".to_string(), fork_info.to_value());
            }
            Ok(Ok(stitched.page))
        }
        Ok(Err(error)) => Ok(Err(error)),
        Err(error) => Err(error),
    };

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
                    .or(screen_prompt)
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
                .or(screen_prompt)
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
                .or(screen_prompt)
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
