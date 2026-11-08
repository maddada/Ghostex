//! An approval the user answered, from the click until its prompt leaves the agent's screen.
//!
//! CDXC:SessionChat 2026-11-08 DECISION:
//! User: "even in the worker thread had to click on yes 3 times and each time the approval appeared again, let's please make it show a spinner and dim the approval card when i responded already until it finishes". An approval card (a permission prompt read off the screen, a folder-trust question, a hook's Allow / Deny card) no longer hides the moment its answer goes out (the 2026-09-16 and 2026-10-06 rules in actions.rs still hold for every other card). It stays where it was, dimmed, its buttons held, with a spinner and "Sending…" (`approvalPending`), while the answer travels and while the agent's screen still shows the prompt; once the prompt is gone it becomes one line saying "Approved" or "Denied" (drawn like the "Approved from <coordinator>" line, `relayedAnswerLines`). A refused answer brings the card back with its reason, as before. A different prompt ends both. The cause of the card coming back was gxserver reading Claude Code's countdown as part of the prompt (CDXC:SessionChat 2026-11-08 in server/src/session_chat_claude_permission.rs); this keeps an answered card from ever reading as a fresh one meanwhile.
//! SEE-ALSO: packages/gx-chat-core/src/extras/relayed_approvals.rs (a coordinator's relayed cards: `busy` while the answer is in flight, then the answered-here line), apps/desktop/src/app/native_chat/notice.rs and approval.rs (the dimmed card).

use serde_json::{json, Value};

use crate::questions::model::TerminalNotice;
use crate::questions::notice_state::notice_dismiss_key;
use crate::state::ChatState;

/// Which card an answer is pending on.
#[derive(Clone, Debug, PartialEq)]
pub enum PendingCard {
    /// The screen notice, by its `kind:detectedAt` key.
    Notice(String),
    /// The hook's approval card, by its prompt key.
    Prompt(String),
}

/// The approval answer on its way, and once gxserver took it, until the prompt is gone.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingApproval {
    pub card: PendingCard,
    /// What the user picked, as the button read.
    pub label: String,
    pub denied: bool,
    /// gxserver applied it; the card waits for the prompt to leave.
    pub applied: bool,
    /// The prompt left before gxserver's reply, and the line already says what was picked.
    pub settled: bool,
}

/// Whether a picked label refuses.
fn denies(label: &str) -> bool {
    let label = label.trim_start();
    label.starts_with("No") || label.starts_with("Deny") || label.starts_with("Don't")
}

/// The pending answer an approval answer starts, or `None` for any other answer, which keeps the
/// card's older behaviour.
pub fn begin(state: &ChatState, kind: &str, answer: &Value) -> Option<PendingApproval> {
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());
    let notice_key = notice_dismiss_key(notice.as_ref());
    let approval_notice = notice
        .as_ref()
        .is_some_and(|notice| crate::questions::notice_tone::notice_tone(notice) == "approval");
    let row_label = |notice: &TerminalNotice| {
        let index = answer.get("choiceIndex").and_then(Value::as_i64)?;
        notice
            .choices
            .as_ref()?
            .iter()
            .find(|choice| choice.index == index)
            .map(|choice| choice.label.clone())
    };
    let (card, label) = match kind {
        "terminalChoice" | "terminalDialog" if approval_notice => {
            let label = notice.as_ref().and_then(row_label)?;
            (PendingCard::Notice(notice_key?), label)
        }
        "approval" => {
            let allows = answer
                .get("approvalSend")
                .and_then(Value::as_str)
                .is_some_and(|send| !send.is_empty());
            let label = if allows { "Allow" } else { "Deny" }.to_string();
            if answer.get("noticeDetectedAt").is_some() {
                (PendingCard::Notice(notice_key?), label)
            } else {
                (
                    PendingCard::Prompt(state.questions.prompt_key.clone()?),
                    label,
                )
            }
        }
        _ => return None,
    };
    Some(PendingApproval {
        denied: denies(&label),
        card,
        label,
        applied: false,
        settled: false,
    })
}

/// Whether the pending answer is on the card now showing.
fn on_screen(state: &ChatState, pending: &PendingApproval) -> bool {
    match &pending.card {
        PendingCard::Notice(key) => state.questions.active_notice_key.as_ref() == Some(key),
        PendingCard::Prompt(key) => state.questions.prompt_key.as_ref() == Some(key),
    }
}

/// Whether the notice card stays drawn (dimmed) for its pending answer.
pub fn holds_notice(state: &ChatState) -> bool {
    state
        .questions
        .pending_approval
        .as_ref()
        .is_some_and(|pending| {
            matches!(pending.card, PendingCard::Notice(_)) && on_screen(state, pending)
        })
}

/// Whether the approval card stays drawn (dimmed) for its pending answer.
pub fn holds_prompt(state: &ChatState) -> bool {
    state
        .questions
        .pending_approval
        .as_ref()
        .is_some_and(|pending| {
            matches!(pending.card, PendingCard::Prompt(_)) && on_screen(state, pending)
        })
}

/// gxserver answered the call: applied, the card waits for its prompt to go (or settles now if it
/// already went); refused, the card comes back as it was.
pub fn answered(state: &mut ChatState, applied: bool) {
    if !applied {
        // A line settled while the refusal was on its way goes with it.
        if state
            .questions
            .pending_approval
            .take()
            .is_some_and(|pending| pending.settled)
        {
            state.questions.settled_approval = None;
        }
        return;
    }
    if let Some(pending) = state.questions.pending_approval.as_mut() {
        pending.applied = true;
    }
    sync(state);
}

/// After every frame: an applied answer whose prompt left settles to its line; a new prompt or
/// notice puts the line away.
pub fn sync(state: &mut ChatState) {
    let Some(pending) = state.questions.pending_approval.clone() else {
        if state.questions.settled_approval.is_some()
            && (state.questions.active_notice_key.is_some()
                || (state.questions.prompt_key.is_some()
                    && state.questions.prompt_key != state.questions.dismissed_prompt))
        {
            state.questions.settled_approval = None;
        }
        return;
    };
    if on_screen(state, &pending) {
        return;
    }
    // The prompt left: the line says what was picked at once, even while gxserver's reply is
    // still on its way (a refusal then takes it back, `answered`).
    if pending.applied {
        state.questions.pending_approval = None;
    } else if pending.settled {
        return;
    } else if let Some(pending) = state.questions.pending_approval.as_mut() {
        pending.settled = true;
    }
    state.questions.settled_approval = Some(json!({
        "text": if pending.denied { "Denied" } else { "Approved" },
        "detail": format!("Picked \"{}\"", pending.label),
        "tone": if pending.denied { "bad" } else { "good" },
        "at": "",
    }));
}

/// The document's `approvalPending`: `{ label, card }` while the drawn card waits on its answer.
pub fn project(state: &ChatState) -> Value {
    match state.questions.pending_approval.as_ref() {
        Some(pending) if on_screen(state, pending) => json!({
            "label": pending.label,
            "card": match pending.card {
                PendingCard::Notice(_) => "notice",
                PendingCard::Prompt(_) => "prompt",
            },
            "sending": "Sending…",
        }),
        _ => Value::Null,
    }
}
