use super::{
    arguments::{Arguments, Delivery},
    identity::{self, text},
};
use crate::ghostex_cli::{
    args::Flags,
    rpc::{call_gxserver_rpc, CliError, CliResult},
    selector,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// The rename reminder gxserver adds when `sender` is the orchestrator of the thread `recipient`
/// (server/src/coordinators/rename.rs `thread_title_reminder`); `None` for any other message,
/// and from a gxserver too old to answer.
pub(super) fn thread_title_reminder(
    sender: &Value,
    recipient: &Value,
    flags: &Flags,
) -> Option<String> {
    call_gxserver_rpc(
        "/api/coordinatorThreadTitleReminder",
        &json!({
            "senderProjectId": sender["projectId"], "senderSessionId": sender["sessionId"],
            "projectId": recipient["projectId"], "sessionId": recipient["sessionId"],
        }),
        flags,
    )
    .ok()?["reminder"]
        .as_str()
        .map(str::to_string)
}

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
    let reference = &args.positional[0];
    let flags = identity::inventory_flags(&args.flags, reference)?;
    let recipient = selector::resolve_live_or_closed_session(reference, &flags)?;
    if !identity::is_agent(&recipient) {
        return Err(CliError::Other(
            "The recipient is not an agent session. Run ghostex agents list --all.".into(),
        ));
    }
    super::outbound::check_agent_session_write(&sender, &recipient, &flags)?;
    let message = identity::message(
        &sender,
        &match thread_title_reminder(&sender, &recipient, &flags) {
            Some(reminder) => format!("{}\n\n{reminder}", body.trim_end()),
            None => body.clone(),
        },
    );
    if message.len() > crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES {
        return Err(CliError::Other(format!(
            "Message including sender header exceeds the {}-byte send limit.",
            crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES
        )));
    }
    // A sleeping or closed recipient has nothing to interrupt; the send itself wakes it and
    // resumes its conversation.
    let waking =
        args.delivery != Delivery::Queue && text(&recipient, "lifecycleState") != "running";
    let mut payload = json!({"globalRef": recipient["globalRef"], "projectId": recipient["projectId"], "sessionId": recipient["sessionId"]});
    // CDXC:Cli 2026-10-05 WHY: every send carries a `sendRequestId` (session_chat_send_requests.rs). A caller that retries a failed or uncertain send passes the id the first attempt printed back as `--request-id`, and gxserver answers from its record of that send instead of typing it a second time; without the flag each run is a new message.
    let send_request_id = args
        .request_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let interrupted = args.delivery == Delivery::Interrupt && !waking;
    if interrupted {
        // gxserver skips the Escape when this id was already sent, so a retry never interrupts
        // the turn the first attempt started.
        let mut interrupt = payload.clone();
        interrupt["sendRequestId"] = json!(send_request_id);
        call_gxserver_rpc("/api/interruptSessionChat", &interrupt, &flags)?;
    }
    let read_payload = payload.clone();
    let sent_at_ms = chrono::Utc::now().timestamp_millis();
    let rows_before = if args.delivery == Delivery::Queue {
        TranscriptRows::Unknown
    } else {
        transcript_rows_before_send(&read_payload, &flags)
    };
    payload["text"] = json!(message);
    payload["sendRequestId"] = json!(send_request_id);
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
        let next_step = format!("{next_step} To retry this same message, add --request-id {send_request_id} so it is never delivered twice.");
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
    let duplicate = result["duplicate"] == json!(true);
    // A coordinator's follow-up to a thread it already marked done reopens that thread, or its reply would go unsupervised and never be reported back; gxserver owns the rule (CDXC:Coordinators in server/src/coordinators/reopen.rs).
    // A repeated request id was linked by its first attempt; linking again would restart the delivery watch from now and miss the row the first attempt produced.
    // CDXC:Coordinators 2026-11-10 WHY: the reopen was silent, so a coordinator could not tell its message had put a done thread back under it, and a reply the thread finished while done was never reported (GitHub #541). The send says so in one line and prints that held reply.
    // gxserver's send route reopens the thread itself and says so in the send's answer (server/src/coordinators/reopen.rs); this link call still watches the message, reopens for `--queue`, and answers for an older gxserver, guarded on the server by `sentAtMs`.
    let reopen = (!duplicate)
        .then(|| call_gxserver_rpc(
            "/api/linkCoordinatorThread",
            &json!({
                "coordinatorProjectId": sender["projectId"], "coordinatorSessionId": sender["sessionId"],
                "projectId": recipient["projectId"], "sessionId": recipient["sessionId"],
                "onlyIfCoordinator": true, "reopenOnly": true,
                "pendingMessage": body, "sentAtMs": sent_at_ms,
            }),
            &flags,
        ).ok())
        .flatten();
    let reopened = result["reopenedThread"] == json!(true)
        || reopen
            .as_ref()
            .is_some_and(|reopen| reopen["reopened"] == json!(true));
    let held_report = result["heldReport"]
        .as_str()
        .or_else(|| {
            reopen
                .as_ref()
                .and_then(|reopen| reopen["heldReport"].as_str())
        })
        .map(str::to_string);
    if reopened {
        // stderr, so the JSON on stdout keeps parsing.
        eprintln!("reopened: this thread was resolved, its report will come back to you");
        if let Some(held) = &held_report {
            eprintln!("Its reply from while it was resolved, not reported before:\n{held}");
        }
    }
    let receipt = receipt(&result);
    let (status, note) = if args.delivery == Delivery::Queue {
        (
            "queued",
            if duplicate {
                "This request id was already queued; Ghostex did not queue it again."
            } else {
                "Held in the recipient's queue until its current turn finishes and its input is ready."
            },
        )
    } else if duplicate {
        if read_chat(&read_payload, DELIVERY_ROWS_BEFORE, &flags)
            .is_some_and(|chat| chat_shows(&chat, &body, &TranscriptRows::Known(HashSet::new())))
        {
            ("delivered", "This request id was already sent and the recipient's transcript shows the message; Ghostex did not type it again.")
        } else {
            ("accepted", "This request id was already sent; Ghostex did not type it again. The recipient's transcript does not show it yet. Read its chat before sending a new message.")
        }
    } else if result["alreadyHeld"] == json!(true) {
        ("pending", "The same message is already waiting for the recipient to start, so Ghostex did not queue it again; it is typed once the recipient's input box appears.")
    } else if transcript_shows(&read_payload, &body, &rows_before, &flags) {
        ("delivered", "The recipient's transcript shows the message.")
    } else if !receipt["queuedPromptId"].is_null() {
        ("pending", "The recipient is still starting; Ghostex types the message as soon as its input box appears. Read its chat before sending it again.")
    } else {
        ("accepted", "Ghostex typed and submitted the message, but the recipient's transcript does not show it yet; a busy agent takes it at its next input boundary. Read its chat before sending it again.")
    };
    if status == "delivered" {
        confirm_coordinator_delivery(&sender, &recipient, &body, &flags);
    } else {
        // The sender's chat card follows the message until it arrives (agent_message_deliveries.rs);
        // an older gxserver without the watch refuses the call and the card keeps this status.
        let _ = call_gxserver_rpc(
            "/api/watchAgentMessageDelivery",
            &json!({
                "senderProjectId": sender["projectId"], "senderSessionId": sender["sessionId"],
                "projectId": recipient["projectId"], "sessionId": recipient["sessionId"],
                "sendRequestId": send_request_id, "message": body, "sentAtMs": sent_at_ms,
            }),
            &flags,
        );
    }
    Ok(json!({
        "ok": true,
        "status": status,
        "note": note,
        "mode": match args.delivery { Delivery::Normal => "normal", Delivery::Interrupt => "interrupt", Delivery::Queue => "queue" },
        "interruptRequested": interrupted,
        "wakingRecipient": waking,
        "sendRequestId": send_request_id,
        "duplicate": duplicate,
        "reopenedThread": reopened,
        "heldReport": held_report,
        "sender": identity::summary(&sender),
        "recipient": identity::summary(&recipient),
        "receipt": receipt,
        // CDXC:SessionChat 2026-10-10 WHY: the sender's chat card shows this text; the command line often holds only `$msg` or a `--body-file` path (gx-chat-core sent_message_script.rs).
        "message": body,
    }))
}

/// Tells gxserver a coordinator's message to its thread arrived, so its delivery watch stops
/// (a no-op for any other sender or recipient, and for a gxserver without the watch). The message
/// names which watch it confirms: an older message's confirmation leaves a newer one watched.
pub(crate) fn confirm_coordinator_delivery(
    sender: &Value,
    recipient: &Value,
    message: &str,
    flags: &Flags,
) {
    let _ = call_gxserver_rpc(
        "/api/linkCoordinatorThread",
        &json!({
            "coordinatorProjectId": sender["projectId"], "coordinatorSessionId": sender["sessionId"],
            "projectId": recipient["projectId"], "sessionId": recipient["sessionId"],
            "onlyIfCoordinator": true, "reopenOnly": true, "messageDelivered": true,
            "pendingMessage": message,
        }),
        flags,
    );
}

/// How long a default send waits for the recipient's transcript to record the message.
const DELIVERY_CONFIRM_WINDOW: Duration = Duration::from_secs(10);
const DELIVERY_CONFIRM_POLL: Duration = Duration::from_millis(500);
/// Newest transcript rows each read covers; the read before the send covers more, so every row
/// an after-send read can return that is not new was seen before the send.
const DELIVERY_ROWS_AFTER: usize = 30;
const DELIVERY_ROWS_BEFORE: usize = 60;

/// CDXC:Cli 2026-10-04 WHY:
/// "accepted" only ever meant that gxserver took the request: a message Claude Code left in its input box (a form feed in the text) came back "accepted", and the coordinator moved on while the recipient never saw it. A default send now reads the recipient's transcript for up to ten seconds and answers "delivered" only when the message is there; gxserver refuses a send whose Return the agent did not take (session_chat_send_submit.rs), so "accepted" is left for an agent that has not recorded it yet, typically a busy one. Only a row that was not in the transcript before the send counts, and it must hold both the start and the end of the body: the first version accepted any matching row up to a minute old, so a message resent to an agent stopped at its usage limit answered "delivered" from the copy it had taken earlier.
fn transcript_shows(session: &Value, body: &str, before: &TranscriptRows, flags: &Flags) -> bool {
    let started = Instant::now();
    loop {
        std::thread::sleep(DELIVERY_CONFIRM_POLL);
        let shown = read_chat(session, DELIVERY_ROWS_AFTER, flags)
            .is_some_and(|chat| chat_shows(&chat, body, before));
        if shown || started.elapsed() >= DELIVERY_CONFIRM_WINDOW {
            return shown;
        }
    }
}

/// The ids of the recipient's newest transcript rows before a send. `Known(empty)` for a session
/// whose transcript is empty; `Unknown` when it could not be read, which never counts as proof.
pub(crate) enum TranscriptRows {
    Known(HashSet<String>),
    Unknown,
}

pub(crate) fn transcript_rows_before_send(session: &Value, flags: &Flags) -> TranscriptRows {
    match read_chat(session, DELIVERY_ROWS_BEFORE, flags) {
        Some(chat) => TranscriptRows::Known(
            chat["messages"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|row| text(row, "id").to_string())
                .filter(|id| !id.is_empty())
                .collect(),
        ),
        None => TranscriptRows::Unknown,
    }
}

fn read_chat(session: &Value, limit: usize, flags: &Flags) -> Option<Value> {
    let params = json!({
        "globalRef": session["globalRef"], "projectId": session["projectId"], "sessionId": session["sessionId"],
        "limit": limit,
    });
    call_gxserver_rpc("/api/readSessionChat", &params, flags).ok()
}

/// Whether a `/api/readSessionChat` result holds a user turn, new since `before`, that carries
/// `body`. gxserver's coordinator supervisor applies the same text match.
pub(crate) fn chat_shows(chat: &Value, body: &str, before: &TranscriptRows) -> bool {
    let TranscriptRows::Known(before) = before else {
        return false;
    };
    let needles = crate::coordinators::delivery_needles(body);
    chat["messages"].as_array().is_some_and(|messages| {
        messages.iter().any(|row| {
            text(row, "role") == "user"
                && text(row, "source") == "transcript"
                && !text(row, "id").is_empty()
                && !before.contains(text(row, "id"))
                && row["blocks"].as_array().is_some_and(|blocks| {
                    crate::coordinators::holds_message(
                        &blocks
                            .iter()
                            .filter_map(|block| block["text"].as_str())
                            .map(crate::coordinators::normalize_delivery_text)
                            .collect::<String>(),
                        &needles,
                    )
                })
        })
    })
}

pub(super) fn receipt(result: &Value) -> Value {
    json!({
        "requestId": result["requestId"],
        "sendRequestId": result["sendRequestId"],
        "queuedPromptId": result.get("queuedPromptId").or_else(|| result.pointer("/prompt/id")),
    })
}
