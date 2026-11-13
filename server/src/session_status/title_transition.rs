use super::*;

#[derive(Default)]
pub(super) struct TitleTransition {
    pub(super) last_title: Option<String>,
    pub(super) last_title_change_at: Option<String>,
}

pub(super) fn is_trusted_spinner_stop_title(
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
    /*
    CDXC:SessionStatus 2026-09-03:
    Cursor Agent writes "<chat> - ✅ Ready" only once its loop has ended, so
    the Ready title that replaces the same chat's "<chat> - ⏳ Working ···"
    is as authoritative as its stop hook. The hook post can be lost (the
    notify helper gives gxserver 1.5s and drops the event on timeout), and
    without this rule an explicit hook-driven working state then survives
    forever because no other Cursor title signal is allowed to end it.
    */
    if agent_name == Some("cursor")
        && is_cursor_ready_title(&title)
        && previous
            .last_title
            .as_deref()
            .map(cursor_title_chat_name)
            .is_some_and(|previous_chat| previous_chat == cursor_title_chat_name(&title))
    {
        return true;
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

pub(super) fn is_trusted_settled_title_stop(
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

pub(super) fn resolve_title_transition(
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

pub(super) fn is_title_derived_working_stale(
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

pub(super) fn state_for_stale_title_working(
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

pub(super) fn state_for_suppressed_attention(
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
    next.attention_source = None;
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

pub(super) fn effective_agent_activity_state(
    state: ActivityState,
    now_ms_value: i64,
) -> ActivityState {
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

pub(super) fn normalize_agent_activity_state(
    value: Option<&Value>,
    fallback: &str,
) -> ActivityState {
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
        attention_source: read_text_from_map(&record, "attentionSource")
            .filter(|value| value == TURN_COMPLETE_ATTENTION_SOURCE),
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
        background_only_since: read_text_from_map(&record, "backgroundOnlySince"),
    }
}

pub(super) fn activity_from_event(event: Option<&str>) -> Option<String> {
    match event {
        Some("bell" | "terminalError") => Some("attention".to_string()),
        Some("terminalExited") => Some("idle".to_string()),
        _ => None,
    }
}

pub(super) fn activity_from_title_signal(
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

pub(super) fn active_attention_suppressed_until(
    state: &ActivityState,
    now_ms_value: i64,
) -> Option<String> {
    state
        .attention_suppressed_until
        .as_deref()
        .and_then(parse_iso_ms)
        .filter(|suppressed_until| now_ms_value < *suppressed_until)
        .and_then(|_| state.attention_suppressed_until.clone())
}

pub(super) fn active_attention_suppressed_until_ms(
    state: &ActivityState,
    now_ms_value: i64,
) -> Option<i64> {
    state
        .attention_suppressed_until
        .as_deref()
        .and_then(parse_iso_ms)
        .filter(|suppressed_until| now_ms_value < *suppressed_until)
}
