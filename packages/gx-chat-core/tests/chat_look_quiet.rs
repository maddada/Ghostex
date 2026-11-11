//! Agent work Quiet and Reduced (`transcript/chat_look.rs`): the edges where work escaped them. No
//! tool-call row draws in the moment between an agent asking a question and its card arriving, a
//! question's sentence never shows as a reply once the answer starts a new run, an agent that dies
//! on a call shows the words Quiet held back, and the subagent viewer reads in the main chat's look.

use ghostex_gx_chat_core::transcript::chat_look::ChatLookSettings;
use ghostex_gx_chat_core::{ChatContext, ChatState, TranscriptItem};
use ghostex_gx_protocol::{ChatMessage, TurnLifecycle, TurnLifecycleState};
use serde_json::{json, Value};

const T0: i64 = 1_793_818_000_000;

/// The rows as the core holds them (`ChatRow`).
fn shared(rows: &[ChatMessage]) -> Vec<ghostex_gx_chat_core::state::ChatRow> {
    rows.iter().cloned().map(std::sync::Arc::new).collect()
}

fn message(value: Value) -> ChatMessage {
    serde_json::from_value(value).expect("a chat message")
}

fn row(id: &str, role: &str, at: i64, blocks: Value) -> ChatMessage {
    message(
        json!({ "id": id, "role": role, "timestamp": at, "source": "transcript", "blocks": blocks }),
    )
}

fn call(id: &str, at: i64, name: &str, input: Value) -> ChatMessage {
    row(
        id,
        "assistant",
        at,
        json!([{ "type": "tool-call", "name": name, "callId": id, "input": input }]),
    )
}

fn result(id: &str, at: i64, call: &str, output: &str) -> ChatMessage {
    row(
        id,
        "tool",
        at,
        json!([{ "type": "tool-result", "callId": call, "output": output }]),
    )
}

fn words(id: &str, at: i64, text: &str) -> ChatMessage {
    row(
        id,
        "assistant",
        at,
        json!([{ "type": "text", "text": text }]),
    )
}

fn ask(id: &str, at: i64) -> ChatMessage {
    call(
        id,
        at,
        "AskUserQuestion",
        json!({ "questions": [{ "question": "Which color?", "header": "Color", "multiSelect": false,
            "options": [{ "label": "Red", "description": "r" }, { "label": "Blue", "description": "b" }] }] }),
    )
}

fn answered(id: &str, at: i64, call: &str) -> ChatMessage {
    result(
        id,
        at,
        call,
        "User has answered your questions: \"Which color?\"=\"Red\".",
    )
}

/// The turn so far: a prompt, two calls with the agent's words between them.
fn turn() -> Vec<ChatMessage> {
    vec![
        row("u1", "user", T0, json!([{ "type": "text", "text": "go" }])),
        words("w1", T0 + 500, "Looking around first."),
        call("c1", T0 + 1_000, "Bash", json!({ "command": "ls" })),
        result("r1", T0 + 2_000, "c1", "a\nb"),
        call("c2", T0 + 3_000, "Read", json!({ "file_path": "/tmp/a" })),
        result("r2", T0 + 4_000, "c2", "x"),
    ]
}

fn look(state: &mut ChatState, agent_work: &str) {
    state.transcript_view.chat_look =
        state
            .transcript_view
            .chat_look
            .with_settings(&ChatLookSettings {
                agent_work: Some(agent_work.into()),
                ..Default::default()
            });
}

fn quiet_chat(rows: Vec<ChatMessage>) -> ChatState {
    let mut state = ChatState::default();
    state.session.agent = Some("claude".into());
    look(&mut state, "quiet");
    set_rows(&mut state, rows);
    working(&mut state, true, T0);
    state
}

fn set_rows(state: &mut ChatState, mut rows: Vec<ChatMessage>) {
    for (at, row) in rows.iter_mut().enumerate() {
        row.byte_offset.get_or_insert(at as u64 * 100);
    }
    state.messages.composed = shared(&rows);
    state.messages.list = shared(&rows);
    state.messages.compose_generation += 1;
}

/// The agent's live signal, and a Working lifecycle stamped at `since`.
fn working(state: &mut ChatState, on: bool, since: i64) {
    state.session.server_working = on;
    state.session.working_started_at_ms = on.then_some(since as f64);
    state.session.lifecycle = Some(TurnLifecycle {
        state: TurnLifecycleState::Working,
        turn_id: "u1".into(),
        timestamp: Some(since),
    });
}

fn question_prompt() -> Value {
    json!({ "kind": "question", "toolUseId": "q1",
        "questions": [{ "question": "Which color?", "header": "Color",
            "options": [{ "label": "Red" }, { "label": "Blue" }] }] })
}

fn items(state: &mut ChatState, now: i64) -> Vec<Value> {
    let context = ChatContext {
        now_ms: now as f64,
        ..Default::default()
    };
    // The pass a publish runs: the session settle (which clears a settled Stop), then the rows.
    ghostex_gx_chat_core::session::before_compose(state, &context);
    ghostex_gx_chat_core::transcript::rows::refresh(state, &context);
    ghostex_gx_chat_core::transcript::rows::rows(state, &context)
        .iter()
        .map(|item| serde_json::to_value(item).expect("an item"))
        .collect()
}

/// The agent's words the list draws, inside grouped rows too.
fn drawn_words(items: &[Value]) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        let mut push = |message: &Value| {
            if message["role"] == "user" {
                return;
            }
            if let Some(text) = message["text"].as_str().filter(|text| !text.is_empty()) {
                out.push(text.to_string());
            }
        };
        match item["kind"].as_str() {
            Some("message") => push(&item["message"]),
            Some("grouped-work") => {
                for turn in item["turns"].as_array().into_iter().flatten() {
                    push(&turn["final"]);
                }
            }
            _ => {}
        }
    }
    out
}

/// How many answered question cards the list draws.
fn answered_cards(items: &[Value]) -> usize {
    let count = |value: &Value| value.as_array().map_or(0, Vec::len);
    items
        .iter()
        .map(|item| match item["kind"].as_str() {
            Some("message") => count(&item["message"]["questions"]),
            Some("grouped-work") => item["turns"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|turn| count(&turn["questions"]))
                .sum(),
            _ => 0,
        })
        .sum()
}

fn kinds(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .map(|item| item["kind"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn no_tool_call_row_draws_before_a_question_card() {
    let mut rows = turn();
    rows.push(ask("q1", T0 + 5_000));
    let mut state = quiet_chat(rows);
    assert_eq!(
        kinds(&items(&mut state, T0 + 5_100)),
        ["message"],
        "the agent works"
    );
    // The agent stops reporting work as it asks, a moment before the card arrives.
    working(&mut state, false, T0);
    let gap = items(&mut state, T0 + 5_200);
    assert_eq!(
        kinds(&gap),
        ["message"],
        "nothing but the prompt draws: {gap:#?}"
    );
    state.session.prompt = Some(question_prompt());
    assert_eq!(
        kinds(&items(&mut state, T0 + 5_300)),
        ["message"],
        "the card is up"
    );
}

#[test]
fn stop_releases_the_question_hold() {
    let mut rows = turn();
    rows.push(ask("q1", T0 + 5_000));
    let mut state = quiet_chat(rows);
    items(&mut state, T0 + 5_100);
    // Stop lands after the question call, before its card, while the agent still reads as working.
    let context = ChatContext {
        now_ms: (T0 + 5_150) as f64,
        ..Default::default()
    };
    assert!(ghostex_gx_chat_core::session::sends::begin_interrupt(
        &mut state, &context
    ));
    let stopped = items(&mut state, T0 + 5_200);
    assert_ne!(
        kinds(&stopped),
        ["message"],
        "the interrupted turn is in view: {stopped:#?}"
    );
    // An idle update clears the Stop flag before the call's result or lifecycle arrives.
    working(&mut state, false, T0);
    let idle = items(&mut state, T0 + 5_300);
    assert!(!state.session.interrupted, "the settle cleared the flag");
    assert_ne!(
        kinds(&idle),
        ["message"],
        "the stopped turn stays in view: {idle:#?}"
    );
}

#[test]
fn a_question_s_sentence_and_its_card_show_once_when_the_answer_starts_a_new_run() {
    let mut rows = turn();
    rows.push(words("s1", T0 + 4_500, "Which color do you want?"));
    rows.push(ask("q1", T0 + 5_000));
    let mut state = quiet_chat(rows.clone());
    state.session.prompt = Some(question_prompt());
    items(&mut state, T0 + 5_300);
    // The answer lands and the agent runs on, its lifecycle stamped at the answer.
    state.session.prompt = None;
    rows.push(answered("a1", T0 + 9_000, "q1"));
    rows.push(words("reply", T0 + 9_500, "Red it is."));
    set_rows(&mut state, rows);
    working(&mut state, true, T0 + 9_000);
    let running = items(&mut state, T0 + 9_600);
    assert!(
        drawn_words(&running).is_empty(),
        "no words while it runs: {running:#?}"
    );
    assert_eq!(
        answered_cards(&running),
        1,
        "one answered card: {running:#?}"
    );
    state.session.server_working = false;
    state.session.lifecycle = Some(TurnLifecycle {
        state: TurnLifecycleState::Completed,
        turn_id: "u1".into(),
        timestamp: Some(T0 + 10_000),
    });
    let done = items(&mut state, T0 + 11_000);
    assert_eq!(drawn_words(&done), ["Red it is."], "{done:#?}");
    assert_eq!(answered_cards(&done), 1, "{done:#?}");
}

#[test]
fn the_words_quiet_held_back_show_once_the_agent_has_left_its_terminal() {
    let mut rows = turn();
    rows.push(ask("q1", T0 + 5_000));
    let mut state = quiet_chat(rows.clone());
    working(&mut state, false, T0);
    state.session.prompt = Some(question_prompt());
    items(&mut state, T0 + 5_300);
    // Answered, then the agent starts a call and dies on it without reporting anything.
    state.session.prompt = None;
    rows.push(answered("a1", T0 + 9_000, "q1"));
    rows.push(words("w2", T0 + 9_500, "Now writing the file."));
    rows.push(call(
        "c3",
        T0 + 9_600,
        "Bash",
        json!({ "command": "sleep 100" }),
    ));
    set_rows(&mut state, rows.clone());
    let held = items(&mut state, T0 + 9_700);
    assert!(
        !drawn_words(&held)
            .iter()
            .any(|text| text.contains("Now writing")),
        "the hold keeps the words between calls hidden: {held:#?}"
    );
    state.session.terminal_notice = Some(json!({ "kind": "agentExited", "severity": "warning",
        "title": "Claude Code is no longer running in this terminal" }));
    let stopped = items(&mut state, T0 + 20_000);
    let shown = drawn_words(&stopped);
    assert!(
        shown.iter().any(|text| text.contains("Now writing"))
            || kinds(&stopped).contains(&"grouped-work".to_string()),
        "the work is in view once the agent left: {stopped:#?}"
    );
    assert!(
        shown.iter().any(|text| text.contains("Looking around")) || kinds(&stopped).len() > 1,
        "{stopped:#?}"
    );
}

/// A subagent's page: one finished turn with a call and a reply.
fn subagent_chat(agent_work: Option<&str>) -> Vec<String> {
    let mut state = ChatState::default();
    if let Some(agent_work) = agent_work {
        look(&mut state, agent_work);
    }
    let rows = [
        row(
            "su1",
            "user",
            T0,
            json!([{ "type": "text", "text": "Explore the repo" }]),
        ),
        call("sc1", T0 + 1_000, "Bash", json!({ "command": "ls" })),
        result("sr1", T0 + 2_000, "sc1", "a"),
        words("sreply", T0 + 3_000, "The repo has one file."),
    ];
    state.extras.subagent.page = Some(json!({
        "subagent": { "name": "explorer" },
        "messages": rows.iter().map(|row| serde_json::to_value(row).unwrap()).collect::<Vec<_>>(),
    }));
    let context = ChatContext {
        now_ms: (T0 + 60_000) as f64,
        ..Default::default()
    };
    ghostex_gx_chat_core::extras::subagent_rows::refresh(&mut state, &context);
    let items: Vec<Value> = ghostex_gx_chat_core::extras::subagent_rows::rows(&state, &context)
        .iter()
        .map(|item: &TranscriptItem| serde_json::to_value(item).unwrap())
        .collect();
    // No child reply carries the work button: it stands on its own row everywhere.
    assert!(
        items.iter().all(|item| item["workCarrier"].is_null()),
        "{items:#?}"
    );
    kinds(&items)
}

#[test]
fn the_subagent_viewer_reads_in_the_main_chat_s_look() {
    assert_eq!(subagent_chat(None), ["message", "completed-work"]);
    assert_eq!(subagent_chat(Some("reduced")), ["message", "grouped-work"]);
    assert_eq!(subagent_chat(Some("quiet")), ["message", "grouped-work"]);
}

/// A Reduced chat running a described Bash call after some earlier words and calls.
fn reduced_running(summary: bool, detailed: bool) -> ChatState {
    let mut rows = turn();
    rows.push(words("w2", T0 + 5_000, "Building now."));
    rows.push(call(
        "c3",
        T0 + 6_000,
        "Bash",
        json!({ "command": "cargo build", "description": "Build the project" }),
    ));
    let mut state = quiet_chat(rows);
    look(&mut state, "reduced");
    state.transcript_view.summary_mode = summary;
    state.transcript_view.simple_override = Some(!detailed);
    state
}

/// The summary turn's id and the id of the row running a call, wherever the list draws it.
fn running_row(items: &[Value]) -> (Option<String>, String) {
    let runs = |row: &Value| {
        row["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["status"] == "running"))
    };
    let id = |row: &Value| row["id"].as_str().unwrap_or_default().to_string();
    for item in items.iter().rev() {
        match item["kind"].as_str() {
            Some("message") if runs(&item["message"]) => return (None, id(&item["message"])),
            Some("summary") => {
                let rows = ["cards", "work"]
                    .iter()
                    .flat_map(|key| item[*key].as_array().into_iter().flatten());
                for row in rows {
                    if runs(row) {
                        return (Some(id(item)), id(row));
                    }
                }
            }
            _ => {}
        }
    }
    panic!("no running row: {items:#?}")
}

fn status_bar(state: &ChatState) -> Option<String> {
    ghostex_gx_chat_core::extras::working_card::working_card(
        state,
        &ghostex_gx_chat_core::WorkingStrip::default(),
        &[],
    )
    .label
}

fn report_folds(state: &mut ChatState, expanded: &[String], collapsed: &[String]) {
    let folds = &mut state.extras.search.folds;
    folds.expanded = expanded.iter().cloned().collect();
    folds.collapsed = collapsed.iter().cloned().collect();
}

/// The status bar names the running call exactly when the reader's folds leave its words shut.
#[test]
fn the_status_bar_follows_the_folds_the_reader_opened_or_closed_under_reduced() {
    // Summary view: Active work and the row's line start shut, so the bar names the call.
    let mut state = reduced_running(true, false);
    let list = items(&mut state, T0 + 6_500);
    assert_eq!(status_bar(&state).as_deref(), Some("Build the project"));
    // The reader opens Active work, then the row's line: the call's words are in the chat now.
    let (summary, row) = running_row(&list);
    let summary = summary.expect("the running row sits in the summary turn");
    report_folds(
        &mut state,
        &[format!("active-work:{summary}"), format!("tools:{row}")],
        &[],
    );
    assert_ne!(
        status_bar(&state).as_deref(),
        Some("Build the project"),
        "the words show once"
    );
    // Detailed view: the line starts open, so the bar leaves the call out until the reader closes it.
    let mut state = reduced_running(false, true);
    let list = items(&mut state, T0 + 6_500);
    assert_ne!(status_bar(&state).as_deref(), Some("Build the project"));
    let (_, row) = running_row(&list);
    report_folds(&mut state, &[], &[format!("tools:{row}")]);
    assert_eq!(
        status_bar(&state).as_deref(),
        Some("Build the project"),
        "a closed line leaves the words to the bar"
    );
}

/// Reduced, Summary view: a new approval shows outside the shut Active work fold, and the fold
/// stays shut over the earlier, unrelated work.
#[test]
fn an_approval_keeps_the_summary_work_shut_under_reduced() {
    let mut state = reduced_running(true, false);
    state.session.prompt = Some(json!({
        "kind": "approval", "tool": "Bash", "summary": "cargo build", "toolUseId": "c3",
    }));
    let list = items(&mut state, T0 + 6_500);
    let summary = list
        .iter()
        .rev()
        .find(|item| item["kind"] == "summary")
        .expect("a summary turn");
    let carded: Vec<&str> = summary["cards"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert!(!carded.is_empty(), "the waiting row shows: {summary:#?}");
    assert_ne!(
        summary["workOpen"], true,
        "Active work stays shut: {summary:#?}"
    );
}

/// The summary turn's id, and where it draws the row running the call described as
/// `description`: `cards` (in full under the prompt, with its calls on a line) or `work`.
fn summary_running_row(items: &[Value], description: &str) -> (String, String, &'static str) {
    for item in items.iter().rev() {
        if item["kind"] != "summary" {
            continue;
        }
        for key in ["cards", "work"] {
            for row in item[key].as_array().into_iter().flatten() {
                let holds = row["tools"].as_array().is_some_and(|tools| {
                    tools.iter().any(|tool| {
                        tool["status"] == "running" && tool["description"] == description
                    })
                });
                if holds {
                    return (
                        item["id"].as_str().unwrap_or_default().to_string(),
                        row["id"].as_str().unwrap_or_default().to_string(),
                        key,
                    );
                }
            }
        }
    }
    panic!("no running row for {description:?}: {items:#?}")
}

/// Two calls still out in separate rows of the Summary view's running turn: an older one under a
/// one-click question, drawn under the prompt with its calls on a line, and the newest in the shut
/// Active work. The status bar names the newest call, so it follows that call's folds, never the
/// older row's line.
#[test]
fn the_status_bar_follows_the_folds_of_the_call_it_names() {
    let rows = vec![
        row("u1", "user", T0, json!([{ "type": "text", "text": "go" }])),
        words(
            "w1",
            T0 + 500,
            "Keep the lockfile? [Yes](ghostex-answer:Q1) [No](ghostex-answer:Q1)",
        ),
        call(
            "c1",
            T0 + 1_000,
            "Bash",
            json!({ "command": "npm install", "description": "Install the deps" }),
        ),
        words("w2", T0 + 5_000, "Building now."),
        call(
            "c2",
            T0 + 6_000,
            "Bash",
            json!({ "command": "cargo build", "description": "Build the project" }),
        ),
    ];
    let mut state = quiet_chat(rows);
    look(&mut state, "reduced");
    state.transcript_view.summary_mode = true;
    let list = items(&mut state, T0 + 6_500);
    assert_eq!(status_bar(&state).as_deref(), Some("Build the project"));
    let (summary, older, older_in) = summary_running_row(&list, "Install the deps");
    let (_, newer, newer_in) = summary_running_row(&list, "Build the project");
    assert_eq!(
        (older_in, newer_in),
        ("cards", "work"),
        "the question's row shows, the newest work stays in the fold: {list:#?}"
    );
    assert_ne!(older, newer);
    let work = format!("active-work:{summary}");
    let older_line = format!("tools:{older}");
    // The older row's line open, Active work shut: the newest call's words are still hidden.
    report_folds(&mut state, &[older_line.clone()], &[]);
    assert_eq!(
        status_bar(&state).as_deref(),
        Some("Build the project"),
        "the bar keeps naming the call it names"
    );
    // Active work open, the older row's line shut: the newest call's words are in the chat.
    report_folds(&mut state, &[work], &[older_line]);
    assert_ne!(
        status_bar(&state).as_deref(),
        Some("Build the project"),
        "the words show once"
    );
}

/// Every answered question card a list draws: on rows, and on a grouped turn's tail (its own
/// cards, its artifacts and its reply).
fn every_answered_card(items: &[Value]) -> usize {
    let count = |value: &Value| value.as_array().map_or(0, Vec::len);
    items
        .iter()
        .map(|item| match item["kind"].as_str() {
            Some("message") => count(&item["message"]["questions"]),
            Some("completed-work") => {
                count(&item["questions"])
                    + count(&item["final"]["questions"])
                    + item["artifacts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|row| count(&row["questions"]))
                        .sum::<usize>()
            }
            Some("grouped-work") => item["turns"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|turn| {
                    count(&turn["questions"])
                        + count(&turn["final"]["questions"])
                        + turn["artifacts"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|row| count(&row["questions"]))
                            .sum::<usize>()
                })
                .sum(),
            _ => 0,
        })
        .sum()
}

/// The turn's Completed reaches the chat a moment before the reply the agent wrote after the
/// answer: for that moment the list ends at the answer. The question's sentence is no reply and
/// its answered card shows once, under Quiet and Reduced, before and after the reply lands.
#[test]
fn a_completed_turn_before_its_reply_shows_the_answered_card_once() {
    for agent_work in ["quiet", "reduced"] {
        let mut rows = turn();
        rows.push(words("s1", T0 + 4_500, "Which color do you want?"));
        rows.push(ask("q1", T0 + 5_000));
        let mut state = quiet_chat(rows.clone());
        look(&mut state, agent_work);
        state.session.prompt = Some(question_prompt());
        items(&mut state, T0 + 5_300);
        // Answered; the agent replies and stops, and its Completed lands before the reply row.
        state.session.prompt = None;
        rows.push(answered("a1", T0 + 9_000, "q1"));
        set_rows(&mut state, rows.clone());
        state.session.server_working = false;
        state.session.lifecycle = Some(TurnLifecycle {
            state: TurnLifecycleState::Completed,
            turn_id: "u1".into(),
            timestamp: Some(T0 + 9_600),
        });
        let gap = items(&mut state, T0 + 9_700);
        assert_eq!(every_answered_card(&gap), 1, "{agent_work}: {gap:#?}");
        assert!(
            !drawn_words(&gap)
                .iter()
                .any(|text| text.contains("Which color")),
            "{agent_work}: the question's sentence is no reply: {gap:#?}"
        );
        rows.push(words("reply", T0 + 9_500, "Red it is."));
        set_rows(&mut state, rows);
        let done = items(&mut state, T0 + 10_000);
        assert_eq!(every_answered_card(&done), 1, "{agent_work}: {done:#?}");
        assert_eq!(
            drawn_words(&done),
            ["Red it is."],
            "{agent_work}: {done:#?}"
        );
    }
}

/// A question asked in the same message as a picture: the picture keeps the row in view, so the
/// row's own answered card is the only one, whether the row is the reply or the work.
#[test]
fn a_question_beside_a_picture_keeps_one_answered_card() {
    for agent_work in ["quiet", "reduced", "default"] {
        let mut rows = turn();
        rows.push(row(
            "pic",
            "assistant",
            T0 + 4_500,
            json!([
                { "type": "image-ref", "path": "/shots/after.png" },
                { "type": "text", "text": "Here is the shot. Which color do you want?" },
                { "type": "tool-call", "name": "AskUserQuestion", "callId": "q1",
                  "input": { "questions": [{ "question": "Which color?", "header": "Color", "multiSelect": false,
                    "options": [{ "label": "Red", "description": "r" }, { "label": "Blue", "description": "b" }] }] } },
            ]),
        ));
        rows.push(answered("a1", T0 + 9_000, "q1"));
        let mut state = quiet_chat(rows);
        look(&mut state, agent_work);
        state.session.server_working = false;
        state.session.lifecycle = Some(TurnLifecycle {
            state: TurnLifecycleState::Completed,
            turn_id: "u1".into(),
            timestamp: Some(T0 + 9_600),
        });
        let list = items(&mut state, T0 + 9_700);
        assert_eq!(every_answered_card(&list), 1, "{agent_work}: {list:#?}");
        // The reply lands: the picture's row is work kept in view, and still the card's only owner.
        let mut rows: Vec<ChatMessage> = state.messages.list.iter().map(|row| (**row).clone()).collect();
        rows.push(words("reply", T0 + 9_500, "Red it is."));
        set_rows(&mut state, rows);
        let list = items(&mut state, T0 + 10_000);
        assert_eq!(
            every_answered_card(&list),
            1,
            "{agent_work} replied: {list:#?}"
        );
    }
}

/// An agent killed mid-call with no exit notice: gxserver's `agentGone` on a state frame shows
/// the words Quiet held back, as the notice does, while the working signal still says it works.
#[test]
fn the_words_quiet_held_back_show_once_gxserver_says_the_agent_is_gone() {
    let mut rows = turn();
    rows.push(words("w2", T0 + 5_000, "Now writing the file."));
    rows.push(call(
        "c3",
        T0 + 5_100,
        "Bash",
        json!({ "command": "sleep 100" }),
    ));
    let mut state = quiet_chat(rows);
    let held = items(&mut state, T0 + 5_200);
    assert!(
        !drawn_words(&held)
            .iter()
            .any(|text| text.contains("Now writing")),
        "the words between calls are held: {held:#?}"
    );
    let frame = |gone: bool| -> ghostex_gx_protocol::ChatStateFrame {
        let mut frame = json!({
            "type": "sessionChatState", "projectId": "P1", "sessionId": "S1", "epoch": 1,
            "seq": 2, "protocolVersion": 1, "serverId": "S", "status": "working", "working": true,
        });
        if gone {
            frame["agentGone"] = json!(true);
        }
        serde_json::from_value(frame).expect("a state frame")
    };
    let context = ChatContext {
        now_ms: (T0 + 20_000) as f64,
        ..Default::default()
    };
    ghostex_gx_chat_core::session::apply::apply_state_frame(&mut state, &frame(true), &context);
    let gone = items(&mut state, T0 + 20_000);
    assert!(
        drawn_words(&gone)
            .iter()
            .any(|text| text.contains("Now writing"))
            || kinds(&gone).contains(&"grouped-work".to_string()),
        "the work is in view once the agent is gone: {gone:#?}"
    );
    // The stale working flag no longer keeps the card on the dead call or Stop in the composer.
    assert!(!ghostex_gx_chat_core::session::working::presents_working(
        &state
    ));
    // A later frame without it (the agent was started again) holds the running words again.
    ghostex_gx_chat_core::session::apply::apply_state_frame(&mut state, &frame(false), &context);
    assert!(!state.session.agent_gone);
    assert!(ghostex_gx_chat_core::session::working::working_signal(
        &state
    ));
}
