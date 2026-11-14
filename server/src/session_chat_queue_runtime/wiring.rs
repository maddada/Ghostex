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
        // CDXC:Notifications 2026-11-14 DECISION (Q3242 in server/src/worker_alerts.rs): the
        // user's screenshot showed this row for a coordinator's message to its thread, which they
        // can do nothing about. A thread's stuck message goes to its coordinator, as a held note
        // it reads at its next stop; the row only while Workers brings them back.
        if let Some((coordinator_project, coordinator_session)) =
            crate::worker_alerts::worker_coordinator(&db, project_id, session_id)
        {
            if tell_coordinator_message_stuck(
                &state,
                &db,
                &repository,
                (project_id, session_id),
                (&coordinator_project, &coordinator_session),
                body,
            ) {
                super::broadcast_session_chat_queue_state(
                    &state,
                    &coordinator_project,
                    &coordinator_session,
                );
            }
            if !crate::worker_alerts::worker_alerts().rows() {
                return;
            }
        }
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

/// Puts a note in the coordinator's queue that its thread has not taken a message for minutes
/// because its input box is not on screen: `body` is the bell's words about the thread, which name
/// the wait and quote its terminal. Held, so a busy coordinator takes it at its next input
/// boundary. Returns whether the note was queued.
fn tell_coordinator_message_stuck(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &crate::domain::DomainRepository<'_>,
    (project_id, session_id): (&str, &str),
    (coordinator_project, coordinator_session): (&str, &str),
    body: &str,
) -> bool {
    let Ok(Some(thread)) = repository.get_session(project_id, session_id) else {
        return false;
    };
    let title = crate::coordinators::session_title_of(&thread);
    let thread_ref = crate::ids::create_global_session_ref(
        state.metadata.server_id.as_str(),
        project_id,
        session_id,
    );
    let wait = body
        .strip_prefix("A message has waited ")
        .and_then(|rest| rest.split_once(" to reach this agent"))
        .map_or("several minutes", |(minutes, _)| minutes);
    let shows = body
        .find(" The terminal shows: ")
        .and_then(|start| {
            let rest = &body[start + 1..];
            rest.find(". Open the terminal").map(|end| &rest[..end + 1])
        })
        .map(|shows| format!(" {shows}"))
        .unwrap_or_default();
    let note = format!(
        "Ghostex thread report: a message has waited {wait} to reach \"{title}\" ({thread_ref}) because its input box isn't on screen.{shows} Ghostex keeps trying and sends the message once the box is back. If it stays stuck, `ghostex agents reload {thread_ref}`, or ask the user to open its terminal."
    );
    crate::session_chat_queue::enqueue_session_chat_prompt(
        db,
        coordinator_project,
        coordinator_session,
        &note,
        true,
    )
    .is_ok()
}

/// The input-box ladder's reload step (held_reload.rs).
pub(crate) fn session_chat_queue_input_box_reloader(
    state: &Arc<AppState>,
) -> crate::session_chat_queue_runtime::SessionChatQueueInputBoxReloader {
    let state = state.clone();
    Arc::new(
        move |project_id: String, session_id: String, prompt_id: String| {
            Box::pin(super::held_reload::reload_for_held_message(
                state.clone(),
                project_id,
                session_id,
                prompt_id,
            ))
        },
    )
}
