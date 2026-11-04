use super::*;

#[test]
fn stream_frames_are_published_in_seq_order() {
    use std::sync::Mutex;
    let stream = Arc::new(SessionChatStream::new());
    stream.begin_generation();
    let published: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let stream = stream.clone();
        let published = published.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..50 {
                stream.emit_sequenced(
                    |seq| json!(seq),
                    |frame| {
                        published
                            .lock()
                            .expect("publish lock")
                            .push(frame.as_i64().expect("seq"));
                    },
                );
            }
        }));
    }
    for handle in handles {
        handle.join().expect("thread");
    }
    let published = published.lock().expect("publish lock").clone();
    assert_eq!(published.len(), 400);
    assert!(
        published.windows(2).all(|pair| pair[0] < pair[1]),
        "frames must reach the hub in seq order",
    );
}

pub(super) fn write_temp_transcript(lines: &[&str]) -> PathBuf {
    // Tests run in parallel: the name must be unique per CALL, not per
    // line count, or two tests share (and then delete) one file.
    static NEXT_TEMP_TRANSCRIPT: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "gxserver-session-chat-test-{}-{}.jsonl",
        std::process::id(),
        NEXT_TEMP_TRANSCRIPT.fetch_add(1, Ordering::SeqCst),
    ));
    let mut file = File::create(&path).expect("create temp transcript");
    for line in lines {
        writeln!(file, "{line}").expect("write line");
    }
    path
}

#[test]
fn tail_reader_limit_and_has_more_semantics() {
    let lines: Vec<String> = (0..10)
        .map(|index| {
            format!(
                r#"{{"type":"user","uuid":"u{index}","message":{{"role":"user","content":"prompt {index}"}}}}"#
            )
        })
        .collect();
    let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let path = write_temp_transcript(&line_refs);

    let all = read_session_chat_transcript_tail_file(
        &path,
        300,
        decode_claude_transcript_line,
        true,
        None,
        Some(decode_claude_turn_lifecycle),
        Some(claude_transcript_lineage),
    )
    .expect("tail read");
    assert_eq!(all.messages.len(), 10);
    assert!(!all.has_more);
    assert_eq!(all.before_offset, 0);
    assert_eq!(
        all.lifecycle.as_ref().map(|lifecycle| lifecycle.state),
        Some(SessionChatTurnLifecycleState::Working)
    );

    let limited = read_session_chat_transcript_tail_file(
        &path,
        3,
        decode_claude_transcript_line,
        true,
        None,
        None,
        Some(claude_transcript_lineage),
    )
    .expect("limited read");
    assert_eq!(limited.messages.len(), 3);
    assert!(limited.has_more);
    assert_eq!(
        limited.messages.last().map(|message| message.id.as_str()),
        Some("u9")
    );

    // limit 0 ⇒ [] (never "everything").
    let zero = read_session_chat_transcript_tail_file(
        &path,
        0,
        decode_claude_transcript_line,
        true,
        None,
        None,
        Some(claude_transcript_lineage),
    )
    .expect("zero read");
    assert!(zero.messages.is_empty());
    assert!(!zero.has_more);

    // Paging by beforeOffset excludes the newer window.
    let older = read_session_chat_transcript_tail_file(
        &path,
        300,
        decode_claude_transcript_line,
        true,
        Some(limited.before_offset),
        None,
        Some(claude_transcript_lineage),
    )
    .expect("older page");
    assert_eq!(older.messages.len(), 7);
    assert_eq!(
        older.messages.last().map(|message| message.id.as_str()),
        Some("u6")
    );

    let _ = fs::remove_file(&path);
}

#[test]
fn incremental_reader_resumes_from_offset() {
    let path = write_temp_transcript(&[
        r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"one"}}"#,
    ]);
    let mut state = SessionChatIncrementalState::new();
    let first = read_incremental_transcript_messages(
        &path,
        &mut state,
        decode_claude_transcript_line,
        None,
        None,
        None,
        Some(claude_transcript_lineage),
    )
    .expect("first pass");
    assert_eq!(first.len(), 1);
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("append");
    writeln!(
        file,
        r#"{{"type":"assistant","uuid":"a1","message":{{"role":"assistant","content":[{{"type":"text","text":"two"}}]}}}}"#
    )
    .expect("append line");
    drop(file);
    let second = read_incremental_transcript_messages(
        &path,
        &mut state,
        decode_claude_transcript_line,
        None,
        None,
        None,
        Some(claude_transcript_lineage),
    )
    .expect("second pass");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].id, "a1");
    let _ = fs::remove_file(&path);
}

/// Claude records a message that reaches it mid-turn where the model took it, after the tool
/// result it rode in with, but stamped with the time it was queued; the readers stamp it no
/// earlier than the row before it (`stamp_user_row_after_predecessor`), so clients that order by
/// time keep it after the send it followed.
#[test]
fn a_message_delivered_mid_turn_is_stamped_after_the_row_before_it() {
    let lines = [
        r#"{"type":"user","uuid":"u1","timestamp":"2026-10-31T04:16:00.000Z","message":{"role":"user","content":"Tell the thread to rebuild."}}"#,
        r#"{"type":"assistant","uuid":"a1","timestamp":"2026-10-31T04:17:19.088Z","message":{"id":"m1","role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ghostex agents send S90:P3lv0:G1co8 --queue \"Go ahead.\""}}]}}"#,
        r#"{"type":"user","uuid":"r1","timestamp":"2026-10-31T04:17:22.919Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"  \"status\": \"queued\","}]}}"#,
        r#"{"type":"attachment","uuid":"q1","timestamp":"2026-10-31T04:17:01.570Z","attachment":{"type":"queued_command","prompt":"Ghostex thread report: finished its turn.","timestamp":"2026-10-31T04:17:01.570Z"}}"#,
        r#"{"type":"assistant","uuid":"a2","timestamp":"2026-10-31T04:17:39.107Z","message":{"id":"m2","role":"assistant","content":[{"type":"text","text":"The rebuild is running."}]}}"#,
    ];
    let stamp = |iso: &str| {
        chrono::DateTime::parse_from_rfc3339(iso)
            .unwrap()
            .timestamp_millis()
    };
    let times = |messages: &[SessionChatMessage]| -> Vec<(String, Option<i64>)> {
        messages
            .iter()
            .map(|message| (message.id.clone(), message.timestamp))
            .collect()
    };
    let expected = vec![
        ("u1".to_string(), Some(stamp("2026-10-31T04:16:00.000Z"))),
        ("a1".to_string(), Some(stamp("2026-10-31T04:17:19.088Z"))),
        ("r1".to_string(), Some(stamp("2026-10-31T04:17:22.919Z"))),
        // The report: queued at 04:17:01.570, taken with r1's result.
        ("q1".to_string(), Some(stamp("2026-10-31T04:17:22.919Z"))),
        ("a2".to_string(), Some(stamp("2026-10-31T04:17:39.107Z"))),
    ];
    let path = write_temp_transcript(&lines);

    let tail = read_session_chat_transcript_tail_file(
        &path,
        300,
        decode_claude_transcript_line,
        true,
        None,
        None,
        Some(claude_transcript_lineage),
    )
    .expect("tail read");
    assert_eq!(times(&tail.messages), expected);

    let mut state = SessionChatIncrementalState::new();
    let appended = read_incremental_transcript_messages(
        &path,
        &mut state,
        decode_claude_transcript_line,
        None,
        None,
        None,
        Some(claude_transcript_lineage),
    )
    .expect("incremental read");
    assert_eq!(times(&appended), expected);

    // The append stream after a snapshot knows the newest row only from the tail read it was
    // seeded with, which is how the follower hands it over.
    let snapshot = write_temp_transcript(&lines[..3]);
    let tail = read_session_chat_transcript_tail_file(
        &snapshot,
        300,
        decode_claude_transcript_line,
        true,
        None,
        None,
        Some(claude_transcript_lineage),
    )
    .expect("snapshot read");
    let mut state = SessionChatIncrementalState::new();
    state.rebase(tail.consumed_to);
    state.seed_last_message_timestamp(tail.messages.last().and_then(|message| message.timestamp));
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&snapshot)
        .expect("append");
    writeln!(file, "{}", lines[3]).expect("append line");
    drop(file);
    let appended = read_incremental_transcript_messages(
        &snapshot,
        &mut state,
        decode_claude_transcript_line,
        None,
        None,
        None,
        Some(claude_transcript_lineage),
    )
    .expect("append read");
    assert_eq!(times(&appended), vec![expected[3].clone()]);
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&snapshot);
}

/// CDXC:SessionChat 2026-11-02: a Codex turn's reasoning summaries join into one Thought row in
/// the tail window, and the append stream sends that row again, grown, under its first id
/// (`session_chat_codex_thoughts.rs`).
#[test]
fn codex_summaries_join_into_one_thought_row_across_tail_and_append() {
    let reasoning = |text: &str| {
        format!(
            r#"{{"type":"response_item","payload":{{"type":"reasoning","summary":[{{"type":"summary_text","text":"{text}"}}]}}}}"#
        )
    };
    let call = |id: &str| {
        format!(
            r#"{{"type":"response_item","payload":{{"type":"function_call","name":"exec_command","arguments":"{{}}","call_id":"{id}"}}}}"#
        )
    };
    let output = |id: &str| {
        format!(
            r#"{{"type":"response_item","payload":{{"type":"function_call_output","call_id":"{id}","output":"ok"}}}}"#
        )
    };
    let path = write_temp_transcript(&[
        r#"{"type":"session_meta","payload":{"id":"s1"}}"#,
        r#"{"type":"event_msg","payload":{"type":"user_message","message":"check the settings"}}"#,
        &reasoning("**Checking agent configuration**"),
        &call("c1"),
        &output("c1"),
        &reasoning("**Checking permission configuration**"),
        &call("c2"),
    ]);
    let tail = read_session_chat_transcript_tail_file(
        &path,
        300,
        decode_codex_transcript_line,
        false,
        None,
        None,
        None,
    )
    .expect("tail read");
    let roles: Vec<SessionChatRole> = tail
        .messages
        .iter()
        .map(|message| message.role.clone())
        .collect();
    assert_eq!(
        roles,
        [
            SessionChatRole::User,
            SessionChatRole::Reasoning,
            SessionChatRole::Assistant,
            SessionChatRole::Tool,
            SessionChatRole::Assistant,
        ]
    );
    let thought = tail.messages[1].clone();
    assert_eq!(
        thought.blocks,
        vec![text_block(
            "**Checking agent configuration**\n\n**Checking permission configuration**\n"
        )]
    );

    let mut state = SessionChatIncrementalState::new();
    state.rebase(tail.consumed_to);
    state.seed_codex_thoughts(tail.codex_thoughts.clone());
    {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open for append");
        for line in [
            output("c2"),
            reasoning("**Checking session model options**"),
            r#"{"type":"event_msg","payload":{"type":"agent_message","message":"Both look fine."}}"#
                .to_string(),
            reasoning("**Summarising**"),
        ] {
            writeln!(file, "{line}").expect("append line");
        }
    }
    let appended = read_incremental_transcript_messages(
        &path,
        &mut state,
        decode_codex_transcript_line,
        None,
        None,
        None,
        None,
    )
    .expect("forward read");
    let rows: Vec<(String, SessionChatRole)> = appended
        .iter()
        .map(|message| (message.id.clone(), message.role.clone()))
        .collect();
    assert_eq!(rows.len(), 4);
    // The grown row keeps its first id and offset, so the chat replaces it in place.
    assert_eq!(rows[1], (thought.id.clone(), SessionChatRole::Reasoning));
    assert_eq!(appended[1].byte_offset, thought.byte_offset);
    assert_eq!(
        appended[1].blocks,
        vec![text_block(
            "**Checking agent configuration**\n\n**Checking permission configuration**\n\n**Checking session model options**\n"
        )]
    );
    // The reply sentence closed the stretch: the next summary is a row of its own.
    assert_eq!(rows[2].1, SessionChatRole::Assistant);
    assert_ne!(rows[3].0, thought.id);
    assert_eq!(appended[3].blocks, vec![text_block("**Summarising**")]);
    let _ = fs::remove_file(&path);
}
