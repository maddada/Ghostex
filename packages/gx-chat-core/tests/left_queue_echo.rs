//! A card never claims to wait once its queue row is gone, and can always be removed
//! (CDXC:SessionChat 2026-10-31 on `settle_rows_gone_from_queue`). The fixture is the card that sat
//! over a working agent on a remote Windows machine: answers delivered hours before the reader
//! opened the chat, restored from the reader's saved snapshot, and missing from a transcript that
//! lost them to a power cut.

use ghostex_gx_chat_core::composer::actions::handle as handle_action;
use ghostex_gx_chat_core::composer::send::settle_queue_mutation;
use ghostex_gx_chat_core::session::before_compose;
use ghostex_gx_chat_core::session::constants::{LEFT_QUEUE_RETENTION_MS, TIMER_LEFT_QUEUE};
use ghostex_gx_chat_core::session::startup_sends::settle_rows_gone_from_queue;
use ghostex_gx_chat_core::state::PendingSend;
use ghostex_gx_chat_core::{ChatContext, ChatState, Effect, RpcOutcome, UserAction};
use ghostex_gx_protocol::{ChatMessage, StartupDelivery};
use serde_json::{json, Value};

const OPENED: f64 = 1_793_461_000_000.0;
const ANSWERS: &str = "<ghostex-page-answers page=\"8a3b144d8039afda078e02b869d222cf\" title=\"Restore Claude Code settings.json\">\n{\n  \"Restore settings.json from which copy?\": \"5 Oct\"\n}\n</ghostex-page-answers>";

fn queued_row(id: &str, text: &str) -> Value {
    json!({
        "id": id, "text": text, "startupSend": false, "state": "queued",
        "createdAt": "2026-10-31T10:12:32.019Z", "updatedAt": "2026-10-31T10:12:32.019Z",
    })
}

fn at(now_ms: f64) -> ChatContext {
    ChatContext {
        now_ms,
        ..Default::default()
    }
}

fn claude_with_queue(queue: Vec<Value>) -> ChatState {
    let mut state = ChatState::default();
    state.session.agent = Some("claude".into());
    state.session.queue_prompts = Some(queue);
    state
}

fn delivery_state(state: &ChatState) -> Vec<String> {
    state
        .pending
        .sends
        .iter()
        .map(|entry| {
            entry
                .startup_delivery
                .as_ref()
                .map(|delivery| delivery.state.clone())
                .unwrap_or_default()
        })
        .collect()
}

const PAGE: &str = "8a3b144d8039afda078e02b869d222cf";

/// Opens the chat on the saved snapshot's row, then takes gxserver's first read: the row is gone.
fn opened_on_a_stale_row() -> ChatState {
    let mut state = claude_with_queue(vec![queued_row("Q8f07", ANSWERS)]);
    before_compose(&mut state, &at(OPENED));
    assert_eq!(delivery_state(&state), ["queued"]);
    assert!(!state.core.timers.is_armed(TIMER_LEFT_QUEUE));
    state.session.queue_prompts = Some(Vec::new());
    before_compose(&mut state, &at(OPENED + 500.0));
    state
}

fn removal(prompt_id: &str) -> UserAction {
    serde_json::from_value(json!({"type": "removeQueue", "promptId": prompt_id}))
        .expect("an action")
}

#[test]
fn a_row_that_left_stops_waiting_then_says_it_is_unconfirmed_with_no_more_timers() {
    let mut state = opened_on_a_stale_row();
    assert_eq!(
        delivery_state(&state),
        ["sending"],
        "the card no longer says it waits for the agent"
    );
    assert!(
        state.core.timers.is_armed(TIMER_LEFT_QUEUE),
        "an idle chat has a wake to move the card on"
    );

    let retention = LEFT_QUEUE_RETENTION_MS as f64;
    before_compose(&mut state, &at(OPENED + 500.0 + retention - 1.0));
    assert_eq!(delivery_state(&state), ["sending"]);
    state.core.timers.cancel(TIMER_LEFT_QUEUE);
    before_compose(&mut state, &at(OPENED + 500.0 + retention));
    assert_eq!(delivery_state(&state), ["unconfirmed"]);
    let delivery = state.pending.sends[0].startup_delivery.clone().unwrap();
    assert_eq!(
        delivery.wait_reason.as_deref(),
        Some("This left the queue, but the conversation doesn't show it.")
    );
    assert!(
        !state.core.timers.is_armed(TIMER_LEFT_QUEUE),
        "nothing more is coming, so nothing stays armed"
    );
}

#[test]
fn answers_gxserver_indexed_as_answered_go_at_once() {
    // The user's own case: the file lost the answers, but the index saw them before that.
    let mut state = claude_with_queue(vec![queued_row("Q8f07", ANSWERS)]);
    before_compose(&mut state, &at(OPENED));
    state.session.resource_index = vec![json!({"id": PAGE, "kind": "form", "status": "answered"})];
    state.session.queue_prompts = Some(Vec::new());
    before_compose(&mut state, &at(OPENED + 500.0));
    assert!(state.pending.sends.is_empty());
}

#[test]
fn a_held_row_is_not_confirmed_by_the_index() {
    let mut row = queued_row("Q2", ANSWERS);
    row["state"] = json!("failed");
    let mut state = claude_with_queue(vec![row]);
    state.session.resource_index = vec![json!({"id": PAGE, "kind": "form", "status": "answered"})];
    before_compose(&mut state, &at(OPENED));
    assert_eq!(delivery_state(&state), ["failed"], "Retry stays on offer");
}

#[test]
fn a_send_older_than_the_loaded_rows_goes_quietly() {
    // A busy session: the transcript above the loaded rows can hold it, so nothing is claimed.
    let mut state = claude_with_queue(vec![queued_row("Q8f07", "an ordinary prompt")]);
    state.pending.sends = vec![PendingSend {
        id: "pending:1".into(),
        queued_prompt_id: Some("Q8f07".into()),
        text: "an ordinary prompt".into(),
        sent_at_ms: OPENED as i64 - 3_600_000,
        ..PendingSend::default()
    }];
    let newer: ChatMessage = serde_json::from_value(json!({
        "id": "a1", "role": "assistant", "timestamp": OPENED as i64 - 60_000, "source": "transcript",
        "blocks": [{ "type": "text", "text": "later work" }],
    }))
    .expect("a chat message");
    state.messages.list = vec![newer];
    state.messages.list_revision += 1;
    state.messages.has_more = true;
    state.session.queue_prompts = Some(Vec::new());
    before_compose(&mut state, &at(OPENED));
    assert_eq!(delivery_state(&state), ["sending"]);
    before_compose(&mut state, &at(OPENED + LEFT_QUEUE_RETENTION_MS as f64));
    assert!(state.pending.sends.is_empty());
}

#[test]
fn removing_a_card_whose_row_left_is_local_and_final() {
    let mut state = opened_on_a_stale_row();
    let effects = handle_action(&mut state, &removal("Q8f07"), &at(OPENED + 40_000.0));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::SendRpc { .. })),
        "there is no row left for gxserver to remove"
    );
    assert!(state.pending.sends.is_empty());
    before_compose(&mut state, &at(OPENED + 41_000.0));
    assert!(state.pending.sends.is_empty(), "it does not come back");
}

/// A held answers card with Remove pressed: answers the call gxserver would get.
fn removing_a_held_row(state: &mut ChatState) -> u64 {
    state.composer.transport.remove_queued_prompt = true;
    let effects = handle_action(state, &removal("Q8f07"), &at(OPENED + 1_000.0));
    effects
        .iter()
        .find_map(|effect| match effect {
            Effect::SendRpc { request_id, .. } => Some(*request_id),
            _ => None,
        })
        .expect("a row still in the queue goes through gxserver")
}

#[test]
fn removing_a_held_row_takes_it_out_of_the_queue_and_reopens_the_form() {
    let mut state = claude_with_queue(vec![queued_row("Q8f07", ANSWERS)]);
    before_compose(&mut state, &at(OPENED));
    state
        .questions
        .forms
        .entry(PAGE.to_string())
        .or_default()
        .sent = Some(json!({"q": "5 Oct"}));
    let request = removing_a_held_row(&mut state);
    assert_eq!(delivery_state(&state), ["queued"], "until gxserver has it");
    let answered = settle_queue_mutation(
        &mut state,
        request,
        &RpcOutcome::Ok {
            result: json!({"queue": []}),
        },
    );
    assert!(answered.is_some());
    assert!(state.pending.sends.is_empty());
    assert_eq!(state.session.queue_prompts, Some(Vec::new()));
    assert!(
        state.questions.forms[PAGE].sent.is_none(),
        "the answers never went, so the question can be answered again"
    );
}

#[test]
fn removing_a_row_gxserver_is_typing_right_now_keeps_the_card() {
    // Remove lost the race: the answers are going in, so the card stays until the transcript shows
    // them, and the form stays answered.
    let mut state = claude_with_queue(vec![queued_row("Q8f07", ANSWERS)]);
    before_compose(&mut state, &at(OPENED));
    state
        .questions
        .forms
        .entry(PAGE.to_string())
        .or_default()
        .sent = Some(json!({"q": "5 Oct"}));
    let request = removing_a_held_row(&mut state);
    settle_queue_mutation(
        &mut state,
        request,
        &RpcOutcome::Err {
            message: "This prompt is being delivered right now.".into(),
            code: Some("badRequest".into()),
            endpoint: None,
        },
    );
    assert_eq!(state.pending.sends.len(), 1);
    assert!(state.questions.forms[PAGE].sent.is_some());
}

#[test]
fn removing_a_row_that_just_went_ends_the_card_without_an_error() {
    // Remove arrived a moment after the row was typed: gxserver no longer knows it.
    let mut state = claude_with_queue(vec![queued_row("Q8f07", ANSWERS)]);
    before_compose(&mut state, &at(OPENED));
    state
        .questions
        .forms
        .entry(PAGE.to_string())
        .or_default()
        .sent = Some(json!({"q": "5 Oct"}));
    let request = removing_a_held_row(&mut state);
    settle_queue_mutation(
        &mut state,
        request,
        &RpcOutcome::Err {
            message: "That prompt no longer exists.".into(),
            code: Some("notFound".into()),
            endpoint: None,
        },
    );
    assert!(state.pending.sends.is_empty());
    assert!(
        state.questions.forms[PAGE].sent.is_some(),
        "it was typed, so the form does not ask again"
    );
}

#[test]
fn a_transcript_row_replaces_the_echo_inside_the_gap() {
    let mut state = claude_with_queue(vec![queued_row("Q8f07", ANSWERS)]);
    before_compose(&mut state, &at(OPENED));
    state.session.queue_prompts = Some(Vec::new());
    before_compose(&mut state, &at(OPENED + 1_000.0));
    assert_eq!(delivery_state(&state), ["sending"]);

    let recorded: ChatMessage = serde_json::from_value(json!({
        "id": "u1", "role": "user", "timestamp": OPENED as i64 + 2_000, "source": "transcript",
        "blocks": [{ "type": "text", "text": ANSWERS }],
    }))
    .expect("a chat message");
    state.messages.list = vec![recorded];
    state.messages.list_revision += 1;
    before_compose(&mut state, &at(OPENED + 2_500.0));
    assert!(state.pending.sends.is_empty());
}

#[test]
fn a_row_still_held_keeps_waiting_however_long_it_takes() {
    // Hours behind a hung tool: the wait is real, so nothing about the row or its reason changes.
    let mut row = queued_row("Q1", ANSWERS);
    row["waitReason"] = json!("Waiting for the agent to finish its turn");
    let mut state = claude_with_queue(vec![row]);
    for hours in 0..12 {
        before_compose(&mut state, &at(OPENED + f64::from(hours) * 3_600_000.0));
    }
    assert_eq!(delivery_state(&state), ["queued"]);
    assert_eq!(
        state.pending.sends[0]
            .startup_delivery
            .as_ref()
            .and_then(|delivery| delivery.wait_reason.as_deref()),
        Some("Waiting for the agent to finish its turn")
    );
    assert!(!state.core.timers.is_armed(TIMER_LEFT_QUEUE));
}

#[test]
fn a_row_that_comes_back_restores_what_it_says() {
    // A stale read can miss a row that is still there.
    let mut state = claude_with_queue(vec![queued_row("Q1", ANSWERS)]);
    before_compose(&mut state, &at(OPENED));
    state.session.queue_prompts = Some(Vec::new());
    before_compose(&mut state, &at(OPENED + 1_000.0));
    assert_eq!(delivery_state(&state), ["sending"]);

    state.session.queue_prompts = Some(vec![queued_row("Q1", ANSWERS)]);
    let later = OPENED + 1_000.0 + LEFT_QUEUE_RETENTION_MS as f64 * 3.0;
    before_compose(&mut state, &at(later));
    assert_eq!(delivery_state(&state), ["queued"]);
    assert_eq!(state.pending.sends[0].left_queue_at_ms, None);
}

#[test]
fn without_a_queue_read_nothing_is_assumed_to_have_left() {
    // gxserver away (`queue_prompts` keeps the last rows it sent; `None` is not loaded yet).
    let mut state = ChatState::default();
    state.pending.sends = vec![PendingSend {
        id: "pending:1".into(),
        queued_prompt_id: Some("Q1".into()),
        text: "a queued prompt".into(),
        ..PendingSend::default()
    }];
    before_compose(&mut state, &at(OPENED));
    assert_eq!(state.pending.sends[0].left_queue_at_ms, None);
    assert_eq!(state.pending.sends[0].startup_delivery, None);
}

#[test]
fn a_form_echo_that_names_no_row_is_left_alone() {
    let echo = PendingSend {
        id: "answers:8a3b".into(),
        startup_delivery: Some(StartupDelivery {
            prompt_id: String::new(),
            state: "reconnecting".into(),
            error_message: None,
            wait_reason: None,
        }),
        text: ANSWERS.into(),
        ..PendingSend::default()
    };
    let far_later = OPENED as i64 + LEFT_QUEUE_RETENTION_MS * 100;
    let kept = settle_rows_gone_from_queue(vec![echo.clone()], &[], far_later, None);
    assert_eq!(kept, vec![echo]);
}
