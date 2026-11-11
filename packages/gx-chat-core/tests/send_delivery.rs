//! A message from the chat box goes to the agent exactly once, is never handed back for a failure
//! time can fix, and stays in the chat saying why for one time cannot (CDXC:SessionChat 2026-11-11
//! in `composer/delivery.rs`). The core is driven as its host drives it: actions in, every effect
//! performed, gxserver's answers chosen by the test.

use ghostex_gx_chat_core::{
    ChatContext, ChatCore, ChatRpcMethod, Effect, Event, RpcOutcome, StorageKey, TranscriptItem,
    UserAction,
};
use serde_json::{json, Value};

/// What a dropped connection reads as on the desktop: words and no code.
const RESET: &str = "Could not read gxserver response: the connection to 127.0.0.1:58744 was \
closed before an answer arrived (Connection reset by peer (os error 54)).";

/// The host of one chat: it performs what the core asks and keeps what a test reads.
struct Host {
    core: ChatCore,
    now_ms: f64,
    turn: u128,
    /// The delivery calls still waiting for gxserver's answer: (request id, method, params).
    out: Vec<(u64, ChatRpcMethod, Value)>,
    /// Every delivery call ever made, in order.
    delivered: Vec<(ChatRpcMethod, Value)>,
    /// The host actions the core raised, by name, with their params.
    host_actions: Vec<(String, Value)>,
    /// Client storage refuses every write.
    storage_fails: bool,
}

fn delivers(method: &ChatRpcMethod) -> bool {
    matches!(
        method,
        ChatRpcMethod::SendSessionChatMessage | ChatRpcMethod::QueueSessionChatPrompt
    )
}

impl Host {
    fn new() -> Self {
        let mut core = ChatCore::new();
        core.state_mut().session.agent = Some("claude".into());
        core.state_mut().core.controller_started = true;
        Self {
            core,
            now_ms: 1_794_358_800_000.0,
            turn: 0,
            out: Vec::new(),
            delivered: Vec::new(),
            host_actions: Vec::new(),
            storage_fails: false,
        }
    }

    fn context(&mut self) -> ChatContext {
        self.turn += 1;
        ChatContext {
            now_ms: self.now_ms,
            ..Default::default()
        }
        .with_random_ids([0x1000 + self.turn, 0x2000 + self.turn])
    }

    /// One event, then everything it asks for until only gxserver's answers are owed.
    fn drive(&mut self, event: Event) {
        let context = self.context();
        let mut effects = self.core.handle(event, context);
        for _ in 0..40 {
            if effects.is_empty() {
                return;
            }
            let mut next = Vec::new();
            for effect in std::mem::take(&mut effects) {
                let answer = match effect {
                    Effect::WriteStorage { key, .. } => Event::StorageWritten {
                        key,
                        error: self.storage_fails.then(|| "The disk is full.".to_string()),
                    },
                    Effect::FlushStorage { store } => Event::StorageWritten {
                        key: StorageKey {
                            store,
                            suffix: String::new(),
                        },
                        error: self.storage_fails.then(|| "The disk is full.".to_string()),
                    },
                    Effect::SendRpc {
                        request_id,
                        method,
                        params,
                    } if delivers(&method) => {
                        self.delivered.push((method.clone(), (*params).clone()));
                        self.out.push((request_id, method, *params));
                        continue;
                    }
                    // The draft push and the reads answer at once.
                    Effect::SendRpc { request_id, .. } => Event::RpcSettled {
                        request_id,
                        outcome: Box::new(RpcOutcome::Ok { result: json!({}) }),
                    },
                    Effect::HostAction { action, params } => {
                        self.host_actions.push((action, *params));
                        continue;
                    }
                    _ => continue,
                };
                let context = self.context();
                next.extend(self.core.handle(answer, context));
            }
            effects = next;
        }
        panic!("the core never settled");
    }

    fn act(&mut self, action: Value) {
        let action: UserAction = serde_json::from_value(action).expect("the action parses");
        self.drive(Event::Action(Box::new(action)));
    }

    fn send(&mut self, text: &str, id: &str) {
        self.act(json!({ "type": "send", "text": text, "sendRequestId": id,
            "draftVersion": { "draftId": format!("draft-{id}"), "revision": 1 } }));
    }

    /// gxserver answers the oldest delivery call still out.
    fn answer(&mut self, outcome: RpcOutcome) {
        let (request_id, ..) = self.out.remove(0);
        self.drive(Event::RpcSettled {
            request_id,
            outcome: Box::new(outcome),
        });
    }

    fn ok(&mut self) {
        self.answer(RpcOutcome::Ok { result: json!({}) });
    }

    fn refuse(&mut self, code: Option<&str>, message: &str) {
        self.answer(RpcOutcome::Err {
            code: code.map(str::to_string),
            message: message.to_string(),
            endpoint: Some("/api/sendSessionChatMessage".into()),
        });
    }

    /// The clock moves on and the host's timer fires.
    fn wait(&mut self, ms: f64) {
        self.now_ms += ms;
        self.drive(Event::Tick);
    }

    /// The user's rows the chat draws: (text, its `startupDelivery`).
    fn rows(&mut self) -> Vec<(String, Value)> {
        let context = self.context();
        ghostex_gx_chat_core::transcript::rows::rows(self.core.state(), &context)
            .into_iter()
            .filter_map(|item| match item {
                TranscriptItem::Message { message } => Some(message),
                _ => None,
            })
            .filter(|message| message.get("role").and_then(Value::as_str) == Some("user"))
            .map(|message| {
                let row = serde_json::to_value(&message).unwrap();
                (
                    row["text"]
                        .as_str()
                        .or(row["markdown"].as_str())
                        .unwrap_or_default()
                        .to_string(),
                    row["startupDelivery"].clone(),
                )
            })
            .collect()
    }

    /// The ids every delivery call so far went under.
    fn ids(&self) -> Vec<String> {
        self.delivered
            .iter()
            .map(|(_, params)| params["sendRequestId"].as_str().unwrap_or("").to_string())
            .collect()
    }

    /// The texts handed back to the chat box.
    fn handed_back(&self) -> Vec<String> {
        self.host_actions
            .iter()
            .filter(|(action, _)| action == "submissionFailed")
            .map(|(_, params)| params["text"].as_str().unwrap_or("").to_string())
            .collect()
    }

    fn error_line(&self) -> Option<String> {
        self.core.state().core.operation_error.clone()
    }
}

#[test]
fn a_message_that_arrives_twice_under_one_id_goes_once() {
    let mut host = Host::new();
    host.send("hello", "id-1");
    // The chat box could not see it taken and sent the same action again, twice.
    host.send("hello", "id-1");
    host.send("hello", "id-1");
    assert_eq!(host.delivered.len(), 1, "one call to gxserver");
    assert_eq!(host.ids(), ["id-1"]);
    assert_eq!(host.rows().len(), 1, "one row for it");
    let document = serde_json::to_value(host.core.document()).unwrap();
    assert_eq!(document["submissionsReceived"], 1);
    assert_eq!(document["sendsTaken"], json!(["id-1"]));
    host.ok();
    // And again after it was delivered: still the same message.
    host.send("hello", "id-1");
    assert_eq!(host.delivered.len(), 1);
    assert!(host.handed_back().is_empty());

    // Another Enter on the same words is another message.
    host.send("hello", "id-2");
    assert_eq!(host.ids(), ["id-1", "id-2"]);
}

#[test]
fn a_send_ghostex_did_not_answer_goes_again_by_itself_under_the_same_id() {
    let mut host = Host::new();
    host.send("hello", "id-1");
    assert!(host.rows()[0].1.is_null(), "a send on its ordinary way");
    // gxserver restarts under it, and stays away for a long while.
    for attempt in 1..=12 {
        host.refuse(None, RESET);
        assert!(
            host.handed_back().is_empty(),
            "attempt {attempt}: never handed back to the chat box"
        );
        assert_eq!(host.error_line(), None, "attempt {attempt}: no error line");
        let rows = host.rows();
        assert_eq!(rows.len(), 1, "attempt {attempt}: one row, the same one");
        assert_eq!(rows[0].1["state"], "waiting");
        let why = rows[0].1["waitReason"].as_str().unwrap();
        assert!(why.starts_with("Couldn't reach Ghostex."), "{why}");
        assert!(!why.contains("os error"), "no socket words on screen");
        let document = serde_json::to_value(host.core.document()).unwrap();
        assert_eq!(document["sendHeld"], why, "the host's log has the reason");
        assert!(host.out.is_empty(), "nothing out while it waits");
        // The reads' ladder: 2 s at first, 15 s at most.
        host.wait(15_000.0);
        assert_eq!(host.out.len(), 1, "attempt {attempt}: it went again");
    }
    host.ok();
    assert_eq!(host.delivered.len(), 13);
    assert!(
        host.ids().iter().all(|id| id == "id-1"),
        "every attempt is the same send to gxserver"
    );
    assert!(host.rows()[0].1.is_null(), "sent: the note is gone");
    assert!(host.handed_back().is_empty());
    assert_eq!(
        host.host_actions
            .iter()
            .filter(|(action, _)| action == "draftSubmitted")
            .count(),
        1
    );
}

#[test]
fn a_starting_or_busy_ghostex_is_waited_out_too() {
    for (code, says) in [
        ("serverStarting", "Ghostex is starting."),
        ("sendInProgress", "Ghostex is still sending this message."),
        ("unreachable", "Couldn't reach Ghostex."),
    ] {
        let mut host = Host::new();
        host.send("hello", "id-1");
        host.refuse(Some(code), "gxserver's own words");
        let rows = host.rows();
        assert_eq!(rows[0].1["state"], "waiting", "{code}");
        assert!(
            rows[0].1["waitReason"].as_str().unwrap().starts_with(says),
            "{code}"
        );
        // Two seconds on it goes again.
        host.wait(1_900.0);
        assert!(host.out.is_empty(), "{code}: not before its time");
        host.wait(200.0);
        assert_eq!(host.out.len(), 1, "{code}");
        host.ok();
        assert_eq!(host.ids(), ["id-1", "id-1"], "{code}");
        assert!(host.handed_back().is_empty(), "{code}");
    }
}

#[test]
fn a_send_ghostex_refused_stays_in_the_chat_and_says_why() {
    let mut host = Host::new();
    host.send("hello", "id-1");
    host.refuse(Some("notFound"), "The session no longer exists.");
    assert!(
        host.handed_back().is_empty(),
        "not moved back to the chat box"
    );
    assert_eq!(
        host.error_line(),
        None,
        "the row says it, not an error line"
    );
    let rows = host.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1["state"], "failed");
    assert_eq!(
        rows[0].1["errorMessage"],
        "Not sent. The session no longer exists."
    );
    let prompt_id = rows[0].1["promptId"].as_str().unwrap().to_string();
    assert!(prompt_id.starts_with("unsent:"));
    // Waiting does not send it.
    host.wait(60_000.0);
    assert!(host.out.is_empty());
    assert_eq!(host.rows()[0].1["state"], "failed", "it stays");

    // Retry: the same message, the same row, the same id.
    host.act(json!({ "type": "retryQueue", "promptId": prompt_id }));
    assert_eq!(host.out.len(), 1);
    assert_eq!(host.ids(), ["id-1", "id-1"]);
    assert_eq!(host.out[0].2["text"], "hello");
    let rows = host.rows();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].1.is_null(), "on its way again");
    host.ok();
    assert_eq!(host.rows().len(), 1);
    assert!(host.handed_back().is_empty());
}

#[test]
fn edit_gives_an_unsent_message_back_to_the_chat_box_and_remove_drops_it() {
    let mut host = Host::new();
    host.send("first", "id-1");
    host.refuse(Some("invalidParams"), "The message is too long.");
    host.send("second", "id-2");
    host.refuse(Some("invalidParams"), "The message is too long.");
    let rows = host.rows();
    assert_eq!(
        rows.iter()
            .map(|(_, note)| note["state"].as_str().unwrap_or(""))
            .collect::<Vec<_>>(),
        ["failed", "failed"]
    );
    let first = rows[0].1["promptId"].as_str().unwrap().to_string();
    let second = rows[1].1["promptId"].as_str().unwrap().to_string();

    host.act(json!({ "type": "editUnsent", "promptId": first }));
    assert_eq!(host.handed_back(), ["first"], "Edit asked for it");
    assert_eq!(host.error_line(), None);
    assert_eq!(host.rows().len(), 1);

    host.act(json!({ "type": "removeQueue", "promptId": second }));
    assert!(host.rows().is_empty());
    assert_eq!(host.handed_back(), ["first"]);
    assert_eq!(host.delivered.len(), 2, "neither went again");
}

#[test]
fn a_send_whose_outcome_ghostex_cannot_tell_is_retried_as_a_new_send() {
    let mut host = Host::new();
    host.send("hello", "id-1");
    host.refuse(
        Some("sendOutcomeUnknown"),
        "Ghostex could not tell whether this message reached the agent.",
    );
    let prompt_id = host.rows()[0].1["promptId"].as_str().unwrap().to_string();
    host.act(json!({ "type": "retryQueue", "promptId": prompt_id }));
    let ids = host.ids();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[1], "id-1", "that id can never be delivered again");
    assert!(!ids[1].is_empty());
}

#[test]
fn the_message_behind_one_that_was_refused_still_goes() {
    let mut host = Host::new();
    host.send("first", "id-1");
    host.send("second", "id-2");
    assert_eq!(host.out.len(), 1, "one at a time, in order");
    host.refuse(Some("notFound"), "The session no longer exists.");
    assert_eq!(host.out.len(), 1, "the second went");
    assert_eq!(host.out[0].2["text"], "second");
    host.ok();
    assert!(host.handed_back().is_empty());
    let rows = host.rows();
    assert_eq!(rows[0].1["state"], "failed");
    assert!(rows[1].1.is_null());
}

#[test]
fn a_queued_message_is_waited_out_and_kept_the_same_way() {
    let mut host = Host::new();
    // A gxserver that keeps a queue, empty so far.
    host.core.state_mut().session.queue_prompts = Some(Vec::new());
    host.act(
        json!({ "type": "queue", "text": "later", "sendRequestId": "id-q",
        "draftVersion": { "draftId": "draft-q", "revision": 1 } }),
    );
    assert_eq!(host.out[0].1, ChatRpcMethod::QueueSessionChatPrompt);
    host.refuse(None, RESET);
    assert!(host.handed_back().is_empty());
    host.wait(2_000.0);
    assert_eq!(host.out.len(), 1, "it went again");
    assert_eq!(host.ids(), ["id-q", "id-q"]);
    host.refuse(Some("badRequest"), "Too many messages are already waiting.");
    assert!(host.handed_back().is_empty());
    let rows = host.rows();
    assert_eq!(rows.len(), 1, "it stays in the chat as a row");
    assert_eq!(rows[0].1["state"], "failed");
    let prompt_id = rows[0].1["promptId"].as_str().unwrap().to_string();
    host.act(json!({ "type": "retryQueue", "promptId": prompt_id }));
    assert_eq!(host.out[0].1, ChatRpcMethod::QueueSessionChatPrompt);
    assert_eq!(host.ids(), ["id-q", "id-q", "id-q"]);
}

#[test]
fn a_call_that_is_out_for_a_while_says_it_is_still_sending() {
    let mut host = Host::new();
    host.send("hello", "id-1");
    host.wait(1_000.0);
    assert!(host.rows()[0].1.is_null(), "an ordinary send says nothing");
    host.wait(2_000.0);
    let rows = host.rows();
    assert_eq!(rows[0].1["state"], "sending");
    assert!(rows[0].1["waitReason"].is_null());
    host.ok();
    assert!(host.rows()[0].1.is_null());
}

#[test]
fn escape_while_a_send_waits_to_go_again_is_the_users_own_cancel() {
    let mut host = Host::new();
    host.send("hello", "id-1");
    host.refuse(None, RESET);
    host.act(json!({ "type": "interrupt" }));
    assert_eq!(host.handed_back(), ["hello"]);
    host.wait(20_000.0);
    assert!(host.out.is_empty(), "it does not go after that");
}

#[test]
fn a_full_disk_neither_stops_a_send_nor_hands_a_sent_message_back() {
    let mut host = Host::new();
    host.storage_fails = true;
    host.send("hello", "id-1");
    assert_eq!(
        host.out.len(),
        1,
        "the message goes without its draft record"
    );
    host.ok();
    assert!(
        host.handed_back().is_empty(),
        "a delivered message is never handed back to be sent again"
    );
    assert_eq!(host.error_line(), None);
    assert_eq!(host.delivered.len(), 1);
}

/// The row a host draws on the Enter itself is the row the core then draws for the message: the
/// same projection, so nothing about it changes when the core's row takes its place.
#[test]
fn the_row_drawn_at_enter_is_the_row_the_core_draws() {
    use ghostex_gx_chat_core::composer::delivery::{sent_row, sent_row_state, SentRow};
    let text = "look at this\n[Image #1](/Users/me/Screenshots/shot@2x.png)\n> a quoted line\n\nand `code`";
    for working in [false, true] {
        let mut host = Host::new();
        if working {
            host.core.state_mut().session.server_working = true;
        }
        host.send(text, "id-1");
        let context = host.context();
        let drawn = ghostex_gx_chat_core::transcript::rows::rows(host.core.state(), &context)
            .into_iter()
            .find_map(|item| match item {
                TranscriptItem::Message { message } => {
                    Some(serde_json::to_value(&message).unwrap())
                }
                _ => None,
            })
            .expect("the core's row");

        let row = SentRow {
            id: "id-1",
            text,
            agent: Some("claude"),
            working,
            on_its_way: false,
            sessions: &[],
        };
        let state = sent_row_state(&row, host.now_ms as i64);
        let at_enter = match sent_row(&state, &context).expect("a row") {
            TranscriptItem::Message { message } => serde_json::to_value(&message).unwrap(),
            other => panic!("a message row, not {other:?}"),
        };
        // Everything a renderer draws is the same; only the row's id differs.
        let mut expected = drawn.clone();
        expected["id"] = at_enter["id"].clone();
        assert_eq!(at_enter, expected, "working: {working}");
        assert_eq!(
            at_enter["queued"] == true,
            working,
            "the play button of a queued message"
        );

        // Once it has waited a moment, the same row with the spinner's `startupDelivery`.
        let waiting = SentRow {
            on_its_way: true,
            ..row
        };
        let state = sent_row_state(&waiting, host.now_ms as i64);
        let TranscriptItem::Message { message } = sent_row(&state, &context).unwrap() else {
            panic!("a message row");
        };
        assert_eq!(message["startupDelivery"]["state"], "sending");
    }
}

/// A message sent while a dialog on the agent's terminal holds its input, with that dialog's card
/// not on screen (answered a moment ago): it is held, says so on its row, and goes by itself the
/// moment the dialog is gone. The silent hold of 11 Nov 03:36 (CDXC:SessionChat 2026-11-11 in
/// `composer/policy.rs` on `hold_words`).
#[test]
fn a_held_message_says_why_on_its_row_and_goes_when_the_hold_ends() {
    let mut host = Host::new();
    host.core.state_mut().session.terminal_notice = Some(json!({
        "kind": "terminalDialog", "severity": "info", "title": "Pick one", "source": "screen",
        "detectedAt": "2026-11-11T01:00:00.000Z",
        "choices": [{ "index": 0, "label": "Yes" }, { "index": 1, "label": "No" }],
    }));
    host.drive(Event::Tick);
    // Its card was answered: no longer drawn, though the agent's screen still shows the dialog.
    let key = host.core.state().questions.active_notice_key.clone();
    assert!(key.is_some(), "the core knows the notice");
    host.core.state_mut().questions.answered_notice_key = key;

    host.send("hello", "id-1");
    assert!(host.out.is_empty(), "held: nothing goes to gxserver");
    assert!(
        host.handed_back().is_empty(),
        "and nothing goes back to the chat box"
    );
    let rows = host.rows();
    assert_eq!(rows.len(), 1, "it shows at once");
    assert_eq!(rows[0].1["state"], "waiting");
    let why = rows[0].1["waitReason"].as_str().unwrap().to_string();
    assert!(why.contains("dialog"), "{why}");
    let document = serde_json::to_value(host.core.document()).unwrap();
    assert_eq!(
        document["sendHeld"],
        why.as_str(),
        "the host's log says what holds it"
    );
    // It stays held however long the dialog stays.
    host.wait(120_000.0);
    assert!(host.out.is_empty());

    // The dialog leaves the agent's screen: the message goes with the very next event.
    host.core.state_mut().session.terminal_notice = None;
    host.drive(Event::Tick);
    assert_eq!(host.out.len(), 1, "released");
    assert_eq!(host.ids(), ["id-1"]);
    let document = serde_json::to_value(host.core.document()).unwrap();
    assert!(document["sendHeld"].is_null());
    host.ok();
    assert!(host.rows()[0].1.is_null());
    assert_eq!(host.delivered.len(), 1, "once");
}
