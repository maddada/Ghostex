//! Family e's uniform settle hook.
//!
//! Family e has no event of its own: the option store rebuilds from the catalog and the agent, the
//! accounts poll runs on the clock, the switch card advances every frame, and the model picker's
//! deadlines and the fork read all arrive on events nothing routes. That was `useMemo` and
//! `useEffect` work in `native-host.ts`, so it runs once per event here.
//!
//! e1's `observe` and e2's own settle are called from one place, in that order, so the model menu
//! and the context meter (e2) are built from the option catalog e1 has already rebuilt this turn.

use crate::effect::Effect;
use crate::event::Event;
use crate::state::{ChatContext, ChatState};
use crate::wire::RpcOutcome;

/// Settles family e's carried state for this event.
pub fn settle(state: &mut ChatState, event: &Event, context: &ChatContext) -> Vec<Effect> {
    let mut effects = Vec::new();
    match event {
        Event::ComposerBootRead(read) => {
            effects.extend(crate::menus::lifecycle::boot_read(state, read));
        }
        // `listeners`: `adoptNativeChatSettings` and `adoptNativeContextPreferences` call
        // `setNow(Date.now())` (`native-context.ts:53`), which moves the meter's clock and
        // publishes; the boot read adopts both before the controller exists, so it latches nothing.
        Event::SettingsChanged(_) | Event::ContextPreferencesChanged { .. } => {
            if state.menus.meter_now_ms.is_some() {
                state.menus.meter_now_ms = Some(context.now_ms);
                state.core.request_render();
            }
        }
        Event::StorageWritten { key, error } => {
            effects.extend(crate::menus::actions::switch_after_flush(
                state,
                key,
                error.as_deref(),
            ));
        }
        Event::RpcSettled {
            request_id,
            outcome,
        } => {
            settle_accounts(state, *request_id, outcome.as_ref());
            effects.extend(crate::menus::actions::switch_settled(
                state,
                context,
                *request_id,
            ));
            // The option dispatch walks its steps on the same answers; it claims only the id it
            // issued, so family d's keystroke marker still sees the very same call.
            effects.extend(
                crate::menus::dispatch_run::settle(state, context, *request_id, outcome)
                    .unwrap_or_default(),
            );
        }
        _ => {}
    }
    effects.extend(crate::menus::lifecycle::observe(state, context));
    let mut allocated = state.core.next_request_id;
    let picker = crate::menus::picker::settle(state, event, context, || {
        allocated += 1;
        allocated
    });
    state.core.next_request_id = allocated;
    effects.extend(picker);
    effects
}

/// `requestAccounts`'s `try`, `catch` and `finally`, from `native-controls.ts`.
///
/// A newer request wins: the generation is bumped when the read goes out, and an answer whose
/// request is no longer the one in flight is dropped rather than overwriting a fresher list.
fn settle_accounts(state: &mut ChatState, request_id: u64, outcome: &RpcOutcome) {
    if state.menus.accounts_request != Some(request_id) {
        return;
    }
    state.menus.accounts_request = None;
    state.menus.accounts_busy = false;
    match outcome {
        RpcOutcome::Ok { result } => {
            state.menus.accounts = Some(result.clone());
            state.menus.account_error = None;
        }
        RpcOutcome::Err { message, .. } => {
            state.menus.account_error = Some(message.clone());
        }
    }
}
