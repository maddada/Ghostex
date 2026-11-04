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
fn escape_demotes_explicit_working_to_title_derived_without_clearing_it() {
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
    assert_eq!(working.get("workingSource"), Some(&json!("title")));
    assert_eq!(
        working.get("lastTitleChangeAt"),
        Some(&json!("2026-06-11T04:46:11.000Z"))
    );
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
        agent_activity_stale_projection_delay_ms(Some(&malformed_title_working), 1780814853000_i64),
        Some(0)
    );
    assert_eq!(
        effective_agent_activity_value(Some(&malformed_title_working), "idle", 1780814853000_i64)
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

/// Claude's agents view titled the terminal this way on 2026-10-16; it is not a turn.
#[test]
fn claude_agents_view_title_gives_no_status() {
    for title in ["2 awaiting input · claude agents", "claude agents"] {
        assert!(
            title_classify::classify_terminal_title_status(Some(title), Some("claude")).is_none(),
            "{title}"
        );
    }
    assert!(title_classify::classify_terminal_title_status(
        Some("· Fixing the tests"),
        Some("claude")
    )
    .is_some());
}

#[test]
fn deepseek_harness_titles_carry_no_status() {
    for title in ["⠂ 🐋 Run the tests", "✦ 🐋 Run the tests"] {
        assert!(title_classify::classify_terminal_title_status(Some(title), Some("dsh")).is_none());
    }
    assert!(title_classify::titles_carry_no_status(Some("dsh")));
}
