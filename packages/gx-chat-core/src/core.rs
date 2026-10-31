//! The state machine: events in, document plus effects out.
//!
//! One owner, no I/O, no clock read. The core holds a [`ChatState`], routes each event to the
//! family that owns it (`crate::dispatch`), and reassembles the [`Document`] from the whole state
//! afterwards (`crate::document::assemble`). The port fills the family handlers one directory at a
//! time (`docs/2026-09-21/rust-chat/PLAN.md` step 3) without this file changing.

use crate::dispatch::events;
use crate::document::{
    assemble, frame_parts, frame_parts_with_items, Document, Frame, FrameParts, ItemsSplice,
    MinimapMarker, RowDetails, TranscriptItem,
};
use crate::effect::Effect;
use crate::event::Event;
use crate::state::{ChatContext, ChatState};
use crate::transcript::items::ItemRows;

/// One chat's brain.
///
/// The host owns the socket, storage and timers, feeds this events, and hands [`ChatCore::frame`]
/// to the renderer. One instance per session lives in the store, so switching chats is a pointer
/// swap rather than a teardown.
#[derive(Clone, Debug, Default)]
pub struct ChatCore {
    state: ChatState,
    document: Document,
    parts: FrameParts,
    /// Bumped on every publish, so the host can ask for "only what changed since N".
    revision: u64,
    /// The clock and locale the host last passed in. The core never reads either itself.
    context: ChatContext,
    /// The context the published document was assembled at.
    ///
    /// The change test runs at THIS clock rather than the current one, which is what keeps the
    /// publish decision the TypeScript's: its document is a function of state and `Date.now()`, but
    /// it republishes only when the STATE changed, so a label that merely counts seconds never
    /// ships a new snapshot on its own.
    published_context: ChatContext,
    /// The moment the host was last told to wake at, in its epoch milliseconds, so a
    /// [`Effect::SetTimer`] is emitted only when the earliest deadline actually moved.
    ///
    /// CDXC:SessionChat 2026-09-25 WHY: A `Tick` forgets it. The backfill timer re-arms itself at
    /// 0 ms after every batch; compared with the wake the host had already spent, the re-arm looked
    /// unchanged, no `SetTimer` went out, and every row older than the first batch stayed a
    /// placeholder skeleton until some unrelated event moved the wake.
    /// CDXC:SessionChat 2026-10-23 WHY: Kept as a moment rather than a delay. The delay to an
    /// unchanged deadline shrinks as time passes, so every event re-sent the same wake and the
    /// host's `setTimer` count followed the events it handled rather than the timers that moved.
    armed_wake_at_ms: Option<f64>,
    /// What the host was last sent, so a frame carries only the changed window.
    sent: SentFrame,
    /// Bumped whenever `republish` rebuilds the frame parts, which stands in for the array
    /// identity `take` compares on.
    parts_revision: u64,
    /// Whether a publish has happened at all.
    ///
    /// `native-host.ts` started with its OWN empty `minimapMarkers` and `subagentItems` arrays
    /// (native-host.ts:145, :149) and replaced both inside `publish` with the projection's rail and
    /// the viewer's list. So a drain before the first publish shipped the module's arrays and the
    /// drain after it shipped the replacements, which are different objects however equal they are.
    /// The core has no identities, so the first publish clears those two pointers by hand.
    published_once: bool,
    /// Whether `parts.items` is the projection's current list, so a publish the projection did not
    /// rebuild for can keep it instead of copying and comparing every row again.
    parts_items_current: bool,
}

/// The parts the host already has, which is what turns a whole list into a splice.
///
/// `native-host.ts` kept one `sent…` variable per channel and compared by array identity; unchanged
/// items kept their identity there (`native-presentation.ts` cached its projection), so comparing
/// by value here is the same test.
#[derive(Clone, Debug, Default)]
pub struct SentFrame {
    /// The parts counter the host was last sent, so a drain after a publish ships all four
    /// channels and a drain without one ships none.
    parts_revision: u64,
    items: Option<std::sync::Arc<crate::transcript::items::TranscriptItems>>,
    item_identities: Vec<u64>,
    subagent_items: Option<Vec<TranscriptItem>>,
    minimap: Option<Vec<MinimapMarker>>,
    row_details: Option<RowDetails>,
}

impl ChatCore {
    /// A core with nothing loaded yet: the document says "loading" and draws the skeleton.
    ///
    /// Assembled rather than hand-built, so an event that changes nothing the document can see
    /// leaves the revision at zero and the host's first drain ships no snapshot, which is where the
    /// TypeScript starts too (`start` pushes its boot read and publishes nothing).
    pub fn new() -> Self {
        let mut core = Self::default();
        core.document = assemble(&core.state, &core.context);
        core.parts = frame_parts(&core.state, &core.context);
        core
    }

    /// Applies one event and returns what the host must do.
    ///
    /// `context` carries the host's clock and UTC offset for this turn; every deadline, elapsed
    /// label, retry backoff and local-midnight boundary is measured against it, which is what
    /// makes the same inputs produce the same document.
    pub fn handle(&mut self, event: Event, context: ChatContext) -> Vec<Effect> {
        self.context = context;
        // Index zero of the turn's clock reads is `now_ms` itself.
        self.state.core.clock_cursor = 1;
        // A tick is the host's timer firing, so the host holds no timer now: the wake it was told
        // is spent, and the next one must reach it even when it is the same delay.
        if matches!(event, Event::Tick) {
            self.armed_wake_at_ms = None;
        }
        // `dispatch` routes the event to its owner and then runs the six per-family settle hooks
        // in a fixed order, which is where every `useMemo` and `useEffect` of the TypeScript lives.
        //
        // A batch storage answer is ONE host round trip carrying several records, so it is
        // expanded into the per-key answers it stands for and dispatched in order. Every family's
        // existing arm then serves it unchanged, and the whole batch still publishes once: the
        // republish below runs after the last of them.
        let mut effects = Vec::new();
        // A hidden chat held its redraw clocks and its accounts poll; the tick right after the
        // show fires whatever they owe, so the document this turn publishes is already current.
        let shown_again =
            self.state.core.hidden && matches!(event, Event::Visibility { shown: true });
        for expanded in expand(event) {
            effects.extend(events::dispatch(&mut self.state, &expanded, &self.context));
        }
        if shown_again {
            effects.extend(events::dispatch(
                &mut self.state,
                &Event::Tick,
                &self.context,
            ));
        }
        self.republish();
        // Read aloud's speech rides after the publish that shows it (`read_aloud/playback.rs`).
        effects.append(&mut self.state.read_aloud.outbox);
        // One wake for the whole core, not one per timer: the table knows which key is earliest,
        // and the host only has to be asked again when that answer changed.
        let wake = self.next_wake_ms();
        // The delay is rounded to a millisecond, so two reads of one deadline land within one.
        let wake_at = wake.map(|delay| self.context.now_ms + delay as f64);
        let moved = match (wake_at, self.armed_wake_at_ms) {
            (Some(at), Some(armed)) => (at - armed).abs() >= 1.0,
            (None, None) => false,
            _ => true,
        };
        if moved {
            self.armed_wake_at_ms = wake_at;
            effects.push(Effect::SetTimer { delay_ms: wake });
        }
        effects
    }

    /// Milliseconds until the core's earliest armed deadline, or `None` when nothing is armed.
    pub fn next_wake_ms(&self) -> Option<u64> {
        self.state.core.next_wake_ms(self.context.now_ms)
    }

    /// The state every family reads.
    pub fn state(&self) -> &ChatState {
        &self.state
    }

    /// The state, for the host that seeds a core from what it had cached.
    pub fn state_mut(&mut self) -> &mut ChatState {
        &mut self.state
    }

    /// What the renderer should draw right now.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The current publish revision.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The clock and locale the host last passed in.
    pub fn context(&self) -> &ChatContext {
        &self.context
    }

    /// The frame for a host whose last seen revision is `last_revision`.
    ///
    /// The document is left out when the host is already current, which is what keeps a
    /// once-a-second status frame from shipping the whole transcript.
    pub fn frame(&mut self, last_revision: u64) -> Frame {
        let now_ms = self.context.now_ms;
        self.frame_at(last_revision, now_ms)
    }

    /// The same frame, with `nextWakeMs` measured at the host's clock AT THE DRAIN.
    ///
    /// `take` in `native-host.ts` read `Date.now()` itself, so its wake is the remaining delay
    /// from the moment the host asks, not from the last event the core handled. A host that drains
    /// on a different turn from the one it last fed (which is every host: it ticks, then takes)
    /// would otherwise arm its timer that much too late.
    pub fn frame_at(&mut self, last_revision: u64, now_ms: f64) -> Frame {
        // `take` compared its four channels by IDENTITY, so a rebuild that reused every row
        // shipped the degenerate splice `{start: length, deleteCount: 0, items: []}`. The core's
        // projection revision stands in for that identity, and a splice that moves nothing is then
        // left out of the frame.
        // CDXC:SessionChat 2026-10-23 WHY: A channel that changes nothing on the receiver is not
        // sent: an empty splice, or an empty rail or row-detail map to a renderer that was never
        // sent one (it starts empty). Each still woke the view, which copied its whole row list
        // (desktop) or rebuilt it (phone) and redrew, and a backfill pass shipped one per pass.
        let rebuilt = self.sent.items.is_none() || self.parts_revision != self.sent.parts_revision;
        self.sent.parts_revision = self.parts_revision;
        let mut items_splice = None;
        if rebuilt {
            let moved = splice(
                self.sent
                    .items
                    .as_deref()
                    .map(|items| (items, self.sent.item_identities.as_slice())),
                (self.parts.items.as_ref(), &self.parts.item_identities),
            );
            self.sent.items = Some(std::sync::Arc::clone(&self.parts.items));
            self.sent.item_identities = self.parts.item_identities.clone();
            items_splice = (!moved.changes_nothing()).then_some(moved);
        }
        // The other three keep their identity when their content does: the minimap rail caches its
        // own projection, the subagent viewer hands back one stable list, and `rowDetails` is a
        // STRING, so `sentRowDetails === rowDetails` is a value comparison already.
        let subagent_splice =
            if self.sent.subagent_items.as_deref() == Some(self.parts.subagent_items.as_slice()) {
                None
            } else {
                let splice = splice(
                    self.sent
                        .subagent_items
                        .as_deref()
                        .map(|items| (items, &[][..])),
                    (self.parts.subagent_items.as_slice(), &[]),
                );
                self.sent.subagent_items = Some(self.parts.subagent_items.clone());
                (!splice.changes_nothing()).then_some(splice)
            };
        let minimap = if self.sent.minimap.as_deref() == Some(self.parts.minimap.as_slice()) {
            None
        } else {
            let first = self.sent.minimap.is_none();
            self.sent.minimap = Some(self.parts.minimap.clone());
            (!(first && self.parts.minimap.is_empty())).then(|| self.parts.minimap.clone())
        };
        let row_details = if self.sent.row_details.as_ref() == Some(&self.parts.row_details) {
            None
        } else {
            let first = self.sent.row_details.is_none();
            self.sent.row_details = Some(self.parts.row_details.clone());
            (!(first && self.parts.row_details.is_empty())).then(|| self.parts.row_details.clone())
        };
        Frame {
            items_splice,
            minimap,
            subagent_splice,
            row_details,
            revision: self.revision,
            snapshot: if last_revision == self.revision {
                None
            } else {
                Some(Box::new(self.document.clone()))
            },
            requests: Vec::new(),
            next_wake_ms: self.state.core.next_wake_ms(now_ms),
        }
    }

    /// Forgets what the host was last sent, so the next frame ships every channel whole.
    ///
    /// For a renderer that attaches to a core another renderer was drained from. The TypeScript
    /// never needed it, because each view booted its own brain with nothing sent; a host that keeps
    /// one core per session across views must call it, or the new view receives only the splice
    /// against the rows the OLD view held and draws an empty transcript.
    pub fn forget_sent(&mut self) {
        self.sent = SentFrame::default();
    }

    /// [`Self::frame_at`] for one of several renderers drawing the same chat, each keeping its own
    /// record of what it was sent. A new renderer starts from `SentFrame::default()` and so gets
    /// every channel whole.
    ///
    /// For a host that shows one chat in more than one window (the desktop's File > New Window):
    /// the splices are relative to what THIS renderer holds, so draining one renderer never
    /// leaves another splicing against rows it was not sent.
    pub fn frame_for(&mut self, sent: &mut SentFrame, last_revision: u64, now_ms: f64) -> Frame {
        std::mem::swap(&mut self.sent, sent);
        let frame = self.frame_at(last_revision, now_ms);
        std::mem::swap(&mut self.sent, sent);
        frame
    }

    /// The id for the next request the core asks for.
    ///
    /// One counter for the whole core, on the state, because every family draws from it and
    /// [`crate::Event::RpcSettled`] routes by id alone.
    pub fn allocate_request_id(&mut self) -> u64 {
        self.state.core.allocate_request_id()
    }

    /// Reassembles the document and bumps the revision when it changed.
    ///
    /// Only a real change bumps, because the revision is what the host's change gate reads: a bump
    /// with identical content would ship the whole document once a second for nothing.
    pub fn republish(&mut self) {
        crate::session::working::stamp_working_started(&mut self.state, self.context.now_ms);
        // The two `useEffect`s the TypeScript ran before the composition read their state: the
        // pending echoes pruned against the authoritative list, and the pending tool row dropped
        // once the transcript retired it.
        crate::session::before_compose(&mut self.state, &self.context);
        let working = crate::session::working::is_working(&self.state);
        crate::session::composition::compose_cached(
            &mut self.state,
            &crate::session::constants::DEFAULT_COMMAND_CATALOG
                .iter()
                .map(|name| (*name).to_string())
                .collect::<Vec<_>>(),
            None,
            working,
        );
        // `presentation.update(state.messages, …)` runs here in `publish`, on the list the
        // composition above has just produced.
        crate::transcript::rows::refresh(&mut self.state, &self.context);
        // A "Go to chat" on a Resources item older than the loaded rows scrolls once they hold it.
        crate::questions::resource_modal::after_refresh(&mut self.state, &self.context);
        // Auto-read reads the `finalIds` the refresh above just computed.
        crate::read_aloud::observe(&mut self.state, &self.context);
        // The test is "did the STATE change", asked in the only terms the core has: assemble the
        // new state at the clock the published document was assembled at. Equal means nothing but
        // the clock moved, and the TypeScript would not have published either, so the host keeps
        // the snapshot it has (its `snapshot` variable is not refreshed without a publish).
        let requested = std::mem::take(&mut self.state.core.publish_requested);
        let rendered = std::mem::take(&mut self.state.core.render_requested);
        let chain_continued = std::mem::take(&mut self.state.core.chain_continued);
        // `start`'s `.catch`: `transcriptItems = []; snapshot = {status: 'error', error};
        // revision++`. It runs with no controller at all, so it is decided before the gate below
        // and replaces the document rather than assembling one.
        if let Some(error) = self.state.core.boot_error.take() {
            self.document = crate::document::Document {
                status: "error".to_string(),
                error: ghostex_gx_protocol::Tri::Value(error),
                ..Default::default()
            };
            self.parts = FrameParts::default();
            self.parts_items_current = false;
            self.state.transcript_view.projection_revision += 1;
            self.parts_revision = self.state.transcript_view.projection_revision;
            self.published_context = self.context.clone();
            self.revision += 1;
            return;
        }
        // `if (controller) publish(...)`: before the boot read answers there is no controller, so
        // nothing the core has done can ship yet and the host's first drain is empty.
        if !self.state.core.controller_started {
            return;
        }
        if chain_continued && !requested {
            return;
        }
        let mut probe = assemble(&self.state, &self.published_context);
        // A rebuilt projection is a publish of its own: `update` rebuilds only when one of its
        // inputs changed identity, every one of those is a `useState` value the lifecycle
        // republishes on, and `take` then ships the new (if equal) array as a degenerate splice.
        // Holding the flag until the next real change shipped that splice a turn late.
        let rebuilt = self.state.transcript_view.projection_rebuilt;
        // The transcript items only move on a rebuild, so the probe compares the three small
        // channels and leaves the items alone: cloning every projected row twice per event was
        // most of a state frame's cost on a long chat.
        // The minimap rail is written only by the projection rebuild, which is `rebuilt` itself,
        // so it is not serialized again here to be compared on every other event; the subagent
        // list is compared where it is stored instead of through a copy.
        let parts_moved = rebuilt
            || !self
                .state
                .extras
                .subagent
                .view
                .items
                .eq_slice(&self.parts.subagent_items)
            || crate::transcript::row_details(&self.state, &self.published_context)
                != self.parts.row_details;
        // `probe.reactive() != self.document.reactive()` without copying both documents: the one
        // key `reactive` blanks is made equal on the probe, which is dropped right after.
        probe.async_questions = self.document.async_questions.clone();
        let state_moved = probe != self.document;
        if !requested && !parts_moved && !state_moved {
            return;
        }
        // A publish that follows a state change re-rendered the controller, and the render is
        // where `nativeAccountPanel` reads the clock; one that did not (a fleet tick, a backfill
        // batch, an action's close) ships the last render's panel.
        let quiet = std::mem::take(&mut self.state.core.quiet_action);
        if rendered || (state_moved && !quiet) || !self.published_once {
            // The LAST render of the call is the one that published, and a call that renders
            // twice (a page answer, then the loading flag) reads its panel clock twice.
            self.state.menus.panel_clock_ms = Some(self.context.clock_read(usize::MAX));
        }
        let document = assemble(&self.state, &self.context);
        // The rows only change when the projection rebuilt (`transcript::rows` hands back its
        // list), so without a rebuild the list already held is kept: copying it and comparing it
        // row by row on every publish was most of a keystroke's cost on a long chat.
        let keep_items =
            !rebuilt && self.parts_items_current && !self.state.transcript_view.items.is_empty();
        let parts = if keep_items {
            let items = std::mem::take(&mut self.parts.items);
            frame_parts_with_items(&self.state, &self.context, items)
        } else {
            frame_parts(&self.state, &self.context)
        };
        // CDXC:SessionChat 2026-10-01 WHY: The imperative publishes above (`requested`) ship
        // whether or not anything moved, and every keystroke asks for one (`composerSelection`
        // follows the caret, the composer measures itself), so a typing user received about ten
        // snapshots a second identical to the one before, and a streaming reply shipped the
        // unchanged document again beside every row splice; each one redraws the whole chat on the
        // desktop, the web build and the phone. The revision, which is what puts the document in a
        // frame, now moves only when the document did; the rows, the minimap, the subagent list and
        // the row details still ship whenever they change, on their own channels.
        let document_moved = !self.published_once || document != self.document;
        // Compared only when the answer matters: a rebuild publishes whatever the parts say.
        let parts_unchanged = !document_moved
            && !rebuilt
            && if keep_items {
                parts.item_identities == self.parts.item_identities
                    && parts.subagent_items == self.parts.subagent_items
                    && parts.minimap == self.parts.minimap
                    && parts.row_details == self.parts.row_details
            } else {
                parts == self.parts
            };
        if parts_unchanged {
            if keep_items {
                self.parts.items = parts.items;
            }
            return;
        }
        self.document = document;
        self.parts = parts;
        self.parts_items_current = !self.state.transcript_view.items.is_empty();
        if std::mem::take(&mut self.state.transcript_view.projection_rebuilt) {
            self.state.transcript_view.projection_revision += 1;
        }
        self.parts_revision = self.state.transcript_view.projection_revision;
        self.published_context = self.context.clone();
        if document_moved {
            self.revision += 1;
        }
        if !self.published_once {
            self.published_once = true;
            self.sent.minimap = None;
            self.sent.subagent_items = None;
        }
    }

    /// Replaces the document and bumps the revision, for a host seeding a cached frame.
    pub fn publish(&mut self, document: Document) {
        self.document = document;
        self.published_context = self.context.clone();
        self.revision += 1;
    }
}

/// One event, or the per-key answers a batch answer stands for.
///
/// `composer('asyncQuestionRead')` and `composer('asyncQuestionRetire')` each touch two stores and
/// answer once, so the core asks for them as one effect and hears back one event. Every family
/// reads storage per key, so the answer is fanned out here rather than in six settle hooks.
fn expand(event: Event) -> Vec<Event> {
    match event {
        Event::StorageBatchLoaded { records } => records
            .into_iter()
            .map(|record| Event::StorageLoaded {
                key: record.key,
                value: record.value,
            })
            .collect(),
        Event::StorageBatchWritten { keys, error } => keys
            .into_iter()
            .map(|key| Event::StorageWritten {
                key,
                error: error.clone(),
            })
            .collect(),
        other => vec![other],
    }
}

/// `itemsSplice(previous, next)`: replace `deleteCount` items at `start` with `items`.
///
/// CDXC:SessionChat 2026-09-18 WHY:
/// A live status row changes once a second, and shipping the whole transcript for it cost about 1MB
/// of JSON per frame on a 139-message session. Only the changed window ships; GPUI
/// splices its item list and list state the same way.
/// SEE-ALSO: apps/desktop/src/app/native_chat/state.rs (pump).
///
/// Each side is the items and their identities ([`FrameParts::item_identities`]): an item is the
/// same object only when its value AND its identity are unchanged.
///
/// A row both lists hold from one shared chunk (the settled turns, `transcript/items.rs`) is the same
/// value, so only its identity is compared.
fn splice<Rows: ItemRows + ?Sized>(
    previous: Option<(&Rows, &[u64])>,
    next: (&Rows, &[u64]),
) -> ItemsSplice {
    let (next, next_identities) = next;
    let Some((previous, previous_identities)) = previous else {
        return ItemsSplice {
            start: 0,
            delete_count: 0,
            items: (0..next.row_count())
                .map(|at| next.row(at).clone())
                .collect(),
            length: next.row_count(),
        };
    };
    let identity = |identities: &[u64], at: usize| identities.get(at).copied().unwrap_or_default();
    let same = |left: usize, right: usize| {
        identity(previous_identities, left) == identity(next_identities, right)
            && (previous.same_row(left, next, right) || previous.row(left) == next.row(right))
    };
    let (previous_len, next_len) = (previous.row_count(), next.row_count());
    let limit = previous_len.min(next_len);
    let mut start = 0;
    while start < limit && same(start, start) {
        start += 1;
    }
    let mut end = 0;
    while end < limit - start && same(previous_len - 1 - end, next_len - 1 - end) {
        end += 1;
    }
    ItemsSplice {
        start,
        delete_count: previous_len - start - end,
        items: (start..next_len - end)
            .map(|at| next.row(at).clone())
            .collect(),
        length: next_len,
    }
}
