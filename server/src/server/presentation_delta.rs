use super::*;
use crate::session_chat_follower::{
    stop_session_chat_follower, sync_session_chat_follower_for_session,
};

pub(crate) fn schedule_presentation_project_delta(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    delta_type: &str,
) -> std::result::Result<(), DomainStateError> {
    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29-00:00 (warm widened in the P5 fix
    round):
    A project should reach Sidebar V2 already carrying its `origin` remote, so
    the very delta that announces it lands in the right cross-machine group
    instead of moving there up to a minute later. That is true of a freshly
    registered project AND of one that returns to presentation after being
    parked, whose cache entry the refresh pass evicted while it was away — hence
    the gate is "presentation publishes this project", not "delta_type is
    projectAdded" (see `ensure_published_project_git_remote_probed`).

    This still warms only the FIRST sighting of a path, and it runs outside the
    presentation sequencer below, so no other producer waits on a git spawn;
    every later delta for the same project is a pure cache read.
    */
    /*
    CDXC:ProjectRemoteCopy 2026-08-26:
    Both sidebar versions expose the project's Git origin now: V2 uses it for
    repository grouping and the classic sidebar offers Copy Remote URL. Warm it
    before publishing either version so the menu does not appear a pass later.
    */
    if let Some(project) = repository.get_project(project_id)? {
        project_git_remote::ensure_published_project_git_remote_probed(&project, true);
        /*
        CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
        The project's own icon rides the same first-sighting warm, for the same
        reason and under the same gate: a project should reach the sidebar with
        its repository's icon already on it instead of showing a folder glyph
        until the next background pass. Same bounded cost — the warm reads the
        candidate list once per NEW family root and never again.
        */
        project_icon::ensure_published_project_icon_probed(&project);
    }
    let _event_sequence = lock_presentation_event_sequence(state)?;
    let delta = build_presentation_project_delta(repository, project_id, delta_type)?;
    let revision = increment_presentation_revision(db)?;
    state.event_hub.broadcast(json!({
        "delta": delta,
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "revision": revision,
        "serverId": state.metadata.server_id.clone(),
        "type": "presentationDelta",
    }));
    Ok(())
}

pub(crate) fn schedule_presentation_session_delta(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> std::result::Result<(), DomainStateError> {
    {
        let _event_sequence = lock_presentation_event_sequence(state)?;
        let delta = build_presentation_session_delta(db, repository, project_id, session_id)?;
        let revision = increment_presentation_revision(db)?;
        state.event_hub.broadcast(json!({
            "delta": delta,
            "protocolVersion": GXSERVER_PROTOCOL_VERSION,
            "revision": revision,
            "serverId": state.metadata.server_id.clone(),
            "type": "presentationDelta",
        }));
    }
    match repository.get_session(project_id, session_id) {
        Ok(Some(session)) => {
            sync_zmx_title_observer_for_session(state, &session, "presentation-session-delta");
            sync_session_chat_follower_for_session(state, &session, "presentation-session-delta");
        }
        Ok(None) => {
            stop_zmx_title_observer(state, project_id, session_id, "session-removed");
            stop_session_chat_follower(state, project_id, session_id, "session-removed");
        }
        /*
        CDXC:SessionChatFollowerLiveness 2026-08-24:
        Only `Ok(None)` means the session is gone. A read error is the database
        being busy, not a removal, and tearing the observers down for it killed
        a healthy chat follower whose respawn then had to wait for some later
        delta to arrive. Leave them running: the session state is unchanged.
        */
        Err(_) => {}
    }
    Ok(())
}

pub(crate) fn lock_presentation_event_sequence(
    state: &AppState,
) -> std::result::Result<std::sync::MutexGuard<'_, ()>, DomainStateError> {
    /*
    Every producer shares this short critical section for state projection,
    revision allocation, and event publication. That makes revision order and
    broadcast order identical, including compensating removal/repair deltas,
    while process work and durable lifecycle mutations remain outside it.
    */
    state
        .presentation_event_sequence
        .lock()
        .map_err(|_| DomainStateError::corrupt_state("Presentation event sequencer is poisoned."))
}

/*
CDXC:SidebarSpaces 2026-08-27:
Every collections mutation can invalidate the Space document: grouping a
project strips that project's direct Space memberships, and a collection that
just emptied stops existing, so every Space referencing it must let it go. The
collections routes call this immediately after their own broadcast, still
holding the event sequencer, so the repair is published in the same ordered run
rather than waiting for the next client write. The Space broadcast takes its own
revision so revision order stays identical to broadcast order.
*/
pub(crate) fn broadcast_pruned_sidebar_spaces(
    state: &AppState,
    db: &rusqlite::Connection,
    collections: &Value,
) -> std::result::Result<(), DomainStateError> {
    let Some(spaces) = prune_sidebar_spaces_for_collections(db, collections)? else {
        return Ok(());
    };
    let revision = increment_presentation_revision(db)?;
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "revision": revision,
        "serverId": state.metadata.server_id.clone(),
        "sidebarSpaces": spaces,
        "type": "sidebarSpacesChanged",
    }));
    Ok(())
}

pub(crate) fn read_presentation_snapshot_in_sequence(
    state: &AppState,
    db: &rusqlite::Connection,
    server_id: &str,
) -> std::result::Result<Value, DomainStateError> {
    /*
    Snapshot callers own the sequencer exactly once, after any repair pass
    that may publish deltas. Hold it across every projection read, including
    the final revision read inside read_presentation_snapshot, so a producer
    cannot label stale rows with the revision for a delta it just published.
    */
    let auto_settle_after_days = session_lifecycle::read_sweep_auto_settle_after_days(&state.paths);
    let sidebar_v2_selected = session_lifecycle::read_sidebar_v2_selected(&state.paths);
    let _event_sequence = lock_presentation_event_sequence(state)?;
    read_presentation_snapshot(db, server_id, auto_settle_after_days, sidebar_v2_selected)
}

pub(crate) fn schedule_stale_activity_presentation_refresh(
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
    let key = format!("{project_id}/{session_id}");
    let delay_ms = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"))
        .and_then(|activity| {
            agent_activity_presentation_refresh_delay_ms(Some(activity), now_ms())
        });
    let Ok(mut timers) = state.stale_activity_timers.lock() else {
        return;
    };
    if let Some(handle) = timers.remove(&key) {
        handle.abort();
    }
    let Some(delay_ms) = delay_ms else {
        return;
    };
    /*
    CDXC:SessionStatus 2026-06-21-19:26:
    Rust event streams must match TypeScript gxserver's stale title-derived working refresh. zmx may not emit another terminal-title event after a spinner freezes, so schedule one presentation delta at the projection boundary without rewriting durable activity state or re-scheduling from the timer callback.

    CDXC:ActivitySuppressionPolicy 2026-07-29-12:00:
    The same timer also fires when an ongoing working stint crosses the
    meaningful-activity threshold, so recency sorting can promote a genuinely
    busy session even when no further hook or title event arrives; the
    combined deadline comes from agent_activity_presentation_refresh_delay_ms.
    */
    let state = state.clone();
    let timer_key = key.clone();
    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(delay_ms.max(0) as u64 + 25)).await;
        if let Ok(mut timers) = state.stale_activity_timers.lock() {
            timers.remove(&timer_key);
        }
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let _ =
            schedule_presentation_session_delta(&state, &db, &repository, &project_id, &session_id);
    });
    timers.insert(key, handle);
}

pub(crate) fn sync_zmx_title_observers_for_all_sessions(state: &AppState, reason: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(sessions) = repository.list_sessions(None) else {
        return;
    };
    let mut desired_keys = HashSet::new();
    for session in sessions {
        if let (Some(project_id), Some(session_id)) = (
            read_session_text(&session, "projectId"),
            read_session_text(&session, "sessionId"),
        ) {
            if is_zmx_title_observable_session(&session) {
                desired_keys.insert(session_observer_key(&project_id, &session_id));
            }
        }
        sync_zmx_title_observer_for_session(state, &session, reason);
    }
    let existing_keys = state
        .zmx_title_observers
        .lock()
        .map(|observers| observers.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    for key in existing_keys {
        if !desired_keys.contains(&key) {
            stop_zmx_title_observer_by_key(state, &key, "session-no-longer-observable");
        }
    }
}

pub(crate) fn sync_zmx_title_observer_for_session(state: &AppState, session: &Value, reason: &str) {
    let Some(project_id) = read_session_text(session, "projectId") else {
        return;
    };
    let Some(session_id) = read_session_text(session, "sessionId") else {
        return;
    };
    if !is_zmx_title_observable_session(session) {
        stop_zmx_title_observer(state, &project_id, &session_id, reason);
        return;
    }
    let Some(zmx_name) = read_session_text(session, "zmxName") else {
        return;
    };
    let key = session_observer_key(&project_id, &session_id);
    let Ok(mut observers) = state.zmx_title_observers.lock() else {
        return;
    };
    if let Some(existing) = observers.get(&key) {
        if existing.zmx_name == zmx_name && !existing.handle.is_finished() {
            return;
        }
    }
    if let Some(existing) = observers.remove(&key) {
        existing.handle.abort();
    }
    /*
    CDXC:ZmxTitleObservations 2026-06-21-22:23:
    Rust gxserver must own the same zmx title-observation loop as TypeScript gxserver. Native deliberately avoids forwarding every zmx title frame, so without `zmx watch-title` the Rust daemon can keep Codex sessions stuck in working after hooks miss Stop or title-derived spinner state needs to expire.
    */
    let state = state.clone();
    let handle = tokio::spawn(run_zmx_title_observer(
        state,
        project_id,
        session_id,
        zmx_name.clone(),
    ));
    observers.insert(key, ZmxTitleObserverTask { handle, zmx_name });
}

pub(crate) fn stop_zmx_title_observer(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    reason: &str,
) {
    stop_zmx_title_observer_by_key(state, &session_observer_key(project_id, session_id), reason);
}

pub(crate) fn stop_zmx_title_observer_by_key(state: &AppState, key: &str, _reason: &str) {
    if let Ok(mut observers) = state.zmx_title_observers.lock() {
        if let Some(existing) = observers.remove(key) {
            existing.handle.abort();
        }
    }
}

pub(crate) fn stop_all_zmx_title_observers(state: &AppState) {
    if let Ok(mut observers) = state.zmx_title_observers.lock() {
        for (_, existing) in observers.drain() {
            existing.handle.abort();
        }
    }
}

pub(crate) const ZMX_TITLE_OBSERVER_HEALTHY_WATCH_DURATION: Duration = Duration::from_secs(5);

pub(crate) async fn run_zmx_title_observer(
    state: AppState,
    project_id: String,
    session_id: String,
    zmx_name: String,
) {
    let mut failure_count = 0usize;
    loop {
        let zmx = match require_bundled_zmx() {
            Ok(zmx) => zmx,
            Err(_) => {
                failure_count += 1;
                delay_zmx_title_observer_retry(failure_count).await;
                continue;
            }
        };
        let mut child = match Command::new(&zmx.executable_path)
            .args(["watch-title", zmx_name.as_str()])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(_) => {
                failure_count += 1;
                delay_zmx_title_observer_retry(failure_count).await;
                continue;
            }
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };
        /*
        watch-title spawns successfully even when the zmx session no longer
        exists, then exits immediately with no output. A spawn therefore does
        not prove a healthy watch: reset the retry ladder only after the watch
        produced output or stayed alive for a while, so dead sessions back off
        instead of respawning several times per second forever.
        */
        let watch_started_at = Instant::now();
        let mut observed_output = false;
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    observed_output = true;
                    if let Some(title) = parse_zmx_title_line(&line) {
                        let should_probe_identity =
                            ingest_zmx_title_observation(&state, &project_id, &session_id, &title);
                        if should_probe_identity {
                            /*
                            CDXC:GxserverSessionIdentity 2026-08-16:
                            A terminal-button row starts without agent identity.
                            Supported CLIs announce themselves through the zmx
                            title stream that gxserver already watches; use that
                            event to run one targeted foreground-process repair
                            off the async worker. This avoids a permanent `ps`
                            poll while still publishing the canonical identity
                            delta as soon as Codex (or another recognized CLI)
                            starts in an existing terminal.
                            */
                            let probe_state = state.clone();
                            let probe_project_id = project_id.clone();
                            let probe_session_id = session_id.clone();
                            let _ = tokio::task::spawn_blocking(move || {
                                sync_title_signaled_zmx_process_identity(
                                    &probe_state,
                                    &probe_project_id,
                                    &probe_session_id,
                                )
                            })
                            .await;
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        let _ = child.wait().await;
        if observed_output
            || watch_started_at.elapsed() >= ZMX_TITLE_OBSERVER_HEALTHY_WATCH_DURATION
        {
            failure_count = 0;
        }
        failure_count += 1;
        delay_zmx_title_observer_retry(failure_count).await;
    }
}

pub(crate) async fn delay_zmx_title_observer_retry(failure_count: usize) {
    /*
    The observer owns its session until sync stops it, so it must never exit
    on failures: a finished task handle makes every presentation delta respawn
    a fresh fast-retry cycle for the same dead session. Cap the backoff and
    keep waiting instead.
    */
    const DELAYS_MS: [u64; 6] = [250, 500, 1_000, 2_000, 5_000, 60_000];
    let delay_index = failure_count.saturating_sub(1).min(DELAYS_MS.len() - 1);
    tokio::time::sleep(Duration::from_millis(DELAYS_MS[delay_index])).await;
}

pub(crate) fn ingest_zmx_title_observation(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    title: &str,
) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let should_probe_identity = terminal_title_indicates_agent_identity(title)
        && repository
            .get_session(project_id, session_id)
            .ok()
            .flatten()
            .as_ref()
            .is_some_and(should_probe_title_signaled_zmx_process_identity);
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("rawTitle".to_string(), json!(title));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("sessionPersistenceProvider".to_string(), json!("zmx"));
    let Ok(output) = dispatch_agent_endpoint(
        &repository,
        &db,
        &state.paths.home_dir,
        "/api/ingestTerminalTitleEvent",
        &params,
        None,
    ) else {
        return false;
    };
    if let Some((project_id, session_id)) = output.presentation_session {
        let _ =
            schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id);
        if let Ok(Some(session)) = repository.get_session(&project_id, &session_id) {
            schedule_stale_activity_presentation_refresh(
                state,
                &session,
                "terminal-title-stale-activity",
            );
        }
    }
    should_probe_identity
}

pub(crate) fn is_zmx_title_observable_session(session: &Value) -> bool {
    let kind = read_session_text(session, "kind");
    let lifecycle = read_session_text(session, "lifecycleState");
    let provider_state = session
        .get("providerState")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let provider_lifecycle = provider_state
        .get("lifecycleState")
        .and_then(Value::as_str)
        .map(str::trim);
    let provider_text = provider_state
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| read_runtime_text(session, "sessionPersistenceProvider"));
    matches!(kind.as_deref(), Some("terminal" | "agent"))
        && lifecycle.as_deref() == Some("running")
        && provider_lifecycle == Some("exists")
        && matches!(provider_text.as_deref(), None | Some("zmx"))
        && read_session_text(session, "zmxName").is_some()
}

pub(crate) fn parse_zmx_title_line(line: &str) -> Option<String> {
    serde_json::from_str::<Value>(line).ok().and_then(|value| {
        value
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
    })
}

pub(crate) fn session_observer_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}/{session_id}")
}
