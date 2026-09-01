use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Value};
use uuid::Uuid;

use super::*;
use crate::{
    domain::DomainRepository,
    paths::get_gxserver_paths,
    session_status::compute_activity_update,
    storage::{initialize_gxserver_storage, open_gxserver_database},
};

fn open_test_database() -> (tempfile::TempDir, Connection) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    (temp, db)
}

fn write_metadata_value(db: &Connection, key: &str, value: Value) {
    db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3)",
        rusqlite::params![
            key,
            serde_json::to_string(&value).expect("serialize metadata value"),
            "2026-06-19T00:00:00.000Z"
        ],
    )
    .expect("write metadata value");
}

fn create_agent_session(
    repository: &DomainRepository<'_>,
    agent_id: &str,
    agent_session_id: &str,
    project_path: &Path,
) -> (LifecycleParams, Value) {
    let project = repository
        .create_project(
            json!({
                "name": "Rename Test Project",
                "path": project_path.to_string_lossy()
            })
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
                "agentId": agent_id,
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": agent_id,
                    "agentSessionId": agent_session_id,
                    "titleSource": "placeholder"
                },
                "title": create_agent_session_default_title(None, Some(agent_id))
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id: session
            .get("projectId")
            .and_then(Value::as_str)
            .expect("session project id")
            .to_string(),
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };
    (lifecycle, session)
}

fn create_codex_agent_session(
    repository: &DomainRepository<'_>,
    agent_session_id: &str,
    project_path: &Path,
) -> (LifecycleParams, Value) {
    create_agent_session(repository, "codex", agent_session_id, project_path)
}

fn create_claude_agent_session(
    repository: &DomainRepository<'_>,
    agent_session_id: &str,
    project_path: &Path,
) -> (LifecycleParams, Value) {
    create_agent_session(repository, "claude", agent_session_id, project_path)
}

fn create_pi_agent_session_without_launch_lock(
    repository: &DomainRepository<'_>,
) -> (LifecycleParams, Value) {
    let project = repository
        .create_project(
            json!({ "name": "Pi Lock Project", "path": std::env::temp_dir() })
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
                "agentId": "pi",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "pi",
                    "titleSource": "terminal-auto"
                },
                "title": "Pi Investigation"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id: session
            .get("projectId")
            .and_then(Value::as_str)
            .expect("session project id")
            .to_string(),
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };
    (lifecycle, session)
}

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
    assert!(decision.should_run);
    assert_eq!(decision.strategy, Some("sendBareRenameCommand"));

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
        "agentId": "claude",
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
    let (lifecycle, session) = create_claude_agent_session(
        &repository,
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
            "agentName": "claude",
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
    let (lifecycle, session) = create_claude_agent_session(
        &repository,
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

#[test]
fn terminal_title_capture_preserves_decision_reason_when_identity_title_is_trusted() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Terminal Title Capture", "path": std::env::temp_dir() })
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
                "agentId": "codex",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "codex",
                    "agentSessionId": "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
                    "titleSource": "user"
                },
                "title": "Phase 6 Ingested Title"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id: session
            .get("projectId")
            .and_then(Value::as_str)
            .expect("session project id")
            .to_string(),
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };
    let captured_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    let result = ingest_terminal_title_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "rawTitle": captured_id,
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = result.result;

    assert_eq!(result.get("changed"), Some(&json!(true)));
    assert_eq!(
        result.get("reason"),
        Some(&json!("captured-agent-session-id"))
    );
    assert_eq!(result.get("agentSessionId"), Some(&json!(captured_id)));
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("Phase 6 Ingested Title")));
    assert_eq!(
        session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("agentSessionId")),
        Some(&json!(captured_id))
    );
}

#[test]
fn terminal_title_applies_zmx_title_with_previous_source_reason_without_agent_promotion() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Terminal Title Canonical", "path": std::env::temp_dir() })
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
                "kind": "terminal",
                "projectId": project_id,
                "runtimeSettings": { "titleSource": "placeholder" },
                "title": "Search by Text"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id,
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };

    let output = ingest_terminal_title_event(
        &repository,
        &lifecycle,
        json!({
            "rawTitle": "Find previous Codex work",
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = output.result;

    assert!(output.schedule_presentation_delta);
    assert_eq!(result.get("changed"), Some(&json!(true)));
    assert_eq!(
        result.get("reason"),
        Some(&json!("zmx-terminal-title-from-placeholder"))
    );
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("kind"), Some(&json!("terminal")));
    assert_eq!(session.get("agentId"), None);
    assert_eq!(
        session.get("title"),
        Some(&json!("Find previous Codex work"))
    );
    assert_eq!(
        session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("agentName")),
        None
    );
}

#[test]
fn terminal_title_strips_factory_droid_status_marker_before_sync() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Factory Droid Titles", "path": std::env::temp_dir() })
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
                "agentId": "droid",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "factory droid",
                    "titleSource": "placeholder"
                },
                "title": "Factory Droid Session"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id,
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };

    let output = ingest_terminal_title_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "factory droid",
            "rawTitle": "\u{26ec} New Session",
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = output.result;

    assert!(output.schedule_presentation_delta);
    assert_eq!(result.get("changed"), Some(&json!(true)));
    assert_eq!(
        result.get("reason"),
        Some(&json!("zmx-terminal-title-from-placeholder"))
    );
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("New Session")));
    assert_eq!(
        session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("titleSource")),
        Some(&json!("terminal-auto"))
    );
}

#[test]
fn terminal_title_rejects_untrusted_provider_off_title() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Terminal Title Trust", "path": std::env::temp_dir() })
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
                "kind": "terminal",
                "projectId": project_id,
                "runtimeSettings": { "titleSource": "user" },
                "title": "Terminal Session"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id,
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };

    let output = ingest_terminal_title_event(
        &repository,
        &lifecycle,
        json!({
            "rawTitle": "Untrusted local shell title",
            "sessionPersistenceProvider": "off"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = output.result;

    assert!(!output.schedule_presentation_delta);
    assert_eq!(result.get("changed"), Some(&json!(false)));
    assert_eq!(
        result.get("reason"),
        Some(&json!("terminal-title-not-trusted"))
    );
    assert_eq!(
        result
            .get("session")
            .and_then(|session| session.get("title")),
        Some(&json!("Terminal Session"))
    );
}

#[test]
fn terminal_title_status_bookkeeping_does_not_schedule_presentation_delta() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Terminal Title Status", "path": std::env::temp_dir() })
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
                "kind": "terminal",
                "projectId": project_id,
                "runtimeSettings": { "titleSource": "terminal-auto" },
                "title": "Search by Text"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id,
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };

    let output = ingest_terminal_title_event(
        &repository,
        &lifecycle,
        json!({
            "rawTitle": "Search by Text",
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = output.result;

    assert!(!output.schedule_presentation_delta);
    assert_eq!(result.get("changed"), Some(&json!(false)));
    assert_eq!(
        result
            .get("activity")
            .and_then(|activity| activity.get("activity")),
        Some(&json!("idle"))
    );
    assert_eq!(
        result
            .get("session")
            .and_then(|session| session.get("runtimeSettings"))
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("agentActivity"))
            .and_then(|activity| activity.get("lastTitle")),
        Some(&json!("Search by Text"))
    );
}

#[test]
fn session_state_event_reconciles_codex_metadata_title() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "codex-state-thread";
    let (lifecycle, _session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());
    let codex_dir = temp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
        codex_dir.join("session_index.jsonl"),
        format!("{{\"id\":\"{agent_session_id}\",\"thread_name\":\"State Metadata Title\"}}\n"),
    )
    .expect("write session index");

    let result = ingest_session_state_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "agentSessionId": agent_session_id
        })
        .as_object()
        .expect("state params"),
        temp.path(),
    )
    .expect("state result");

    assert_eq!(result.get("changed"), Some(&json!(true)));
    assert_eq!(result.get("reason"), Some(&json!("metadata-title-applied")));
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("State Metadata Title")));
    assert_eq!(
        session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("titleMetadataSource")),
        Some(&json!("agent-metadata"))
    );
}

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
fn session_state_rejects_cross_agent_metadata_for_stored_pi_session_without_launch_lock() {
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

    let result = ingest_session_state_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "factory droid",
            "agentSessionId": "d7f1ca76-435b-4102-acdb-e3e786cd72a9",
            "agentSessionPath": "/tmp/.factory/sessions/thread.jsonl",
            "projectId": lifecycle.project_id.clone(),
            "sessionId": lifecycle.session_id.clone(),
            "startupText": "droid",
            "status": "working",
            "title": "Wrong Droid Thread"
        })
        .as_object()
        .expect("state params"),
        temp.path(),
    )
    .expect("state result");

    assert_eq!(result.get("changed"), Some(&json!(false)));
    assert_eq!(
        result.get("reason"),
        Some(&json!("session-state-agent-mismatch"))
    );
    let response_session = result.get("session").expect("response session");
    assert_eq!(response_session.get("agentId"), Some(&json!("pi")));
    let runtime_settings = response_session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .expect("runtime settings");
    assert_eq!(runtime_settings.get("agentName"), Some(&json!("pi")));
    assert_eq!(runtime_settings.get("agentSessionId"), None);
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

#[test]
fn terminal_title_capture_reconciles_codex_metadata_title() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Terminal Title Metadata", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id")
        .to_string();
    let agent_session_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    let session = repository
        .create_session(
            json!({
                "agentId": "codex",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "codex",
                    "titleSource": "placeholder"
                },
                "title": "Codex Session"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id,
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };
    let codex_dir = temp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
        codex_dir.join("session_index.jsonl"),
        format!("{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Captured Metadata Title\"}}\n"),
    )
    .expect("write session index");

    let output = ingest_terminal_title_event_with_home(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "rawTitle": agent_session_id,
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
        temp.path(),
    )
    .expect("terminal title result");
    let result = output.result;

    assert!(output.schedule_presentation_delta);
    assert_eq!(result.get("changed"), Some(&json!(true)));
    assert_eq!(result.get("reason"), Some(&json!("metadata-title-applied")));
    assert_eq!(result.get("agentSessionId"), Some(&json!(agent_session_id)));
    let session = result.get("session").expect("result session");
    assert_eq!(
        session.get("title"),
        Some(&json!("Captured Metadata Title"))
    );
    assert_eq!(
        session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("agentSessionId")),
        Some(&json!(agent_session_id))
    );
}

#[test]
fn zmx_status_title_reconciles_codex_rename_metadata() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "codex-zmx-status-title";
    let (lifecycle, _session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());
    let codex_dir = temp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
        codex_dir.join("session_index.jsonl"),
        format!("{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Renamed From Agent CLI\"}}\n"),
    )
    .expect("write session index");

    let output = ingest_terminal_title_event_with_home(
        &repository,
        &lifecycle,
        json!({
            "rawTitle": "⣸ ghostex",
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
        temp.path(),
    )
    .expect("terminal title result");

    assert!(output.schedule_presentation_delta);
    assert_eq!(output.result.get("changed"), Some(&json!(true)));
    assert_eq!(
        output.result.get("reason"),
        Some(&json!("metadata-title-applied"))
    );
    assert_eq!(
        output
            .result
            .get("session")
            .and_then(|session| session.get("title")),
        Some(&json!("Renamed From Agent CLI"))
    );
}

#[test]
fn request_session_rename_reconciles_codex_metadata_title() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "codex-thread-rename";
    let (lifecycle, _session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());
    let codex_dir = temp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
            codex_dir.join("session_index.jsonl"),
            format!(
                "{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Old title\"}}\n{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Renamed Investigation\",\"updated_at\":\"2026-06-21T15:35:00.000Z\"}}\n"
            ),
        )
        .expect("write session index");

    let result = request_session_rename(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "agentSessionId": agent_session_id,
            "title": "Renamed Investigation",
            "titleSource": "user"
        })
        .as_object()
        .expect("rename params"),
        temp.path(),
    )
    .expect("rename result");

    assert_eq!(result.get("changed"), Some(&json!(true)));
    assert_eq!(result.get("pendingAgentMetadata"), Some(&json!(true)));
    assert_eq!(result.get("reason"), Some(&json!("metadata-title-applied")));
    assert_eq!(
        result.get("shouldSendAgentRenameCommand"),
        Some(&json!(true))
    );
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("Renamed Investigation")));
    let runtime_settings = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .expect("runtime settings");
    assert_eq!(
        runtime_settings.get("pendingAgentTitleRequestStatus"),
        Some(&json!("confirmed"))
    );
    assert_eq!(
        runtime_settings.get("titleMetadataProvider"),
        Some(&json!("codex-session-index"))
    );
    assert_eq!(
        runtime_settings.get("titleMetadataSource"),
        Some(&json!("agent-metadata"))
    );
    assert_eq!(
        runtime_settings.get("titleMetadataUpdatedAt"),
        Some(&json!("2026-06-21T15:35:00.000Z"))
    );
    assert_eq!(
        runtime_settings.get("titleSource"),
        Some(&json!("terminal-auto"))
    );
    assert!(runtime_settings.get("titleMetadataCheckedAt").is_some());
    let stored = repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)
        .expect("get stored session")
        .expect("stored session");
    assert_eq!(stored.get("title"), Some(&json!("Renamed Investigation")));
}

#[test]
fn request_session_rename_keeps_pending_when_codex_metadata_is_missing() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "codex-thread-missing";
    let (lifecycle, _session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());

    let result = request_session_rename(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "agentSessionId": agent_session_id,
            "title": "Requested Missing Title",
            "titleSource": "user"
        })
        .as_object()
        .expect("rename params"),
        temp.path(),
    )
    .expect("rename result");

    assert_eq!(
        result.get("reason"),
        Some(&json!("agent-rename-request-pending-metadata"))
    );
    assert_eq!(
        result.get("shouldSendAgentRenameCommand"),
        Some(&json!(true))
    );
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("Codex Session")));
    let runtime_settings = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .expect("runtime settings");
    assert_eq!(
        runtime_settings.get("pendingAgentTitleRequestStatus"),
        Some(&json!("pending"))
    );
    assert_eq!(
        runtime_settings.get("pendingAgentTitleRequestTitle"),
        Some(&json!("Requested Missing Title"))
    );
    assert!(runtime_settings.get("titleMetadataSource").is_none());
}

#[test]
fn trailing_agent_metadata_reconcile_marks_request_mismatch() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let agent_session_id = "codex-thread-trailing";
    let (lifecycle, _session) =
        create_codex_agent_session(&repository, agent_session_id, temp.path());
    let _pending = request_session_rename(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "agentSessionId": agent_session_id,
            "title": "Requested Title",
            "titleSource": "user"
        })
        .as_object()
        .expect("rename params"),
        temp.path(),
    )
    .expect("pending rename");
    let codex_dir = temp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
        codex_dir.join("session_index.jsonl"),
        format!("{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Accepted Different Title\"}}\n"),
    )
    .expect("write session index");

    let changed = reconcile_agent_metadata_title_for_session(
        &repository,
        &lifecycle.project_id,
        &lifecycle.session_id,
        temp.path(),
        "metadata-mismatch",
    )
    .expect("trailing reconcile");

    assert!(changed);
    let stored = repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)
        .expect("get stored session")
        .expect("stored session");
    assert_eq!(
        stored.get("title"),
        Some(&json!("Accepted Different Title"))
    );
    let runtime_settings = stored
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .expect("runtime settings");
    assert_eq!(
        runtime_settings.get("pendingAgentTitleRequestStatus"),
        Some(&json!("metadata-mismatch"))
    );
}

#[test]
fn live_process_identity_promotes_running_zmx_terminal_to_codex() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Ghostex", "path": std::env::temp_dir() })
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
                "kind": "terminal",
                "lifecycleState": "running",
                "projectId": project_id,
                "runtimeSettings": {
                    "sessionPersistenceProvider": "zmx",
                    "titleSource": "user"
                },
                "surface": "workspace",
                "title": "Sidebar scrolls after closing (set above)"
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

    let changed = apply_live_process_session_identity(
        &repository,
        &project_id,
        &session_id,
        Some("codex".to_string()),
        Some("019EB8D0-D27B-7F30-B6D7-7A04AB8FAE78".to_string()),
        None,
    )
    .expect("apply live process identity");

    assert!(changed);
    let updated = repository
        .get_session(&project_id, &session_id)
        .expect("get updated session")
        .expect("updated session");
    assert_eq!(updated.get("kind"), Some(&json!("agent")));
    assert_eq!(updated.get("agentId"), Some(&json!("codex")));
    assert_eq!(
        updated.get("title"),
        Some(&json!("Sidebar scrolls after closing (set above)"))
    );
    let runtime_settings = updated
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .expect("runtime settings");
    assert_eq!(runtime_settings.get("agentName"), Some(&json!("codex")));
    assert_eq!(runtime_settings.get("launchAgentId"), Some(&json!("codex")));
    assert_eq!(
        runtime_settings.get("agentSessionId"),
        Some(&json!("019EB8D0-D27B-7F30-B6D7-7A04AB8FAE78"))
    );
}

#[test]
fn live_process_identity_claims_codex_id_observed_before_process_promotion() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({
                "name": "Ghostex",
                "path": temp.path().to_string_lossy()
            })
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
                "kind": "terminal",
                "lifecycleState": "running",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentActivity": {
                        "lastTitle": "019ff871-8b5c-7ce2-bcf7-5409263e2e0e"
                    },
                    "sessionPersistenceProvider": "zmx",
                    "titleSource": "placeholder"
                },
                "surface": "workspace",
                "title": "Terminal Session"
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

    let changed = apply_live_process_session_identity(
        &repository,
        &project_id,
        &session_id,
        Some("codex".to_string()),
        None,
        None,
    )
    .expect("apply live process identity");

    assert!(changed);
    let updated = repository
        .get_session(&project_id, &session_id)
        .expect("get updated session")
        .expect("updated session");
    assert_eq!(updated.get("kind"), Some(&json!("agent")));
    assert_eq!(updated.get("agentId"), Some(&json!("codex")));
    assert_eq!(
        updated
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("agentSessionId")),
        Some(&json!("019ff871-8b5c-7ce2-bcf7-5409263e2e0e"))
    );
}

#[test]
fn live_process_identity_replaces_wsl_shell_title_and_defers_to_codex_auto_title() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({
                "name": "Ghostex",
                "path": temp.path().to_string_lossy()
            })
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
                "kind": "terminal",
                "lifecycleState": "running",
                "projectId": project_id,
                "runtimeSettings": {
                    "sessionPersistenceProvider": "zmx",
                    "titleSource": "terminal-auto"
                },
                "surface": "workspace",
                "title": "madda@M7-Desktop: /mnt/c/dev/Ghostex"
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

    let changed = apply_live_process_session_identity(
        &repository,
        &project_id,
        &session_id,
        Some("codex".to_string()),
        Some("019EB8D0-D27B-7F30-B6D7-7A04AB8FAE78".to_string()),
        None,
    )
    .expect("apply live process identity");

    assert!(changed);
    let updated = repository
        .get_session(&project_id, &session_id)
        .expect("get updated session")
        .expect("updated session");
    assert_eq!(updated.get("kind"), Some(&json!("agent")));
    assert_eq!(updated.get("agentId"), Some(&json!("codex")));
    assert_eq!(updated.get("title"), Some(&json!("Codex Session")));
    assert_eq!(
        updated
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("titleSource")),
        Some(&json!("placeholder"))
    );

    let lifecycle = LifecycleParams {
        project_id: project_id.clone(),
        session_id: session_id.clone(),
    };
    let result = ingest_agent_hook_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "agentSessionId": "019EB8D0-D27B-7F30-B6D7-7A04AB8FAE78",
            "eventName": "UserPromptSubmit",
            "firstUserMessage": "Please summarize this repository"
        })
        .as_object()
        .expect("hook params"),
        temp.path(),
    )
    .expect("hook result");

    assert_eq!(result.get("reason"), Some(&json!("activity-updated")));
    let hooked_session = result.get("session").expect("hooked session");
    assert_eq!(hooked_session.get("title"), Some(&json!("Codex Session")));
    assert_eq!(
        hooked_session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("gxserverFirstPromptAutoTitleStatus")),
        None
    );
}

#[test]
fn agent_settings_use_current_metadata_key_and_default_prompt_agent() {
    let (_temp, db) = open_test_database();
    let initial = read_agent_settings_with_metadata(&db).expect("initial settings");
    assert_eq!(initial.get("isPersisted"), Some(&json!(false)));
    assert_eq!(
        initial
            .get("settings")
            .and_then(|settings| settings.get("agentAcceptAllEnabled")),
        Some(&json!(true))
    );
    assert_eq!(
        initial
            .get("settings")
            .and_then(|settings| settings.get("defaultPromptAgentId")),
        Some(&json!("codex"))
    );

    let updated = update_agent_settings(
        &db,
        json!({ "agentAcceptAllEnabled": false, "defaultPromptAgentId": " claude " })
            .as_object()
            .expect("params"),
    )
    .expect("update settings");
    assert_eq!(updated.get("agentAcceptAllEnabled"), Some(&json!(false)));
    assert_eq!(updated.get("defaultPromptAgentId"), Some(&json!("claude")));
    let persisted: String = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [AGENT_SETTINGS_METADATA_KEY],
            |row| row.get(0),
        )
        .expect("persisted settings");
    let persisted_value = parse_json_object(&persisted);
    assert_eq!(
        persisted_value
            .get("defaultPromptAgentId")
            .and_then(Value::as_str),
        Some("claude")
    );
    let legacy_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM metadata WHERE key = 'gxserverAgentSettings'",
            [],
            |row| row.get(0),
        )
        .expect("legacy count");
    assert_eq!(legacy_count, 0);
}

#[test]
fn agent_settings_ignore_legacy_metadata_key() {
    let (_temp, db) = open_test_database();
    let legacy_value = json!({ "agentAcceptAllEnabled": false, "defaultPromptAgentId": "claude" });
    write_metadata_value(&db, "gxserverAgentSettings", legacy_value.clone());

    let settings = read_agent_settings_with_metadata(&db).expect("legacy ignored");
    assert_eq!(settings.get("isPersisted"), Some(&json!(false)));
    assert_eq!(
        settings
            .get("settings")
            .and_then(|settings| settings.get("agentAcceptAllEnabled")),
        Some(&json!(true))
    );
    assert_eq!(
        settings
            .get("settings")
            .and_then(|settings| settings.get("defaultPromptAgentId")),
        Some(&json!("codex"))
    );

    let updated = update_agent_settings(
        &db,
        json!({ "defaultPromptAgentId": " claude " })
            .as_object()
            .expect("params"),
    )
    .expect("update settings with legacy row present");
    assert_eq!(updated.get("agentAcceptAllEnabled"), Some(&json!(true)));
    assert_eq!(updated.get("defaultPromptAgentId"), Some(&json!("claude")));

    let current: String = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [AGENT_SETTINGS_METADATA_KEY],
            |row| row.get(0),
        )
        .expect("current metadata");
    let current_value = parse_json_object(&current);
    assert_eq!(
        current_value
            .get("agentAcceptAllEnabled")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        current_value
            .get("defaultPromptAgentId")
            .and_then(Value::as_str),
        Some("claude")
    );

    let legacy: String = db
        .query_row(
            "SELECT value FROM metadata WHERE key = 'gxserverAgentSettings'",
            [],
            |row| row.get(0),
        )
        .expect("legacy metadata remains unrelated");
    assert_eq!(parse_json_object(&legacy), legacy_value);
}

#[test]
fn agent_settings_normalize_default_prompt_agent_id() {
    let (_temp, db) = open_test_database();
    let blank = update_agent_settings(
        &db,
        json!({ "defaultPromptAgentId": "   " })
            .as_object()
            .expect("params"),
    )
    .expect("blank update");
    assert_eq!(blank.get("defaultPromptAgentId"), Some(&json!("codex")));

    let long_id = "x".repeat(MAX_DEFAULT_PROMPT_AGENT_ID_LENGTH + 10);
    let capped = update_agent_settings(
        &db,
        json!({ "defaultPromptAgentId": long_id })
            .as_object()
            .expect("params"),
    )
    .expect("long update");
    let stored = capped
        .get("defaultPromptAgentId")
        .and_then(Value::as_str)
        .expect("stored id");
    assert_eq!(stored.len(), MAX_DEFAULT_PROMPT_AGENT_ID_LENGTH);
}

#[test]
fn create_agent_session_params_use_project_agent_config_and_settings() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    update_agent_settings(
        &db,
        json!({ "agentAcceptAllEnabled": false })
            .as_object()
            .expect("settings params"),
    )
    .expect("agent settings");
    let project = repository
        .create_project(
            json!({
                "customAgents": [{
                    "acceptAllMode": "enabled",
                    "agentId": "claude",
                    "command": "claude",
                    "icon": "claude"
                }],
                "name": "Agent CRUD",
                "path": std::env::temp_dir()
            })
            .as_object()
            .expect("project params"),
        )
        .expect("project created");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id");
    let params = json!({
        "agentId": "claude",
        "launchSettings": {
            "agentCommand": "ignored-local-command",
            "delayedSendDeadlineAt": "2026-06-22T05:40:00.000Z"
        },
        "projectId": project_id,
        "runtimeSettings": {
            "firstUserMessage": "Summarize this repository."
        },
        "title": "Claude Agent"
    });
    let create_params = create_agent_session_params_for_project(
        &db,
        &project,
        params.as_object().expect("create params"),
    )
    .expect("normalized create params");

    let launch_settings = create_params
        .get("launchSettings")
        .and_then(Value::as_object)
        .expect("launch settings");
    let launch_plan = launch_settings
        .get("agentLaunchPlan")
        .and_then(Value::as_object)
        .expect("launch plan");
    assert_eq!(launch_plan.get("agentCommand"), Some(&json!("claude")));
    assert_eq!(
        launch_plan.get("command"),
        Some(&json!("claude --dangerously-skip-permissions"))
    );
    assert_eq!(
        launch_plan.get("firstUserMessage"),
        Some(&json!("Summarize this repository."))
    );
    assert_eq!(
        launch_plan
            .get("delayedSend")
            .and_then(|value| value.get("deadlineAt")),
        Some(&json!("2026-06-22T05:40:00.000Z"))
    );
    assert_eq!(
        launch_settings
            .get("runtimeRelevant")
            .and_then(|value| value.get("queueProviderStartupText")),
        Some(&json!(true))
    );
    let runtime_settings = create_params
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .expect("runtime settings");
    assert_eq!(runtime_settings.get("agentCommand"), Some(&json!("claude")));
    assert_eq!(
        runtime_settings.get("launchAgentId"),
        Some(&json!("claude"))
    );
    assert_eq!(
        runtime_settings
            .get("agentActivity")
            .and_then(|value| value.get("activity")),
        Some(&json!("working"))
    );
    assert_eq!(
        runtime_settings
            .get("agentActivity")
            .and_then(|value| value.get("agentName")),
        Some(&json!("claude"))
    );

    let session = repository
        .create_session(&create_params, false)
        .expect("agent session created");
    assert_eq!(session.get("kind"), Some(&json!("agent")));
    assert_eq!(session.get("agentId"), Some(&json!("claude")));
    assert_eq!(
        session
            .get("launchSettings")
            .and_then(|value| value.get("agentLaunchPlan"))
            .and_then(|value| value.get("command")),
        Some(&json!("claude --dangerously-skip-permissions"))
    );
}

#[test]
fn launch_plan_applies_agent_settings_accept_all() {
    let (_temp, db) = open_test_database();
    let project = json!({
        "customAgents": [{ "agentId": "codex", "command": "codex" }],
        "launchSettings": {},
    });
    let default_settings = read_agent_settings(&db).expect("settings");
    let plan = build_project_agent_launch_plan(&project, "codex", None, &default_settings);
    assert_eq!(plan.get("command"), Some(&json!("codex --yolo")));
    update_agent_settings(
        &db,
        json!({ "agentAcceptAllEnabled": false })
            .as_object()
            .expect("params"),
    )
    .expect("update settings");
    let disabled_settings = read_agent_settings(&db).expect("disabled settings");
    let plan = build_project_agent_launch_plan(&project, "codex", None, &disabled_settings);
    assert_eq!(plan.get("command"), Some(&json!("codex --yolo")));
}

#[test]
fn launch_plan_keeps_typescript_custom_agent_lookup_and_empty_shape() {
    let settings = normalize_agent_settings(None);
    let project_with_id_only_agent = json!({
        "customAgents": [{ "id": "codex", "command": "codex --profile ignored" }],
        "launchSettings": {},
    });
    let plan =
        build_project_agent_launch_plan(&project_with_id_only_agent, "codex", None, &settings);
    assert_eq!(plan.get("agentCommand"), Some(&json!("codex")));
    assert_eq!(plan.get("command"), Some(&json!("codex --yolo")));

    let unknown_plan = build_project_agent_launch_plan(
        &json!({ "customAgents": [], "launchSettings": {} }),
        "custom-local",
        None,
        &settings,
    );
    assert_eq!(unknown_plan.get("agentCommand"), Some(&json!("")));
    assert_eq!(unknown_plan.get("command"), Some(&json!("")));
    assert_eq!(unknown_plan.get("startupText"), Some(&json!("")));
    assert_eq!(
        unknown_plan.get("startupTextDisposition"),
        Some(&json!("none"))
    );
}

#[test]
fn accept_all_specs_match_typescript_aliases_and_icon_mapping() {
    assert_eq!(
        resolve_agent_launch_command("cursor", "cursor-agent --allow-all", None, true, None),
        "cursor-agent --allow-all --yolo"
    );
    assert_eq!(
        resolve_agent_launch_command(
            "cursor",
            "cursor-agent --force --yolo",
            Some("disabled"),
            true,
            None,
        ),
        "cursor-agent"
    );
    assert_eq!(
        resolve_agent_launch_command("gemini", "gemini --allow-all", None, true, None),
        "gemini --allow-all --yolo"
    );
    assert_eq!(
        resolve_agent_launch_command("copilot", "copilot -y", None, true, None),
        "copilot -y --yolo"
    );
    assert_eq!(
        resolve_agent_launch_command(
            "custom-cursor",
            "cursor-agent",
            None,
            true,
            Some("cursor-cli")
        ),
        "cursor-agent --yolo"
    );
    assert_eq!(
        resolve_agent_launch_command(
            "grok",
            "grok --permission-mode bypassPermissions --always-approve",
            None,
            true,
            None,
        ),
        "grok --permission-mode bypassPermissions"
    );
    assert_eq!(
        resolve_agent_launch_command(
            "grok",
            "grok --permission-mode=bypassPermissions --always-approve",
            Some("disabled"),
            true,
            None,
        ),
        "grok"
    );
}

#[test]
fn cursor_launch_appends_only_normalized_resume_chat_ids() {
    let valid = build_agent_launch_plan(AgentLaunchInput {
        accept_all_mode: None,
        agent_id: "cursor".to_string(),
        agent_session_id: Some("8B16E7E6-3CE1-4D0B-9F35-78261B7F0767".to_string()),
        command: Some("cursor-agent".to_string()),
        delayed_send_deadline_at: None,
        first_user_message: None,
        global_accept_all_enabled: true,
        icon: None,
    });
    assert_eq!(
        valid.get("command"),
        Some(&json!(
            "cursor-agent --yolo --resume \"8b16e7e6-3ce1-4d0b-9f35-78261b7f0767\""
        ))
    );

    let invalid = build_agent_launch_plan(AgentLaunchInput {
        accept_all_mode: None,
        agent_id: "cursor".to_string(),
        agent_session_id: Some("not-a-chat-id".to_string()),
        command: Some("cursor-agent".to_string()),
        delayed_send_deadline_at: None,
        first_user_message: None,
        global_accept_all_enabled: true,
        icon: None,
    });
    assert_eq!(invalid.get("command"), Some(&json!("cursor-agent --yolo")));
}

#[test]
fn resume_and_fork_plans_shape_agent_commands() {
    let project = json!({ "path": "/tmp/project", "customAgents": [], "launchSettings": {} });
    let session = json!({
        "agentId": "codex",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "codex",
            "agentSessionId": "12345678-1234-1234-1234-123456789abc",
            "titleSource": "terminal-auto"
        },
        "title": "Investigate bug",
    });
    let settings = normalize_agent_settings(None);
    let resume = build_agent_resume_plan(&project, &session, &settings);
    let primary_command = resume
        .get("primaryCommand")
        .and_then(Value::as_str)
        .expect("primary command");
    assert!(primary_command.contains("CODEX_RESUME_SESSION_ID"));
    assert!(primary_command.contains("--exact"));
    assert!(primary_command.contains("codex --yolo resume \"$CODEX_RESUME_SESSION_ID\""));
    assert_eq!(
        resume.get("displayCommand"),
        Some(&json!(
            "codex --yolo resume \"12345678-1234-1234-1234-123456789abc\""
        ))
    );
    assert_eq!(
        resume.get("copyCommand"),
        Some(&json!(
            "codex --yolo resume \"12345678-1234-1234-1234-123456789abc\""
        ))
    );
    assert!(resume
        .get("fallbackCommand")
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains("--title")));
    let startup_text = resume
        .get("startupText")
        .and_then(Value::as_str)
        .expect("startup text");
    assert!(startup_text.starts_with(' '));
    assert!(startup_text.contains("Restoring session..."));
    assert!(startup_text
        .contains("__ghostex_restore_resume_primary || __ghostex_restore_resume_status=$?"));
    assert!(startup_text.contains("Exact resume failed; trying saved fallback resume command."));
    assert!(startup_text.contains("codex --yolo resume \"12345678-1234-1234-1234-123456789abc\""));
    let fork = build_agent_fork_plan(&project, &session, &settings);
    assert_eq!(
        fork.get("primaryCommand"),
        Some(&json!(
            "codex --yolo fork \"12345678-1234-1234-1234-123456789abc\""
        ))
    );
}

#[test]
fn resume_plan_extracts_provider_exact_identity_hints() {
    let project = json!({ "path": "/repo/ghostex", "customAgents": [], "launchSettings": {} });
    let settings = {
        let mut settings = normalize_agent_settings(None);
        settings.insert("agentAcceptAllEnabled".to_string(), Value::Bool(false));
        settings
    };
    let claude = json!({
        "agentId": "claude",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "claude",
            "agentSessionPath": "/Users/example/.claude/projects/-repo-ghostex/9970b270-b39f-4d63-a764-fa8d88083995.jsonl",
            "titleSource": "user"
        },
        "title": "Readable Claude title",
    });
    let claude_plan = build_agent_resume_plan(&project, &claude, &settings);
    assert_eq!(
        claude_plan.get("primaryCommand"),
        Some(&json!(
            "claude --dangerously-skip-permissions --resume \"9970b270-b39f-4d63-a764-fa8d88083995\""
        ))
    );
    assert_eq!(
        claude_plan.get("displayCommand"),
        claude_plan.get("primaryCommand")
    );
    assert_eq!(
        claude_plan.get("copyCommand"),
        claude_plan.get("primaryCommand")
    );
    assert!(claude_plan
        .get("fallbackCommand")
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains("CLAUDE_RESUME_SESSION_ID")));

    let cursor = json!({
        "agentId": "cursor",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "cursor-agent",
            "resumeCommand": "cd '/repo/ghostex' && cursor-agent --resume \"E10971DA-CBD7-459A-9AC3-B9B0313199A3\"",
            "titleSource": "user"
        },
        "title": "∗ Cursor CLI Session",
    });
    let cursor_plan = build_agent_resume_plan(&project, &cursor, &settings);
    assert_eq!(
        cursor_plan.get("primaryCommand"),
        Some(&json!(
            "cursor-agent --resume \"e10971da-cbd7-459a-9ac3-b9b0313199a3\""
        ))
    );
    assert!(cursor_plan.get("fallbackCommand").is_none());

    let pi = json!({
        "agentId": "pi",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "pi",
            "agentSessionId": "pi-id",
            "agentSessionPath": "/tmp/pi/session/path",
            "titleSource": "user"
        },
        "title": "Pi thread",
    });
    let pi_plan = build_agent_resume_plan(&project, &pi, &settings);
    assert_eq!(
        pi_plan.get("primaryCommand"),
        Some(&json!("pi --session \"/tmp/pi/session/path\""))
    );
}

#[test]
fn opencode_resume_keeps_lookup_command_separate_from_runtime_accept_all() {
    let project = json!({ "path": "/repo/ghostex", "customAgents": [], "launchSettings": {} });
    let settings = normalize_agent_settings(None);
    let titled = json!({
        "agentId": "opencode",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "opencode",
            "titleSource": "user"
        },
        "title": "Readable thread title",
    });
    let plan = build_agent_resume_plan(&project, &titled, &settings);
    assert_eq!(
        plan.get("runtimeCommand"),
        Some(&json!(
            "OPENCODE_CONFIG_CONTENT='{\"permission\":\"allow\"}' opencode"
        ))
    );
    assert_eq!(plan.get("lookupCommand"), Some(&json!("opencode")));
    let primary = plan
        .get("primaryCommand")
        .and_then(Value::as_str)
        .expect("primary command");
    assert!(primary.contains("OPENCODE_CONFIG_CONTENT='{\"permission\":\"allow\"}' opencode -s"));
    assert!(primary.contains("opencode session list --format json"));
    assert!(!primary
        .contains("OPENCODE_CONFIG_CONTENT='{\"permission\":\"allow\"}' opencode session list"));
    assert!(plan.get("copyCommand").is_none());
}

#[test]
fn attach_startup_text_uses_agent_resume_plan_and_settings() {
    let project = json!({ "path": "/tmp/project", "customAgents": [], "launchSettings": {} });
    let session = json!({
        "agentId": "codex",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "codex",
            "agentSessionId": "12345678-1234-1234-1234-123456789abc"
        },
        "title": "Restorable Codex",
    });
    let mut settings = normalize_agent_settings(None);
    settings.insert("agentAcceptAllEnabled".to_string(), Value::Bool(false));

    let startup_text =
        get_agent_startup_text_for_session(&project, &session, &settings).expect("startup text");
    assert!(startup_text.starts_with(' '));
    assert!(startup_text.ends_with('\r'));
    assert!(startup_text.contains("Restoring session..."));
    assert!(startup_text.contains(
        "printf '> %s\\n\\n' 'codex --yolo resume \"12345678-1234-1234-1234-123456789abc\"'"
    ));
    assert!(startup_text.contains("codex --yolo resume \"12345678-1234-1234-1234-123456789abc\""));
}

#[test]
fn resume_plan_rejects_gxserver_session_id_titles() {
    let project = json!({ "path": "/tmp/project", "customAgents": [], "launchSettings": {} });
    let session = json!({
        "agentId": "cursor",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "cursor-agent",
            "titleSource": "user"
        },
        "title": "G3gnt",
    });
    let settings = normalize_agent_settings(None);
    let resume = build_agent_resume_plan(&project, &session, &settings);
    assert_eq!(resume.get("primaryCommand"), None);
    assert_eq!(resume.get("startupTextDisposition"), Some(&json!("none")));
}

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
    assert_eq!(
        normalize_agent_hook_activity(
            Some(&json!("attention")),
            Some(&json!("Notification")),
            Some(&json!("GitHub Copilot"))
        ),
        Some("idle".to_string())
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
CDXC:SessionChatIdentity 2026-08-02:
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
