//! A send gxserver never answered (it was restarting, or it dropped the connection): the chat says
//! it couldn't reach Ghostex instead of the socket's own words, offers Retry, and that Retry is the
//! same send to gxserver (its first `sendRequestId`), so a send that did land is answered from
//! gxserver's ledger instead of being typed twice (`session/reads.rs` `humanize_unreachable`).

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

#[test]
fn a_send_gxserver_dropped_shows_a_human_line_with_retry_and_retries_under_the_same_id() {
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

    assert!(
        core.state().composer.failed_send.is_some(),
        "failed send remembered"
    );
    let chat = &core.state().core;
    assert_eq!(chat.operation_error.as_deref(), Some(UNREACHABLE_MESSAGE));
    assert_eq!(chat.operation_error_code.as_deref(), Some("unreachable"));
    let document = projected(&core);
    assert_eq!(document.operation_error_resend.as_deref(), Some("send"));
    let shown = serde_json::to_string(&document).unwrap();
    assert!(!shown.contains("os error"), "no socket text on screen");
    assert!(!shown.contains("127.0.0.1"), "no address on screen");

    // Retry submits the same text again; the host would mint a new id, but the send keeps its own.
    let retry = send(
        &mut core,
        "hello",
        8,
        RpcOutcome::Ok {
            result: serde_json::json!({}),
        },
    );
    assert_eq!(retry["sendRequestId"], first_id.as_str());
}

#[test]
fn a_refusal_gxserver_sent_itself_keeps_its_own_words_and_offers_no_resend() {
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
    assert_eq!(
        core.state().core.operation_error.as_deref(),
        Some("The agent is not running.")
    );
    assert_ne!(
        core.state().core.operation_error_code.as_deref(),
        Some("unreachable")
    );
    let document = projected(&core);
    assert_eq!(document.operation_error_resend, None);
}

#[test]
fn the_phone_bridges_unreachable_code_gets_the_same_line() {
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
    assert_eq!(
        core.state().core.operation_error.as_deref(),
        Some(UNREACHABLE_MESSAGE)
    );
}
