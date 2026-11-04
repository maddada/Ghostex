//! A sent message's card follows gxserver's delivery watch even when the agent filtered the send's
//! output down to its status line, so the `sendRequestId` never reached the transcript
//! (CDXC:SessionChat 2026-10-31 on `apply_live_delivery`), and a message that reached the agent
//! mid-turn does not draw the card of a send before it twice (CDXC:SessionChat 2026-10-31 on
//! gxserver's `stamp_user_row_after_predecessor`).

use ghostex_gx_chat_core::{ChatContext, ChatState, TranscriptItem};
use ghostex_gx_protocol::agent_message_delivery::message_fingerprint;
use ghostex_gx_protocol::ChatMessage;
use serde_json::{json, Value};

const THREAD: &str = "S90:P3lv0:G1co8";
const T0: i64 = 1_793_420_000_000;

fn message(value: Value) -> ChatMessage {
    serde_json::from_value(value).expect("a chat message")
}

fn user(id: &str, at: i64, text: &str) -> ChatMessage {
    message(json!({
        "id": id, "role": "user", "timestamp": at, "source": "transcript",
        "blocks": [{ "type": "text", "text": text }],
    }))
}

fn assistant(id: &str, at: i64, text: &str) -> ChatMessage {
    message(json!({
        "id": id, "role": "assistant", "timestamp": at, "source": "transcript",
        "blocks": [{ "type": "text", "text": text }],
    }))
}

/// `ghostex agents send <thread> --queue "<body>"` with its output filtered to the status line, as
/// the coordinator ran it: the call at `at`, its result two seconds later.
fn filtered_send(call: &str, at: i64, body: &str, output: &str) -> [ChatMessage; 2] {
    let command =
        format!("ghostex agents send {THREAD} --queue \"{body}\" 2>&1 | grep -m1 '\"status\"'");
    [
        message(json!({
            "id": call, "role": "assistant", "timestamp": at, "source": "transcript",
            "blocks": [{ "type": "tool-call", "name": "Bash", "callId": call,
                         "input": { "command": command } }],
        })),
        message(json!({
            "id": format!("{call}-result"), "role": "tool", "timestamp": at + 2_000,
            "source": "transcript",
            "blocks": [{ "type": "tool-result", "callId": call, "output": output }],
        })),
    ]
}

/// gxserver's list entry for a send of `body` to the thread at `sent_at`.
fn watched(id: &str, body: &str, sent_at: i64, state: &str) -> Value {
    json!({
        "sendRequestId": id, "projectId": "P3lv0", "sessionId": "G1co8",
        "fingerprint": message_fingerprint(body), "sentAtMs": sent_at, "state": state,
    })
}

fn state_with(mut rows: Vec<ChatMessage>, deliveries: Option<Value>) -> ChatState {
    // Every transcript row carries its place in the file, as gxserver's readers stamp it.
    for (at, row) in rows.iter_mut().enumerate() {
        row.byte_offset.get_or_insert(at as u64 * 100);
    }
    let mut state = ChatState::default();
    state.session.agent = Some("claude".into());
    state.session.agent_message_deliveries = deliveries;
    state.messages.composed = rows.clone();
    state.messages.list = rows;
    state
}

/// Every sent card the transcript draws, as (status label, body), in order: a finished turn draws
/// its work rows' cards lifted out of the fold, so those are the ones counted there.
fn sent_cards(state: &ChatState) -> Vec<(String, String)> {
    let context = ChatContext {
        now_ms: (T0 + 600_000) as f64,
        ..Default::default()
    };
    let read = |cards: &[Value]| -> Vec<(String, String)> {
        cards
            .iter()
            .map(|card| {
                (
                    card["card"]["status"]["label"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    card["body"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect()
    };
    let mut cards = Vec::new();
    for item in ghostex_gx_chat_core::transcript::rows::rows(state, &context) {
        match item {
            TranscriptItem::Message { message } => {
                if let Some(Value::Array(sent)) = message.get("sentMessages") {
                    cards.extend(read(sent));
                }
            }
            TranscriptItem::CompletedWork { sent_messages, .. } => {
                cards.extend(read(&sent_messages));
            }
            _ => {}
        }
    }
    cards
}

/// A finished turn with one filtered send in it.
fn one_send_turn() -> Vec<ChatMessage> {
    let mut rows = vec![user("u1", T0, "Tell the thread to rebuild.")];
    rows.extend(filtered_send(
        "c1",
        T0 + 10_000,
        "Go ahead with the rebuild.",
        "  \"status\": \"queued\",\n",
    ));
    rows.push(assistant("a1", T0 + 20_000, "Told it."));
    rows
}

#[test]
fn a_filtered_queued_send_reaches_delivered() {
    let body = "Go ahead with the rebuild.";
    let deliveries = json!({
        "@sends": [watched("id-1", body, T0 + 11_000, "delivered")],
    });
    let state = state_with(one_send_turn(), Some(deliveries));
    assert_eq!(
        sent_cards(&state),
        [("Delivered".to_string(), body.to_string())]
    );
}

#[test]
fn a_filtered_queued_send_gxserver_gave_up_on_is_not_sent() {
    let mut failed = watched("id-1", "Go ahead with the rebuild.", T0 + 11_000, "failed");
    failed["failure"] =
        json!("Its transcript does not show the message, and it went idle without taking it.");
    let state = state_with(one_send_turn(), Some(json!({ "@sends": [failed] })));
    assert_eq!(sent_cards(&state)[0].0, "Not sent");
}

#[test]
fn a_filtered_send_still_waiting_stays_queued() {
    let deliveries = json!({
        "@sends": [watched("id-1", "Go ahead with the rebuild.", T0 + 11_000, "waiting")],
    });
    let state = state_with(one_send_turn(), Some(deliveries));
    assert_eq!(sent_cards(&state)[0].0, "Queued");
}

/// A send gxserver does not follow (it stopped watching, it is older than the list, or another
/// message with the same words went elsewhere) no longer claims to be on its way.
#[test]
fn an_untracked_send_stops_claiming_queued() {
    for sends in [
        json!([]),
        json!([watched(
            "id-1",
            "Go ahead with the rebuild.",
            T0 + 11_000,
            "untracked"
        )]),
        json!([watched(
            "id-1",
            "A different message.",
            T0 + 11_000,
            "delivered"
        )]),
        // Sent long before this call ran: a send of its own.
        json!([watched(
            "id-1",
            "Go ahead with the rebuild.",
            T0 - 60_000,
            "delivered"
        )]),
    ] {
        let state = state_with(one_send_turn(), Some(json!({ "@sends": sends })));
        assert_eq!(sent_cards(&state)[0].0, "Sent", "sends: {sends}");
    }
}

/// A gxserver from before the list gives no word on a send without an id: the card keeps what the
/// send said, as it did.
#[test]
fn an_older_gxserver_leaves_the_card_as_the_send_said() {
    for deliveries in [
        None,
        Some(json!({ "some-other-id": { "state": "delivered" } })),
    ] {
        let state = state_with(one_send_turn(), deliveries);
        assert_eq!(sent_cards(&state)[0].0, "Queued");
    }
}

/// The same message sent twice, in two calls, is two cards, each following its own send.
#[test]
fn a_repeated_send_shows_twice_and_each_follows_its_own_send() {
    let body = "Go ahead with the rebuild.";
    let mut rows = vec![user("u1", T0, "Tell the thread twice.")];
    rows.extend(filtered_send(
        "c1",
        T0 + 10_000,
        body,
        "  \"status\": \"queued\",\n",
    ));
    rows.extend(filtered_send(
        "c2",
        T0 + 20_000,
        body,
        "  \"status\": \"queued\",\n",
    ));
    rows.push(assistant("a1", T0 + 30_000, "Told it twice."));
    let deliveries = json!({
        "@sends": [
            watched("id-2", body, T0 + 21_000, "waiting"),
            watched("id-1", body, T0 + 11_000, "delivered"),
        ],
    });
    let state = state_with(rows, Some(deliveries));
    assert_eq!(
        sent_cards(&state),
        [
            ("Delivered".to_string(), body.to_string()),
            ("Queued".to_string(), body.to_string()),
        ]
    );
}

/// A thread's report that reached the agent while it worked, after a send: gxserver folds the turn
/// before it by file order (the send in its deferred work) and stamps the report no earlier than
/// the row before it, so the send's card is drawn once. Stamped with the time it was queued
/// (`T0 + 5_000`, before the send), the report opened its turn before the send and both turns drew
/// the card.
#[test]
fn a_report_that_arrives_mid_turn_does_not_draw_the_send_twice() {
    let body = "Go ahead with the rebuild.";
    let [call, result] = filtered_send("c1", T0 + 10_000, body, "  \"status\": \"queued\",\n");
    let report_at = T0 + 12_000;
    let mut opener = user("u1", T0, "Tell the thread to rebuild.");
    opener.deferred_work = Some(
        serde_json::from_value(json!({
            "beforeOffset": 400, "startId": "u1", "endId": "c1-result",
            "completedAt": T0 + 12_000, "messageCount": 2, "filePaths": [],
        }))
        .unwrap(),
    );
    let mut report = user(
        "u2",
        report_at,
        "Ghostex thread report: finished its turn. Its final message:\n\nRebuild started.",
    );
    report.deferred_work = Some(
        serde_json::from_value(json!({
            "beforeOffset": 600, "startId": "u2", "endId": "a2",
            "completedAt": T0 + 20_000, "messageCount": 2, "filePaths": [],
        }))
        .unwrap(),
    );
    let later = message(json!({
        "id": "c2", "role": "assistant", "timestamp": T0 + 15_000, "source": "transcript",
        "blocks": [{ "type": "tool-call", "name": "Bash", "callId": "c2",
                     "input": { "command": "git status" } }],
    }));
    let last = assistant("a2", T0 + 20_000, "The rebuild is running.");
    let mut state = state_with(
        vec![opener, call.clone(), result.clone(), report, last],
        None,
    );
    state
        .transcript_view
        .deferred
        .insert("u1".to_string(), vec![call, result]);
    state
        .transcript_view
        .deferred
        .insert("u2".to_string(), vec![later]);
    assert_eq!(sent_cards(&state).len(), 1, "{:?}", sent_cards(&state));
}
