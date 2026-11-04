use super::*;
use crate::session_chat_follower::{
    subscribe_session_chat_follower, sync_session_chat_follower_for_session,
};

/// GitHub #410: an account login finishes on a plain thread, and `register` updates sessions while it holds `accounts.mutations`. A session update (re)starts the session's chat follower and zmx title observer, and `tokio::spawn` without a runtime panicked there and poisoned the gate, so every later account operation and chat send failed until gxserver restarted.
#[test]
fn a_session_update_on_a_plain_thread_holding_the_accounts_gate_does_not_panic() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    let session = json!({
        "agentId": "claude",
        "kind": "agent",
        "lifecycleState": "running",
        "projectId": "P9k9k",
        "sessionId": "G9mmz",
        "zmxName": "S7k-P9k9k-G9mmz",
    });
    // A chat window follows the session (no row is stored, so subscribing starts nothing yet).
    subscribe_session_chat_follower(&state, "P9k9k", "G9mmz", 300, true);

    let login_state = state.clone();
    let login_session = session.clone();
    let login = std::thread::spawn(move || {
        let _gate = login_state.accounts.mutation_gate();
        sync_session_chat_follower_for_session(&login_state, &login_session, "test");
        sync_zmx_title_observer_for_session(&login_state, &login_session, "test");
    });
    assert!(login.join().is_ok(), "the login thread panicked");
    let followers = state.session_chat_followers.lock().unwrap();
    assert!(followers.values().all(|entry| entry.task.is_none()));
    drop(followers);

    // The next update on a runtime thread starts the follower.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        sync_session_chat_follower_for_session(&state, &session, "test");
    });
    let followers = state.session_chat_followers.lock().unwrap();
    assert!(followers.values().all(|entry| entry.task.is_some()));
}
