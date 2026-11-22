use serde_json::{json, Value};

use super::*;
use crate::domain::DomainRepository;

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

/// After `/new` Codex's title names the new conversation before its rollout exists: the stored
/// rollout of the old one goes with the old id, and a rollout of the named one stays.
#[test]
fn a_captured_codex_id_drops_the_rollout_of_another_conversation() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Codex New", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let project_id = project["projectId"]
        .as_str()
        .expect("project id")
        .to_string();
    let old = "01a1cad7-aea0-7a60-8e25-fac7d7f2ac40";
    let new = "01a1cad8-1187-7f80-9c06-8236a85f6550";
    let rollout = |id: &str| {
        format!("/home/person/.codex/sessions/2026/11/11/rollout-2026-11-11T17-41-35-{id}.jsonl")
    };
    let capture = |stored_path: String, title_id: &str| {
        let session = repository
            .create_session(
                json!({
                    "agentId": "codex",
                    "kind": "agent",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "agentName": "codex",
                        "agentSessionId": old,
                        "agentSessionPath": stored_path,
                        "titleSource": "user"
                    },
                    "title": "Codex New"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let lifecycle = LifecycleParams {
            project_id: project_id.clone(),
            session_id: session["sessionId"]
                .as_str()
                .expect("session id")
                .to_string(),
        };
        let result = ingest_terminal_title_event(
            &repository,
            &lifecycle,
            json!({ "agentName": "codex", "rawTitle": title_id, "sessionPersistenceProvider": "zmx" })
                .as_object()
                .expect("terminal title params"),
        )
        .expect("terminal title result");
        result.result["session"]["runtimeSettings"].clone()
    };
    let moved = capture(rollout(old), new);
    assert_eq!(moved["agentSessionId"], json!(new));
    assert!(moved.get("agentSessionPath").is_none());
    // A stored rollout that already names the captured conversation is kept.
    let kept = capture(rollout(new), new);
    assert_eq!(kept["agentSessionPath"], json!(rollout(new)));
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
fn terminal_title_prompt_return_resets_auto_command_title_to_default() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project_dir = std::env::temp_dir();
    let project_path = project_dir.to_str().expect("project path").to_string();
    let project = repository
        .create_project(
            json!({ "name": "Prompt Reset", "path": &project_path })
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
                "cwd": &project_path,
                "kind": "terminal",
                "projectId": project_id,
                "runtimeSettings": { "titleSource": "terminal-auto" },
                "title": "top"
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
            "rawTitle": &project_path,
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
        Some(&json!("terminal-title-returned-to-prompt"))
    );
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("Terminal Session")));
    assert_eq!(
        session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("titleSource")),
        Some(&json!("placeholder"))
    );
}

#[test]
fn terminal_title_prompt_return_preserves_user_title() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project_dir = std::env::temp_dir();
    let project_path = project_dir.to_str().expect("project path").to_string();
    let project = repository
        .create_project(
            json!({ "name": "User Title Preserve", "path": &project_path })
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
                "cwd": &project_path,
                "kind": "terminal",
                "projectId": project_id,
                "runtimeSettings": { "titleSource": "user" },
                "title": "My Custom Terminal"
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
            "rawTitle": &project_path,
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = output.result;
    assert_eq!(result.get("changed"), Some(&json!(false)));
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("My Custom Terminal")));
    assert_eq!(
        session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("titleSource")),
        Some(&json!("user"))
    );
}

#[test]
fn terminal_title_prompt_return_does_not_reset_agent_session() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project_dir = std::env::temp_dir();
    let project_path = project_dir.to_str().expect("project path").to_string();
    let project = repository
        .create_project(
            json!({ "name": "Agent Session Preserve", "path": &project_path })
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
                "cwd": &project_path,
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "claude",
                    "titleSource": "terminal-auto"
                },
                "title": "Claude Working Title"
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
            "agentName": "claude",
            "rawTitle": &project_path,
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = output.result;

    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("Claude Working Title")));
}

#[test]
fn terminal_title_prompt_return_does_not_reset_cursor_placeholder() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project_dir = std::env::temp_dir();
    let project_path = project_dir.to_str().expect("project path").to_string();
    let project = repository
        .create_project(
            json!({ "name": "Cursor Preserve", "path": &project_path })
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
                "cwd": &project_path,
                "kind": "terminal",
                "projectId": project_id,
                "runtimeSettings": { "titleSource": "terminal-auto" },
                "title": "Running Task"
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
            "rawTitle": "Cursor Agent",
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = output.result;
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("Running Task")));
    assert_ne!(
        result.get("reason"),
        Some(&json!("terminal-title-returned-to-prompt"))
    );
}

#[test]
fn terminal_title_prompt_or_directory_matches_windows_absolute_paths() {
    let session = json!({
        "cwd": "C:\\Users\\test\\my-project",
        "kind": "terminal"
    });
    assert!(is_terminal_prompt_or_directory_title(
        &session,
        Some("C:\\Users\\test\\my-project")
    ));
    assert!(is_terminal_prompt_or_directory_title(
        &session,
        Some("\\\\wsl$\\Ubuntu\\home\\user")
    ));
}
