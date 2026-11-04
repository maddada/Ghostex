use super::*;

/*
CDXC:SessionChat 2026-08-21:
The delivery handle the queue module (and the scheduler in
session_chat_queue_runtime/scheduler.rs) holds. It closes over the daemon state so those
modules never learn about AppState, zmx names, or the send watchdog, and every
queued prompt still travels the same internals /api/sendSessionChatMessage uses.
*/
pub(crate) fn session_chat_queue_sender(
    state: &Arc<AppState>,
    project_id: &str,
    session_id: &str,
    source: SessionChatMessageSource,
) -> crate::session_chat_queue::SessionChatQueueSender {
    let state = state.clone();
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    Arc::new(move |text: String, images: Vec<String>| {
        let state = state.clone();
        let project_id = project_id.clone();
        let session_id = session_id.clone();
        Box::pin(async move {
            send_session_chat_message_internal(
                &state,
                &project_id,
                &session_id,
                &text,
                &images,
                source,
            )
            .await
            .map(|_| ())
        })
    })
}

/// Per-session sender factory for the scheduler, which delivers for whichever
/// session becomes ready rather than one it was built for.
pub(crate) fn session_chat_queue_sender_factory(
    state: &Arc<AppState>,
) -> crate::session_chat_queue::SessionChatQueueSenderFactory {
    let state = state.clone();
    Arc::new(move |project_id: &str, session_id: &str| {
        session_chat_queue_sender(
            &state,
            project_id,
            session_id,
            SessionChatMessageSource::AutomaticQueue,
        )
    })
}

/// Per-session state-frame publisher for the scheduler, so a row it delivers or
/// fails reaches the same clients an endpoint mutation would.
pub(crate) fn session_chat_queue_publisher_factory(
    state: &Arc<AppState>,
) -> crate::session_chat_queue::SessionChatQueuePublisherFactory {
    let state = state.clone();
    Arc::new(move |project_id: &str, session_id: &str| {
        let state = state.clone();
        let project_id = project_id.to_string();
        let session_id = session_id.to_string();
        let publisher: crate::session_chat_queue::SessionChatQueuePublisher =
            Arc::new(move || broadcast_session_chat_queue_state(&state, &project_id, &session_id));
        publisher
    })
}

/*
CDXC:SessionChat 2026-08-21:
The scheduler's view of "is this terminal able to take a prompt at all". It
reads the SAME resolved notice the chat card shows — the cached screen
classification merged with the send watchdog's verdict — and never triggers a
detection itself, so a tick can never spawn a `zmx history` capture.
*/
pub(crate) fn session_chat_queue_notice_reader(
    state: &Arc<AppState>,
) -> crate::session_chat_queue_runtime::SessionChatQueueNoticeReader {
    let state = state.clone();
    Arc::new(move |project_id: &str, session_id: &str| {
        cached_session_chat_terminal_notice(&state, project_id, session_id)
    })
}

/// CDXC:SessionChat 2026-08-26: the composer half of the same
/// view, under the same no-spawn rule.
pub(crate) fn session_chat_queue_composer_reader(
    state: &Arc<AppState>,
) -> crate::session_chat_queue_runtime::SessionChatQueueComposerReader {
    let state = state.clone();
    Arc::new(move |project_id: &str, session_id: &str| {
        cached_session_chat_composer_readiness(&state, project_id, session_id)
    })
}

pub(crate) fn session_chat_queue_waker(
    state: &Arc<AppState>,
) -> crate::session_chat_queue_runtime::SessionChatQueueWaker {
    let state = state.clone();
    Arc::new(move |project_id: &str, session_id: &str, prompt_id: &str| {
        crate::session_chat_send_wake::wake_session_holding_startup_send(
            &state,
            project_id,
            session_id,
            Some(prompt_id.to_string()),
        );
    })
}

pub(crate) fn session_chat_queue_compacting_refresher(
    state: &Arc<AppState>,
) -> SessionChatQueueCompactingRefresher {
    let detector = SessionChatOptionDetector::new(state);
    let state = state.clone();
    Arc::new(
        move |project_id: &str, session_id: &str, agent: Option<&str>| {
            let detector = detector.clone();
            let state = state.clone();
            let project_id = project_id.to_string();
            let session_id = session_id.to_string();
            let agent = agent.map(str::to_string);
            tokio::task::spawn_blocking(move || {
                // Startup, compaction and input-box holds request this probe. Fresh evidence
                // releases input promptly instead of waiting out the five-second cache.
                let detection =
                    detector.detect_blocking(&project_id, &session_id, agent.as_deref(), true);
                // An idle Claude whose input box stays unreadable is asked to redraw, since
                // nothing else repaints an unwatched one (held_redraw.rs).
                super::held_redraw::redraw_held_claude_if_due(
                    &state,
                    &project_id,
                    &session_id,
                    agent.as_deref(),
                    &detection,
                );
                // Still unreadable after that: a one-row resize and back, for any agent
                // (repaint_nudge.rs).
                super::repaint_nudge::nudge_held_repaint_if_due(
                    &state,
                    &project_id,
                    &session_id,
                    agent.as_deref(),
                    &detection,
                );
            });
        },
    )
}

/// Posts a stuck row's notification to the bell, the way `ghostex notify` does, so it reaches the
/// user on every client even when the session it waits on is the one nobody is watching.
pub(crate) fn session_chat_queue_stuck_notifier(
    state: &Arc<AppState>,
) -> crate::session_chat_queue_runtime::SessionChatQueueStuckNotifier {
    let state = state.clone();
    Arc::new(move |project_id: &str, session_id: &str, body: &str| {
        let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
            return;
        };
        let repository =
            crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str());
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        params.insert("body".to_string(), json!(body));
        let _ = crate::notification_feed::create_notification_endpoint(
            &state,
            &repository,
            &db,
            &params,
        );
    })
}
