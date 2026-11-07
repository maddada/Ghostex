//! A screen dialog's card whose answer gxserver refused (GitHub #458: Codex's "Hooks need review"
//! card said "Codex's dialog changed" on every pick and could not be closed). Until an answer is
//! refused the card can only be answered; once one is, Dismiss closes it and it stops holding the
//! composer. Every test drives the chat core the way a host does and reads the renderer document.

use ghostex_gx_chat_core::questions::gates::terminal_choice_pending;
use ghostex_gx_chat_core::questions::model::TerminalNotice;
use ghostex_gx_chat_core::{
    ChatContext, ChatCore, ChatFrame, ChatRpcMethod, ChatSnapshotFrame, ChatStateFrame,
    ComposerBootRead, Effect, Event, RpcOutcome, StartConfig, UserAction,
};
use serde_json::{json, Value};

/// 2026-11-07 04:30 UTC.
const NOW_MS: f64 = 1_794_025_800_000.0;
const PROJECT: &str = "hooks-project";
const SESSION: &str = "s-hooks";
const DIALOG_ID: &str = "05dcdf2950a9ecfa70429d73e0b1b6b1352854cde71185e77b00f873a63af24a";
const REFUSAL: &str = "Codex's dialog changed. Review the current choices and try again.";

/// The notice gxserver sent for Codex 0.167's startup hook review (read off a live test server).
fn hooks_review() -> Value {
    let rows = [
        "Review hooks",
        "Trust all and continue",
        "Continue without trusting (hooks won't run)",
    ];
    json!({
        "kind": "codexInputBlocked",
        "severity": "info",
        "source": "screen",
        "title": "Hooks need review",
        "detail": "2 hooks are new or changed.\n  Hooks can run outside the sandbox after you trust them.",
        "detectedAt": "2026-11-07T04:27:04.294Z",
        "dialog": {
            "id": DIALOG_ID,
            "title": "Hooks need review",
            "body": "2 hooks are new or changed.\n  Hooks can run outside the sandbox after you trust them.",
            "footer": "enter confirm · esc skip",
            "rows": rows.iter().enumerate().map(|(index, label)| json!({
                "number": index + 1,
                "label": label,
                "description": null,
                "selected": index == 0,
            })).collect::<Vec<_>>(),
            "input": null,
            "inputValue": "",
            "actions": ["confirm", "cancel"],
        },
        "choices": rows.iter().enumerate().map(|(index, label)| json!({
            "index": index,
            "label": label,
            "selected": index == 0,
        })).collect::<Vec<_>>(),
    })
}

struct Chat {
    core: ChatCore,
    seq: i64,
}

impl Chat {
    fn open() -> Self {
        let mut chat = Self {
            core: ChatCore::new(),
            seq: 0,
        };
        let start: StartConfig = serde_json::from_value(json!({
            "clientId": "client-1",
            "projectId": PROJECT,
            "sessionId": SESSION,
        }))
        .expect("the start config parses");
        chat.drive(Event::Start(Box::new(start)));
        let boot: ComposerBootRead = serde_json::from_value(json!({
            "sessionKey": format!("{PROJECT}:{SESSION}"),
            "clientId": "client-1",
        }))
        .expect("the boot read parses");
        chat.drive(Event::ComposerBootRead(Box::new(boot)));
        chat.seq += 1;
        let frame: ChatSnapshotFrame = serde_json::from_value(json!({
            "projectId": PROJECT,
            "sessionId": SESSION,
            "serverId": "server-1",
            "protocolVersion": 1,
            "epoch": 1,
            "seq": chat.seq,
            "status": "ready",
            "agent": "codex",
            "messages": [],
            "hasMore": false,
            "beforeOffset": 0,
        }))
        .expect("the frame parses");
        chat.drive(Event::Frame(Box::new(ChatFrame::Snapshot(Box::new(frame)))));
        chat
    }

    fn drive(&mut self, event: Event) -> Vec<Effect> {
        self.core.handle(event, ChatContext::at(NOW_MS))
    }

    fn screen(&mut self, notice: Value) {
        self.seq += 1;
        let frame: ChatStateFrame = serde_json::from_value(json!({
            "projectId": PROJECT,
            "sessionId": SESSION,
            "serverId": "server-1",
            "protocolVersion": 1,
            "epoch": 1,
            "seq": self.seq,
            "status": "ready",
            "terminalNotice": notice,
        }))
        .expect("the frame parses");
        self.drive(Event::Frame(Box::new(ChatFrame::State(Box::new(frame)))));
    }

    fn act(&mut self, action: Value) -> Vec<Effect> {
        let action: UserAction = serde_json::from_value(action).expect("the action parses");
        self.drive(Event::Action(Box::new(action)))
    }

    /// Picks row `index` on the card and has gxserver refuse it with `REFUSAL`.
    fn pick_refused(&mut self, index: u64) {
        let effects = self.act(json!({
            "type": "answer",
            "answer": { "kind": "terminalDialog", "choiceIndex": index, "dialogId": DIALOG_ID },
        }));
        let request_id = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::SendRpc {
                    request_id, method, ..
                } if *method == ChatRpcMethod::AnswerSessionChatPrompt => Some(*request_id),
                _ => None,
            })
            .expect("the pick goes to gxserver");
        self.drive(Event::RpcSettled {
            request_id,
            outcome: Box::new(RpcOutcome::Err {
                code: Some("invalidState".into()),
                message: REFUSAL.into(),
                endpoint: Some("/api/answerSessionChatPrompt".into()),
            }),
        });
    }

    fn document(&self) -> Value {
        serde_json::to_value(self.core.document()).expect("the document is JSON")
    }

    fn holds_composer(&self) -> bool {
        let state = self.core.state();
        let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());
        terminal_choice_pending(state, notice.as_ref())
    }
}

#[test]
fn a_dialog_card_is_answered_not_closed_while_its_answers_go_through() {
    let mut chat = Chat::open();
    chat.screen(hooks_review());
    assert_eq!(chat.document()["noticeVisible"], true);
    chat.act(json!({ "type": "dismissNotice" }));
    assert_eq!(chat.document()["noticeVisible"], true);
    assert!(chat.holds_composer());
}

#[test]
fn a_refused_pick_says_why_and_dismiss_then_closes_the_card() {
    let mut chat = Chat::open();
    chat.screen(hooks_review());
    chat.pick_refused(2);
    let document = chat.document();
    assert_eq!(
        document["noticeVisible"], true,
        "the refusal brings the card back"
    );
    assert_eq!(document["noticeError"], REFUSAL);

    chat.act(json!({ "type": "dismissNotice" }));
    assert_eq!(chat.document()["noticeVisible"], false);
    assert!(
        !chat.holds_composer(),
        "a closed card no longer holds the chat's messages back"
    );
    // gxserver keeps reporting the same dialog while it is on Codex's screen: it stays closed.
    chat.screen(hooks_review());
    assert_eq!(chat.document()["noticeVisible"], false);
}

#[test]
fn a_refusal_belongs_to_its_own_card() {
    let mut chat = Chat::open();
    chat.screen(hooks_review());
    chat.pick_refused(1);
    // Codex draws a different dialog: the refusal belonged to the old card, so the new one is
    // answered again rather than closed.
    let mut next = hooks_review();
    next["detectedAt"] = json!("2026-11-07T04:31:00.000Z");
    next["dialog"]["id"] = json!("another-dialog");
    chat.screen(next);
    let document = chat.document();
    assert_eq!(document["noticeVisible"], true);
    assert!(document["noticeError"].is_null());
    chat.act(json!({ "type": "dismissNotice" }));
    assert_eq!(chat.document()["noticeVisible"], true);
}
