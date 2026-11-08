//! A voice tool whose call gxserver answered with `approvalRequired` (GitHub #495): the tool tells
//! the model at once that it waits for the user's approval, which the model says; the request
//! shows as a card on the desktop and the phone, as any agent's does. Once the user approves it,
//! the tool runs again with the approval (each call that already went through is not repeated,
//! `loopback::ToolCaller`) and the result is announced; a denial or a lapse is announced too.
//!
//! CDXC:Security 2026-11-08 DECISION:
//! User (#495 follow-up, 2A): "on approvalRequired they show the approval (desktop and phone
//! cards, as the desktop's own calls do) and retry once approved, telling the user by voice that
//! it is waiting for their approval". Only a call whose client holds neither the owner session nor
//! a device token gets here: the owner's devices answer and type without asking.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::conversation::{Announcement, Input};
use super::loopback::{self, PendingApproval, ToolCaller};
use super::tools::{self, ToolMemory, ToolOutcome};
use crate::server::AppState;

const POLL: Duration = Duration::from_secs(2);
const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// How long past its lapse a request is still read, for a clock that runs a little apart.
const GRACE_MS: u64 = 5_000;

/// What the model hears while the approval is pending.
pub(super) fn waiting_outcome(pending: &PendingApproval) -> ToolOutcome {
    ToolOutcome {
        output: json!({
            "ok": true,
            "waitingForApproval": true,
            "summary": pending.summary,
            "note": "Not done yet: this waits for the user's approval on their computer or phone, where Ghostex shows it as a card. Tell the user that in a few words. Once they approve it, it is done and you will hear how it went; don't call the tool again.",
        }),
        ok: true,
    }
}

/// Runs tool `name` for a conversation; when it stopped at an approval, answers
/// [`waiting_outcome`] and finishes the tool in the background once the user decided.
pub(super) async fn run_in_call(
    state: Arc<AppState>,
    memory: Arc<ToolMemory>,
    name: String,
    args: Value,
    tx: mpsc::UnboundedSender<Input>,
) -> ToolOutcome {
    let caller = memory.tool_caller();
    let outcome = tools::run_as(&state, &memory, &caller, &name, &args).await;
    let Some(pending) = caller.take_pending() else {
        return outcome;
    };
    let waiting = waiting_outcome(&pending);
    tokio::spawn(finish_after_approval(
        state, memory, caller, name, args, pending, tx,
    ));
    waiting
}

/// How the user decided on one request.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Decision {
    Approved,
    Denied,
    Lapsed,
    /// The conversation ended while it waited.
    Gone,
}

async fn finish_after_approval(
    state: Arc<AppState>,
    memory: Arc<ToolMemory>,
    caller: ToolCaller,
    name: String,
    args: Value,
    mut pending: PendingApproval,
    tx: mpsc::UnboundedSender<Input>,
) {
    loop {
        let decision = wait_for_decision(&state, &caller, &pending, &tx).await;
        let (text, model_text) = match decision {
            Decision::Gone => return,
            Decision::Denied => (
                format!("You denied it: {}.", pending.summary),
                format!(
                    "The user denied it on their computer or phone, so it was not done: {}. Tell them in a few words.",
                    pending.summary
                ),
            ),
            Decision::Lapsed => (
                format!("Nobody approved it in time: {}.", pending.summary),
                format!(
                    "Nobody approved it in time, so it was not done: {}. Tell the user; they can ask again.",
                    pending.summary
                ),
            ),
            Decision::Approved => {
                caller.approve(&pending);
                let outcome = tools::run_as(&state, &memory, &caller, &name, &args).await;
                if let Some(next) = caller.take_pending() {
                    // The tool's next call needs its own approval (typing, then Enter).
                    announce(
                        &tx,
                        &next,
                        format!("Waiting for your approval: {}.", next.summary),
                        format!(
                            "The user approved the first part; the next part needs their approval too, on their computer or phone: {}. Tell them in a few words.",
                            next.summary
                        ),
                    );
                    pending = next;
                    continue;
                }
                let result = outcome.output.to_string();
                if outcome.ok {
                    (
                        format!("Approved and done: {}.", pending.summary),
                        format!(
                            "The user approved it and it is done: {}. The result: {result}. Tell them in a few words.",
                            pending.summary
                        ),
                    )
                } else {
                    (
                        format!("Approved, but it didn't work: {}.", pending.summary),
                        format!(
                            "The user approved it, but it failed: {}. The result: {result}. Tell them in a few words.",
                            pending.summary
                        ),
                    )
                }
            }
        };
        announce(&tx, &pending, text, model_text);
        return;
    }
}

fn announce(
    tx: &mpsc::UnboundedSender<Input>,
    pending: &PendingApproval,
    text: String,
    model_text: String,
) {
    let _ = tx.send(Input::Announce(vec![Announcement {
        text,
        kind: "approvalDecided",
        project_id: pending.project_id.clone(),
        session_id: pending.session_id.clone(),
        server_id: None,
        model_text: Some(model_text),
    }]));
}

/// Reads the request every two seconds, as the caller that made it, until it is decided.
pub(super) async fn wait_for_decision(
    state: &Arc<AppState>,
    caller: &ToolCaller,
    pending: &PendingApproval,
    tx: &mpsc::UnboundedSender<Input>,
) -> Decision {
    loop {
        tokio::time::sleep(POLL).await;
        if tx.is_closed() {
            return Decision::Gone;
        }
        let read = loopback::with_caller(
            caller.fresh(),
            loopback::call(
                state,
                &pending.read_path,
                json!({"id": pending.id}),
                READ_TIMEOUT,
            ),
        )
        .await;
        let status = read
            .as_ref()
            .ok()
            .and_then(|read| read.pointer("/request/status"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        match status.as_str() {
            "approved" => return Decision::Approved,
            "denied" => return Decision::Denied,
            "expired" | "consumed" => return Decision::Lapsed,
            _ => {}
        }
        if pending.expires_at_ms > 0 && crate::security::now_ms() > pending.expires_at_ms + GRACE_MS
        {
            return Decision::Lapsed;
        }
    }
}
