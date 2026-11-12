use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::session_chat::*;
use crate::session_chat_follower::SUCCESSOR_SCAN_INTERVAL;
use crate::session_chat_successor::SUCCESSOR_STALE_SUBSTANTIVE_IDLE_MS;

/// CDXC:SessionChat 2026-10-14 WHY:
/// Clients order chat frames by `(serverId, epoch, seq)` and drop a frame at or behind the position they hold as a duplicate. `serverId` is the machine's id, which a gxserver restart keeps, and every follower counted its epochs from 1, so after a restart (or a follower entry made again) the desktop chat read the new follower's snapshot and frames as stale copies of the old one's and dropped them until the new seq passed the old: the transcript froze, and a message sent then stayed drawn as "Queued" after the agent had already answered it (2026-10-14, 20 s after a restart). Every generation now takes its epoch from one counter for the whole process that starts at the wall clock in milliseconds, so a new process or a new follower always starts above every epoch handed out before it.
/// SEE-ALSO: `snapshot_is_stale` and `accept_sequenced_frame` in packages/gx-chat-core/src/session/stream.rs; `notePosition` in apps/mobile/app/src/chat/rust/transport.ts.
fn next_stream_epoch() -> i64 {
    static NEXT: std::sync::OnceLock<AtomicI64> = std::sync::OnceLock::new();
    NEXT.get_or_init(|| AtomicI64::new(now_epoch_ms()))
        .fetch_add(1, Ordering::SeqCst)
        + 1
}

pub struct SessionChatStream {
    epoch: AtomicI64,
    seq: AtomicI64,
    /*
    CDXC:SessionChat 2026-08-01:
    Two threads publish into one seq counter: the follower task and hook ingest
    (prompt/activity state frames). Taking the seq and broadcasting as separate
    steps let thread B's frame reach the hub BEFORE thread A's lower seq, and
    the client treats an out-of-order seq as a gap and forces a full resync.
    Every publisher must therefore hold this lock across "take seq + broadcast",
    which `emit_sequenced` does; `begin_generation` takes it too so an epoch
    rollover can never interleave with an in-flight frame.
    */
    emit_order: std::sync::Mutex<()>,
    /// The generation's latest frames, for a client that resumes ([`SessionChatStream::resume`]).
    recent: std::sync::Mutex<RecentFrames>,
}

/// CDXC:SessionChat 2026-11-12 DECISION:
/// The user chose to resume a chat from where it was after a reconnect (Q2720, "resume after reconnect"): every reconnect and every return to the phone app resubscribed each open chat and got its whole snapshot again (60 KiB for a two-turn chat, up to four warm chats). A client that holds a position of the current generation sends it with its subscribe (`resume`, behind the `chatResume` capability) and gets a `sessionChatResumed` frame and only the frames it missed, from this buffer of the generation's latest frames; a position this buffer cannot bridge (another generation, older than the buffer) gets the snapshot as before.
#[derive(Default)]
struct RecentFrames {
    frames: std::collections::VecDeque<(i64, Value, usize)>,
    bytes: usize,
}

const RECENT_FRAMES_MAX: usize = 128;
const RECENT_BYTES_MAX: usize = 512 * 1024;

impl RecentFrames {
    fn push(&mut self, seq: i64, frame: &Value) {
        let size = crate::server::chat_frame_dedupe::json_len(frame);
        self.frames.push_back((seq, frame.clone(), size));
        self.bytes += size;
        while self.frames.len() > RECENT_FRAMES_MAX || self.bytes > RECENT_BYTES_MAX {
            let Some((_, _, dropped)) = self.frames.pop_front() else {
                break;
            };
            self.bytes -= dropped;
        }
    }
}

impl SessionChatStream {
    pub fn new() -> Self {
        Self {
            epoch: AtomicI64::new(0),
            seq: AtomicI64::new(0),
            emit_order: std::sync::Mutex::new(()),
            recent: std::sync::Mutex::new(RecentFrames::default()),
        }
    }

    fn lock_recent(&self) -> std::sync::MutexGuard<'_, RecentFrames> {
        self.recent
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Brings a client at `(epoch, seq)` of this generation up to date, all under the emission
    /// lock: `announce` (given the current seq) and `send` (each frame after `seq`) queue for the
    /// client first, then `register` lets later frames reach it. A frame of this generation already
    /// on its way to the client's socket when it registers is one the client holds and drops; one
    /// queued ahead of the replay would read as a gap and force a full read. False, with nothing
    /// done, when the position is not one this buffer can bridge.
    pub fn resume(
        &self,
        epoch: i64,
        seq: i64,
        announce: impl FnOnce(i64),
        mut send: impl FnMut(Value),
        register: impl FnOnce(),
    ) -> bool {
        let _order = self.lock_emit_order();
        let (current_epoch, current_seq) = self.current();
        if epoch != current_epoch || current_epoch == 0 || seq > current_seq || seq < 0 {
            return false;
        }
        let recent = self.lock_recent();
        let missed: Vec<Value> = recent
            .frames
            .iter()
            .filter(|(frame_seq, _, _)| *frame_seq > seq)
            .map(|(_, frame, _)| frame.clone())
            .collect();
        if missed.len() as i64 != current_seq - seq {
            return false;
        }
        drop(recent);
        announce(current_seq);
        for frame in missed {
            send(frame);
        }
        register();
        true
    }

    /// Starts a new follower generation: takes a fresh epoch, resets seq to 0.
    pub fn begin_generation(&self) -> i64 {
        let _order = self.lock_emit_order();
        *self.lock_recent() = RecentFrames::default();
        self.seq.store(0, Ordering::SeqCst);
        let epoch = next_stream_epoch();
        self.epoch.store(epoch, Ordering::SeqCst);
        epoch
    }

    pub fn current(&self) -> (i64, i64) {
        (
            self.epoch.load(Ordering::SeqCst),
            self.seq.load(Ordering::SeqCst),
        )
    }

    fn lock_emit_order(&self) -> std::sync::MutexGuard<'_, ()> {
        self.emit_order
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Allocates the next seq and publishes the frame it builds while holding
    /// the emission lock, so frames always reach the hub in seq order.
    /// `build` must not block: `broadcast` is a synchronous channel send.
    pub fn emit_sequenced<Build, Broadcast>(&self, build: Build, broadcast: Broadcast)
    where
        Build: FnOnce(i64) -> Value,
        Broadcast: FnOnce(Value),
    {
        let _order = self.lock_emit_order();
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let frame = build(seq);
        self.lock_recent().push(seq, &frame);
        broadcast(frame);
    }
}

impl Default for SessionChatStream {
    fn default() -> Self {
        Self::new()
    }
}

/*
CDXC:AgentScreenDetection 2026-08-24:
A follower task that is alive but WEDGED — stuck in an inline await inside the
reconcile loop (the blocking drain, the steady-state detection probe, a path
re-resolution, a successor scan) — still passes `task.is_finished()`, so
`sync_session_chat_follower_for_session` reads it as healthy and never respawns
it. The transcript keeps growing while chat freezes, and hook ingest keeps
publishing "working" state frames, so the surface even looks alive.

The task therefore publishes its own progress: it stamps at the top of every
reconcile iteration, and marks itself PARKED before each await it is SUPPOSED
to sit in for a long time (the reconcile sleep, the resolve-poll backoff, the
unsupported-agent resnapshot wait), clearing the flag when that await returns.
"Not parked AND the stamp is older than the wedge deadline" is then the only
wedged signature, and it is the one the sync path aborts and respawns. Parked
waits are exempt, so the deadline never has to clear MAX_RESOLVE_POLL.
*/
pub struct SessionChatFollowerHeartbeat {
    last_progress_ms: AtomicI64,
    parked: AtomicBool,
}

impl SessionChatFollowerHeartbeat {
    pub fn new() -> Self {
        Self {
            last_progress_ms: AtomicI64::new(now_epoch_ms()),
            parked: AtomicBool::new(false),
        }
    }

    /// The task reached a new reconcile iteration / returned from an await.
    pub fn stamp(&self) {
        self.last_progress_ms
            .store(now_epoch_ms(), Ordering::SeqCst);
    }

    /// Entering a deliberate long wait: no stamp is expected until it returns.
    pub fn park(&self) {
        self.stamp();
        self.parked.store(true, Ordering::SeqCst);
    }

    pub fn unpark(&self) {
        self.parked.store(false, Ordering::SeqCst);
        self.stamp();
    }

    /// True when the task is running (not parked) but has not made progress
    /// within `deadline` — i.e. it is stuck in an inline await.
    pub fn is_wedged(&self, deadline: Duration) -> bool {
        if self.parked.load(Ordering::SeqCst) {
            return false;
        }
        let elapsed_ms = now_epoch_ms() - self.last_progress_ms.load(Ordering::SeqCst);
        elapsed_ms > deadline.as_millis() as i64
    }
}

impl Default for SessionChatFollowerHeartbeat {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Follower engine (upstream chat spec §5, poll-only: 1s reconcile owns liveness)
// ---------------------------------------------------------------------------

/// The session's CURRENT hook-derived state: the stored interactive prompt,
/// whether agent hooks report the session as working, and the provider session
/// identity the follower should be tailing right now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionChatLiveState {
    pub prompt: Option<SessionChatInteractivePrompt>,
    pub working: bool,
    pub agent_session_id: Option<String>,
    pub agent_session_path: Option<String>,
}

/// Reads `SessionChatLiveState` so authoritative frames carry the live prompt
/// and working flag. Kept as a closure so the follower stays decoupled from the
/// domain repository.
pub type SessionChatStateReader = Arc<dyn Fn() -> SessionChatLiveState + Send + Sync>;

/*
CDXC:SessionChat 2026-08-21:
Reads the session's Ghostex-owned prompt queue and synced composer draft so
snapshot / replaced / state frames carry them. Deliberately NOT read once per
reconcile tick: it opens the state database, so it is called only when a frame
is actually being emitted. `appended` frames never carry either field.
*/
pub type SessionChatQueueReader =
    Arc<dyn Fn() -> crate::session_chat_queue::SessionChatQueueSnapshot + Send + Sync>;

/// A proven successor transcript the follower wants to bind the session to.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionChatIdentityAdoption {
    /// Identity the follower believes the registry holds right now. The write
    /// is compare-and-set against it so a real hook observation that landed in
    /// the meantime always wins.
    pub previous_agent_session_id: Option<String>,
    /// Filename stem of the stale transcript the lineage was proven against
    /// (usually the same id, but resolution can reach a file by other means).
    pub predecessor_transcript_session_id: String,
    pub agent_session_id: String,
    pub agent_session_path: String,
    pub lineage: &'static str,
    pub hops: usize,
}

/// What the follower reports about a successor search; the server maps it to a
/// gxserver log line.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionChatSuccessorNotice {
    Adopted(SessionChatIdentityAdoption),
    AdoptionRejected {
        agent_session_id: String,
        reason: &'static str,
    },
    Ambiguous {
        predecessor_session_id: String,
        candidate_session_ids: Vec<String>,
    },
    /// A proven successor exists but a live session already owns it.
    OwnedByAnotherSession {
        predecessor_session_id: String,
        candidate_session_ids: Vec<String>,
    },
}

/*
CDXC:SessionIdentity 2026-08-02:
Successor adoption needs three things the follower must NOT own itself (they all
touch the domain database / logger): the ids already bound to other sessions, a
write of the corrected identity through the same passive path hook observations
use, and a log sink. They travel together so the spawn site wires them once.
*/
#[derive(Clone)]
pub struct SessionChatSuccessorHooks {
    /// Agent session ids bound to other sessions that could actually be tailing
    /// them (running / sleeping / provider-alive — NEVER stopped history rows).
    /// Read only when a successor search actually runs.
    pub bound_agent_session_ids: Arc<dyn Fn() -> Vec<String> + Send + Sync>,
    /*
    CDXC:SessionFork 2026-09-02:
    Creation time (epoch ms) of the OLDEST live session forked from this one
    that has not yet reported its own conversation id. Between a fork's launch
    and its first hook, the fork's transcript is a proven, unowned successor of
    this session's id; a follower scan landing in that window would adopt the
    child's conversation for the parent. Any candidate that began at or after
    this instant belongs to that pending child. `None` when no such child exists.
    The argument is the id being scanned for successors: a child that still
    carries THAT id (seeded from its launch) has not reported its own yet either.
    */
    pub pending_fork_child_since_ms: Arc<dyn Fn(&str) -> Option<i64> + Send + Sync>,
    /// The conversation (id, file) an agent that names its chat itself (Freebuff) started for this
    /// session, read only while no id is bound yet.
    pub unbound_agent_chat: Arc<dyn Fn() -> Option<(String, std::path::PathBuf)> + Send + Sync>,
    /// Persists the corrected identity; returns true when the registry changed.
    pub adopt_identity: Arc<dyn Fn(SessionChatIdentityAdoption) -> bool + Send + Sync>,
    pub log: Arc<dyn Fn(SessionChatSuccessorNotice) + Send + Sync>,
}

#[derive(Clone)]
pub struct SessionChatFollowerConfig {
    pub project_id: String,
    pub session_id: String,
    /// Raw agent id (`claude`, `openclaude`, `codex`, `grok`, …).
    pub agent: Option<String>,
    pub agent_session_id: Option<String>,
    pub agent_session_path: Option<String>,
    pub limit: usize,
    pub protocol_version: u64,
    pub server_id: String,
    pub state_reader: Option<SessionChatStateReader>,
    /// Detected model/effort source (see session_chat_options/). Snapshot and
    /// replaced frames carry the cached value; a periodic probe re-detects and
    /// emits a state frame only when it CHANGED.
    pub options_reader: Option<crate::session_chat_options::SessionChatOptionsReader>,
    /// CDXC:AgentScreenDetection 2026-09-03 WHY: answers "did the stored statusline
    /// payload for this agent session id change since I last asked?", so the
    /// reconcile loop re-probes within a tick of a `/model`, `/effort` or
    /// compaction instead of waiting for the idle 30s tier. Absent ⇒ the
    /// steady cadence alone.
    pub options_change_watch: Option<crate::session_chat_options::SessionChatOptionsChangeWatch>,
    /// Answers "did the live terminal grid change since I last looked?", so an
    /// idle session re-probes within a tick of a dialog appearing or closing
    /// in its terminal (see session_chat_screen_watch.rs). Absent ⇒ the
    /// steady cadence alone.
    pub screen_change_watch: Option<crate::session_chat_screen_watch::SessionChatScreenChangeWatch>,
    /// Queue + draft source for snapshot / replaced / state frames. Absent ⇒
    /// the fields are omitted, which clients read as "this daemon has no queue"
    /// and answer by hiding every queue control.
    pub queue_reader: Option<SessionChatQueueReader>,
    /// Registry access for successor-transcript adoption (claude only). Absent
    /// ⇒ the follower still re-resolves by the stored identity but never
    /// re-binds the session.
    pub successor_hooks: Option<SessionChatSuccessorHooks>,
    /// Pushes the session's current terminal notice to every live follower
    /// after the drain stores a transcript-derived notice (the API refusal
    /// card, CDXC:AgentScreenDetection). Absent ⇒ refusal rows still render
    /// inline as chat text but no notice card is raised.
    pub notice_publisher: Option<crate::session_chat_watchdog::SessionChatWatchdogPublisher>,
    /// Timers the reconcile loop runs on. Production uses `Default`; tests
    /// shrink them so the whole identity-repair path runs in milliseconds.
    pub tuning: SessionChatFollowerTuning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionChatFollowerTuning {
    pub reconcile_interval: Duration,
    /// Drain silence before the identity is re-derived.
    pub stale_transcript_idle: Duration,
    /// Minimum spacing between successor directory scans.
    pub successor_scan_interval: Duration,
    /// Age of the tailed transcript's newest `user`/`assistant` record before a
    /// successor may be adopted.
    pub successor_stale_substantive_idle_ms: i64,
}

impl Default for SessionChatFollowerTuning {
    fn default() -> Self {
        Self {
            reconcile_interval: RECONCILIATION_INTERVAL,
            stale_transcript_idle: STALE_TRANSCRIPT_IDLE,
            successor_scan_interval: SUCCESSOR_SCAN_INTERVAL,
            successor_stale_substantive_idle_ms: SUCCESSOR_STALE_SUBSTANTIVE_IDLE_MS,
        }
    }
}

pub type SessionChatFrameEmitter = Arc<dyn Fn(Value) + Send + Sync>;
