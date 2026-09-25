//! The React effects of the option and account surfaces, collapsed into one pass.
//!
//! `native-options.ts`, `session-options.ts` and `native-controls.ts` did their work in
//! `useMemo` and `useEffect`: rebuild the option store when the catalog or the storage key
//! changed, fold a new detection in, read the accounts on mount and every 30 seconds, and advance
//! the switch card. None of that survives as a hook here, so it runs once per event, before the
//! document is assembled.
//!
//! [`observe`] is family e1's only entry point besides `document` and `handle`. It must be called
//! on every event the core handles, which is what `crate::dispatch::events::dispatch` does.

use crate::effect::Effect;
use crate::menus::controls::{
    accounts_poll_key, session_accounts_request, switch_progress, switch_ready, ACCOUNTS_POLL_MS,
};
use crate::menus::option_catalog::session_option_catalog;
use crate::menus::option_storage::{
    option_state_to_value, remember_option_state, session_options_key, stored_option_state,
};
use crate::menus::option_store::OptionStore;
use crate::menus::option_values::DetectedOptions;
use crate::menus::options::option_storage_key;
use crate::state::{ChatContext, ChatState};
use crate::wire::ChatRpcMethod;

/// Advances family e1's own clocks and reads for this event.
///
/// Returns the effects the host must perform: the periodic accounts read, and the timer the grace
/// window and the switch card need.
pub fn observe(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let now_ms = context.now_millis();
    let mut effects = Vec::new();
    rebuild_option_store(state, now_ms);
    apply_detection(state);
    state.menus.options.expire(now_ms);
    // `persistence.write` runs inside the store's own `publish`, which the layout effect above
    // reaches before any passive effect, so the write goes out ahead of the accounts poll.
    effects.extend(persist_options(state));
    // The render's own clock reads, measured before any effect below moves the state they read.
    let render_reads = effect_clock_index(state);
    let accounts_key = state.menus.accounts_key.clone();
    effects.extend(poll_accounts(state, context, now_ms, render_reads));
    // The accounts effect (declared before `computeAccountSwitchStatus`) re-ran and armed its
    // interval, which is one more read ahead of the card's effects.
    let accounts_armed =
        state.menus.accounts_key.is_some() && state.menus.accounts_key != accounts_key;
    let progress = switch_progress(state);
    let ready = switch_ready(state);
    let progress_reads = progress_effect_reads(state, progress.as_ref(), ready, now_ms);
    // The switch card's own clock starts when the controller first renders, which is the frame
    // after the composer boot read lands; before that there is nothing to draw a card from.
    let switch_wake = if state.menus.options_seeded {
        // `useState(Date.now)` in `computeAccountSwitchStatus`: the first render's second read,
        // after the controller's own `useRef(Date.now())`, and the number `accountStatus.now`
        // then carries until a card is up.
        if state.menus.account_switch.now_ms.is_none() {
            state.menus.account_switch.now_ms = Some(state.core.read_clock(context).round() as i64);
        }
        let wake = state
            .menus
            .account_switch
            .observe(progress.as_ref(), ready, now_ms);
        switch_clock(
            state,
            context,
            progress.as_ref(),
            now_ms,
            render_reads + usize::from(accounts_armed) + progress_reads,
        );
        wake
    } else {
        None
    };
    arm(state, "menus.switchCard", switch_wake, context);
    let clock_due = state.menus.account_switch.clock_due_ms;
    arm(state, SWITCH_CLOCK_TIMER, clock_due, context);
    let option_wake = state.menus.options.next_wake_ms();
    arm(state, "menus.optionGrace", option_wake, context);
    let accounts_wake = state.menus.accounts_poll_due_ms;
    arm(state, ACCOUNTS_POLL_TIMER, accounts_wake, context);
    // CDXC:SessionChat 2026-09-22 WHY:
    // `setInterval(() => setNow(Date.now()), 30_000)` on a `useEffect` with no dependencies
    // (`native-context.ts:63`): the meter's countdown labels (five-hour reset, seven-day reset,
    // cache time left) have no other reason to redraw. `CONTEXT_METER_REFRESH_MS` was written and
    // never read, so they froze at whatever clock the last unrelated event carried. `arm_once`,
    // because re-arming on every event would push the deadline forward for ever.
    // `observe` runs on every event, and the drain removes a one-shot before any family sees it,
    // so re-arming here is the interval's next period.
    if state.core.controller_started {
        // `useState(Date.now)` on the first render; `setNow(Date.now())` when the interval fires.
        // The setter is a state change, so the TypeScript brain published on it even when no label
        // moved, which is what `request_publish` reproduces. Both are reads of their own, past
        // the turn's first: the initializer runs after the switch card's, the callback's is the
        // one the drain assigned it.
        if state.menus.meter_now_ms.is_none() {
            state.menus.meter_now_ms = Some(state.core.read_clock(context));
        }
        if state.core.timer_fired("menus.contextMeter") {
            state.menus.meter_now_ms = Some(state.core.timer_now("menus.contextMeter", context));
            state.core.request_render();
        }
        // One `setInterval` from a `useEffect` with no dependencies: armed once, and it keeps its
        // place in the table across fires, which is the order the host runs coincident callbacks
        // in. A one-shot re-armed after every fire moved to the end of the table instead.
        if !state.core.timers.is_armed("menus.contextMeter") {
            state.core.timers.arm_interval(
                "menus.contextMeter",
                context.now_ms,
                crate::menus::context::meter::CONTEXT_METER_REFRESH_MS,
            );
        }
        if state.core.timer_fired(REFRESH_AFTER_SWITCH_SOON)
            || state.core.timer_fired(REFRESH_AFTER_SWITCH_LATER)
        {
            effects.extend(crate::session::reads::request_resync(state, context));
        }
    }
    effects
}

/// The two `schedule(() => chat.refresh(), delay)` after a draft-agent switch
/// (`native-host.ts`, the `switchDraftAgent` arm's `finally`): the daemon settles the new agent
/// a moment after the call answers, so the session is re-read at once, then again 2 and 6
/// seconds later.
pub const REFRESH_AFTER_SWITCH_SOON: &str = "menus.refreshAfterSwitch.2000";
pub const REFRESH_AFTER_SWITCH_LATER: &str = "menus.refreshAfterSwitch.6000";
pub const REFRESH_AFTER_SWITCH_DELAYS_MS: [f64; 2] = [2_000.0, 6_000.0];

/// Moves one of family e1's rows in the core's timer table, or drops it.
fn arm(state: &mut ChatState, key: &str, due_at_ms: Option<i64>, context: &ChatContext) {
    match due_at_ms {
        Some(due_at_ms) => {
            let delay = (due_at_ms as f64 - context.now_ms).max(0.0);
            state.core.timers.arm(key, context.now_ms, delay);
        }
        None => {
            state.core.timers.cancel(key);
        }
    }
}

/// The `useMemo` on `[catalog, storageKey]`: a different agent, catalog document or storage key
/// builds a new store, seeded from what was persisted for that key.
fn rebuild_option_store(state: &mut ChatState, now_ms: i64) {
    let agent = state.session.agent.clone();
    let draft_agent_id = state
        .session
        .available_agents
        .as_ref()
        .and(state.session.session_agent_id.clone());
    let session_key = state.menus.session_key.clone();
    let mut latched = state.menus.latched_draft_agent.clone();
    let storage_key = option_storage_key(
        &mut latched,
        session_key.as_deref(),
        draft_agent_id.as_deref(),
    );
    state.menus.latched_draft_agent = latched;
    let catalog_generation = state.menus.model_catalog_generation;
    let unchanged = state.menus.options_agent == agent
        && state.menus.options_catalog_generation == catalog_generation
        && state.menus.options.storage_key == storage_key;
    if unchanged {
        return;
    }
    let catalog = session_option_catalog(&state.menus.model_catalog, agent.as_deref());
    let stored = stored_option_state(&state.menus.stored_options, storage_key.as_deref());
    state.menus.options = OptionStore::seeded(catalog.as_ref(), storage_key, &stored, now_ms);
    state.menus.options_agent = agent;
    state.menus.options_catalog_generation = catalog_generation;
    state.menus.options_store_generation = state.menus.options_store_generation.wrapping_add(1);
}

/// The `useLayoutEffect` on `[sessionOptions.applyDetected, chat.selectedOptions]`.
///
/// Both deps are IDENTITIES in the TypeScript: `applyDetected` is new whenever the store was
/// rebuilt, and `chat.selectedOptions` is new whenever `applySelectedOptions` built a fresh
/// object. Running the fold on every event instead would be harmless for the pill values, which
/// settle, but not for the `optionWrite` it makes: the store's `publish` compares object identity
/// rather than contents, so every run persists.
fn apply_detection(state: &mut ChatState) {
    let deps = (
        state.session.selected_options_generation,
        state.menus.options_store_generation,
    );
    if state.menus.applied_detection == Some(deps) {
        return;
    }
    state.menus.applied_detection = Some(deps);
    let Some(selected) = state.session.selected_options.clone() else {
        return;
    };
    let Some(detected) = DetectedOptions::from_value(&selected) else {
        return;
    };
    let catalog =
        session_option_catalog(&state.menus.model_catalog, state.session.agent.as_deref());
    state
        .menus
        .options
        .apply_detected(catalog.as_ref(), Some(&detected));
}

/// `persistence.write(key, state)`: one `composer('optionWrite', …)` per publish of the store.
fn persist_options(state: &mut ChatState) -> Vec<Effect> {
    let published = state.menus.options.take_published();
    // `write: (key, state) => { if (!key) return; … }`.
    let Some(option_key) = state.menus.options.storage_key.clone() else {
        return Vec::new();
    };
    let mut effects = Vec::with_capacity(published.len());
    for next in published {
        remember_option_state(&mut state.menus.stored_options, &option_key, &next);
        effects.push(Effect::WriteStorage {
            key: session_options_key(&option_key),
            value: Some(option_state_to_value(&next).to_string()),
            durable: true,
        });
    }
    effects
}

/// The key of the switch card's 30 second `setInterval`.
const SWITCH_CLOCK_TIMER: &str = "menus.switchClock";

/// `useEffect(() => { if (!visible) return; setNow(Date.now()); const timer = setInterval(() =>
/// setNow(Date.now()), 30000); return () => clearInterval(timer); }, [visible?.id])` in
/// `computeAccountSwitchStatus`.
///
/// `setNow` takes the effect's own read, `effect_read` (the render's reads and every effect ahead
/// of it come first), and the interval's first deadline the read after it; each fire re-anchors at
/// the tick and `setNow`s the callback's own read. The core used to latch the event's first read
/// and re-latch on the first event 30 seconds on, which put `accountStatus.now` a few
/// milliseconds early for as long as a card stood.
fn switch_clock(
    state: &mut ChatState,
    context: &ChatContext,
    progress: Option<&crate::menus::accounts_presentation::SwitchProgress>,
    now_ms: i64,
    effect_read: usize,
) {
    let switch = &mut state.menus.account_switch;
    let visible = progress
        .and_then(|progress| switch.visible_id(progress, now_ms))
        .map(|progress| progress.id.clone());
    if visible != switch.clock_for {
        switch.clock_for = visible.clone();
        switch.clock_due_ms = visible.map(|_| {
            switch.now_ms = Some(context.clock_read(effect_read).round() as i64);
            context.clock_read(effect_read + 1).round() as i64
                + crate::menus::account_switch::SWITCH_CLOCK_INTERVAL_MS
        });
        return;
    }
    if switch.clock_due_ms.is_some() && state.core.timer_fired(SWITCH_CLOCK_TIMER) {
        let at = state.core.timer_now(SWITCH_CLOCK_TIMER, context).round() as i64;
        let switch = &mut state.menus.account_switch;
        switch.now_ms = Some(at);
        switch.clock_due_ms = Some(now_ms + crate::menus::account_switch::SWITCH_CLOCK_INTERVAL_MS);
    }
}

/// The clock reads the switch card's progress effect makes when it runs this render.
///
/// It runs when `[progress?.id, progress?.phase, progress?.updatedAt, ready]` moved, and only a
/// `success` reads: `Date.now() - Date.parse(updatedAt)` for its age, then the 1.8 s dismissal's
/// `setTimeout` once the account is ready.
fn progress_effect_reads(
    state: &mut ChatState,
    progress: Option<&crate::menus::accounts_presentation::SwitchProgress>,
    ready: bool,
    now_ms: i64,
) -> usize {
    let deps = progress.map(|progress| {
        (
            progress.id.clone(),
            progress.phase.clone(),
            progress.updated_at.clone(),
            ready,
        )
    });
    let switch = &mut state.menus.account_switch;
    if deps == switch.progress_deps {
        return 0;
    }
    switch.progress_deps = deps;
    let Some(progress) = progress.filter(|progress| progress.phase == "success") else {
        return 0;
    };
    let recent = crate::menus::time::parse_iso_millis(&progress.updated_at)
        .is_some_and(|at| now_ms - at < crate::menus::account_switch::SWITCH_RECENT_MS);
    let watched = switch.observed.as_deref() == Some(progress.id.as_str());
    1 + usize::from((watched || recent) && ready)
}

/// The key of the poll's `setInterval` in the timer table.
const ACCOUNTS_POLL_TIMER: &str = "menus.accountsPoll";

/// The `useEffect` that reads the session's accounts on mount and every 30 seconds.
///
/// One request path for the periodic read and the panel's own requests: a newer request wins, and
/// polls skip while one is pending so an older read cannot land after a switch or policy change.
///
/// The interval is the host's own timer: it fires on a `tick`, re-anchors at that tick's clock
/// (`timer.at = now + timer.interval`) whether or not it read, and its first deadline is the clock
/// `setInterval` itself read, which is past the render's reads rather than the call's first.
/// Measured from the call's first read, the first poll after every restart went out one host tick
/// early on a large frame, and `accountPanel.busy` moved a document ahead of the TypeScript brain.
fn poll_accounts(
    state: &mut ChatState,
    context: &ChatContext,
    now_ms: i64,
    render_reads: usize,
) -> Vec<Effect> {
    let key = accounts_poll_key(state);
    let Some(key) = key else {
        // `if (!provider && !panelProvider) { setAccounts(undefined); return; }`
        state.menus.accounts = None;
        state.menus.accounts_key = None;
        state.menus.accounts_poll_due_ms = None;
        return Vec::new();
    };
    if state.menus.accounts_key.as_deref() != Some(key.as_str()) {
        // The cleanup bumps the generation, so an answer to the previous key is dropped, and
        // clears `pending`; the effect then reads at once and arms the interval.
        state.menus.accounts_generation += 1;
        state.menus.accounts_busy = false;
        state.menus.accounts_key = Some(key);
        let armed_at = context.clock_read(render_reads).round() as i64;
        state.menus.accounts_poll_due_ms = Some(armed_at + ACCOUNTS_POLL_MS);
        return request_session_accounts(state);
    }
    if !state.core.timer_fired(ACCOUNTS_POLL_TIMER) {
        return Vec::new();
    }
    state.menus.accounts_poll_due_ms = Some(now_ms + ACCOUNTS_POLL_MS);
    // `if (!pending.current) void requestAccounts({ operation: 'session' })`
    if state.menus.accounts_busy {
        return Vec::new();
    }
    request_session_accounts(state)
}

/// `requestAccounts({ operation: 'session' })`.
fn request_session_accounts(state: &mut ChatState) -> Vec<Effect> {
    state.menus.accounts_generation += 1;
    state.menus.accounts_busy = true;
    state.menus.account_error = None;
    let request_id = state.core.allocate_request_id();
    state.menus.accounts_request = Some(request_id);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AgentAccounts,
        params: Box::new(session_accounts_request()),
    }]
}

/// Which of this call's clock reads a passive effect of the controller's first render takes.
///
/// The recordings show the order: the call's own read (a frame's or a read's liveness stamp,
/// the tick's own), then the render's (`useRef(Date.now())` at the top of `computeSessionChat` on
/// every render; `workingStartedAtRef.current ??= Date.now()` when work starts; the switch
/// card's `Date.now() - Date.parse(updatedAt)` for a success it did not watch; the account
/// panel's own `Date.now()`), then the effects in declaration order. Timers other effects arm
/// earlier in the same render are not counted, so this can land one read early; within one call
/// the reads are almost always the same millisecond, and the ones that are not are the large
/// first frames this is for.
fn effect_clock_index(state: &ChatState) -> usize {
    let mut index = 2;
    if crate::session::working::working_signal(state)
        && state.session.working_started_at_ms.is_none()
    {
        index += 1;
    }
    if let Some(progress) = switch_progress(state) {
        let switch = &state.menus.account_switch;
        if progress.phase == "success"
            && switch.dismissed.as_deref() != Some(progress.id.as_str())
            && switch.observed.as_deref() != Some(progress.id.as_str())
        {
            index += 1;
        }
    }
    if crate::menus::controls::account_providers(state)
        .panel
        .is_some()
    {
        index += 1;
    }
    index
}

/// The composer boot read, as `start`'s `.then(...)` in `native-host.ts` adopted it.
///
/// `adoptAgentModelCatalog(result.modelCatalog)`, then `nativeOptionPersistence(result, …)`, which
/// is what seeds the option store from the records already on disk for this session key. Family e2
/// takes the context preferences and the model outboxes out of the same object.
pub fn boot_read(state: &mut ChatState, read: &crate::event::ComposerBootRead) -> Vec<Effect> {
    // `adoptAgentModelCatalog` replaces the lineup outright, whatever its `updatedAt`.
    if let Some(parsed) = crate::menus::catalog::parse_agent_model_catalog(&read.model_catalog) {
        state.menus.model_catalog = parsed;
        state.menus.model_catalog_generation = state.menus.model_catalog_generation.wrapping_add(1);
    }
    state.menus.session_key = Some(read.session_key.clone());
    state.menus.stored_options = read.option_states.clone();
    state.menus.options_seeded = true;
    crate::menus::picker::settle::adopt_boot_read(state, read);
    Vec::new()
}
