//! Copy Report: the report gxserver writes about what it sees in this session's terminal
//! (`/api/readSessionChatReport`), copied for the user to send to us.

use serde_json::{json, Value};

use crate::effect::Effect;
use crate::state::TerminalTailState;
use crate::wire::ChatRpcMethod;

/// The toast after a copy, which also says what the Send Bug / Feedback form opening next is for.
const COPIED_TOAST: &str = "Report copied! Paste it into Send Bug / Feedback and send it to us, and it becomes a GitHub issue we can fix.";

/// CDXC:SessionChat 2026-10-25 DECISION:
/// User: a "Copy Report" button where the chat shows that the terminal isn't ready "that copies report about the current state so i can fix it", then "show the Send Bug/Feedback screen after copying with Collect more data with agent selected also so people can send the info to us if they want", with "a friendly toast when they copy telling them they can paste and send this report to us to create a github issue about it". Every chat runs this one sequence: the report from gxserver goes to the clipboard, the toast says what to do with it, and the host opens Send Bug / Feedback with the agent switch on (`sendFeedback` with `collectWithAgent`).
/// SEE-ALSO: server/src/session_chat_report.rs (the report), apps/desktop/src/app/native_chat/composer_not_ready.rs and apps/mobile/app/src/chat/native/cards/ComposerNotReadyCard.tsx (the buttons), apps/desktop/src/app/session_chat/host_actions.rs, apps/gpui-web/src/app/web_host/chat_host.rs and the phone's chat screen (`sendFeedback`).
pub fn request(tail: &mut TerminalTailState, request_id: u64) -> Vec<Effect> {
    // A second click while the first read is out replaces it; only the newest answer is copied.
    tail.report_request = Some(request_id);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadSessionChatReport,
        params: Box::new(json!({})),
    }]
}

/// The answer to [`request`]: copy, toast and open Send Bug / Feedback, or say why there is no
/// report. `None` when `request_id` is not the report read.
pub fn settle(
    tail: &mut TerminalTailState,
    request_id: u64,
    outcome: Result<&Value, String>,
) -> Option<Vec<Effect>> {
    if tail.report_request != Some(request_id) {
        return None;
    }
    tail.report_request = None;
    let report = outcome.map(|result| {
        result
            .get("report")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    });
    Some(match report {
        Ok(report) if !report.trim().is_empty() => vec![
            Effect::Copy { text: report },
            Effect::Toast {
                level: "info".to_string(),
                message: COPIED_TOAST.to_string(),
            },
            Effect::HostAction {
                action: "sendFeedback".to_string(),
                params: Box::new(json!({ "collectWithAgent": true })),
            },
        ],
        Ok(_) => vec![report_failed(String::new())],
        Err(message) => vec![report_failed(message)],
    })
}

fn report_failed(message: String) -> Effect {
    Effect::Toast {
        level: "error".to_string(),
        message: if message.is_empty() {
            "Ghostex could not write the report for this session.".to_string()
        } else {
            format!("Ghostex could not write the report: {message}")
        },
    }
}
