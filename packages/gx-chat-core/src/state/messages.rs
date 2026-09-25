//! The authoritative transcript: the merged message list, where the stream stands, and how far
//! back the history window reaches.
//!
//! Owned by family a. Family b projects `composed` into rows and must not write anything here.
//!
//! Anti-drop law, carried over from `packages/core-ui/chat/session-chat-merge.ts`: the live list
//! only ever grows. Reads window the history they seed; appends are never trimmed, because a trim
//! removes the OLDEST rows and the pagination cursor cannot reach them again.

use std::collections::BTreeMap;

use ghostex_gx_protocol::ChatMessage;

use crate::session::fold::FoldedSnapshot;
use crate::wire::StreamPosition;

/// The transcript and its bookkeeping.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MessagesState {
    /// The authoritative list in first-seen order, id-deduplicated by the merger.
    pub list: Vec<ChatMessage>,
    /// `id` to index into `list`. Ordered so a rebuild is deterministic across platforms.
    pub index_by_id: BTreeMap<String, usize>,
    /// The list the renderer sees: the transcript plus markers, terminal statuses, the streaming
    /// bubble and the pending echoes, in the composition order of `controller.ts`.
    pub composed: Vec<ChatMessage>,
    /// The retained fold: every frame and read merged into one read result.
    ///
    /// This is what the store keeps and what persistence writes, so it stays a wire value rather
    /// than being flattened into the fields above.
    pub snapshot: Option<FoldedSnapshot>,
    /// Where the accepted stream stands.
    pub position: FramePosition,
    /// The window the next read asks for. Grows with the live list so a reconnect's snapshot never
    /// comes back smaller than what is already on screen.
    pub limit: u32,
    /// Byte cursor after the most recently accepted history window.
    pub before_offset: u64,
    pub has_more: bool,
    pub loading_earlier: bool,
    /// The epoch the detail-mode history prefix belongs to, or `None` when no prefix is held.
    pub history_epoch: Option<i64>,
    /// How many rows of that prefix are held, so the read window can exclude them.
    pub history_prefix_count: usize,
    /// The cursor the automatic boundary fill already tried, so it runs once per cursor.
    pub boundary_attempt: Option<u64>,
    /// The page read in flight, if any.
    pub load_earlier_request: Option<LoadEarlierRequest>,
    /// The reads family a has asked for and not seen answered, oldest first.
    ///
    /// The TypeScript kept each read's identity in the closure that awaited it; the core has no
    /// closures, so the same facts ride here and the settled request id picks the row back out.
    pub reads: Vec<OutstandingRead>,
    /// Bumped every time the subscription is rebuilt. A read in flight across the swap belongs to
    /// the previous conversation and must not apply.
    pub generation: u64,
    pub resync: ResyncState,
    /// When a frame, or a read that stood in for one, last reached this session.
    pub last_frame_at_ms: f64,
    /// When the stall watchdog last asked for a resync, so it cannot fire twice in a row.
    pub last_watchdog_resync_at_ms: f64,
    /// Automatic socket recycles spent on this session. Bounded, because a transport that is
    /// simply down would otherwise reconnect forever.
    pub auto_reconnects: u32,
    /// When the current seed-read patience window opened.
    pub seed_started_at_ms: f64,
    /// How many seed reads have been retried inside that window.
    pub seed_attempt: u32,
    /// `JSON.stringify([machineId, projectId, sessionId])`, the retained record's own key.
    ///
    /// The host builds it (it is the one that knows the machine) and passes it in with
    /// [`crate::event::StartConfig::retained_key`]; the core writes it back inside the record so
    /// the bytes match what an installed Ghostex already has on disk.
    pub retained_key: String,
    /// `updatedAt`: when the fold the next write will carry was made.
    pub retained_saved_at_ms: f64,
    /// When the debounced retained write is due, or `None` when none is pending.
    pub persist_due_ms: Option<f64>,
    /// The fold the last [`crate::session::persistence::schedule`] was made for, so one fold asks
    /// for one write.
    pub persisted_revision: u64,
    /// Bumped whenever an authoritative frame or read replaces the folded snapshot.
    ///
    /// The composed list the projection read was `useMemo`'d over that fold in the TypeScript, so
    /// a new fold was a new array even when the rows in it were identical, and `take`'s identity
    /// test shipped a splice for it. This counter is what stands in for that identity.
    pub authoritative_revision: u64,
    /// The identity of the composed list as the TypeScript's `messages` memo saw it.
    ///
    /// `NativeChatPresentation.update` rebuilt its item list when `state.messages` was a NEW
    /// ARRAY, and the memo that built it re-ran when one of its dependencies changed identity:
    /// `setTranscript` after every fold that the assembler cannot treat as a suffix extension,
    /// `setAppCommands` on every carrier that names the field, `setTerminalStream` on every
    /// activity that is a stream, `setPending` and `setMarkers` through `filter` and `map`, and
    /// every queue answer. None of those compared by value, so a frame that changed nothing a row
    /// can show still shipped a degenerate splice, and the core ships it on the same turn.
    /// Bumped by [`MessagesState::new_composition_identity`] at exactly those sites; a value
    /// change of the composed list rebuilds on its own.
    pub composition_identity: u64,
    /// The transcript after assembly, skill surfacing and the `/clear` boundary, as the last
    /// composition saw it (`boundaried` in `controller.ts`), with the inputs it was built from.
    ///
    /// The TypeScript memoises it on the transcript's identity and the markers; rebuilding it on
    /// every event re-keyed and re-sorted the whole transcript once a second on a working session.
    pub boundaried: Vec<ChatMessage>,
    pub boundaried_key: Option<crate::session::composition::BoundariedKey>,
    /// The dependencies the last composed list was built from (the `messages` memo's deps).
    pub compose_key: Option<crate::session::composition::ComposeKey>,
    /// Bumped every time the composed list is rebuilt: the memo's output is a new array whenever
    /// it re-ran, equal rows or not, and the projection compares that identity.
    pub compose_generation: u64,
    /// Which `deferredWork` OBJECT each user message carries, by message id.
    ///
    /// A completed turn's item holds `deferred: item.turn.user.deferredWork` by reference, and
    /// `reuseItems` keeps the previous item only when every field is the same object, so a read or
    /// frame that hands the controller a new user message with an equal but new `deferredWork`
    /// ships that turn in the splice again. A value compare cannot see that; this token can. It
    /// moves when a message arrives carrying its own `deferredWork` and stays when
    /// `applyAuthoritative` carries the old object over (`{ ...message, deferredWork: old... }`).
    pub deferred_objects: BTreeMap<String, u64>,
    /// Which OBJECT each message is, by message id: moved every time the message arrives in a
    /// read, a frame or a page, because each of those hands the controller a new object.
    ///
    /// A row outside the eager tail ships as a placeholder until its backfill batch runs, and the
    /// placeholder is cached per message OBJECT (`placeholders` is a `WeakMap`), so the same row
    /// arriving again before it is backfilled is a new placeholder, a new item, and a splice.
    pub message_objects: BTreeMap<String, u64>,
    /// The last token handed out in `deferred_objects` and `message_objects`.
    pub deferred_object_counter: u64,
}

impl MessagesState {
    /// The TypeScript handed a setter the composition depends on a new object.
    pub fn new_composition_identity(&mut self) {
        self.composition_identity = self.composition_identity.wrapping_add(1);
    }

    /// `message` arrived as a new object. Its `deferredWork`, if it has one, is a new one too
    /// unless `carried` (`{ ...message, deferredWork: old.deferredWork }`).
    pub fn note_arrival(&mut self, message: &ChatMessage, carried: bool) {
        self.deferred_object_counter += 1;
        self.message_objects
            .insert(message.id.clone(), self.deferred_object_counter);
        if message.deferred_work.is_some() && !carried {
            self.deferred_objects
                .insert(message.id.clone(), self.deferred_object_counter);
        }
    }
}

/// Where the accepted stream stands, and whether it ever started.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FramePosition {
    /// The follower generation, or `None` before the first authoritative frame or read.
    pub epoch: Option<i64>,
    pub seq: i64,
    /// The daemon that owns the epoch. A different one restarts the numbering.
    pub server_id: String,
    /// Whether any frame has arrived at all, which is what the initial-window watchdog gates on.
    pub frame_arrived: bool,
}

impl FramePosition {
    /// This position as a comparable stream position.
    pub fn stream_position(&self) -> StreamPosition {
        StreamPosition {
            server_id: self.server_id.clone(),
            epoch: self.epoch.unwrap_or(0),
            seq: self.seq,
        }
    }
}

/// The resync flight's own bookkeeping, kept apart from the page read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResyncState {
    /// A resync read is in flight; a second gap must not start another.
    pub in_flight: bool,
    /// The newest frame position observed while that read was in flight, because the read answers
    /// from a position the server captured before it.
    pub seen_in_flight: Option<StreamPosition>,
    /// Paced follow-ups spent chasing bytes a successful read outran.
    pub follow_ups: u32,
    /// Reads that never landed, retried on their own backoff. A shared counter would let one
    /// budget exhaust the other.
    pub failures: u32,
}

/// Why a read was issued, which decides what its answer is allowed to do.
///
/// The three lanes are separate in the TypeScript too (`seedRead`, `requestResync`, `loadEarlier`),
/// and they must not settle each other: a failed page leaves the live tail valid, while a failed
/// seed read inside the patience window retries and outside it becomes the view's error state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadKind {
    /// The first window, and its retries while the session is still starting.
    #[default]
    Seed,
    /// A gap, a stall, or the user's Refresh.
    Resync,
    /// One "Load earlier" page.
    Page,
}

/// One read family a is waiting on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutstandingRead {
    /// The id that rode out with [`crate::Effect::SendRpc`].
    pub request_id: u64,
    pub kind: ReadKind,
    /// The subscription generation that issued it. An answer from an older one belongs to the
    /// previous conversation and must not apply.
    pub generation: u64,
    /// The `beforeOffset` a page read asked for, which its `hasMore` verdict is measured against.
    pub before_offset: Option<u64>,
    /// When it went out, for the read deadline that settles the state machine.
    pub started_at_ms: f64,
}

/// The page read the user asked for, identified so a cancelled answer cannot complete a newer
/// request in the same epoch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadEarlierRequest {
    pub epoch: Option<i64>,
    pub before_offset: u64,
    /// The generation that issued it.
    pub generation: u64,
}
