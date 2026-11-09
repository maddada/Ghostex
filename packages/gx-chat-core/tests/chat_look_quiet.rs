//! Agent work Quiet and Reduced (`transcript/chat_look.rs`): the edges where work escaped them. No
//! tool-call row draws in the moment between an agent asking a question and its card arriving, a
//! question's sentence never shows as a reply once the answer starts a new run, an agent that dies
//! on a call shows the words Quiet held back, and the subagent viewer reads in the main chat's look.

use ghostex_gx_chat_core::transcript::chat_look::ChatLookSettings;
use ghostex_gx_chat_core::{ChatContext, ChatState, TranscriptItem};
use ghostex_gx_protocol::{ChatMessage, TurnLifecycle, TurnLifecycleState};
use serde_json::{json, Value};

const T0: i64 = 1_793_818_000_000;

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
    state.messages.composed = rows.clone();
    state.messages.list = rows;
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
    let items = ghostex_gx_chat_core::extras::subagent_rows::rows(&state, &context);
    kinds(
        &items
            .iter()
            .map(|item: &TranscriptItem| serde_json::to_value(item).unwrap())
            .collect::<Vec<_>>(),
    )
}

#[test]
fn the_subagent_viewer_reads_in_the_main_chat_s_look() {
    assert_eq!(subagent_chat(None), ["message", "completed-work"]);
    assert_eq!(subagent_chat(Some("reduced")), ["message", "grouped-work"]);
    assert_eq!(subagent_chat(Some("quiet")), ["message", "grouped-work"]);
}
