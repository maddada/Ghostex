use super::*;

/*
Real-data sanity checks: decode this machine's actual transcripts when
they exist and assert non-trivial role coverage. Skipped silently on
machines without the files.
*/
#[test]
fn decodes_real_claude_transcript_when_present() {
    let path = home_dir()
        .join(".claude/projects/-Users-madda-dev--active-Ghostex")
        .join("a3aadc10-7b82-417a-96de-e5ca56ae2e3e.jsonl");
    if !path.is_file() {
        return;
    }
    let result = read_session_chat_transcript_tail_file(
        &path,
        100_000,
        decode_claude_transcript_line,
        true,
        None,
        Some(decode_claude_turn_lifecycle),
        Some(claude_transcript_lineage),
    )
    .expect("read real claude transcript");
    let users = result
        .messages
        .iter()
        .filter(|message| message.role == SessionChatRole::User)
        .count();
    let assistants = result
        .messages
        .iter()
        .filter(|message| message.role == SessionChatRole::Assistant)
        .count();
    let tools = result
        .messages
        .iter()
        .filter(|message| message.role == SessionChatRole::Tool)
        .count();
    eprintln!(
        "real claude transcript: {} messages ({} user / {} assistant / {} tool), lifecycle: {:?}, malformed {} oversized {}",
        result.messages.len(),
        users,
        assistants,
        tools,
        result.lifecycle.as_ref().map(|lifecycle| lifecycle.state),
        result.malformed_record_count,
        result.oversized_record_count,
    );
    assert!(users > 0, "expected user messages, got {users}");
    assert!(
        assistants > 0,
        "expected assistant messages, got {assistants}"
    );
    assert!(tools > 0, "expected tool messages, got {tools}");
}

#[test]
fn decodes_real_codex_transcript_when_present() {
    let root = home_dir().join(".codex/sessions/2026");
    if !root.is_dir() {
        return;
    }
    let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            if newest
                .as_ref()
                .map(|(_, newest_modified)| modified > *newest_modified)
                .unwrap_or(true)
            {
                newest = Some((path, modified));
            }
        }
    }
    let Some((path, _)) = newest else {
        return;
    };
    let result = read_session_chat_transcript_tail_file(
        &path,
        100_000,
        decode_codex_transcript_line,
        true,
        None,
        Some(decode_codex_turn_lifecycle),
        None,
    )
    .expect("read real codex transcript");
    let users = result
        .messages
        .iter()
        .filter(|message| message.role == SessionChatRole::User)
        .count();
    let tools = result
        .messages
        .iter()
        .filter(|message| message.role == SessionChatRole::Tool)
        .count();
    let assistants = result
        .messages
        .iter()
        .filter(|message| message.role == SessionChatRole::Assistant)
        .count();
    let reasoning = result
        .messages
        .iter()
        .filter(|message| message.role == SessionChatRole::Reasoning)
        .count();
    eprintln!(
        "real codex transcript {}: {} messages ({} user / {} assistant / {} reasoning / {} tool), lifecycle: {:?}",
        path.display(),
        result.messages.len(),
        users,
        assistants,
        reasoning,
        tools,
        result.lifecycle.as_ref().map(|lifecycle| lifecycle.state),
    );
    assert!(
        users > 0,
        "expected user messages in {}, got {users}",
        path.display()
    );
    // The newest rollout is whatever Codex chat ran last, and a chat that never called a
    // tool is valid, so tool rows are only required when the file records a tool output.
    let records_tool_output = fs::read_to_string(&path)
        .expect("read real codex transcript text")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .any(|record| {
            matches!(
                record
                    .pointer("/payload/type")
                    .and_then(|kind| kind.as_str()),
                Some("function_call_output" | "custom_tool_call_output")
            )
        });
    if records_tool_output {
        assert!(
            tools > 0,
            "expected tool outputs in {}, got {tools}",
            path.display()
        );
    }
}

/*
Live-files probe for the 2026-08-02 repro: Ghostex session G1ipk still stores
`agentSessionId = f8ba5a62…` while the pane runs the background-job
continuation `fb7572ef…`. The directory also holds a SECOND continuation of
the same predecessor (`d34a3f3c…`, last substantive record 2026-08-01) plus
~180 unrelated transcripts, so this asserts the detector picks exactly the
live one. Skipped on machines without those files.
*/
#[test]
fn real_stale_session_resolves_to_its_live_successor_when_present() {
    let stale_session_id = "f8ba5a62-94b4-410f-ba6e-4f462a3e55bc";
    let expected_successor_id = "fb7572ef-2965-4e5e-b21e-bb0e3c455b66";
    let stale_path = home_dir()
        .join(".claude/projects/-Users-madda-dev--active-Ghostex")
        .join(format!("{stale_session_id}.jsonl"));
    let successor_path = stale_path.with_file_name(format!("{expected_successor_id}.jsonl"));
    if !stale_path.is_file() || !successor_path.is_file() {
        return;
    }
    let stale_last_substantive_ms = last_substantive_transcript_timestamp_ms(&stale_path)
        .expect("stale transcript has a substantive record");
    let successor_last_substantive_ms = last_substantive_transcript_timestamp_ms(&successor_path)
        .expect("successor transcript has a substantive record");
    eprintln!(
        "stale last substantive {stale_last_substantive_ms}, successor last substantive {successor_last_substantive_ms}"
    );
    // Live-files drift guard (2026-08-07): the "stale" transcript keeps
    // receiving fresh substantive records on this machine, so the
    // staleness premise can expire. Never freeze relationships between
    // live files in tests — skip when the repro state is gone.
    if successor_last_substantive_ms <= stale_last_substantive_ms {
        eprintln!("skipping: stale transcript is no longer stale on this machine");
        return;
    }
    match find_claude_successor_transcript(
        stale_session_id,
        &stale_path,
        stale_last_substantive_ms,
        &[],
    ) {
        SessionChatSuccessorOutcome::Found(successor) => {
            eprintln!(
                "real successor: {} via {} ({} hop(s))",
                successor.agent_session_id,
                successor.lineage.as_str(),
                successor.hops
            );
            assert_eq!(successor.agent_session_id, expected_successor_id);
            assert_eq!(successor.path, successor_path);
        }
        other => panic!("expected the live successor, got {other:?}"),
    }
    /*
    Bound to another registry session ⇒ never adopted. These are LIVE
    transcripts that keep moving: the second real continuation
    (d34a3f3c…) participates as a fallback candidate only while it is
    still substantively newer than the stale transcript, so the
    drift-dependent shape is asserted conditionally (on 2026-08-02 the
    stale file gained a fresher substantive record and the fallback aged
    out of candidacy).
    */
    let second_continuation_id = "d34a3f3c-3947-4053-8811-af237c1ae288";
    let second_continuation_path =
        stale_path.with_file_name(format!("{second_continuation_id}.jsonl"));
    let second_continuation_ms =
        last_substantive_transcript_timestamp_ms(&second_continuation_path)
            .expect("second continuation has a substantive record");
    if second_continuation_ms > stale_last_substantive_ms {
        assert_eq!(
            find_claude_successor_transcript(
                stale_session_id,
                &stale_path,
                stale_last_substantive_ms,
                &[expected_successor_id.to_string()],
            ),
            SessionChatSuccessorOutcome::Found(SessionChatSuccessorTranscript {
                agent_session_id: second_continuation_id.to_string(),
                path: second_continuation_path.clone(),
                lineage: SessionChatSuccessorLineage::PredecessorIdField,
                last_substantive_ms: second_continuation_ms,
                hops: 1,
            }),
            "excluding the live successor must fall to the other proven continuation, never to an unrelated file",
        );
        /*
        The 2026-08-02 runtime failure in one assertion: BOTH real
        continuations are also carried by long-STOPPED registry rows
        (G7vq1 / G1z1z here). Feeding those in as owners is what produced
        a silent no-op, which is why the ownership list must contain
        active sessions only — and why this case now reports
        OwnedByAnotherSession instead of NotFound.
        */
        assert_eq!(
            find_claude_successor_transcript(
                stale_session_id,
                &stale_path,
                stale_last_substantive_ms,
                &[
                    expected_successor_id.to_string(),
                    second_continuation_id.to_string(),
                ],
            ),
            SessionChatSuccessorOutcome::OwnedByAnotherSession {
                candidate_session_ids: vec![
                    expected_successor_id.to_string(),
                    second_continuation_id.to_string(),
                ],
            },
        );
    } else {
        // Drifted world: with the fallback aged out, excluding the live
        // successor must leave no adoptable candidate — the exclusion
        // itself is the invariant under test.
        assert_eq!(
            find_claude_successor_transcript(
                stale_session_id,
                &stale_path,
                stale_last_substantive_ms,
                &[expected_successor_id.to_string()],
            ),
            SessionChatSuccessorOutcome::OwnedByAnotherSession {
                candidate_session_ids: vec![expected_successor_id.to_string()],
            },
        );
    }
}
