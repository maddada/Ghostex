//! Closing one approval card must not hide the next one. Every permission picker reaches the chat
//! with the title "Allow this?", so a card closed after its answer was refused used to hide every
//! later approval of that session for 30 minutes, the time other screen notices stay closed when
//! their words come back. Every test drives the chat core the way a host does and reads the
//! renderer document.

use ghostex_gx_chat_core::{
    ChatContext, ChatCore, ChatFrame, ChatRpcMethod, ChatSnapshotFrame, ChatStateFrame,
    ComposerBootRead, Effect, Event, RpcOutcome, StartConfig, UserAction,
};
use serde_json::{json, Value};

/// 2026-11-21 11:30 UTC.
const NOW_MS: f64 = 1_795_260_600_000.0;
const PROJECT: &str = "ghostex-features";
const SESSION: &str = "s-omp";
const REFUSAL: &str = "OMP's approval changed. Review the current choices and try again.";

/// The notice gxserver sends while OMP shows `Allow tool: <tool>` (the box of the 2026-11-21
/// report: a `commit` call from an eval cell), first read at `detected_at`.
fn approval(tool: &str, detected_at: &str) -> Value {
    json!({
        "kind": "permissionPrompt",
        "severity": "warning",
        "source": "screen",
        "title": "Allow this?",
        "detail": format!("Allow tool: {tool}"),
        "detectedAt": detected_at,
        "choices": [
            { "index": 0, "label": "Approve", "selected": true },
            { "index": 1, "label": "Deny", "selected": false },
        ],
    })
}

/// A screen notice that only describes a state, which keeps the 30-minute cooldown.
fn usage_limit(detected_at: &str) -> Value {
    json!({
        "kind": "usageLimit",
        "severity": "warning",
        "source": "screen",
        "title": "Usage limit reached",
        "detail": "The limit resets at 3pm.",
        "detectedAt": detected_at,
    })
}

struct Chat {
    core: ChatCore,
    seq: i64,
    now_ms: f64,
}

impl Chat {
    fn open() -> Self {
        let mut chat = Self {
            core: ChatCore::new(),
            seq: 0,
            now_ms: NOW_MS,
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
            "agent": "omp",
            "messages": [],
            "hasMore": false,
            "beforeOffset": 0,
        }))
        .expect("the frame parses");
        chat.drive(Event::Frame(Box::new(ChatFrame::Snapshot(Box::new(frame)))));
        chat
    }

    fn drive(&mut self, event: Event) -> Vec<Effect> {
        self.core.handle(event, ChatContext::at(self.now_ms))
    }

    /// A `sessionChatState` with the terminal notice gxserver reads off the screen, or none.
    fn screen(&mut self, notice: Option<Value>) {
        self.seq += 1;
        let mut frame = json!({
            "projectId": PROJECT,
            "sessionId": SESSION,
            "serverId": "server-1",
            "protocolVersion": 1,
            "epoch": 1,
            "seq": self.seq,
            "status": "ready",
        });
        if let Some(notice) = notice {
            frame["terminalNotice"] = notice;
        }
        let frame: ChatStateFrame = serde_json::from_value(frame).expect("the frame parses");
        self.drive(Event::Frame(Box::new(ChatFrame::State(Box::new(frame)))));
    }

    fn act(&mut self, action: Value) -> Vec<Effect> {
        let action: UserAction = serde_json::from_value(action).expect("the action parses");
        self.drive(Event::Action(Box::new(action)))
    }

    /// Picks Deny on the card and has gxserver refuse it, which is when a card can be closed.
    fn deny_refused(&mut self) {
        let effects = self.act(json!({
            "type": "answer",
            "answer": { "kind": "terminalChoice", "choiceIndex": 1 },
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

    fn close(&mut self) {
        self.act(json!({ "type": "dismissNotice" }));
    }

    fn card_shown(&self) -> bool {
        let document = serde_json::to_value(self.core.document()).expect("the document is JSON");
        document["noticeVisible"] == true
    }
}

#[test]
fn the_next_approval_shows_its_card_after_one_was_closed() {
    let mut chat = Chat::open();
    chat.screen(Some(approval("commit", "2026-11-21T11:28:00.000Z")));
    assert!(chat.card_shown());
    chat.deny_refused();
    assert!(chat.card_shown(), "the refusal brings the card back");
    chat.close();
    assert!(!chat.card_shown(), "the closed approval is hidden");

    // OMP answers it in the terminal; a minute later it asks for another tool with the same title.
    chat.screen(None);
    chat.now_ms += 60_000.0;
    chat.screen(Some(approval("bash", "2026-11-21T11:31:00.000Z")));
    assert!(
        chat.card_shown(),
        "a new approval with the same title shows its card"
    );
}

#[test]
fn the_closed_approval_itself_stays_hidden() {
    let mut chat = Chat::open();
    let closed = approval("commit", "2026-11-21T11:28:00.000Z");
    chat.screen(Some(closed.clone()));
    chat.deny_refused();
    chat.close();
    // gxserver keeps reporting the same request while it is on OMP's screen.
    chat.screen(Some(closed.clone()));
    assert!(!chat.card_shown());
    chat.screen(None);
    chat.screen(Some(closed));
    assert!(!chat.card_shown(), "the same request, by its own key");
}

#[test]
fn a_state_notice_keeps_its_cooldown() {
    let mut chat = Chat::open();
    chat.screen(Some(usage_limit("2026-11-21T11:28:00.000Z")));
    assert!(chat.card_shown());
    chat.close();
    assert!(!chat.card_shown());
    // The banner misses a read and comes back as a new detection within the 30 minutes.
    chat.screen(None);
    chat.now_ms += 60_000.0;
    chat.screen(Some(usage_limit("2026-11-21T11:40:00.000Z")));
    assert!(
        !chat.card_shown(),
        "the same words stay closed for 30 minutes"
    );
    chat.screen(Some(usage_limit("2026-11-21T12:05:00.000Z")));
    assert!(
        chat.card_shown(),
        "after 30 minutes they are worth showing again"
    );
}
