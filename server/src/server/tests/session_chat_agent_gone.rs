//! An agent gone with no notice saying so (`agentGone`) wakes a long-polling reader: the
//! `readSessionChat` fingerprint moves when the flag turns on, so a phone over SSH reads it; and a
//! confirmed absence outlives a probe whose screen capture failed.

use serde_json::json;

use super::*;
use crate::domain::DomainRepository;
use crate::storage::open_gxserver_database;

#[test]
fn the_read_fingerprint_moves_when_the_agent_is_gone() {
    let temp = tempfile::tempdir().unwrap();
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let db = open_gxserver_database(&paths).unwrap();
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let project = repository
        .create_project(
            json!({"name":"Agent gone","path":temp.path()})
                .as_object()
                .unwrap(),
        )
        .unwrap();
    let project_id = project["projectId"].as_str().unwrap().to_string();
    let session = repository
        .create_session(
            json!({"projectId":project_id,"kind":"agent","agentId":"claude","cwd":temp.path(),
                "lifecycleState":"running"})
            .as_object()
            .unwrap(),
            false,
        )
        .unwrap();
    let session_id = session["sessionId"].as_str().unwrap().to_string();
    let fingerprint = || {
        crate::session_chat_read::resolve_session_chat_read_state(
            &state,
            &project_id,
            &session_id,
            None,
        )
        .unwrap()
        .fingerprint
    };
    let seed = |agent_gone: bool| {
        state.session_chat_option_cache.lock().unwrap().insert(
            crate::server::session_observer_key(&project_id, &session_id),
            crate::session_chat_options::SessionChatOptionCacheEntry {
                fetched_at: std::time::Instant::now(),
                projected_compacting: None,
                projected_fleet: None,
                projected_monitor: None,
                model_grace_started: None,
                retired_notice: None,
                value: crate::session_chat_options::SessionChatTerminalDetection {
                    captured: true,
                    attempted: true,
                    agent_gone,
                    ..Default::default()
                },
            },
        );
    };
    seed(false);
    // The first read settles what a read stores (the queue, the agents list).
    let _ = fingerprint();
    let running = fingerprint();
    assert_eq!(fingerprint(), running, "nothing changed, nothing moves");
    seed(true);
    assert_ne!(fingerprint(), running, "the agent going wakes the reader");
    seed(false);
    assert_eq!(
        fingerprint(),
        running,
        "a running agent keeps the fingerprint it had"
    );
}

/// A probe that could not read the screen (here there is no zmx session to capture) keeps a
/// confirmed absence instead of answering that the agent runs.
#[test]
fn a_failed_capture_keeps_a_confirmed_absence() {
    let temp = tempfile::tempdir().unwrap();
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let db = open_gxserver_database(&paths).unwrap();
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let project = repository
        .create_project(
            json!({"name":"Agent gone","path":temp.path()})
                .as_object()
                .unwrap(),
        )
        .unwrap();
    let project_id = project["projectId"].as_str().unwrap().to_string();
    let session = repository
        .create_session(
            json!({"projectId":project_id,"kind":"agent","agentId":"claude","cwd":temp.path(),
                "lifecycleState":"running"})
            .as_object()
            .unwrap(),
            false,
        )
        .unwrap();
    let session_id = session["sessionId"].as_str().unwrap().to_string();
    let detect = || {
        crate::session_chat_options::detect_session_chat_terminal_state(
            &repository,
            &crate::session_chat_options::session_chat_hook_state_directory(&paths),
            &project_id,
            &session_id,
            Some("claude"),
        )
    };
    let unread = detect();
    assert!(!unread.captured && !unread.agent_gone, "{unread:?}");
    crate::session_chat_start_failure::confirm_agent_gone_for_test(&project_id, &session_id);
    let failed = detect();
    assert!(!failed.captured, "the capture failed: {failed:?}");
    assert!(
        failed.agent_gone,
        "the confirmed absence stands: {failed:?}"
    );
    crate::session_chat_start_failure::forget_agent_gone(&project_id, &session_id);
    assert!(!detect().agent_gone, "a restarted agent is no longer gone");
}
