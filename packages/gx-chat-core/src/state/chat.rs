//! The one state every family reads and exactly one family writes.
//!
//! Six sub-states, six owners. Family a owns `identity`, `session`, `messages`, `pending` and
//! `core`; the other five each own the field named after their surface. A family writes its own
//! field and reads the rest, which is what lets six agents port in parallel without touching the
//! same file. `docs/2026-09-21/rust-chat/FAMILIES.md` is the full ownership table.

use crate::state::{
    ComposerState, ExtrasState, MenusState, MessagesState, PendingState, QuestionsState,
    SessionIdentity, SessionState, TranscriptViewState,
};

/// One chat's whole state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatState {
    /// Who this chat is. Family a.
    pub identity: SessionIdentity,
    /// The session facts the wire carries. Family a.
    pub session: SessionState,
    /// The authoritative transcript and its pagination. Family a.
    pub messages: MessagesState,
    /// The optimistic echoes and terminal lines. Family a.
    pub pending: PendingState,
    /// Errors and settings that belong to no single surface. Family a.
    pub core: CoreState,
    /// The transcript projection's own state. Family b.
    pub transcript_view: TranscriptViewState,
    /// Questions, approvals and notices. Family c.
    pub questions: QuestionsState,
    /// The composer. Family d.
    pub composer: ComposerState,
    /// Menus, pickers, options, accounts and context. Family e.
    pub menus: MenusState,
    /// The model picker, the model menu, model selection and the context surfaces. Family e2.
    pub pickers: crate::state::PickersState,
    /// The minimap, search, subagents, panels and the terminal tail. Family f.
    pub extras: ExtrasState,
}

/// The few things that belong to the seam rather than to a surface.
///
/// Family a owns this. The other families set `operation_error` through
/// [`CoreState::fail`] when an action of theirs is refused, because the refusal is drawn in one
/// place for all of them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CoreState {
    /// The last action refusal, shown above the composer.
    pub operation_error: Option<String>,
    /// The refusal's own code, so the composer can draw `composerNotReady` instead of an error
    /// line.
    pub operation_error_code: Option<String>,
    /// Masks account text everywhere the chat shows it.
    pub hide_account_emails: bool,
    /// The session's display title, or `None` when it has none.
    pub title: Option<String>,
    /// Every deadline the core is waiting on. Any family may arm one by key; the host only ever
    /// sees the earliest, as the frame's `nextWakeMs`.
    pub timers: crate::session::timers::TimerTable,
    /// The timer keys that came due during the dispatch running right now, cleared before the next
    /// one. A family reads its own keys out of this rather than being called back.
    pub fired_timers: Vec<String>,
    /// The clock each fired timer's callback read, assigned in fire order at the drain.
    ///
    /// `tick()` in `native-host.ts` read `Date.now()` once to decide what was due and then ran
    /// the callbacks in the order they were armed; a callback that began with its own
    /// `Date.now()` (the stall watchdog, `setNow` inside an interval) therefore saw a LATER read
    /// than the tick's. [`CoreState::timer_now`] answers that read, and the same clock when the
    /// host recorded none.
    pub timer_clocks: Vec<(String, f64)>,
    /// How many of this turn's clock reads have been taken ([`CoreState::read_clock`]).
    ///
    /// Reset to one by `ChatCore::handle`: index zero is `now_ms`, the read every rule measures
    /// against by default.
    pub clock_cursor: usize,
    /// Set for this dispatch when a family's TypeScript called `publish(controller.current())`
    /// unconditionally rather than through a state change.
    ///
    /// The core's own rule is "publish when the state changed", which is what the TypeScript's
    /// reactive path did. Its imperative path did not: `action` ended with a publish whatever
    /// happened, and several rpc continuations did the same. A family that ports one of those calls
    /// [`CoreState::request_publish`] so the revision moves on exactly the same turns.
    pub publish_requested: bool,
    /// Set with [`CoreState::request_render`]: the publish this dispatch asks for follows a
    /// `useState` setter, so the TypeScript brain re-ran its controller before publishing. A plain
    /// `publish(controller.current())` (the fleet clock, a backfill batch, an action's close)
    /// ships the LAST render's values, which is what the account panel's clock reads.
    pub render_requested: bool,
    /// This dispatch is an action that only moves the host module's own variables (the composer
    /// collapse, a measurement, an open row, a panel, the search) and publishes without calling
    /// a controller setter, so no render ran even when the document changed.
    pub quiet_action: bool,
    /// The answers an action that is still in flight publishes on.
    ///
    /// `action` in `native-host.ts` was `async`: its closing `publish(controller.current())` ran
    /// after the last `await` in the arm that handled the command, so the snapshot shipped on the
    /// record that ANSWERED the call rather than on the record that made it. The core's handlers
    /// return instead of awaiting, so the same turn is named here: the dispatcher records what the
    /// action asked for, and family a's settle publishes when that answer lands.
    ///
    /// A chain, not a single answer: an arm that awaits three things in a row (the submission's
    /// draft write, its flush and its delivery) publishes once, after the LAST of them. The family
    /// that continues the chain records the next answer with [`CoreState::publish_after`] from its
    /// own settle, and the publish is asked for at the end of the dispatch that empties the list.
    ///
    /// One chain PER ACTION, keyed by the number beside each entry: every `action` call is its own
    /// promise, so an arm that never hears back (a chrome refresh whose stashed-prompt read the
    /// host never answers) must not hold the closing publish of another arm that did. With one
    /// shared list, a picture read answered while such a refresh hung published nothing.
    pub publish_awaits: Vec<(u64, PublishAwait)>,
    /// The chain [`CoreState::publish_after`] adds to during this dispatch: the action being
    /// dispatched, or the chain whose answer just settled (its continuation is the same arm).
    pub publish_chain: Option<u64>,
    /// The last chain number handed out.
    pub publish_chain_counter: u64,
    /// The chains an answer settled during the dispatch running right now.
    ///
    /// The decision is deferred to the end of the dispatch because the family that owns the chain
    /// settles AFTER family a, which is where the answer is matched: publishing on the spot would
    /// end an arm that has just asked for its next step.
    pub settled_chains: Vec<u64>,
    /// The controller exists, which is only true once the composer boot read has answered.
    ///
    /// Every publish in `native-host.ts` was written
    /// `if (controller) publish(controller.current())` or ran inside the controller itself, and
    /// `startController` was called from the boot read's `.then(...)`. Nothing the core does before
    /// that can ship a document, which is why `start` leaves the host's first drain empty.
    pub controller_started: bool,
    /// The effects this arm raised are fire and forget: the arm did not await them.
    ///
    /// `AsyncQuestions.save` is the case it exists for: it calls `changed()` at once and then
    /// chains its write onto `this.writes` WITHOUT awaiting it, so the arm publishes on the
    /// action's own turn and the write's answer publishes again later. Set by the handler, read
    /// and cleared by `crate::dispatch::actions::dispatch`.
    pub effects_not_awaited: bool,
    /// This dispatch answered one step of an action's chain and the arm asked for another.
    ///
    /// Read and cleared by `ChatCore::republish`, which then ships nothing: the TypeScript arm is
    /// still suspended and has not reached its `publish(controller.current())`.
    pub chain_continued: bool,
    /// The refusal the composer was showing when this action started, before it was cleared.
    ///
    /// `native-host.ts` read `const clearedError = operationError !== undefined` just above the
    /// clear, and two arms published only when it was true. Recorded here so the handler that needs
    /// it does not have to be handed a second argument.
    pub cleared_error: bool,
    /// This action's arm returned before the closing `publish(controller.current())`.
    ///
    /// Three arms of the switch do: `editDraft` when neither the refusal nor the history index
    /// moved, `receiveHandoff` for a handoff already being received, and the wheel gestures, which
    /// the dispatcher knows about on its own. Set by the handler, read and cleared by
    /// `crate::dispatch::actions::dispatch`.
    pub skip_closing_publish: bool,
    /// A gxserver call an in-flight action was awaiting refused during this dispatch.
    ///
    /// `action` wrapped its whole switch in one `try`/`catch` (`native-host.ts:1630`), so a refused
    /// `await` threw out of the arm and landed on `operationError` whatever the arm was doing.
    /// The core's arms have already returned by the time the answer comes back, so the decision is
    /// made once at the end of the dispatch instead, over the same list of answers the closing
    /// publish waits on.
    pub awaited_refusal: Option<(String, Option<String>)>,
    /// Set by a family that has put this dispatch's refusal somewhere of its own.
    pub refusal_claimed: bool,
    /// The boot read was refused, and the document it leaves behind has not shipped yet.
    ///
    /// `start`'s `.catch` in `native-host.ts:659` replaced the whole snapshot with
    /// `{status: 'error', error}` and bumped the revision, WITHOUT a controller: it is the one
    /// document the core publishes that is not assembled from the families.
    pub boot_error: Option<String>,
    /// The id the next request carries, for every family.
    ///
    /// One counter for the whole core, because [`crate::Event::RpcSettled`] routes by id alone: two
    /// families drawing from their own counters would both claim request 3 and each would settle
    /// the other's answer.
    pub next_request_id: u64,
}

/// One answer an in-flight action is waiting for before the publish that ends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublishAwait {
    /// A gxserver call, by request id.
    Rpc(u64),
    /// A stored record being read or written.
    Storage(crate::event::StorageKey),
}

impl CoreState {
    /// Records that an action is waiting on these answers before it publishes.
    ///
    /// Nothing is recorded when the effects hold no answerable request: the TypeScript arm then
    /// never suspends, and its closing publish runs on the action's own turn.
    pub fn publish_after(&mut self, effects: &[crate::effect::Effect]) -> bool {
        use crate::effect::Effect;
        let chain = self.current_publish_chain();
        let mut awaited = false;
        for effect in effects {
            let await_on = match effect {
                Effect::SendRpc { request_id, .. } => PublishAwait::Rpc(*request_id),
                Effect::ReadStorage { key } | Effect::WriteStorage { key, .. } => {
                    PublishAwait::Storage(key.clone())
                }
                Effect::FlushStorage { store } => PublishAwait::Storage(crate::event::StorageKey {
                    store: store.clone(),
                    suffix: String::new(),
                }),
                // One host round trip, several records: the arm resumes when the LAST of them has
                // been folded in, which is what one await per key says.
                Effect::ReadStorageBatch { keys } => {
                    for key in keys {
                        awaited = true;
                        let await_on = (chain, PublishAwait::Storage(key.clone()));
                        if !self.publish_awaits.contains(&await_on) {
                            self.publish_awaits.push(await_on);
                        }
                    }
                    continue;
                }
                Effect::WriteStorageBatch { writes } => {
                    for write in writes {
                        awaited = true;
                        let await_on = (chain, PublishAwait::Storage(write.key.clone()));
                        if !self.publish_awaits.contains(&await_on) {
                            self.publish_awaits.push(await_on);
                        }
                    }
                    continue;
                }
                _ => continue,
            };
            awaited = true;
            let await_on = (chain, await_on);
            if !self.publish_awaits.contains(&await_on) {
                self.publish_awaits.push(await_on);
            }
        }
        awaited
    }

    /// The chain this dispatch adds to, starting one when none is open.
    fn current_publish_chain(&mut self) -> u64 {
        match self.publish_chain {
            Some(chain) => chain,
            None => self.begin_publish_chain(),
        }
    }

    /// Opens a new chain for an action about to be dispatched and makes it the current one.
    pub fn begin_publish_chain(&mut self) -> u64 {
        self.publish_chain_counter += 1;
        self.publish_chain = Some(self.publish_chain_counter);
        self.publish_chain_counter
    }

    /// Whether an in-flight action is waiting on this answer.
    pub fn awaits(&self, await_on: &PublishAwait) -> bool {
        self.publish_awaits
            .iter()
            .any(|(_, pending)| pending == await_on)
    }

    /// Takes this answer off the chain an action is waiting on.
    ///
    /// It does not publish: the family that owns the chain has not run yet this dispatch and may
    /// ask for the next step. [`CoreState::finish_publish_awaits`] makes the decision once every
    /// family has settled.
    pub fn settle_publish_await(&mut self, await_on: &PublishAwait) {
        if let Some(at) = self
            .publish_awaits
            .iter()
            .position(|(_, pending)| pending == await_on)
        {
            let (chain, _) = self.publish_awaits.remove(at);
            self.settled_chains.push(chain);
            // A continuation the owning family asks for from its settle is the same arm.
            self.publish_chain = Some(chain);
        }
    }

    /// Ends the dispatch: the arm publishes when the chain it was waiting on has run out.
    ///
    /// `action` is `async` and its closing `publish(controller.current())` runs after the LAST
    /// `await` in the arm, so a chain that continued into a new request publishes nothing yet.
    pub fn finish_publish_awaits(&mut self) {
        self.publish_chain = None;
        let settled = std::mem::take(&mut self.settled_chains);
        if !settled.is_empty() {
            let ended = settled.iter().any(|chain| {
                !self
                    .publish_awaits
                    .iter()
                    .any(|(pending, _)| pending == chain)
            });
            if ended {
                self.request_publish();
            } else {
                // The arm asked for its next step instead of ending. Its `publish` has not run, so
                // nothing it changed on the way ships yet: the state the answer moved is the
                // controller's own, not React's, and only the closing publish carries it.
                self.chain_continued = true;
            }
        }
    }

    /// Whether one of this dispatch's due timers is `key`.
    pub fn timer_fired(&self, key: &str) -> bool {
        self.fired_timers.iter().any(|fired| fired == key)
    }

    /// The clock the callback of a timer that fired this dispatch read, or the turn's clock.
    pub fn timer_now(&self, key: &str, context: &crate::state::ChatContext) -> f64 {
        self.timer_clocks
            .iter()
            .find(|(fired, _)| fired == key)
            .map(|(_, at)| *at)
            .unwrap_or(context.now_ms)
    }

    /// The next clock read of this turn, where the TypeScript called `Date.now()` a further time.
    ///
    /// Only the sites whose value is LATCHED past the turn need this (a `useState` initializer,
    /// a `setNow` inside a callback); a read that is compared and forgotten keeps `now_ms`.
    pub fn read_clock(&mut self, context: &crate::state::ChatContext) -> f64 {
        let value = context.clock_read(self.clock_cursor);
        self.clock_cursor += 1;
        value
    }

    /// Ships a snapshot this turn even when nothing the document can see changed, for the places
    /// the TypeScript published unconditionally.
    pub fn request_publish(&mut self) {
        self.publish_requested = true;
    }

    /// A publish that follows a `useState` setter: the controller renders first.
    pub fn request_render(&mut self) {
        self.publish_requested = true;
        self.render_requested = true;
    }

    /// The id for the next request any family asks for. Monotonic and never reused, so a late
    /// answer to a retired request is dropped rather than misrouted.
    pub fn allocate_request_id(&mut self) -> u64 {
        self.next_request_id += 1;
        self.next_request_id
    }

    /// Records a refusal, replacing whatever was shown before.
    ///
    /// `sendCancelled` keeps its CODE and shows no message, because the person cancelled the send
    /// themselves and there is nothing to tell them (`native-host.ts:1632`).
    pub fn fail(&mut self, message: impl Into<String>, code: Option<String>) {
        self.operation_error_code = code;
        self.operation_error = match self.operation_error_code.as_deref() {
            Some("sendCancelled") => None,
            _ => Some(message.into()),
        };
    }

    /// The refusal this dispatch carries already has an owner, so the action's own `catch` must
    /// not write it a second time.
    ///
    /// Family c's picker answers are the case: a refused `terminalChoice` belongs on the notice
    /// card and the composer's error line stays clear (`native-host.ts:1634`).
    pub fn claim_refusal(&mut self) {
        self.refusal_claimed = true;
    }

    /// Clears the refusal, which every successful action does.
    pub fn clear_error(&mut self) {
        self.operation_error = None;
        self.operation_error_code = None;
    }
}
