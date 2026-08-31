/*
CDXC:SessionChatPromptQueue 2026-08-21:
The scheduler half of the Ghostex chat prompt queue. `session_chat_queue.rs`
owns storage, the endpoints and the frame carriage; this module owns the clock
that decides WHEN a queued row is allowed to reach the agent.

The daemon owns this decision rather than any client, so a queue drains with
every client closed, the phone locked, or the desktop app quit.

Shape is deliberately `delayed_sends.rs`'s: a 1s tick, a non-working stability
window tracked per session, a guarded claim so two ticks cannot double-fire, and
restart recovery that never silently re-sends. The differences are the readiness
rule and the drain rate:

  - Ready = `presentation_activity == "idle"` AND the chat transcript lifecycle
    is not `Working`, held for SESSION_CHAT_QUEUE_STABILITY_MS.
  - `attention` NEVER releases the queue. A prompt fired while the agent sits on
    a permission/approval prompt would be swallowed as the ANSWER to that
    prompt. Late delivery is harmless; early delivery corrupts a turn.
  - ONE prompt per idle window. Delivering the head makes the agent work again,
    so the clock restarts from zero after every attempt and row #2 waits for the
    next stop. This is never a "drain the whole queue" loop.
  - An INPUT-BLOCKING terminal notice is not a delivery opportunity: the head
    row is marked `failed` with the notice title and the drain stops until the
    user retries or deletes it. The text is never lost. "Input-blocking" is
    `session_chat_notice.rs`'s own predicate, NOT `severity == error`: a trust
    dialog or a first-run setup screen is only catalogued `Warning`/`Info` and
    would still eat a prompt as the ANSWER to itself.

Cost discipline: only sessions that actually hold a pending row are considered,
so a daemon with no queues anywhere does one indexed SQLite query per second and
nothing else. Transcripts are never walked for a session with an empty queue.
*/

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use tokio::sync::broadcast;

use crate::domain::{read_domain_rpc_params, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, read_runtime_text, routed_json, schedule_presentation_session_delta,
    session_observer_key, AppState, RoutedResponse,
};
use crate::session_chat_composer::SessionChatComposerReadiness;
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::session_chat_options::{
    cached_session_chat_composer_readiness, cached_session_chat_screen_state,
    cached_session_chat_terminal_notice, emit_session_chat_options_state_frame,
    schedule_session_chat_option_redetect, session_chat_terminal_notice_publisher,
    session_chat_watchdog_state_reader, SessionChatOptionDetector,
};
use crate::session_chat_send::resolve_session_chat_send_target;
use crate::{
    domain::DomainRepository,
    paths::GxserverPaths,
    presentation::presentation_activity,
    session_chat::{
        read_session_chat_tail_page, resolve_session_chat_transcript_agent,
        resolve_session_chat_transcript_path, SessionChatTailPage, SessionChatTurnLifecycleState,
    },
    session_chat_notice::SessionChatTerminalNotice,
    session_chat_queue::{
        deliver_session_chat_queued_prompt, fail_session_chat_queued_prompt,
        list_sessions_with_pending_queue, read_session_chat_queue_snapshot_with,
        SessionChatQueuePublisherFactory, SessionChatQueueSenderFactory,
    },
    storage::open_gxserver_database,
};
use axum::http::StatusCode;
use serde_json::{json, Map};

const SESSION_CHAT_QUEUE_TICK_SECONDS: u64 = 1;

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
CDXC:SessionChatComposerReady 2026-08-26:
The session's last known composer verdict, injected the same way. Read from the
cache only — a tick must never spawn a capture — so a session nobody has probed
reads `Unknown` and the queue behaves exactly as it did before this feature.
*/
pub type SessionChatQueueComposerReader =
    Arc<dyn Fn(&str, &str) -> SessionChatComposerReadiness + Send + Sync>;

/*
The one deliberate refresh the queue scheduler may request. It is called only
for a session whose last whole screen capture proved `/compact` was live. That
keeps headless queues moving after the chat client disconnects without turning
the one-second scheduler into a general terminal-screen poller.
*/
pub type SessionChatQueueCompactingRefresher = Arc<dyn Fn(&str, &str, Option<&str>) + Send + Sync>;

#[derive(Default)]
struct SessionQueueGate {
    /// First moment this session looked stopped without looking busy since.
    /// `None` means the window has not started (or was just reset).
    stopped_since: Option<DateTime<Utc>>,
    /// `(agent, agentSessionId, agentSessionPath)` the cached path was resolved
    /// for. Re-resolving can scan agent home directories, which must not happen
    /// once per second.
    transcript_identity: String,
    transcript_path: Option<PathBuf>,
}

struct ReadyDelivery {
    project_id: String,
    session_id: String,
    prompt_id: String,
}

#[derive(Clone)]
pub struct SessionChatQueueRuntime {
    paths: GxserverPaths,
    server_id: String,
    sender_factory: SessionChatQueueSenderFactory,
    publisher_factory: SessionChatQueuePublisherFactory,
    notice_reader: SessionChatQueueNoticeReader,
    composer_reader: SessionChatQueueComposerReader,
    compacting_refresher: SessionChatQueueCompactingRefresher,
    gates: Arc<Mutex<HashMap<String, SessionQueueGate>>>,
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
        publisher_factory: SessionChatQueuePublisherFactory,
        notice_reader: SessionChatQueueNoticeReader,
        composer_reader: SessionChatQueueComposerReader,
        compacting_refresher: SessionChatQueueCompactingRefresher,
    ) -> Self {
        Self {
            paths,
            server_id: server_id.into(),
            sender_factory,
            publisher_factory,
            notice_reader,
            composer_reader,
            compacting_refresher,
            gates: Arc::new(Mutex::new(HashMap::new())),
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
                }
            }
        });
    }

    async fn run_tick(&self) {
        for ready in self.collect_ready_deliveries() {
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

        for (project_id, session_id) in targets {
            let key = session_queue_key(&project_id, &session_id);
            if self.is_delivering(&key) {
                // A send in flight is the busiest a session ever is.
                self.reset_gate(&key);
                continue;
            }
            let Ok(Some(session)) = repository.get_session(&project_id, &session_id) else {
                self.reset_gate(&key);
                continue;
            };
            /*
            A sleeping or stopped session keeps its queue rather than failing it:
            the rows drain once it is awake again. Writing into a dead provider
            would lose the text with nothing to show for it.
            */
            if session_text(&session, "lifecycleState").as_deref() != Some("running") {
                self.reset_gate(&key);
                continue;
            }
            /*
            A compacting marker is written by the same whole zmx screen capture
            that feeds chat's progress card. Refresh only this marked state so
            a queued prompt can leave as soon as compaction disappears even if
            every client has disconnected. Failed/capped captures do not clear
            the marker, so uncertainty always holds the prompt safely.
            */
            if crate::session_chat_compacting::session_chat_compacting_detected_at(&session)
                .is_some()
            {
                let agent = session_chat_agent_for_session(&session);
                (self.compacting_refresher)(&project_id, &session_id, agent.as_deref());
                self.reset_gate(&key);
                continue;
            }
            /*
            "working" is obvious. "attention" is the load-bearing one: the agent
            is sitting on a permission/approval prompt, and a prompt delivered
            now becomes the ANSWER to it.
            */
            if presentation_activity(&session, &generated_at) != "idle" {
                self.reset_gate(&key);
                continue;
            }
            if self.transcript_lifecycle_is_working(&key, &session) {
                self.reset_gate(&key);
                continue;
            }
            if !self.stability_window_elapsed(&key, now) {
                continue;
            }
            let snapshot = read_session_chat_queue_snapshot_with(&db, &project_id, &session_id);
            /*
            `None` means the queue is empty, or its head is `failed`/`sending`.
            A failed head is the stop signal: the drain does not step over it.
            */
            let Some(head) = snapshot.deliverable_head() else {
                continue;
            };
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
            if let Some(notice) = (self.notice_reader)(&project_id, &session_id) {
                if notice.blocks_input() {
                    blocked.push((
                        project_id,
                        session_id,
                        head.id.clone(),
                        notice.title.clone(),
                    ));
                    continue;
                }
            }
            /*
            CDXC:SessionChatComposerReady 2026-08-26:
            No input box on screen ⇒ HOLD, and deliberately not the `blocked`
            treatment above. A blocking notice is a state only the user can
            leave — a trust dialog waits forever for a keypress — so failing the
            row makes the stall visible and retryable. A missing composer is the
            opposite: it is what a CLI looks like while it BOOTS, and it clears
            on its own within seconds. Burning the head row for that would fail
            every queued prompt on a session that was merely restarted, which is
            precisely the moment a queue exists to cover.

            `Unknown` is not a hold. Only positive evidence that the box is
            absent stops the drain.
            */
            if (self.composer_reader)(&project_id, &session_id).is_not_ready() {
                self.reset_gate(&key);
                continue;
            }
            ready.push(ReadyDelivery {
                project_id,
                session_id,
                prompt_id: head.id.clone(),
            });
        }
        drop(repository);
        drop(db);

        for (project_id, session_id, prompt_id, reason) in blocked {
            if fail_session_chat_queued_prompt(
                &self.paths,
                &project_id,
                &session_id,
                &prompt_id,
                &reason,
            )
            .is_ok()
            {
                (self.publisher_factory)(&project_id, &session_id)();
            }
            self.reset_gate(&session_queue_key(&project_id, &session_id));
        }
        ready
    }

    async fn deliver(&self, key: String, ready: ReadyDelivery) {
        let sender = (self.sender_factory)(&ready.project_id, &ready.session_id);
        /*
        The shared claim → send → settle path. It deletes the row on success and
        marks it `failed` with the reason on error, so "Send now" and the
        scheduler can never both deliver the same row.
        */
        let delivered = deliver_session_chat_queued_prompt(
            &self.paths,
            &self.server_id,
            &ready.project_id,
            &ready.session_id,
            &ready.prompt_id,
            &sender,
        )
        .await;
        if delivered.is_ok() {
            (self.publisher_factory)(&ready.project_id, &ready.session_id)();
        }
        /*
        ONE prompt per idle window, whatever happened: a fresh stability window
        has to elapse before the next row is even considered.
        */
        self.reset_gate(&key);
        self.finish_delivery(&key);
    }

    /*
    The second readiness signal. Agent hooks (presentation activity) know a turn
    started before the transcript flushes, and the transcript knows a turn is
    still running when hooks are missing or stale — the queue needs BOTH to be
    quiet. Read from the same bounded reverse tail the chat surfaces use; the
    resolved path is cached per session because resolving it can scan agent home
    directories.
    */
    fn transcript_lifecycle_is_working(&self, key: &str, session: &Value) -> bool {
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
        let Some(transcript_agent) = resolve_session_chat_transcript_agent(resolved_agent) else {
            return false;
        };
        let agent_session_id = runtime_text(session, "agentSessionId");
        let agent_session_path = runtime_text(session, "agentSessionPath");
        let identity = format!(
            "{}|{}|{}",
            resolved_agent.unwrap_or_default(),
            agent_session_id.clone().unwrap_or_default(),
            agent_session_path.clone().unwrap_or_default(),
        );
        let cached = self.gates.lock().ok().and_then(|gates| {
            gates
                .get(key)
                .filter(|gate| gate.transcript_identity == identity)
                .and_then(|gate| gate.transcript_path.clone())
                .filter(|path| path.is_file())
        });
        let path = match cached {
            Some(path) => Some(path),
            None => resolve_session_chat_transcript_path(
                transcript_agent,
                agent_session_id.as_deref(),
                agent_session_path.as_deref(),
            ),
        };
        if let Ok(mut gates) = self.gates.lock() {
            let gate = gates.entry(key.to_string()).or_default();
            gate.transcript_identity = identity;
            gate.transcript_path = path.clone();
        }
        let Some(path) = path else {
            // No transcript on disk yet: agent hooks are the only signal, and
            // they already said idle.
            return false;
        };
        match read_session_chat_tail_page(
            transcript_agent,
            &path,
            SESSION_CHAT_QUEUE_LIFECYCLE_TAIL_LIMIT,
            None,
        ) {
            Ok(SessionChatTailPage::Page {
                lifecycle: Some(lifecycle),
                ..
            }) => lifecycle.state == SessionChatTurnLifecycleState::Working,
            _ => false,
        }
    }

    fn stability_window_elapsed(&self, key: &str, now: DateTime<Utc>) -> bool {
        let Ok(mut gates) = self.gates.lock() else {
            return false;
        };
        let gate = gates.entry(key.to_string()).or_default();
        match gate.stopped_since {
            Some(since) => {
                now.signed_duration_since(since).num_milliseconds()
                    >= SESSION_CHAT_QUEUE_STABILITY_MS
            }
            None => {
                gate.stopped_since = Some(now);
                false
            }
        }
    }

    fn reset_gate(&self, key: &str) {
        if let Ok(mut gates) = self.gates.lock() {
            if let Some(gate) = gates.get_mut(key) {
                gate.stopped_since = None;
            }
        }
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

fn session_queue_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}\u{1f}{session_id}")
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

/*
CDXC:SessionChatPromptQueue 2026-08-21:
THE internal chat-message send. `/api/sendSessionChatMessage` is one caller;
the prompt queue ("Send now" and the scheduler) is the other, which is why it
lives here instead of inside the HTTP handler. Everything a chat send needs
travels with it — the per-session send mutex in session_chat_send.rs, the
answerable-picker refusal, the terminal-input clear, the delivery watchdog and
the option re-detect — so a queued prompt is indistinguishable from one the user
typed and can never interleave with a Delayed Send.
Returns the number of text bytes handed to zmx.
*/
///
/// `prompt_source` is the analytics attribution for this send: `chat` when the
/// user typed it into the composer, `queue` when the prompt queue delivered it.
/// The two are indistinguishable from inside this function (that is the whole
/// point of the queue), so the caller is the only place that knows.
pub(crate) async fn send_session_chat_message_internal(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    image_paths: &[String],
    prompt_source: &'static str,
) -> std::result::Result<usize, DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    let target = resolve_session_chat_send_target(state, &params, "sendSessionChatMessage")?;
    let agent = session_chat_agent_for_session(&target.session);
    let terminal_agent =
        crate::session_chat_composer::session_chat_composer_agent_id(&target.session)
            .or_else(|| agent.clone());
    /*
    CDXC:SessionChatTerminalNotices 2026-08-19:
    Sample where the transcript ends BEFORE the message is enqueued: everything
    written past this offset is a candidate for "the agent recorded it". Sampling
    afterwards would race the agent's own write. This is the only work the
    watchdog puts in front of a send — a path resolve plus one `metadata()`, both
    on a blocking thread — and it is skipped entirely for agents the watchdog
    does not cover.
    */
    /*
    CDXC:SessionChatTerminalPicker 2026-08-21:
    Claude Code's resume-usage picker owns the input line when a large session
    is resumed. This send used to answer it automatically ("Resume full session
    as-is") before typing, which was wrong twice over: the summary-vs-full
    trade-off is the user's to make, and whenever the walk missed, the message's
    own trailing Enter confirmed the HIGHLIGHTED row instead — silently
    compacting the conversation the user was continuing, with nothing to show
    for it but a delivery-failed banner.

    So the send refuses instead. The same capture that proves the picker is up
    caches the notice carrying its rows, and publishing it puts the answer
    picker in front of the user on every subscribed client. Only an ANSWERABLE
    state stops a send here: the catalog's other blocking dialogs still go
    through to the delivery watchdog, which is the only thing that can explain
    them.
    */
    /*
    CDXC:SessionChatComposerReady 2026-08-26:
    Terminal capture must use the concrete CLI identity, not the normalized
    transcript family. Omp shares Pi's transcript decoder but paints different
    composer and statusline chrome, so folding it to Pi here loses both screen
    readings. Agents without a concrete screen identity fall back to the
    transcript family.
    */
    let detection = SessionChatOptionDetector::new(state)
        .detect(
            &target.project_id,
            &target.session_id,
            terminal_agent.as_deref(),
            true,
        )
        .await;
    if let Some(blocking) = detection
        .notice
        .as_ref()
        .filter(|notice| notice.is_answerable())
    {
        session_chat_terminal_notice_publisher(state, &target.project_id, &target.session_id)();
        return Err(DomainStateError {
            code: "invalidState",
            message: format!("{}. Answer it in chat before sending.", blocking.title),
        });
    }
    /*
    CDXC:SessionChatComposerReady 2026-08-26:
    The positive gate. Everything above is "is a screen we RECOGNISE in the
    way?"; this is "did the CLI paint an input box at all?". It catches the
    states no notice rule covers — a CLI still booting, an auth screen shipped
    after our catalog, a dialog we have never seen — which are exactly the ones
    that used to eat a message and answer with a delivery-failed banner minutes
    later.

    `Unknown` FAILS OPEN and is by far the common case for unmeasured agents, so
    this can only ever refuse a send it has positive evidence about. The screen
    tail behind the verdict is not squeezed into the error (DomainStateError
    carries a code and a message and nothing else, at 169 construction sites);
    clients read it from /api/readSessionTerminalTail instead.
    */
    if detection.composer.is_not_ready() {
        return Err(DomainStateError {
            code: "composerNotReady",
            message: detection
                .composer
                .reason
                .clone()
                .unwrap_or_else(|| "The agent's input box is not accepting input yet.".to_string()),
        });
    }
    let send_probe = crate::session_chat_watchdog::SessionChatSendProbe::sample(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        agent.as_deref(),
        read_runtime_text(&target.session, "agentSessionId").as_deref(),
        read_runtime_text(&target.session, "agentSessionPath").as_deref(),
        text,
    )
    .await;
    /*
    Chat owns the terminal composer when it sends. Discard anything already
    sitting on that hidden input line instead of turning it into a user-facing
    Saved Prompt: `build_session_chat_message_steps` starts with the measured
    Ctrl+U/Ctrl+K clear burst, settles it, and only then pastes this message.
    Terminal -> Chat view switching remains the separate, loss-safe draft
    transfer path for text the user actually wants to carry between views.
    */
    let steps = crate::session_chat_send::build_session_chat_message_steps(
        terminal_agent.as_deref(),
        text,
        image_paths,
    );
    if let Err(error) = crate::session_chat_send::execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-message",
        steps,
    )
    .await
    {
        /*
        CDXC:SessionChatTerminalNotices 2026-08-19:
        The case this feature exists for — the agent CLI in this pane is dead —
        fails HERE, not at the delivery watchdog: zmx refuses the clear or paste,
        or the terminal screen proves that the paste never landed, so the user
        would otherwise see only a generic toast. When the TERMINAL is what
        refused the message (as opposed to the send being superseded or
        cancelled), escalate once with the same one-capture verdict the watchdog
        takes at its deadline. It runs as its own task so the error response is
        not made to wait for the capture, it never retries the send, and the
        response below is unchanged.
        */
        if error.terminal_refused() {
            if let Some(send_probe) = send_probe {
                crate::session_chat_watchdog::escalate_failed_session_chat_send(
                    send_probe,
                    session_chat_terminal_notice_publisher(
                        state,
                        &target.project_id,
                        &target.session_id,
                    ),
                    session_chat_watchdog_state_reader(
                        state,
                        &target.project_id,
                        &target.session_id,
                    ),
                );
            }
        }
        // CDXC:SessionChatComposerReady 2026-08-26: the in-worker wait raises
        // the same code the pre-send gate does, so a client has one case to
        // handle whichever of the two caught it.
        if error.composer_not_ready() {
            return Err(DomainStateError {
                code: "composerNotReady",
                message: error.message,
            });
        }
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: error.message,
        });
    }
    /*
    CDXC:SessionChatTerminalNotices 2026-08-19:
    The bytes reached zmx, which says nothing about the agent having received
    them: a message typed into a login screen, a trust dialog or a shell where
    the CLI already exited is accepted and lost. The watchdog verifies delivery
    against the transcript and surfaces the terminal's own explanation when it
    cannot. It never retries and never writes to the terminal.
    */
    if let Some(send_probe) = send_probe {
        crate::session_chat_watchdog::start_session_chat_send_watchdog(
            send_probe,
            session_chat_terminal_notice_publisher(state, &target.project_id, &target.session_id),
            session_chat_watchdog_state_reader(state, &target.project_id, &target.session_id),
        );
    }
    /*
    CDXC:AnonymousAnalytics 2026-08-26:
    Counted after the bytes reached zmx and only on the success path, so a
    refused send is not reported as a prompt. The COUNT and the agent id are all
    that leave; the prompt text is not in scope for the emitter and cannot be.
    */
    crate::telemetry::prompt_sent(agent.as_deref(), prompt_source);
    /*
    An option command changes what the statusline reports: read it back. It is
    also NOT a user prompt — Ghostex itself typed it on the user's behalf when
    they picked a model or an effort level out of the composer's dropdown.
    */
    let is_option_command = crate::session_chat_options::is_session_chat_option_command_text(
        terminal_agent.as_deref(),
        text,
    ) || crate::session_chat_options::is_session_chat_activity_command_text(
        terminal_agent.as_deref(),
        text,
    );
    /*
    CDXC:DraftSessions 2026-08-28:
    THE chat half of the draft promotion choke point. Both callers reach the
    agent through this one function — the user's composer and the prompt queue's
    scheduler — so clearing the marker here covers every Ghostex-delivered first
    prompt, and it happens only after the bytes were accepted: a send refused by
    the composer gate or by zmx returns above and leaves the row a draft.
    (The terminal-direct half lives in `agents/drafts.rs`.)

    Option commands are carved out of BOTH halves. `/model` is delivered through
    this same function, and the dropdown that sends it is the one that will host
    the draft's own Agents section — so promoting on it would let a user destroy
    their draft's agent switcher by picking a model in it. The carve-out has to
    cover the terminal churn those bytes cause as well, which is why the
    non-promoting branch re-arms the draft's launch suppression window instead
    of doing nothing: the command's spinner is then folded back to idle exactly
    like a startup spinner, and cannot promote the draft through the
    activity-based half a second later.
    */
    if is_option_command {
        suppress_draft_activity_after_app_command(state, &target.project_id, &target.session_id);
    } else {
        promote_draft_session_after_send(state, &target.project_id, &target.session_id);
    }
    if is_option_command {
        schedule_session_chat_option_redetect(
            state,
            &target.project_id,
            &target.session_id,
            terminal_agent.as_deref(),
        );
    }
    Ok(text.len())
}

/*
CDXC:SessionChatPromptQueue 2026-08-21:
Dispatch-only glue for the Ghostex chat prompt queue. Storage, validation and
the endpoint bodies live in session_chat_queue.rs; server.rs supplies only the
three things that module deliberately does not know about — the state-database
path, the delivery path, and the live follower stream a state frame rides.
*/
pub(crate) async fn handle_session_chat_queue_http(
    state: &Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    if endpoint_path == "/api/sendSessionChatQueuedPrompt" {
        return handle_send_session_chat_queued_prompt_http(
            state,
            endpoint_path,
            request_id,
            &params,
        )
        .await;
    }
    match crate::session_chat_queue::handle_session_chat_queue_endpoint(
        &state.paths,
        state.metadata.server_id.as_str(),
        &endpoint_path,
        &params,
    ) {
        Ok(result) => {
            /*
            Every mutation restates the queue to the session's other followers,
            so a row queued on the phone appears in the desktop composer without
            anyone re-reading.
            */
            if result.broadcast {
                broadcast_session_chat_queue_state(state, &result.project_id, &result.session_id);
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result.value),
            )
        }
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

/*
"Send now" delivers immediately regardless of agent state, exactly like pressing
Enter. `sent: false` therefore means the send itself FAILED and the row is now
`failed` with an errorMessage — it never means "deferred".
*/
pub(crate) async fn handle_send_session_chat_queued_prompt_http(
    state: &Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    params: &Map<String, Value>,
) -> RoutedResponse {
    let (project_id, session_id, prompt_id) = match session_chat_queue_prompt_target(params) {
        Ok(target) => target,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let sender = session_chat_queue_sender(state, &project_id, &session_id);
    match crate::session_chat_queue::deliver_session_chat_queued_prompt(
        &state.paths,
        state.metadata.server_id.as_str(),
        &project_id,
        &session_id,
        &prompt_id,
        &sender,
    )
    .await
    {
        Ok(delivery) => {
            broadcast_session_chat_queue_state(state, &project_id, &session_id);
            let mut result = Map::new();
            delivery.snapshot.insert_into(&mut result);
            result.insert("sent".to_string(), json!(delivery.sent));
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, Value::Object(result)),
            )
        }
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn session_chat_queue_prompt_target(
    params: &Map<String, Value>,
) -> std::result::Result<(String, String, String), DomainStateError> {
    let read = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    match (read("projectId"), read("sessionId"), read("promptId")) {
        (Some(project_id), Some(session_id), Some(prompt_id)) => {
            Ok((project_id, session_id, prompt_id))
        }
        _ => Err(DomainStateError {
            code: "invalidParams",
            message: "sendSessionChatQueuedPrompt requires projectId, sessionId and promptId."
                .to_string(),
        }),
    }
}

/*
Publishes the session's current queue + draft on a `sessionChatState` frame.
Same restating discipline as the terminal-notice publisher: a state frame
REPLACES the client's prompt card, notice, pills and queue, so it must carry all
of them, which is exactly what emit_session_chat_options_state_frame builds.
No live follower ⇒ nothing is emitted and clients pick the change up from their
next readSessionChat (or, on mobile, from the long-poll fingerprint).
*/
pub(crate) fn broadcast_session_chat_queue_state(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) {
    let key = session_observer_key(project_id, session_id);
    let options = state
        .session_chat_option_cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).map(|entry| entry.value.options.clone()))
        .unwrap_or_default();
    let screen = cached_session_chat_screen_state(state, project_id, session_id);
    emit_session_chat_options_state_frame(
        &state.session_chat_followers,
        &state.event_hub,
        &state.paths,
        &state.metadata.server_id,
        project_id,
        session_id,
        options.as_ref(),
        screen.borrow(),
    );
    publish_session_chat_queue_presentation_delta(state, project_id, session_id);
}

/*
CDXC:SessionChatPromptQueue 2026-08-21:
The sidebar's queued-prompt badge reads `queuedPromptCount` off the presentation
projection, NOT off the chat frame — the sidebar renders every session from that
snapshot and holds no per-session chat subscription. A queue change on an
otherwise idle session produces no other delta, so without this publish the badge
would only appear whenever some unrelated event happened to fire.

CDXC:DraftSessions 2026-08-28:
`/api/setSessionChatDraft` rides the same publish, which is what keeps a DRAFT
session's sidebar title following the user's typing on every client: the draft
display title is a presentation overlay read from `session_chat_drafts`, so the
delta this schedules is the only thing that republishes it.

Every queue mutation and every scheduler delivery already funnels through
`broadcast_session_chat_queue_state`, so the delta is published there rather than
at each call site. Same shape as `delayed_sends::publish_session_change`: one
short sequencer-locked section that projects, allocates the revision and
broadcasts, so revision order and broadcast order stay identical.
*/
/// Clears a draft session's marker after a chat/queue send reached the agent,
/// and republishes the row so every client stops drawing it as a draft. A send
/// into a session that was never a draft costs one indexed read and no write.
fn promote_draft_session_after_send(state: &AppState, project_id: &str, session_id: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    if matches!(
        crate::agents::promote_draft_session(&repository, project_id, session_id),
        Ok(true)
    ) {
        let _ =
            schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
    }
}

/// Re-arms a draft's launch activity-suppression window after Ghostex typed one
/// of its OWN commands (a model/effort pick) into the terminal, so the churn
/// that command causes cannot be mistaken for the user prompting the agent. A
/// no-op on every session that is not a draft.
fn suppress_draft_activity_after_app_command(state: &AppState, project_id: &str, session_id: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    let _ = crate::agents::arm_draft_launch_activity_suppression(&repository, &session);
}

pub(crate) fn publish_session_chat_queue_presentation_delta(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let _ = schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
}

/*
CDXC:SessionChatPromptQueue 2026-08-21:
The delivery handle the queue module (and the scheduler in
session_chat_queue_runtime.rs) holds. It closes over the daemon state so those
modules never learn about AppState, zmx names, or the send watchdog, and every
queued prompt still travels the same internals /api/sendSessionChatMessage uses.
*/
pub(crate) fn session_chat_queue_sender(
    state: &Arc<AppState>,
    project_id: &str,
    session_id: &str,
) -> crate::session_chat_queue::SessionChatQueueSender {
    let state = state.clone();
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    Arc::new(move |text: String| {
        let state = state.clone();
        let project_id = project_id.clone();
        let session_id = session_id.clone();
        Box::pin(async move {
            send_session_chat_message_internal(
                &state,
                &project_id,
                &session_id,
                &text,
                &[],
                "queue",
            )
            .await
            .map(|_| ())
            .map_err(|error| error.message)
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
        session_chat_queue_sender(&state, project_id, session_id)
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
CDXC:SessionChatPromptQueue 2026-08-21:
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

/// CDXC:SessionChatComposerReady 2026-08-26: the composer half of the same
/// view, under the same no-spawn rule.
pub(crate) fn session_chat_queue_composer_reader(
    state: &Arc<AppState>,
) -> crate::session_chat_queue_runtime::SessionChatQueueComposerReader {
    let state = state.clone();
    Arc::new(move |project_id: &str, session_id: &str| {
        cached_session_chat_composer_readiness(&state, project_id, session_id)
    })
}

pub(crate) fn session_chat_queue_compacting_refresher(
    state: &Arc<AppState>,
) -> SessionChatQueueCompactingRefresher {
    let detector = SessionChatOptionDetector::new(state);
    Arc::new(
        move |project_id: &str, session_id: &str, agent: Option<&str>| {
            let detector = detector.clone();
            let project_id = project_id.to_string();
            let session_id = session_id.to_string();
            let agent = agent.map(str::to_string);
            tokio::task::spawn_blocking(move || {
                // Respect the shared TTL. The scheduler asks every second, but at
                // most one fresh zmx capture is needed per cache window.
                detector.detect_blocking(&project_id, &session_id, agent.as_deref(), false)
            });
        },
    )
}
