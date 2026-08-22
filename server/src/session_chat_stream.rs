use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::session_chat::*;
use crate::session_chat_follower::SUCCESSOR_SCAN_INTERVAL;
use crate::session_chat_successor::SUCCESSOR_STALE_SUBSTANTIVE_IDLE_MS;

pub struct SessionChatStream {
    epoch: AtomicI64,
    seq: AtomicI64,
    /*
    CDXC:SessionChatCore 2026-08-01:
    Two threads publish into one seq counter: the follower task and hook ingest
    (prompt/activity state frames). Taking the seq and broadcasting as separate
    steps let thread B's frame reach the hub BEFORE thread A's lower seq, and
    the client treats an out-of-order seq as a gap and forces a full resync.
    Every publisher must therefore hold this lock across "take seq + broadcast",
    which `emit_sequenced` does; `begin_generation` takes it too so an epoch
    rollover can never interleave with an in-flight frame.
    */
    emit_order: std::sync::Mutex<()>,
}

impl SessionChatStream {
    pub fn new() -> Self {
        Self {
            epoch: AtomicI64::new(0),
            seq: AtomicI64::new(0),
            emit_order: std::sync::Mutex::new(()),
        }
    }

    /// Starts a new follower generation: bumps epoch, resets seq to 0.
    pub fn begin_generation(&self) -> i64 {
        let _order = self.lock_emit_order();
        self.seq.store(0, Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst) + 1
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
        broadcast(build(seq));
    }
}

impl Default for SessionChatStream {
    fn default() -> Self {
        Self::new()
    }
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
CDXC:SessionChatQueueCarriage 2026-08-21:
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
CDXC:SessionChatIdentity 2026-08-02:
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
    /// Detected model/effort source (see session_chat_options.rs). Snapshot and
    /// replaced frames carry the cached value; a periodic probe re-detects and
    /// emits a state frame only when it CHANGED.
    pub options_reader: Option<crate::session_chat_options::SessionChatOptionsReader>,
    /// Queue + draft source for snapshot / replaced / state frames. Absent ⇒
    /// the fields are omitted, which clients read as "this daemon has no queue"
    /// and answer by hiding every queue control.
    pub queue_reader: Option<SessionChatQueueReader>,
    /// Registry access for successor-transcript adoption (claude only). Absent
    /// ⇒ the follower still re-resolves by the stored identity but never
    /// re-binds the session.
    pub successor_hooks: Option<SessionChatSuccessorHooks>,
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

