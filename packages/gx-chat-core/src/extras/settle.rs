//! What family f has to do before a document can be assembled.
//!
//! [`crate::extras::document`] is pure, the way `crate::document::assemble` needs it to be, but
//! several of family f's values are carried rather than derived: the stint word only changes when
//! a stint begins, the loading stage only advances on a timer, the task fold resets when the plan
//! changes, the search cursor re-anchors against the rows it can still see, and the terminal tail
//! sheet retires with the refusal that opened it. All of that is the mutating half of the
//! TypeScript's `publish`, so it runs here, once per event, before the document is built.

use serde_json::Value;

use crate::effect::Effect;
use crate::event::Event;
use crate::extras::panels::FLEET_CLOCK_TICK_MS;
use crate::extras::{
    panels, save_markdown, search, subagent, subagent_rows, terminal_tail, working_strip,
};
use crate::session::timers::TimerTable;
use crate::state::{
    ChatContext, ChatState, LOADING_STAGE_BLANK, LOADING_STAGE_INDICATOR, LOADING_STAGE_RETRY,
};
use crate::wire::RpcOutcome;

/// Settles family f's carried state for this event and returns whatever it has to ask the host
/// for.
///
/// One of the six uniform per-family hooks `crate::dispatch::events::dispatch` runs in a fixed
/// order. Ids come off the core's one allocator, taken out for the call and written back, so the
/// same inputs always produce the same ids and a late answer to a retired read cannot reach
/// another family.
pub fn settle(state: &mut ChatState, event: &Event, context: &ChatContext) -> Vec<Effect> {
    let mut allocated = state.core.next_request_id;
    let effects = settle_with_ids(state, event, context, || {
        allocated += 1;
        allocated
    });
    state.core.next_request_id = allocated;
    effects
}

fn settle_with_ids(
    state: &mut ChatState,
    event: &Event,
    context: &ChatContext,
    mut next_request_id: impl FnMut() -> u64,
) -> Vec<Effect> {
    let mut effects = Vec::new();
    match event {
        Event::RpcSettled {
            request_id,
            outcome,
        } => {
            let answer = match outcome.as_ref() {
                RpcOutcome::Ok { result } => Ok(result),
                RpcOutcome::Err { message, .. } => Err(message.clone()),
            };
            if let Some(more) = subagent::settle_rpc(
                &mut state.extras.subagent,
                *request_id,
                answer.clone(),
                context,
                next_request_id(),
            ) {
                effects.extend(more);
            } else if let Some(more) = save_markdown::settle_rpc(
                &mut state.extras.save_markdown,
                *request_id,
                answer.clone(),
                next_request_id(),
            ) {
                effects.extend(more);
            } else if let Some(more) = settle_terminal_tail_rpc(state, *request_id, answer) {
                effects.extend(more);
            }
        }
        Event::Tick => {
            advance_loading_stage(state, context);
            effects.extend(subagent::settle_tick(
                &mut state.extras.subagent,
                context,
                next_request_id(),
            ));
            // The viewer's own projector has its own `onBackfill`, which re-projects and publishes
            // without a state change of its own.
            if state.core.timer_fired(SUBAGENT_BACKFILL) {
                subagent_rows::advance(state, context);
                state.core.request_publish();
            }
            // `setInterval(() => this.republish(), FLEET_CLOCK_TICK_MS)` in `native-panels.ts`:
            // an unconditional publish every second while a row's clock moves, whose projection
            // reads the clock afresh. The document is compared at the last publish's clock, so
            // without the request a moving elapsed label never shipped on its own tick.
            if state.core.timer_fired(FLEET_CLOCK) {
                state.core.request_publish();
            }
            // `setInterval(() => setNow(Date.now()), ACTIVITY_CLOCK_TICK_MS)`: a new clock, so a
            // state change the lifecycle publishes on.
            if state.core.timer_fired(ACTIVITY_CLOCK) {
                state.extras.activity_now_ms = Some(state.core.timer_now(ACTIVITY_CLOCK, context));
                state.core.request_render();
            }
        }
        _ => {}
    }
    // `project()` runs at the two moments the page, the agent path or the working flag can move
    // (a restart, a finished read), both of which have already happened by the time this line
    // runs.
    subagent_rows::refresh(state, context);
    track_transcript_loading(state, context);
    let working = working(state);
    let tasks = state.session.agent_tasks.clone();
    // Serialising every row is only worth it while the search field is open.
    let items = if state.extras.search.open {
        transcript_items(state, context)
    } else {
        Vec::new()
    };
    working_strip::settle_working_word(&mut state.extras.working_word, working, context);
    panels::settle_task_signature(&mut state.extras.panels, tasks.as_ref());
    if state.core.operation_error_code.as_deref() != Some("composerNotReady") {
        terminal_tail::retire(&mut state.extras.terminal_tail);
    }
    search::settle(&mut state.extras.search, &items);
    settle_activity_clock(state, context);
    arm_timers(state, context);
    effects
}

/// `computeSessionChatActivity`'s clock: the initializer on the first render, then
/// `useEffect(() => { if (!hasClock) return; setNow(Date.now()); setInterval(...) },
/// [activity?.detectedAt, hasClock])`. A new sample restarts the interval from its own moment,
/// which is what put the strip's ticks on the same records as the TypeScript brain's.
fn settle_activity_clock(state: &mut ChatState, context: &ChatContext) {
    if !state.core.controller_started {
        return;
    }
    let activity = state.session.terminal_activity.as_ref();
    let has_clock = activity.is_some_and(|activity| activity.get("elapsedSeconds").is_some());
    let detected_at = activity
        .and_then(|activity| activity.get("detectedAt"))
        .and_then(Value::as_str)
        .map(str::to_string);
    if state.extras.activity_now_ms.is_none() {
        state.extras.activity_now_ms = Some(context.now_ms);
    }
    let deps = (detected_at, has_clock);
    if state.extras.activity_clock_deps.as_ref() == Some(&deps) {
        return;
    }
    state.extras.activity_clock_deps = Some(deps);
    state.core.timers.cancel(ACTIVITY_CLOCK);
    if has_clock {
        if state.extras.activity_now_ms != Some(context.now_ms) {
            state.extras.activity_now_ms = Some(context.now_ms);
            state.core.request_publish();
        }
        state.core.timers.arm_interval(
            ACTIVITY_CLOCK,
            context.now_ms,
            crate::extras::activity::ACTIVITY_CLOCK_TICK_MS as f64,
        );
    }
}

/// The one live-work flag the strip keys off, which family a folds.
fn working(state: &ChatState) -> bool {
    state.session.server_working || state.session.external_working
}

/// `trackTranscriptLoading`: the stage restarts whenever a read starts, and clears when it ends.
fn track_transcript_loading(state: &mut ChatState, context: &ChatContext) {
    // `trackTranscriptLoading(state.view.kind === 'loading')` is a line of `publish`, and `publish`
    // only runs once the composer boot read has answered. Starting the run at `Event::Start` armed
    // the two stage one-shots before the TypeScript had any timer at all, so the host's very first
    // drain asked for a wake that should have been `null`.
    if !state.core.controller_started {
        return;
    }
    // `transcriptLoading = state.view.kind === 'loading' && !showNewSessionWelcome`: a draft
    // session greets instead of loading, so its stages never start.
    let loading = state.session.server_status.as_str() == "loading"
        && state.session.available_agents.is_none();
    if loading == state.extras.loading_started_at_ms.is_some() {
        return;
    }
    state.extras.loading_stage = LOADING_STAGE_BLANK.to_string();
    state.extras.loading_started_at_ms = loading.then_some(context.now_ms);
}

/// The two stage timers, which the host drives with a tick the way `tick()` drains the queue.
fn advance_loading_stage(state: &mut ChatState, context: &ChatContext) {
    let Some(started) = state.extras.loading_started_at_ms else {
        return;
    };
    let elapsed = context.now_ms - started;
    if elapsed >= crate::extras::welcome::LOADING_RETRY_DELAY_MS as f64 {
        state.extras.loading_stage = LOADING_STAGE_RETRY.to_string();
    } else if elapsed >= crate::extras::welcome::LOADING_INDICATOR_DELAY_MS as f64 {
        state.extras.loading_stage = LOADING_STAGE_INDICATOR.to_string();
    }
}

/// The hover read and the expanded sheet's read, both of which land on the same method.
fn settle_terminal_tail_rpc(
    state: &mut ChatState,
    request_id: u64,
    outcome: Result<&Value, String>,
) -> Option<Vec<Effect>> {
    let tail = &mut state.extras.terminal_tail;
    if tail.hover_request == Some(request_id) {
        tail.hover_request = None;
        // Keep the last verdict; a failed read is "unknown", never "not ready".
        if let Ok(result) = outcome {
            tail.tail = Some(result.clone());
        }
        return Some(Vec::new());
    }
    if tail.notice_request == Some(request_id) {
        tail.notice_request = None;
        match outcome {
            Ok(result) => tail.notice_tail = Some(result.clone()),
            Err(message) => {
                tail.notice_tail = None;
                tail.notice_error = Some(if message.is_empty() {
                    "The terminal screen could not be read.".to_string()
                } else {
                    message
                });
            }
        }
        tail.notice_loading = false;
        return Some(Vec::new());
    }
    None
}

/// The rows transcript search runs over, as the values its matcher reads.
fn transcript_items(state: &ChatState, context: &ChatContext) -> Vec<Value> {
    crate::transcript::rows(state, context)
        .into_iter()
        .map(|item| serde_json::to_value(item).unwrap_or(Value::Null))
        .collect()
}

/// Keeps family f's rows in the core's timer table, which is what the frame's `nextWakeMs` reads.
///
/// The fleet's once a second while any row's clock is moving, the two loading stages, the
/// subagent viewer's poll. The terminal activity's clock restarts with every
/// sample, so it is armed by `settle_activity_clock` instead. Each is a key, so re-arming is
/// idempotent and a surface that goes away cancels its own row.
fn arm_timers(state: &mut ChatState, context: &ChatContext) {
    let (_, _, fleet_ticking) = panels::project(state, context);
    let loading = state.extras.loading_started_at_ms;
    let poll = state.extras.subagent.poll_at_ms;
    let now = context.now_ms;
    let timers = &mut state.core.timers;

    interval(timers, FLEET_CLOCK, fleet_ticking, now, FLEET_CLOCK_TICK_MS);
    let armed_at = state.extras.loading_timers_armed_at_ms;
    let timers = &mut state.core.timers;
    match loading {
        // Armed once per run, not once per settle: `trackTranscriptLoading` calls `setTimeout`
        // once, and the indicator's own delay is 0 ms, so re-arming republished a wake the tick
        // that ran it had already deleted.
        Some(started) if armed_at != Some(started) => {
            deadline(
                timers,
                LOADING_INDICATOR,
                Some(started + crate::extras::welcome::LOADING_INDICATOR_DELAY_MS as f64),
                now,
            );
            deadline(
                timers,
                LOADING_RETRY,
                Some(started + crate::extras::welcome::LOADING_RETRY_DELAY_MS as f64),
                now,
            );
            state.extras.loading_timers_armed_at_ms = Some(started);
        }
        Some(_) => {}
        None => {
            timers.cancel(LOADING_INDICATOR);
            timers.cancel(LOADING_RETRY);
            state.extras.loading_timers_armed_at_ms = None;
        }
    }
    let backfill = state.extras.subagent.view.has_pending_backfill();
    let timers = &mut state.core.timers;
    deadline(timers, SUBAGENT_POLL, poll, now);
    if backfill {
        timers.arm_once(SUBAGENT_BACKFILL, now, 0.0);
    } else {
        timers.cancel(SUBAGENT_BACKFILL);
    }
}

/// Family f's timer keys, namespaced so no other family can collide with them.
const FLEET_CLOCK: &str = "extras.fleetClock";
const ACTIVITY_CLOCK: &str = "extras.activityClock";
const LOADING_INDICATOR: &str = "extras.loadingIndicator";
const LOADING_RETRY: &str = "extras.loadingRetry";
const SUBAGENT_POLL: &str = "extras.subagentPoll";
/// The viewer's own `scheduleBackfill`, zero delay, separate from family b's because the two
/// projectors keep separate placeholder queues.
const SUBAGENT_BACKFILL: &str = "extras.subagentBackfill";

/// A repeating clock, armed while `live` and dropped when it stops.
fn interval(timers: &mut TimerTable, key: &str, live: bool, now_ms: f64, every_ms: f64) {
    if !live {
        timers.cancel(key);
    } else if !timers.is_armed(key) {
        timers.arm_interval(key, now_ms, every_ms);
    }
}

/// A one-shot at an absolute moment, dropped when the deadline goes away.
fn deadline(timers: &mut TimerTable, key: &str, at_ms: Option<f64>, now_ms: f64) {
    match at_ms {
        Some(at) => timers.arm(key, now_ms, (at - now_ms).max(0.0)),
        None => {
            timers.cancel(key);
        }
    }
}
