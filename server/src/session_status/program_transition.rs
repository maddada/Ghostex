use super::*;

/// CDXC:SessionStatus 2026-10-14 WHY:
/// A program's own report is the truth about it, so none of the detection guards apply: no launch suppression, no Escape suppression, no minimum working time before attention, no title stale-window. Working is explicit (the projection never expires it), attention gets a fresh event id only when the session enters it (moving between waiting-for-you and finished keeps the id, so nothing rings twice), and the attention source follows the report: a finished or failed turn is `turnComplete`, a blocked program is not.
pub(super) fn program_status_transition(
    input: &ActivityInput,
    previous: &ActivityState,
    activity: &str,
) -> ActivityState {
    let previous_activity = previous.activity.as_str();
    match activity {
        "working" => ActivityState {
            activity: "working".to_string(),
            agent_name: previous.agent_name.clone(),
            has_seen_working: Some(true),
            is_acknowledged: Some(false),
            last_changed_at: Some(if previous_activity == "working" {
                previous
                    .last_changed_at
                    .clone()
                    .unwrap_or_else(|| input.now_iso.clone())
            } else {
                input.now_iso.clone()
            }),
            last_meaningful_activity_at: previous.last_meaningful_activity_at.clone(),
            working_source: Some("explicit".to_string()),
            working_started_at: Some(if previous_activity == "working" {
                previous
                    .working_started_at
                    .clone()
                    .unwrap_or_else(|| input.now_iso.clone())
            } else {
                input.now_iso.clone()
            }),
            ..ActivityState::default()
        },
        "attention" => {
            if previous_activity == "attention" {
                let mut next = previous.clone();
                next.attention_source = input.attention_source.clone();
                return next;
            }
            ActivityState {
                activity: "attention".to_string(),
                agent_name: previous.agent_name.clone(),
                attention_event_id: Some(
                    input
                        .reuse_attention_event_id
                        .clone()
                        .unwrap_or_else(|| create_attention_event_id(input.now_ms)),
                ),
                attention_source: input.attention_source.clone(),
                has_seen_working: Some(true),
                is_acknowledged: Some(false),
                last_changed_at: Some(input.now_iso.clone()),
                last_meaningful_activity_at: previous.last_meaningful_activity_at.clone(),
                working_started_at: previous.ended_stint_start(),
                ..ActivityState::default()
            }
        }
        _ => {
            let mut next = previous.clone();
            next.activity = "idle".to_string();
            next.attention_event_id = None;
            next.attention_source = None;
            next.attention_suppressed_until = None;
            next.suppressed_until = None;
            next.working_source = None;
            next.working_started_at = None;
            if previous_activity != "idle" {
                next.last_changed_at = Some(input.now_iso.clone());
            }
            next
        }
    }
}
