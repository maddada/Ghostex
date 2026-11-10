use super::*;

/*
CDXC:AgentScreenDetection 2026-08-01:
Model/effort detection reads structured transcript metadata plus the session's
zmx scrollback. The latter costs one short-lived process, so the combined read
is NEVER done per frame or per long-poll tick.
Every trigger goes through this 5s-TTL per-session cache: chat reads, the
+2s/+6s probes after a dispatched `/model`//`/effort`//`/fast`, and the
follower's ~30s piggyback. A miss is cached too — a session whose agent prints
no statusline must not re-spawn `zmx history` on every read. Detection is
deliberately absent from resolve_session_chat_read_state's fingerprint: hashing
it would make each 500ms long-poll tick spawn a process.

CDXC:AgentScreenDetection 2026-08-19: the SAME capture is classified for
terminal-state notices (login expired, trust dialog, usage limit, a crashed
CLI), so the cache entry carries both readings and neither costs an extra spawn.
*/
pub(crate) struct SessionChatOptionCacheEntry {
    pub(crate) fetched_at: std::time::Instant,
    /*
    The compaction state last projected into presentation. This is deliberately
    not identical to `value.activity`: Claude leaves completed status rows in
    terminal scrollback, while its hook-owned working transition tells chat
    those rows are no longer live. `None` means no trustworthy projection has
    been made yet (for example, a first capture failed while the hook still
    reported working).
    */
    pub(crate) projected_compacting: Option<bool>,
    pub(crate) projected_fleet: Option<bool>,
    pub(crate) projected_monitor: Option<bool>,
    /// First capture that was settle-eligible except for its missing model —
    /// the anchor `SESSION_CHAT_OPTION_MODEL_SETTLE_GRACE` counts from. Cleared
    /// the moment a model (or a screen-owning notice) shows up.
    pub(crate) model_grace_started: Option<std::time::Instant>,
    /*
    CDXC:AgentScreenDetection 2026-09-03:
    The last screen notice that stopped classifying, with the instant it left.
    Claude Code's Ink redraws and a capture that lands mid-repaint make a banner
    (the usage-limit line) miss a probe every so often, so the cached notice
    flipped to None and the next probe minted a fresh `detectedAt` — which is
    the client's dismissal key, so a card the user had closed popped back up
    every few seconds. A re-detection that says the same thing within
    `SESSION_CHAT_NOTICE_REAPPEAR_GRACE` of the notice leaving is the same
    instance and inherits its `detectedAt`; only a longer absence makes the
    same words a new event.
    */
    pub(crate) retired_notice: Option<(
        crate::session_chat_notice::SessionChatTerminalNotice,
        std::time::Instant,
    )>,
    pub(crate) value: crate::session_chat_options::SessionChatTerminalDetection,
}

/// How long a screen notice that stopped classifying still counts as the same
/// instance when the identical words come back; see
/// `SessionChatOptionCacheEntry::retired_notice`.
pub(crate) const SESSION_CHAT_NOTICE_REAPPEAR_GRACE: std::time::Duration =
    std::time::Duration::from_secs(10 * 60);

/// Where the Ghostex agent hooks (and the Claude statusline script) keep their
/// per-session state — the same resolution the installer bakes into them.
pub(crate) fn session_chat_hook_state_directory(paths: &GxserverPaths) -> std::path::PathBuf {
    crate::agent_hooks::config::HookPaths::from_paths(paths).hook_state_directory
}

#[derive(Clone)]
pub(crate) struct SessionChatOptionDetector {
    cache: Arc<Mutex<HashMap<String, SessionChatOptionCacheEntry>>>,
    recovery: Arc<crate::accounts::recovery::RecoverySignals>,
    compacting_publisher: crate::session_chat_compacting::SessionChatCompactingPublisher,
    followers: Arc<Mutex<HashMap<String, SessionChatFollowerEntry>>>,
    event_hub: GxserverEventHub,
    paths: GxserverPaths,
    server_id: String,
    /// For the reconcile of an agent seen gone (session_chat_agent_gone_activity.rs), which writes
    /// through the agent endpoint and its follow-ups.
    app_state: AppState,
}

impl SessionChatOptionDetector {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            cache: state.session_chat_option_cache.clone(),
            recovery: state.accounts.recovery.clone(),
            compacting_publisher:
                crate::session_chat_compacting::SessionChatCompactingPublisher::new(state),
            followers: state.session_chat_followers.clone(),
            event_hub: state.event_hub.clone(),
            paths: state.paths.clone(),
            server_id: state.metadata.server_id.clone(),
            app_state: state.clone(),
        }
    }

    /// Last known value with no process spawn. Used by frames that must stay
    /// free (snapshot/replaced).
    pub(crate) fn cached(
        &self,
        project_id: &str,
        session_id: &str,
    ) -> crate::session_chat_options::SessionChatTerminalDetection {
        let key = session_observer_key(project_id, session_id);
        let mut detected = self
            .cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&key).map(|entry| entry.value.clone()))
            .unwrap_or_default();
        detected.notice = detected.notice.filter(|notice| {
            crate::session_chat_notice_progress::visible(project_id, session_id, notice)
                && !crate::session_chat_notice::session_chat_notice_hidden(
                    project_id, session_id, notice,
                )
        });
        detected
    }

    /// BLOCKING: refreshes through the TTL (`force` bypasses it).
    pub(crate) fn detect_blocking(
        &self,
        project_id: &str,
        session_id: &str,
        agent: Option<&str>,
        force: bool,
    ) -> crate::session_chat_options::SessionChatTerminalDetection {
        // CDXC:SessionChat 2026-08-26: same two-door gate the
        // funnel itself uses, restated here so an agent with only a composer
        // signature is not turned away before the cache is even consulted.
        if crate::session_chat_options::session_chat_option_agent(agent).is_none()
            && !crate::session_chat_composer::has_session_chat_composer_signature(agent)
        {
            return crate::session_chat_options::SessionChatTerminalDetection::default();
        }
        let key = session_observer_key(project_id, session_id);
        if !force {
            if let Ok(cache) = self.cache.lock() {
                if let Some(entry) = cache.get(&key) {
                    // A startup miss is still waiting for the CLI to paint. Keep paced read clients on the same startup cadence as subscribed chat clients.
                    let ttl = if entry.value.attempted {
                        SESSION_CHAT_OPTION_CACHE_TTL
                    } else {
                        crate::session_chat::INITIAL_RESOLVE_POLL
                    };
                    if entry.fetched_at.elapsed() < ttl {
                        let mut detected = entry.value.clone();
                        detected.notice = detected.notice.filter(|notice| {
                            crate::session_chat_notice_progress::visible(
                                project_id, session_id, notice,
                            ) && !crate::session_chat_notice::session_chat_notice_hidden(
                                project_id, session_id, notice,
                            )
                        });
                        return detected;
                    }
                }
            }
        }
        let mut auto_trust_plan = None;
        let mut bypass_consent_plan = None;
        let mut feedback_card_plan = None;
        let mut detected = open_gxserver_database(&self.paths)
            .ok()
            .map(|db| {
                let repository = DomainRepository::new(&db, self.server_id.as_str());
                let detected = crate::session_chat_options::detect_session_chat_terminal_state(
                    &repository,
                    &session_chat_hook_state_directory(&self.paths),
                    project_id,
                    session_id,
                    agent,
                );
                auto_trust_plan = crate::session_chat_trust_memory::auto_trust_plan(
                    &repository,
                    project_id,
                    session_id,
                    detected.notice.as_ref(),
                );
                bypass_consent_plan =
                    crate::session_chat_claude_bypass_consent::notice_may_be_warning(
                        detected.notice.as_ref(),
                    )
                    .then(|| {
                        repository
                            .get_session(project_id, session_id)
                            .ok()
                            .flatten()
                    })
                    .flatten()
                    .filter(|session| {
                        crate::session_chat_claude_bypass_consent::funnel_may_answer(
                            session,
                            detected.notice.as_ref(),
                        )
                    })
                    .and_then(|session| crate::zmx::provider_zmx_session_name(&session).ok());
                feedback_card_plan = crate::session_chat_claude_feedback_card::dismiss_plan(
                    &repository,
                    project_id,
                    session_id,
                    &detected.composer,
                );
                detected
            })
            .unwrap_or_default();
        /*
        CDXC:AgentScreenDetection 2026-08-19:
        This is the ONE funnel every fresh capture goes through (the follower's
        probe, a read-triggered detect, the post-dispatch redetect), so it owns
        the two rules a single detection cannot state on its own:

        1. A capture that succeeded WHOLE and classified to nothing proves the
           screen is clean, which retires a watchdog verdict about screen state.
           `deliveryFailed` is exempt inside the store — it describes a lost
           message, not the current screen.
        2. A re-classification that says the same thing as the cached one is the
           SAME notice instance and keeps its `detectedAt`; see
           `SessionChatTerminalNotice::carry_forward_detected_at`.

        Neither publishes anything itself: every consumer already re-reads this
        cache (plus the watchdog store) and emits on change.
        */
        /*
        CDXC:SessionChat 2026-08-26: only an agent the NOTICE
        catalog covers can prove a screen clean. A composer-only agent always
        classifies to no notice — there are no rules for it — so retiring on
        that absence would clear a watchdog verdict on evidence that was never
        collected.
        */
        if detected.captured
            && detected.notice.is_none()
            && crate::session_chat_options::session_chat_option_agent(agent).is_some()
        {
            crate::session_chat_notice::retire_session_chat_watchdog_notice_on_clean_screen(
                project_id, session_id,
            );
        }
        // An agent back at the shell is one that restarts by hand: what it offered goes with it.
        if detected.captured
            && (detected.notice.as_ref().is_some_and(|notice| {
                notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_AGENT_EXITED
            }) || crate::session_chat_notice::session_chat_screen_ends_with_shell_prompt(
                &detected.composer.screen_tail.join("\n"),
            ))
        {
            crate::session_chat_mode_availability::forget_for_agent_restart(
                &self.compacting_publisher,
                project_id,
                session_id,
            );
        }
        crate::session_chat_mode_availability::note_screen_mode(
            project_id,
            session_id,
            detected
                .options
                .as_ref()
                .and_then(|options| options.selection.mode.as_ref())
                .map(|mode| mode.value.as_str()),
        );
        let mut compacting_transition: Option<Option<String>> = None;
        let mut fleet_transition: Option<Option<String>> = None;
        let mut monitor_transition: Option<Option<String>> = None;
        if let Ok(mut cache) = self.cache.lock() {
            let previous_compacting = cache.get(&key).and_then(|entry| entry.projected_compacting);
            let previous_fleet = cache.get(&key).and_then(|entry| entry.projected_fleet);
            let previous_monitor = cache.get(&key).and_then(|entry| entry.projected_monitor);
            let detected_monitor = if detected.captured {
                Some(
                    crate::session_chat_terminal_activity::is_session_chat_background_work_activity(
                        detected.activity.as_ref(),
                    ),
                )
            } else {
                previous_monitor
            };
            if detected_monitor != previous_monitor {
                if let Some(active) = detected_monitor {
                    monitor_transition = Some(
                        active.then(|| detected.activity.as_ref().unwrap().detected_at.clone()),
                    );
                }
            }
            if !detected.fleet_observed {
                detected.fleet = cache
                    .get(&key)
                    .and_then(|entry| entry.value.fleet.clone())
                    .map(|fleet| fleet.unavailable());
                detected.fleet_observed = true;
            }
            let detected_fleet = Some(
                detected
                    .fleet
                    .as_ref()
                    .is_some_and(|fleet| fleet.is_working()),
            );
            if detected_fleet != previous_fleet {
                if let Some(active) = detected_fleet {
                    fleet_transition =
                        Some(active.then(|| detected.fleet.as_ref().unwrap().detected_at.clone()));
                }
            }
            // A notice that left the screen recently still counts as the
            // instance to inherit from; see `retired_notice`.
            let recently_retired_notice = cache
                .get(&key)
                .and_then(|entry| entry.retired_notice.clone())
                .filter(|(_, retired_at)| {
                    retired_at.elapsed() < SESSION_CHAT_NOTICE_REAPPEAR_GRACE
                });
            let cached_notice = cache.get(&key).and_then(|entry| entry.value.notice.clone());
            let retired_notice = match detected.notice.as_mut() {
                Some(notice) => {
                    notice.carry_forward_detected_at(
                        cached_notice
                            .as_ref()
                            .or(recently_retired_notice.as_ref().map(|(notice, _)| notice)),
                    );
                    None
                }
                None => cached_notice
                    .map(|notice| (notice, std::time::Instant::now()))
                    .or(recently_retired_notice),
            };
            /*
            CDXC:AgentScreenDetection 2026-08-22: same instance-not-sample
            rule, and load-bearing here — the client anchors its elapsed clock to
            `detectedAt`, so re-minting it on every probe would peg the timer at
            zero for the whole run.
            */
            let cached_activity = cache
                .get(&key)
                .and_then(|entry| entry.value.activity.as_ref());
            // CDXC:AgentScreenDetection 2026-09-11 WHY:
            // An `agent-stream` probe is one grid of a message that may already have scrolled; stitching it onto the rows earlier probes accumulated needs the previous value, and this cache is the one place every fresh capture passes through.
            // A capture that failed or lost its tail says nothing about the message, so the accumulated stream stands until a whole capture replaces it; letting the miss overwrite it tore the stream at its next headless sample (seen once per reply in testing, when a 1s capture came back empty).
            if !detected.captured && detected.activity.is_none() {
                if let Some(stream) = cached_activity.filter(|activity| {
                    activity.kind
                        == crate::session_chat_terminal_activity::SESSION_CHAT_ACTIVITY_AGENT_STREAM
                }) {
                    detected.activity = Some(stream.clone());
                }
            }
            if detected.activity.as_ref().is_some_and(|activity| {
                activity.kind
                    == crate::session_chat_terminal_activity::SESSION_CHAT_ACTIVITY_AGENT_STREAM
            }) {
                let mut activity = detected.activity.take().unwrap();
                if crate::session_chat_terminal_activity::merge_agent_stream(
                    &mut activity,
                    cached_activity,
                ) {
                    detected.activity = Some(activity);
                }
            }
            if let Some(activity) = detected.activity.as_mut() {
                activity.carry_forward_detected_at(cached_activity);
            }
            /*
            CDXC:AgentScreenDetection 2026-08-23: deliberately NOT carried
            forward, unlike the notice and the activity row above. A fleet's
            `detectedAt` is the anchor its per-row clocks count from, so it has
            to stay paired with the seconds it was read beside; giving a fresh
            reading an older anchor would make every client count that interval
            twice. Holding a fleet still is `same_fleet`'s job.
            */
            /*
            CDXC:AgentScreenDetection (settled 2026-08-30): the model grace.
            The pure detector settles on any recognized chrome, but Claude's
            permission-mode footer and composer paint seconds before the async
            statusline that names the model. For an agent whose grammar CAN
            name a model, an otherwise-settled capture with no model stays
            unsettled for `SESSION_CHAT_OPTION_MODEL_SETTLE_GRACE` from the
            first such capture, so the model pill keeps its skeleton until the
            statusline lands — or until the grace decides no statusline is
            coming. A screen-owning notice (trust dialog, expired login) is
            exempt: the model cannot render behind it, and that state can hold
            indefinitely, so it settles at once. Lives here, not in the pure
            function, because the anchor needs the per-session cache.
            */
            let model_missing = detected
                .options
                .as_ref()
                .is_none_or(|options| options.selection.model.is_none());
            let mut model_grace_started = None;
            if detected.attempted
                && model_missing
                && detected.notice.is_none()
                && crate::session_chat_options::session_chat_option_agent(agent).is_some()
            {
                let started = cache
                    .get(&key)
                    .and_then(|entry| entry.model_grace_started)
                    .unwrap_or_else(std::time::Instant::now);
                if started.elapsed()
                    < crate::session_chat_options::SESSION_CHAT_OPTION_MODEL_SETTLE_GRACE
                {
                    detected.attempted = false;
                }
                model_grace_started = Some(started);
            }
            /*
            CDXC:AgentScreenDetection 2026-09-02:
            The screen owns this marker outright. It used to be forced false
            whenever the hooks said idle, on the theory that a finished
            compaction's row lingers in scrollback like a `⏺` status does — but
            Claude repaints the compacting row in place and replaces it with
            its `Compacted` line, so a whole capture is both the start and the
            end evidence, and the hook gate only hid compactions the hooks
            never learned about (a `/compact` typed in the terminal). Only a
            whole capture may change the verdict; a failed/capped capture
            preserves the last safe state.
            */
            let detected_compacting = if detected.captured {
                Some(
                    crate::session_chat_terminal_activity::is_session_chat_compacting_activity(
                        detected.activity.as_ref(),
                    ),
                )
            } else {
                previous_compacting
            };
            cache.insert(
                key,
                SessionChatOptionCacheEntry {
                    fetched_at: std::time::Instant::now(),
                    projected_compacting: detected_compacting,
                    projected_fleet: detected_fleet,
                    projected_monitor: detected_monitor,
                    model_grace_started,
                    retired_notice,
                    value: detected.clone(),
                },
            );
            if detected_compacting != previous_compacting {
                if let Some(detected_compacting) = detected_compacting {
                    compacting_transition = Some(
                        detected_compacting
                            .then(|| {
                                detected
                                    .activity
                                    .as_ref()
                                    .map(|activity| activity.detected_at.clone())
                            })
                            .flatten(),
                    );
                }
            }
        }
        if let Some(detected_at) = compacting_transition {
            self.compacting_publisher
                .publish(project_id, session_id, detected_at.as_deref());
        }
        if let Some(detected_at) = fleet_transition {
            self.compacting_publisher
                .publish_fleet(project_id, session_id, detected_at.as_deref());
        }
        if let Some(detected_at) = monitor_transition {
            self.compacting_publisher.publish_monitor(
                project_id,
                session_id,
                detected_at.as_deref(),
            );
        }
        if let Some(notice) = detected
            .notice
            .as_ref()
            .filter(|notice| crate::accounts::recovery::retryable(notice) || notice.sign_in_error())
        {
            if let Ok(db) = open_gxserver_database(&self.paths) {
                let repository = DomainRepository::new(&db, self.server_id.as_str());
                crate::session_chat_notice_progress::refresh(
                    &repository,
                    project_id,
                    session_id,
                    agent,
                    notice,
                );
            }
        }
        // An agent gone with no notice: its stored working claim ends from here, the one place every
        // fresh capture passes through, on every probe that still reads it gone.
        if let Some(generation) = detected
            .agent_gone
            .then(|| {
                crate::session_chat_start_failure::agent_gone_generation(project_id, session_id)
            })
            .flatten()
        {
            crate::session_chat_agent_gone_activity::dispatch_gone_agent_reconcile(
                self.app_state.clone(),
                project_id,
                session_id,
                generation,
            );
            // And its approval or question card, which nothing can answer any more.
            crate::session_chat_agent_gone_prompt::dispatch_gone_agent_prompt_retire(
                self.app_state.clone(),
                project_id,
                session_id,
                generation,
            );
        }
        // A trust prompt on a remembered folder is answered from here, the one
        // place every fresh capture passes through, so it is accepted whichever
        // probe first sees it (session_chat_trust_memory.rs).
        if let Some(plan) = auto_trust_plan {
            if crate::session_chat_trust_memory::claim_auto_trust_attempt(project_id, session_id) {
                crate::session_chat_trust_memory::dispatch_auto_trust(
                    self.clone(),
                    plan,
                    project_id,
                    session_id,
                    agent,
                );
            }
        }
        // Claude's Bypass Permissions warning on a session started in bypass is accepted from here
        // for the same reason (session_chat_claude_bypass_consent.rs).
        if let Some(zmx_name) = bypass_consent_plan {
            crate::session_chat_claude_bypass_consent::dispatch_bypass_consent(
                self.clone(),
                project_id,
                session_id,
                zmx_name,
                agent,
            );
        }
        // Claude's self-drafted feedback card is closed from here for the same reason
        // (session_chat_claude_feedback_card.rs).
        if let Some(plan) = feedback_card_plan {
            if crate::session_chat_claude_feedback_card::claim_dismissal(project_id, session_id) {
                crate::session_chat_claude_feedback_card::dispatch_dismissal(
                    self.clone(),
                    plan,
                    project_id,
                    session_id,
                    agent,
                );
            }
        }
        // The cache above keeps the raw notice so a repaint keeps its identity; callers only ever see the visible one, and the account-switch suppression and a remembered folder's auto-trust are part of visibility (see `session_chat_notice_hidden`).
        detected.notice = detected.notice.filter(|notice| {
            crate::session_chat_notice_progress::visible(project_id, session_id, notice)
                && !crate::session_chat_notice::session_chat_notice_hidden(
                    project_id, session_id, notice,
                )
        });
        if detected.captured {
            self.recovery
                .observe(project_id, session_id, detected.notice.as_ref());
        }
        crate::agent_model_pins::observe_detection(
            &self.paths,
            self.server_id.as_str(),
            project_id,
            session_id,
            agent,
            &detected,
        );
        detected
    }

    /// Republish the cached screen state to this session's chat followers,
    /// for a server-side writer that just changed the screen and re-probed.
    pub(crate) fn publish_screen_state(&self, project_id: &str, session_id: &str) {
        publish_cached_session_chat_screen_state(
            &self.followers,
            &self.event_hub,
            &self.paths,
            &self.server_id,
            &self.cache,
            project_id,
            session_id,
        );
    }

    /// Async handlers must not block the executor on a process spawn.
    pub(crate) async fn detect(
        &self,
        project_id: &str,
        session_id: &str,
        agent: Option<&str>,
        force: bool,
    ) -> crate::session_chat_options::SessionChatTerminalDetection {
        let detector = self.clone();
        let project_id = project_id.to_string();
        let session_id = session_id.to_string();
        let agent = agent.map(str::to_string);
        tokio::task::spawn_blocking(move || {
            detector.detect_blocking(&project_id, &session_id, agent.as_deref(), force)
        })
        .await
        .unwrap_or_default()
    }
}
