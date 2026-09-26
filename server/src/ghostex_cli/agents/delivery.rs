use super::{
    arguments::{Arguments, Delivery},
    identity::{self, text},
};
use crate::ghostex_cli::{
    rpc::{call_gxserver_rpc, CliError, CliResult},
    selector, sessions,
};
use serde_json::{json, Value};

/// CDXC:Cli 2026-09-26 DECISION:
/// User: messages between agents "shouldn't be added to the queue and stuck there, they should be just sent as normal, except if the agent intentionally queues". A default or interrupt send to a sleeping recipient goes to `/api/sendSessionChatMessage` like Enter in chat, which wakes the session and types the message once its agent is ready (SessionChat 2026-09-25 decision); only `--queue` holds a message for the recipient's next stop.
/// WHY: the old refusal for a sleeping recipient told the sender to use `--queue`, and agents kept reaching for `--queue` afterwards, even for running recipients, so their messages waited behind whole turns.
pub(super) fn send(args: &Arguments) -> CliResult<Value> {
    let body = match &args.body_file {
        Some(path) => std::fs::read_to_string(path).map_err(|error| {
            CliError::Other(format!("Could not read message file {path}: {error}"))
        })?,
        None => args.positional[1].clone(),
    };
    if body.trim().is_empty() {
        return Err(CliError::Other("Message body must not be empty.".into()));
    }
    let sender = identity::caller()?;
    let message = identity::message(&sender, &body);
    if message.len() > crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES {
        return Err(CliError::Other(format!(
            "Message including sender header exceeds the {}-byte send limit.",
            crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES
        )));
    }
    let reference = &args.positional[0];
    let flags = identity::inventory_flags(&args.flags, reference)?;
    let rows = sessions::fetch_session_list(&flags, false)?;
    let recipient = selector::resolve_one_listed_session(reference, &rows, &flags)?;
    if !identity::is_agent(&recipient) {
        return Err(CliError::Other(
            "The recipient is not an agent session. Run ghostex agents list --all.".into(),
        ));
    }
    // A sleeping recipient has nothing to interrupt; the send itself wakes it.
    let waking =
        args.delivery != Delivery::Queue && text(&recipient, "lifecycleState") != "running";
    let mut payload = json!({"globalRef": recipient["globalRef"], "projectId": recipient["projectId"], "sessionId": recipient["sessionId"]});
    let interrupted = args.delivery == Delivery::Interrupt && !waking;
    if interrupted {
        call_gxserver_rpc("/api/interruptSessionChat", &payload, &flags)?;
    }
    payload["text"] = json!(message);
    let endpoint = if args.delivery == Delivery::Queue {
        "/api/queueSessionChatPrompt"
    } else {
        "/api/sendSessionChatMessage"
    };
    let result = call_gxserver_rpc(endpoint, &payload, &flags).map_err(|error| {
        let next_step = if waking {
            format!("The recipient was asleep: run ghostex wake {} and send again once it is running, after inspecting its chat.", text(&recipient, "globalRef"))
        } else {
            "Inspect chat and queue before retrying.".to_string()
        };
        CliError::Other(format!(
            "{}{} {}",
            if interrupted {
                "Interruption was requested, but message delivery failed or is uncertain: "
            } else {
                "Message delivery failed or is uncertain: "
            },
            error,
            next_step
        ))
    })?;
    Ok(json!({
        "ok": true,
        "status": if args.delivery == Delivery::Queue { "queued" } else { "accepted" },
        "mode": match args.delivery { Delivery::Normal => "normal", Delivery::Interrupt => "interrupt", Delivery::Queue => "queue" },
        "interruptRequested": interrupted,
        "wakingRecipient": waking,
        "sender": identity::summary(&sender),
        "recipient": identity::summary(&recipient),
        "receipt": receipt(&result),
    }))
}

pub(super) fn receipt(result: &Value) -> Value {
    json!({
        "requestId": result["requestId"],
        "queuedPromptId": result.get("queuedPromptId").or_else(|| result.pointer("/prompt/id")),
    })
}
