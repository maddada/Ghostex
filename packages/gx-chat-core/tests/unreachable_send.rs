//! A send gxserver never answered (it was restarting, or it dropped the connection): the message
//! says under its bubble that it couldn't reach Ghostex, in those words instead of the socket's,
//! and goes again by itself as the same send to gxserver (its first `sendRequestId`), so a send
//! that did land is answered from gxserver's ledger instead of being typed twice
//! (`session/reads.rs` `humanize_unreachable`, `composer/delivery.rs`). A refusal gxserver sent
//! itself stays on the message in gxserver's own words.
//!
//! Until 2026-11-11 the text went back to the chat box with an error line and a Retry button;
//! the CDXC:SessionChat 2026-11-11 DECISION in `composer/delivery.rs` supersedes that.

use ghostex_gx_chat_core::composer::send::begin;
use ghostex_gx_chat_core::composer::submission::SubmissionMode;
use ghostex_gx_chat_core::session::reads::UNREACHABLE_MESSAGE;
use ghostex_gx_chat_core::{
    ChatContext, ChatCore, ChatRpcMethod, Effect, Event, RpcOutcome, StorageKey,
};
use serde_json::Value;

const RESET: &str = "Could not read gxserver response: the connection to 127.0.0.1:58744 was \
closed before an answer arrived (Connection reset by peer (os error 54)). gxserver may have \
stopped, or security software may be blocking local connections.";

fn context(random_id: u128) -> ChatContext {
    ChatContext {
        now_ms: 1_793_622_600_000.0,
        ..Default::default()
    }
    .with_random_ids([random_id, 0])
}

/// Submits `text` and answers every step as a host would until the delivery call goes out; that
/// call is answered with `answer`. Returns the delivery's params.
fn send(core: &mut ChatCore, text: &str, random_id: u128, answer: RpcOutcome) -> Value {
    let mut pending = begin(
        core.state_mut(),
        &context(random_id),
        SubmissionMode::Send,
        text,
        None,
        Vec::new(),
        false,
        None,
    );
    for _ in 0..20 {
        let mut next = Vec::new();
        for effect in pending {
            let event = match effect {
                Effect::WriteStorage { key, .. } => Event::StorageWritten { key, error: None },
                Effect::FlushStorage { store } => Event::StorageWritten {
                    key: StorageKey {
                        store,
                        suffix: String::new(),
                    },
                    error: None,
                },
                Effect::SendRpc {
                    request_id,
                    method,
                    params,
                } if method == ChatRpcMethod::SendSessionChatMessage => {
                    core.handle(
                        Event::RpcSettled {
                            request_id,
                            outcome: Box::new(answer),
                        },
                        context(random_id),
                    );
                    return *params;
                }
                Effect::SendRpc { request_id, .. } => Event::RpcSettled {
                    request_id,
                    outcome: Box::new(RpcOutcome::Ok {
                        result: serde_json::json!({}),
                    }),
                },
                _ => continue,
            };
            next.extend(core.handle(event, context(random_id)));
        }
        pending = next;
    }
    panic!("the submission never reached its send");
}

/// Family a's part of the document, which carries the error line.
fn projected(core: &ChatCore) -> ghostex_gx_chat_core::Document {
    let mut document = ghostex_gx_chat_core::Document::default();
    ghostex_gx_chat_core::session::document::document(core.state(), &context(7), &mut document);
    document
}

fn core() -> ChatCore {
    let mut core = ChatCore::new();
    core.state_mut().session.agent = Some("claude".into());
    core
}

/// What the message's row says under its bubble: its `startupDelivery`.
fn note(core: &ChatCore) -> Value {
    let rows = ghostex_gx_chat_core::transcript::rows::rows(core.state(), &context(7));
    let row = rows
        .into_iter()
        .find_map(|item| match item {
            ghostex_gx_chat_core::TranscriptItem::Message { message } => Some(message),
            _ => None,
        })
        .expect("the message keeps its row");
    serde_json::to_value(&row).unwrap()["startupDelivery"].clone()
}

#[test]
fn a_send_gxserver_dropped_says_so_in_human_words_and_goes_again_under_the_same_id() {
    let mut core = core();
    let first = send(
        &mut core,
        "hello",
        7,
        RpcOutcome::Err {
            code: None,
            message: RESET.into(),
            endpoint: Some("/api/sendSessionChatMessage".into()),
        },
    );
    let first_id = first["sendRequestId"]
        .as_str()
        .expect("a named send")
        .to_string();

    // Nothing is handed back: no error line, no Retry to press, the message waits on its row.
    let chat = &core.state().core;
    assert_eq!(chat.operation_error, None);
    let document = projected(&core);
    assert_eq!(document.operation_error_resend, None);
    let note = note(&core);
    assert_eq!(note["state"], "waiting");
    let shown = note["waitReason"].as_str().expect("why it waits");
    assert!(shown.starts_with(UNREACHABLE_MESSAGE), "{shown}");
    assert!(!shown.contains("os error"), "no socket text on screen");
    assert!(!shown.contains("127.0.0.1"), "no address on screen");

    // Two seconds on it goes again by itself, as the same send.
    let mut later = context(8);
    later.now_ms += 2_000.0;
    let again = core.handle(Event::Tick, later);
    let retried = again
        .iter()
        .find_map(|effect| match effect {
            Effect::SendRpc { method, params, .. }
                if *method == ChatRpcMethod::SendSessionChatMessage =>
            {
                Some((**params).clone())
            }
            _ => None,
        })
        .expect("the send went again");
    assert_eq!(retried["sendRequestId"], first_id.as_str());
    assert_eq!(retried["text"], "hello");
}

#[test]
fn a_refusal_gxserver_sent_itself_keeps_its_own_words_on_the_message() {
    let mut core = core();
    send(
        &mut core,
        "hello",
        7,
        RpcOutcome::Err {
            code: Some("agentNotRunning".into()),
            message: "The agent is not running.".into(),
            endpoint: Some("/api/sendSessionChatMessage".into()),
        },
    );
    assert_eq!(core.state().core.operation_error, None);
    let document = projected(&core);
    assert_eq!(document.operation_error_resend, None);
    let note = note(&core);
    assert_eq!(note["state"], "failed");
    assert_eq!(note["errorMessage"], "Not sent. The agent is not running.");
}

#[test]
fn the_phone_bridges_unreachable_code_gets_the_same_words() {
    let mut core = core();
    send(
        &mut core,
        "hello",
        7,
        RpcOutcome::Err {
            code: Some("unreachable".into()),
            message: "SSH bridge: connection closed".into(),
            endpoint: None,
        },
    );
    assert_eq!(core.state().core.operation_error, None);
    let note = note(&core);
    assert_eq!(note["state"], "waiting");
    assert!(note["waitReason"]
        .as_str()
        .unwrap()
        .starts_with(UNREACHABLE_MESSAGE));
}
