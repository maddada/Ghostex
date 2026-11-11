//! "Compact context" in the context meter's panel types `/compact` into the agent, the way a typed
//! one goes, and a click it cannot run says why on the chat box's error line (GitHub #572: the
//! Rust port of the arm returned no effects, so the click did nothing at all).

use ghostex_gx_chat_core::{
    ActionKind, ChatContext, ChatCore, ChatRpcMethod, Effect, Event, RpcOutcome, UserAction,
};

const NOW_MS: f64 = 1_793_622_600_000.0;

fn core() -> ChatCore {
    let mut core = ChatCore::new();
    core.state_mut().core.controller_started = true;
    core.state_mut().session.agent = Some("claude".into());
    core
}

fn compact(core: &mut ChatCore) -> Vec<Effect> {
    core.handle(
        Event::Action(Box::new(UserAction::new(ActionKind::ContextCompact))),
        ChatContext::at(NOW_MS),
    )
}

/// The `/compact` sends among `effects`, with their request ids.
fn compact_sends(effects: &[Effect]) -> Vec<u64> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SendRpc {
                request_id,
                method,
                params,
            } if *method == ChatRpcMethod::SendSessionChatMessage
                && params["text"] == "/compact" =>
            {
                Some(*request_id)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn compact_context_sends_compact_to_an_idle_agent() {
    let mut core = core();
    let sends = compact_sends(&compact(&mut core));
    assert_eq!(sends.len(), 1, "the click types /compact into the agent");
    assert!(
        core.state()
            .pending
            .markers
            .iter()
            .any(|marker| marker.command == "/compact"),
        "its row shows at once, as a typed /compact's does"
    );
    core.handle(
        Event::RpcSettled {
            request_id: sends[0],
            outcome: Box::new(RpcOutcome::Ok {
                result: serde_json::json!({}),
            }),
        },
        ChatContext::at(NOW_MS),
    );
    assert_eq!(core.state().core.operation_error, None);
    // The run is over, so the next click compacts again.
    assert_eq!(compact_sends(&compact(&mut core)).len(), 1);
}

#[test]
fn a_refused_compact_takes_its_row_back_and_says_why() {
    let mut core = core();
    let sends = compact_sends(&compact(&mut core));
    core.handle(
        Event::RpcSettled {
            request_id: sends[0],
            outcome: Box::new(RpcOutcome::Err {
                code: Some("agentNotRunning".into()),
                message: "The agent is not running.".into(),
                endpoint: Some("/api/sendSessionChatMessage".into()),
            }),
        },
        ChatContext::at(NOW_MS),
    );
    assert_eq!(
        core.state().core.operation_error.as_deref(),
        Some("The agent is not running.")
    );
    assert!(core.state().pending.markers.is_empty());
}

#[test]
fn compact_context_while_the_agent_works_says_so_instead_of_doing_nothing() {
    let mut core = core();
    core.state_mut().session.server_working = true;
    assert!(compact_sends(&compact(&mut core)).is_empty());
    assert_eq!(
        core.state().core.operation_error.as_deref(),
        Some("Compact is available once the agent is idle.")
    );
}
