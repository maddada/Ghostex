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
    ) -> Self {
        Self {
            paths,
            server_id: server_id.into(),
            sender_factory,
            publisher_factory,
            notice_reader,
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
                    blocked.push((project_id, session_id, head.id.clone(), notice.title.clone()));
                    continue;
                }
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
