use serde_json::{json, Value};

use super::*;
use crate::{domain::DomainRepository, session_status::compute_activity_update};

#[test]
fn activity_escape_suppresses_attention_without_logging_titles() {
    let session = json!({
        "agentId": "codex",
        "lastActiveAt": "2026-06-16T09:59:00.000Z",
        "runtimeSettings": {
            "agentActivity": {
                "activity": "attention",
                "agentName": "codex",
                "attentionEventId": "attn_old",
                "hasSeenWorking": true,
                "isAcknowledged": false,
                "lastChangedAt": "2026-06-16T10:00:00.000Z"
            }
        }
    });
    let update = compute_activity_update(
        &session,
        json!({ "event": "escape", "nowMs": 1781604000000_i64 })
            .as_object()
            .expect("params"),
        None,
    );
    assert_eq!(update.previous_activity, "attention");
    assert_eq!(
        update.activity.get("activity").and_then(Value::as_str),
        Some("idle")
    );
    assert!(update.activity.get("attentionSuppressedUntil").is_some());
    assert!(update.activity.get("attentionEventId").is_none());
}

#[test]
fn hook_activity_normalizes_provider_events() {
    assert_eq!(
        normalize_agent_hook_activity(
            None,
            Some(&json!("UserPromptSubmit")),
            Some(&json!("Claude Code"))
        ),
        Some("working".to_string())
    );
    assert_eq!(
        normalize_agent_hook_activity(None, Some(&json!("Stop")), Some(&json!("Claude Code"))),
        Some("attention".to_string())
    );
    assert_eq!(
        normalize_agent_hook_activity(
            None,
            Some(&json!("StopFailure")),
            Some(&json!("Claude Code"))
        ),
        Some("idle".to_string())
    );
    assert_eq!(
        normalize_agent_hook_activity(
            Some(&json!("idle")),
            Some(&json!("Stop")),
            Some(&json!("Codex"))
        ),
        Some("attention".to_string())
    );
    assert_eq!(
        normalize_agent_hook_activity(
            Some(&json!("attention")),
            Some(&json!("SessionEnd")),
            Some(&json!("Codex"))
        ),
        Some("idle".to_string())
    );
    // Copilot's Notification needs its payload to say why it fired (agent_hooks/event_mapping.rs).
    assert_eq!(
        normalize_agent_hook_activity(
            Some(&json!("attention")),
            Some(&json!("Notification")),
            Some(&json!("GitHub Copilot"))
        ),
        None
    );
    assert_eq!(
        normalize_agent_hook_activity(None, Some(&json!("pre_approval_request")), None),
        Some("attention".to_string())
    );
    assert_eq!(
        normalize_agent_hook_activity(None, Some(&json!("session.updated")), None),
        Some("attention".to_string())
    );
    assert_eq!(
        normalize_agent_hook_activity(None, Some(&json!("on_session_start")), None),
        Some("working".to_string())
    );
    assert_eq!(
        normalize_agent_hook_activity(None, Some(&json!("on_session_finalize")), None),
        Some("idle".to_string())
    );
    assert_eq!(
        normalize_agent_hook_activity(None, Some(&json!("session_shutdown")), None),
        Some("idle".to_string())
    );
}

#[test]
fn codex_stop_hook_enters_attention_from_working() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "019e7af5-c610-7f62-a129-db7bb510b48d";
    let (lifecycle, session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    runtime_settings.insert(
        "agentActivity".to_string(),
        json!({
            "activity": "working",
            "agentName": "codex",
            "hasSeenWorking": true,
            "isAcknowledged": false,
            "lastChangedAt": "2026-08-08T01:00:00.000Z",
            "lastMeaningfulActivityAt": "2026-08-08T01:00:00.000Z",
            "workingSource": "hook",
            "workingStartedAt": "2026-08-08T01:00:00.000Z"
        }),
    );
    let mut update = lifecycle_update(&lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&update).expect("working session");

    let result = ingest_agent_hook_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "agentSessionId": agent_session_id,
            "eventName": "Stop",
            "status": "idle",
            "statusUpdatedAt": "2026-08-08T01:00:05.000Z"
        })
        .as_object()
        .expect("hook params"),
        temp.path(),
    )
    .expect("hook result");

    assert_eq!(result.get("previousActivity"), Some(&json!("working")));
    assert_eq!(result.get("enteredAttention"), Some(&json!(true)));
    let activity = result
        .get("activity")
        .and_then(Value::as_object)
        .expect("activity");
    assert_eq!(activity.get("activity"), Some(&json!("attention")));
    assert!(activity
        .get("attentionEventId")
        .and_then(Value::as_str)
        .is_some());
}

/*
CDXC:SessionIdentity 2026-08-02:
The Session Chat successor detector asks this predicate which sessions could
still be tailing an agent conversation. The registry keeps every session ever
created (3487 stopped rows on the machine the chat-identity bug was debugged
on), and stopped rows still carry the agentSessionIds of conversations that
were later continued. Counting those as owners silently blocked every
legitimate re-binding, so the stopped cases are pinned here.
*/
#[test]
fn stopped_sessions_are_not_identity_owners() {
    assert!(is_active_identity_owner(
        &json!({ "lifecycleState": "running" })
    ));
    assert!(is_active_identity_owner(
        &json!({ "lifecycleState": "sleeping" })
    ));
    assert!(!is_active_identity_owner(&json!({
        "lifecycleState": "stopped",
        "providerState": { "lifecycleState": "missing" }
    })));
    assert!(!is_active_identity_owner(&json!({
        "lifecycleState": "stopped",
        "providerState": { "lifecycleState": "exists" }
    })));
    // Not stopped and the provider is still alive ⇒ still an owner.
    assert!(is_active_identity_owner(&json!({
        "lifecycleState": "unknown",
        "providerState": { "lifecycleState": "exists" }
    })));
    assert!(!is_active_identity_owner(&json!({
        "lifecycleState": "unknown",
        "providerState": { "lifecycleState": "missing" }
    })));
}

#[test]
fn transcript_successor_identity_write_is_compare_and_set() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Successor Identity Project", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id")
        .to_string();
    let session = repository
        .create_session(
            json!({
                "agentId": "claude",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "claude",
                    "agentSessionId": "stale-session",
                    "agentSessionPath": "/Users/test/.claude/projects/demo/stale-session.jsonl"
                },
                "title": "Claude Session"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let session_id = session
        .get("sessionId")
        .and_then(Value::as_str)
        .expect("session id")
        .to_string();

    // A hook that landed after the follower read the identity must win.
    assert!(!apply_transcript_successor_session_identity(
        &repository,
        &project_id,
        &session_id,
        Some("some-other-session"),
        "successor-session",
        "/Users/test/.claude/projects/demo/successor-session.jsonl",
    )
    .expect("stale expectation refused"));

    assert!(apply_transcript_successor_session_identity(
        &repository,
        &project_id,
        &session_id,
        Some("stale-session"),
        "successor-session",
        "/Users/test/.claude/projects/demo/successor-session.jsonl",
    )
    .expect("successor identity applied"));

    let stored = repository
        .get_session(&project_id, &session_id)
        .expect("get session")
        .expect("session row");
    let runtime_settings = object_field(&stored, "runtimeSettings");
    assert_eq!(
        runtime_settings.get("agentSessionId"),
        Some(&json!("successor-session"))
    );
    assert_eq!(
        runtime_settings.get("agentSessionPath"),
        Some(&json!(
            "/Users/test/.claude/projects/demo/successor-session.jsonl"
        ))
    );
    assert_eq!(stored.get("agentId"), Some(&json!("claude")));

    // Re-running the same adoption is a no-op, not a churn write.
    assert!(!apply_transcript_successor_session_identity(
        &repository,
        &project_id,
        &session_id,
        Some("successor-session"),
        "successor-session",
        "/Users/test/.claude/projects/demo/successor-session.jsonl",
    )
    .expect("idempotent adoption"));
}
