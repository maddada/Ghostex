use serde_json::{json, Value};

use super::*;
use crate::domain::DomainRepository;

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
        &session,
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
        &session,
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
fn live_process_identity_replaces_wsl_shell_title_without_claiming_codex_auto_title_job() {
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
        &session,
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

    assert_ne!(
        result.get("reason"),
        Some(&json!("first-prompt-auto-title-claimed"))
    );
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

/// CDXC:SessionIdentity 2026-11-05: a real identity change writes one `sessionIdentity.changed`
/// line naming who made it, and the same observation again writes nothing.
#[test]
fn a_live_process_identity_change_is_logged_once() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S90");
    let (lifecycle, session) = create_agent_session(
        &repository,
        "claude",
        "086ffbff-18b6-4953-a742-44443b05a623",
        temp.path(),
    );
    let created = crate::session_identity_log::take_captured_identity_changes();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0]["previousAgentSessionId"], Value::Null);
    assert_eq!(
        created[0]["agentSessionId"],
        json!("086ffbff-18b6-4953-a742-44443b05a623")
    );

    let observe = |current: &Value| {
        apply_live_process_session_identity(
            &repository,
            current,
            &lifecycle.project_id,
            &lifecycle.session_id,
            Some("claude".to_string()),
            Some("4f56cc17-81b1-4ff1-922e-d1324add22f2".to_string()),
            None,
        )
        .expect("apply live process identity")
    };
    assert!(observe(&session));
    let lines = crate::session_identity_log::take_captured_identity_changes();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        lines[0]["sessionRef"],
        json!(format!(
            "S90:{}:{}",
            lifecycle.project_id, lifecycle.session_id
        ))
    );
    assert_eq!(
        lines[0]["previousAgentSessionId"],
        json!("086ffbff-18b6-4953-a742-44443b05a623")
    );
    assert_eq!(
        lines[0]["agentSessionId"],
        json!("4f56cc17-81b1-4ff1-922e-d1324add22f2")
    );
    assert_eq!(
        lines[0]["source"],
        json!("liveProcessScan > identity:liveProcess")
    );

    let current = repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)
        .expect("get session")
        .expect("session");
    observe(&current);
    assert_eq!(
        crate::session_identity_log::take_captured_identity_changes(),
        Vec::<Value>::new()
    );
}
