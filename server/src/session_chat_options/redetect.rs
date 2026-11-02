use super::*;

/*
CDXC:AgentScreenDetection 2026-09-05 WHY:
Optimistic option controls need fresh evidence even when a CLI refuses a change and its footer stays the same.
Post-delivery probes start immediately, then cover slower repaints at 150ms, 2s and 6s; captured option state is republished even when unchanged so the client can settle or undo its pending selection.
*/
pub(crate) fn schedule_session_chat_option_redetect(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    agent: Option<&str>,
) {
    if crate::session_chat_options::session_chat_option_agent(agent).is_none() {
        return;
    }
    let detector = SessionChatOptionDetector::new(state);
    let followers = state.session_chat_followers.clone();
    let event_hub = state.event_hub.clone();
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    let agent = agent.map(str::to_string);
    tokio::spawn(async move {
        let cached = detector.cached(&project_id, &session_id);
        let mut published = cached.options;
        // CDXC:AgentScreenDetection 2026-08-19: this probe re-reads the
        // screen anyway, so a notice that appeared or cleared with the dispatch
        // rides the same frame instead of waiting for the ~30s follower probe.
        let mut published_notice = crate::session_chat_notice::resolve_session_chat_terminal_notice(
            &project_id,
            &session_id,
            cached.notice,
        );
        let mut published_activity = cached.activity;
        let mut published_fleet = cached.fleet;
        let mut published_tasks = cached.tasks;
        let mut published_workflows = cached.workflows;
        let mut published_prompt = cached.prompt;
        /*
        CDXC:SessionChat 2026-09-10 WHY:
        Starts empty so the first probe publishes the rows this send created:
        the send worker's own capture has usually filled a command's output
        before this burst begins, and nothing else ever pushes it to an open
        chat — the row used to appear only when switching views forced a read.
        */
        let mut published_app_commands = String::new();
        for delay_ms in crate::session_chat_options::SESSION_CHAT_OPTION_REDETECT_DELAYS_MS {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            let detection = detector
                .detect(&project_id, &session_id, agent.as_deref(), true)
                .await;
            let notice = crate::session_chat_notice::resolve_session_chat_terminal_notice(
                &project_id,
                &session_id,
                detection.notice,
            );
            let notice_changed = detection.captured
                && !crate::session_chat_notice::same_session_chat_terminal_notice(
                    notice.as_ref(),
                    published_notice.as_ref(),
                );
            let options_changed = detection
                .options
                .as_ref()
                .is_some_and(|detected| !detected.same_selection(published.as_ref()));
            let options_refreshed = detection.captured && detection.options.is_some();
            let activity_changed = detection.captured
                && !crate::session_chat_terminal_activity::same_session_chat_terminal_activity(
                    detection.activity.as_ref(),
                    published_activity.as_ref(),
                );
            let fleet_changed = detection.fleet_observed
                && !crate::session_chat_agent_fleet::same_session_chat_agent_fleet(
                    detection.fleet.as_ref(),
                    published_fleet.as_ref(),
                );
            // Disk-backed, so no capture gate: the store is authoritative on its own.
            let tasks_changed = !crate::session_chat_agent_tasks::same_session_chat_agent_tasks(
                detection.tasks.as_ref(),
                published_tasks.as_ref(),
            );
            let workflows_changed =
                !crate::session_chat_claude_workflow_cards::same_session_chat_workflows(
                    detection.workflows.as_ref(),
                    published_workflows.as_ref(),
                );
            let prompt_changed = detection.captured && detection.prompt != published_prompt;
            let app_commands = crate::session_chat_app_command::session_chat_app_commands_identity(
                &project_id,
                &session_id,
            );
            let app_commands_changed = app_commands != published_app_commands;
            if !options_changed
                && !options_refreshed
                && !notice_changed
                && !activity_changed
                && !fleet_changed
                && !tasks_changed
                && !workflows_changed
                && !prompt_changed
                && !app_commands_changed
            {
                continue;
            }
            published_app_commands = app_commands;
            if options_changed || options_refreshed {
                published = detection.options;
            }
            if notice_changed {
                published_notice = notice;
            }
            if activity_changed {
                published_activity = detection.activity;
            }
            if fleet_changed {
                published_fleet = detection.fleet;
            }
            if tasks_changed {
                published_tasks = detection.tasks;
            }
            if workflows_changed {
                published_workflows = detection.workflows;
            }
            if prompt_changed {
                published_prompt = detection.prompt;
            }
            emit_session_chat_options_state_frame(
                &followers,
                &event_hub,
                &paths,
                &server_id,
                &project_id,
                &session_id,
                published.as_ref(),
                crate::session_chat::SessionChatScreenState {
                    prompt: published_prompt.as_ref(),
                    notice: published_notice.as_ref(),
                    activity: published_activity.as_ref(),
                    fleet: published_fleet.as_ref(),
                    tasks: published_tasks.as_ref(),
                    workflows: published_workflows.as_ref(),
                    probed: detection.attempted,
                },
            );
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_session_chat_options_state_frame(
    followers: &Arc<Mutex<HashMap<String, SessionChatFollowerEntry>>>,
    event_hub: &GxserverEventHub,
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    detected: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
    screen: crate::session_chat::SessionChatScreenState<'_>,
) {
    let stream = {
        let Ok(followers) = followers.lock() else {
            return;
        };
        let Some(entry) = followers.get(&session_observer_key(project_id, session_id)) else {
            return;
        };
        let follower_active =
            entry.subscribers > 0 && entry.task.as_ref().is_some_and(|task| !task.is_finished());
        if !follower_active {
            return;
        }
        entry.stream.clone()
    };
    let Ok(db) = open_gxserver_database(paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, server_id);
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    let prompt = crate::agents::session_chat_prompt_setting(&session)
        .as_deref()
        .and_then(crate::session_chat::parse_stored_session_chat_prompt);
    let working = session_chat_hook_working(&session);
    let status = if working {
        crate::session_chat::SessionChatStatus::Working
    } else {
        crate::session_chat::SessionChatStatus::Ready
    };
    let agent_session_id = read_runtime_text(&session, "agentSessionId");
    let queue =
        crate::session_chat_queue::read_session_chat_queue_snapshot(paths, project_id, session_id);
    // Same seq discipline as the prompt frame: take the epoch and the seq and
    // publish as one step, because the follower task publishes into the SAME
    // counter and can start a new generation in between
    // (CDXC:AgentScreenDetection 2026-08-24).
    stream.emit_sequenced(
        |seq| {
            let (epoch, _) = stream.current();
            let mut frame = crate::session_chat::build_session_chat_prompt_state_frame(
                project_id,
                session_id,
                epoch,
                seq,
                status,
                prompt.as_ref(),
                agent_session_id.as_deref(),
                GXSERVER_PROTOCOL_VERSION,
                server_id,
                working,
                detected,
                screen,
                Some(&queue),
            );
            // The local-command rows a screen probe just refined ride the same
            // frame the probe publishes, or an open chat never sees them.
            if let Value::Object(map) = &mut frame {
                crate::session_chat_app_command::insert_session_chat_app_commands(
                    map, project_id, session_id,
                );
                crate::coordinators::insert_coordinator_threads(map, project_id, session_id);
                crate::side_chats::insert_side_chats(map, project_id, session_id);
                crate::agent_message_deliveries::insert_agent_message_deliveries(
                    map,
                    project_id,
                    session_id,
                    crate::agent_message_deliveries::DeliveriesFrame::State,
                );
            }
            frame
        },
        |frame| event_hub.broadcast(frame),
    );
}
