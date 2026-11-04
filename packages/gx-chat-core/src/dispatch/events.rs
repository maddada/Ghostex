//! Which family handles which event.
//!
//! Almost everything that arrives from outside is family a's: the frames, the connection, the
//! seed and resync reads, the clock. The exceptions are the answers to requests another family
//! asked for, which come back with the request id that family allocated, and the measurements the
//! renderer reports.

use crate::dispatch::actions;
use crate::effect::Effect;
use crate::event::Event;
use crate::state::{ChatContext, ChatState};

/// Routes one event to its owner and returns what the host must do.
///
/// `RpcSettled` and `StorageLoaded`/`StorageWritten` are deliberately not routed by kind: the
/// family that asked owns the answer, and the request id is what says which family that is. Family
/// a keeps that table in [`crate::ChatCore`], so those arms land there rather than here.
pub fn dispatch(state: &mut ChatState, event: &Event, context: &ChatContext) -> Vec<Effect> {
    // The core owns no threads, so a due timer is a key on the state rather than a callback. The
    // table is drained once here, before any family runs, and the keys stay readable for the whole
    // dispatch: a family answers its own timer inside its ordinary handler
    // (`state.core.timer_fired(KEY)`), which is why the drain cannot live in one family's arm.
    state.core.fired_timers = if matches!(event, Event::Tick) {
        state.core.take_due_timers(context.now_ms)
    } else {
        Vec::new()
    };
    // Each due callback that reads the clock itself gets the next recorded read, in the order the
    // host would have run them, before any family handles its own key.
    state.core.timer_clocks = state
        .core
        .fired_timers
        .clone()
        .into_iter()
        .filter(|key| crate::session::timers::callback_reads_clock(key))
        .map(|key| {
            let at = state.core.read_clock(context);
            (key, at)
        })
        .collect();
    state.core.publish_chain = None;
    state.core.quiet_action = false;
    let mut effects = route(state, event, context);
    // Every family carries state a pure `document` cannot derive, and every family has `try`,
    // `catch` and `finally` bodies that run when a call it started answers. That is the mutating
    // half of the TypeScript's `publish`, so each family gets ONE hook with the same signature,
    // run here in the assembly order (a, b, c, d, e, f) before `crate::document::assemble`. The
    // order is fixed only so two runs produce the same bytes: a settle reads `ChatState`, never a
    // half-built document, exactly like `document`.
    for settle in SETTLE {
        effects.extend(settle(state, event, context));
    }
    // `action`'s one `catch` (native-host.ts:1630): a call the arm awaited that refused throws
    // out of the arm, whatever the arm was doing, and the message lands on `operationError`. Run
    // after every family so a family that wants the refusal on a surface of its own can claim it.
    if let Some((message, code)) = state.core.awaited_refusal.take() {
        if !std::mem::take(&mut state.core.refusal_claimed) {
            state.core.fail(message, code);
        }
    }
    state.core.refusal_claimed = false;
    // Every family has had its chance to continue an action's chain, so the closing publish of an
    // arm whose last `await` just answered is asked for here rather than in family a's settle.
    state.core.finish_publish_awaits();
    effects
}

/// One settle per family, in assembly order.
type Settle = fn(&mut ChatState, &Event, &ChatContext) -> Vec<Effect>;
const SETTLE: [Settle; 8] = [
    crate::session::settle,
    crate::transcript::settle,
    crate::questions::settle,
    crate::composer::settle,
    crate::menus::settle,
    // A send parked behind the gate resumes once the option dispatch (family e) has let go of it.
    crate::composer::send::release_held,
    // The controller's own `restoreReturned` effect is declared right after
    // `computeNativeChatOptions` in `startController`, so it runs after the option store's write.
    crate::composer::settle::restore_returned_effect,
    crate::extras::settle,
];

/// The event's own owner, before any family settles.
fn route(state: &mut ChatState, event: &Event, context: &ChatContext) -> Vec<Effect> {
    match event {
        Event::Action(action) => actions::dispatch(state, action, context),
        Event::Start(_)
        | Event::Frame(_)
        | Event::Connection(_)
        | Event::RpcSettled { .. }
        // The clock is family a's: it owns the timer table's own keys (the seed and resync
        // backoffs, the stall watchdog, the read deadline, the tool-row hold). Every other family
        // reads `state.core.fired_timers` from its own settle hook.
        | Event::Tick
        | Event::ComposerBootRead(_)
        | Event::ComposerBootFailed { .. }
        | Event::RetainedSnapshotLoaded { .. }
        | Event::SettingsChanged(_) => crate::session::handle_event(state, event, context),
        // The two batch answers never reach here: `ChatCore::handle` expands one into the per-key
        // answers it stands for, so every family's existing arm serves it unchanged.
        Event::StorageBatchLoaded { .. }
        | Event::StorageBatchWritten { .. }
        | Event::StorageLoaded { .. }
        | Event::StorageWritten { .. }
        | Event::ContextPreferencesChanged { .. }
        | Event::ModelCatalogChanged { .. }
        | Event::Measured(_)
        | Event::DraftChanged { .. } => Vec::new(),
        Event::ReadAloud(input) => crate::read_aloud::handle_event(state, input, context),
        // `ChatCore::handle` follows a show with a tick, which runs what the held timers owe.
        Event::Visibility { shown } => {
            state.core.hidden = !shown;
            Vec::new()
        }
        Event::SessionDirectoryChanged { sessions } => {
            let view = &mut state.transcript_view;
            // The `+` picker's live fields (activity, recency) change all the time; only a change
            // to what the transcript names sessions by rebuilds it.
            if !crate::event::KnownSession::same_names(&view.session_directory, sessions) {
                view.session_directory_revision += 1;
            }
            if view.session_directory != *sessions {
                view.session_directory = sessions.clone();
            }
            Vec::new()
        }
    }
}
