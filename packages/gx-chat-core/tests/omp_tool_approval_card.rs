//! OMP's inline tool approval as the chat core shows it: gxserver reports it as a pass-through
//! notice, the card takes the chat's keyboard, and its release button hides the card for that one
//! approval only. Every test drives the chat core the way a host does (start, composer boot read,
//! gxserver frames, the user's actions) and reads the document the renderers draw.

use ghostex_gx_chat_core::{
    ActionKind, ChatContext, ChatCore, ChatFrame, ChatSnapshotFrame, ChatStateFrame,
    ComposerBootRead, Event, StartConfig, UserAction,
};
use serde_json::{json, Value};

/// 2026-11-02 20:19 UTC.
const NOW_MS: f64 = 1_793_650_787_000.0;
const PROJECT: &str = "billing-service";
const SESSION: &str = "s-approval";
/// gxserver keeps a screen notice's `detectedAt` when the same words come back within ten minutes
/// of leaving the screen, so OMP's next approval carries the first one's.
const DETECTED_AT: &str = "2026-11-02T20:19:48.000Z";

/// The notice gxserver sends while OMP 18.13 asks to run `command`.
fn approval(command: &str) -> Value {
    json!({
        "kind": "ompInputBlocked",
        "severity": "warning",
        "source": "screen",
        "title": "OMP is waiting for tool approval",
        "detail": "Approve or deny the pending tool call in the terminal before sending a message.",
        "detectedAt": DETECTED_AT,
        "passthrough": true,
        "screenTail": format!(
            "╭─ Allow tool: bash ─────╮\n│ Command: {command} │\n│ ❯ Approve │\n│   Deny │\n╰────────────────────────╯"
        ),
    })
}

struct Chat {
    core: ChatCore,
    seq: i64,
}

impl Chat {
    fn open() -> Self {
        Self::open_as("omp")
    }

    /// A chat on a session of `agent`, as gxserver's first snapshot names it.
    fn open_as(agent: &str) -> Self {
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
        chat.snapshot(agent);
        chat
    }

    fn drive(&mut self, event: Event) {
        self.core.handle(event, ChatContext::at(NOW_MS));
    }

    fn snapshot(&mut self, agent: &str) {
        self.seq += 1;
        let frame: ChatSnapshotFrame = serde_json::from_value(json!({
            "projectId": PROJECT,
            "sessionId": SESSION,
            "serverId": "server-1",
            "protocolVersion": 1,
            "epoch": 1,
            "seq": self.seq,
            "status": "ready",
            "agent": agent,
            "messages": [],
            "hasMore": false,
            "beforeOffset": 0,
        }))
        .expect("the frame parses");
        self.drive(Event::Frame(Box::new(ChatFrame::Snapshot(Box::new(frame)))));
    }

    /// A `sessionChatState` with the terminal notice gxserver reads off OMP's screen, or none.
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

    fn act(&mut self, kind: ActionKind) {
        self.drive(Event::Action(Box::new(UserAction::new(kind))));
    }

    fn document(&self) -> Value {
        serde_json::to_value(self.core.document()).expect("the document is JSON")
    }

    fn card_shown(&self) -> bool {
        let document = self.document();
        document["noticeVisible"] == true && document["terminalNotice"]["passthrough"].is_object()
    }
}

#[test]
fn the_release_button_says_it_hides_the_card_and_answers_nothing() {
    let mut chat = Chat::open();
    chat.screen(Some(approval("echo ghostex-approval-test")));
    assert!(chat.card_shown());
    let passthrough = chat.document()["terminalNotice"]["passthrough"].clone();
    assert_eq!(passthrough["releaseLabel"], "Hide and type in chat");
    let hint = passthrough["hint"].as_str().expect("the card's hint");
    assert!(hint.contains("Hiding it sends nothing"), "{hint}");
    assert!(hint.contains("the agent keeps waiting"), "{hint}");
}

#[test]
fn a_hidden_approval_card_stays_hidden_only_until_omp_closes_that_approval() {
    let mut chat = Chat::open();
    chat.screen(Some(approval("echo ghostex-approval-test")));
    assert!(chat.card_shown());

    // The user hides the card: it stays hidden while OMP keeps asking, through its repaints.
    chat.act(ActionKind::DismissNotice);
    assert!(!chat.card_shown());
    chat.screen(Some(approval("echo ghostex-approval-test")));
    assert!(!chat.card_shown());

    // The call is approved in the terminal, so OMP removes the frame, then asks for the next call
    // within ten minutes, under the same `detectedAt`.
    chat.screen(None);
    chat.screen(Some(approval("echo second-approval")));
    assert!(
        chat.card_shown(),
        "the next approval's card shows, so its keys can reach OMP"
    );
}

/// The permission card gxserver sends for an agent's own approval rows.
fn permission(agent_title: &str, rows: &[&str]) -> Value {
    json!({
        "kind": "permissionPrompt",
        "severity": "warning",
        "source": "screen",
        "title": agent_title,
        "detectedAt": DETECTED_AT,
        "choices": rows
            .iter()
            .enumerate()
            .map(|(index, label)| json!({ "index": index, "label": label, "selected": index == 0 }))
            .collect::<Vec<_>>(),
    })
}

/// OMP's declining row is Deny: Escape answers it and the folded card shows it beside Approve, as
/// Claude Code's No row is (the control).
#[test]
fn omps_deny_row_is_the_declining_row_like_claudes_no() {
    let cases: [(&str, &[&str], Value, Value); 3] = [
        (
            "OMP is asking for permission to proceed",
            &["Approve", "Deny"],
            json!(1),
            json!([0, 1]),
        ),
        (
            "OMP is asking for permission to proceed",
            &["Approve", "Approve bash for this session", "Deny"],
            json!(2),
            json!([0, 2]),
        ),
        (
            "Claude Code is asking for permission to proceed",
            &[
                "Yes",
                "Yes, and don't ask again for this command",
                "No, and tell Claude what to do differently (esc)",
            ],
            json!(2),
            json!([0, 2]),
        ),
    ];
    for (title, rows, escape_row, folded) in cases {
        let mut chat = Chat::open();
        chat.screen(Some(permission(title, rows)));
        let notice = chat.document()["terminalNotice"].clone();
        assert_eq!(notice["secondaryChoice"], escape_row, "{rows:?}");
        assert_eq!(notice["collapsedChoices"], folded, "{rows:?}");
    }
}

/// Empryo's approval card has a Deny row too, but Esc cancels there, so Escape answers no row and
/// the folded card keeps its first two rows.
#[test]
fn empryos_allow_deny_card_leaves_escape_unanswered() {
    let mut chat = Chat::open_as("empryo");
    chat.screen(Some(permission(
        "Empryo is asking for permission",
        &["Allow", "Deny"],
    )));
    let notice = chat.document()["terminalNotice"].clone();
    assert_eq!(notice["choices"][1]["label"], "Deny");
    assert_eq!(notice["secondaryChoice"], Value::Null);
    assert_eq!(notice["collapsedChoices"], json!([0, 1]));
}

#[test]
fn a_stored_approval_steps_aside_while_a_live_notice_holds_a_typed_draft() {
    let mut duplicate_cards = Vec::new();
    for (agent, kind, title, screen) in [
        (
            "omp",
            "ompInputBlocked",
            "OMP is waiting for tool approval",
            "╭─ Allow tool: bash ─────╮\n│ Command: echo approval-test │\n│   Approve │\n│ ❯ Deny and send message │\n├────────────────────────┤\n│ ⏎ deny and send · ⎋ deny │\n╰────────────────────────╯\n──────────────────────────\n N\n──────────────────────────",
        ),
        (
            "codex",
            "codexInputBlocked",
            "Codex is waiting for terminal interaction",
            "Review the pending command\n N\nEnter to continue · Esc to cancel",
        ),
    ] {
        let mut chat = Chat::open();
        let prompt = json!({
            "kind": "approval",
            "tool": "bash",
            "summary": "echo approval-test",
            "toolUseId": "approval-1",
        });
        chat.seq += 1;
        let frame: ChatStateFrame = serde_json::from_value(json!({
            "projectId": PROJECT,
            "sessionId": SESSION,
            "serverId": "server-1",
            "protocolVersion": 1,
            "epoch": 1,
            "seq": chat.seq,
            "status": "ready",
            "agent": agent,
            "prompt": prompt,
        }))
        .expect("the stored approval frame parses");
        chat.drive(Event::Frame(Box::new(ChatFrame::State(Box::new(frame)))));
        assert_eq!(chat.document()["questionCard"]["visible"], true, "{agent}");

        let mut notice = approval("echo approval-test");
        notice["kind"] = json!(kind);
        notice["title"] = json!(title);
        notice["screenTail"] = json!(screen);
        chat.seq += 1;
        let frame: ChatStateFrame = serde_json::from_value(json!({
            "projectId": PROJECT,
            "sessionId": SESSION,
            "serverId": "server-1",
            "protocolVersion": 1,
            "epoch": 1,
            "seq": chat.seq,
            "status": "ready",
            "agent": agent,
            "prompt": prompt,
            "terminalNotice": notice,
        }))
        .expect("the typed draft frame parses");
        chat.drive(Event::Frame(Box::new(ChatFrame::State(Box::new(frame)))));
        let document = chat.document();
        assert!(chat.card_shown(), "{agent}: the draft notice has the keyboard");
        assert_eq!(document["prompt"], prompt, "{agent}: approval stays stored");
        let card_count = usize::from(document["noticeVisible"] == true)
            + usize::from(document["questionCard"]["visible"] == true);
        if card_count != 1 || document["questionCard"]["visible"] != false {
            duplicate_cards.push((agent, card_count));
        }
    }
    assert!(
        duplicate_cards.is_empty(),
        "the draft notice must be the only card: {duplicate_cards:?}"
    );
}
