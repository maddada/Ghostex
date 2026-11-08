//! The voice agent's tools act as the client that opened the call (#495 follow-up, 2A), through
//! this computer's real gxserver on its own loopback port: the owner's devices answer prompts and
//! type into another session without asking, as their own clicks do; the token file and another
//! session's token ask, wait for the owner's signed approval, and go through when they repeat the
//! call with it.

use super::*;
use crate::security::{test_keys::TestApprover, SessionRole};
use crate::voice_agent::VoiceCallerForTests;
use ghostex_gx_protocol::approval::approval_signing_payload;

async fn this_computer() -> (tempfile::TempDir, Arc<AppState>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("address").port();
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    std::fs::create_dir_all(paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
    let state = test_app_state_on(paths, "S6l", Some(port));
    let served = state.clone();
    let app = axum::Router::new().fallback_service(tower::service_fn(move |request| {
        handle_http_request(served.clone(), request)
    }));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (temp, state)
}

fn typing() -> Value {
    json!({ "projectId": "P1", "sessionId": "G2", "text": "1" })
}

fn answering() -> Value {
    json!({ "projectId": "P1", "sessionId": "G2", "kind": "terminalChoice", "choiceIndex": 0 })
}

fn asked(result: &Result<Value, String>) -> bool {
    result
        .as_ref()
        .err()
        .is_some_and(|message| message.contains("needs the user's approval"))
}

/// The owner approves (or denies) the request `id` from the desktop, with its key.
async fn decide(state: &Arc<AppState>, approver: &TestApprover, owner: &str, id: &str, deny: bool) {
    let request = state
        .security
        .approvals
        .read(
            id,
            &crate::security::Principal::Owner {
                device_id: "mac".into(),
            },
            crate::security::now_ms(),
        )
        .expect("the request");
    let (path, params) = if deny {
        ("/api/denyApprovalRequest", json!({ "id": id }))
    } else {
        let signature = approver.sign(&approval_signing_payload("S1", &request).expect("payload"));
        (
            "/api/approveApprovalRequest",
            json!({ "id": id, "keyId": approver.key.key_id, "signature": signature }),
        )
    };
    let routed = route_http(
        state.clone(),
        rpc_request(path, owner, json!({ "params": params })),
        "decide".to_string(),
    )
    .await;
    assert_eq!(routed.response.status(), StatusCode::OK, "{path}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_owners_voice_call_answers_and_types_without_asking() {
    let (_temp, state) = this_computer().await;
    let owner = state.security.session_tokens.mint_owner("mac");
    // A paired phone's own token.
    let device = crate::auth::mint_device_api_token();
    state
        .device_api_tokens
        .replace([crate::auth::hash_device_api_token(&device)]);
    for token in [&owner, &device] {
        let caller = VoiceCallerForTests::new(Some(token));
        for (path, params) in [
            ("/api/sendSessionText", typing()),
            ("/api/answerSessionChatPrompt", answering()),
            (
                "/api/interruptSessionChat",
                json!({ "projectId": "P1", "sessionId": "G2" }),
            ),
        ] {
            let result = caller.call(&state, path, params).await;
            assert!(!asked(&result), "{path}: {result:?}");
            assert_eq!(caller.pending_id(), None, "{path}");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_token_file_voice_call_waits_for_the_approval_then_goes_through() {
    let (_temp, state) = this_computer().await;
    let approver = TestApprover::new("mac");
    state
        .security
        .approver_keys
        .insert(approver.key.clone())
        .expect("key");
    let owner = state.security.session_tokens.mint_owner("mac");

    // The token file (the CLI's `ghostex voice` outside a session, the web build) asks.
    let caller = VoiceCallerForTests::new(None);
    let first = caller.call(&state, "/api/sendSessionText", typing()).await;
    assert!(asked(&first), "{first:?}");
    let id = caller.pending_id().expect("a pending approval");
    // It is an agent's request, so the desktop shows it as a card (owner_security/ui.rs).
    let request = state
        .security
        .approvals
        .read(
            &id,
            &crate::security::Principal::LocalAgent,
            crate::security::now_ms(),
        )
        .expect("the request");
    assert_ne!(request.requested_by.kind, "owner");

    let owner_for_decide = owner.clone();
    let deciding = {
        let state = state.clone();
        let id = id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            decide(&state, &approver, &owner_for_decide, &id, false).await;
        })
    };
    assert_eq!(caller.decide(&state).await, "approved");
    deciding.await.expect("decided");
    let retry = caller.call(&state, "/api/sendSessionText", typing()).await;
    assert!(!asked(&retry), "{retry:?}");
    assert_eq!(caller.pending_id(), Some(id), "nothing new to approve");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_session_voice_call_asks_for_another_session_and_a_denial_ends_it() {
    let (_temp, state) = this_computer().await;
    let owner = state.security.session_tokens.mint_owner("mac");
    let session = state
        .security
        .session_tokens
        .mint_session("P1", "G1", SessionRole::Agent);

    // Its own terminal: no question.
    let caller = VoiceCallerForTests::new(Some(&session));
    let own = caller
        .call(
            &state,
            "/api/sendSessionText",
            json!({ "projectId": "P1", "sessionId": "G1", "text": "1" }),
        )
        .await;
    assert!(!asked(&own), "{own:?}");

    let caller = VoiceCallerForTests::new(Some(&session));
    let other = caller
        .call(&state, "/api/answerSessionChatPrompt", answering())
        .await;
    assert!(asked(&other), "{other:?}");
    let id = caller.pending_id().expect("a pending approval");
    let deciding = {
        let state = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            decide(&state, &TestApprover::new("unused"), &owner, &id, true).await;
        })
    };
    assert_eq!(caller.decide(&state).await, "denied");
    deciding.await.expect("decided");
}

/// A terminal-only session the voice tools can name ("Build box").
fn terminal_session(state: &Arc<AppState>, cwd: &std::path::Path) -> (String, String) {
    let db = crate::storage::open_gxserver_database(&state.paths).expect("open db");
    let repository = crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project = repository
        .create_project(
            json!({ "name": "demo-app", "path": cwd })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let session = repository
        .create_session(
            json!({
                "projectId": project["projectId"],
                "kind": "terminal",
                "title": "Build box",
                "lifecycleState": "running",
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    (
        project["projectId"]
            .as_str()
            .expect("projectId")
            .to_string(),
        session["sessionId"]
            .as_str()
            .expect("sessionId")
            .to_string(),
    )
}

/// The whole tool, as a call opened with the token file runs it: it answers at once that it waits
/// for the user's approval (which the model says), the request waits as a card, and once the owner
/// approves it the tool runs again with the approval and the conversation hears how it went.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_voice_tool_says_it_waits_then_announces_the_approved_result() {
    let (temp, state) = this_computer().await;
    let approver = TestApprover::new("mac");
    state
        .security
        .approver_keys
        .insert(approver.key.clone())
        .expect("key");
    let owner = state.security.session_tokens.mint_owner("mac");
    let (project_id, session_id) = terminal_session(&state, temp.path());
    let args = json!({ "session": "Build box", "text": "ls" });

    // The owner's call types it at once.
    let (output, _) = crate::voice_agent::run_tool_in_call_for_tests(
        &state,
        Some(&owner),
        "draft_prompt",
        args.clone(),
    )
    .await;
    assert!(output.get("waitingForApproval").is_none(), "{output}");
    assert!(!output.to_string().contains("approval"), "{output}");

    // The token file's call waits for the approval.
    let (output, mut announced) =
        crate::voice_agent::run_tool_in_call_for_tests(&state, None, "draft_prompt", args).await;
    assert_eq!(output["waitingForApproval"], true, "{output}");
    assert!(output["note"]
        .as_str()
        .unwrap_or_default()
        .contains("approval"));
    let pending = state
        .security
        .approvals
        .list_pending(crate::security::now_ms());
    let request = pending
        .iter()
        .find(|request| request.target.as_deref() == Some(&format!("{project_id}:{session_id}")))
        .expect("the request waits as a card");
    decide(&state, &approver, &owner, &request.id, false).await;
    let (text, model_text) = tokio::time::timeout(Duration::from_secs(15), announced.recv())
        .await
        .expect("an announcement")
        .expect("announced");
    // Approved and run again with the approval, not asked again. (No terminal stands behind the
    // test's session, so the typing itself fails after the gate.)
    assert!(text.starts_with("Approved"), "{text}");
    let model_text = model_text.unwrap_or_default();
    assert!(model_text.contains("The user approved it"), "{model_text}");
    assert!(
        !model_text.contains("needs the user's approval"),
        "{model_text}"
    );
    assert!(state
        .security
        .approvals
        .list_pending(crate::security::now_ms())
        .is_empty());
}
