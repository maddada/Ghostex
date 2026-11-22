use serde_json::{json, Value};

use super::*;
use crate::domain::DomainRepository;

#[test]
fn agent_hook_rejects_cross_agent_metadata_for_stored_pi_session_without_launch_lock() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let (lifecycle, before) = create_pi_agent_session_without_launch_lock(&repository);
    assert_eq!(
        before
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("launchAgentId")),
        None
    );

    let result = ingest_agent_hook_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "droid",
            "agentSessionId": "d7f1ca76-435b-4102-acdb-e3e786cd72a9",
            "agentSessionPath": "/tmp/.factory/sessions/thread.jsonl",
            "eventName": "Stop",
            "firstUserMessage": "private prompt text",
            "projectId": lifecycle.project_id.clone(),
            "rawEventName": "Stop",
            "sessionId": lifecycle.session_id.clone(),
            "status": "attention",
            "statusUpdatedAt": "2026-06-24T00:08:05.000Z",
            "title": "Wrong Droid Thread"
        })
        .as_object()
        .expect("hook params"),
        temp.path(),
    )
    .expect("hook result");

    assert_eq!(result.get("changed"), Some(&json!(false)));
    assert_eq!(
        result.get("reason"),
        Some(&json!("agent-hook-agent-mismatch"))
    );
    let response_session = result.get("session").expect("response session");
    assert_eq!(response_session.get("agentId"), Some(&json!("pi")));
    let runtime_settings = response_session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .expect("runtime settings");
    assert_eq!(runtime_settings.get("agentName"), Some(&json!("pi")));
    assert_eq!(runtime_settings.get("agentSessionId"), None);
    assert_eq!(runtime_settings.get("firstUserMessage"), None);
    let stored = repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)
        .expect("read stored")
        .expect("stored session");
    assert_eq!(stored.get("updatedAt"), before.get("updatedAt"));
    assert_eq!(stored.get("title"), before.get("title"));
    assert_eq!(stored.get("agentId"), Some(&json!("pi")));
    let stored_runtime_settings = stored
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .expect("stored runtime settings");
    assert_eq!(stored_runtime_settings.get("agentName"), Some(&json!("pi")));
    assert_eq!(stored_runtime_settings.get("agentSessionId"), None);
    assert_eq!(stored_runtime_settings.get("firstUserMessage"), None);
}

#[test]
fn agent_hook_rejects_passive_identity_conflict_before_activity_prompt_and_title() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let current_codex_session_id = "019e7af5-c610-7f62-a129-db7bb510b48d";
    let incoming_codex_session_id = "019e7c39-7ba7-7ac3-b79c-02757e299516";
    let (lifecycle, session) =
        create_codex_agent_session(&repository, current_codex_session_id, temp.path());
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    runtime_settings.insert(
        "agentActivity".to_string(),
        json!({ "activity": "idle", "isAcknowledged": true }),
    );
    runtime_settings.insert("titleSource".to_string(), json!("terminal-auto"));
    let mut update = lifecycle_update(&lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    update.insert("title".to_string(), json!("Target Codex Thread"));
    repository
        .update_session(&update)
        .expect("prepare target session");

    let result = ingest_agent_hook_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "agentSessionId": incoming_codex_session_id,
            "eventName": "Stop",
            "firstUserMessage": "private prompt text",
            "projectId": lifecycle.project_id.clone(),
            "rawEventName": "Stop",
            "sessionId": lifecycle.session_id.clone(),
            "status": "attention",
            "statusUpdatedAt": "2026-06-09T18:08:19.857Z",
            "title": "Wrong Codex Thread"
        })
        .as_object()
        .expect("hook params"),
        temp.path(),
    )
    .expect("hook result");

    assert_eq!(result.get("changed"), Some(&json!(false)));
    assert_eq!(
        result.get("reason"),
        Some(&json!("passive-session-identity-conflict"))
    );
    assert_eq!(
        result
            .get("activity")
            .and_then(|activity| activity.get("activity")),
        Some(&json!("idle"))
    );
    assert!(result.get("identityConflict").is_some());
    let response_session = result.get("session").expect("response session");
    assert_eq!(
        response_session.get("title"),
        Some(&json!("Target Codex Thread"))
    );
    assert_eq!(
        response_session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("agentSessionId")),
        Some(&json!(current_codex_session_id))
    );
    assert_eq!(
        response_session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("firstUserMessage")),
        None
    );
}

#[test]
fn agent_hook_unchanged_activity_reports_unchanged_without_rewriting_state() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    let (lifecycle, session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());
    let activity_at = "2026-06-09T18:08:19.857Z";
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    runtime_settings.insert(
        "agentActivity".to_string(),
        json!({
            "activity": "working",
            "agentName": "codex",
            "hasSeenWorking": true,
            "isAcknowledged": false,
            "lastChangedAt": activity_at,
            "workingSource": "explicit",
            "workingStartedAt": activity_at
        }),
    );
    let mut update = lifecycle_update(&lifecycle);
    update.insert("lastActiveAt".to_string(), json!(activity_at));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    let before = repository
        .update_session(&update)
        .expect("prepare working session");

    let result = ingest_agent_hook_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "agentSessionId": agent_session_id,
            "eventName": "PreToolUse",
            "projectId": lifecycle.project_id.clone(),
            "rawEventName": "PreToolUse",
            "sessionId": lifecycle.session_id.clone(),
            "status": "working",
            "statusUpdatedAt": activity_at
        })
        .as_object()
        .expect("hook params"),
        temp.path(),
    )
    .expect("hook result");

    assert_eq!(
        result.get("changed"),
        Some(&json!(false)),
        "hook result: {result:#}"
    );
    assert_eq!(result.get("reason"), Some(&json!("activity-unchanged")));
    assert_eq!(
        result
            .get("activity")
            .and_then(|activity| activity.get("activity")),
        Some(&json!("working"))
    );
    assert_eq!(result.get("previousActivity"), Some(&json!("working")));
    let after = repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)
        .expect("read after")
        .expect("after session");
    assert_eq!(after, before);
}

#[test]
fn non_hook_activity_writes_preserve_session_chat_prompt() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
    let (lifecycle, session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());
    let stored_prompt = r#"{"kind":"question","questions":[{"question":"Which color?","options":[{"label":"Red"},{"label":"Blue"}]}]}"#;
    let activity_at = "2026-08-01T05:30:00.000Z";
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    runtime_settings.insert(
        "agentActivity".to_string(),
        json!({
            "activity": "working",
            "agentName": "codex",
            "hasSeenWorking": true,
            "isAcknowledged": false,
            "lastChangedAt": activity_at,
            "sessionChatPrompt": stored_prompt,
            "workingSource": "explicit",
            "workingStartedAt": activity_at
        }),
    );
    let mut update = lifecycle_update(&lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository
        .update_session(&update)
        .expect("seed stored prompt");

    // A terminal-title observation rebuilds agentActivity from the fixed
    // ActivityState struct; the stored card must be carried forward, or a
    // pending AskUserQuestion (which produces no output, so title ticks
    // keep firing) loses its card seconds after the hook stored it.
    ingest_terminal_title_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "rawTitle": "quiet title",
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let after_title = repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)
        .expect("read after title")
        .expect("session after title");
    assert_eq!(
        session_chat_prompt_setting(&after_title).as_deref(),
        Some(stored_prompt),
        "title observation must not erase the stored Session Chat prompt"
    );

    // Explicit activity RPCs (bell/escape/acknowledge) go through
    // update_agent_activity_endpoint and must preserve it too.
    update_agent_activity_endpoint(
        &repository,
        &lifecycle,
        json!({ "activity": "attention", "agentName": "codex" })
            .as_object()
            .expect("activity params"),
    )
    .expect("activity endpoint result");
    let after_activity = repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)
        .expect("read after activity")
        .expect("session after activity");
    assert_eq!(
        session_chat_prompt_setting(&after_activity).as_deref(),
        Some(stored_prompt),
        "explicit activity updates must not erase the stored Session Chat prompt"
    );
}

/// An approval answered with Escape in the terminal ends through no hook, so the Escape the
/// terminal reports retires the stored approval card (and the red waiting-for-approval dot); a
/// question card and every other explicit event keep theirs.
#[test]
fn terminal_escape_retires_a_stored_approval_card_only() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let (lifecycle, session) = create_codex_agent_session(
        &repository,
        "cccccccc-cccc-cccc-cccc-cccccccccccc",
        temp.path(),
    );
    let seed = |prompt: &str| {
        let mut runtime_settings = object_field(&session, "runtimeSettings");
        runtime_settings.insert(
            "agentActivity".to_string(),
            json!({
                "activity": "attention",
                "agentName": "codex",
                "hasSeenWorking": true,
                "lastChangedAt": "2026-10-27T00:00:00.000Z",
                "sessionChatPrompt": prompt,
            }),
        );
        let mut update = lifecycle_update(&lifecycle);
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        repository.update_session(&update).expect("seed prompt");
    };
    let report = |event: &str| {
        update_agent_activity_endpoint(
            &repository,
            &lifecycle,
            json!({ "event": event, "agentName": "codex" })
                .as_object()
                .expect("activity params"),
        )
        .expect("activity endpoint result")
    };
    let stored = || {
        let session = repository
            .get_session(&lifecycle.project_id, &lifecycle.session_id)
            .expect("read session")
            .expect("session");
        session_chat_prompt_setting(&session)
    };
    let approval = r#"{"kind":"approval","tool":"Bash","summary":"rm -rf ./scratch-dir"}"#;
    let question = r#"{"kind":"question","questions":[{"question":"Which color?","options":[{"label":"Red"}]}]}"#;

    seed(approval);
    report("acknowledge");
    assert_eq!(stored().as_deref(), Some(approval));
    let result = report("escape");
    assert_eq!(stored(), None);
    assert_eq!(result["sessionChatPromptChanged"], json!(true));

    seed(question);
    let result = report("escape");
    assert_eq!(stored().as_deref(), Some(question));
    assert!(result.get("sessionChatPromptChanged").is_none());
}

#[test]
fn agent_hook_reconciles_metadata_title_before_first_prompt_reason() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "codex-hook-thread";
    let (lifecycle, _session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());
    let codex_dir = temp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
        codex_dir.join("session_index.jsonl"),
        format!("{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Hook Metadata Title\"}}\n"),
    )
    .expect("write session index");

    let result = ingest_agent_hook_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "agentSessionId": agent_session_id,
            "eventName": "UserPromptSubmit",
            "firstUserMessage": "Please summarize this repository",
            "projectId": lifecycle.project_id.clone(),
            "sessionId": lifecycle.session_id.clone(),
            "status": "working",
            "statusUpdatedAt": "2026-06-09T18:08:19.857Z"
        })
        .as_object()
        .expect("hook params"),
        temp.path(),
    )
    .expect("hook result");

    assert_eq!(result.get("changed"), Some(&json!(true)));
    assert_eq!(result.get("reason"), Some(&json!("metadata-title-applied")));
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("Hook Metadata Title")));
    assert_eq!(
        session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("firstUserMessage")),
        Some(&json!("Please summarize this repository"))
    );
}
