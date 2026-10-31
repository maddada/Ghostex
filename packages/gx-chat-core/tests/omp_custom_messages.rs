//! OMP's own messages in the chat: a finished background job is a tool-call row, another OMP
//! agent's IRC message is the "Message from" card, advisor notes are one "Advisor notes" card, and
//! one of these that reaches the agent after its reply opens a turn of its own. Rows from an older
//! gxserver (no `customMessage`) and OMP types the chat does not draw stay the marker line, and
//! other agents' rows are unchanged.
//!
//! The rows are synthetic, written the way oh-my-pi 18.13.3 writes them: `async-result` from
//! `session/async-job-delivery.ts` and `prompts/tools/async-result.md`, `irc:incoming` from
//! `session/irc-bridge.ts` and `prompts/system/irc-incoming.md`, `advisor` from
//! `advisor/advise-tool.ts` and `session/session-advisors.ts`.

use ghostex_gx_chat_core::protocol::ChatMessage;
use ghostex_gx_chat_core::transcript::background_rows::background_task_source;
use ghostex_gx_chat_core::transcript::line_breaks::AgentLineBreaks;
use ghostex_gx_chat_core::transcript::presentation::{build, project_message};
use ghostex_gx_chat_core::transcript::transcript::project_chat_transcript;
use ghostex_gx_chat_core::transcript::turns::RenderItem;
use ghostex_gx_chat_core::transcript::work_summary::tool_duration_label;
use ghostex_gx_chat_core::{ChatContext, ChatState, TranscriptItem};
use serde_json::{json, Value};
use std::cell::OnceCell;
use std::collections::HashMap;

/// 2026-10-30 12:30 UTC.
const T0: i64 = 1_793_363_400_000;
const MINUTE: i64 = 60_000;

fn context() -> ChatContext {
    ChatContext {
        now_ms: (T0 + 30 * MINUTE) as f64,
        utc_offset_minutes: 0,
        ..Default::default()
    }
}

fn message(value: Value) -> ChatMessage {
    serde_json::from_value(value).expect("a chat row")
}

fn row(id: &str, role: &str, text: &str, at: i64) -> ChatMessage {
    message(json!({
        "id": id,
        "role": role,
        "source": "transcript",
        "timestamp": at,
        "blocks": [{ "type": "text", "text": text }],
    }))
}

/// An assistant row that says something and calls a tool, then the tool's result: work, not a reply.
fn tool_work(id: &str, at: i64) -> [ChatMessage; 2] {
    [
        message(json!({
            "id": id,
            "role": "assistant",
            "source": "transcript",
            "timestamp": at,
            "blocks": [
                { "type": "text", "text": "Reading the middleware." },
                { "type": "tool-call", "name": "read", "input": { "path": "src/http/middleware.rs" }, "callId": format!("{id}-call") },
            ],
        })),
        message(json!({
            "id": format!("{id}-result"),
            "role": "tool",
            "source": "transcript",
            "timestamp": at + 1_000,
            "blocks": [{ "type": "tool-result", "output": "pub fn layer() {}", "callId": format!("{id}-call") }],
        })),
    ]
}

/// A system row OMP wrote as a `custom_message`, with the type and details gxserver passes on.
fn omp(id: &str, custom_type: &str, details: Value, text: &str, at: i64) -> ChatMessage {
    let mut row = row(id, "system", text, at);
    row.custom_message = Some(
        serde_json::from_value(json!({ "customType": custom_type, "details": details }))
            .expect("a custom message"),
    );
    row
}

const THREE_JOBS: &str = "<system-notice>\n3 background jobs have completed. Resume your work using the results below.\n\n── Job 7c1e (Find every login entry point) ──\n## Login entry points\n1. POST /login behind the shared middleware\n3. GET /sso/callback skips the middleware\n\nAgent: agent://LoginScout, history://LoginScout\n── Job 8d2f (cargo test -p auth) ──\nrunning 112 tests\ntest result: ok. 112 passed; 0 failed\n── Job 9e3a (cargo clippy -p auth) ──\nCommand exited with code 1\nerror: this `if` has identical blocks\n</system-notice>";

fn three_jobs(id: &str, at: i64) -> ChatMessage {
    omp(
        id,
        "async-result",
        json!({ "jobs": [
            { "jobId": "7c1e", "status": "completed", "label": "Find every login entry point", "durationMs": 64_000 },
            { "jobId": "8d2f", "status": "completed", "label": "cargo test -p auth", "durationMs": 48_000 },
            { "jobId": "9e3a", "status": "failed", "label": "cargo clippy -p auth", "durationMs": 22_000 },
        ] }),
        THREE_JOBS,
        at,
    )
}

const ONE_JOB: &str = "<system-notice>\nBackground job 8d2f has completed. Resume your work using the result below.\n\nrunning 112 tests\ntest result: ok. 112 passed; 0 failed\n</system-notice>";

fn one_job(id: &str, at: i64) -> ChatMessage {
    omp(
        id,
        "async-result",
        json!({ "jobs": [{ "jobId": "8d2f", "status": "completed", "label": "cargo test -p auth", "durationMs": 48_000 }] }),
        ONE_JOB,
        at,
    )
}

const IRC: &str = "<irc>\nIncoming IRC message from agent `LoginScout`:\n\nFound three login entry points. The SSO callback in `src/http/sso.rs` skips the middleware.\n\n\n\nIf response expected, reply via `write` (`path: \"agent://LoginScout\"`, `content: \"…\"`); may finish current step first. No one replies on your behalf.\n</irc>";

fn irc(id: &str, at: i64) -> ChatMessage {
    omp(
        id,
        "irc:incoming",
        json!({ "from": "LoginScout", "message": "Found three login entry points. The SSO callback in `src/http/sso.rs` skips the middleware." }),
        IRC,
        at,
    )
}

const ADVICE: &str = "<advisory severity=\"nit\" guidance=\"Weigh this note.\">\n`WINDOW_SECS` is defined twice.\n</advisory>\n<advisory severity=\"concern\" guidance=\"Weigh this note.\">\nThe limiter keys on the client IP only.\n</advisory>";

fn advisor(id: &str, at: i64) -> ChatMessage {
    omp(
        id,
        "advisor",
        json!({ "notes": [
            { "note": "`WINDOW_SECS` is defined twice.", "severity": "nit" },
            { "note": "The limiter keys on the client IP only.", "severity": "concern" },
        ] }),
        ADVICE,
        at,
    )
}

/// The projected message the renderers draw for one row.
fn projected(message: &ChatMessage) -> Value {
    project_message(
        message,
        &OnceCell::new(),
        std::slice::from_ref(message),
        "omp",
        None,
        AgentLineBreaks::default(),
        &context(),
        &HashMap::new(),
        &[],
        false,
        false,
    )
}

/// The whole transcript document for a settled chat, as JSON.
fn document(messages: Vec<ChatMessage>) -> Vec<Value> {
    let mut state = ChatState::default();
    state.messages.composed = messages;
    build(&state, &context())
        .items
        .iter()
        .map(|item: &TranscriptItem| serde_json::to_value(item).expect("an item"))
        .collect()
}

fn ids(rows: &[ChatMessage]) -> Vec<&str> {
    rows.iter().map(|row| row.id.as_str()).collect()
}

/// The verbose transcript's items: `message:<id>` or `work:<opener>-><final or none>`.
fn shape(messages: &[ChatMessage], working: bool) -> Vec<String> {
    project_chat_transcript(messages, working, &[], None)
        .items
        .iter()
        .map(|item| match item {
            RenderItem::Message(row) => format!("message:{}", row.id),
            RenderItem::CompletedWork(turn) => format!(
                "work:{}->{}",
                turn.user.id,
                turn.final_message
                    .as_ref()
                    .map_or("none", |row| row.id.as_str())
            ),
        })
        .collect()
}

/// The work folded under the turn `opener` opened.
fn folded(messages: &[ChatMessage], opener: &str) -> Vec<String> {
    project_chat_transcript(messages, false, &[], None)
        .items
        .iter()
        .find_map(|item| match item {
            RenderItem::CompletedWork(turn) if turn.user.id == opener => {
                Some(turn.work.iter().map(|row| row.id.clone()).collect())
            }
            _ => None,
        })
        .expect("a folded turn")
}

#[test]
fn background_job_results_are_tool_call_rows_with_their_result_duration_and_reason() {
    let [work, result] = tool_work("a1", T0 + MINUTE);
    let items = document(vec![
        row("u1", "user", "Add rate limiting to login", T0),
        work,
        result,
        three_jobs("j1", T0 + 3 * MINUTE),
        row(
            "a2",
            "assistant",
            "Rate limiting covers all three entry points.",
            T0 + 4 * MINUTE,
        ),
    ]);
    let fold = items
        .iter()
        .find(|item| item["kind"] == "completed-work")
        .expect("the settled turn's fold");
    // The three jobs count as tool calls beside the read.
    assert!(fold["stats"]
        .as_array()
        .unwrap()
        .iter()
        .any(|stat| stat["label"] == "4 tool calls"));
    let jobs = fold["work"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "j1")
        .expect("the job row is in the fold");
    assert!(jobs["systemCard"].is_null(), "not the marker line any more");
    let tools = jobs["tools"].as_array().expect("tool rows");
    let labels: Vec<&str> = tools
        .iter()
        .map(|tool| tool["label"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        [
            "Background job \"Find every login entry point\" completed",
            "Background job \"cargo test -p auth\" completed",
            "Background job \"cargo clippy -p auth\" failed",
        ]
    );
    let statuses: Vec<&str> = tools
        .iter()
        .map(|tool| tool["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["done", "done", "failed"]);
    assert_eq!(tools[2]["failure"], "Command exited with code 1");
    assert_eq!(tools[0]["failure"], "");
    // The time OMP recorded each job ran, the way a tool call's duration reads.
    let durations: Vec<&str> = tools
        .iter()
        .map(|tool| tool["duration"].as_str().unwrap())
        .collect();
    assert_eq!(
        durations,
        [
            tool_duration_label(64_000),
            tool_duration_label(48_000),
            tool_duration_label(22_000),
        ]
    );
    assert!(tools.iter().all(|tool| tool["hasDetail"] == true));
}

/// A job OMP cancelled neither finished nor failed: its row and its tool card entry read as
/// stopped. A Claude background task that was stopped keeps its row as before.
#[test]
fn a_cancelled_job_reads_as_stopped_neither_done_nor_failed() {
    let cancelled = omp(
        "j3",
        "async-result",
        json!({ "jobs": [{ "jobId": "5b1d", "status": "cancelled", "label": "cargo bench", "durationMs": 9_000 }] }),
        "<system-notice>\nBackground job 5b1d has completed. Resume your work using the result below.\n\nCancelled by the agent.\n</system-notice>",
        T0 + 2 * MINUTE,
    );
    let items = document(vec![
        row("u1", "user", "Benchmark the limiter", T0),
        row("a1", "assistant", "Started the benchmark.", T0 + MINUTE),
        cancelled.clone(),
        row(
            "a2",
            "assistant",
            "I stopped the benchmark.",
            T0 + 3 * MINUTE,
        ),
    ]);
    let tool = items
        .iter()
        .filter(|item| item["kind"] == "completed-work")
        .flat_map(|item| item["work"].as_array().unwrap().iter())
        .find(|row| row["id"] == "j3")
        .map(|row| row["tools"][0].clone())
        .expect("the job row");
    assert_eq!(tool["label"], "Background job \"cargo bench\" cancelled");
    assert_eq!(tool["status"], "stopped");
    assert_eq!(tool["failed"], false);
    assert_eq!(tool["failure"], "");
    // The tool call card says the same.
    let source = background_task_source(&cancelled).into_owned();
    let card = project_message(
        &source,
        &OnceCell::new(),
        std::slice::from_ref(&source),
        "omp",
        None,
        AgentLineBreaks::default(),
        &context(),
        &HashMap::new(),
        &[],
        true,
        false,
    );
    assert_eq!(card["toolCard"]["entries"][0]["status"], "stopped");
    // A Claude background task that was stopped is not an OMP cancelled job: its row is unchanged.
    let notification = "<task-notification>\n<task-id>b2</task-id>\n<status>killed</status>\n<summary>Background command \"cargo bench\" was stopped</summary>\n</task-notification>";
    let claude = background_task_source(&row("n2", "user", notification, T0)).into_owned();
    let rows = project_message(
        &claude,
        &OnceCell::new(),
        std::slice::from_ref(&claude),
        "claude",
        None,
        AgentLineBreaks::default(),
        &context(),
        &HashMap::new(),
        &[],
        false,
        false,
    );
    assert_eq!(rows["tools"][0]["status"], "done");
}

#[test]
fn each_job_opens_to_its_own_result_without_the_wrapper_and_the_instruction() {
    let source = background_task_source(&three_jobs("j1", T0)).into_owned();
    let outputs: Vec<&str> = source
        .blocks
        .iter()
        .filter_map(|block| match block {
            ghostex_gx_chat_core::protocol::ChatBlock::ToolResult { output, .. } => {
                Some(output.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(outputs.len(), 3);
    assert!(outputs[0].contains("## Login entry points"));
    assert!(!outputs[0].contains("running 112 tests"));
    assert!(outputs[1].contains("test result: ok. 112 passed"));
    assert!(!outputs[1].contains("Login entry points"));
    // A failed job's details say why first, as a failed background command's do.
    assert!(outputs[2].starts_with("Command exited with code 1\n"));
    assert!(outputs[2].contains("error: this `if` has identical blocks"));
    for output in &outputs {
        assert!(!output.contains("<system-notice>"));
        assert!(!output.contains("Resume your work"));
        assert!(!output.contains("── Job"));
    }
    let single = background_task_source(&one_job("j2", T0)).into_owned();
    let ghostex_gx_chat_core::protocol::ChatBlock::ToolResult {
        output, is_error, ..
    } = &single.blocks[1]
    else {
        panic!("a result block");
    };
    assert_eq!(
        output,
        "Background job \"cargo test -p auth\" completed\n\nrunning 112 tests\ntest result: ok. 112 passed; 0 failed"
    );
    assert_eq!(*is_error, None);
}

#[test]
fn an_irc_message_is_the_message_from_card_that_opens_to_the_message_only() {
    let model = projected(&irc("i1", T0 + 8 * MINUTE));
    let card = &model["systemCard"];
    assert_eq!(card["kind"], "agent-message");
    assert_eq!(
        card["body"],
        "Found three login entry points. The SSO callback in `src/http/sso.rs` skips the middleware."
    );
    assert!(!card["markdown"].as_str().unwrap().contains("reply via"));
    let shown = &card["card"];
    assert_eq!(shown["direction"], "received");
    assert_eq!(shown["agentIcon"], "omp");
    assert_eq!(shown["name"], "Message from LoginScout");
    assert_eq!(shown["status"]["label"], "Delivered");
    assert_eq!(shown["status"]["tone"], "good");
    assert_eq!(shown["time"], model["time"]["label"]);
    assert_eq!(shown["time"], "12:38 PM");
    assert_eq!(shown["failure"], "");
    assert!(
        shown["session"].is_null(),
        "an OMP helper is no Ghostex session to jump to"
    );
}

#[test]
fn advisor_notes_are_one_card_with_a_pill_per_severity_and_the_notes_worst_first() {
    let model = projected(&advisor("v1", T0 + 11 * MINUTE));
    let card = &model["systemCard"];
    assert_eq!(card["kind"], "advisor");
    assert_eq!(card["name"], "Advisor notes");
    assert_eq!(card["time"], "12:41 PM");
    let counts: Vec<(&str, &str)> = card["counts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|count| {
            (
                count["label"].as_str().unwrap(),
                count["tone"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(counts, [("1 concern", "waiting"), ("1 nit", "neutral")]);
    let notes: Vec<(&str, &str)> = card["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| {
            (
                note["label"].as_str().unwrap(),
                note["text"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        notes,
        [
            ("Concern", "The limiter keys on the client IP only."),
            ("Nit", "`WINDOW_SECS` is defined twice."),
        ]
    );
    assert!(card["notes"][1]["markdown"]
        .as_str()
        .unwrap()
        .contains("WINDOW_SECS"));
    // The guidance written for the agent is in neither the pills nor the notes the card draws.
    assert!(!card["notes"].to_string().contains("Weigh this note"));
    assert!(!card["counts"].to_string().contains("Weigh this note"));
    // OMP's other severities: a blocker first, a note without one last.
    let mixed = projected(&omp(
        "v2",
        "advisor",
        json!({ "notes": [
            { "note": "Untyped." },
            { "note": "Stop here.", "severity": "blocker" },
            { "note": "Also untyped." },
        ] }),
        "<advisory>\nUntyped.\n</advisory>",
        T0,
    ));
    let labels: Vec<&str> = mixed["systemCard"]["counts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|count| count["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["1 blocker", "2 notes"]);
    assert_eq!(mixed["systemCard"]["notes"][0]["label"], "Blocker");
}

#[test]
fn a_row_from_an_older_gxserver_stays_the_marker_line() {
    let mut old = advisor("v1", T0);
    assert_eq!(projected(&old)["systemCard"]["kind"], "advisor");
    old.custom_message = None;
    let card = &projected(&old)["systemCard"];
    assert_eq!(card["kind"], "marker");
    assert_eq!(card["text"], ADVICE);
    // The projection draws a job row from its tool calls (`background_task_source`).
    let mut old_jobs = three_jobs("j1", T0);
    assert!(projected(&background_task_source(&old_jobs))["systemCard"].is_null());
    old_jobs.custom_message = None;
    assert_eq!(
        projected(&background_task_source(&old_jobs))["systemCard"]["kind"],
        "marker"
    );
    assert_eq!(
        background_task_source(&old_jobs).into_owned(),
        old_jobs,
        "no tool rows without the type"
    );
}

#[test]
fn an_omp_type_the_chat_does_not_draw_stays_the_marker_line() {
    assert_eq!(
        projected(&irc("i1", T0))["systemCard"]["kind"],
        "agent-message"
    );
    let launch = omp(
        "l1",
        "launch-completion",
        json!({ "daemons": [{ "name": "web", "exitCode": 0 }] }),
        "Supervised process completed web (exit 0)",
        T0,
    );
    let card = &projected(&launch)["systemCard"];
    assert_eq!(card["kind"], "marker");
    assert_eq!(card["text"], "Supervised process completed web (exit 0)");
    // A drawn type whose details did not come along is the marker line too.
    let mut bare = irc("i2", T0);
    bare.custom_message.as_mut().unwrap().details = None;
    assert_eq!(projected(&bare)["systemCard"]["kind"], "marker");
}

#[test]
fn a_client_that_ignores_the_field_still_reads_the_rows_text() {
    let wire = json!({
        "id": "v1",
        "role": "system",
        "source": "transcript",
        "timestamp": T0,
        "blocks": [{ "type": "text", "text": ADVICE }],
        "customMessage": { "customType": "advisor", "details": { "notes": [{ "note": "x", "severity": "nit" }] } },
    });
    let with_field = message(wire.clone());
    assert_eq!(
        with_field
            .custom_message
            .as_ref()
            .map(|custom| custom.custom_type.as_str()),
        Some("advisor")
    );
    let mut without = wire;
    without.as_object_mut().unwrap().remove("customMessage");
    let mut ignored = with_field.clone();
    ignored.custom_message = None;
    assert_eq!(
        ignored,
        message(without),
        "the field changes nothing else on the row"
    );
    // What a renderer that does not know the advisor card falls back to is the text it drew before.
    let model = projected(&with_field);
    assert_eq!(model["systemCard"]["kind"], "advisor");
    assert_eq!(model["systemCard"]["text"], ADVICE);
    assert_eq!(model["text"], projected(&ignored)["text"]);
}

#[test]
fn an_omp_wake_up_after_the_reply_opens_its_own_turn() {
    let wakes: [fn(&str, i64) -> ChatMessage; 3] = [three_jobs, irc, advisor];
    for wake in wakes {
        let [work, result] = tool_work("a0", T0 + MINUTE);
        let messages = vec![
            row("u1", "user", "Add rate limiting to login", T0),
            work,
            result,
            row(
                "a1",
                "assistant",
                "Started the test run in the background.",
                T0 + 2 * MINUTE,
            ),
            wake("w1", T0 + 5 * MINUTE),
            row("a2", "assistant", "The tests pass.", T0 + 6 * MINUTE),
        ];
        assert_eq!(
            shape(&messages, false),
            ["message:u1", "work:u1->a1", "work:w1->a2"],
            "{}",
            messages[4].custom_message.as_ref().unwrap().custom_type
        );
        assert_eq!(
            project_chat_transcript(&messages, false, &[], None).final_ids,
            ["a1", "a2"]
        );
        // The rows before the wake-up stay in the first turn's fold; the wake-up folds with its own.
        let first = folded(&messages, "u1");
        assert!(first.iter().any(|id| id == "a0"));
        assert!(!first.iter().any(|id| id == "w1"));
        assert_eq!(folded(&messages, "w1"), ["w1"]);
    }
}

#[test]
fn an_omp_message_while_the_agent_works_stays_in_its_turn() {
    let mids: [fn(&str, i64) -> ChatMessage; 3] = [three_jobs, irc, advisor];
    for mid in mids {
        let [work, result] = tool_work("a0", T0 + MINUTE);
        let messages = vec![
            row("u1", "user", "Add rate limiting to login", T0),
            work,
            result,
            mid("m1", T0 + 2 * MINUTE),
            row("a1", "assistant", "Done.", T0 + 3 * MINUTE),
        ];
        assert_eq!(shape(&messages, false), ["message:u1", "work:u1->a1"]);
        assert!(folded(&messages, "u1").iter().any(|id| id == "m1"));
    }
}

#[test]
fn a_running_wake_up_turn_stays_open_below_the_folded_reply() {
    let messages = vec![
        row("u1", "user", "Add rate limiting to login", T0),
        row(
            "a1",
            "assistant",
            "Started the test run in the background.",
            T0 + MINUTE,
        ),
        three_jobs("w1", T0 + 5 * MINUTE),
        row("a2", "assistant", "Weighing the results.", T0 + 6 * MINUTE),
    ];
    assert_eq!(
        shape(&messages, true),
        ["message:u1", "work:u1->a1", "message:w1", "message:a2"]
    );
}

#[test]
fn summary_mode_keeps_the_reply_before_an_omp_wake_up() {
    let messages = vec![
        row("u1", "user", "Add rate limiting to login", T0),
        row(
            "a1",
            "assistant",
            "Started the test run in the background.",
            T0 + MINUTE,
        ),
        irc("w1", T0 + 5 * MINUTE),
        row(
            "a2",
            "assistant",
            "LoginScout found the SSO gap.",
            T0 + 6 * MINUTE,
        ),
    ];
    let turns = project_chat_transcript(&messages, false, &[], None).summary_turns;
    assert_eq!(turns.len(), 1, "a wake-up opens no summary turn of its own");
    assert_eq!(
        turns[0].final_message.as_ref().map(|row| row.id.as_str()),
        Some("a2")
    );
    assert_eq!(ids(&turns[0].earlier_replies), ["a1"]);
}

/// OMP's `/tan` dispatch notice is appended with `triggerTurn: false`
/// (`modes/controllers/tan-command-controller.ts`): it never starts a turn.
#[test]
fn an_omp_type_that_starts_no_turn_keeps_its_place_in_the_turn() {
    let dispatch = omp(
        "d1",
        "background-tan-dispatch",
        json!({}),
        "Dispatched a background task",
        T0 + 5 * MINUTE,
    );
    let messages = vec![
        row("u1", "user", "Start the dev server", T0),
        row("a1", "assistant", "Started it.", T0 + MINUTE),
        dispatch,
        row("a2", "assistant", "It exited cleanly.", T0 + 6 * MINUTE),
    ];
    assert_eq!(shape(&messages, false), ["message:u1", "work:u1->a2"]);
    // Next to it, the same place with a background job opens a turn.
    let mut with_job = messages.clone();
    with_job[2] = three_jobs("d1", T0 + 5 * MINUTE);
    assert_eq!(
        shape(&with_job, false),
        ["message:u1", "work:u1->a1", "work:d1->a2"]
    );
}

/// The other visible OMP messages that start a turn when the agent is idle: `launch-completion`
/// and `lsp-late-diagnostic` (the yield queue's idle flush), `live-delegation` (`triggerTurn`),
/// `collab-prompt` and `skill-prompt` (`promptCustomMessage`). After the reply each opens its own
/// turn and stays the marker line; while the agent works it stays in the turn.
#[test]
fn other_visible_omp_messages_that_start_a_turn_open_one_after_the_reply() {
    for custom_type in [
        "launch-completion",
        "lsp-late-diagnostic",
        "live-delegation",
        "collab-prompt",
        "skill-prompt",
    ] {
        let wake = omp(
            "w1",
            custom_type,
            json!({}),
            "Supervised process completed web (exit 0)",
            T0 + 5 * MINUTE,
        );
        assert_eq!(
            projected(&wake)["systemCard"]["kind"],
            "marker",
            "{custom_type}"
        );
        let messages = vec![
            row("u1", "user", "Start the dev server", T0),
            row("a1", "assistant", "Started it.", T0 + MINUTE),
            wake.clone(),
            row("a2", "assistant", "It exited cleanly.", T0 + 6 * MINUTE),
        ];
        assert_eq!(
            shape(&messages, false),
            ["message:u1", "work:u1->a1", "work:w1->a2"],
            "{custom_type}"
        );
        let [work, result] = tool_work("a0", T0 + MINUTE);
        let mid = vec![
            row("u1", "user", "Start the dev server", T0),
            work,
            result,
            wake,
            row("a1", "assistant", "Done.", T0 + 6 * MINUTE),
        ];
        assert_eq!(
            shape(&mid, false),
            ["message:u1", "work:u1->a1"],
            "{custom_type}"
        );
    }
}

#[test]
fn other_agents_rows_keep_their_cards_and_turns() {
    // Claude: a background task that ends after the reply opens a turn and is a tool-call row.
    let notification = "<task-notification>\n<task-id>b1</task-id>\n<status>completed</status>\n<summary>Background command \"cargo test\" completed (exit code 0)</summary>\n</task-notification>";
    let claude = vec![
        row("u1", "user", "Run the tests in the background", T0),
        row("a1", "assistant", "Started them.", T0 + MINUTE),
        row("n1", "user", notification, T0 + 5 * MINUTE),
        row("a2", "assistant", "They pass.", T0 + 6 * MINUTE),
    ];
    assert_eq!(
        shape(&claude, false),
        ["message:u1", "work:u1->a1", "work:n1->a2"]
    );
    let source = background_task_source(&claude[2]).into_owned();
    assert!(matches!(
        &source.blocks[0],
        ghostex_gx_chat_core::protocol::ChatBlock::ToolCall { name, .. } if name == "Background command"
    ));
    // Codex: a subagent's message on a system row stays the subagent's card.
    let codex = row("c1", "system", "Message from Explore\n\nFound it.", T0);
    let card = &projected(&codex)["systemCard"];
    assert_eq!(card["kind"], "agent-message");
    assert_eq!(card["card"]["direction"], "subagent");
    // Plain pi (and Hermes): a system row after the reply is a marker that opens no turn.
    let pi = vec![
        row("u1", "user", "Summarize", T0),
        row("a1", "assistant", "Here is the summary.", T0 + MINUTE),
        row(
            "s1",
            "system",
            "Branch summary: earlier work on the parser.",
            T0 + 2 * MINUTE,
        ),
        row("a2", "assistant", "Anything else?", T0 + 3 * MINUTE),
    ];
    assert_eq!(shape(&pi, false), ["message:u1", "work:u1->a2"]);
    assert_eq!(projected(&pi[2])["systemCard"]["kind"], "marker");
}

/// The rows gxserver's collapsed snapshot keeps for `u1, a1, IRC wake, a2, u2, a3`
/// (`collapse_snapshot_messages`, server/src/session_chat_history.rs): both replies of the first
/// prompt survive, so a reopened chat shows the same turns as the full list.
#[test]
fn a_reopened_chat_keeps_the_reply_before_an_omp_wake_up() {
    let mut u1 = row("u1", "user", "Add rate limiting to login", T0);
    u1.byte_offset = Some(10);
    u1.deferred_work = Some(
        serde_json::from_value(
            json!({ "beforeOffset": 50, "startId": "u1", "endId": "a2", "messageCount": 4 }),
        )
        .unwrap(),
    );
    let mut u2 = row("u2", "user", "Thanks", T0 + 7 * MINUTE);
    u2.byte_offset = Some(50);
    let mut collapsed = vec![
        u1,
        row("a1", "assistant", "Started the test run.", T0 + MINUTE),
        irc("w1", T0 + 5 * MINUTE),
        row(
            "a2",
            "assistant",
            "LoginScout found the gap.",
            T0 + 6 * MINUTE,
        ),
        u2,
        row("a3", "assistant", "You're welcome.", T0 + 8 * MINUTE),
    ];
    // Every row the server sends carries its line's offset.
    for (index, row) in collapsed.iter_mut().enumerate() {
        row.byte_offset = Some(10 * (index as u64 + 1));
    }
    assert_eq!(
        shape(&collapsed, false),
        [
            "message:u1",
            "work:u1->a1",
            "work:w1->a2",
            "message:u2",
            "work:u2->a3"
        ]
    );
}

/// OMP's `/tan` dispatch notice (`modes/controllers/tan-command-controller.ts`) between the reply
/// and a real job delivery does not hide the reply: the delivery still opens its own turn.
#[test]
fn a_passive_omp_notice_does_not_hide_the_reply_before_a_wake_up() {
    let messages = vec![
        row("u1", "user", "Benchmark the limiter", T0),
        row("a1", "assistant", "Dispatched the benchmark.", T0 + MINUTE),
        omp(
            "d1",
            "background-tan-dispatch",
            json!({}),
            "Background task dispatched",
            T0 + 2 * MINUTE,
        ),
        one_job("j1", T0 + 4 * MINUTE),
        row("a2", "assistant", "The benchmark is done.", T0 + 5 * MINUTE),
    ];
    assert_eq!(
        shape(&messages, false),
        ["message:u1", "work:u1->a1", "work:j1->a2"]
    );
    // After an unfinished tool call it is no wake-up, notice or not.
    let [work, result] = tool_work("a0", T0 + MINUTE);
    let mid = vec![
        row("u1", "user", "Benchmark the limiter", T0),
        work,
        result,
        omp(
            "d1",
            "background-tan-dispatch",
            json!({}),
            "Background task dispatched",
            T0 + 2 * MINUTE,
        ),
        one_job("j1", T0 + 4 * MINUTE),
        row("a2", "assistant", "The benchmark is done.", T0 + 5 * MINUTE),
    ];
    assert_eq!(shape(&mid, false), ["message:u1", "work:u1->a2"]);
}

/// OMP writes a delivery it records without a turn (a preserved advisor card, an IRC message in
/// plan mode, a result after the user stopped the agent) like any other; no agent row follows it
/// before the next prompt, so it stays in the previous turn's fold.
#[test]
fn an_omp_row_the_agent_did_not_answer_stays_in_the_previous_fold() {
    for kept in [advisor("v1", T0 + 5 * MINUTE), irc("v1", T0 + 5 * MINUTE)] {
        let messages = vec![
            row("u1", "user", "Add rate limiting to login", T0),
            row("a1", "assistant", "Done.", T0 + MINUTE),
            kept,
            row("u2", "user", "Thanks", T0 + 7 * MINUTE),
            row("a2", "assistant", "You're welcome.", T0 + 8 * MINUTE),
        ];
        assert_eq!(
            shape(&messages, false),
            ["message:u1", "work:u1->a1", "message:u2", "work:u2->a2"]
        );
        assert!(folded(&messages, "u1").iter().any(|id| id == "v1"));
    }
}

/// Job parts go to their jobs only when OMP's `── Job <id>` lines name those jobs in order.
#[test]
fn job_results_in_another_order_show_the_whole_text() {
    let text = "<system-notice>\n2 background jobs have completed. Resume your work using the results below.\n\n── Job beta (beta) ──\nBETA RESULT\n── Job alpha (alpha) ──\nALPHA RESULT\n</system-notice>";
    let row = omp(
        "j1",
        "async-result",
        json!({ "jobs": [
            { "jobId": "alpha", "status": "completed", "label": "alpha" },
            { "jobId": "beta", "status": "completed", "label": "beta" },
        ] }),
        text,
        T0,
    );
    let source = background_task_source(&row).into_owned();
    let outputs: Vec<&str> = source
        .blocks
        .iter()
        .filter_map(|block| match block {
            ghostex_gx_chat_core::protocol::ChatBlock::ToolResult { output, .. } => {
                Some(output.as_str())
            }
            _ => None,
        })
        .collect();
    for output in outputs {
        assert!(output.contains("ALPHA RESULT") && output.contains("BETA RESULT"));
    }
}
