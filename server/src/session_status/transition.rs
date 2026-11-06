use super::*;

pub(super) fn apply_agent_activity_transition(input: ActivityInput) -> ActivityState {
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
        next.attention_source = None;
        next.is_acknowledged = Some(true);
        next.last_changed_at = Some(input.now_iso.clone());
        // The acknowledged stint's start no longer counts for anything (`ended_stint_start` in
        // model.rs); the next working stint sets a new one.
        next.working_started_at = None;
        return next;
    }

    if let Some(activity) = input.program_status_activity.as_deref() {
        return program_status_transition(&input, &previous, activity);
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
            next.attention_source = None;
            next.last_changed_at = Some(input.now_iso.clone());
            next.working_source = None;
            next.working_started_at = None;
        } else if previous_activity == "working"
            && previous.working_source.as_deref() == Some("explicit")
        {
            /*
            CDXC:SessionStatus 2026-09-04 WHY:
            Escape is how a turn gets interrupted, and Claude Code fires no Stop
            hook for an interrupted turn (it has no interrupt hook either), so a
            hook-backed working claim made by UserPromptSubmit or PreToolUse
            survived forever after the user pressed Stop: a turn cut off after a
            second never showed a spinner title, and the same-title stop rules
            above need one, so the idle title was ignored for hours (observed
            2026-09-03 and 2026-09-04). An Escape therefore ends the hook's
            claim and hands the state to the title: the stale-title window
            closes it to idle if the agent really stopped, the spinner heartbeat
            keeps it working if Escape stopped nothing, and the next hook makes
            it explicit again. Sent by the chat-view interrupt endpoint and the
            terminal-pane Escape key alike.
            */
            next.working_source = Some("title".to_string());
            next.last_title_change_at = Some(input.now_iso.clone());
            if next.last_changed_at.is_none() {
                next.last_changed_at = Some(input.now_iso.clone());
            }
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

    /*
    CDXC:SessionStatus 2026-10-16 WHY:
    For agents whose titles carry no status, a title event cannot end an attention. Run through the rules below it fell to the closing idle: ZCode 3.15 repaints its spinner title every 120ms while it waits at its permission picker, so the attention its PermissionRequest hook set lasted half a second (observed live 2026-10-16, throwaway G2jmr). Their hooks end it. Other states keep the rules below, which settle a turn interrupted with Escape on the next title.
    */
    if input.event.as_deref() == Some("title")
        && !has_explicit_activity
        && previous.activity == "attention"
        && title_signal.is_none()
        && titles_carry_no_status(
            normalize_status_agent_name(input.agent_id.as_deref())
                .or_else(|| previous.agent_name.clone())
                .as_deref(),
        )
    {
        let mut next = previous.clone();
        next.last_title = title_transition.last_title;
        next.last_title_change_at = title_transition.last_title_change_at;
        return next;
    }

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

    /*
    A finished turn stays in attention until the user acknowledges it or the
    agent starts working again. The idle title Claude paints right after its
    Stop hook is only the screen catching up with the hook, not a new signal,
    so it is recorded but changes nothing. A working spinner still falls
    through and moves the session to working.
    */
    if input.event.as_deref() == Some("title")
        && !has_explicit_activity
        && previous.is_unacknowledged_turn_complete_attention()
        && title_signal.as_ref().map(|signal| signal.state.as_str()) != Some("working")
    {
        let mut next = previous.clone();
        next.last_title = title_transition.last_title;
        next.last_title_change_at = title_transition.last_title_change_at;
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
            // A Stop landing on a still-standing prompt attention is the same
            // dot with a new meaning: the turn is over, so the queue may
            // deliver. The event id stays, so nothing rings twice.
            if input.attention_source.is_some()
                && previous.attention_source != input.attention_source
            {
                let mut next = previous.clone();
                next.attention_source = input.attention_source.clone();
                return next;
            }
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
            attention_source: input.attention_source.clone(),
            has_seen_working: Some(true),
            is_acknowledged: Some(false),
            last_changed_at: Some(input.now_iso.clone()),
            last_title: title_transition.last_title,
            last_title_change_at: title_transition.last_title_change_at,
            working_started_at: previous.ended_stint_start(),
            ..ActivityState::default()
        };
    }

    let mut next = previous.clone();
    next.activity = "idle".to_string();
    next.agent_name = agent_name;
    next.attention_source = None;
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
CDXC:AgentScreenDetection 2026-07-29-12:00:
The meaningful-activity clock is applied once, after every transition branch,
so early returns and from-scratch state rebuilds (launch/resume/wake resets,
working entry, agent switches) cannot drop or double-apply it. It advances on
attention entry, on any event observed while a working stint has already
lasted MIN_MEANINGFUL_WORKING_DURATION_MS, and when a qualifying stint ends.
Lifecycle reset events never advance it: a wake that clears frozen stale
working state is not user-visible activity.
*/
pub(super) fn apply_meaningful_activity_clock(
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

pub(super) fn states_equal_ignoring_meaningful_clock(
    left: &ActivityState,
    right: &ActivityState,
) -> bool {
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
CDXC:AgentScreenDetection 2026-07-29-12:00:
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
        .then_some(effective.working_started_at)
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
