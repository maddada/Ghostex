use super::*;

/*
CDXC:AgentScreenDetection 2026-07-29-12:00:
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
pub(super) const TITLE_ACTIVITY_WINDOW_MS: i64 = 1_000;
pub(super) const TITLE_ACTIVITY_HEARTBEAT_MS: i64 = 2_000;
pub(super) const SLOW_SPINNER_ACTIVITY_WINDOW_MS: i64 = 5_000;
/*
CDXC:Notifications 2026-09-04 WHY:
`attentionSource` distinguishes the attention a hook's Stop enters (a finished
turn waiting to be read) from every other attention (a permission, approval, or
question waiting to be answered, a bell, an "Action Required" title). The chat
prompt queue delivers into the first and must never deliver into the second,
and a passive idle title must not end the first: Claude paints its idle title
about a second after its Stop hook, which otherwise settled the fresh attention
straight back to idle. SEE-ALSO: server/src/agents/activity.rs (sets it),
server/src/session_chat_queue_runtime/scheduler.rs (reads it).
*/
pub const TURN_COMPLETE_ATTENTION_SOURCE: &str = "turnComplete";

pub(super) const CLAUDE_CODE_IDLE_MARKERS: &[char] = &['\u{2733}', '*'];
pub(super) const CLAUDE_CODE_WORKING_MARKERS: &[char] = &[
    '\u{2810}', '\u{2802}', '\u{00b7}', '\u{2736}', '\u{273b}', '\u{273d}', '\u{2738}', '\u{2739}',
    '\u{273a}', '\u{2737}', '\u{2734}', '\u{25d0}', '\u{25d1}', '\u{25d2}', '\u{25d3}',
];
pub(super) const CODEX_WORKING_MARKERS: &[char] = &[
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
pub(super) struct ActivityState {
    pub(super) activity: String,
    pub(super) agent_name: Option<String>,
    pub(super) attention_event_id: Option<String>,
    pub(super) attention_source: Option<String>,
    pub(super) attention_suppressed_until: Option<String>,
    pub(super) has_seen_working: Option<bool>,
    pub(super) is_acknowledged: Option<bool>,
    pub(super) last_changed_at: Option<String>,
    pub(super) last_meaningful_activity_at: Option<String>,
    pub(super) last_title: Option<String>,
    pub(super) last_title_change_at: Option<String>,
    pub(super) suppressed_until: Option<String>,
    pub(super) working_source: Option<String>,
    pub(super) working_started_at: Option<String>,
}

impl ActivityState {
    pub(super) fn is_unacknowledged_turn_complete_attention(&self) -> bool {
        self.activity == "attention"
            && self.is_acknowledged != Some(true)
            && self.attention_source.as_deref() == Some(TURN_COMPLETE_ATTENTION_SOURCE)
    }
}

#[derive(Clone, Debug)]
pub(super) struct TitleStatusSignal {
    pub(super) agent_name: String,
    pub(super) state: String,
}

pub(super) struct ActivityInput {
    pub(super) activity: Option<String>,
    pub(super) agent_id: Option<String>,
    pub(super) attention_source: Option<String>,
    pub(super) event: Option<String>,
    pub(super) now_iso: String,
    pub(super) now_ms: i64,
    pub(super) previous: ActivityState,
    pub(super) settled_title: Option<String>,
    pub(super) title: Option<String>,
    /// Activity derived from the session's OSC 7501 records (`programStatusActivity`).
    pub(super) program_status_activity: Option<String>,
    /// The event id of an attention the program's same report already
    /// announced (`reuseAttentionEventId`), so re-applying it does not notify again.
    pub(super) reuse_attention_event_id: Option<String>,
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
    let program_status_activity = read_text(params, "programStatusActivity")
        .filter(|value| matches!(value.as_str(), "idle" | "working" | "attention"));
    // An Escape says the user stopped this turn, so its idle is not a finished turn.
    if event.as_deref() == Some("escape") {
        if let (Some(project_id), Some(session_id)) = (
            read_text_value(session, "projectId"),
            read_text_value(session, "sessionId"),
        ) {
            crate::program_status::note_program_status_interrupt(&project_id, &session_id);
        }
    }
    // Restoring detection's own view when a program stops driving the session
    // (program_status/ingest.rs `hand_back_to_detection`).
    if let Some(restored) = params.get("restoreDetectionActivity") {
        let next = normalize_agent_activity_state(Some(restored), "idle");
        return activity_update_from(session, previous_state, next, now_ms_value);
    }
    let run_detection = |previous: ActivityState| -> ActivityState {
        let next = apply_agent_activity_transition(ActivityInput {
            activity: params
                .get("activity")
                .and_then(Value::as_str)
                .filter(|value| matches!(*value, "idle" | "working" | "attention"))
                .map(str::to_string),
            agent_id: read_text(params, "agentName")
                .or_else(|| session_status_agent_family(session))
                .or_else(|| previous.agent_name.clone()),
            attention_source: read_text(params, "attentionSource")
                .filter(|value| value == TURN_COMPLETE_ATTENTION_SOURCE),
            event: event.clone(),
            now_iso: iso_from_ms(now_ms_value),
            now_ms: now_ms_value,
            previous: previous.clone(),
            settled_title: read_text(params, "settledTitle"),
            title: read_text(params, "title"),
            program_status_activity: program_status_activity.clone(),
            reuse_attention_event_id: read_text(params, "reuseAttentionEventId"),
        });
        /*
        Sessions that predate the meaningful-activity clock seed it from their
        durable lastActiveAt so recency sorting stays stable across the migration,
        but only when the transition already produced a persistable change: a
        seed-only difference must never turn a no-op event into a state rewrite.
        */
        let seed_recency = (previous.last_meaningful_activity_at.is_none()
            && !states_equal_ignoring_meaningful_clock(&previous, &next))
        .then(|| read_text_value(session, "lastActiveAt"))
        .flatten();
        apply_meaningful_activity_clock(
            &previous,
            next,
            seed_recency,
            event.as_deref(),
            now_ms_value,
        )
    };
    /*
    CDXC:SessionStatus 2026-10-15 WHY:
    While a program's report joins a session's status (while its reporter runs), detection keeps its own view of the session as a shadow state: every hook, title, bell, Escape and acknowledgement moves the shadow exactly as it would move the session, while the session shows the combined result. When the program stops joining the session (it exits or clears, its watch ends) the session takes the shadow, never a forced idle: forcing idle on a switch to Detection dropped a Claude thread that was mid-command to idle, Ghostex reported its turn finished, and the auto-sleep sweep slept it (its last activity was older than the 10-minute idle limit), killing the command.
    SEE-ALSO: server/src/program_status/ingest.rs (`hand_back_to_detection`, the shadow's start).
    */
    let in_charge = program_status_activity.is_none()
        && crate::program_status::program_status_combined_state(session).is_some();
    let mut detection_view: Option<ActivityState> = None;
    if in_charge {
        let shadow_previous = crate::program_status::program_status_detection_shadow(session)
            .map(|shadow| normalize_agent_activity_state(Some(&shadow), "idle"))
            .unwrap_or_else(|| previous_state.clone());
        let shadow_next = run_detection(shadow_previous);
        crate::program_status::set_program_status_detection_shadow(session, shadow_next.to_value());
        detection_view = Some(shadow_next);
    }
    /*
    CDXC:SessionStatus 2026-10-15 DECISION:
    User (Q120): under Combined, detection stays in charge and the program's report is one more signal; in a disagreement "the more urgent signal wins": while the program says blocked, detection cannot take the session out of attention (the program knows it waits on the user), and while the program says working, done or error, detection cannot set it idle (a finished turn stays until the user sees it; found when Claude's Notification hook cleared a reported done). Every other disagreement goes to detection's own view (its shadow), which keeps what only it knows (subagent fleets and compaction, applied when the session is presented). The user's acknowledgement and Escape always pass. Detection's own result is recorded for the mismatch log, which keeps only disagreements that outlast its settle window (program_status/mismatch.rs).
    */
    if program_status_activity.is_none()
        && !matches!(event.as_deref(), Some("acknowledge" | "escape"))
    {
        if let (Some(program_state), Some(detected_state)) = (
            crate::program_status::program_status_combined_state(session),
            detection_view.as_ref(),
        ) {
            let detected = detected_state.activity.as_str();
            let title = read_text(params, "title")
                .or_else(|| read_text(params, "rawTitle"))
                .map(|title| title.chars().take(80).collect::<String>());
            crate::program_status::observe_detection_opinion(
                session,
                detected,
                serde_json::json!({
                    "kind": event.clone().or_else(|| params.get("activity").map(|_| "activity".to_string())),
                    "hookEvent": read_text(params, "eventName"),
                    "title": title,
                }),
            );
            if crate::program_status::combined_program_wins(program_state, detected) {
                return ActivityUpdate {
                    activity: previous_state.to_value(),
                    entered_attention: false,
                    last_active_at: read_text_value(session, "lastActiveAt"),
                    previous_activity,
                };
            }
            return activity_update_from(
                session,
                previous_state,
                detected_state.clone(),
                now_ms_value,
            );
        }
    }
    let activity = run_detection(previous_state.clone());
    activity_update_from(session, previous_state, activity, now_ms_value)
}

/// The update that moves the session from `previous` to `next`.
fn activity_update_from(
    session: &Value,
    previous: ActivityState,
    next: ActivityState,
    now_ms_value: i64,
) -> ActivityUpdate {
    let previous_activity = previous.activity;
    let next_activity = next.activity.as_str();
    let last_active_at = if matches!(next_activity, "working" | "attention") {
        next.last_changed_at
            .clone()
            .or_else(|| Some(iso_from_ms(now_ms_value)))
    } else {
        read_text_value(session, "lastActiveAt")
    };
    ActivityUpdate {
        entered_attention: previous_activity != "attention" && next_activity == "attention",
        activity: next.to_value(),
        last_active_at,
        previous_activity,
    }
}

pub fn normalize_agent_activity_value(value: Option<&Value>, fallback: &str) -> Value {
    normalize_agent_activity_state(value, fallback).to_value()
}

/// Whether the stored activity is an unacknowledged attention entered by a
/// hook's Stop: a finished turn waiting to be read, not a prompt to answer.
pub fn is_turn_complete_attention(value: Option<&Value>) -> bool {
    normalize_agent_activity_state(value, "idle").is_unacknowledged_turn_complete_attention()
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

pub(super) fn agent_activity_stale_projection_delay_ms(
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
            "attentionSource",
            self.attention_source.clone(),
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

/// The session's agent id, or for a `custom-…` agent the CLI family its launch icon declares, so
/// a custom agent's titles are read by its own family's rules.
fn session_status_agent_family(session: &Value) -> Option<String> {
    let agent_id = read_text_value(session, "agentId")?;
    if !agent_id.starts_with("custom-") {
        return Some(agent_id);
    }
    read_text_from_map(&object_field(session, "launchSettings"), "icon")
        .and_then(|icon| crate::agents::default_agent_icon_to_id(&icon).map(str::to_string))
        .or(Some(agent_id))
}

pub(super) fn read_text_from_map(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .and_then(normalize_text_str)
}

pub(super) fn normalize_text_str(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn insert_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        map.insert(key.to_string(), Value::String(value));
    }
}

pub(super) fn normalize_spaces(value: &str) -> String {
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

pub(super) fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}
