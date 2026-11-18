use super::*;

#[test]
fn embedded_id_scan_requires_a_records_own_session_id() {
    let target = "aaaaaaaa-1111-2222-3333-444444444444";
    let other = "bbbbbbbb-5555-6666-7777-888888888888";
    // A transcript that merely QUOTES the id (orchestrator prompt, hook
    // payload, tool output) must not be adopted.
    let quoting = write_temp_transcript(&[&format!(
        r#"{{"type":"user","sessionId":"{other}","message":{{"role":"user","content":"resume \"session_id\":\"{target}\" please"}}}}"#
    )]);
    assert!(!head_declares_session_id(&quoting, target));
    assert!(head_declares_session_id(&quoting, other));

    // Snake-case spelling counts, sidechain rows do not.
    let sidechain = write_temp_transcript(&[
        &format!(
            r#"{{"type":"user","isSidechain":true,"session_id":"{target}","message":{{"role":"user","content":"sub"}}}}"#
        ),
        r#"{"type":"summary"}"#,
    ]);
    assert!(!head_declares_session_id(&sidechain, target));

    let owned = write_temp_transcript(&[
        &format!(
            r#"{{"type":"user","session_id":"{target}","message":{{"role":"user","content":"go"}}}}"#
        ),
        r#"{"type":"summary"}"#,
        r#"{"type":"other"}"#,
    ]);
    assert!(head_declares_session_id(&owned, target));

    // `agent-*.jsonl` sub-agent transcripts record the PARENT session id in
    // every row and are usually the newest file in the directory.
    let mut sidechain_named = std::env::temp_dir();
    sidechain_named.push(format!("agent-{}.jsonl", std::process::id()));
    fs::write(
        &sidechain_named,
        format!(
            "{{\"type\":\"user\",\"sessionId\":\"{target}\",\"message\":{{\"role\":\"user\",\"content\":\"x\"}}}}\n"
        ),
    )
    .expect("write sidechain transcript");
    assert!(!head_declares_session_id(&sidechain_named, target));

    for path in [quoting, sidechain, owned, sidechain_named] {
        let _ = fs::remove_file(path);
    }
}

// ---------------------------------------------------------------------
// Successor-transcript detection fixtures
// ---------------------------------------------------------------------

const FIXTURE_STALE_ID: &str = "11111111-1111-4111-8111-111111111111";
const FIXTURE_SUCCESSOR_ID: &str = "22222222-2222-4222-8222-222222222222";
const FIXTURE_SECOND_SUCCESSOR_ID: &str = "33333333-3333-4333-8333-333333333333";
const FIXTURE_DECOY_ID: &str = "44444444-4444-4444-8444-444444444444";

fn successor_fixture_dir(name: &str) -> PathBuf {
    static NEXT_FIXTURE_DIR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "gxserver-session-chat-successor-{}-{}-{}",
        std::process::id(),
        name,
        NEXT_FIXTURE_DIR.fetch_add(1, Ordering::SeqCst),
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create fixture dir");
    path
}

fn write_fixture_transcript(directory: &Path, file_stem: &str, lines: &[String]) -> PathBuf {
    let path = directory.join(format!("{file_stem}.jsonl"));
    let mut body = String::new();
    for line in lines {
        body.push_str(line);
        body.push('\n');
    }
    fs::write(&path, body).expect("write fixture transcript");
    path
}

fn user_row(session_id: &str, uuid: &str, timestamp: &str, text: &str) -> String {
    format!(
        r#"{{"type":"user","sessionId":"{session_id}","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"user","content":"{text}"}}}}"#
    )
}

fn assistant_row(session_id: &str, uuid: &str, timestamp: &str, text: &str) -> String {
    format!(
        r#"{{"type":"assistant","sessionId":"{session_id}","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"assistant","stop_reason":"end_turn","content":[{{"type":"text","text":"{text}"}}]}}}}"#
    )
}

/// Claude copies the PREDECESSOR id into the resumed file's snake-case
/// `session_id` while the camelCase `sessionId` stays the new file's own id.
fn resume_fork_assistant_row(
    session_id: &str,
    predecessor_session_id: &str,
    uuid: &str,
    timestamp: &str,
) -> String {
    format!(
        r#"{{"type":"assistant","sessionId":"{session_id}","session_id":"{predecessor_session_id}","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"assistant","stop_reason":"end_turn","content":[{{"type":"text","text":"resumed"}}]}}}}"#
    )
}

fn continuation_marker_row(session_id: &str, uuid: &str, timestamp: &str) -> String {
    format!(
        r#"{{"type":"user","sessionId":"{session_id}","isCompactSummary":true,"uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"user","content":[{{"type":"text","text":"This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation."}}]}}}}"#
    )
}

fn inherited_snapshot_row(predecessor_session_id: &str) -> String {
    format!(
        r#"{{"type":"file-history-snapshot","messageId":"m1","snapshot":{{"trackedFileBackups":{{"/private/tmp/claude-501/project/{predecessor_session_id}/scratchpad/a.txt":{{"realParentDir":"/private/tmp/claude-501/project/{predecessor_session_id}/scratchpad"}}}}}}}}"#
    )
}

fn bare_mode_row(session_id: &str) -> String {
    format!(r#"{{"type":"mode","sessionId":"{session_id}","mode":"default"}}"#)
}

fn bare_permission_mode_row(session_id: &str) -> String {
    format!(
        r#"{{"type":"permission-mode","sessionId":"{session_id}","permissionMode":"acceptEdits","timestamp":null}}"#
    )
}

/// Stale predecessor: real conversation up to 2026-07-31T21:14, then only
/// bare mode records — exactly the shape that keeps bumping mtime.
fn write_stale_fixture(directory: &Path) -> PathBuf {
    write_fixture_transcript(
        directory,
        FIXTURE_STALE_ID,
        &[
            user_row(FIXTURE_STALE_ID, "u1", "2026-07-31T21:00:00.000Z", "start"),
            assistant_row(FIXTURE_STALE_ID, "a1", "2026-07-31T21:14:49.796Z", "done"),
            bare_mode_row(FIXTURE_STALE_ID),
            bare_permission_mode_row(FIXTURE_STALE_ID),
        ],
    )
}

fn stale_last_substantive_ms(stale_path: &Path) -> i64 {
    last_substantive_transcript_timestamp_ms(stale_path)
        .expect("stale fixture has a substantive record")
}

#[test]
fn continuation_marker_successor_is_adopted() {
    let directory = successor_fixture_dir("continuation");
    let stale = write_stale_fixture(&directory);
    let successor = write_fixture_transcript(
        &directory,
        FIXTURE_SUCCESSOR_ID,
        &[
            // Deliberately NO predecessor id field: this exercises the
            // continuation-marker + inherited-snapshot lineage on its own.
            inherited_snapshot_row(FIXTURE_STALE_ID),
            continuation_marker_row(FIXTURE_SUCCESSOR_ID, "c1", "2026-08-01T09:00:00.000Z"),
            assistant_row(
                FIXTURE_SUCCESSOR_ID,
                "a2",
                "2026-08-01T09:05:00.000Z",
                "continuing",
            ),
        ],
    );
    let outcome = find_claude_successor_transcript(
        FIXTURE_STALE_ID,
        &stale,
        stale_last_substantive_ms(&stale),
        &[],
    );
    match outcome {
        SessionChatSuccessorOutcome::Found(found) => {
            assert_eq!(found.agent_session_id, FIXTURE_SUCCESSOR_ID);
            assert_eq!(found.path, successor);
            assert_eq!(
                found.lineage,
                SessionChatSuccessorLineage::ContinuationSnapshot
            );
            assert_eq!(found.hops, 1);
        }
        other => panic!("expected the continuation successor, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn resume_fork_copy_successor_is_adopted() {
    let directory = successor_fixture_dir("resume-fork");
    let stale = write_stale_fixture(&directory);
    write_fixture_transcript(
        &directory,
        FIXTURE_SUCCESSOR_ID,
        &[
            resume_fork_assistant_row(
                FIXTURE_SUCCESSOR_ID,
                FIXTURE_STALE_ID,
                "a2",
                "2026-08-01T09:05:00.000Z",
            ),
            user_row(
                FIXTURE_SUCCESSOR_ID,
                "u2",
                "2026-08-01T09:06:00.000Z",
                "keep going",
            ),
        ],
    );
    match find_claude_successor_transcript(
        FIXTURE_STALE_ID,
        &stale,
        stale_last_substantive_ms(&stale),
        &[],
    ) {
        SessionChatSuccessorOutcome::Found(found) => {
            assert_eq!(found.agent_session_id, FIXTURE_SUCCESSOR_ID);
            assert_eq!(
                found.lineage,
                SessionChatSuccessorLineage::PredecessorIdField
            );
        }
        other => panic!("expected the resume-fork successor, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn transcripts_that_merely_mention_the_stale_id_are_not_adopted() {
    let directory = successor_fixture_dir("mention-only");
    let stale = write_stale_fixture(&directory);
    // A busy neighbour session: it declares its own id, carries a
    // continuation marker of its OWN (unrelated) predecessor, and quotes the
    // stale id inside tool input, tool output and a summary record.
    write_fixture_transcript(
        &directory,
        FIXTURE_DECOY_ID,
        &[
            continuation_marker_row(FIXTURE_DECOY_ID, "c9", "2026-08-01T09:00:00.000Z"),
            format!(
                r#"{{"type":"assistant","sessionId":"{FIXTURE_DECOY_ID}","uuid":"d1","timestamp":"2026-08-01T09:01:00.000Z","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"Bash","input":{{"command":"rg {FIXTURE_STALE_ID} ~/.claude/projects"}}}}]}}}}"#
            ),
            format!(
                r#"{{"type":"user","sessionId":"{FIXTURE_DECOY_ID}","uuid":"d2","timestamp":"2026-08-01T09:02:00.000Z","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","content":"{FIXTURE_STALE_ID}.jsonl"}}]}}}}"#
            ),
            format!(
                r#"{{"type":"summary","summary":"work on {FIXTURE_STALE_ID}","leafUuid":"d1"}}"#
            ),
            assistant_row(FIXTURE_DECOY_ID, "d3", "2026-08-01T09:03:00.000Z", "ok"),
        ],
    );
    // A sub-agent transcript with textbook lineage: still never the
    // session's own conversation.
    write_fixture_transcript(
        &directory,
        "agent-abc123",
        &[resume_fork_assistant_row(
            "agent-abc123",
            FIXTURE_STALE_ID,
            "s1",
            "2026-08-01T09:07:00.000Z",
        )],
    );
    // Records that carry ONLY the stale id (no own identity) — the
    // 6d27b5150 guardrail.
    write_fixture_transcript(
        &directory,
        FIXTURE_SECOND_SUCCESSOR_ID,
        &[format!(
            r#"{{"type":"assistant","sessionId":"{FIXTURE_STALE_ID}","uuid":"z1","timestamp":"2026-08-01T09:08:00.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"copy"}}]}}}}"#
        )],
    );
    assert_eq!(
        find_claude_successor_transcript(
            FIXTURE_STALE_ID,
            &stale,
            stale_last_substantive_ms(&stale),
            &[],
        ),
        SessionChatSuccessorOutcome::NotFound
    );
    let _ = fs::remove_dir_all(&directory);
}

/*
CDXC:SessionIdentity 2026-08-02:
End-to-end runtime path, not just the detector: the real follower loop, a
fake registry behind the state reader / adopt hook, real transcript files.
The first cut passed every detector test and still never fired in the live
daemon, so this exercises spawn → drain → staleness → scan → registry write
→ re-snapshot with the production code path and only the timers shrunk.
Note `working: false` throughout: hooks never report a background-job
continuation, so adoption must not depend on the working flag.
*/
#[tokio::test]
async fn follower_adopts_a_successor_and_delivers_its_tail() {
    use std::sync::Mutex;

    let directory = successor_fixture_dir("follower-loop");
    let stale = write_stale_fixture(&directory);
    let successor = write_fixture_transcript(
        &directory,
        FIXTURE_SUCCESSOR_ID,
        &[
            resume_fork_assistant_row(
                FIXTURE_SUCCESSOR_ID,
                FIXTURE_STALE_ID,
                "a2",
                "2026-08-01T09:05:00.000Z",
            ),
            assistant_row(
                FIXTURE_SUCCESSOR_ID,
                "a3",
                "2026-08-01T09:06:00.000Z",
                "LIVE-SUCCESSOR-TAIL",
            ),
        ],
    );

    // Fake registry: (agentSessionId, agentSessionPath).
    let registry = Arc::new(Mutex::new((
        FIXTURE_STALE_ID.to_string(),
        stale.to_string_lossy().into_owned(),
    )));
    let notices: Arc<Mutex<Vec<SessionChatSuccessorNotice>>> = Arc::new(Mutex::new(Vec::new()));
    let frames: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));

    let state_reader: SessionChatStateReader = {
        let registry = registry.clone();
        Arc::new(move || {
            let (agent_session_id, agent_session_path) = registry.lock().expect("registry").clone();
            SessionChatLiveState {
                prompt: None,
                working: false,
                agent_session_id: Some(agent_session_id),
                agent_session_path: Some(agent_session_path),
            }
        })
    };
    let hooks = SessionChatSuccessorHooks {
        bound_agent_session_ids: Arc::new(Vec::new),
        pending_fork_child_since_ms: Arc::new(|_| None),
        unbound_agent_chat: Arc::new(|| None),
        probe_live_identity: Arc::new(|| {}),
        adopt_identity: {
            let registry = registry.clone();
            Arc::new(move |adoption| {
                let mut stored = registry.lock().expect("registry");
                // Compare-and-set, exactly like the domain write.
                if Some(stored.0.as_str()) != adoption.previous_agent_session_id.as_deref() {
                    return false;
                }
                *stored = (adoption.agent_session_id, adoption.agent_session_path);
                true
            })
        },
        log: {
            let notices = notices.clone();
            Arc::new(move |notice| notices.lock().expect("notices").push(notice))
        },
    };
    let config = SessionChatFollowerConfig {
        project_id: "P1".to_string(),
        session_id: "S1".to_string(),
        agent: Some("claude".to_string()),
        agent_session_id: Some(FIXTURE_STALE_ID.to_string()),
        agent_session_path: Some(stale.to_string_lossy().into_owned()),
        limit: 50,
        protocol_version: 1,
        server_id: "test-server".to_string(),
        state_reader: Some(state_reader),
        options_reader: None,
        options_change_watch: None,
        screen_change_watch: None,
        queue_reader: None,
        successor_hooks: Some(hooks),
        notice_publisher: None,
        tuning: SessionChatFollowerTuning {
            reconcile_interval: Duration::from_millis(20),
            stale_transcript_idle: Duration::from_millis(40),
            successor_scan_interval: Duration::from_millis(60),
            successor_stale_substantive_idle_ms: 1_000,
        },
    };
    let emit: SessionChatFrameEmitter = {
        let frames = frames.clone();
        Arc::new(move |frame| frames.lock().expect("frames").push(frame))
    };
    let task = tokio::spawn(run_session_chat_follower(
        config,
        Arc::new(SessionChatStream::new()),
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(SessionChatFollowerHeartbeat::new()),
        emit,
    ));

    let mut adopted_frame: Option<Value> = None;
    for _ in 0..200 {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let seen = frames.lock().expect("frames").clone();
        adopted_frame = seen.into_iter().find(|frame| {
            frame.get("agentSessionId").and_then(Value::as_str) == Some(FIXTURE_SUCCESSOR_ID)
                && frame
                    .get("messages")
                    .map(|messages| messages.to_string().contains("LIVE-SUCCESSOR-TAIL"))
                    .unwrap_or(false)
        });
        if adopted_frame.is_some() {
            break;
        }
    }
    task.abort();

    let notices = notices.lock().expect("notices").clone();
    assert!(
        adopted_frame.is_some(),
        "the follower never delivered the successor tail; notices: {notices:?}, frames: {:?}",
        frames
            .lock()
            .expect("frames")
            .iter()
            .map(|frame| frame.get("type").cloned().unwrap_or(Value::Null))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        *registry.lock().expect("registry"),
        (
            FIXTURE_SUCCESSOR_ID.to_string(),
            successor.to_string_lossy().into_owned()
        ),
        "the corrected identity must be written back to the registry",
    );
    assert!(
        notices.iter().any(|notice| matches!(
            notice,
            SessionChatSuccessorNotice::Adopted(adoption)
                if adoption.agent_session_id == FIXTURE_SUCCESSOR_ID
                    && adoption.previous_agent_session_id.as_deref() == Some(FIXTURE_STALE_ID)
        )),
        "adoption must be logged once: {notices:?}",
    );
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn successor_bound_to_another_registry_session_is_not_adopted() {
    let directory = successor_fixture_dir("already-bound");
    let stale = write_stale_fixture(&directory);
    write_fixture_transcript(
        &directory,
        FIXTURE_SUCCESSOR_ID,
        &[resume_fork_assistant_row(
            FIXTURE_SUCCESSOR_ID,
            FIXTURE_STALE_ID,
            "a2",
            "2026-08-01T09:05:00.000Z",
        )],
    );
    let stale_ms = stale_last_substantive_ms(&stale);
    assert!(matches!(
        find_claude_successor_transcript(FIXTURE_STALE_ID, &stale, stale_ms, &[]),
        SessionChatSuccessorOutcome::Found(_)
    ));
    assert_eq!(
        find_claude_successor_transcript(
            FIXTURE_STALE_ID,
            &stale,
            stale_ms,
            &[FIXTURE_SUCCESSOR_ID.to_string()],
        ),
        // Reported as owned, NOT as NotFound: the difference is what makes
        // a blocked adoption visible in the log.
        SessionChatSuccessorOutcome::OwnedByAnotherSession {
            candidate_session_ids: vec![FIXTURE_SUCCESSOR_ID.to_string()],
        },
        "a transcript another live session already owns must never be stolen",
    );
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn equally_recent_continuations_adopt_none() {
    let directory = successor_fixture_dir("ambiguous");
    let stale = write_stale_fixture(&directory);
    for stem in [FIXTURE_SUCCESSOR_ID, FIXTURE_SECOND_SUCCESSOR_ID] {
        write_fixture_transcript(
            &directory,
            stem,
            &[resume_fork_assistant_row(
                stem,
                FIXTURE_STALE_ID,
                "a2",
                "2026-08-01T09:05:00.000Z",
            )],
        );
    }
    match find_claude_successor_transcript(
        FIXTURE_STALE_ID,
        &stale,
        stale_last_substantive_ms(&stale),
        &[],
    ) {
        SessionChatSuccessorOutcome::Ambiguous {
            predecessor_session_id,
            candidate_session_ids,
        } => {
            assert_eq!(predecessor_session_id, FIXTURE_STALE_ID);
            assert_eq!(candidate_session_ids.len(), 2);
        }
        other => panic!("expected an ambiguous outcome, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn chained_successors_are_followed_to_the_newest() {
    let directory = successor_fixture_dir("chain");
    let stale = write_stale_fixture(&directory);
    write_fixture_transcript(
        &directory,
        FIXTURE_SUCCESSOR_ID,
        &[
            resume_fork_assistant_row(
                FIXTURE_SUCCESSOR_ID,
                FIXTURE_STALE_ID,
                "a2",
                "2026-08-01T09:05:00.000Z",
            ),
            bare_mode_row(FIXTURE_SUCCESSOR_ID),
        ],
    );
    let newest = write_fixture_transcript(
        &directory,
        FIXTURE_SECOND_SUCCESSOR_ID,
        &[
            inherited_snapshot_row(FIXTURE_SUCCESSOR_ID),
            continuation_marker_row(
                FIXTURE_SECOND_SUCCESSOR_ID,
                "c2",
                "2026-08-01T12:00:00.000Z",
            ),
            assistant_row(
                FIXTURE_SECOND_SUCCESSOR_ID,
                "a3",
                "2026-08-01T12:10:00.000Z",
                "still going",
            ),
        ],
    );
    match find_claude_successor_transcript(
        FIXTURE_STALE_ID,
        &stale,
        stale_last_substantive_ms(&stale),
        &[],
    ) {
        SessionChatSuccessorOutcome::Found(found) => {
            assert_eq!(found.agent_session_id, FIXTURE_SECOND_SUCCESSOR_ID);
            assert_eq!(found.path, newest);
            assert_eq!(found.hops, 2, "the chain must be followed to its end");
        }
        other => panic!("expected the second successor, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&directory);
}

/*
The dead transcript keeps receiving bare `mode`/`permission-mode` records,
so its mtime tracks "now" forever. Neither the substantive-staleness clock
nor the follower's drain may treat that as activity.
*/
#[test]
fn bare_mode_appends_do_not_look_like_transcript_activity() {
    let directory = successor_fixture_dir("mtime-bumps");
    let stale = write_stale_fixture(&directory);
    let before = stale_last_substantive_ms(&stale);
    let version_before = read_transcript_file_version(&stale).expect("stat fixture");

    let mut drain_state = FollowerFileState::new();
    let first = follower_drain_once(
        &stale,
        300,
        SessionChatTranscriptAgent::Claude,
        decode_claude_transcript_line,
        Some(decode_claude_turn_lifecycle),
        &mut drain_state,
        true,
    );
    assert!(matches!(first, FollowerDrainOutcome::Snapshot { .. }));

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&stale)
        .expect("append to fixture");
    writeln!(file, "{}", bare_permission_mode_row(FIXTURE_STALE_ID)).expect("write bare record");
    drop(file);

    let version_after = read_transcript_file_version(&stale).expect("stat fixture");
    assert!(
        version_after.size > version_before.size,
        "the fixture must actually have grown"
    );
    assert_eq!(
        last_substantive_transcript_timestamp_ms(&stale),
        Some(before),
        "bare records must not move the substantive clock"
    );
    let second = follower_drain_once(
        &stale,
        300,
        SessionChatTranscriptAgent::Claude,
        decode_claude_transcript_line,
        Some(decode_claude_turn_lifecycle),
        &mut drain_state,
        false,
    );
    assert!(
        matches!(second, FollowerDrainOutcome::Idle),
        "a bare mode append must not count as transcript activity"
    );
    let _ = fs::remove_dir_all(&directory);
}
