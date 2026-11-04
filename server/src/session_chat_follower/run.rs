use super::*;

/*
Per-session follower task. Runs only while ≥1 client subscribes AND the
session is running (the server.rs registry enforces both). `resnapshot` is
signaled when another subscriber joins a live follower: every subscribe must
be answered by an authoritative snapshot, so the follower starts a fresh
generation (epoch bump, seq reset) and re-reads the tail instead of being
torn down and respawned mid-drain.
*/
pub async fn run_session_chat_follower(
    mut config: SessionChatFollowerConfig,
    stream: Arc<SessionChatStream>,
    resnapshot: Arc<tokio::sync::Notify>,
    // CDXC:AgentScreenDetection 2026-08-24: the task's own progress
    // signal, read by `sync_session_chat_follower_for_session`.
    heartbeat: Arc<SessionChatFollowerHeartbeat>,
    emit: SessionChatFrameEmitter,
) {
    let read_live_state = || match config.state_reader.as_ref() {
        Some(reader) => reader(),
        None => SessionChatLiveState::default(),
    };
    // Cached detection only: frames must never pay for a process spawn. Carries
    // BOTH the model/effort pills and the terminal-state notice.
    let read_cached_detection = || {
        config
            .options_reader
            .as_ref()
            .map(|reader| reader(crate::session_chat_options::SessionChatOptionsReadMode::Cached))
            .unwrap_or_default()
    };
    let Some(transcript_agent) = resolve_session_chat_transcript_agent(config.agent.as_deref())
    else {
        // A Claude cloud session's chat is empty until its first message starts it on claude.ai.
        let status = if config.agent.as_deref() == Some(crate::claude_cloud::CLAUDE_CLOUD_AGENT_ID)
        {
            SessionChatStatus::Empty
        } else {
            SessionChatStatus::Unsupported
        };
        loop {
            let epoch = stream.begin_generation();
            emit_state_frame(
                &emit,
                &config,
                &stream,
                epoch,
                status,
                None,
                None,
                None,
                SessionChatScreenState::default(),
            );
            heartbeat.park();
            resnapshot.notified().await;
            heartbeat.unpark();
        }
    };
    let decode = session_chat_line_decoder(transcript_agent);
    let decode_lifecycle = session_chat_lifecycle_decoder(transcript_agent);

    let mut epoch = stream.begin_generation();
    let mut want_snapshot = true;
    let mut emitted_starting = false;
    let mut resolved: Option<PathBuf> = None;
    let mut resolve_delay = INITIAL_RESOLVE_POLL;
    // CDXC:AgentScreenDetection 2026-09-01: paces the slow steady
    // re-probe of the resolve-poll branch once the launch screen has settled.
    let mut unresolved_last_probe = std::time::Instant::now();
    let mut file_state = FollowerFileState::new();
    // Rolling AskUserQuestion state folded over everything decoded so far, plus
    // the last prompt/working pair actually published to clients.
    let mut transcript_prompt = SessionChatTranscriptPromptState::default();
    let mut published_prompt: Option<SessionChatInteractivePrompt> = None;
    let mut published_working = false;
    let mut published_state_valid = false;
    let mut identity = SessionChatFollowerIdentity {
        agent_session_id: config.agent_session_id.clone(),
        agent_session_path: config.agent_session_path.clone(),
    };
    let mut last_transcript_change = std::time::Instant::now();
    let mut last_staleness_check = std::time::Instant::now();
    let mut last_successor_scan = std::time::Instant::now();
    // "Adopt none and log once" for an ambiguous successor set.
    let mut logged_successor_ambiguity: Option<String> = None;
    // Model/effort the follower has published, plus counters that pace the
    // fast startup probes and periodic steady-state re-detects.
    let mut published_options: Option<crate::session_chat_options::SessionChatDetectedOptions> =
        None;
    // Terminal-state notice the follower has published. Tracked separately
    // because it can legitimately go back to `None` (the screen cleared), which
    // MUST be published as an omitted field.
    let mut published_notice: Option<crate::session_chat_notice::SessionChatTerminalNotice> = None;
    /*
    CDXC:AgentScreenDetection 2026-08-22: the progress row the follower
    has published. Tracked like the notice (it can legitimately go back to
    `None` when the work finishes) but compared on its NUMBERS too, because a
    moving percentage is the whole point of publishing it again.
    */
    let mut published_activity: Option<
        crate::session_chat_terminal_activity::SessionChatTerminalActivity,
    > = None;
    /*
    CDXC:AgentScreenDetection 2026-08-23: deliberately NOT cleared when the
    main agent goes idle, unlike the activity row above. A `⏺` status line is
    stale scrollback the moment Claude stops, but sub-agents outlive the
    turn that spawned them — clearing on idle would blank the strip exactly when
    it is the only thing telling the user work is still running.
    */
    let mut published_fleet: Option<crate::session_chat_agent_fleet::SessionChatAgentFleet> = None;
    // CDXC:SessionChat 2026-09-03: the task list last published.
    // Compared whole; a task flipping to completed is exactly the change the
    // panel exists to show.
    let mut published_tasks: Option<crate::session_chat_agent_tasks::SessionChatAgentTasks> = None;
    // CDXC:SessionChat 2026-10-22: the Workflow runs last published, compared whole like tasks.
    let mut published_workflows: Option<
        crate::session_chat_claude_workflow_cards::SessionChatWorkflows,
    > = None;
    // CDXC:AgentScreenDetection 2026-08-22: latched, not sampled. It answers
    // "has detection run for this session yet", so a later capture failure (the
    // session stopped, the daemon went away) must not put the composer back
    // under a loading skeleton.
    let mut published_screen_probed = false;
    // The statusline watch fired during the resolve-poll sleep; the next pass probes for it.
    let mut statusline_landed = false;
    let mut reconcile_ticks: u64 = 0;
    let mut startup_option_reconcile_ticks: u64 = 0;
    // CDXC:AgentScreenDetection 2026-09-02: reconciles left in the
    // back-to-back probe burst a `/compact` transcript row starts.
    let mut activity_command_probe_ticks: u64 = 0;

    /*
    CDXC:AgentScreenDetection 2026-08-22:
    Probe once, here, before the first frame goes out.

    The snapshot frame reads detection from the shared cache and never spawns,
    which was right when a capture cost a login shell and a process. On a cold
    cache — the first chat open after a gxserver start — that meant the
    snapshot carried NO model/effort at all, and the client's seed read (which
    does force a detection) is explicitly outranked by the first frame, so its
    freshly detected value was discarded. The pills then stayed blank until the
    startup probe below fired on the second reconcile, a second or more later,
    and only then snapped to the real model. That flash of "Model"/"Options"
    turning into "Opus 5"/"High" is what this removes.

    A capture is now a direct socket read (CDXC:AppShots),
    ~0.1ms typical and ~6ms against a very large scrollback, so paying for one
    before the snapshot is cheaper than the frame it rides on. The deadline
    exists only for a wedged daemon that accepts the connection and never
    answers: the capture's own read timeout is 5s, and stalling a subscribe
    that long to populate a pill is not a trade worth making. Missing the
    deadline is not an error — the startup probe below still runs.
    */
    if let Some(reader) = config.options_reader.clone() {
        let _ = tokio::time::timeout(
            SEED_OPTION_DETECTION_DEADLINE,
            tokio::task::spawn_blocking(move || {
                reader(crate::session_chat_options::SessionChatOptionsReadMode::Refresh)
            }),
        )
        .await;
    }

    loop {
        heartbeat.stamp();
        if want_snapshot {
            let live = read_live_state();
            if live.agent_session_id.is_some() && live.agent_session_id != identity.agent_session_id
            {
                // CDXC:SessionChat 2026-09-07 WHY:
                // Codex rewind adopts a new conversation before requesting this snapshot. Re-resolve now instead of re-sending the abandoned file until the idle successor scan.
                identity.adopt(live);
                resolved = None;
                file_state = FollowerFileState::new();
                transcript_prompt = SessionChatTranscriptPromptState::default();
                emitted_starting = false;
                published_state_valid = false;
                if identity.agent_session_path.is_none() {
                    emit_snapshot_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        "sessionChatSnapshot",
                        &SessionChatTailFileResult::default(),
                        None,
                        false,
                        published_options.as_ref(),
                        SessionChatScreenState::default(),
                    );
                }
            }
        }
        if resolved.is_none() {
            let unbound_lineage = match transcript_agent {
                SessionChatTranscriptAgent::Freebuff => Some("freebuff-chat-folder"),
                SessionChatTranscriptAgent::Dsh => Some("dsh-session-store"),
                _ => None,
            };
            if let Some(lineage) = unbound_lineage.filter(|_| identity.agent_session_id.is_none()) {
                if let Some((id, path)) = adopt_unbound_agent_chat(&config, lineage).await {
                    identity.agent_session_id = Some(id);
                    identity.agent_session_path = Some(path);
                }
            }
            let agent_session_id = identity.agent_session_id.clone();
            let agent_session_path = identity.agent_session_path.clone();
            resolved = tokio::task::spawn_blocking(move || {
                resolve_session_chat_transcript_path(
                    transcript_agent,
                    agent_session_id.as_deref(),
                    agent_session_path.as_deref(),
                )
            })
            .await
            .ok()
            .flatten();
            if resolved.is_none() {
                /*
                CDXC:AgentScreenDetection 2026-09-01:
                A freshly launched agent has no transcript file at all until its
                first prompt (Claude creates the session .jsonl on the first
                message), so this resolve-poll branch is the follower's ONLY
                state for the whole pre-first-prompt phase — including the
                moment the TUI paints its model/effort footer, seconds after
                the subscribe-time seed probe read a still-blank screen.
                Emitting one Starting frame and then polling only for the path
                left the composer's model pill under its loading skeleton until
                something else happened to refresh detection, which nothing was
                obliged to ever do. So this branch probes too: every pass while
                the screen has not settled (`attempted` covers launch paint and
                the model settle grace), then on a slow steady cadence, and it
                re-emits the Starting state frame whenever detection actually
                changed. The first pass keeps reading the cache — the
                subscribe's own seed probe just captured at t=0.
                */
                let live = read_live_state();
                // CDXC:AgentScreenDetection 2026-09-15 WHY:
                // A new Claude chat can stay transcriptless indefinitely. Watch its statusline here too, so first paint and idle option changes do not wait for the 30-second history-resolution cadence.
                let statusline_changed = std::mem::take(&mut statusline_landed)
                    || config
                        .options_change_watch
                        .as_ref()
                        .is_some_and(|watch| watch(identity.agent_session_id.as_deref()));
                let probe_due = config.options_reader.is_some()
                    && emitted_starting
                    && (!published_screen_probed
                        || statusline_changed
                        || live.working
                        || published_activity.is_some()
                        || fleet_is_working(published_fleet.as_ref())
                        || unresolved_last_probe.elapsed() >= UNRESOLVED_STEADY_PROBE_INTERVAL
                        || crate::session_chat_screen_watch::session_chat_screen_changed(
                            config.screen_change_watch.as_ref(),
                        )
                        .await);
                let detection = if probe_due {
                    unresolved_last_probe = std::time::Instant::now();
                    let reader = config.options_reader.clone();
                    match tokio::time::timeout(
                        STEADY_OPTION_DETECTION_DEADLINE,
                        tokio::task::spawn_blocking(move || {
                            reader
                                .map(|reader| {
                                    reader(
                                        crate::session_chat_options::SessionChatOptionsReadMode::Refresh,
                                    )
                                })
                                .unwrap_or_default()
                        }),
                    )
                    .await
                    {
                        Ok(Ok(detection)) => detection,
                        _ => read_cached_detection(),
                    }
                } else {
                    read_cached_detection()
                };
                let activity =
                    crate::session_chat_terminal_activity::publishable_session_chat_terminal_activity(
                        live.working,
                        detection.activity.clone(),
                    );
                let options_changed = detection
                    .options
                    .as_ref()
                    .is_some_and(|detected| !detected.same_selection(published_options.as_ref()));
                let starting_changed = options_changed
                    || !crate::session_chat_notice::same_session_chat_terminal_notice(
                        detection.notice.as_ref(),
                        published_notice.as_ref(),
                    )
                    || !crate::session_chat_terminal_activity::same_session_chat_terminal_activity(
                        activity.as_ref(),
                        published_activity.as_ref(),
                    )
                    || !crate::session_chat_agent_fleet::same_session_chat_agent_fleet(
                        detection.fleet.as_ref(),
                        published_fleet.as_ref(),
                    )
                    || !crate::session_chat_agent_tasks::same_session_chat_agent_tasks(
                        detection.tasks.as_ref(),
                        published_tasks.as_ref(),
                    )
                    || !crate::session_chat_claude_workflow_cards::same_session_chat_workflows(
                        detection.workflows.as_ref(),
                        published_workflows.as_ref(),
                    )
                    || (detection.attempted && !published_screen_probed);
                if !emitted_starting || starting_changed {
                    emit_state_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        SessionChatStatus::Starting,
                        live.prompt.as_ref(),
                        Some(live.working),
                        detection.options.as_ref(),
                        SessionChatScreenState {
                            prompt: detection.prompt.as_ref(),
                            notice: detection.notice.as_ref(),
                            activity: activity.as_ref(),
                            fleet: detection.fleet.as_ref(),
                            tasks: detection.tasks.as_ref(),
                            workflows: detection.workflows.as_ref(),
                            probed: published_screen_probed || detection.attempted,
                        },
                    );
                    if options_changed {
                        published_options = detection.options;
                    }
                    published_notice = detection.notice;
                    published_activity = activity;
                    published_fleet = detection.fleet;
                    published_tasks = detection.tasks;
                    published_workflows = detection.workflows;
                    published_screen_probed = published_screen_probed || detection.attempted;
                    emitted_starting = true;
                }
                // CDXC:AgentScreenDetection 2026-09-15 WHY:
                // Transcript resolution can lag live compaction. Backing off both the loop and its screen probe freezes progress for up to 30s; active screens need the same reconcile cadence as a resolved transcript.
                let poll_delay = if !published_screen_probed {
                    INITIAL_RESOLVE_POLL
                } else if live.working
                    || published_activity.is_some()
                    || fleet_is_working(published_fleet.as_ref())
                    || config.options_change_watch.is_some()
                {
                    resolve_delay.min(config.tuning.reconcile_interval)
                } else {
                    resolve_delay
                };
                heartbeat.park();
                tokio::select! {
                    _ = tokio::time::sleep(poll_delay) => {}
                    _ = resnapshot.notified() => {
                        epoch = stream.begin_generation();
                        emitted_starting = false;
                        want_snapshot = true;
                    }
                    _ = statusline_payload_written(
                        config.options_change_watch.as_ref(),
                        identity.agent_session_id.as_deref(),
                    ) => {
                        statusline_landed = true;
                    }
                }
                heartbeat.unpark();
                resolve_delay = (resolve_delay * 2).min(MAX_RESOLVE_POLL);
                // A stale hook identity is the usual reason the path never
                // appears: re-read the session's current identity each poll.
                identity.adopt(read_live_state());
                continue;
            }
            want_snapshot = true;
            file_state = FollowerFileState::new();
            transcript_prompt = SessionChatTranscriptPromptState::default();
            last_transcript_change = std::time::Instant::now();
        }

        let path = resolved.clone().expect("resolved transcript path");
        let drain_limit = config.limit;
        let drain_want_snapshot = want_snapshot;
        let mut drain_state = std::mem::replace(&mut file_state, FollowerFileState::new());
        let Ok((returned_state, outcome)) = tokio::task::spawn_blocking(move || {
            let outcome = follower_drain_once(
                &path,
                drain_limit,
                transcript_agent,
                decode,
                decode_lifecycle,
                &mut drain_state,
                drain_want_snapshot,
            );
            (drain_state, outcome)
        })
        .await
        else {
            return;
        };
        file_state = returned_state;
        // One live-state read per reconcile: it opens the domain database.
        let live = read_live_state();

        match outcome {
            FollowerDrainOutcome::Missing => {
                // Rotation to a missing path — resolve-poll again and deliver
                // an authoritative frame once the successor file appears.
                resolved = None;
                resolve_delay = INITIAL_RESOLVE_POLL;
                epoch = stream.begin_generation();
                emitted_starting = false;
                want_snapshot = true;
                continue;
            }
            FollowerDrainOutcome::Snapshot {
                tail,
                appended,
                appended_lifecycle,
                content_replaced,
            } => {
                // A prompt Claude handed back to its composer stays in the
                // JSONL as an orphan row until the next prompt abandons it.
                let mut tail = tail;
                let mut appended = appended;
                let mut appended_lifecycle = appended_lifecycle;
                crate::session_chat_returned_prompt::filter_session_chat_returned_prompts(
                    &config.project_id,
                    &config.session_id,
                    &mut tail.messages,
                    &mut tail.lifecycle,
                );
                crate::session_chat_returned_prompt::filter_session_chat_returned_prompts(
                    &config.project_id,
                    &config.session_id,
                    &mut appended,
                    &mut appended_lifecycle,
                );
                let frame_type = if want_snapshot {
                    "sessionChatSnapshot"
                } else {
                    if content_replaced {
                        epoch = stream.begin_generation();
                    }
                    "sessionChatReplaced"
                };
                // The tail window replaces everything the client had, so the
                // question fold restarts from it.
                transcript_prompt.restart();
                transcript_prompt.advance(&tail.messages);
                transcript_prompt.advance(&appended);
                // A subscribing client gets the detected pills value and any
                // terminal-state notice with its snapshot, so it needs no
                // separate read.
                let mut snapshot_detection = read_cached_detection();
                file_state
                    .incremental
                    .codex_stats
                    .apply(&mut snapshot_detection.options);
                // The seed capture is the FIRST look a chat opened mid-compaction
                // gets at the screen; the compacting row it finds is live
                // whatever the hooks last said (CDXC:AgentScreenDetection).
                let snapshot_activity =
                    crate::session_chat_terminal_activity::publishable_session_chat_terminal_activity(
                        live.working,
                        snapshot_detection.activity.clone(),
                    );
                transcript_prompt.observe_stored(live.prompt.as_ref());
                let prompt = resolve_session_chat_prompt(live.prompt.clone(), &transcript_prompt)
                    .or_else(|| snapshot_detection.prompt.clone());
                emit_snapshot_frame(
                    &emit,
                    &config,
                    &stream,
                    epoch,
                    frame_type,
                    &tail,
                    prompt.as_ref(),
                    live.working,
                    snapshot_detection.options.as_ref(),
                    SessionChatScreenState {
                        prompt: None,
                        notice: snapshot_detection.notice.as_ref(),
                        activity: snapshot_activity.as_ref(),
                        fleet: snapshot_detection.fleet.as_ref(),
                        tasks: snapshot_detection.tasks.as_ref(),
                        workflows: snapshot_detection.workflows.as_ref(),
                        probed: published_screen_probed || snapshot_detection.attempted,
                    },
                );
                published_screen_probed = published_screen_probed || snapshot_detection.attempted;
                if snapshot_detection.options.is_some() {
                    published_options = snapshot_detection.options;
                }
                published_notice = snapshot_detection.notice;
                published_activity = snapshot_activity;
                published_fleet = snapshot_detection.fleet;
                published_tasks = snapshot_detection.tasks;
                published_workflows = snapshot_detection.workflows;
                published_prompt = prompt;
                published_working = live.working;
                published_state_valid = true;
                want_snapshot = false;
                last_transcript_change = std::time::Instant::now();
                if !appended.is_empty() || appended_lifecycle.is_some() {
                    emit_appended_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        &appended,
                        appended_lifecycle.as_ref(),
                        &[],
                    );
                }
            }
            FollowerDrainOutcome::Appended {
                batches,
                lifecycle,
                superseded,
                api_refusal,
            } => {
                last_transcript_change = std::time::Instant::now();
                /*
                CDXC:AgentScreenDetection 2026-08-28:
                Stored in the watchdog store on purpose: it inherits the
                store's dismissal identity, its 10-minute expiry, and its
                retirement by the next send (the send watchdog clears the
                store when a new message goes in — which is exactly when a
                refusal card stops being news). Each refusal row is seen by
                exactly one drain window, so this publishes once per refusal.
                */
                if let Some(refusal) = api_refusal {
                    crate::session_chat_notice::set_session_chat_watchdog_notice(
                        &config.project_id,
                        &config.session_id,
                        crate::session_chat_notice::session_chat_api_refusal_notice(refusal),
                    );
                    if let Some(notice_publisher) = config.notice_publisher.as_ref() {
                        notice_publisher();
                    }
                }
                if batches.is_empty() {
                    // Lifecycle-only and retraction-only frames ARE emitted.
                    emit_appended_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        &[],
                        lifecycle.as_ref(),
                        &superseded,
                    );
                } else {
                    /*
                    CDXC:AgentScreenDetection 2026-09-02:
                    A `/compact` row is the cue to look at the screen NOW rather
                    than at the idle 30s tier: the user who typed it — in the
                    composer or straight into the terminal, both record the
                    same row — is watching for the card. The burst runs through
                    the ordinary probe below, so what it finds is published and
                    remembered by this one loop.
                    */
                    if batches.iter().flatten().any(|message| {
                        crate::session_chat_terminal_activity::transcript_message_starts_session_chat_activity(
                            config.agent.as_deref(),
                            message,
                        )
                    }) {
                        activity_command_probe_ticks = crate::session_chat_terminal_activity::SESSION_CHAT_ACTIVITY_COMMAND_PROBE_TICKS;
                    }
                    let last_index = batches.len() - 1;
                    for (index, batch) in batches.iter().enumerate() {
                        transcript_prompt.advance(batch);
                        let batch_lifecycle = if index == last_index {
                            lifecycle.as_ref()
                        } else {
                            None
                        };
                        // The retraction rides the FIRST frame so a client can
                        // never re-order it after the rows that replace it.
                        let batch_superseded: &[String] =
                            if index == 0 { &superseded } else { &[] };
                        emit_appended_frame(
                            &emit,
                            &config,
                            &stream,
                            epoch,
                            batch,
                            batch_lifecycle,
                            batch_superseded,
                        );
                    }
                }
            }
            FollowerDrainOutcome::Idle => {
                // The returned-prompt detector cannot publish into this stream
                // itself; its retraction rides the next reconcile tick.
                if let Some((retracted, lifecycle)) =
                    crate::session_chat_returned_prompt::take_session_chat_returned_prompt_retraction(
                        &config.project_id,
                        &config.session_id,
                    )
                {
                    emit_appended_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        &[],
                        Some(&lifecycle),
                        &retracted,
                    );
                }
            }
        }

        /*
        CDXC:SessionChat 2026-08-01:
        Interactive cards used to depend entirely on agent hooks. When the
        installed hook script does not forward toolName/toolInput the card never
        appeared, and when it never reports PostToolUse a card answered in the
        terminal stayed on screen forever. The transcript itself answers both:
        a trailing AskUserQuestion tool call with no tool result means "pending",
        a tool result after it means "answered". The hook prompt still wins when
        both exist, so approvals and richer hook payloads are unaffected.
        */
        transcript_prompt.observe_stored(live.prompt.as_ref());
        let effective_prompt = resolve_session_chat_prompt(live.prompt.clone(), &transcript_prompt)
            .or_else(|| read_cached_detection().prompt);
        let previous_options = published_options.clone();
        file_state
            .incremental
            .codex_stats
            .apply(&mut published_options);
        let stats_changed = published_options
            .as_ref()
            .is_some_and(|next| !next.same_selection(previous_options.as_ref()));
        let became_ready = published_state_valid && published_working && !live.working;
        // A `⏺` row remains on Claude's primary screen after it stops. Clear
        // that stale status on the ready transition, but retain the
        // screen-proven kinds (`remains_live_when_ready`): a background shell
        // outlives the main turn by definition, and a compaction is retired by
        // the next whole capture that no longer shows its row, which the
        // 1s activity tier below takes within a second of the `Compacted` line.
        if !live.working
            && !published_activity
                .as_ref()
                .is_some_and(|activity| activity.remains_live_when_ready())
        {
            published_activity = None;
        }
        if !published_state_valid
            || stats_changed
            || effective_prompt != published_prompt
            || live.working != published_working
        {
            /*
            CDXC:SessionChat 2026-10-22 WHY:
            This frame restates the notice, and the post-answer redetect (session_chat_options/redetect.rs) publishes through the shared detector cache, not through this loop. Restating this loop's own last copy sent an answered permission prompt back out right after the redetect had cleared it (the answer turns the agent to working, which lands here): the chat drew the answered card again for a moment, then the next frame took it away. The newest whole capture in the cache is the current notice.
            */
            let cached = read_cached_detection();
            if cached.captured {
                published_notice = cached.notice;
            }
            if published_state_valid {
                emit_state_frame(
                    &emit,
                    &config,
                    &stream,
                    epoch,
                    if live.working {
                        SessionChatStatus::Working
                    } else {
                        SessionChatStatus::Ready
                    },
                    effective_prompt.as_ref(),
                    Some(live.working),
                    published_options.as_ref(),
                    SessionChatScreenState {
                        prompt: None,
                        notice: published_notice.as_ref(),
                        activity: published_activity.as_ref(),
                        fleet: published_fleet.as_ref(),
                        tasks: published_tasks.as_ref(),
                        workflows: published_workflows.as_ref(),
                        probed: published_screen_probed,
                    },
                );
            }
            published_prompt = effective_prompt;
            published_working = live.working;
            published_state_valid = true;
        }

        /*
        CDXC:AgentScreenDetection 2026-08-01:
        Model/effort probe: a newly launched agent can paint its footer just
        after the seed probe read an empty screen. Probe each 1s reconcile for
        up to ten seconds until both values arrive, then retain the ~30s
        steady-state cadence that catches direct TUI changes. The follower only
        exists while subscribed, and a frame is emitted only when the detected
        value actually changed.

        `reconcile_ticks > 1` skips the first pass on purpose: the subscribe's
        own probe (CDXC:AgentScreenDetection) already captured at t=0 and
        the snapshot frame published it, so probing again immediately would
        capture the same unchanged screen twice.

        CDXC:AgentScreenDetection 2026-08-19:
        The same probe classifies the captured screen, so a trust dialog or an
        expired login reaches chat on this cadence for free. A notice may also
        legitimately CLEAR, which the options half can never do — but only a
        capture that actually succeeded proves a clean screen, so a failed or
        capped read leaves the published notice standing.
        */
        reconcile_ticks = reconcile_ticks.wrapping_add(1);
        let startup_probe_due = published_state_valid
            && config.options_reader.is_some()
            && reconcile_ticks > 1
            && startup_option_reconcile_ticks
                < crate::session_chat_options::SESSION_CHAT_OPTION_STARTUP_RECONCILE_TICKS
            && published_options.as_ref().is_none_or(|options| {
                // CDXC:AgentScreenDetection 2026-09-08 WHY:
                // A restored Codex can show Ultra while its last turn still records High; transcript-only values must not end the startup footer probes before the live setting is read.
                [&options.selection.model, &options.selection.effort]
                    .into_iter()
                    .any(|choice| {
                        choice.as_ref().is_none_or(|choice| {
                            matches!(
                                choice.source,
                                SessionChatOptionEvidence::Transcript
                                    | SessionChatOptionEvidence::Launch
                            )
                        })
                    })
            });
        if startup_probe_due {
            startup_option_reconcile_ticks += 1;
        }
        /*
        CDXC:AgentScreenDetection 2026-08-22:
        The steady 30s cadence is right for state that either holds or does not
        (a login screen, a model pill), and useless for a progress bar: a
        compaction can be over before the second sample lands. So the interval
        is chosen by what the LAST probe found —

          - a live activity ⇒ every few ticks, because its numbers are the
            reason to publish again at all;
          - the agent working with no activity known ⇒ a middle cadence, which
            is what discovers an AUTOMATIC compaction (nothing announces it,
            and the user never typed a command we could hang a re-detect on);
          - idle ⇒ the original 30s, plus one immediate probe on the working
            → ready edge so a newly painted background-shell footer is not
            hidden until the next steady sample.

        Only followed sessions probe at all, and only while a client is
        subscribed, so the faster tiers are bounded by what is actually on
        screen in front of someone.
        */
        let transcript_pager_open = published_notice
            .as_ref()
            .and_then(|notice| notice.dialog.as_ref())
            .is_some_and(|dialog| {
                dialog.id == crate::session_chat_codex_pager::CODEX_TRANSCRIPT_PAGER_ID
            });
        // An open side question streams its answer and has its clipped window read whole in the
        // background; both land only through a probe.
        let side_question_open = published_notice
            .as_ref()
            .and_then(|notice| notice.dialog.as_ref())
            .is_some_and(|dialog| dialog.side_question.is_some());
        let probe_interval_ticks = if published_activity.is_some()
            || fleet_is_working(published_fleet.as_ref())
            || transcript_pager_open
            || side_question_open
        {
            crate::session_chat_options::SESSION_CHAT_ACTIVITY_RECONCILE_INTERVAL_TICKS
        } else if published_working {
            crate::session_chat_options::SESSION_CHAT_WORKING_RECONCILE_INTERVAL_TICKS
        } else {
            crate::session_chat_options::SESSION_CHAT_OPTION_RECONCILE_INTERVAL_TICKS
        };
        let activity_command_probe_due = activity_command_probe_ticks > 0;
        activity_command_probe_ticks = activity_command_probe_ticks.saturating_sub(1);
        /*
        CDXC:AgentScreenDetection 2026-09-03 WHY: Claude re-runs its statusLine command
        within 300ms of a model, effort, compaction or permission-mode change,
        and the Ghostex script stores the payload. A changed file is the one
        signal that says "the pills are stale right now", so it is a probe on
        its own, whatever tier the loop is in.
        */
        let statusline_changed = config
            .options_change_watch
            .as_ref()
            .is_some_and(|watch| watch(identity.agent_session_id.as_deref()));
        // The faster tiers already probe every tick; only the idle one waits.
        let screen_changed = probe_interval_ticks > 1
            && published_state_valid
            && config.options_reader.is_some()
            && crate::session_chat_screen_watch::session_chat_screen_changed(
                config.screen_change_watch.as_ref(),
            )
            .await;
        let periodic_probe_due = became_ready
            || activity_command_probe_due
            || statusline_changed
            || screen_changed
            || reconcile_ticks.is_multiple_of(probe_interval_ticks);
        if published_state_valid
            && config.options_reader.is_some()
            && (startup_probe_due || periodic_probe_due)
        {
            let reader = config.options_reader.clone();
            /*
            CDXC:AgentScreenDetection 2026-08-24:
            Awaited inline on the reconcile loop, so a capture that never
            answers used to stall the follower forever while the transcript
            grew. Missing the deadline means this pass simply publishes
            nothing and changes no published state — the next probe tick
            tries again.
            */
            let probe = tokio::time::timeout(
                STEADY_OPTION_DETECTION_DEADLINE,
                tokio::task::spawn_blocking(move || {
                    reader
                        .map(|reader| {
                            reader(crate::session_chat_options::SessionChatOptionsReadMode::Refresh)
                        })
                        .unwrap_or_default()
                }),
            )
            .await;
            if let Ok(Ok(mut detection)) = probe {
                file_state
                    .incremental
                    .codex_stats
                    .apply(&mut detection.options);
                // Detection can project a fleet/compaction transition into the shared status.
                // Read it before emitting this fleet so the same frame carries its working truth.
                // CDXC:SessionChat 2026-10-06 WHY: one fresh read after the capture serves both the working flag and the prompt. The prompt used to come from the read taken before the capture while working came from this one, so a question its hook retired DURING the capture went out again on the working change (observed: the answered card's question re-published for a second until its tool result landed).
                let probe_live = read_live_state();
                let detected_working = probe_live.working;
                let working_changed = detected_working != published_working;
                published_working = detected_working;
                if !published_working
                    && !detection
                        .activity
                        .as_ref()
                        .is_some_and(|activity| activity.remains_live_when_ready())
                {
                    detection.activity = None;
                }
                let options_changed = detection
                    .options
                    .as_ref()
                    .is_some_and(|detected| !detected.same_selection(published_options.as_ref()));
                let notice_changed = detection.captured
                    && !crate::session_chat_notice::same_session_chat_terminal_notice(
                        detection.notice.as_ref(),
                        published_notice.as_ref(),
                    );
                // Same capture rule as the notice: only a WHOLE capture proves
                // the progress line is gone, so a capped read leaves the row
                // standing.
                let activity_changed = detection.captured
                    && !crate::session_chat_terminal_activity::same_session_chat_terminal_activity(
                        detection.activity.as_ref(),
                        published_activity.as_ref(),
                    );
                /*
                CDXC:AgentScreenDetection 2026-08-22: the first successful
                capture is publishable on its own, even when it changed
                nothing. An agent whose screen names no model detects nothing
                forever, and without this the composer would never hear that
                detection HAD run and would hold its loading skeleton for the
                life of the session.
                */
                // Both providers read child lifecycles. Unreadable evidence publishes
                // an unavailable roster so local clocks cannot impersonate live work.
                let fleet_changed = detection.fleet_observed
                    && !crate::session_chat_agent_fleet::same_session_chat_agent_fleet(
                        detection.fleet.as_ref(),
                        published_fleet.as_ref(),
                    );
                // CDXC:SessionChat 2026-09-03: no capture gate, the
                // store on disk is authoritative whether or not the screen read.
                let tasks_changed = !crate::session_chat_agent_tasks::same_session_chat_agent_tasks(
                    detection.tasks.as_ref(),
                    published_tasks.as_ref(),
                );
                let workflows_changed =
                    !crate::session_chat_claude_workflow_cards::same_session_chat_workflows(
                        detection.workflows.as_ref(),
                        published_workflows.as_ref(),
                    );
                transcript_prompt.observe_stored(probe_live.prompt.as_ref());
                let resolved_prompt =
                    resolve_session_chat_prompt(probe_live.prompt.clone(), &transcript_prompt);
                // The hook and transcript half needs no capture; without one, only a screen
                // prompt (which never carries a call id) is kept as published.
                let detected_prompt = if detection.captured {
                    resolved_prompt.or_else(|| detection.prompt.clone())
                } else {
                    resolved_prompt.or_else(|| {
                        published_prompt
                            .clone()
                            .filter(|prompt| prompt.tool_use_id().is_none())
                    })
                };
                let prompt_changed = detected_prompt != published_prompt;
                let probed_changed = detection.attempted && !published_screen_probed;
                if options_changed
                    || working_changed
                    || notice_changed
                    || activity_changed
                    || fleet_changed
                    || tasks_changed
                    || workflows_changed
                    || prompt_changed
                    || probed_changed
                {
                    if options_changed {
                        published_options = detection.options;
                    }
                    if notice_changed {
                        published_notice = detection.notice;
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
                        published_prompt = detected_prompt;
                    }
                    published_screen_probed = published_screen_probed || detection.attempted;
                    emit_state_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        if published_working {
                            SessionChatStatus::Working
                        } else {
                            SessionChatStatus::Ready
                        },
                        published_prompt.as_ref(),
                        Some(published_working),
                        published_options.as_ref(),
                        SessionChatScreenState {
                            prompt: None,
                            notice: published_notice.as_ref(),
                            activity: published_activity.as_ref(),
                            fleet: published_fleet.as_ref(),
                            tasks: published_tasks.as_ref(),
                            workflows: published_workflows.as_ref(),
                            probed: published_screen_probed,
                        },
                    );
                }
            }
        }

        /*
        CDXC:SessionChat 2026-08-01:
        Stale-identity guard. `/clear` and `resume` make the agent start a NEW
        transcript file while the old one stays on disk, so the follower keeps
        tailing a file that will never grow again and the chat freezes at the
        switch point — with no Missing outcome to recover from. When the tailed
        file has been silent, re-derive the path from the session's CURRENT
        identity; a different file is treated exactly like a content
        replacement.

        CDXC:SessionIdentity 2026-08-02:
        The hook-driven re-resolution above only runs while hooks report the
        session as `working`. The case successor detection must recover from is
        precisely the one where hooks never fire at all (a background-job
        continuation writes a NEW transcript and nothing updates the registry),
        so `working` cannot gate it. It gets its own, slower cadence instead:
        every SUCCESSOR_SCAN_INTERVAL while the tailed file stays silent.
        */
        // Codex is in scope too: `codex fork` changes the session id
        // (CDXC:SessionIdentity 2026-08-24).
        let successor_scan_due = matches!(
            transcript_agent,
            SessionChatTranscriptAgent::Claude | SessionChatTranscriptAgent::Codex
        ) && config.successor_hooks.is_some()
            && last_successor_scan.elapsed() >= config.tuning.successor_scan_interval;
        if last_transcript_change.elapsed() >= config.tuning.stale_transcript_idle
            && ((published_working
                && last_staleness_check.elapsed() >= config.tuning.stale_transcript_idle)
                || successor_scan_due)
        {
            last_staleness_check = std::time::Instant::now();
            identity.adopt(live);
            let agent_session_id = identity.agent_session_id.clone();
            let agent_session_path = identity.agent_session_path.clone();
            let re_resolved = tokio::task::spawn_blocking(move || {
                resolve_session_chat_transcript_path(
                    transcript_agent,
                    agent_session_id.as_deref(),
                    agent_session_path.as_deref(),
                )
            })
            .await
            .ok()
            .flatten();
            if let Some(next_path) = re_resolved {
                if Some(&next_path) != resolved.as_ref() {
                    resolved = Some(next_path);
                    file_state = FollowerFileState::new();
                    transcript_prompt = SessionChatTranscriptPromptState::default();
                    epoch = stream.begin_generation();
                    want_snapshot = true;
                    published_state_valid = false;
                    last_transcript_change = std::time::Instant::now();
                    continue;
                }
                /*
                CDXC:SessionIdentity 2026-08-02:
                Re-resolution landed on the SAME file, so the registry identity
                itself is stale. Look for a transcript that proves it continues
                this one and re-bind the session to it.
                */
                if successor_scan_due {
                    last_successor_scan = std::time::Instant::now();
                    if let Some(adopted) = detect_and_adopt_successor_transcript(
                        transcript_agent,
                        &config,
                        &next_path,
                        identity.agent_session_id.as_deref(),
                        &mut logged_successor_ambiguity,
                    )
                    .await
                    {
                        identity.agent_session_id = Some(adopted.agent_session_id.clone());
                        identity.agent_session_path = Some(adopted.agent_session_path.clone());
                        config.agent_session_id = Some(adopted.agent_session_id);
                        config.agent_session_path = Some(adopted.agent_session_path.clone());
                        resolved = Some(PathBuf::from(&adopted.agent_session_path));
                        file_state = FollowerFileState::new();
                        transcript_prompt = SessionChatTranscriptPromptState::default();
                        epoch = stream.begin_generation();
                        want_snapshot = true;
                        published_state_valid = false;
                        last_transcript_change = std::time::Instant::now();
                        continue;
                    }
                }
            }
        }

        heartbeat.park();
        tokio::select! {
            _ = tokio::time::sleep(config.tuning.reconcile_interval) => {}
            _ = resnapshot.notified() => {
                epoch = stream.begin_generation();
                want_snapshot = true;
            }
        }
        heartbeat.unpark();
    }
}

/// CDXC:AgentScreenDetection 2026-10-23 WHY:
/// The fast probe tiers used to key on a published fleet existing at all. Claude keeps idle resident children in its footer after their work ends, so a session that had ever run a subagent stayed on the 1s tier forever and every probe re-read every child transcript. Only a roster with a working child needs the per-second clock; an idle roster is still renewed on the idle tier (and on any screen change) well inside its 90s lease, and it stays published, so the strip does not change.
fn fleet_is_working(
    fleet: Option<&crate::session_chat_agent_fleet::SessionChatAgentFleet>,
) -> bool {
    fleet.is_some_and(|fleet| fleet.is_working())
}

/// Identity the follower is currently tailing. Seeded from the spawn config and
/// refreshed from the live session so a hook update that did not respawn the
/// task still reaches the resolver.
pub(crate) struct SessionChatFollowerIdentity {
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
}

impl SessionChatFollowerIdentity {
    fn adopt(&mut self, live: SessionChatLiveState) {
        let session_changed =
            live.agent_session_id.is_some() && live.agent_session_id != self.agent_session_id;
        if live.agent_session_id.is_some() {
            self.agent_session_id = live.agent_session_id;
        }
        if session_changed || live.agent_session_path.is_some() {
            self.agent_session_path = live.agent_session_path;
        }
    }
}

/// Resolves once the agent's statusline payload changes, so the resolve-poll sleep ends as soon as
/// the payload that names a new chat's model and effort lands instead of at the next poll. Never
/// resolves for an agent without a statusline watch.
async fn statusline_payload_written(
    watch: Option<&crate::session_chat_options::SessionChatOptionsChangeWatch>,
    agent_session_id: Option<&str>,
) {
    let Some(watch) = watch else {
        return std::future::pending().await;
    };
    loop {
        tokio::time::sleep(crate::session_chat::STATUSLINE_WATCH_POLL).await;
        if watch(agent_session_id) {
            return;
        }
    }
}
