use super::*;
use crate::session_chat_decode_codex::{decode_codex_transcript_line, decode_codex_turn_lifecycle};
use crate::session_chat_follower::{follower_drain_once, FollowerDrainOutcome, FollowerFileState};
use std::io::Write;

fn append(path: &Path, row: Value) {
    writeln!(
        fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .unwrap(),
        "{row}"
    )
    .unwrap();
}

fn message(id: &str) -> Value {
    json!({"type":"event_msg", "timestamp":"2026-09-11T00:00:00Z", "payload":{"type":"agent_message", "id":id, "message":id}})
}

#[test]
fn fork_snapshot_inherits_history_before_first_prompt_and_keeps_child_stream_position() {
    let temp = tempfile::tempdir().unwrap();
    let day = temp.path().join("sessions/2026/09/11");
    fs::create_dir_all(&day).unwrap();
    let parent_id = "11111111-1111-4111-8111-111111111111";
    let child_id = "22222222-2222-4222-8222-222222222222";
    let parent = day.join(format!("rollout-2026-09-11T00-00-00-{parent_id}.jsonl"));
    let child = day.join(format!("rollout-2026-09-11T00-01-00-{child_id}.jsonl"));
    append(
        &parent,
        json!({"type":"session_meta", "payload":{"id":parent_id}}),
    );
    append(&parent, message("inherited-1"));
    append(&parent, message("inherited-2"));
    let boundary = fs::metadata(&parent).unwrap().len();
    append(&parent, message("later-parent-reply"));
    append(
        &child,
        json!({"type":"session_meta", "timestamp":"2026-09-11T00:01:00Z", "payload":{
            "id":child_id, "forked_from_id":parent_id,
            "history_base":{"thread_id":parent_id,"end_byte_offset":boundary,"end_ordinal_exclusive":3}
        }}),
    );
    let mut state = FollowerFileState::new();
    let drain = |state: &mut FollowerFileState, limit, snapshot| {
        follower_drain_once(
            &child,
            limit,
            SessionChatTranscriptAgent::Codex,
            decode_codex_transcript_line,
            Some(decode_codex_turn_lifecycle),
            state,
            snapshot,
        )
    };
    let FollowerDrainOutcome::Snapshot { tail, .. } = drain(&mut state, 20, true) else {
        panic!("snapshot expected")
    };
    assert_eq!(
        tail.messages
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "inherited-1",
            "inherited-2",
            &format!("fork-boundary:{child_id}")
        ]
    );
    assert!(!tail.has_more);
    assert_eq!(tail.consumed_to, fs::metadata(&child).unwrap().len());
    assert!(matches!(
        drain(&mut state, 20, false),
        FollowerDrainOutcome::Idle
    ));

    append(&child, message("child-reply"));
    let FollowerDrainOutcome::Appended { batches, .. } = drain(&mut state, 20, false) else {
        panic!("append expected")
    };
    assert_eq!(
        batches
            .iter()
            .flatten()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        vec!["child-reply"]
    );

    let FollowerDrainOutcome::Snapshot { tail, .. } = drain(&mut state, 2, true) else {
        panic!("snapshot expected")
    };
    assert!(tail.has_more);
    assert!(tail.before_offset >= ANCESTOR_CURSOR_BASE);
    let page = read_session_chat_tail_page_stitched(
        SessionChatTranscriptAgent::Codex,
        &child,
        20,
        Some(tail.before_offset),
    )
    .unwrap();
    let SessionChatTailPage::Page {
        messages, has_more, ..
    } = page.page
    else {
        panic!("page expected")
    };
    assert!(!has_more);
    assert_eq!(
        messages.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        vec!["inherited-1"]
    );
}
