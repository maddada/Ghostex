//! The send that wakes a stopped Cloud Box (`session/cloud_box.rs`): the host refuses the draft
//! push ahead of it with `cloudBoxAsleep`, so the box never saves that draft revision, and the
//! send goes without it. With the revision, the woken box refused the message ("The submitted
//! draft revision is no longer available. Your text has not been sent.").

use ghostex_gx_chat_core::composer::queue::DraftVersion;
use ghostex_gx_chat_core::composer::send::begin;
use ghostex_gx_chat_core::composer::submission::SubmissionMode;
use ghostex_gx_chat_core::session::cloud_box::CLOUD_BOX_ASLEEP_CODE;
use ghostex_gx_chat_core::{
    ChatContext, ChatCore, ChatRpcMethod, Effect, Event, RpcOutcome, StorageKey,
};
use serde_json::{json, Value};

fn context() -> ChatContext {
    ChatContext {
        now_ms: 1_793_622_600_000.0,
        ..Default::default()
    }
}

/// Sends "hello box" under revision 10 of a draft, answering each step as a host would: storage
/// settles, the draft push gets `draft_push`, and the delivery's params are returned.
fn delivered_params(draft_push: RpcOutcome) -> Value {
    let mut core = ChatCore::new();
    core.state_mut().session.agent = Some("claude".into());
    let version = DraftVersion {
        draft_id: "d1".into(),
        revision: 10,
    };
    let mut pending = begin(
        core.state_mut(),
        &context(),
        SubmissionMode::Send,
        "hello box",
        Some(version),
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
                    request_id, method, ..
                } if method == ChatRpcMethod::SetSessionChatDraft => Event::RpcSettled {
                    request_id,
                    outcome: Box::new(draft_push.clone()),
                },
                Effect::SendRpc { method, params, .. }
                    if method == ChatRpcMethod::SendSessionChatMessage =>
                {
                    return *params;
                }
                _ => continue,
            };
            next.extend(core.handle(event, context()));
        }
        pending = next;
    }
    panic!("the submission never reached its send");
}

#[test]
fn the_send_that_wakes_a_stopped_box_goes_without_the_refused_draft_revision() {
    let params = delivered_params(RpcOutcome::Err {
        code: Some(CLOUD_BOX_ASLEEP_CODE.into()),
        message: "This cloud box is stopped. Sending a message wakes it.".into(),
        endpoint: Some("/api/setSessionChatDraft".into()),
    });
    assert_eq!(params["text"], "hello box");
    assert!(
        params.get("draftVersion").is_none_or(Value::is_null),
        "the box never saved this revision: {params}"
    );
}

#[test]
fn a_saved_draft_still_sends_under_its_revision() {
    let params = delivered_params(RpcOutcome::Ok { result: json!({}) });
    assert_eq!(
        params["draftVersion"],
        json!({ "draftId": "d1", "revision": 10 })
    );
}
