//! Timing check for refreshing a long transcript (CDXC:SessionChat 2026-11-06 in
//! `transcript/read_images.rs` and `transcript/settled.rs`).
//!
//! Ignored by default: it needs a real long chat, exported read-only with
//! `ghostex read-session-chat <ref> --all --format json > chat.json`, and runs in release:
//! `GX_CHAT_TIMING_FIXTURE=chat.json cargo test --release -p ghostex-gx-chat-core --test long_transcript_refresh_timing -- --ignored --nocapture`.

use ghostex_gx_chat_core::{ChatContext, ChatState, KnownSession};
use ghostex_gx_protocol::ChatMessage;
use std::time::{Duration, Instant};

/// A host directory the size of a busy computer's: every session the sidebar lists.
fn directory(renamed: usize) -> Vec<KnownSession> {
    (0..400)
        .map(|index| KnownSession {
            global_ref: format!("S90:P{index:04}:G{index:04}"),
            project_id: format!("P{index:04}"),
            session_id: format!("G{index:04}"),
            title: if index == 7 {
                format!("Thread number {index} rename {renamed}")
            } else {
                format!("Thread number {index}")
            },
            agent_id: "claude".into(),
            agent_name: "Claude".into(),
            ..Default::default()
        })
        .collect()
}

fn timed(label: &str, runs: u32, mut run: impl FnMut()) -> Duration {
    let started = Instant::now();
    for _ in 0..runs {
        run();
    }
    let each = started.elapsed() / runs;
    println!("{each:>12.2?}  {label}");
    each
}

#[test]
#[ignore = "needs GX_CHAT_TIMING_FIXTURE, a long chat exported with read-session-chat"]
fn a_long_transcript_refreshes_without_rebuilding_what_is_settled() {
    let Ok(path) = std::env::var("GX_CHAT_TIMING_FIXTURE") else {
        eprintln!("GX_CHAT_TIMING_FIXTURE is not set; skipping");
        return;
    };
    let export: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("the fixture")).expect("JSON");
    let rows: Vec<ChatMessage> =
        serde_json::from_value(export["messages"].clone()).expect("chat messages");
    println!("{} messages", rows.len());
    let mut state = ChatState::default();
    state.session.agent = export["agent"].as_str().map(str::to_string);
    state.messages.composed = rows.clone();
    state.messages.list = rows;
    state.transcript_view.session_directory = directory(0);
    state.transcript_view.session_directory_revision = 1;
    let context = ChatContext {
        now_ms: 1_800_000_000_000.0,
        ..Default::default()
    };
    let refresh = |state: &mut ChatState| {
        ghostex_gx_chat_core::transcript::rows::refresh(state, &context);
    };

    timed("first refresh (whole build and settle)", 1, || {
        refresh(&mut state)
    });
    assert!(
        state.transcript_view.settled.is_some(),
        "the prefix settles"
    );
    let rows_before = state.transcript_view.items.len();

    // A refresh the settled prefix serves: only the rows after it are projected.
    let mut revision = 0;
    let reused = timed("refresh on the settled prefix", 20, || {
        revision += 1;
        state.transcript_view.detail_revision += 1;
        refresh(&mut state);
    });

    // A session renamed: the cards that name it read again, which settles the prefix afresh.
    let mut renamed = 0;
    let resettled = timed("refresh after a session rename (settled afresh)", 5, || {
        renamed += 1;
        state.transcript_view.session_directory = directory(renamed);
        state.transcript_view.session_directory_revision += 1;
        refresh(&mut state);
    });

    // The directory reordered with the same names (a thread became newest): nothing to rebuild.
    let mut reordered = directory(renamed);
    reordered.reverse();
    let directory_events = timed("session directory reordered, same names", 20, || {
        reordered.reverse();
        let sessions = reordered.clone();
        if !KnownSession::same_names(&state.transcript_view.session_directory, &sessions) {
            state.transcript_view.session_directory_revision += 1;
        }
        state.transcript_view.session_directory = sessions;
        refresh(&mut state);
    });

    assert_eq!(state.transcript_view.items.len(), rows_before);
    println!("reused {reused:?}, resettled {resettled:?}, reordered {directory_events:?}");
}

#[test]
fn a_reordered_session_list_names_every_session_as_before() {
    let sessions = directory(0);
    let mut reordered = sessions.clone();
    reordered.reverse();
    assert!(KnownSession::same_names(&sessions, &reordered));
    // The `+` picker's live fields are not names.
    reordered[3].activity = "working".into();
    reordered[3].active_at = "2026-11-06T09:00:00Z".into();
    assert!(KnownSession::same_names(&sessions, &reordered));
    assert!(!KnownSession::same_names(&sessions, &directory(1)));
    assert!(!KnownSession::same_names(&sessions, &sessions[1..]));
}

/// Sessions that share a title are named alike in any order, so the sidebar moving one above its
/// twin settles no transcript again (CDXC:SessionChat 2026-11-11 in `transcript/sent_message.rs`
/// on `HostDirectory::new`). A real change to a name still counts.
#[test]
fn sessions_sharing_a_title_are_named_alike_in_any_order() {
    let mut sessions = directory(0);
    for index in [1, 2, 3, 40, 41] {
        sessions[index].title = "Claude Session".into();
    }
    for index in [10, 11] {
        sessions[index].title = String::new();
    }
    let mut swapped = sessions.clone();
    swapped.swap(1, 2);
    swapped.swap(3, 41);
    swapped.swap(10, 11);
    assert!(KnownSession::same_names(&sessions, &swapped));
    let mut reversed = sessions.clone();
    reversed.reverse();
    assert!(KnownSession::same_names(&sessions, &reversed));
    // One of the twins renamed, or closed: the names did change.
    let mut renamed = swapped.clone();
    renamed[1].title = "Release notes".into();
    assert!(!KnownSession::same_names(&sessions, &renamed));
    assert!(!KnownSession::same_names(&sessions, &swapped[1..]));
}
