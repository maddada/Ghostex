use chrono::{SecondsFormat, TimeZone, Utc};
use serde_json::{Map, Value};

/*
CDXC:ActivitySuppressionPolicy 2026-07-29-12:00:
Every activity-suppression rule lives here so clients never re-implement or
partially mirror them. The policy has four layers:

1. Initial suppression (`suppressedUntil`, INITIAL_ACTIVITY_SUPPRESSION_MS):
   launch/resume/wake/agentDetected reset activity to idle and ignore
   passive (title-derived) signals for the window, so replayed terminal
   titles cannot resurrect stale working/attention. Explicit agent-hook
   activity intentionally bypasses this layer.
2. Attention suppression (`attentionSuppressedUntil`,
   ESCAPE_ATTENTION_SUPPRESSION_MS): a user Escape acknowledges the session
   and blocks attention (except bell/terminalError) for the window.
3. Attention promotion gate (MIN_WORKING_DURATION_BEFORE_ATTENTION_MS):
   passive working→attention promotion requires the working stint to have
   lasted the minimum duration, so spinner flickers cannot ring attention.
4. Meaningful-activity clock (`lastMeaningfulActivityAt`,
   MIN_MEANINGFUL_WORKING_DURATION_MS): sidebar recency ordering must not be
   bumped by short working blips (tiny commands, wake redraws). The clock
   advances only on attention entry and on working stints that persist past
   the minimum duration. `lastActiveAt` keeps its historical semantics (any
   working/attention entry) because auto-sleep and "Last Active" labels want
   raw activity; recency sorting reads `meaningfulActivityAt` instead.
*/
pub const INITIAL_ACTIVITY_SUPPRESSION_MS: i64 = 12_000;
pub const ESCAPE_ATTENTION_SUPPRESSION_MS: i64 = 5_000;
pub const MIN_WORKING_DURATION_BEFORE_ATTENTION_MS: i64 = 5_000;
pub const MIN_MEANINGFUL_WORKING_DURATION_MS: i64 = 10_000;
const TITLE_ACTIVITY_WINDOW_MS: i64 = 1_000;
const TITLE_ACTIVITY_HEARTBEAT_MS: i64 = 2_000;
const SLOW_SPINNER_ACTIVITY_WINDOW_MS: i64 = 5_000;

const CLAUDE_CODE_IDLE_MARKERS: &[char] = &['\u{2733}', '*'];
const CLAUDE_CODE_WORKING_MARKERS: &[char] = &[
    '\u{2810}', '\u{2802}', '\u{00b7}', '\u{2736}', '\u{273b}', '\u{273d}', '\u{2738}', '\u{2739}',
    '\u{273a}', '\u{2737}', '\u{2734}', '\u{25d0}', '\u{25d1}', '\u{25d2}', '\u{25d3}',
];
const CODEX_WORKING_MARKERS: &[char] = &[
    '\u{2838}', '\u{2834}', '\u{283c}', '\u{2827}', '\u{2826}', '\u{280f}', '\u{280b}', '\u{2807}',
    '\u{2819}', '\u{2839}',
];

#[derive(Debug)]
pub struct ActivityUpdate {
    pub activity: Value,
    pub entered_attention: bool,
    pub last_active_at: Option<String>,
    pub previous_activity: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ActivityState {
    activity: String,
    agent_name: Option<String>,
    attention_event_id: Option<String>,
    attention_suppressed_until: Option<String>,
    has_seen_working: Option<bool>,
    is_acknowledged: Option<bool>,
    last_changed_at: Option<String>,
    last_meaningful_activity_at: Option<String>,
    last_title: Option<String>,
    last_title_change_at: Option<String>,
    suppressed_until: Option<String>,
    working_source: Option<String>,
    working_started_at: Option<String>,
}

#[derive(Clone, Debug)]
struct TitleStatusSignal {
    agent_name: String,
    state: String,
}

struct ActivityInput {
    activity: Option<String>,
    agent_id: Option<String>,
    event: Option<String>,
    now_iso: String,
    now_ms: i64,
    previous: ActivityState,
    settled_title: Option<String>,
    title: Option<String>,
}

/*
CDXC:SessionStatus 2026-06-21-19:26:
Rust server must match TypeScript gxserver for working, idle, and attention transitions. Agent hooks are the authoritative explicit activity source, while title-derived spinner state is only trusted through the same suppression, stale-window, and same-title stop rules used by every existing Ghostex client.
*/
pub fn compute_activity_update(
    session: &Value,
    params: &Map<String, Value>,
    forced_event: Option<&str>,
) -> ActivityUpdate {
    let runtime_settings = object_field(session, "runtimeSettings");
    let fallback_activity = runtime_settings
        .get("activity")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "working" | "attention"))
        .unwrap_or("idle");
    let previous =
        normalize_agent_activity_state(runtime_settings.get("agentActivity"), fallback_activity);
    let previous_activity = previous.activity.clone();
    let now_ms_value = params
        .get("nowMs")
        .and_then(Value::as_i64)
        .unwrap_or_else(now_ms);
    let event = forced_event
        .map(str::to_string)
        .or_else(|| read_text(params, "event"))
        .filter(|value| normalize_activity_event(Some(value.as_str())).is_some());
    let previous_state = previous.clone();
    let activity = apply_agent_activity_transition(ActivityInput {
        activity: params
            .get("activity")
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "idle" | "working" | "attention"))
            .map(str::to_string),
        agent_id: read_text(params, "agentName")
            .or_else(|| read_text_value(session, "agentId"))
            .or_else(|| previous.agent_name.clone()),
        event: event.clone(),
        now_iso: iso_from_ms(now_ms_value),
        now_ms: now_ms_value,
        previous,
        settled_title: read_text(params, "settledTitle"),
        title: read_text(params, "title"),
    });
    /*
    Sessions that predate the meaningful-activity clock seed it from their
    durable lastActiveAt so recency sorting stays stable across the migration,
    but only when the transition already produced a persistable change: a
    seed-only difference must never turn a no-op event into a state rewrite.
    */
    let seed_recency = (previous_state.last_meaningful_activity_at.is_none()
        && !states_equal_ignoring_meaningful_clock(&previous_state, &activity))
    .then(|| read_text_value(session, "lastActiveAt"))
    .flatten();
    let activity = apply_meaningful_activity_clock(
        &previous_state,
        activity,
        seed_recency,
        event.as_deref(),
        now_ms_value,
    );
    let next_activity = activity.activity.as_str();
    let last_active_at = if matches!(next_activity, "working" | "attention") {
        activity
            .last_changed_at
            .clone()
            .or_else(|| Some(iso_from_ms(now_ms_value)))
    } else {
        read_text_value(session, "lastActiveAt")
    };
    ActivityUpdate {
        entered_attention: previous_activity != "attention" && next_activity == "attention",
        activity: activity.to_value(),
        last_active_at,
        previous_activity,
    }
}

pub fn normalize_agent_activity_value(value: Option<&Value>, fallback: &str) -> Value {
    normalize_agent_activity_state(value, fallback).to_value()
}

/*
CDXC:SessionStatus 2026-06-21-19:26:
Rust presentation must use the same effective activity projection as TypeScript gxserver: title-derived spinner working is allowed to expire on read without rewriting durable session state, and a timed presentation refresh emits the idle transition when no later terminal title arrives.
*/
pub fn effective_agent_activity_value(
    value: Option<&Value>,
    fallback: &str,
    now_ms_value: i64,
) -> Value {
    effective_agent_activity_state(
        normalize_agent_activity_state(value, fallback),
        now_ms_value,
    )
    .to_value()
}

fn agent_activity_stale_projection_delay_ms(
    value: Option<&Value>,
    now_ms_value: i64,
) -> Option<i64> {
    let state = normalize_agent_activity_state(value, "idle");
    if state.activity != "working"
        || state.working_source.as_deref() == Some("explicit")
        || !requires_observed_title_transitions(state.agent_name.as_deref())
    {
        return None;
    }
    let Some(last_title_change_ms) = state.last_title_change_at.as_deref().and_then(parse_iso_ms)
    else {
        return Some(0);
    };
    let title_delay_ms = 0.max(
        last_title_change_ms + get_title_activity_window_ms(state.agent_name.as_deref())
            - now_ms_value,
    );
    let Some(attention_suppressed_until) =
        active_attention_suppressed_until_ms(&state, now_ms_value)
    else {
        return Some(title_delay_ms);
    };
    Some(title_delay_ms.max(attention_suppressed_until - now_ms_value))
}

pub fn is_stale_activity_event(session: &Value, incoming_now_ms: i64) -> bool {
    let current_changed_at = object_field(session, "runtimeSettings")
        .get("agentActivity")
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("lastChangedAt"))
        .and_then(Value::as_str)
        .and_then(parse_iso_ms);
    current_changed_at
        .map(|current| incoming_now_ms < current)
        .unwrap_or(false)
}

fn apply_agent_activity_transition(input: ActivityInput) -> ActivityState {
    let previous = input.previous.clone();
    let previous_activity = previous.activity.clone();
    let has_explicit_activity = input.activity.is_some();
    if matches!(
        input.event.as_deref(),
        Some("launch" | "resume" | "agentDetected" | "wake")
    ) {
        return ActivityState {
            activity: "idle".to_string(),
            agent_name: normalize_status_agent_name(input.agent_id.as_deref()),
            has_seen_working: Some(false),
            is_acknowledged: Some(true),
            last_changed_at: Some(input.now_iso.clone()),
            suppressed_until: Some(iso_from_ms(input.now_ms + INITIAL_ACTIVITY_SUPPRESSION_MS)),
            ..ActivityState::default()
        };
    }

    if input.event.as_deref() == Some("acknowledge") {
        let mut next = previous.clone();
        next.activity = "idle".to_string();
        next.is_acknowledged = Some(true);
        next.last_changed_at = Some(input.now_iso.clone());
        return next;
    }

    if input.event.as_deref() == Some("escape") {
        let mut next = previous.clone();
        next.agent_name =
            normalize_status_agent_name(input.agent_id.as_deref()).or(next.agent_name);
        next.attention_suppressed_until =
            Some(iso_from_ms(input.now_ms + ESCAPE_ATTENTION_SUPPRESSION_MS));
        next.is_acknowledged = Some(true);
        if previous_activity == "attention" {
            next.activity = "idle".to_string();
            next.attention_event_id = None;
            next.last_changed_at = Some(input.now_iso.clone());
            next.working_source = None;
            next.working_started_at = None;
        } else if next.last_changed_at.is_none() {
            next.last_changed_at = Some(input.now_iso.clone());
        }
        return next;
    }

    let title_signal = classify_terminal_title_status(
        input.title.as_deref(),
        input.agent_id.as_deref().or(previous.agent_name.as_deref()),
    );
    let title_transition = resolve_title_transition(&input, &previous, title_signal.as_ref());

    if input.event.as_deref() == Some("title")
        && !has_explicit_activity
        && previous.activity == "working"
        && previous.working_source.as_deref() == Some("explicit")
        && title_signal.as_ref().map(|signal| signal.state.as_str()) != Some("attention")
        && !is_trusted_spinner_stop_title(&input, &previous, title_signal.as_ref())
        && !is_trusted_settled_title_stop(&input, &previous, title_signal.as_ref())
    {
        let mut next = previous.clone();
        next.agent_name = title_signal
            .as_ref()
            .map(|signal| signal.agent_name.clone())
            .or_else(|| normalize_status_agent_name(input.agent_id.as_deref()))
            .or(next.agent_name);
        if next.last_changed_at.is_none() {
            next.last_changed_at = Some(input.now_iso.clone());
        }
        if title_signal.as_ref().map(|signal| signal.state.as_str()) == Some("working") {
            next.last_title = title_transition.last_title;
            next.last_title_change_at = title_transition.last_title_change_at;
        }
        return next;
    }

    if let Some(signal) = title_signal.as_ref() {
        if input.event.as_deref() == Some("title")
            && previous.agent_name.is_some()
            && previous.agent_name.as_deref() != Some(signal.agent_name.as_str())
            && previous.activity == "idle"
            && previous.has_seen_working != Some(true)
        {
            return ActivityState {
                activity: "idle".to_string(),
                agent_name: Some(signal.agent_name.clone()),
                has_seen_working: Some(false),
                is_acknowledged: Some(true),
                last_changed_at: Some(input.now_iso.clone()),
                last_title: title_transition.last_title,
                last_title_change_at: title_transition.last_title_change_at,
                suppressed_until: Some(iso_from_ms(input.now_ms + INITIAL_ACTIVITY_SUPPRESSION_MS)),
                ..ActivityState::default()
            };
        }
    }

    if !has_explicit_activity
        && previous
            .suppressed_until
            .as_deref()
            .and_then(parse_iso_ms)
            .map(|suppressed_until| input.now_ms < suppressed_until)
            == Some(true)
    {
        let mut next = previous.clone();
        next.activity = "idle".to_string();
        next.has_seen_working = Some(false);
        next.is_acknowledged = Some(true);
        next.last_changed_at = Some(input.now_iso.clone());
        return next;
    }

    let requested = input
        .activity
        .clone()
        .or_else(|| activity_from_event(input.event.as_deref()))
        .or_else(|| {
            activity_from_title_signal(
                title_signal.as_ref().map(|signal| signal.state.as_str()),
                &previous,
                title_signal
                    .as_ref()
                    .map(|signal| signal.agent_name.as_str()),
                input.event.as_deref(),
            )
        });
    let agent_name = title_signal
        .as_ref()
        .map(|signal| signal.agent_name.clone())
        .or_else(|| normalize_status_agent_name(input.agent_id.as_deref()))
        .or_else(|| previous.agent_name.clone());
    let active_attention_suppressed_until =
        active_attention_suppressed_until(&previous, input.now_ms);

    if requested.as_deref() == Some("working") {
        let working_source = if input.event.as_deref() == Some("title")
            && title_signal.as_ref().map(|signal| signal.state.as_str()) == Some("working")
        {
            "title"
        } else {
            "explicit"
        };
        if working_source == "title"
            && is_title_derived_working_stale(
                title_signal
                    .as_ref()
                    .map(|signal| signal.agent_name.as_str()),
                title_transition.last_title_change_at.as_deref(),
                input.now_ms,
            )
        {
            return state_for_stale_title_working(
                &previous,
                agent_name,
                title_transition,
                &input.now_iso,
            );
        }
        return ActivityState {
            activity: "working".to_string(),
            agent_name,
            attention_suppressed_until: active_attention_suppressed_until,
            has_seen_working: Some(true),
            is_acknowledged: Some(false),
            last_changed_at: Some(if previous_activity == "working" {
                previous
                    .last_changed_at
                    .unwrap_or_else(|| input.now_iso.clone())
            } else {
                input.now_iso.clone()
            }),
            last_title: title_transition.last_title,
            last_title_change_at: title_transition.last_title_change_at,
            working_source: Some(working_source.to_string()),
            working_started_at: Some(if previous_activity == "working" {
                previous
                    .working_started_at
                    .unwrap_or_else(|| input.now_iso.clone())
            } else {
                input.now_iso.clone()
            }),
            ..ActivityState::default()
        };
    }

    if requested.as_deref() == Some("attention") {
        if let Some(until) = active_attention_suppressed_until.clone() {
            if !matches!(input.event.as_deref(), Some("bell" | "terminalError")) {
                return state_for_suppressed_attention(
                    &previous,
                    agent_name,
                    title_transition,
                    &input.now_iso,
                    until,
                );
            }
        }
        if previous_activity == "attention" {
            return previous;
        }
        if !has_explicit_activity
            && previous.is_acknowledged == Some(true)
            && title_signal.as_ref().map(|signal| signal.state.as_str()) == Some("attention")
        {
            let mut next = previous.clone();
            next.activity = "idle".to_string();
            next.agent_name = agent_name;
            next.last_title = title_transition.last_title;
            next.last_title_change_at = title_transition.last_title_change_at;
            return next;
        }
        let working_started_ms = previous
            .working_started_at
            .as_deref()
            .and_then(parse_iso_ms);
        let can_enter_attention = has_explicit_activity
            || matches!(input.event.as_deref(), Some("bell" | "terminalError"))
            || title_signal
                .as_ref()
                .map(|signal| signal.agent_name.as_str())
                == Some("antigravity")
            || working_started_ms
                .map(|started| input.now_ms - started >= MIN_WORKING_DURATION_BEFORE_ATTENTION_MS)
                == Some(true);
        if !can_enter_attention {
            let mut next = previous.clone();
            next.activity = "idle".to_string();
            next.agent_name = agent_name;
            next.last_changed_at = Some(input.now_iso.clone());
            next.working_source = None;
            next.working_started_at = None;
            return next;
        }
        return ActivityState {
            activity: "attention".to_string(),
            agent_name,
            attention_event_id: Some(create_attention_event_id(input.now_ms)),
            has_seen_working: Some(true),
            is_acknowledged: Some(false),
            last_changed_at: Some(input.now_iso.clone()),
            last_title: title_transition.last_title,
            last_title_change_at: title_transition.last_title_change_at,
            working_started_at: previous.working_started_at,
            ..ActivityState::default()
        };
    }

    let mut next = previous.clone();
    next.activity = "idle".to_string();
    next.agent_name = agent_name;
    if previous_activity == "idle" {
        next.last_changed_at = next.last_changed_at.or_else(|| Some(input.now_iso.clone()));
    } else {
        next.last_changed_at = Some(input.now_iso.clone());
    }
    next.last_title = title_transition.last_title;
    next.last_title_change_at = title_transition.last_title_change_at;
    if let Some(until) = active_attention_suppressed_until {
        next.attention_suppressed_until = Some(until);
    }
    next.working_source = None;
    next.working_started_at = None;
    next
}

/*
CDXC:ActivitySuppressionPolicy 2026-07-29-12:00:
The meaningful-activity clock is applied once, after every transition branch,
so early returns and from-scratch state rebuilds (launch/resume/wake resets,
working entry, agent switches) cannot drop or double-apply it. It advances on
attention entry, on any event observed while a working stint has already
lasted MIN_MEANINGFUL_WORKING_DURATION_MS, and when a qualifying stint ends.
Lifecycle reset events never advance it: a wake that clears frozen stale
working state is not user-visible activity.
*/
fn apply_meaningful_activity_clock(
    previous: &ActivityState,
    mut next: ActivityState,
    seed_recency: Option<String>,
    event: Option<&str>,
    now_ms_value: i64,
) -> ActivityState {
    if next.last_meaningful_activity_at.is_none() {
        next.last_meaningful_activity_at = previous
            .last_meaningful_activity_at
            .clone()
            .or(seed_recency);
    }
    if matches!(event, Some("launch" | "resume" | "wake" | "agentDetected")) {
        return next;
    }
    let entered_attention = next.activity == "attention" && previous.activity != "attention";
    if entered_attention {
        next.last_meaningful_activity_at = Some(iso_from_ms(now_ms_value));
        return next;
    }
    if next.activity == "working" {
        if working_stint_is_meaningful(
            next.working_started_at.as_deref(),
            meaningful_stint_end_ms(&next, now_ms_value),
        ) {
            next.last_meaningful_activity_at = Some(iso_from_ms(now_ms_value));
        }
        return next;
    }
    if previous.activity == "working" {
        let stint_end_ms = meaningful_stint_end_ms(previous, now_ms_value);
        if working_stint_is_meaningful(previous.working_started_at.as_deref(), stint_end_ms) {
            next.last_meaningful_activity_at = Some(iso_from_ms(stint_end_ms));
        }
    }
    next
}

/*
Title-derived working can go stale and be closed out well after the spinner
actually stopped, so a stint's end time is the last observed working evidence
(the last title change) rather than the transition's wall clock. Explicit hook
working stops exactly when the idle hook arrives, so `now` is accurate there.
*/
fn meaningful_stint_end_ms(state: &ActivityState, now_ms_value: i64) -> i64 {
    if state.working_source.as_deref() == Some("title") {
        return state
            .last_title_change_at
            .as_deref()
            .and_then(parse_iso_ms)
            .unwrap_or(now_ms_value)
            .min(now_ms_value);
    }
    now_ms_value
}

fn states_equal_ignoring_meaningful_clock(left: &ActivityState, right: &ActivityState) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.last_meaningful_activity_at = None;
    right.last_meaningful_activity_at = None;
    left == right
}

fn working_stint_is_meaningful(working_started_at: Option<&str>, stint_end_ms: i64) -> bool {
    working_started_at
        .and_then(parse_iso_ms)
        .map(|started| stint_end_ms - started >= MIN_MEANINGFUL_WORKING_DURATION_MS)
        == Some(true)
}

/*
CDXC:ActivitySuppressionPolicy 2026-07-29-12:00:
Presentation reads recency through this projection: while a session is
effectively working past the meaningful threshold the published recency is
"now", so recency keeps advancing between durable writes; otherwise it is the
stored clock. Blip stints never reach either branch's bump.
*/
pub fn meaningful_activity_at(value: Option<&Value>, now_ms_value: i64) -> Option<String> {
    let state = normalize_agent_activity_state(value, "idle");
    let effective = effective_agent_activity_state(state.clone(), now_ms_value);
    if effective.activity == "working"
        && working_stint_is_meaningful(effective.working_started_at.as_deref(), now_ms_value)
    {
        return Some(iso_from_ms(now_ms_value));
    }
    state.last_meaningful_activity_at
}

pub fn effective_working_started_at(value: Option<&Value>, now_ms_value: i64) -> Option<String> {
    let effective =
        effective_agent_activity_state(normalize_agent_activity_state(value, "idle"), now_ms_value);
    (effective.activity == "working")
        .then(|| effective.working_started_at)
        .flatten()
}

/*
One combined refresh deadline for the presentation timer: the earlier of the
stale title-derived-working expiry and the moment an ongoing working stint
crosses the meaningful threshold. Both boundaries change published projection
state without a durable write, so a timed delta must surface them.
*/
pub fn agent_activity_presentation_refresh_delay_ms(
    value: Option<&Value>,
    now_ms_value: i64,
) -> Option<i64> {
    let stale = agent_activity_stale_projection_delay_ms(value, now_ms_value);
    let crossing = meaningful_working_crossing_delay_ms(value, now_ms_value);
    match (stale, crossing) {
        (Some(stale), Some(crossing)) => Some(stale.min(crossing)),
        (delay, None) | (None, delay) => delay,
    }
}

fn meaningful_working_crossing_delay_ms(value: Option<&Value>, now_ms_value: i64) -> Option<i64> {
    let effective =
        effective_agent_activity_state(normalize_agent_activity_state(value, "idle"), now_ms_value);
    if effective.activity != "working" {
        return None;
    }
    let started_ms = effective
        .working_started_at
        .as_deref()
        .and_then(parse_iso_ms)?;
    let elapsed = now_ms_value - started_ms;
    (elapsed < MIN_MEANINGFUL_WORKING_DURATION_MS)
        .then(|| MIN_MEANINGFUL_WORKING_DURATION_MS - elapsed.max(0))
}

#[derive(Default)]
struct TitleTransition {
    last_title: Option<String>,
    last_title_change_at: Option<String>,
}

fn is_trusted_spinner_stop_title(
    input: &ActivityInput,
    previous: &ActivityState,
    title_signal: Option<&TitleStatusSignal>,
) -> bool {
    let Some(title) = (input.event.as_deref() == Some("title"))
        .then_some(input.title.as_deref())
        .flatten()
        .and_then(normalize_text_str)
    else {
        return false;
    };
    let agent_name = title_signal
        .map(|signal| signal.agent_name.as_str())
        .or(previous.agent_name.as_deref());
    if title_signal.map(|signal| signal.state.as_str()) != Some("idle")
        || !requires_observed_title_transitions(agent_name)
        || previous.last_title.is_none()
    {
        return false;
    }
    let previous_signal =
        classify_terminal_title_status(previous.last_title.as_deref(), agent_name);
    if previous_signal.as_ref().map(|signal| signal.state.as_str()) != Some("working") {
        return false;
    }
    let previous_signature =
        create_title_activity_signature(previous.last_title.as_deref(), previous_signal.as_ref());
    let current_signal = TitleStatusSignal {
        agent_name: previous_signal.expect("checked").agent_name,
        state: "working".to_string(),
    };
    let current_signature = create_title_activity_signature(Some(&title), Some(&current_signal));
    previous_signature.is_some() && previous_signature == current_signature
}

fn is_trusted_settled_title_stop(
    input: &ActivityInput,
    previous: &ActivityState,
    title_signal: Option<&TitleStatusSignal>,
) -> bool {
    if input.event.as_deref() != Some("title")
        || previous.activity != "working"
        || previous.working_source.as_deref() != Some("explicit")
        || title_signal.map(|signal| signal.state.as_str()) != Some("idle")
        || !requires_observed_title_transitions(
            title_signal.map(|signal| signal.agent_name.as_str()),
        )
    {
        return false;
    }
    let settled_signature = create_title_activity_signature(input.settled_title.as_deref(), None);
    let title_signature = create_title_activity_signature(input.title.as_deref(), title_signal);
    if settled_signature.is_none() || settled_signature != title_signature {
        return false;
    }
    let Some(working_started_ms) = previous
        .working_started_at
        .as_deref()
        .and_then(parse_iso_ms)
    else {
        return false;
    };
    input.now_ms - working_started_ms >= MIN_WORKING_DURATION_BEFORE_ATTENTION_MS
}

fn resolve_title_transition(
    input: &ActivityInput,
    previous: &ActivityState,
    title_signal: Option<&TitleStatusSignal>,
) -> TitleTransition {
    let title = if input.event.as_deref() == Some("title") {
        input.title.as_deref().and_then(normalize_text_str)
    } else {
        None
    };
    let Some(title) = title else {
        return TitleTransition {
            last_title: previous.last_title.clone(),
            last_title_change_at: previous.last_title_change_at.clone(),
        };
    };
    let same_agent = previous.agent_name.is_none()
        || title_signal.is_none()
        || previous.agent_name.as_deref() == title_signal.map(|signal| signal.agent_name.as_str());
    let same_title = previous.last_title.as_deref().map(str::trim) == Some(title.trim());
    let keep_previous = same_agent
        && (same_title
            || is_within_same_semantic_title_heartbeat(
                previous,
                &title,
                title_signal,
                input.now_ms,
            ));
    TitleTransition {
        last_title: Some(title),
        last_title_change_at: if keep_previous {
            previous
                .last_title_change_at
                .clone()
                .or_else(|| Some(input.now_iso.clone()))
        } else {
            Some(input.now_iso.clone())
        },
    }
}

fn is_within_same_semantic_title_heartbeat(
    previous: &ActivityState,
    title: &str,
    title_signal: Option<&TitleStatusSignal>,
    now_ms_value: i64,
) -> bool {
    let Some(signal) = title_signal else {
        return false;
    };
    if signal.state != "working"
        || !requires_observed_title_transitions(Some(signal.agent_name.as_str()))
        || previous.last_title.is_none()
        || previous.last_title_change_at.is_none()
    {
        return false;
    }
    let Some(last_title_change_ms) = previous
        .last_title_change_at
        .as_deref()
        .and_then(parse_iso_ms)
    else {
        return false;
    };
    if now_ms_value - last_title_change_ms >= TITLE_ACTIVITY_HEARTBEAT_MS {
        return false;
    }
    create_title_activity_signature(previous.last_title.as_deref(), Some(signal))
        == create_title_activity_signature(Some(title), Some(signal))
}

fn is_title_derived_working_stale(
    agent_name: Option<&str>,
    last_title_change_at: Option<&str>,
    now_ms_value: i64,
) -> bool {
    if !requires_observed_title_transitions(agent_name) {
        return false;
    }
    let Some(last_title_change_ms) = last_title_change_at.and_then(parse_iso_ms) else {
        return true;
    };
    now_ms_value - last_title_change_ms > get_title_activity_window_ms(agent_name)
}

fn state_for_stale_title_working(
    previous: &ActivityState,
    agent_name: Option<String>,
    title_transition: TitleTransition,
    now_iso_value: &str,
) -> ActivityState {
    let mut next = previous.clone();
    next.activity = "idle".to_string();
    next.agent_name = agent_name;
    next.last_changed_at = if previous.activity == "idle" {
        previous
            .last_changed_at
            .clone()
            .or_else(|| Some(now_iso_value.to_string()))
    } else {
        Some(now_iso_value.to_string())
    };
    next.last_title = title_transition.last_title;
    next.last_title_change_at = title_transition.last_title_change_at;
    next.working_source = None;
    next.working_started_at = None;
    next
}

fn state_for_suppressed_attention(
    previous: &ActivityState,
    agent_name: Option<String>,
    title_transition: TitleTransition,
    now_iso_value: &str,
    attention_suppressed_until: String,
) -> ActivityState {
    let mut next = previous.clone();
    next.activity = "idle".to_string();
    next.agent_name = agent_name;
    next.attention_event_id = None;
    next.attention_suppressed_until = Some(attention_suppressed_until);
    next.has_seen_working = Some(false);
    next.is_acknowledged = Some(true);
    next.last_changed_at = Some(now_iso_value.to_string());
    next.last_title = title_transition.last_title;
    next.last_title_change_at = title_transition.last_title_change_at;
    next.working_source = None;
    next.working_started_at = None;
    next
}

fn effective_agent_activity_state(state: ActivityState, now_ms_value: i64) -> ActivityState {
    if !is_stored_title_derived_working_stale(&state, now_ms_value) {
        return state;
    }
    let title_transition = TitleTransition {
        last_title: state.last_title.clone(),
        last_title_change_at: state.last_title_change_at.clone(),
    };
    if let Some(until) = active_attention_suppressed_until(&state, now_ms_value) {
        return state_for_suppressed_attention(
            &state,
            state.agent_name.clone(),
            title_transition,
            &iso_from_ms(now_ms_value),
            until,
        );
    }
    state_for_stale_title_working(
        &state,
        state.agent_name.clone(),
        title_transition,
        &iso_from_ms(now_ms_value),
    )
}

fn is_stored_title_derived_working_stale(state: &ActivityState, now_ms_value: i64) -> bool {
    state.activity == "working"
        && state.working_source.as_deref() != Some("explicit")
        && is_title_derived_working_stale(
            state.agent_name.as_deref(),
            state.last_title_change_at.as_deref(),
            now_ms_value,
        )
}

fn normalize_agent_activity_state(value: Option<&Value>, fallback: &str) -> ActivityState {
    let record = value
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    ActivityState {
        activity: record
            .get("activity")
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "idle" | "working" | "attention"))
            .unwrap_or(fallback)
            .to_string(),
        agent_name: normalize_status_agent_name(
            read_text_from_map(&record, "agentName").as_deref(),
        ),
        attention_event_id: read_text_from_map(&record, "attentionEventId"),
        attention_suppressed_until: read_text_from_map(&record, "attentionSuppressedUntil"),
        has_seen_working: record.get("hasSeenWorking").and_then(Value::as_bool),
        is_acknowledged: record.get("isAcknowledged").and_then(Value::as_bool),
        last_changed_at: read_text_from_map(&record, "lastChangedAt"),
        last_meaningful_activity_at: read_text_from_map(&record, "lastMeaningfulActivityAt"),
        last_title: read_text_from_map(&record, "lastTitle"),
        last_title_change_at: read_text_from_map(&record, "lastTitleChangeAt"),
        suppressed_until: read_text_from_map(&record, "suppressedUntil"),
        working_source: read_text_from_map(&record, "workingSource")
            .filter(|value| matches!(value.as_str(), "explicit" | "title")),
        working_started_at: read_text_from_map(&record, "workingStartedAt"),
    }
}

fn activity_from_event(event: Option<&str>) -> Option<String> {
    match event {
        Some("bell" | "terminalError") => Some("attention".to_string()),
        Some("terminalExited") => Some("idle".to_string()),
        _ => None,
    }
}

fn activity_from_title_signal(
    signal: Option<&str>,
    previous: &ActivityState,
    agent_name: Option<&str>,
    event: Option<&str>,
) -> Option<String> {
    if matches!(signal, Some("working" | "attention")) {
        return signal.map(str::to_string);
    }
    if signal == Some("idle") {
        if agent_name == Some("claude") {
            return Some("idle".to_string());
        }
        let same_agent = previous.agent_name.is_none()
            || agent_name.is_none()
            || previous.agent_name.as_deref() == agent_name;
        return Some(
            if same_agent
                && previous.working_source.as_deref() == Some("explicit")
                && previous.has_seen_working == Some(true)
                && previous.is_acknowledged != Some(true)
            {
                "attention"
            } else {
                "idle"
            }
            .to_string(),
        );
    }
    if event == Some("title") && previous.has_seen_working == Some(true) {
        if previous.agent_name.as_deref() == Some("claude") {
            return Some("idle".to_string());
        }
        return Some(
            if previous.is_acknowledged != Some(true)
                && previous.working_source.as_deref() == Some("explicit")
            {
                "attention"
            } else {
                "idle"
            }
            .to_string(),
        );
    }
    None
}

fn active_attention_suppressed_until(state: &ActivityState, now_ms_value: i64) -> Option<String> {
    state
        .attention_suppressed_until
        .as_deref()
        .and_then(parse_iso_ms)
        .filter(|suppressed_until| now_ms_value < *suppressed_until)
        .and_then(|_| state.attention_suppressed_until.clone())
}

fn active_attention_suppressed_until_ms(state: &ActivityState, now_ms_value: i64) -> Option<i64> {
    state
        .attention_suppressed_until
        .as_deref()
        .and_then(parse_iso_ms)
        .filter(|suppressed_until| now_ms_value < *suppressed_until)
}

fn classify_terminal_title_status(
    title: Option<&str>,
    known_agent_name: Option<&str>,
) -> Option<TitleStatusSignal> {
    let title = title?;
    let normalized_agent_name = normalize_status_agent_name(known_agent_name);
    let normalized_title = normalize_spaces(title);
    if is_opencode_title_prefix(&normalized_title) {
        return None;
    }
    if let Some(state) =
        get_cursor_title_state(title, normalized_agent_name.as_deref() == Some("cursor"))
    {
        return Some(signal("cursor", state));
    }
    if let Some(state) = get_antigravity_title_state(
        title,
        normalized_agent_name.as_deref() == Some("antigravity"),
    ) {
        return Some(signal("antigravity", state));
    }
    if let Some(state) =
        get_claude_code_title_state(title, normalized_agent_name.as_deref() == Some("claude"))
    {
        return Some(signal("claude", state));
    }
    if let Some(state) = get_pi_title_state(title, normalized_agent_name.as_deref() == Some("pi")) {
        return Some(signal("pi", state));
    }
    if let Some(state) =
        get_codex_title_state(title, normalized_agent_name.as_deref() == Some("codex"))
    {
        return Some(signal("codex", state));
    }
    if let Some(state) =
        get_gemini_title_state(title, normalized_agent_name.as_deref() == Some("gemini"))
    {
        return Some(signal("gemini", state));
    }
    if let Some(state) =
        get_copilot_title_state(title, normalized_agent_name.as_deref() == Some("copilot"))
    {
        return Some(signal("copilot", state));
    }
    None
}

fn signal(agent_name: &str, state: &str) -> TitleStatusSignal {
    TitleStatusSignal {
        agent_name: agent_name.to_string(),
        state: state.to_string(),
    }
}

fn normalize_status_agent_name(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().to_ascii_lowercase();
    let mapped = match normalized.as_str() {
        "claude code" => "claude",
        "codex cli" => "codex",
        "github copilot" => "copilot",
        "agy" | "antigravity cli" | "antigravity" => "antigravity",
        "cursor cli" | "cursor-agent" | "cursor agent" => "cursor",
        "open code" => "opencode",
        "\u{03c0}" => "pi",
        other => other,
    };
    matches!(
        mapped,
        "antigravity" | "claude" | "codex" | "cursor" | "gemini" | "copilot" | "opencode" | "pi"
    )
    .then(|| mapped.to_string())
}

fn requires_observed_title_transitions(agent_name: Option<&str>) -> bool {
    matches!(agent_name, Some("claude" | "codex" | "cursor" | "pi"))
}

fn get_title_activity_window_ms(agent_name: Option<&str>) -> i64 {
    if requires_observed_title_transitions(agent_name) {
        SLOW_SPINNER_ACTIVITY_WINDOW_MS
    } else {
        TITLE_ACTIVITY_WINDOW_MS
    }
}

fn create_title_activity_signature(
    title: Option<&str>,
    signal: Option<&TitleStatusSignal>,
) -> Option<String> {
    let normalized_title = title
        .map(normalize_spaces)
        .filter(|value| !value.is_empty())?;
    if !matches!(
        signal.map(|signal| signal.state.as_str()),
        Some("working" | "attention")
    ) {
        return Some(normalized_title);
    }
    let mut chars = normalized_title.chars().collect::<Vec<_>>();
    match signal.map(|signal| signal.agent_name.as_str()) {
        Some("codex" | "pi") => {
            for ch in &mut chars {
                if CODEX_WORKING_MARKERS.contains(ch) {
                    *ch = ' ';
                }
            }
        }
        Some("claude") => {
            for ch in &mut chars {
                if CLAUDE_CODE_WORKING_MARKERS.contains(ch) || CLAUDE_CODE_IDLE_MARKERS.contains(ch)
                {
                    *ch = ' ';
                }
            }
        }
        _ => {}
    }
    let mut signature = chars.into_iter().collect::<String>();
    if matches!(
        signal.map(|signal| signal.agent_name.as_str()),
        Some("codex" | "pi")
    ) && is_codex_action_required_title(&signature)
    {
        signature = "Action Required".to_string();
    }
    if signal.map(|signal| signal.agent_name.as_str()) == Some("cursor") {
        signature = replace_cursor_working_suffix(&signature);
    }
    Some(collapse_signature_noise(&signature))
}

fn get_cursor_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    if lower == "cursor agent - \u{2705} ready" || normalized.ends_with("\u{2705} Ready") {
        return Some("idle");
    }
    if normalized.ends_with_working_suffix() {
        return Some("working");
    }
    if lower == "cursor agent" {
        return Some("idle");
    }
    let has_cursor_keyword = lower.contains("cursor cli")
        || lower.contains("cursor-agent")
        || lower.contains("cursor agent")
        || lower == "cursor";
    (allow_agent_hint_match && has_cursor_keyword).then_some("idle")
}

trait CursorTitleExt {
    fn ends_with_working_suffix(&self) -> bool;
}

impl CursorTitleExt for str {
    fn ends_with_working_suffix(&self) -> bool {
        let Some(prefix) = self.strip_suffix('.') else {
            return self.ends_with("\u{23f3} Working ·") || self.ends_with("\u{23f3} Working .");
        };
        prefix.ends_with("\u{23f3} Working ") || prefix.ends_with("\u{23f3} Working .")
    }
}

fn get_codex_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    let has_codex_keyword = lower.contains("codex");
    let has_codex_working_marker = get_codex_working_marker(&normalized).is_some();
    if allow_agent_hint_match && is_codex_action_required_title(&normalized) {
        return Some("attention");
    }
    if !allow_agent_hint_match && !has_codex_keyword && !has_codex_working_marker {
        return None;
    }
    if has_codex_working_marker {
        Some("working")
    } else {
        Some("idle")
    }
}

fn get_claude_code_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    if get_cursor_title_state(title, false).is_some()
        || get_codex_title_state(title, true) == Some("attention")
    {
        return None;
    }
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    let has_claude_keyword = lower.contains("claude code") || lower.contains("claude");
    let has_inference_marker = contains_any_marker(&normalized, CLAUDE_CODE_IDLE_MARKERS)
        || contains_any_marker(&normalized, CLAUDE_CODE_WORKING_MARKERS);
    if !allow_agent_hint_match && !has_claude_keyword && !has_inference_marker {
        return None;
    }
    if contains_any_marker(&normalized, CLAUDE_CODE_IDLE_MARKERS) {
        return Some("idle");
    }
    if contains_any_marker(&normalized, CLAUDE_CODE_WORKING_MARKERS) {
        return Some("working");
    }
    has_claude_keyword.then_some("idle")
}

fn get_pi_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    let normalized = normalize_spaces(title);
    let has_prefix = pi_title_prefix(&normalized);
    if !allow_agent_hint_match && !has_prefix {
        return None;
    }
    if get_codex_working_marker(&normalized).is_some() {
        return Some("working");
    }
    (has_prefix || allow_agent_hint_match).then_some("idle")
}

fn get_gemini_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    if !allow_agent_hint_match
        && !lower.contains("gemini")
        && !normalized.contains('\u{2726}')
        && !normalized.contains('\u{25c7}')
    {
        return None;
    }
    if normalized.contains('\u{2726}') {
        return Some("working");
    }
    normalized.contains('\u{25c7}').then_some("idle")
}

fn get_antigravity_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    if lower == "\u{1f514} agy" {
        return Some("attention");
    }
    if lower == "agy" {
        return Some("idle");
    }
    (allow_agent_hint_match
        && (lower == "antigravity" || lower == "antigravity cli" || lower == "agy"))
        .then_some("idle")
}

fn get_copilot_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    if get_antigravity_title_state(title, false).is_some() {
        return None;
    }
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    if !allow_agent_hint_match
        && !lower.contains("copilot")
        && !lower.contains("github copilot")
        && !normalized.contains('\u{1f916}')
        && !normalized.contains('\u{1f514}')
    {
        return None;
    }
    if normalized.contains('\u{1f916}') {
        return Some("working");
    }
    normalized.contains('\u{1f514}').then_some("idle")
}

fn is_opencode_title_prefix(title: &str) -> bool {
    let stripped = trim_title_prefix_markers(title);
    stripped.to_ascii_lowercase().starts_with("oc |")
        || stripped.to_ascii_lowercase().starts_with("oc|")
}

fn pi_title_prefix(title: &str) -> bool {
    trim_title_prefix_markers(title).starts_with("\u{03c0} -")
        || trim_title_prefix_markers(title).starts_with("\u{03c0}-")
}

fn trim_title_prefix_markers(value: &str) -> &str {
    value.trim_start_matches(|ch: char| {
        ch.is_whitespace()
            || ('\u{2800}'..='\u{28ff}').contains(&ch)
            || matches!(
                ch,
                '\u{00b7}'
                    | '\u{2022}'
                    | '\u{22c5}'
                    | '\u{25e6}'
                    | '\u{2733}'
                    | '*'
                    | '\u{25d0}'
                    | '\u{25d1}'
                    | '\u{25d2}'
                    | '\u{25d3}'
                    | '\u{2726}'
                    | '\u{25c7}'
                    | '\u{1f916}'
                    | '\u{1f514}'
            )
    })
}

fn is_codex_action_required_title(title: &str) -> bool {
    let trimmed = title.trim_start();
    let Some(rest) = trimmed.strip_prefix('[') else {
        return false;
    };
    let Some((marker, after)) = rest.split_once(']') else {
        return false;
    };
    marker
        .chars()
        .any(|ch| matches!(ch, '!' | '.' | '\u{00b7}' | '\u{2802}'))
        && after.trim_start().starts_with("Action Required")
}

fn get_codex_working_marker(title: &str) -> Option<char> {
    title.chars().find(|ch| CODEX_WORKING_MARKERS.contains(ch))
}

fn contains_any_marker(title: &str, markers: &[char]) -> bool {
    title.chars().any(|ch| markers.contains(&ch))
}

fn replace_cursor_working_suffix(value: &str) -> String {
    let normalized = normalize_spaces(value);
    if let Some(index) = normalized.find("\u{23f3} Working") {
        format!("{}Working", normalized[..index].trim_end())
    } else {
        normalized
    }
}

fn collapse_signature_noise(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_digit() {
            let mut digits = String::from(ch);
            while let Some(next) = chars.peek().copied() {
                if next.is_ascii_digit() {
                    digits.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            let mut lookahead = chars.clone();
            let mut saw_space = false;
            while let Some(next) = lookahead.peek().copied() {
                if next.is_whitespace() {
                    saw_space = true;
                    lookahead.next();
                } else {
                    break;
                }
            }
            if lookahead.peek().copied() == Some('s') && saw_space {
                while let Some(next) = chars.peek().copied() {
                    if next.is_whitespace() {
                        chars.next();
                    } else {
                        break;
                    }
                }
                if chars.peek().copied() == Some('s') {
                    chars.next();
                    result.push_str("<elapsed>");
                    continue;
                }
            }
            result.push_str(&digits);
            continue;
        }
        if ch.is_whitespace()
            || ('\u{2800}'..='\u{28ff}').contains(&ch)
            || matches!(
                ch,
                '\u{00b7}'
                    | '\u{2022}'
                    | '\u{22c5}'
                    | '\u{25e6}'
                    | '\u{2733}'
                    | '*'
                    | '\u{25d0}'
                    | '\u{25d1}'
                    | '\u{25d2}'
                    | '\u{25d3}'
                    | '\u{2726}'
                    | '\u{25c7}'
                    | '\u{1f916}'
                    | '\u{1f514}'
            )
        {
            result.push(' ');
        } else {
            result.push(ch);
        }
    }
    normalize_spaces(&result)
}

fn normalize_activity_event(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| {
            matches!(
                *value,
                "launch"
                    | "resume"
                    | "wake"
                    | "escape"
                    | "agentDetected"
                    | "title"
                    | "bell"
                    | "terminalError"
                    | "terminalExited"
                    | "acknowledge"
            )
        })
        .map(str::to_string)
}

fn create_attention_event_id(now_ms_value: i64) -> String {
    format!("attn_{}", to_base36(now_ms_value))
}

fn to_base36(value: i64) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let negative = value < 0;
    let mut value = value.unsigned_abs();
    let mut digits = Vec::new();
    while value > 0 {
        let digit = (value % 36) as u8;
        digits.push(match digit {
            0..=9 => (b'0' + digit) as char,
            _ => (b'a' + digit - 10) as char,
        });
        value /= 36;
    }
    digits.reverse();
    let mut output = digits.into_iter().collect::<String>();
    if negative {
        output.insert(0, '-');
    }
    output
}

impl ActivityState {
    fn to_value(&self) -> Value {
        let mut output = Map::new();
        output.insert("activity".to_string(), Value::String(self.activity.clone()));
        insert_optional_string(&mut output, "agentName", self.agent_name.clone());
        insert_optional_string(
            &mut output,
            "attentionEventId",
            self.attention_event_id.clone(),
        );
        insert_optional_string(
            &mut output,
            "attentionSuppressedUntil",
            self.attention_suppressed_until.clone(),
        );
        if let Some(value) = self.has_seen_working {
            output.insert("hasSeenWorking".to_string(), Value::Bool(value));
        }
        if let Some(value) = self.is_acknowledged {
            output.insert("isAcknowledged".to_string(), Value::Bool(value));
        }
        insert_optional_string(&mut output, "lastChangedAt", self.last_changed_at.clone());
        insert_optional_string(
            &mut output,
            "lastMeaningfulActivityAt",
            self.last_meaningful_activity_at.clone(),
        );
        insert_optional_string(&mut output, "lastTitle", self.last_title.clone());
        insert_optional_string(
            &mut output,
            "lastTitleChangeAt",
            self.last_title_change_at.clone(),
        );
        insert_optional_string(
            &mut output,
            "suppressedUntil",
            self.suppressed_until.clone(),
        );
        insert_optional_string(&mut output, "workingSource", self.working_source.clone());
        insert_optional_string(
            &mut output,
            "workingStartedAt",
            self.working_started_at.clone(),
        );
        Value::Object(output)
    }
}

fn object_field(value: &Value, key: &str) -> Map<String, Value> {
    value
        .get(key)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn read_text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .and_then(normalize_text_str)
}

fn read_text_value(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .and_then(normalize_text_str)
}

fn read_text_from_map(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .and_then(normalize_text_str)
}

fn normalize_text_str(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn insert_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        map.insert(key.to_string(), Value::String(value));
    }
}

fn normalize_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse_iso_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp_millis())
}

pub fn iso_from_ms(value: i64) -> String {
    Utc.timestamp_millis_opt(value)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn transition(params: Value) -> Value {
        let session = json!({
            "agentId": params.get("agentId").and_then(Value::as_str).unwrap_or("codex"),
            "runtimeSettings": {
                "agentActivity": params.get("previous").cloned().unwrap_or_else(|| json!({ "activity": "idle" }))
            }
        });
        let mut map = params.as_object().cloned().unwrap_or_default();
        map.remove("previous");
        map.remove("agentId");
        compute_activity_update(&session, &map, None).activity
    }

    #[test]
    fn explicit_hook_activity_bypasses_launch_suppression_and_plain_title_downgrades() {
        let launched = transition(json!({
            "agentId": "codex",
            "event": "launch",
            "nowMs": 1780814845000_i64
        }));
        assert_eq!(launched.get("activity"), Some(&json!("idle")));
        assert_eq!(
            launched.get("suppressedUntil"),
            Some(&json!("2026-06-07T06:47:37.000Z"))
        );

        let working = transition(json!({
            "activity": "working",
            "agentId": "codex",
            "nowMs": 1780814850000_i64,
            "previous": launched
        }));
        assert_eq!(working.get("activity"), Some(&json!("working")));
        assert_eq!(working.get("workingSource"), Some(&json!("explicit")));

        let plain_title = transition(json!({
            "agentId": "codex",
            "event": "title",
            "nowMs": 1780814851000_i64,
            "previous": working,
            "title": "Monaco Ctrl+G Switch"
        }));
        assert_eq!(plain_title.get("activity"), Some(&json!("working")));
        assert_eq!(plain_title.get("workingSource"), Some(&json!("explicit")));
    }

    #[test]
    fn escape_suppresses_attention_without_clearing_working() {
        let working = transition(json!({
            "agentId": "codex",
            "event": "escape",
            "nowMs": 1781153171000_i64,
            "previous": {
                "activity": "working",
                "agentName": "codex",
                "hasSeenWorking": true,
                "isAcknowledged": false,
                "lastChangedAt": "2026-06-11T04:46:10.000Z",
                "workingSource": "explicit",
                "workingStartedAt": "2026-06-11T04:46:10.000Z"
            }
        }));
        assert_eq!(working.get("activity"), Some(&json!("working")));
        assert_eq!(
            working.get("attentionSuppressedUntil"),
            Some(&json!("2026-06-11T04:46:16.000Z"))
        );
        assert_eq!(working.get("workingSource"), Some(&json!("explicit")));
    }

    #[test]
    fn same_title_codex_spinner_stop_clears_explicit_hook_working() {
        let title_working = transition(json!({
            "agentId": "codex",
            "event": "title",
            "nowMs": 1780808007000_i64,
            "title": "\u{280f} Ghostex 4.0.0 Beta"
        }));
        assert_eq!(title_working.get("activity"), Some(&json!("working")));
        assert_eq!(title_working.get("workingSource"), Some(&json!("title")));

        let explicit_working = transition(json!({
            "activity": "working",
            "agentId": "codex",
            "nowMs": 1780808009000_i64,
            "previous": title_working
        }));
        let stopped_spinner = transition(json!({
            "agentId": "codex",
            "event": "title",
            "nowMs": 1780808455000_i64,
            "previous": explicit_working,
            "title": "Ghostex 4.0.0 Beta"
        }));
        assert_eq!(stopped_spinner.get("activity"), Some(&json!("attention")));
        assert_eq!(stopped_spinner.get("workingSource"), None);
    }

    #[test]
    fn stale_title_derived_working_projects_idle_without_rewriting_state() {
        let activity = json!({
            "activity": "working",
            "agentName": "codex",
            "hasSeenWorking": true,
            "isAcknowledged": false,
            "lastChangedAt": "2026-06-07T06:47:30.000Z",
            "lastTitle": "\u{280b} Implementing",
            "lastTitleChangeAt": "2026-06-07T06:47:30.000Z",
            "workingSource": "title",
            "workingStartedAt": "2026-06-07T06:47:30.000Z"
        });
        assert_eq!(
            agent_activity_stale_projection_delay_ms(Some(&activity), 1780814853000_i64),
            Some(2000)
        );

        let effective = effective_agent_activity_value(Some(&activity), "idle", 1780814856000_i64);
        assert_eq!(effective.get("activity"), Some(&json!("idle")));
        assert_eq!(effective.get("workingSource"), None);
        assert_eq!(activity.get("activity"), Some(&json!("working")));

        let malformed_title_working = json!({
            "activity": "working",
            "agentName": "codex",
            "hasSeenWorking": true,
            "isAcknowledged": true,
            "lastChangedAt": "2026-06-07T06:47:30.000Z",
            "workingSource": "title",
            "workingStartedAt": "2026-06-07T06:47:30.000Z"
        });
        assert_eq!(
            agent_activity_stale_projection_delay_ms(
                Some(&malformed_title_working),
                1780814853000_i64
            ),
            Some(0)
        );
        assert_eq!(
            effective_agent_activity_value(
                Some(&malformed_title_working),
                "idle",
                1780814853000_i64
            )
            .get("activity"),
            Some(&json!("idle"))
        );
    }

    #[test]
    fn short_working_blip_does_not_advance_meaningful_activity() {
        let working = transition(json!({
            "activity": "working",
            "agentId": "codex",
            "nowMs": 1780814845000_i64
        }));
        assert_eq!(working.get("activity"), Some(&json!("working")));
        assert_eq!(working.get("lastMeaningfulActivityAt"), None);

        let idle = transition(json!({
            "activity": "idle",
            "agentId": "codex",
            "nowMs": 1780814848000_i64,
            "previous": working
        }));
        assert_eq!(idle.get("activity"), Some(&json!("idle")));
        assert_eq!(idle.get("lastMeaningfulActivityAt"), None);
    }

    #[test]
    fn meaningful_working_stint_advances_clock_on_events_and_stop() {
        let working = transition(json!({
            "activity": "working",
            "agentId": "codex",
            "nowMs": 1780814845000_i64
        }));
        let held = transition(json!({
            "agentId": "codex",
            "event": "title",
            "nowMs": 1780814857000_i64,
            "previous": working,
            "title": "Monaco Ctrl+G Switch"
        }));
        assert_eq!(held.get("activity"), Some(&json!("working")));
        assert_eq!(
            held.get("lastMeaningfulActivityAt"),
            Some(&json!("2026-06-07T06:47:37.000Z"))
        );

        let idle = transition(json!({
            "activity": "idle",
            "agentId": "codex",
            "nowMs": 1780814860000_i64,
            "previous": held
        }));
        assert_eq!(idle.get("activity"), Some(&json!("idle")));
        assert_eq!(
            idle.get("lastMeaningfulActivityAt"),
            Some(&json!("2026-06-07T06:47:40.000Z"))
        );
    }

    #[test]
    fn attention_entry_advances_meaningful_clock() {
        let attention = transition(json!({
            "agentId": "codex",
            "event": "bell",
            "nowMs": 1780814845000_i64,
            "previous": {
                "activity": "working",
                "agentName": "codex",
                "hasSeenWorking": true,
                "workingSource": "explicit",
                "workingStartedAt": "2026-06-07T06:47:20.000Z"
            }
        }));
        assert_eq!(attention.get("activity"), Some(&json!("attention")));
        assert_eq!(
            attention.get("lastMeaningfulActivityAt"),
            Some(&json!("2026-06-07T06:47:25.000Z"))
        );
    }

    #[test]
    fn wake_reset_preserves_meaningful_clock_without_bump() {
        let woken = transition(json!({
            "agentId": "codex",
            "event": "wake",
            "nowMs": 1780814845000_i64,
            "previous": {
                "activity": "working",
                "agentName": "codex",
                "hasSeenWorking": true,
                "lastMeaningfulActivityAt": "2026-06-01T00:00:00.000Z",
                "workingSource": "explicit",
                "workingStartedAt": "2026-06-01T00:00:00.000Z"
            }
        }));
        assert_eq!(woken.get("activity"), Some(&json!("idle")));
        assert_eq!(
            woken.get("lastMeaningfulActivityAt"),
            Some(&json!("2026-06-01T00:00:00.000Z"))
        );
    }

    #[test]
    fn stale_title_stint_measures_end_from_last_title_change() {
        let idle = transition(json!({
            "agentId": "codex",
            "event": "title",
            "nowMs": 1780814865000_i64,
            "previous": {
                "activity": "working",
                "agentName": "codex",
                "hasSeenWorking": true,
                "isAcknowledged": true,
                "lastTitle": "\u{280b} Implementing",
                "lastTitleChangeAt": "2026-06-07T06:47:28.000Z",
                "workingSource": "title",
                "workingStartedAt": "2026-06-07T06:47:25.000Z"
            },
            "title": "Codex Ready"
        }));
        assert_eq!(idle.get("activity"), Some(&json!("idle")));
        assert_eq!(idle.get("lastMeaningfulActivityAt"), None);
    }

    #[test]
    fn meaningful_projection_and_refresh_delay_cross_threshold() {
        let activity = json!({
            "activity": "working",
            "agentName": "codex",
            "hasSeenWorking": true,
            "workingSource": "explicit",
            "workingStartedAt": "2026-06-07T06:47:25.000Z"
        });
        let started_ms = 1780814845000_i64;
        assert_eq!(
            meaningful_activity_at(Some(&activity), started_ms + 4_000),
            None
        );
        assert_eq!(
            meaningful_activity_at(Some(&activity), started_ms + 12_000),
            Some("2026-06-07T06:47:37.000Z".to_string())
        );
        assert_eq!(
            agent_activity_presentation_refresh_delay_ms(Some(&activity), started_ms + 4_000),
            Some(6_000)
        );
        assert_eq!(
            agent_activity_presentation_refresh_delay_ms(Some(&activity), started_ms + 12_000),
            None
        );
        assert_eq!(
            effective_working_started_at(Some(&activity), started_ms + 4_000),
            Some("2026-06-07T06:47:25.000Z".to_string())
        );
    }

    #[test]
    fn claude_idle_terminal_titles_settle_without_attention() {
        let title_working = transition(json!({
            "agentId": "claude",
            "event": "title",
            "nowMs": 1781199780000_i64,
            "title": "\u{2736} Claude Code"
        }));
        let explicit_working = transition(json!({
            "activity": "working",
            "agentId": "claude",
            "nowMs": 1781199781000_i64,
            "previous": title_working
        }));
        let settled = transition(json!({
            "agentId": "claude",
            "event": "title",
            "nowMs": 1781199788000_i64,
            "previous": explicit_working,
            "title": "\u{2733} Claude Code"
        }));
        assert_eq!(settled.get("activity"), Some(&json!("idle")));
        assert_eq!(settled.get("attentionEventId"), None);
    }
}
