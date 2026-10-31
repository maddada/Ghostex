use super::*;

/*
CDXC:AgentScreenDetection 2026-08-19:
The notice a session should be showing RIGHT NOW, with no detection of its own:
the last classification the shared 5s cache holds, overridden by a watchdog
notice when one is pending. Every path that must stay spawn-free — the 500ms
long-poll fingerprint, prompt-driven state frames — reads it through here.
*/
/*
CDXC:AgentScreenDetection 2026-08-22:
The whole screen-derived half of a session's state, owned, from the shared 5s
cache and the watchdog store. Every spawn-free publisher reads it through here
so the notice and the progress row are always taken from the SAME cache read
and can never be published one frame apart.
*/
#[derive(Default)]
pub(crate) struct CachedSessionChatScreenState {
    pub(crate) prompt: Option<crate::session_chat::SessionChatInteractivePrompt>,
    pub(crate) notice: Option<crate::session_chat_notice::SessionChatTerminalNotice>,
    pub(crate) activity: Option<crate::session_chat_terminal_activity::SessionChatTerminalActivity>,
    pub(crate) fleet: Option<crate::session_chat_agent_fleet::SessionChatAgentFleet>,
    pub(crate) tasks: Option<crate::session_chat_agent_tasks::SessionChatAgentTasks>,
    pub(crate) workflows: Option<crate::session_chat_claude_workflow_cards::SessionChatWorkflows>,
    /// CDXC:AgentScreenDetection 2026-08-22: whether the cache entry these
    /// came from was backed by a whole capture at all.
    pub(crate) probed: bool,
}

impl CachedSessionChatScreenState {
    pub(crate) fn borrow(&self) -> crate::session_chat::SessionChatScreenState<'_> {
        crate::session_chat::SessionChatScreenState {
            prompt: self.prompt.as_ref(),
            notice: self.notice.as_ref(),
            activity: self.activity.as_ref(),
            fleet: self.fleet.as_ref(),
            tasks: self.tasks.as_ref(),
            workflows: self.workflows.as_ref(),
            probed: self.probed,
        }
    }
}

pub(crate) fn cached_session_chat_screen_state(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> CachedSessionChatScreenState {
    let (prompt, screen_notice, activity, fleet, tasks, workflows, probed) = state
        .session_chat_option_cache
        .lock()
        .ok()
        .and_then(|cache| {
            cache
                .get(&session_observer_key(project_id, session_id))
                .map(|entry| {
                    (
                        entry.value.prompt.clone(),
                        entry.value.notice.clone(),
                        entry.value.activity.clone(),
                        entry.value.fleet.clone(),
                        entry.value.tasks.clone(),
                        entry.value.workflows.clone(),
                        entry.value.attempted,
                    )
                })
        })
        .unwrap_or_default();
    CachedSessionChatScreenState {
        prompt,
        notice: crate::session_chat_notice::resolve_session_chat_terminal_notice(
            project_id,
            session_id,
            screen_notice,
        ),
        activity,
        fleet,
        tasks,
        workflows,
        probed,
    }
}

/*
CDXC:SessionChat 2026-08-26:
Last known composer verdict with no process spawn, for the prompt-queue
scheduler. A tick must never trigger a capture (that is the rule the notice
reader next to this one exists to keep), so a session nobody has probed reads
`Unknown` and the queue proceeds exactly as it did before this feature.
*/
pub(crate) fn cached_session_chat_composer_readiness(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> crate::session_chat_composer::SessionChatComposerReadiness {
    state
        .session_chat_option_cache
        .lock()
        .ok()
        .and_then(|cache| {
            cache
                .get(&session_observer_key(project_id, session_id))
                .map(|entry| entry.value.composer.clone())
        })
        .unwrap_or_default()
}

pub(crate) fn cached_session_chat_terminal_notice(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Option<crate::session_chat_notice::SessionChatTerminalNotice> {
    let screen = state
        .session_chat_option_cache
        .lock()
        .ok()
        .and_then(|cache| {
            cache
                .get(&session_observer_key(project_id, session_id))
                .and_then(|entry| entry.value.notice.clone())
        });
    crate::session_chat_notice::resolve_session_chat_terminal_notice(project_id, session_id, screen)
}

/*
CDXC:AgentScreenDetection 2026-08-19:
The send watchdog owns no frames and no database: it mutates the watchdog notice
store and then calls this, which republishes whatever the session should be
showing now — the cached model/effort pills included, so a notice frame can
never blank them.
*/
pub(crate) fn session_chat_terminal_notice_publisher(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> crate::session_chat_watchdog::SessionChatWatchdogPublisher {
    let followers = state.session_chat_followers.clone();
    let event_hub = state.event_hub.clone();
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let option_cache = state.session_chat_option_cache.clone();
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    Arc::new(move || {
        publish_cached_session_chat_screen_state(
            &followers,
            &event_hub,
            &paths,
            &server_id,
            &option_cache,
            &project_id,
            &session_id,
        );
    })
}

/// The publisher's body, callable without an `AppState`: read the cached
/// screen state and emit it to the session's followers.
pub(super) fn publish_cached_session_chat_screen_state(
    followers: &Arc<Mutex<HashMap<String, SessionChatFollowerEntry>>>,
    event_hub: &GxserverEventHub,
    paths: &GxserverPaths,
    server_id: &str,
    option_cache: &Arc<Mutex<HashMap<String, SessionChatOptionCacheEntry>>>,
    project_id: &str,
    session_id: &str,
) {
    let key = session_observer_key(project_id, session_id);
    let (options, prompt, screen_notice, activity, fleet, tasks, workflows, captured) =
        option_cache
            .lock()
            .ok()
            .and_then(|cache| {
                cache.get(&key).map(|entry| {
                    (
                        entry.value.options.clone(),
                        entry.value.prompt.clone(),
                        entry.value.notice.clone(),
                        entry.value.activity.clone(),
                        entry.value.fleet.clone(),
                        entry.value.tasks.clone(),
                        entry.value.workflows.clone(),
                        entry.value.attempted,
                    )
                })
            })
            .unwrap_or_default();
    let notice = crate::session_chat_notice::resolve_session_chat_terminal_notice(
        project_id,
        session_id,
        screen_notice,
    );
    emit_session_chat_options_state_frame(
        followers,
        event_hub,
        paths,
        server_id,
        project_id,
        session_id,
        options.as_ref(),
        crate::session_chat::SessionChatScreenState {
            prompt: prompt.as_ref(),
            notice: notice.as_ref(),
            activity: activity.as_ref(),
            fleet: fleet.as_ref(),
            tasks: tasks.as_ref(),
            workflows: workflows.as_ref(),
            probed: captured,
        },
    );
}

/// Fresh lifecycle/working truth for the watchdog's timeout decision. Blocking
/// (SQLite), so the watchdog calls it from a blocking task.
pub(crate) fn session_chat_watchdog_state_reader(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> crate::session_chat_watchdog::SessionChatWatchdogStateReader {
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    Arc::new(move || {
        let read = || -> Option<crate::session_chat_watchdog::SessionChatWatchdogLiveState> {
            let db = open_gxserver_database(&paths).ok()?;
            let repository = DomainRepository::new(&db, server_id.as_str());
            let session = repository.get_session(&project_id, &session_id).ok()??;
            Some(crate::session_chat_watchdog::SessionChatWatchdogLiveState {
                running: is_session_chat_followable_session(&session),
                working: session_chat_hook_working(&session),
            })
        };
        read().unwrap_or_default()
    })
}

/// Drops EVERY reading taken off this session's screen, not just the option
/// pills: one cache entry holds the detected options, the terminal notice, the
/// terminal activity, the fleet state and the composer-readiness verdict, and
/// they are all readings of the same capture, so they all go stale together.
/// Callers that need to invalidate only the readiness (there are none — a
/// screen whose composer verdict is worthless has worthless pills too) would be
/// splitting an entry that is only ever written whole.
pub(crate) fn forget_session_chat_options(state: &AppState, project_id: &str, session_id: &str) {
    if let Ok(mut cache) = state.session_chat_option_cache.lock() {
        cache.remove(&session_observer_key(project_id, session_id));
    }
    // Sleeping/stopped sessions have no live screen. Clear the durable
    // compaction projection with the cache so waking cannot resurrect an old
    // working status from a run that ended while the provider was stopped.
    crate::session_chat_compacting::SessionChatCompactingPublisher::new(state)
        .publish(project_id, session_id, None);
    crate::session_chat_compacting::SessionChatCompactingPublisher::new(state)
        .publish_fleet(project_id, session_id, None);
    crate::session_chat_compacting::SessionChatCompactingPublisher::new(state)
        .publish_monitor(project_id, session_id, None);
    crate::session_chat_mode_availability::forget_for_agent_restart(
        &crate::session_chat_compacting::SessionChatCompactingPublisher::new(state),
        project_id,
        session_id,
    );
}
