use serde_json::{json, Value};
use uuid::Uuid;

use super::*;
use crate::domain::DomainRepository;

#[test]
fn first_prompt_claim_decision_matches_provider_strategy_and_prompt_normalization() {
    let codex = json!({
        "agentId": "codex",
        "runtimeSettings": {},
        "title": "Terminal",
    });
    let decision = decide_first_prompt_auto_title_claim(
        &codex,
        Some("Please can you help me fix the sidebar."),
        false,
        false,
    );
    assert!(!decision.should_run);
    assert_eq!(decision.reason, "agentAutoTitle");
    assert_eq!(
        decision.normalized_prompt.as_deref(),
        Some("fix the sidebar")
    );
    assert_eq!(decision.strategy, Some("agentAutoTitle"));

    let claude = json!({
        "agentId": "claude",
        "runtimeSettings": {},
        "title": "Claude Code",
    });
    let decision = decide_first_prompt_auto_title_claim(
        &claude,
        Some("Summarize the session logs"),
        false,
        false,
    );
    assert!(!decision.should_run);
    assert_eq!(decision.strategy, Some("agentAutoTitle"));

    let pi = json!({
        "agentId": "pi",
        "runtimeSettings": {},
        "title": "\u{03c0}",
    });
    let decision = decide_first_prompt_auto_title_claim(
        &pi,
        Some("How does resource syncing work?"),
        false,
        false,
    );
    assert!(decision.should_run);
    assert_eq!(
        decision.normalized_prompt.as_deref(),
        Some("resource syncing work")
    );
    assert_eq!(decision.strategy, Some("generateTitleAndName"));
}

#[test]
fn first_prompt_claim_decision_skips_non_claimable_prompts_without_running_state() {
    let claude = json!({
        "agentId": "claude",
        "runtimeSettings": {},
        "title": "Agent",
    });
    let meta = decide_first_prompt_auto_title_claim(
        &claude,
        Some("# AGENTS.md instructions for this repository"),
        false,
        false,
    );
    assert!(!meta.should_run);
    assert_eq!(meta.reason, "metaPrompt");

    let slash = decide_first_prompt_auto_title_claim(
        &claude,
        Some("notes before command\n  /status please"),
        false,
        false,
    );
    assert!(!slash.should_run);
    assert_eq!(slash.reason, "slashCommand");

    let unsupported = json!({
        "agentId": "cursor",
        "runtimeSettings": {},
        "title": "Terminal",
    });
    let unsupported =
        decide_first_prompt_auto_title_claim(&unsupported, Some("Summarize this"), false, false);
    assert!(!unsupported.should_run);
    assert_eq!(unsupported.reason, "unsupportedAgent");

    let named = json!({
        "agentId": "codex",
        "runtimeSettings": { "autoTitleFromFirstPrompt": true },
        "title": "Codex",
    });
    let named = decide_first_prompt_auto_title_claim(&named, Some("Summarize this"), false, false);
    assert!(!named.should_run);
    assert_eq!(named.reason, "alreadyAutoNamed");
}

#[test]
fn first_prompt_claim_retries_cancelled_job_for_new_submit_or_later_prompt() {
    let first_prompt = "Please cancel this generated title before rename";
    let session = json!({
        "agentId": "pi",
        "runtimeSettings": {
            "firstUserMessage": first_prompt,
            "gxserverFirstPromptAutoTitleCancelledAt": "2026-06-22T04:00:00.000Z",
            "gxserverFirstPromptAutoTitleCancelledPrompt": first_prompt,
            "gxserverFirstPromptAutoTitleReason": "escape",
            "gxserverFirstPromptAutoTitleStatus": "cancelled"
        },
        "title": "Terminal",
    });

    let same_passive =
        decide_first_prompt_auto_title_claim(&session, Some(first_prompt), false, false);
    assert!(!same_passive.should_run);
    assert_eq!(same_passive.reason, "already-cancelled");

    let same_explicit =
        decide_first_prompt_auto_title_claim(&session, Some(first_prompt), false, true);
    assert!(same_explicit.should_run);
    assert_eq!(same_explicit.reason, "eligible");

    let later = decide_first_prompt_auto_title_claim(
        &session,
        Some("Now explain the auto sleep defaults"),
        false,
        false,
    );
    assert!(later.should_run);
    assert_eq!(later.reason, "eligible");
    assert_eq!(
        later.normalized_prompt.as_deref(),
        Some("Now explain the auto sleep defaults")
    );
}

#[test]
fn user_prompt_submit_hook_rearms_cancelled_identical_prompt() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let (lifecycle, session) = create_agent_session(
        &repository,
        "pi",
        "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
        temp.path(),
    );
    let first_prompt = "Please cancel this generated title before rename";
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    runtime_settings.insert("firstUserMessage".to_string(), json!(first_prompt));
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleCancelledPrompt".to_string(),
        json!(first_prompt),
    );
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleStatus".to_string(),
        json!("cancelled"),
    );
    runtime_settings.insert(
        FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY.to_string(),
        json!("cancelled-attempt"),
    );
    let mut update = lifecycle_update(&lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&update).expect("cancelled row");

    let result = ingest_agent_hook_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "pi",
            "eventName": "UserPromptSubmit",
            "firstUserMessage": first_prompt,
            "projectId": lifecycle.project_id.clone(),
            "sessionId": lifecycle.session_id.clone(),
            "status": "working"
        })
        .as_object()
        .expect("hook params"),
        temp.path(),
    )
    .expect("hook result");

    assert_eq!(
        result.get("reason"),
        Some(&json!("first-prompt-auto-title-claimed"))
    );
    assert_eq!(
        result
            .get("session")
            .and_then(|session| session.get("runtimeSettings"))
            .and_then(Value::as_object)
            .and_then(|runtime| runtime.get("gxserverFirstPromptAutoTitleStatus")),
        Some(&json!("running"))
    );
    let replacement_attempt = result
        .get("session")
        .and_then(|session| session.get("runtimeSettings"))
        .and_then(Value::as_object)
        .and_then(|runtime| runtime.get(FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY))
        .and_then(Value::as_str)
        .expect("replacement attempt id");
    assert_ne!(replacement_attempt, "cancelled-attempt");
    assert!(Uuid::parse_str(replacement_attempt).is_ok());
}

#[test]
fn first_prompt_claim_clears_cancelled_metadata_for_repeated_explicit_prompt() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let (lifecycle, session) = create_agent_session(
        &repository,
        "pi",
        "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
        temp.path(),
    );
    let first_prompt = "Please cancel this generated title before rename";
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    runtime_settings.insert("firstUserMessage".to_string(), json!(first_prompt));
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleCancelledAt".to_string(),
        json!("2026-06-22T04:00:00.000Z"),
    );
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleCancelledPrompt".to_string(),
        json!(first_prompt),
    );
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleReason".to_string(),
        json!("escape"),
    );
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleStatus".to_string(),
        json!("cancelled"),
    );
    runtime_settings.insert(
        FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY.to_string(),
        json!("cancelled-attempt"),
    );
    let mut update = lifecycle_update(&lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    let cancelled = repository.update_session(&update).expect("cancelled row");

    let same = claim_first_prompt_auto_title(
        &repository,
        &cancelled,
        Some(first_prompt.to_string()),
        false,
    )
    .expect("passive same prompt claim");
    assert!(same.is_none());

    let latest = repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)
        .expect("read latest")
        .expect("latest session");
    let claimed =
        claim_first_prompt_auto_title(&repository, &latest, Some(first_prompt.to_string()), true)
            .expect("explicit repeated prompt claim")
            .expect("claimed session");
    let runtime = object_field(&claimed, "runtimeSettings");
    assert_eq!(
        runtime
            .get("gxserverFirstPromptAutoTitleStatus")
            .and_then(Value::as_str),
        Some("running")
    );
    assert_eq!(
        runtime.get("firstUserMessage").and_then(Value::as_str),
        Some(first_prompt)
    );
    let replacement_attempt = runtime
        .get(FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY)
        .and_then(Value::as_str)
        .expect("replacement attempt id");
    assert_ne!(replacement_attempt, "cancelled-attempt");
    assert!(Uuid::parse_str(replacement_attempt).is_ok());
    assert!(runtime
        .get("gxserverFirstPromptAutoTitleCancelledAt")
        .is_none());
    assert!(runtime
        .get("gxserverFirstPromptAutoTitleCancelledPrompt")
        .is_none());
    assert!(runtime.get("gxserverFirstPromptAutoTitleReason").is_none());
}
