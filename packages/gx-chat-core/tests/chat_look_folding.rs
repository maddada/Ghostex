//! Agent work Reduced and Quiet fold every finished turn of the agent's work, the reports its own
//! subagents hand back included (CDXC:SessionChat 2026-11-09 in `transcript/turns.rs`), and Default
//! keeps today's chat.

use ghostex_gx_chat_core::transcript::chat_look::AgentWork;
use ghostex_gx_chat_core::{ChatContext, ChatState, TranscriptItem};
use ghostex_gx_protocol::ChatMessage;
use serde_json::{json, Value};

const T0: i64 = 1_793_420_000_000;

/// The rows as the core holds them (`ChatRow`).
fn shared(rows: &[ChatMessage]) -> Vec<ghostex_gx_chat_core::state::ChatRow> {
    rows.iter().cloned().map(std::sync::Arc::new).collect()
}

fn message(value: Value) -> ChatMessage {
    serde_json::from_value(value).expect("a chat message")
}

fn text_row(id: &str, role: &str, at: i64, text: &str) -> ChatMessage {
    message(json!({
        "id": id, "role": role, "timestamp": at, "source": "transcript",
        "blocks": [{ "type": "text", "text": text }],
    }))
}

fn user(id: &str, at: i64, text: &str) -> ChatMessage {
    text_row(id, "user", at, text)
}

fn assistant(id: &str, at: i64, text: &str) -> ChatMessage {
    text_row(id, "assistant", at, text)
}

/// One call and its answer.
fn call(id: &str, at: i64, command: &str) -> [ChatMessage; 2] {
    [
        message(json!({
            "id": id, "role": "assistant", "timestamp": at, "source": "transcript",
            "blocks": [{ "type": "tool-call", "name": "Bash", "callId": id, "input": {"command": command} }],
        })),
        message(json!({
            "id": format!("{id}-result"), "role": "tool", "timestamp": at + 500,
            "source": "transcript", "blocks": [{ "type": "tool-result", "callId": id, "output": "ok" }],
        })),
    ]
}

/// A report one of the session's own subagents hands back, as Claude delivers it.
fn report(id: &str, at: i64, agent: &str) -> ChatMessage {
    user(
        id,
        at,
        &format!("<agent-message from=\"{agent}\">\n  The audit found two bugs.\n</agent-message>"),
    )
}

/// The harness's note that the subagent finished.
fn notification(id: &str, at: i64, agent: &str) -> ChatMessage {
    user(
        id,
        at,
        &format!(
            "<task-notification>\n<task-id>{agent}</task-id>\n<status>completed</status>\n<summary>Agent \"Audit\" finished</summary>\n</task-notification>"
        ),
    )
}

fn state_with(look: AgentWork, mut rows: Vec<ChatMessage>) -> ChatState {
    for (at, row) in rows.iter_mut().enumerate() {
        row.byte_offset.get_or_insert(at as u64 * 100);
    }
    let mut state = ChatState::default();
    state.session.agent = Some("claude".into());
    state.messages.composed = shared(&rows);
    state.messages.list = shared(&rows);
    state.transcript_view.chat_look.agent_work = look;
    state
}

fn items(state: &ChatState) -> Vec<TranscriptItem> {
    let context = ChatContext {
        now_ms: (T0 + 600_000) as f64,
        ..Default::default()
    };
    ghostex_gx_chat_core::transcript::rows::rows(state, &context)
}

fn id(row: &Value) -> &str {
    row["id"].as_str().unwrap_or_default()
}

/// Each row in short: `user u1`, `line [u1 r1] replies [r1] work [c1 r1]`, `fold u1`.
fn outline(items: &[TranscriptItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            TranscriptItem::Message { message } => {
                format!(
                    "{} {}",
                    message["role"].as_str().unwrap_or_default(),
                    id(message)
                )
            }
            TranscriptItem::GroupedWork { turns, work, .. } => format!(
                "line [{}] replies [{}] work [{}]",
                turns
                    .iter()
                    .map(|turn| turn.id.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                turns
                    .iter()
                    .filter_map(|turn| turn.final_message.as_deref().map(id))
                    .collect::<Vec<_>>()
                    .join(" "),
                work.iter().map(|row| id(row)).collect::<Vec<_>>().join(" "),
            ),
            TranscriptItem::CompletedWork { id, .. } => format!("fold {id}"),
            _ => "other".to_string(),
        })
        .collect()
}

/// A prompt the agent answered only with calls, then a second prompt it answered in words.
fn replyless_turn() -> Vec<ChatMessage> {
    let mut rows = vec![user("u1", T0, "check the build")];
    rows.extend(call("c1", T0 + 1_000, "cargo check"));
    rows.extend(call("c2", T0 + 2_000, "cargo test"));
    rows.push(user("u2", T0 + 3_000, "and now?"));
    rows.push(assistant("r2", T0 + 4_000, "All green."));
    rows
}

#[test]
fn a_turn_with_no_reply_folds_under_reduced_and_quiet() {
    for look in [AgentWork::Reduced, AgentWork::Quiet] {
        let outline = outline(&items(&state_with(look, replyless_turn())));
        assert_eq!(
            outline,
            [
                "user u1",
                // The chat's tool fold holds both calls in the first one's row.
                "line [u1] replies [] work [c1]",
                "user u2",
                "line [u2] replies [r2] work []",
            ],
            "{look:?}"
        );
    }
}

#[test]
fn default_keeps_a_turn_with_no_reply_as_its_rows() {
    let outline = outline(&items(&state_with(AgentWork::Default, replyless_turn())));
    assert_eq!(outline, ["user u1", "assistant c1", "user u2", "fold u2"]);
}

/// The agent starts a subagent and says it waits; the report wakes it, it works, a second report
/// lands while it works, and it answers.
fn subagent_reports() -> Vec<ChatMessage> {
    let mut rows = vec![user("u1", T0, "audit the chat")];
    rows.extend(call("c1", T0 + 1_000, "start audit"));
    rows.push(assistant(
        "r1",
        T0 + 2_000,
        "Two auditors are running; I'll wait.",
    ));
    rows.push(report("a1", T0 + 60_000, "aed516f06a84e683f"));
    rows.push(notification("n1", T0 + 60_100, "aed516f06a84e683f"));
    rows.push(assistant(
        "m1",
        T0 + 61_000,
        "The first audit is in, checking it.",
    ));
    rows.extend(call("c2", T0 + 62_000, "rg chat_look"));
    rows.push(report("a2", T0 + 63_000, "ac26f28eca2e672ed"));
    rows.push(notification("n2", T0 + 63_100, "ac26f28eca2e672ed"));
    rows.extend(call("c3", T0 + 64_000, "rg grouped_work"));
    rows.push(assistant("r3", T0 + 65_000, "Both audits agree: two bugs."));
    rows
}

#[test]
fn subagent_reports_fold_into_the_line_of_the_message_they_answer() {
    for look in [AgentWork::Reduced, AgentWork::Quiet] {
        let state = state_with(look, subagent_reports());
        let items = items(&state);
        assert_eq!(
            outline(&items)[..2],
            [
                "user u1".to_string(),
                // `m1` holds the call after it (the chat's tool fold).
                "line [u1 a1 a2] replies [r1 r3] work [c1 a1 n1 m1 a2 n2 c3]".to_string(),
            ],
            "{look:?}"
        );
        assert_eq!(outline(&items).len(), 2, "{look:?}: no card, no loose line");
    }
}

#[test]
fn words_a_report_cut_short_are_no_reply_under_reduced_and_quiet() {
    let state = state_with(AgentWork::Quiet, subagent_reports());
    let context = ChatContext::default();
    let projection = ghostex_gx_chat_core::transcript::presentation::build(&state, &context);
    assert_eq!(projection.final_ids, ["r1", "r3"]);
    let state = state_with(AgentWork::Default, subagent_reports());
    let projection = ghostex_gx_chat_core::transcript::presentation::build(&state, &context);
    assert_eq!(projection.final_ids, ["r1", "m1", "r3"]);
}

#[test]
fn default_keeps_each_report_as_its_card() {
    let state = state_with(AgentWork::Default, subagent_reports());
    let outline = outline(&items(&state));
    assert!(outline.contains(&"user a1".to_string()), "{outline:?}");
    assert!(outline.contains(&"user a2".to_string()), "{outline:?}");
}

#[test]
fn a_message_from_another_agent_stays_a_card_under_quiet() {
    let mut rows = vec![user("u1", T0, "start the thread")];
    rows.push(assistant("r1", T0 + 1_000, "Started."));
    rows.push(user(
        "x1",
        T0 + 5_000,
        "<cross-session-message from=\"S1:P1:G1\" from-name=\"Thread\">Done.</cross-session-message>",
    ));
    rows.push(assistant("r2", T0 + 6_000, "The thread is done."));
    let outline = outline(&items(&state_with(AgentWork::Quiet, rows)));
    assert!(outline.contains(&"user x1".to_string()), "{outline:?}");
}

/// The settled turns are chunked at the prompts the user sent; a report starts no chunk, so its
/// turn still joins the line of the message it answers.
#[test]
fn a_long_chat_folds_reports_into_the_same_line() {
    let mut rows = Vec::new();
    for turn in 0..40 {
        let at = T0 - 1_000_000 + turn * 10_000;
        rows.push(user(&format!("p{turn}"), at, "next step"));
        rows.extend(call(&format!("k{turn}"), at + 1_000, "cargo check"));
        rows.push(assistant(&format!("q{turn}"), at + 2_000, "Done."));
    }
    rows.extend(subagent_reports());
    rows.push(user("u9", T0 + 70_000, "thanks"));
    rows.push(assistant("r9", T0 + 71_000, "Any time."));
    let mut state = state_with(AgentWork::Reduced, rows);
    let context = ChatContext {
        now_ms: (T0 + 600_000) as f64,
        ..Default::default()
    };
    ghostex_gx_chat_core::transcript::rows::refresh(&mut state, &context);
    assert!(
        state.transcript_view.settled.is_some(),
        "built on settled turns"
    );
    let outline = outline(&ghostex_gx_chat_core::transcript::rows::rows(
        &state, &context,
    ));
    let at = outline.iter().position(|row| row == "user u1").expect("u1");
    assert_eq!(
        outline[at + 1],
        "line [u1 a1 a2] replies [r1 r3] work [c1 a1 n1 m1 a2 n2 c3]",
        "{outline:?}"
    );
    assert_eq!(outline[at + 2], "user u9");
}

/// A reply that ran calls of its own, and a picture the agent shared mid-turn with a call after it.
fn reply_and_picture_with_calls() -> Vec<ChatMessage> {
    let mut rows = vec![user("u1", T0, "fix and show me")];
    rows.extend(call("c1", T0 + 1_000, "cargo check"));
    rows.push(message(json!({
        "id": "pic", "role": "assistant", "timestamp": T0 + 2_000, "source": "transcript",
        "blocks": [
            { "type": "image-ref", "path": "/shots/after.png", "mimeType": "image/png" },
            { "type": "text", "text": "Here is the screen." },
            { "type": "tool-call", "name": "Bash", "callId": "c2", "input": {"command": "open after.png"} },
        ],
    })));
    rows.push(message(json!({
        "id": "c2-result", "role": "tool", "timestamp": T0 + 2_500, "source": "transcript",
        "blocks": [{ "type": "tool-result", "callId": "c2", "output": "ok" }],
    })));
    rows.extend(call("c3", T0 + 3_000, "cargo test"));
    rows.push(message(json!({
        "id": "r1", "role": "assistant", "timestamp": T0 + 4_000, "source": "transcript",
        "blocks": [
            { "type": "text", "text": "Fixed; committing." },
            { "type": "tool-call", "name": "Bash", "callId": "c4", "input": {"command": "git commit"} },
        ],
    })));
    rows.push(message(json!({
        "id": "c4-result", "role": "tool", "timestamp": T0 + 4_500, "source": "transcript",
        "blocks": [{ "type": "tool-result", "callId": "c4", "output": "ok" }],
    })));
    rows
}

#[test]
fn a_replys_and_a_pictures_calls_are_steps_of_the_line() {
    for look in [AgentWork::Reduced, AgentWork::Quiet] {
        let items = items(&state_with(look, reply_and_picture_with_calls()));
        let Some(TranscriptItem::GroupedWork { work, turns, .. }) = items.get(1) else {
            panic!("{look:?}: {:?}", outline(&items));
        };
        // The picture's calls (the chat's tool fold gives it the call after it) sit where the
        // picture did; the reply's come last.
        let ids: Vec<&str> = work.iter().map(|row| id(row)).collect();
        assert_eq!(ids, ["c1", "pic", "r1"], "{look:?}");
        let calls = |row: &Value| {
            row["tools"].as_array().map_or(0, |tools| {
                tools.iter().filter(|tool| tool["hasCall"] == true).count()
            })
        };
        assert_eq!(calls(&work[1]), 2, "{look:?}");
        assert_eq!(calls(&work[2]), 1, "{look:?}");
        let reply = turns[0].final_message.as_deref().expect("a reply");
        assert_eq!(calls(reply), 0, "{look:?}: the reply draws no calls");
        assert_eq!(reply["text"], "Fixed; committing.");
        let picture = &turns[0].artifacts[0];
        assert_eq!(calls(picture), 0, "{look:?}: the picture draws no calls");
    }
}

fn working(mut state: ChatState, started: i64) -> ChatState {
    use ghostex_gx_protocol::{TurnLifecycle, TurnLifecycleState};
    state.session.server_working = true;
    state.session.working_started_at_ms = Some(started as f64);
    state.session.lifecycle = Some(TurnLifecycle {
        state: TurnLifecycleState::Working,
        turn_id: "u1".into(),
        timestamp: Some(started),
    });
    state
}

#[test]
fn reduced_draws_a_running_sentences_calls_as_its_line() {
    let mut rows = vec![user("u1", T0, "fix it")];
    rows.push(message(json!({
        "id": "m1", "role": "assistant", "timestamp": T0 + 1_000, "source": "transcript",
        "blocks": [
            { "type": "text", "text": "Checking the build first." },
            { "type": "tool-call", "name": "Bash", "callId": "c1", "input": {"command": "cargo check"} },
        ],
    })));
    let state = working(state_with(AgentWork::Reduced, rows.clone()), T0);
    let reduced = items(&state);
    let Some(TranscriptItem::Message { message }) = reduced.last() else {
        panic!("{:?}", outline(&reduced));
    };
    assert_eq!(message["toolsFlat"], true, "the words own no calls");
    assert_eq!(message["toolsShowAllRows"], false, "the calls are one line");
    let state = working(state_with(AgentWork::Default, rows), T0);
    let default = items(&state);
    let Some(TranscriptItem::Message { message }) = default.last() else {
        panic!("{:?}", outline(&default));
    };
    assert_ne!(
        message["toolsFlat"], true,
        "Default keeps the heading over its calls"
    );
}

#[test]
fn quiet_hides_a_running_report_and_the_status_line_names_it() {
    use ghostex_gx_chat_core::extras::working_card::working_card;
    let mut rows = vec![user("u1", T0, "audit the chat")];
    rows.push(assistant(
        "r1",
        T0 + 2_000,
        "Two auditors are running; I'll wait.",
    ));
    rows.push(report("a1", T0 + 60_000, "aed516f06a84e683f"));
    rows.push(notification("n1", T0 + 60_100, "aed516f06a84e683f"));
    let state = working(state_with(AgentWork::Quiet, rows.clone()), T0 + 60_000);
    let outline = outline(&items(&state));
    assert!(!outline.iter().any(|row| row.contains("a1")), "{outline:?}");
    let strip = ghostex_gx_chat_core::WorkingStrip::default();
    let card = working_card(&state, &strip, &[]);
    assert_eq!(card.label.as_deref(), Some("Reading the subagent's report"));
    let state = working(state_with(AgentWork::Default, rows), T0 + 60_000);
    assert_ne!(
        working_card(&state, &strip, &[]).label.as_deref(),
        Some("Reading the subagent's report")
    );
}

#[test]
fn the_summary_views_running_work_follows_reduced_and_quiet() {
    let mut rows = vec![user("u1", T0, "fix it")];
    rows.push(assistant("m1", T0 + 1_000, "Checking the build first."));
    rows.extend(call("c1", T0 + 2_000, "cargo check"));
    let summary = |look| {
        let mut state = working(state_with(look, rows.clone()), T0);
        state.transcript_view.summary_mode = true;
        let items = items(&state);
        let Some(TranscriptItem::Summary {
            work,
            work_open,
            active,
            ..
        }) = items.last()
        else {
            panic!("{look:?}: no summary turn");
        };
        assert!(*active, "{look:?}");
        let opened = work.iter().any(|row| row["toolsOpen"] == true);
        (*work_open, opened, work.len())
    };
    let (open, opened, rows_in) = summary(AgentWork::Default);
    assert!(
        open && opened && rows_in > 0,
        "Default opens the running work"
    );
    let (open, opened, rows_in) = summary(AgentWork::Reduced);
    assert!(!open && !opened && rows_in > 0, "Reduced keeps it shut");
    let (open, _, rows_in) = summary(AgentWork::Quiet);
    assert!(!open, "Quiet keeps it shut");
    assert_eq!(
        rows_in, 0,
        "Quiet shows only the status line while the agent runs"
    );
}

/// An agent that reports no Working (Cursor): Quiet still keeps its running calls out of the
/// chat, while its words stay.
#[test]
fn quiet_hides_the_running_calls_when_the_working_signal_is_in_doubt() {
    use ghostex_gx_chat_core::transcript::grouped_work::hide_live_calls;
    let row = |value: Value| TranscriptItem::Message {
        message: std::sync::Arc::new(value),
    };
    let tool = json!({"hasCall": true, "name": "Bash", "status": "running"});
    let mut items = vec![
        row(json!({"id": "u1", "role": "user", "text": "fix it"})),
        row(json!({"id": "m1", "role": "assistant", "text": "Checking.", "tools": [tool]})),
        row(json!({"id": "m2", "role": "assistant", "text": "", "tools": [tool]})),
        row(
            json!({"id": "m3", "role": "assistant", "text": "", "tools": [
                {"hasCall": true, "name": "Bash", "status": "running", "awaitingApproval": true}
            ]}),
        ),
    ];
    let live = ["m1", "m2", "m3"].into_iter().collect();
    hide_live_calls(&mut items, &live);
    let outline: Vec<String> = items
        .iter()
        .map(|item| match item {
            TranscriptItem::Message { message } => format!(
                "{} {}",
                id(message),
                message["tools"].as_array().map_or(0, Vec::len)
            ),
            _ => String::new(),
        })
        .collect();
    assert_eq!(
        outline,
        ["u1 0", "m1 0", "m3 1"],
        "the words stay, the calls go, an approval stays"
    );
}

/// The line's fold is keyed `work:<first turn id>`, as Default's "Worked for" fold is, so a fold
/// closed under one look stays closed after switching.
#[test]
fn the_line_keeps_default_folds_key() {
    let default = items(&state_with(AgentWork::Default, replyless_turn()));
    let reduced = items(&state_with(AgentWork::Reduced, replyless_turn()));
    let default_id = default.iter().find_map(|item| match item {
        TranscriptItem::CompletedWork { id, .. } => Some(id.clone()),
        _ => None,
    });
    let reduced_ids: Vec<String> = reduced
        .iter()
        .filter_map(|item| match item {
            TranscriptItem::GroupedWork { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(default_id.as_deref(), Some("u2"));
    assert_eq!(reduced_ids, ["u1", "u2"]);
}

/// gxserver's collapsed history: the prompt holds its work up to the report, and the task
/// notification after the report holds the rest of the response. The line reads both.
#[test]
fn a_collapsed_report_turn_reads_every_section() {
    let deferred = |start: &str, end: &str| json!({"beforeOffset": 1, "startId": start, "endId": end, "messageCount": 4, "filePaths": []});
    let mut prompt = user("u1", T0, "audit the chat");
    prompt.deferred_work = Some(serde_json::from_value(deferred("u1", "c2")).unwrap());
    let mut notice = notification("n1", T0 + 60_100, "aed516f06a84e683f");
    notice.deferred_work = Some(serde_json::from_value(deferred("n1", "r3")).unwrap());
    let rows = vec![
        prompt,
        report("a1", T0 + 60_000, "aed516f06a84e683f"),
        notice,
        assistant("r3", T0 + 65_000, "Both audits agree: two bugs."),
    ];
    let items = items(&state_with(AgentWork::Reduced, rows));
    let Some(TranscriptItem::GroupedWork { turns, .. }) = items.get(1) else {
        panic!("{:?}", outline(&items));
    };
    let sections: Vec<&str> = turns
        .iter()
        .filter_map(|turn| turn.deferred.as_ref()?["startId"].as_str())
        .collect();
    assert_eq!(sections, ["u1", "n1"], "{:?}", outline(&items));
    assert_eq!(items.len(), 2, "{:?}", outline(&items));
}

/// The Summary view's running work starts shut under Reduced and Quiet, but a one-click question
/// in a sentence still shows at once, drawn in full under the prompt.
#[test]
fn a_shut_summary_fold_keeps_a_question_in_view() {
    let mut rows = vec![user("u1", T0, "fix it")];
    rows.extend(call("c1", T0 + 1_000, "cargo check"));
    rows.push(assistant(
        "q1",
        T0 + 2_000,
        "Continue? [Yes](ghostex-answer:Q1) [No](ghostex-answer:Q1)",
    ));
    for look in [AgentWork::Reduced, AgentWork::Quiet] {
        let mut state = working(state_with(look, rows.clone()), T0);
        state.transcript_view.summary_mode = true;
        let items = items(&state);
        let Some(TranscriptItem::Summary {
            cards, work_open, ..
        }) = items.last()
        else {
            panic!("{look:?}: no summary turn");
        };
        assert!(!work_open, "{look:?}");
        let ids: Vec<&str> = cards.iter().map(|card| id(card)).collect();
        assert_eq!(ids, ["q1"], "{look:?}");
    }
}

/// A reply's call that read a picture: under Reduced its thumbnail moves with the call into the
/// line, whose strip and count keep it.
#[test]
fn a_moved_calls_picture_stays_in_the_lines_strip() {
    let rows = vec![
        user("u1", T0, "show me"),
        message(json!({
            "id": "r1", "role": "assistant", "timestamp": T0 + 1_000, "source": "transcript",
            "blocks": [
                { "type": "text", "text": "Here it is." },
                { "type": "tool-call", "name": "Read", "callId": "c1", "input": {"file_path": "/shots/after.png"} },
            ],
        })),
        message(json!({
            "id": "c1-result", "role": "tool", "timestamp": T0 + 1_500, "source": "transcript",
            "blocks": [{ "type": "tool-result", "callId": "c1", "output": "ok" }],
        })),
    ];
    let items = items(&state_with(AgentWork::Reduced, rows));
    let Some(TranscriptItem::GroupedWork {
        read_images, stats, ..
    }) = items.get(1)
    else {
        panic!("{:?}", outline(&items));
    };
    let paths: Vec<&str> = read_images
        .iter()
        .filter_map(|image| image["path"].as_str())
        .collect();
    assert_eq!(paths, ["/shots/after.png"]);
    assert_eq!(stats[0]["kind"], "images");
}
