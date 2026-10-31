use super::wait_reason::{
    stuck_input_box_text, stuck_notification_body, HeadWait, InputBoxStep, WaitReason,
    INPUT_BOX_LAST_RESORT_AFTER_MS, WATCHDOG_AFTER_MS,
};
use super::*;

const SESSION_CHAT_QUEUE_TICK_SECONDS: u64 = 1;

/// Wakes the scheduler for a tick now instead of at the next second (`kick_session_chat_queue`).
static SESSION_CHAT_QUEUE_KICKED: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Runs a scheduler tick at once: a row just queued for an idle agent leaves without waiting for
/// the next one-second tick. Ticks are idempotent, so an extra one only re-reads the queue.
///
/// CDXC:SessionChat 2026-10-14 WHY:
/// Answers sent from a question page or form took about five seconds to reach the chat. Most of it was the queue's own pace: up to a second to the next tick, then a stability window that only started on the first tick to see the session stopped. The kick and the window measured from the agent's own idle stamp (`stability_window_elapsed`) take those out while keeping the two-second rule.
pub(crate) fn kick_session_chat_queue() {
    SESSION_CHAT_QUEUE_KICKED.notify_one();
}

/// How long a session must look stopped before the head row is released.
/// Tracked exactly like `delayed_sends`' `nonWorkingSinceAt`: it restarts the
/// instant the session looks busy again, so a blip between two tool calls can
/// never be mistaken for the end of a turn.
pub const SESSION_CHAT_QUEUE_STABILITY_MS: i64 = 2_000;

/// Tail window for the lifecycle probe. Big enough that a turn ending in a run
/// of tool_use / tool_result rows still exposes the boundary record that named
/// the turn, small enough that the read is a couple of reverse chunks.
const SESSION_CHAT_QUEUE_LIFECYCLE_TAIL_LIMIT: usize = 8;

/*
The session's currently resolved terminal notice (screen classification merged
with the send watchdog), which is state `server.rs` owns. Injected the same way
the sender and publisher are, so this module never learns about AppState.
*/
pub type SessionChatQueueNoticeReader =
    Arc<dyn Fn(&str, &str) -> Option<SessionChatTerminalNotice> + Send + Sync>;

/*
CDXC:SessionChat 2026-08-26:
The session's last known composer verdict, injected the same way. Read from the
cache only — a tick must never spawn a capture — so a session nobody has probed
reads `Unknown` and the queue behaves exactly as it did before this feature.
*/
pub type SessionChatQueueComposerReader =
    Arc<dyn Fn(&str, &str) -> SessionChatComposerReadiness + Send + Sync>;

/*
Refresh screen evidence for a queued session waiting for startup or compaction.
This keeps queued delivery progressing even after the chat client disconnects.
*/
pub type SessionChatQueueCompactingRefresher = Arc<dyn Fn(&str, &str, Option<&str>) + Send + Sync>;

/// Wakes a session holding the startup send `(project, session, prompt)` when its daemon is gone
/// (`wake_session_holding_startup_send` in session_chat_send_wake.rs); a no-op for a live daemon.
pub type SessionChatQueueWaker = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;

/// Posts `(project, session, body)` to the notification bell when the session's head row has been
/// stuck on a missing input box for five minutes (`INPUT_BOX_STUCK_AFTER_MS`), once per row.
pub type SessionChatQueueStuckNotifier = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;

#[derive(Default)]
struct SessionQueueGate {
    /// First moment this session looked stopped without looking busy since.
    /// `None` means the window has not started (or was just reset).
    stopped_since: Option<DateTime<Utc>>,
    transcript: SessionChatTranscriptGate,
    /// The head row held for a missing input box, since when, and whether that was logged.
    input_box_hold: Option<InputBoxHold>,
    /// How long the head row has waited and why, for the words it shows and the watchdog.
    head_wait: Option<HeadWait>,
    /// A blocking notice seen for the head row: its title and when a fresh read was asked for.
    notice_check: Option<(String, String, DateTime<Utc>)>,
}

/// A blocking notice fails the head row only when it is still on screen this long after a fresh
/// read was asked for (the read itself takes milliseconds), and the check is redone when it is
/// older than [`NOTICE_CHECK_STALE_MS`].
const NOTICE_CONFIRM_AFTER_MS: i64 = 3_000;
const NOTICE_CHECK_STALE_MS: i64 = 30_000;

/// Starts or reads the fresh-read check of a blocking notice on `prompt_id`; true once it has held
/// for [`NOTICE_CONFIRM_AFTER_MS`].
fn notice_check_confirmed(
    check: &mut Option<(String, String, DateTime<Utc>)>,
    prompt_id: &str,
    title: &str,
    now: DateTime<Utc>,
) -> bool {
    let since = check
        .as_ref()
        .filter(|(id, seen, since)| {
            id == prompt_id
                && seen == title
                && now.signed_duration_since(*since).num_milliseconds() < NOTICE_CHECK_STALE_MS
        })
        .map(|(_, _, since)| *since);
    match since {
        Some(since) => {
            now.signed_duration_since(since).num_milliseconds() >= NOTICE_CONFIRM_AFTER_MS
        }
        None => {
            *check = Some((prompt_id.to_string(), title.to_string(), now));
            false
        }
    }
}

/// What noting the head row's wait changed (`SessionChatQueueRuntime::note_wait`).
#[derive(Default)]
struct NotedWait {
    /// The row's words changed, so the queue is republished.
    changed: bool,
    /// The row has just become stuck on the input box: what to log and post to the bell.
    stuck: Option<StuckReport>,
}

struct StuckReport {
    /// The row's words.
    text: String,
    /// The bell notification's body.
    body: String,
    /// The cached screen the words quote.
    screen: Vec<String>,
}

/// One session's verdict for a tick (`SessionChatQueueRuntime::assess`).
enum Assessment {
    /// Nothing to do: no deliverable row.
    Skip,
    /// A delivery is in flight; the head row's wait clock and words stand until it settles.
    Delivering,
    Deliver(ReadyDelivery),
    /// A notice only the user can clear owns the screen: the head row is marked failed with it.
    Fail {
        prompt_id: String,
        reason: String,
    },
    /// A sleeping session's head row becomes a startup send and the session is woken.
    HoldAndWake(String),
    Hold {
        prompt_id: String,
        startup_send: bool,
        reason: WaitReason,
    },
}

/// How long a head row waits on a missing input box before the wait is logged with its screen.
const INPUT_BOX_HOLD_LOG_AFTER_MS: i64 = 60_000;

#[derive(Clone, Debug, PartialEq, Eq)]
struct InputBoxHold {
    prompt_id: String,
    since: DateTime<Utc>,
    logged: bool,
    last_resort_logged: bool,
}

/// Starts or continues `hold` for `prompt_id`. Answers how long the row has waited on the input
/// box, and true exactly once per row, when it has waited [`INPUT_BOX_HOLD_LOG_AFTER_MS`].
fn input_box_hold_log_due(
    hold: &mut Option<InputBoxHold>,
    prompt_id: &str,
    now: DateTime<Utc>,
) -> (i64, bool) {
    let hold = match hold {
        Some(hold) if hold.prompt_id == prompt_id => hold,
        _ => hold.insert(InputBoxHold {
            prompt_id: prompt_id.to_string(),
            since: now,
            logged: false,
            last_resort_logged: false,
        }),
    };
    let waited = now.signed_duration_since(hold.since).num_milliseconds();
    if hold.logged || waited < INPUT_BOX_HOLD_LOG_AFTER_MS {
        return (waited, false);
    }
    hold.logged = true;
    (waited, true)
}

/// CDXC:DelayedSend 2026-09-16 WHY:
/// Hooks and titles can look stopped before a turn finishes. Conditional delayed sends and queued prompts must both check the transcript before starting their stability window.
#[derive(Default)]
pub(crate) struct SessionChatTranscriptGate {
    /// Cache resolution by agent and conversation identity; resolving can scan agent homes.
    identity: String,
    path: Option<PathBuf>,
}

impl SessionChatTranscriptGate {
    pub(crate) fn is_working(&mut self, session: &Value) -> bool {
        self.lifecycle(session)
            .is_some_and(|lifecycle| lifecycle.state == SessionChatTurnLifecycleState::Working)
    }

    /// The newest turn's lifecycle in the session's transcript, `None` without one.
    pub(crate) fn lifecycle(
        &mut self,
        session: &Value,
    ) -> Option<crate::session_chat::SessionChatTurnLifecycle> {
        let agent = session_text(session, "agentId").or_else(|| runtime_text(session, "agentName"));
        let agent_icon = session
            .get("launchSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("icon"))
            .and_then(Value::as_str);
        let resolved_agent = agent
            .as_deref()
            .filter(|value| resolve_session_chat_transcript_agent(Some(value)).is_some())
            .or(agent_icon);
        let transcript_agent = resolve_session_chat_transcript_agent(resolved_agent)?;
        let agent_session_id = runtime_text(session, "agentSessionId");
        let agent_session_path = runtime_text(session, "agentSessionPath");
        let identity = format!(
            "{}|{}|{}",
            resolved_agent.unwrap_or_default(),
            agent_session_id.clone().unwrap_or_default(),
            agent_session_path.clone().unwrap_or_default(),
        );
        let cached = (self.identity == identity)
            .then(|| self.path.clone())
            .flatten()
            .filter(|path| path.is_file());
        let path = match cached {
            Some(path) => {
                // The Empryo chat is a mirror only a resolve or an open chat keeps current.
                if transcript_agent == crate::session_chat::SessionChatTranscriptAgent::Empryo {
                    crate::session_chat_empryo_mirror::sync_empryo_transcript_mirror_for_path(
                        &path,
                    );
                }
                Some(path)
            }
            None => resolve_session_chat_transcript_path(
                transcript_agent,
                agent_session_id.as_deref(),
                agent_session_path.as_deref(),
            ),
        };
        self.identity = identity;
        self.path = path.clone();
        // No transcript on disk yet: agent hooks are the only signal, and they already said idle.
        match read_session_chat_tail_page(
            transcript_agent,
            &path?,
            SESSION_CHAT_QUEUE_LIFECYCLE_TAIL_LIMIT,
            None,
        ) {
            Ok(SessionChatTailPage::Page { lifecycle, .. }) => lifecycle,
            _ => None,
        }
    }
}

struct ReadyDelivery {
    project_id: String,
    session_id: String,
    prompt_id: String,
    /// A message the user already sent, held until the agent takes input: it is put back for as
    /// long as the input box is missing (CDXC:SessionChat 2026-10-11 in session_chat_send_wake.rs).
    startup_send: bool,
    model_selection: Option<crate::session_chat_model_selection::PendingModelSelection>,
}

#[derive(Clone)]
pub struct SessionChatQueueRuntime {
    paths: GxserverPaths,
    server_id: String,
    sender_factory: SessionChatQueueSenderFactory,
    model_selector: crate::session_chat_model_selection::ModelSelectionSender,
    publisher_factory: SessionChatQueuePublisherFactory,
    notice_reader: SessionChatQueueNoticeReader,
    composer_reader: SessionChatQueueComposerReader,
    compacting_refresher: SessionChatQueueCompactingRefresher,
    waker: SessionChatQueueWaker,
    stuck_notifier: SessionChatQueueStuckNotifier,
    gates: Arc<Mutex<HashMap<String, SessionQueueGate>>>,
    composer_holds: crate::session_chat_queue_undelivered::SessionChatQueueComposerHolds,
    /// Sessions with a delivery in flight. The claim in
    /// `deliver_session_chat_queued_prompt` is the real guard; this only keeps
    /// the scheduler from stacking tasks for the same session every second
    /// while a slow send (draft handshake, resume picker) is still running.
    delivering: Arc<Mutex<HashSet<String>>>,
}

impl SessionChatQueueRuntime {
    pub fn new(
        paths: GxserverPaths,
        server_id: impl Into<String>,
        sender_factory: SessionChatQueueSenderFactory,
        model_selector: crate::session_chat_model_selection::ModelSelectionSender,
        publisher_factory: SessionChatQueuePublisherFactory,
        notice_reader: SessionChatQueueNoticeReader,
        composer_reader: SessionChatQueueComposerReader,
        compacting_refresher: SessionChatQueueCompactingRefresher,
        waker: SessionChatQueueWaker,
        stuck_notifier: SessionChatQueueStuckNotifier,
    ) -> Self {
        Self {
            paths,
            server_id: server_id.into(),
            sender_factory,
            model_selector,
            publisher_factory,
            notice_reader,
            composer_reader,
            compacting_refresher,
            waker,
            stuck_notifier,
            gates: Arc::new(Mutex::new(HashMap::new())),
            composer_holds: Default::default(),
            delivering: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn start(&self, mut shutdown_rx: broadcast::Receiver<()>) {
        /*
        Restart recovery is NOT done here: `recover_session_chat_queue_after_restart`
        already runs once at server start and is idempotent. Rows left in
        `sending` become `failed` there and are never re-sent, because the bytes
        may already have reached the agent.
        */
        let runtime = self.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_secs(SESSION_CHAT_QUEUE_TICK_SECONDS));
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => break,
                    _ = interval.tick() => runtime.run_tick().await,
                    _ = SESSION_CHAT_QUEUE_KICKED.notified() => runtime.run_tick().await,
                    _ = crate::session_chat_model_selection::selection_requested() => runtime.run_tick().await,
                }
            }
        });
    }

    async fn run_tick(&self) {
        // The readiness pass reads SQLite and transcripts (an Empryo transcript is rebuilt from
        // its raw log), so it runs on a blocking thread, never on an async worker.
        let runtime = self.clone();
        let Ok(deliveries) =
            tokio::task::spawn_blocking(move || runtime.collect_ready_deliveries()).await
        else {
            return;
        };
        for ready in deliveries {
            let key = session_queue_key(&ready.project_id, &ready.session_id);
            if !self.begin_delivery(&key) {
                continue;
            }
            let runtime = self.clone();
            tokio::spawn(async move {
                runtime.deliver(key, ready).await;
            });
        }
    }

    /*
    The whole readiness pass, synchronous so the SQLite connection is opened,
    used and dropped without ever crossing an await point.
    */
    fn collect_ready_deliveries(&self) -> Vec<ReadyDelivery> {
        let Ok(targets) = list_sessions_with_pending_queue(&self.paths) else {
            return Vec::new();
        };
        self.retain_gates(&targets);
        if targets.is_empty() {
            return Vec::new();
        }
        let Ok(db) = open_gxserver_database(&self.paths) else {
            return Vec::new();
        };
        let repository = DomainRepository::new(&db, self.server_id.as_str());
        let now = Utc::now();
        let generated_at = now.to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut ready: Vec<ReadyDelivery> = Vec::new();
        let mut blocked: Vec<(String, String, String, String)> = Vec::new();
        let mut to_wake: Vec<(String, String, String)> = Vec::new();
        let mut to_publish: Vec<(String, String)> = Vec::new();
        let mut to_notify: Vec<(String, String, StuckReport)> = Vec::new();

        for (project_id, session_id) in targets {
            let key = session_queue_key(&project_id, &session_id);
            let assessment = self.assess(
                &db,
                &repository,
                &project_id,
                &session_id,
                &key,
                now,
                &generated_at,
            );
            let held = match &assessment {
                Assessment::Hold {
                    prompt_id, reason, ..
                } => Some(Some((prompt_id.as_str(), reason.clone()))),
                Assessment::HoldAndWake(prompt_id) => {
                    Some(Some((prompt_id.as_str(), WaitReason::Asleep)))
                }
                Assessment::Skip | Assessment::Fail { .. } => Some(None),
                // A delivery in flight or about to start may put the row back (a held send), so
                // its wait clock, the ladder's clock and its words stand until the row is gone.
                Assessment::Deliver(_) | Assessment::Delivering => None,
            };
            if let Some(held) = held {
                let noted = self.note_wait(
                    &key,
                    held.as_ref().map(|(id, reason)| (*id, reason)),
                    now,
                    || (self.composer_reader)(&project_id, &session_id).screen_tail,
                );
                if noted.changed {
                    to_publish.push((project_id.clone(), session_id.clone()));
                }
                if let Some(stuck) = noted.stuck {
                    to_notify.push((project_id.clone(), session_id.clone(), stuck));
                }
            }
            match assessment {
                Assessment::Skip | Assessment::Delivering => {}
                Assessment::Deliver(delivery) => ready.push(delivery),
                Assessment::Fail { prompt_id, reason } => {
                    blocked.push((project_id, session_id, prompt_id, reason));
                }
                Assessment::HoldAndWake(prompt_id) => {
                    to_wake.push((project_id, session_id, prompt_id));
                }
                Assessment::Hold {
                    prompt_id,
                    startup_send,
                    reason,
                } => {
                    if let Some(delivery) = self.watchdog(
                        &repository,
                        &project_id,
                        &session_id,
                        &key,
                        &prompt_id,
                        startup_send,
                        &reason,
                        now,
                        &generated_at,
                    ) {
                        ready.push(delivery);
                    }
                }
            }
        }
        drop(repository);
        drop(db);

        for (project_id, session_id) in to_publish {
            (self.publisher_factory)(&project_id, &session_id)();
        }
        for (project_id, session_id, stuck) in to_notify {
            crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
                "sessionChatQueueStuckOnInputBox",
                &project_id,
                &session_id,
                &stuck.text,
                &stuck.screen,
            );
            (self.stuck_notifier)(&project_id, &session_id, &stuck.body);
        }
        for (project_id, session_id, prompt_id, reason) in blocked {
            if let Ok(snapshot) = fail_session_chat_queued_prompt(
                &self.paths,
                &project_id,
                &session_id,
                &prompt_id,
                &reason,
            ) {
                (self.publisher_factory)(&project_id, &session_id)();
                if let Some(prompt) = snapshot.queue.iter().find(|prompt| prompt.id == prompt_id) {
                    self.notify_sender(&project_id, &session_id, &prompt.text, &reason);
                }
            }
            self.reset_gate(&session_queue_key(&project_id, &session_id));
        }
        for (project_id, session_id, prompt_id) in to_wake {
            if crate::session_chat_queue::hold_queued_prompt_for_wake(
                &self.paths,
                &project_id,
                &session_id,
                &prompt_id,
            ) {
                (self.publisher_factory)(&project_id, &session_id)();
                // Seeds the wake check's backoff, so the next ticks do not wake it again at once.
                self.composer_holds.wake_check_due(
                    &session_queue_key(&project_id, &session_id),
                    &prompt_id,
                    now,
                );
                (self.waker)(&project_id, &session_id, &prompt_id);
            }
        }
        ready
    }

    /// One session's verdict for this tick: deliver its head row, fail it, wake the session, or
    /// hold the row for a reason the row shows once it has waited a minute.
    #[allow(clippy::too_many_arguments)]
    fn assess(
        &self,
        db: &rusqlite::Connection,
        repository: &DomainRepository<'_>,
        project_id: &str,
        session_id: &str,
        key: &str,
        now: DateTime<Utc>,
        generated_at: &str,
    ) -> Assessment {
        let deliver = |head: &crate::session_chat_queue::SessionChatQueuedPrompt| {
            Assessment::Deliver(ReadyDelivery {
                project_id: project_id.to_string(),
                session_id: session_id.to_string(),
                prompt_id: head.id.clone(),
                startup_send: head.startup_send,
                model_selection: None,
            })
        };
        let hold = |head: &crate::session_chat_queue::SessionChatQueuedPrompt,
                    reason: WaitReason| Assessment::Hold {
            prompt_id: head.id.clone(),
            startup_send: head.startup_send,
            reason,
        };
        if self.is_delivering(key) {
            // A send in flight is the busiest a session ever is.
            self.reset_gate(key);
            return Assessment::Delivering;
        }
        let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
            self.reset_gate(key);
            return Assessment::Skip;
        };
        /*
        A sleeping or stopped session keeps its queue rather than failing it:
        the rows drain once it is awake again. Writing into a dead provider
        would lose the text with nothing to show for it. A message the user already sent (a
        startup send) wakes it instead (CDXC:SessionChat 2026-10-12 in
        session_chat_send_wake.rs), and so does any other row at the head of a sleeping
        session's queue, which becomes a startup send first (CDXC:SessionChat 2026-10-29 in
        session_chat_send_wake.rs); a closed session's rows keep waiting.
        */
        let lifecycle = session_text(&session, "lifecycleState");
        if lifecycle.as_deref() != Some("running") {
            self.reset_gate(key);
            let snapshot = read_session_chat_queue_snapshot_with(db, project_id, session_id);
            let head = snapshot.deliverable_head();
            return match not_running_head(lifecycle.as_deref(), head) {
                NotRunningHead::CheckWake(id) => {
                    if self.composer_holds.wake_check_due(key, id, now) {
                        (self.waker)(project_id, session_id, id);
                    }
                    head.map_or(Assessment::Skip, |head| hold(head, WaitReason::Asleep))
                }
                NotRunningHead::HoldAndWake(id) => Assessment::HoldAndWake(id.to_string()),
                NotRunningHead::Wait => {
                    head.map_or(Assessment::Skip, |head| hold(head, WaitReason::Closed))
                }
            };
        }
        // CDXC:SessionChat 2026-09-05 DECISION:
        // User: model changes run during a turn whenever the CLI accepts them; only actual delivery failure keeps them pending.
        // Prompt activity, transcript and stability gates below do not apply to model selection. The serialized driver checks a fresh terminal screen.
        let snapshot = read_session_chat_queue_snapshot_with(db, project_id, session_id);
        if let Some(selection) = snapshot.pending_model_selection.as_ref() {
            let head = snapshot.deliverable_head();
            // A failed selection is never redelivered, and it keeps holding the prompts behind
            // it: they were written for the model it asked for (2026-09-27 decision in
            // session_chat_model_selection_alert.rs). Picking a model again releases them.
            if selection.state == "failed" {
                self.reset_gate(key);
                return head.map_or(Assessment::Skip, |head| {
                    hold(head, WaitReason::ModelChangeFailed)
                });
            }
            if selection.retry_at > now.timestamp_millis() {
                return head.map_or(Assessment::Skip, |head| hold(head, WaitReason::ModelChange));
            }
            return Assessment::Deliver(ReadyDelivery {
                project_id: project_id.to_string(),
                session_id: session_id.to_string(),
                prompt_id: String::new(),
                startup_send: false,
                model_selection: Some(selection.clone()),
            });
        }
        // CDXC:SessionChat 2026-09-09 SEE-ALSO:
        // sendSessionChatMessage accepts new-chat sends before the provider exists. Probe startup independently of activity and require positive input-box evidence before the first delivery.
        let Some(head) = snapshot.deliverable_head() else {
            self.reset_gate(key);
            return Assessment::Skip;
        };
        // CDXC:SessionChat 2026-09-25 WHY:
        // A send held for a session that was asleep or still starting (session_chat_send_wake.rs) waits exactly like a new chat's first message: nobody may be viewing the session, so the scheduler refreshes the screen itself and delivers the moment the input box appears. Waiting for the cached reading made a woken session take 40 seconds.
        let draft = crate::agents::session_is_draft(&session);
        let composer_agent = crate::session_chat_composer::session_chat_composer_agent_id(&session);
        let composer = (self.composer_reader)(project_id, session_id);
        // A draft whose Run on row picked a box has no input box to wait for: delivering its
        // first queued prompt starts the box (agents/draft_run_location.rs).
        let pending_box = crate::agentbox::pending_session_agentbox(&session).is_some();
        let awaiting_startup = (draft || head.startup_send) && !pending_box;
        let composer_ready =
            composer.state == crate::session_chat_composer::SessionChatComposerState::Ready;
        // A "running" row whose daemon died before its agent took the message is woken too.
        if awaiting_startup
            && !composer_ready
            && self.composer_holds.wake_check_due(key, &head.id, now)
        {
            (self.waker)(project_id, session_id, &head.id);
        }
        // A panel the send closes itself (Claude's rating survey, an Escape-safe offer) holds a
        // startup send no longer than an ordinary row: it goes on to the gates below, and out as
        // soon as the agent is idle.
        if awaiting_startup
            && crate::session_chat_composer::has_session_chat_composer_signature(
                composer_agent.as_deref(),
            )
            && !composer_ready
            && !composer.should_dismiss()
        {
            self.reset_gate(key);
            // The screen is read again on the startup send's backoff (every 2 s, then less
            // often), not every tick, so an agent that takes minutes to come up is not
            // captured once a second the whole time.
            if self.composer_holds.screen_read_due(key, &head.id, now) {
                (self.compacting_refresher)(project_id, session_id, composer_agent.as_deref());
            }
            // An agent that could not start says so on its row, not as an unreadable box.
            if let Some(notice) = (self.notice_reader)(project_id, session_id) {
                if notice.missing_login() {
                    return hold(head, WaitReason::MissingLogin);
                }
                if notice.start_failed() {
                    return hold(head, WaitReason::StartFailed);
                }
            }
            if draft
                || composer.screen_tail.is_empty()
                || crate::session_chat_send_wake::screen_shows_agent_starting(&composer.screen_tail)
            {
                return hold(head, WaitReason::Starting);
            }
            // An agent that is up but whose input box cannot be read: the input-box ladder. Its
            // last resort needs every other signal to say idle, since this branch runs before
            // the activity and transcript gates.
            let waited = self.hold_for_input_box(project_id, session_id, &head.id, &composer, now);
            if waited >= INPUT_BOX_LAST_RESORT_AFTER_MS
                && !self.composer_holds.waiting(key, &head.id, now)
                && self.idle_by_every_signal(key, &session, generated_at)
            {
                self.log_last_resort(project_id, session_id, &head.id, &composer);
                return deliver(head);
            }
            return hold(head, WaitReason::InputBox(InputBoxStep::after(waited)));
        }
        /*
        A compacting marker is written by the same whole zmx screen capture
        that feeds chat's progress card. Refresh only this marked state so
        a queued prompt can leave as soon as compaction disappears even if
        every client has disconnected. Failed/capped captures do not clear
        the marker, so uncertainty always holds the prompt safely.
        */
        if crate::session_chat_compacting::session_chat_compacting_detected_at(&session).is_some() {
            let agent = session_chat_agent_for_session(&session);
            (self.compacting_refresher)(project_id, session_id, agent.as_deref());
            self.reset_gate(key);
            return hold(head, WaitReason::Compacting);
        }
        /*
        "working" is obvious. "attention" is the load-bearing one: the agent
        is sitting on a permission/approval prompt, and a prompt delivered
        now becomes the ANSWER to it.

        CDXC:SessionChat 2026-09-04 DECISION:
        User: Claude's Stop now rings attention like Codex's, and queued
        prompts must keep draining unattended. The one attention the queue
        may deliver into is the finished turn a hook's Stop entered
        (`attentionSource: turnComplete`) with no question or approval card
        standing; that is exactly the "next stop" this clock waits for.
        Every other attention still holds the row.
        */
        if account_recovery_waiting(&session) {
            self.reset_gate(key);
            return hold(head, WaitReason::AccountRecovery);
        }
        if !session_chat_queue_activity_is_deliverable(&session, generated_at) {
            self.reset_gate(key);
            let reason = if presentation_activity(&session, generated_at) == "working" {
                WaitReason::Working
            } else {
                WaitReason::Answer
            };
            return hold(head, reason);
        }
        let startup_composer_ready = awaiting_startup && composer_ready;
        // CDXC:SessionChat 2026-09-26 WHY:
        // An agent that died mid-turn leaves its transcript ending in a tool result with no reply, which reads as Working forever. A freshly started CLI showing an empty input box cannot be mid-turn, so a startup send trusts the screen over that dead turn; checking the transcript here left a woken session's "continue" queued indefinitely.
        if !startup_composer_ready && self.transcript_lifecycle_is_working(key, &session) {
            self.reset_gate(key);
            return hold(head, WaitReason::Working);
        }
        if !startup_composer_ready
            && !self.stability_window_elapsed(key, now, agent_idle_since(&session))
        {
            return hold(head, WaitReason::Settling);
        }
        if self.composer_holds.waiting(key, &head.id, now) {
            return hold(head, WaitReason::Retrying);
        }
        /*
        A trust dialog, a first-run setup screen, an update modal, a usage
        limit waiting on a keypress, an expired login, the agent process
        gone, a delivery the watchdog could not prove: in every one of
        those the terminal does not pass a prompt to the model, and several
        of them consume it as the answer to what is on screen. Hold the row
        with the notice title as its reason so the stall is VISIBLE and
        retryable — a queue that silently waits forever is the failure mode
        of this rule, not its goal.

        Gating on severity was the original bug: the catalog rates a trust
        prompt `Warning` and onboarding `Info` precisely because the user
        is one keypress from continuing, which says nothing about whether a
        prompt sent meanwhile survives.
        */
        if let Some(notice) = (self.notice_reader)(project_id, session_id) {
            if crate::accounts::recovery::holds_queue(repository, &session, &notice) {
                self.reset_gate(key);
                return hold(head, WaitReason::AccountRecovery);
            }
            // An agent that could not start on an account without a saved login: the row waits
            // behind the card and goes out once the user's fix starts the agent (CDXC:SessionChat
            // 2026-10-14 in session_chat_agent_restart.rs).
            if notice.missing_login() {
                self.reset_gate(key);
                return hold(head, WaitReason::MissingLogin);
            }
            // An agent whose start command ended before it came up: the row waits behind the
            // card the same way (CDXC:SessionChat 2026-10-16 in session_chat_start_failure.rs).
            if notice.start_failed() {
                self.reset_gate(key);
                return hold(head, WaitReason::StartFailed);
            }
            // CDXC:SessionChat 2026-10-09 WHY: an exited agent is restarted by the send itself and the row waits for it (the 2026-10-09 DECISION in send_heal.rs), so the row is delivered instead of failed with "no longer running".
            if notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_AGENT_EXITED {
                return deliver(head);
            }
            if notice.blocks_queued_delivery() && !escape_closes_claude_panel(&composer, &notice) {
                // CDXC:SessionChat 2026-10-29 WHY: a cached notice never fails a row on its own. In the isolated repro, Claude's "Read outside the working directories" dialog was closed with Escape, the hooks said idle at once, and the cached screen still named the dialog, so the queued message was failed with the closed dialog's title. The screen is read fresh first and the row fails only if the notice is still there after that read.
                if !self.notice_confirmed(key, &head.id, &notice.title, now) {
                    (self.compacting_refresher)(project_id, session_id, composer_agent.as_deref());
                    return hold(head, WaitReason::Settling);
                }
                return Assessment::Fail {
                    prompt_id: head.id.clone(),
                    reason: notice.title.clone(),
                };
            }
        }
        /*
        CDXC:SessionChat 2026-08-26:
        No input box on screen ⇒ HOLD, and deliberately not the `blocked`
        treatment above. A blocking notice is a state only the user can
        leave — a trust dialog waits forever for a keypress — so failing the
        row makes the stall visible and retryable. A missing composer is the
        opposite: it is what a CLI looks like while it BOOTS, and it clears
        on its own within seconds. Burning the head row for that would fail
        every queued prompt on a session that was merely restarted, which is
        precisely the moment a queue exists to cover.

        Grok requires positive readiness; unmeasured agents retain their
        existing Unknown behavior.

        CDXC:SessionChat 2026-10-29 DECISION:
        User: "i came back to my pc all these were paused and waiting since yesterday.... and as soon as i switched to terminal on your session then things started flowing. this is crazy bad. please fix this." The coordinator (S90:P3lv0:G1pif) sat idle from 02:14 to 10:09 UTC with page answers, a message and thread reports queued. The head row was an ordinary queued row, and this gate read only the cached screen verdict, which (by everything logged) said Claude's input box was unreadable (Claude repaints only cells it thinks changed, so a frame damaged by a grid resize stays damaged on an idle Claude). Only a held startup send got a fresh look and the redraw (held_redraw.rs); no capture, redraw or write reached the session for eight hours, until the terminal view's resize repainted Claude.
        Asked "Should a queued message show in the chat why it is still waiting?" (Q752), the user answered "show reason" and added "and please self heal better...... this should never happen again". So a row this gate holds climbs a ladder, every step logged in session-chat-send-failures.jsonl (thresholds and reasons in wait_reason.rs): a cached verdict never holds a row on its own (the first tick it would, the screen is read fresh, then every 2 s easing to 15 s); at 20 s Claude gets a Ctrl+L (`sessionChatQueueRedrewClaude`, once a minute); at 45 s any agent gets a one-row resize and back, which is what the terminal view did (`sessionChatQueueResizedForRepaint`, every two minutes, repaint_nudge.rs); at a minute the row says why it waits and is logged (`sessionChatQueueHeldForInputBox`); at three minutes it goes to the send path anyway (`sessionChatQueueLastResort`), whose own fresh capture, repaint and dialog and question guards decide, and which marks it Not sent with the reason and Retry after two more minutes of finding no box. A startup send keeps waiting instead of Not sent (the 2026-10-11 DECISION in session_chat_send_wake.rs). Every reason a row waits for is shown on it after a minute, and a watchdog hands a row held five minutes by a gate that should have let it go, on a session idle by every signal, to the same last resort (`sessionChatQueueWatchdog`).
        */
        if !pending_box
            && composer.blocks_message_for(composer_agent.as_deref())
            && !composer.should_dismiss()
        {
            self.reset_gate(key);
            let waited = self.hold_for_input_box(project_id, session_id, &head.id, &composer, now);
            // A fresh look on the startup send's backoff; the refresher also asks an idle agent
            // whose box stays unreadable to repaint (held_redraw.rs, repaint_nudge.rs).
            if self.composer_holds.screen_read_due(key, &head.id, now) {
                (self.compacting_refresher)(project_id, session_id, composer_agent.as_deref());
            }
            // Every gate above has passed, so the agent is idle by every other signal.
            if waited >= INPUT_BOX_LAST_RESORT_AFTER_MS {
                self.log_last_resort(project_id, session_id, &head.id, &composer);
                return deliver(head);
            }
            return hold(head, WaitReason::InputBox(InputBoxStep::after(waited)));
        }
        deliver(head)
    }

    async fn deliver(&self, key: String, ready: ReadyDelivery) {
        if let Some(selection) = ready.model_selection {
            (self.model_selector)(ready.project_id, ready.session_id, selection).await;
            self.reset_gate(&key);
            self.finish_delivery(&key);
            return;
        }
        let sender = (self.sender_factory)(&ready.project_id, &ready.session_id);
        /*
        The shared claim → send → settle path. It deletes the row on success and
        marks it `failed` with the reason on error, so "Send now" and the
        scheduler can never both deliver the same row.
        */
        let may_hold =
            self.composer_holds
                .may_hold(&key, &ready.prompt_id, ready.startup_send, Utc::now());
        let delivered = deliver_session_chat_queued_prompt(
            &self.paths,
            &self.server_id,
            &ready.project_id,
            &ready.session_id,
            &ready.prompt_id,
            &sender,
            may_hold,
        )
        .await;
        if let Ok(delivery) = &delivered {
            (self.publisher_factory)(&ready.project_id, &ready.session_id)();
            if delivery.held {
                self.composer_holds
                    .record(&key, &ready.prompt_id, ready.startup_send, Utc::now());
            } else {
                self.composer_holds.clear(&key);
            }
            if let Some(reason) = delivery.error_message.as_deref() {
                crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
                    "sessionChatQueueNotSent",
                    &ready.project_id,
                    &ready.session_id,
                    reason,
                    &[],
                );
                if let Some(prompt) = delivery
                    .snapshot
                    .queue
                    .iter()
                    .find(|prompt| prompt.id == ready.prompt_id)
                {
                    self.notify_sender(&ready.project_id, &ready.session_id, &prompt.text, reason);
                }
            }
        }
        /*
        ONE prompt per idle window, whatever happened: a fresh stability window
        has to elapse before the next row is even considered.
        */
        self.reset_gate(&key);
        self.finish_delivery(&key);
    }

    /// The transcript read happens outside the `gates` lock, which deliveries also take.
    fn transcript_lifecycle_is_working(&self, key: &str, session: &Value) -> bool {
        let mut transcript = {
            let Ok(mut gates) = self.gates.lock() else {
                return true;
            };
            std::mem::take(&mut gates.entry(key.to_string()).or_default().transcript)
        };
        let working = transcript.is_working(session);
        if let Ok(mut gates) = self.gates.lock() {
            gates.entry(key.to_string()).or_default().transcript = transcript;
        }
        working
    }

    /// The window starts when the agent's own activity record last changed to stopped
    /// (`idle_since`) when there is one, else on this first tick to see it stopped.
    fn stability_window_elapsed(
        &self,
        key: &str,
        now: DateTime<Utc>,
        idle_since: Option<DateTime<Utc>>,
    ) -> bool {
        let Ok(mut gates) = self.gates.lock() else {
            return false;
        };
        let gate = gates.entry(key.to_string()).or_default();
        let since = *gate
            .stopped_since
            .get_or_insert_with(|| idle_since.filter(|since| *since <= now).unwrap_or(now));
        now.signed_duration_since(since).num_milliseconds() >= SESSION_CHAT_QUEUE_STABILITY_MS
    }

    fn reset_gate(&self, key: &str) {
        if let Ok(mut gates) = self.gates.lock() {
            if let Some(gate) = gates.get_mut(key) {
                gate.stopped_since = None;
            }
        }
    }

    /// Records that the head row waits on a missing input box, and logs it once with the reason
    /// and the screen when it has waited a minute, so a stuck row says what it waits for.
    /// Answers how long it has waited on the box.
    fn hold_for_input_box(
        &self,
        project_id: &str,
        session_id: &str,
        prompt_id: &str,
        composer: &crate::session_chat_composer::SessionChatComposerReadiness,
        now: DateTime<Utc>,
    ) -> i64 {
        let (waited, due) = self
            .gates
            .lock()
            .map(|mut gates| {
                let gate = gates
                    .entry(session_queue_key(project_id, session_id))
                    .or_default();
                input_box_hold_log_due(&mut gate.input_box_hold, prompt_id, now)
            })
            .unwrap_or((0, false));
        if due {
            crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
                "sessionChatQueueHeldForInputBox",
                project_id,
                session_id,
                composer
                    .reason
                    .as_deref()
                    .unwrap_or("The agent's input box could not be read."),
                &composer.screen_tail,
            );
        }
        waited
    }

    fn notice_confirmed(
        &self,
        key: &str,
        prompt_id: &str,
        title: &str,
        now: DateTime<Utc>,
    ) -> bool {
        self.gates.lock().is_ok_and(|mut gates| {
            let gate = gates.entry(key.to_string()).or_default();
            notice_check_confirmed(&mut gate.notice_check, prompt_id, title, now)
        })
    }

    /// Logs, once per row, that the input-box ladder hands it to the send path.
    fn log_last_resort(
        &self,
        project_id: &str,
        session_id: &str,
        prompt_id: &str,
        composer: &crate::session_chat_composer::SessionChatComposerReadiness,
    ) {
        let first = self.gates.lock().is_ok_and(|mut gates| {
            gates
                .get_mut(&session_queue_key(project_id, session_id))
                .and_then(|gate| gate.input_box_hold.as_mut())
                .filter(|hold| hold.prompt_id == prompt_id && !hold.last_resort_logged)
                .map(|hold| hold.last_resort_logged = true)
                .is_some()
        });
        if first {
            crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
                "sessionChatQueueLastResort",
                project_id,
                session_id,
                &format!(
                    "{} Sending through the send path, which checks the screen again.",
                    composer.reason.as_deref().unwrap_or_default()
                ),
                &composer.screen_tail,
            );
        }
    }

    /// Moves the head row's wait clock and puts the words for why it waits on the row (after a
    /// minute), clearing a previous head's. Answers whether the row's words changed, so the
    /// caller republishes the queue, and the bell notification due for a row stuck on the input
    /// box. A wait for anything but the input box ends that ladder. `screen` is the session's
    /// cached screen, read only for a stuck row's words.
    fn note_wait(
        &self,
        key: &str,
        held: Option<(&str, &WaitReason)>,
        now: DateTime<Utc>,
        screen: impl FnOnce() -> Vec<String>,
    ) -> NotedWait {
        let mut noted = NotedWait::default();
        let Ok(mut gates) = self.gates.lock() else {
            return noted;
        };
        let gate = gates.entry(key.to_string()).or_default();
        let previous = gate.head_wait.as_ref().map(|wait| wait.prompt_id.clone());
        if let Some(previous) = previous.filter(|id| held.is_none_or(|(head, _)| head != id)) {
            noted.changed |= crate::session_chat_queue::set_queue_wait_reason(&previous, None);
            gate.head_wait = None;
        }
        let Some((prompt_id, reason)) = held else {
            gate.input_box_hold = None;
            return noted;
        };
        if !reason.is_input_box() {
            gate.input_box_hold = None;
        }
        let words = HeadWait::observe(&mut gate.head_wait, prompt_id, reason, now);
        let stuck = gate.head_wait.as_mut().and_then(|wait| {
            let minutes = wait.stuck_minutes(reason, now)?;
            Some((minutes, !std::mem::replace(&mut wait.stuck_reported, true)))
        });
        drop(gates);
        let Some((minutes, report)) = stuck else {
            noted.changed |= crate::session_chat_queue::set_queue_wait_reason(prompt_id, words);
            return noted;
        };
        let screen = screen();
        let text = stuck_input_box_text(minutes, &screen);
        noted.changed |= crate::session_chat_queue::set_queue_wait_reason(prompt_id, Some(&text));
        if report {
            noted.stuck = Some(StuckReport {
                body: stuck_notification_body(minutes, &screen),
                text,
                screen,
            });
        }
        noted
    }

    /// CDXC:SessionChat 2026-10-29 WHY:
    /// The server-wide backstop for the user's "this should never happen again": a gate bug in one path must not strand a row. A head row held [`WATCHDOG_AFTER_MS`] for a reason that should have let it go (`WaitReason::escalates`), on a session idle by every independent signal (`idle_by_every_signal`), is logged once with the reason (`sessionChatQueueWatchdog`), gets a fresh screen read (which brings the Ctrl+L and resize steps), and is handed to the send path, whose own fresh capture and dialog, question and working guards decide. The reasons it leaves alone are states a person or another process must end, each already shown on the row.
    #[allow(clippy::too_many_arguments)]
    fn watchdog(
        &self,
        repository: &DomainRepository<'_>,
        project_id: &str,
        session_id: &str,
        key: &str,
        prompt_id: &str,
        startup_send: bool,
        reason: &WaitReason,
        now: DateTime<Utc>,
        generated_at: &str,
    ) -> Option<ReadyDelivery> {
        if !reason.escalates() {
            return None;
        }
        let first = {
            let mut gates = self.gates.lock().ok()?;
            let wait = gates.get_mut(key)?.head_wait.as_mut()?;
            if wait.prompt_id != prompt_id || wait.kind_waited_ms(now) < WATCHDOG_AFTER_MS {
                return None;
            }
            !std::mem::replace(&mut wait.watchdog_logged, true)
        };
        let session = repository.get_session(project_id, session_id).ok()??;
        if !self.idle_by_every_signal(key, &session, generated_at) {
            return None;
        }
        let composer_agent = crate::session_chat_composer::session_chat_composer_agent_id(&session);
        if first {
            crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
                "sessionChatQueueWatchdog",
                project_id,
                session_id,
                &format!(
                    "The first queued row waited five minutes ({}) on an agent idle by every other signal; sending through the send path.",
                    reason.kind()
                ),
                &[],
            );
            (self.compacting_refresher)(project_id, session_id, composer_agent.as_deref());
        }
        if self.composer_holds.waiting(key, prompt_id, now) {
            return None;
        }
        Some(ReadyDelivery {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            prompt_id: prompt_id.to_string(),
            startup_send,
            model_selection: None,
        })
    }

    /// Idle by every signal the scheduler reads, each checked on its own: running, no compaction
    /// or account recovery, hooks idle (or a finished turn's attention) with no question or
    /// approval standing, and a transcript whose last turn ended.
    fn idle_by_every_signal(&self, key: &str, session: &Value, generated_at: &str) -> bool {
        session_text(session, "lifecycleState").as_deref() == Some("running")
            && crate::session_chat_compacting::session_chat_compacting_detected_at(session)
                .is_none()
            && !account_recovery_waiting(session)
            && session_chat_queue_activity_is_deliverable(session, generated_at)
            && crate::agents::session_chat_prompt_setting(session).is_none()
            && !self.transcript_lifecycle_is_working(key, session)
    }

    /// Drops the in-memory window for sessions whose queue has gone empty, so a
    /// long-lived daemon does not accumulate a gate per session it ever queued
    /// a prompt for.
    fn retain_gates(&self, targets: &[(String, String)]) {
        let Ok(mut gates) = self.gates.lock() else {
            return;
        };
        if gates.is_empty() {
            return;
        }
        let live: HashSet<String> = targets
            .iter()
            .map(|(project_id, session_id)| session_queue_key(project_id, session_id))
            .collect();
        gates.retain(|key, _| live.contains(key));
        self.composer_holds.retain(|key| live.contains(key));
        let heads: HashSet<String> = gates
            .values()
            .filter_map(|gate| gate.head_wait.as_ref().map(|wait| wait.prompt_id.clone()))
            .collect();
        crate::session_chat_queue::retain_queue_wait_reasons(|id| heads.contains(id));
    }

    fn notify_sender(&self, project_id: &str, session_id: &str, text: &str, reason: &str) {
        if let Some((sender_project, sender_session)) =
            crate::session_chat_queue_undelivered::notify_sender_of_undelivered_agent_message(
                &self.paths,
                &self.server_id,
                project_id,
                session_id,
                text,
                reason,
            )
        {
            (self.publisher_factory)(&sender_project, &sender_session)();
        }
    }

    fn is_delivering(&self, key: &str) -> bool {
        self.delivering
            .lock()
            .map(|delivering| delivering.contains(key))
            .unwrap_or(true)
    }

    fn begin_delivery(&self, key: &str) -> bool {
        self.delivering
            .lock()
            .map(|mut delivering| delivering.insert(key.to_string()))
            .unwrap_or(false)
    }

    fn finish_delivery(&self, key: &str) {
        if let Ok(mut delivering) = self.delivering.lock() {
            delivering.remove(key);
        }
    }
}

/// What the scheduler does for a session that is not running, by the head of its queue.
#[derive(Debug, PartialEq, Eq)]
enum NotRunningHead<'a> {
    /// A startup send: wake the session on the wake check's backoff.
    CheckWake(&'a str),
    /// Any other row of a sleeping session: make it a startup send and wake the session now.
    HoldAndWake(&'a str),
    Wait,
}

fn not_running_head<'a>(
    lifecycle: Option<&str>,
    head: Option<&'a crate::session_chat_queue::SessionChatQueuedPrompt>,
) -> NotRunningHead<'a> {
    match head {
        Some(head) if head.startup_send => NotRunningHead::CheckWake(&head.id),
        Some(head)
            if lifecycle == Some("sleeping")
                && crate::session_chat_send_wake::wakes_sleeping_session_for(&head.created_at) =>
        {
            NotRunningHead::HoldAndWake(&head.id)
        }
        _ => NotRunningHead::Wait,
    }
}

fn session_queue_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}\u{1f}{session_id}")
}

/// An account switch or recovery is under way, which owns the session's next input.
fn account_recovery_waiting(session: &Value) -> bool {
    matches!(
        session
            .pointer("/runtimeSettings/accountRecovery/status")
            .and_then(Value::as_str),
        Some("waiting" | "retrying" | "needsAttention")
    )
}

/// Idle, or a finished turn's attention with no interactive card pending.
/// Working and every other attention hold the queue.
/// When the agent's recorded activity last changed, if it is recorded as stopped (idle, or the
/// attention a finished turn enters): the moment the stability window can start from. A record
/// that still says working (and only reads idle because it went stale) gives none.
fn agent_idle_since(session: &Value) -> Option<DateTime<Utc>> {
    let activity = session.pointer("/runtimeSettings/agentActivity")?;
    if !matches!(
        activity.get("activity").and_then(Value::as_str),
        Some("idle" | "attention")
    ) {
        return None;
    }
    let stamp = activity.get("lastChangedAt").and_then(Value::as_str)?;
    DateTime::parse_from_rfc3339(stamp)
        .ok()
        .map(|stamp| stamp.with_timezone(&Utc))
}

pub(super) fn session_chat_queue_activity_is_deliverable(
    session: &Value,
    generated_at: &str,
) -> bool {
    match presentation_activity(session, generated_at).as_str() {
        "idle" => true,
        "attention" => {
            let activity = session
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("agentActivity"));
            is_turn_complete_attention(activity)
                && crate::agents::session_chat_prompt_setting(session).is_none()
        }
        _ => false,
    }
}

fn session_text(session: &Value, key: &str) -> Option<String> {
    session
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn runtime_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// An Escape-safe Claude panel (its Settings screen, or an offer Claude opened by itself) that the
/// notice classifier also reads as an input-blocking dialog. The send closes it with Escape instead
/// of handing it to the user as a question or failing a queued row on it.
pub(super) fn escape_closes_claude_panel(
    composer: &crate::session_chat_composer::SessionChatComposerReadiness,
    notice: &crate::session_chat_notice::SessionChatTerminalNotice,
) -> bool {
    composer.should_dismiss()
        && notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_CLAUDE_INPUT_BLOCKED
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_chat_queue::SessionChatQueuedPrompt;

    fn row(startup_send: bool, age_hours: i64) -> SessionChatQueuedPrompt {
        let created = (Utc::now() - chrono::Duration::hours(age_hours)).to_rfc3339();
        SessionChatQueuedPrompt {
            id: "Q1".to_string(),
            text: "<ghostex-page-answers page=\"p\" title=\"T\">…</ghostex-page-answers>"
                .to_string(),
            startup_send,
            image_paths: Vec::new(),
            state: "queued".to_string(),
            error_message: None,
            wait_reason: None,
            created_at: created.clone(),
            updated_at: created,
        }
    }

    #[test]
    fn a_row_held_for_the_input_box_is_logged_once_after_a_minute() {
        let start = Utc::now();
        let after = |seconds: i64| start + chrono::Duration::seconds(seconds);
        let mut hold = None;
        assert_eq!(input_box_hold_log_due(&mut hold, "Q1", start), (0, false));
        assert_eq!(
            input_box_hold_log_due(&mut hold, "Q1", after(59)),
            (59_000, false)
        );
        assert_eq!(
            input_box_hold_log_due(&mut hold, "Q1", after(60)),
            (60_000, true)
        );
        assert_eq!(
            input_box_hold_log_due(&mut hold, "Q1", after(8 * 3600)),
            (8 * 3600 * 1000, false)
        );
        // The next head row starts its own minute.
        assert_eq!(
            input_box_hold_log_due(&mut hold, "Q2", after(8 * 3600)),
            (0, false)
        );
        assert_eq!(
            input_box_hold_log_due(&mut hold, "Q2", after(8 * 3600 + 60)),
            (60_000, true)
        );
    }

    #[test]
    fn a_cached_notice_fails_a_row_only_after_a_fresh_read_still_shows_it() {
        let start = Utc::now();
        let after = |ms: i64| start + chrono::Duration::milliseconds(ms);
        let mut check = None;
        assert!(!notice_check_confirmed(&mut check, "Q1", "Trust", start));
        assert!(!notice_check_confirmed(
            &mut check,
            "Q1",
            "Trust",
            after(2_999)
        ));
        assert!(notice_check_confirmed(
            &mut check,
            "Q1",
            "Trust",
            after(3_000)
        ));
        // Another notice, or the same one coming back much later, is read fresh again first.
        assert!(!notice_check_confirmed(
            &mut check,
            "Q1",
            "Login",
            after(4_000)
        ));
        assert!(!notice_check_confirmed(
            &mut check,
            "Q1",
            "Login",
            after(40_000)
        ));
        assert!(notice_check_confirmed(
            &mut check,
            "Q1",
            "Login",
            after(43_000)
        ));
    }

    #[test]
    fn page_answers_and_queued_rows_wake_a_sleeping_session() {
        assert_eq!(
            not_running_head(Some("sleeping"), Some(&row(false, 0))),
            NotRunningHead::HoldAndWake("Q1")
        );
    }

    #[test]
    fn a_held_message_keeps_its_wake_backoff() {
        for lifecycle in [Some("sleeping"), Some("stopped"), None] {
            assert_eq!(
                not_running_head(lifecycle, Some(&row(true, 0))),
                NotRunningHead::CheckWake("Q1")
            );
        }
    }

    #[test]
    fn closed_sessions_and_old_rows_do_not_wake_on_their_own() {
        assert_eq!(
            not_running_head(Some("stopped"), Some(&row(false, 0))),
            NotRunningHead::Wait
        );
        assert_eq!(
            not_running_head(Some("sleeping"), Some(&row(false, 25))),
            NotRunningHead::Wait
        );
        assert_eq!(
            not_running_head(Some("sleeping"), None),
            NotRunningHead::Wait
        );
    }
}
