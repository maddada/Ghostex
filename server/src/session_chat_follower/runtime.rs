use super::*;

/// Identity the follower is currently tailing. Seeded from the spawn config and
/// refreshed from the live session so a hook update that did not respawn the
/// task still reaches the resolver.
pub(crate) struct SessionChatFollowerIdentity {
    pub(super) agent_session_id: Option<String>,
    pub(super) agent_session_path: Option<String>,
}

impl SessionChatFollowerIdentity {
    pub(super) fn adopt(&mut self, live: SessionChatLiveState) {
        if live.agent_session_id.is_some() {
            self.agent_session_id = live.agent_session_id;
        }
        if live.agent_session_path.is_some() {
            self.agent_session_path = live.agent_session_path;
        }
    }
}

// ---------------------------------------------------------------------------
// Interactive prompts (upstream chat spec §8.1-§8.3): question/approval cards
// ---------------------------------------------------------------------------

/*
CDXC:SessionChat 2026-07-31:
Session Chat follower registry. Lifecycle mirrors zmx_title_observers (synced
from schedule_presentation_session_delta, boot sync, shutdown stop-all) with
one addition: followers are refcounted by /api/events `subscribeSessionChat`
clients, so a task only tails a transcript while somebody is watching AND the
session is running. Frames go out as plain hub broadcasts tagged with
projectId/sessionId (clients filter); they deliberately do NOT take
lock_presentation_event_sequence because chat epoch/seq is decoupled from the
presentation revision stream.
*/

pub(crate) fn session_chat_agent_for_session(session: &Value) -> Option<String> {
    let agent_id = normalize_agent_name(first_prompt_agent_name(session).as_deref());
    if let Some(agent) = crate::session_chat::session_chat_transcript_agent_id(agent_id.as_deref())
    {
        return Some(agent.to_string());
    }

    let agent_icon = session
        .get("launchSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("icon"))
        .and_then(Value::as_str);
    crate::session_chat::session_chat_transcript_agent_id(agent_icon).map(str::to_string)
}

pub(crate) fn session_chat_identity_fingerprint(session: &Value) -> String {
    format!(
        "{}|{}|{}",
        session_chat_agent_for_session(session).unwrap_or_default(),
        read_runtime_text(session, "agentSessionId").unwrap_or_default(),
        read_runtime_text(session, "agentSessionPath").unwrap_or_default(),
    )
}

pub(crate) fn is_session_chat_followable_session(session: &Value) -> bool {
    read_session_text(session, "lifecycleState").as_deref() == Some("running")
}

/*
CDXC:SessionChat 2026-08-01:
The chat channel's own working truth. Agent hooks are the only source that knows
a turn started before the transcript flushes its first row, so every chat surface
(gpui, web, mobile) reads it here instead of each host wiring its own session
activity prop — desktop had none at all, so its Working marker and Stop button
were dead.

CDXC:SessionStatus 2026-09-04 DECISION:
User: the chat view's working strip and the sidebar's working spinner must
derive from the same source so they always match and never desync. The chat
used to read the raw stored activity while the sidebar read the presentation's
projection (a stale title-derived working closed out, a detected compaction
counted as working), so the two drifted. Both now read `presentation_activity`,
and the stale-activity timer re-syncs the follower through the presentation
delta, so the flip lands in the chat at the same moment it lands in the sidebar.
*/
pub(crate) fn session_chat_hook_working(session: &Value) -> bool {
    let generated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    crate::presentation::presentation_activity(session, &generated_at) == "working"
}

pub(crate) fn sync_session_chat_follower_for_session(
    state: &AppState,
    session: &Value,
    _reason: &str,
) {
    let Some(project_id) = read_session_text(session, "projectId") else {
        return;
    };
    let Some(session_id) = read_session_text(session, "sessionId") else {
        return;
    };
    let key = session_observer_key(&project_id, &session_id);
    let Ok(mut followers) = state.session_chat_followers.lock() else {
        return;
    };
    let Some(entry) = followers.get_mut(&key) else {
        return; // Nobody subscribed — nothing to follow.
    };
    if entry.subscribers == 0 || !is_session_chat_followable_session(session) {
        if let Some(task) = entry.task.take() {
            task.abort();
        }
        return;
    }
    let fingerprint = session_chat_identity_fingerprint(session);
    /*
    CDXC:AgentScreenDetection 2026-08-24:
    `is_finished()` alone only catches a task that RETURNED. A task wedged in an
    inline await (a blocking read against a daemon that never answers, a path
    resolution on a stalled filesystem) stays "unfinished" forever, so this
    healed nothing while chat sat frozen. The task's heartbeat closes that gap:
    running but no reconcile progress within the wedge deadline is dead too, and
    respawning starts a fresh generation + snapshot, so subscribed clients
    recover on the next frame.
    */
    let task_running = entry.task.as_ref().is_some_and(|task| !task.is_finished());
    let wedged = task_running
        && entry
            .heartbeat
            .is_wedged(crate::session_chat::SESSION_CHAT_FOLLOWER_WEDGE_DEADLINE);
    if wedged {
        log_session_chat_follower_wedged(state, &project_id, &session_id);
    }
    let task_alive = task_running && !wedged;
    if task_alive && entry.fingerprint == fingerprint {
        return;
    }
    if let Some(task) = entry.task.take() {
        task.abort();
    }
    entry.fingerprint = fingerprint;
    // Authoritative snapshot/replaced frames re-read the CURRENT stored
    // interactive prompt so a card pending across subscribe/rotation is
    // never dropped by a stale copy.
    let state_reader: crate::session_chat::SessionChatStateReader = {
        let paths = state.paths.clone();
        let server_id = state.metadata.server_id.clone();
        let project_id = project_id.clone();
        let session_id = session_id.clone();
        Arc::new(move || {
            let read = || -> Option<crate::session_chat::SessionChatLiveState> {
                let db = open_gxserver_database(&paths).ok()?;
                let repository = DomainRepository::new(&db, server_id.as_str());
                let session = repository.get_session(&project_id, &session_id).ok()??;
                Some(crate::session_chat::SessionChatLiveState {
                    prompt: crate::agents::session_chat_prompt_setting(&session)
                        .as_deref()
                        .and_then(crate::session_chat::parse_stored_session_chat_prompt),
                    working: session_chat_hook_working(&session),
                    agent_session_id: read_runtime_text(&session, "agentSessionId"),
                    agent_session_path: read_runtime_text(&session, "agentSessionPath"),
                })
            };
            read().unwrap_or_default()
        })
    };
    let agent = session_chat_agent_for_session(session);
    let terminal_agent = crate::session_chat_composer::session_chat_composer_agent_id(session)
        .or_else(|| agent.clone());
    // Detection source for snapshot/replaced frames (cached) and the follower's
    // ~30s probe (refresh). Both run through the shared 5s-TTL cache.
    // CDXC:AgentScreenDetection 2026-08-19: the reader also answers with
    // the session's terminal notice, watchdog-first, so the follower never
    // learns about the watchdog store.
    let options_reader: crate::session_chat_options::SessionChatOptionsReader = {
        let detector = SessionChatOptionDetector::new(state);
        let project_id = project_id.clone();
        let session_id = session_id.clone();
        let terminal_agent = terminal_agent.clone();
        Arc::new(move |mode| {
            let mut detection = match mode {
                crate::session_chat_options::SessionChatOptionsReadMode::Cached => {
                    detector.cached(&project_id, &session_id)
                }
                crate::session_chat_options::SessionChatOptionsReadMode::Refresh => detector
                    .detect_blocking(&project_id, &session_id, terminal_agent.as_deref(), true),
            };
            detection.notice = crate::session_chat_notice::resolve_session_chat_terminal_notice(
                &project_id,
                &session_id,
                detection.notice,
            );
            detection
        })
    };
    /*
    CDXC:SessionIdentity 2026-08-02:
    Registry access the follower needs to re-bind a session whose Claude
    conversation was continued in a new transcript (compaction / background-job
    resume, where no agent hook ever reports the new id): the ids other sessions
    already own, a compare-and-set identity write through the passive path, and
    a log sink. All three run on the follower's blocking pool.
    */
    let successor_hooks = {
        let bound_paths = state.paths.clone();
        let bound_server_id = state.metadata.server_id.clone();
        let bound_project_id = project_id.clone();
        let bound_session_id = session_id.clone();
        let adopt_paths = state.paths.clone();
        let adopt_server_id = state.metadata.server_id.clone();
        let adopt_project_id = project_id.clone();
        let adopt_session_id = session_id.clone();
        let logger = state.logger.clone();
        let log_server_id = state.metadata.server_id.clone();
        let log_project_id = project_id.clone();
        let log_session_id = session_id.clone();
        let child_paths = state.paths.clone();
        let child_server_id = state.metadata.server_id.clone();
        let child_project_id = project_id.clone();
        let child_session_id = session_id.clone();
        crate::session_chat::SessionChatSuccessorHooks {
            pending_fork_child_since_ms: Arc::new(move |scanned_agent_session_id| {
                let read = || -> Option<i64> {
                    let db = open_gxserver_database(&child_paths).ok()?;
                    let repository = DomainRepository::new(&db, child_server_id.as_str());
                    // Forks are created in the parent's project, so the
                    // project list is the whole candidate set.
                    let sessions = repository.list_sessions(Some(&child_project_id)).ok()?;
                    sessions
                        .iter()
                        .filter(|candidate| {
                            read_session_text(candidate, "sessionId").as_deref()
                                != Some(child_session_id.as_str())
                        })
                        .filter(|candidate| {
                            let forked_from = read_runtime_text(candidate, "forkedFromSessionId")
                                .or_else(|| {
                                    candidate
                                        .get("launchSettings")
                                        .and_then(Value::as_object)
                                        .and_then(|settings| settings.get("forkedFromSessionId"))
                                        .and_then(Value::as_str)
                                        .map(str::to_string)
                                });
                            forked_from.as_deref() == Some(child_session_id.as_str())
                        })
                        .filter(|candidate| crate::agents::is_active_identity_owner(candidate))
                        .filter(|candidate| {
                            let claimed = read_runtime_text(candidate, "agentSessionId");
                            claimed.is_none()
                                || claimed.as_deref() == Some(scanned_agent_session_id)
                        })
                        .filter_map(|candidate| {
                            read_session_text(candidate, "createdAt")
                                .and_then(|value| crate::session_status::parse_iso_ms(&value))
                        })
                        .min()
                };
                read()
            }),
            bound_agent_session_ids: Arc::new(move || {
                let read = || -> Option<Vec<String>> {
                    let db = open_gxserver_database(&bound_paths).ok()?;
                    let repository = DomainRepository::new(&db, bound_server_id.as_str());
                    let sessions = repository.list_sessions(None).ok()?;
                    Some(
                        sessions
                            .iter()
                            .filter(|candidate| {
                                read_session_text(candidate, "projectId").as_deref()
                                    != Some(bound_project_id.as_str())
                                    || read_session_text(candidate, "sessionId").as_deref()
                                        != Some(bound_session_id.as_str())
                            })
                            /*
                            CDXC:SessionIdentity 2026-08-02:
                            ONLY sessions that could actually be tailing the id.
                            The registry keeps every session ever created (3487
                            stopped rows on the machine this was debugged on),
                            and stopped rows still carry the agentSessionIds of
                            conversations that have since been continued — the
                            first cut excluded those too, which silently blocked
                            every real adoption.
                            */
                            .filter(|candidate| crate::agents::is_active_identity_owner(candidate))
                            .filter_map(|candidate| read_runtime_text(candidate, "agentSessionId"))
                            .collect(),
                    )
                };
                read().unwrap_or_default()
            }),
            adopt_identity: Arc::new(move |adoption| {
                let write = || -> Option<bool> {
                    let db = open_gxserver_database(&adopt_paths).ok()?;
                    let repository = DomainRepository::new(&db, adopt_server_id.as_str());
                    crate::agents::apply_transcript_successor_session_identity(
                        &repository,
                        &adopt_project_id,
                        &adopt_session_id,
                        adoption.previous_agent_session_id.as_deref(),
                        &adoption.agent_session_id,
                        &adoption.agent_session_path,
                    )
                    .ok()
                };
                write().unwrap_or(false)
            }),
            log: Arc::new(move |notice| {
                let (level, event, details) = match notice {
                    crate::session_chat::SessionChatSuccessorNotice::Adopted(adoption) => (
                        // Warn, not Info: the persisted gxserver log keeps only
                        // warn/error unless Debugging Mode is on, and a session
                        // whose stored identity had drifted off its live
                        // conversation is exactly what a support bundle needs.
                        LogLevel::Warn,
                        "sessionChatSuccessorTranscriptAdopted",
                        json!({
                            "agentSessionId": adoption.agent_session_id,
                            "agentSessionPath": adoption.agent_session_path,
                            "hops": adoption.hops,
                            "lineage": adoption.lineage,
                            "predecessorTranscriptSessionId":
                                adoption.predecessor_transcript_session_id,
                            "previousAgentSessionId": adoption.previous_agent_session_id,
                        }),
                    ),
                    crate::session_chat::SessionChatSuccessorNotice::AdoptionRejected {
                        agent_session_id,
                        reason,
                    } => (
                        LogLevel::Warn,
                        "sessionChatSuccessorTranscriptRejected",
                        json!({ "agentSessionId": agent_session_id, "reason": reason }),
                    ),
                    crate::session_chat::SessionChatSuccessorNotice::Ambiguous {
                        predecessor_session_id,
                        candidate_session_ids,
                    } => (
                        LogLevel::Warn,
                        "sessionChatSuccessorTranscriptAmbiguous",
                        json!({
                            "candidateAgentSessionIds": candidate_session_ids,
                            "predecessorAgentSessionId": predecessor_session_id,
                        }),
                    ),
                    crate::session_chat::SessionChatSuccessorNotice::OwnedByAnotherSession {
                        predecessor_session_id,
                        candidate_session_ids,
                    } => (
                        LogLevel::Warn,
                        "sessionChatSuccessorTranscriptOwned",
                        json!({
                            "candidateAgentSessionIds": candidate_session_ids,
                            "predecessorAgentSessionId": predecessor_session_id,
                        }),
                    ),
                };
                let mut details = details;
                if let Some(object) = details.as_object_mut() {
                    object.insert("projectId".to_string(), json!(log_project_id));
                    object.insert("sessionId".to_string(), json!(log_session_id));
                }
                let _ = logger.log(GxserverLogInput {
                    level,
                    event: event.to_string(),
                    server_id: Some(log_server_id.clone()),
                    request_id: None,
                    client: None,
                    duration_ms: None,
                    error: None,
                    details: Some(details),
                });
            }),
        }
    };
    /*
    CDXC:SessionChat 2026-08-21:
    Snapshot / replaced / state frames carry the session's prompt queue and
    synced draft, so a client that subscribes mid-session sees the same rows the
    device that queued them sees. Read lazily inside the frame builders, never
    on the reconcile tick.
    */
    let queue_reader: crate::session_chat::SessionChatQueueReader = {
        let paths = state.paths.clone();
        let project_id = project_id.clone();
        let session_id = session_id.clone();
        Arc::new(move || {
            crate::session_chat_queue::read_session_chat_queue_snapshot(
                &paths,
                &project_id,
                &session_id,
            )
        })
    };
    let notice_publisher = crate::session_chat_options::session_chat_terminal_notice_publisher(
        state,
        &project_id,
        &session_id,
    );
    // CDXC:AgentScreenDetection 2026-09-03 WHY: only Claude has a statusline payload
    // to watch; a cheap stat per tick, no capture until it actually changes.
    let options_change_watch =
        (crate::session_chat_options::session_chat_option_agent(terminal_agent.as_deref())
            == Some(crate::session_chat_options::SessionChatOptionAgent::Claude))
        .then(|| {
            crate::session_chat_options::claude_statusline_change_watch(
                crate::session_chat_options::session_chat_hook_state_directory(&state.paths),
            )
        });
    let config = crate::session_chat::SessionChatFollowerConfig {
        project_id,
        session_id,
        agent,
        agent_session_id: read_runtime_text(session, "agentSessionId"),
        agent_session_path: read_runtime_text(session, "agentSessionPath"),
        limit: entry.limit,
        protocol_version: GXSERVER_PROTOCOL_VERSION,
        server_id: state.metadata.server_id.clone(),
        state_reader: Some(state_reader),
        options_reader: Some(options_reader),
        options_change_watch,
        queue_reader: Some(queue_reader),
        successor_hooks: Some(successor_hooks),
        notice_publisher: Some(notice_publisher),
        tuning: crate::session_chat::SessionChatFollowerTuning::default(),
    };
    let event_hub = state.event_hub.clone();
    let emit: crate::session_chat::SessionChatFrameEmitter =
        Arc::new(move |event| event_hub.broadcast(event));
    // CDXC:AgentScreenDetection 2026-08-24: a fresh heartbeat per task, so
    // the aborted one's last stamp can never be read as the new task's progress.
    let heartbeat = Arc::new(crate::session_chat::SessionChatFollowerHeartbeat::new());
    entry.heartbeat = heartbeat.clone();
    entry.task = Some(tokio::spawn(
        crate::session_chat::run_session_chat_follower(
            config,
            entry.stream.clone(),
            entry.resnapshot.clone(),
            heartbeat,
            emit,
        ),
    ));
}

fn log_session_chat_follower_wedged(state: &AppState, project_id: &str, session_id: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionChatFollowerWedged".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({
            "projectId": project_id,
            "sessionId": session_id,
        })),
    });
}

pub(crate) fn sync_session_chat_followers_for_all_sessions(state: &AppState, reason: &str) {
    let subscribed_keys: Vec<String> = {
        let Ok(followers) = state.session_chat_followers.lock() else {
            return;
        };
        followers
            .iter()
            .filter(|(_, entry)| entry.subscribers > 0)
            .map(|(key, _)| key.clone())
            .collect()
    };
    if subscribed_keys.is_empty() {
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    /*
    CDXC:StateSync 2026-09-01:
    Only the handful of SUBSCRIBED sessions matter here, so each one is looked
    up by its primary key instead of hydrating the whole registry to throw all
    but those rows away. A key that does not resolve to a row is exactly the
    "vanished" case the sweep already handled by not marking it seen.
    */
    let mut seen_keys: HashSet<String> = HashSet::new();
    for key in &subscribed_keys {
        let Some((project_id, session_id)) = key.split_once('/') else {
            continue;
        };
        match repository.get_session(project_id, session_id) {
            Ok(Some(session)) => {
                seen_keys.insert(key.clone());
                sync_session_chat_follower_for_session(state, &session, reason);
            }
            Ok(None) => {}
            /*
            CDXC:AgentScreenDetection 2026-08-24:
            Only `Ok(None)` means the session is gone. A read error is the
            database being busy, so the row counts as seen and its follower
            keeps running — the whole-list version could not tear anything
            down on a read error either.
            */
            Err(_) => {
                seen_keys.insert(key.clone());
            }
        }
    }
    // Subscribed sessions that vanished: stop their tasks. The refcounted
    // entry itself lives until the subscribers unsubscribe or disconnect.
    if let Ok(mut followers) = state.session_chat_followers.lock() {
        for key in subscribed_keys {
            if !seen_keys.contains(&key) {
                if let Some(entry) = followers.get_mut(&key) {
                    if let Some(task) = entry.task.take() {
                        task.abort();
                    }
                }
            }
        }
    }
}

pub(crate) fn stop_session_chat_follower(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    _reason: &str,
) {
    if let Ok(mut followers) = state.session_chat_followers.lock() {
        if let Some(entry) = followers.get_mut(&session_observer_key(project_id, session_id)) {
            if let Some(task) = entry.task.take() {
                task.abort();
            }
        }
    }
    // A killed/slept session's statusline is gone; a stale detection must not
    // outlive it.
    forget_session_chat_options(state, project_id, session_id);
}

pub(crate) fn stop_all_session_chat_followers(state: &AppState) {
    if let Ok(mut followers) = state.session_chat_followers.lock() {
        for (_, mut entry) in followers.drain() {
            if let Some(task) = entry.task.take() {
                task.abort();
            }
        }
    }
    if let Ok(mut cache) = state.session_chat_option_cache.lock() {
        cache.clear();
    }
}

/// Asks a live follower to answer its subscribers with a fresh authoritative
/// snapshot (new generation), exactly as a new subscriber would. No-op when
/// nobody is following the session.
///
/// CDXC:SessionChat 2026-09-02: the rewind driver calls this after
/// recording a pending rewind, so every open chat view drops the rewound rows
/// at once instead of waiting for the next transcript append.
pub(crate) fn request_session_chat_resnapshot(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) {
    let Ok(followers) = state.session_chat_followers.lock() else {
        return;
    };
    if let Some(entry) = followers.get(&session_observer_key(project_id, session_id)) {
        if entry.task.as_ref().is_some_and(|task| !task.is_finished()) {
            entry.resnapshot.notify_one();
        }
    }
}

pub(crate) fn subscribe_session_chat_follower(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    limit: usize,
    new_subscriber: bool,
) {
    {
        let Ok(mut followers) = state.session_chat_followers.lock() else {
            return;
        };
        let entry = followers
            .entry(session_observer_key(project_id, session_id))
            .or_insert_with(|| SessionChatFollowerEntry {
                subscribers: 0,
                fingerprint: String::new(),
                limit,
                task: None,
                stream: Arc::new(crate::session_chat::SessionChatStream::new()),
                resnapshot: Arc::new(tokio::sync::Notify::new()),
                heartbeat: Arc::new(crate::session_chat::SessionChatFollowerHeartbeat::new()),
            });
        if new_subscriber {
            entry.subscribers += 1;
        }
        /*
        The window only ever GROWS. Snapshot/replaced frames carry the
        follower's tail window, so a client that already displays 900 rows and
        re-subscribes (reconnect, second host on the same session) must not be
        answered with the 300-row default — that visibly shrinks the list.
        A running follower holds its limit in its spawn config, so a raise
        takes effect by dropping the task: the sync below respawns it, which
        starts a fresh generation and emits the wider snapshot.
        */
        let raised = limit > entry.limit;
        entry.limit = entry.limit.max(limit);
        let task_alive = entry.task.as_ref().is_some_and(|task| !task.is_finished());
        if raised && task_alive {
            if let Some(task) = entry.task.take() {
                task.abort();
            }
        } else if task_alive {
            // Every subscribe is answered with an authoritative snapshot: a
            // live follower re-reads the tail in a fresh generation.
            entry.resnapshot.notify_one();
        }
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    if let Ok(Some(session)) = repository.get_session(project_id, session_id) {
        sync_session_chat_follower_for_session(state, &session, "session-chat-subscribe");
    }
}

pub(crate) fn unsubscribe_session_chat_follower(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) {
    let Ok(mut followers) = state.session_chat_followers.lock() else {
        return;
    };
    let key = session_observer_key(project_id, session_id);
    let Some(entry) = followers.get_mut(&key) else {
        return;
    };
    entry.subscribers = entry.subscribers.saturating_sub(1);
    if entry.subscribers == 0 {
        if let Some(task) = entry.task.take() {
            task.abort();
        }
        followers.remove(&key);
    }
}
