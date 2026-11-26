//! OMP's tool approval answered from a chat card (`answerSessionChatPrompt`): which dialog an
//! answer may reach, and how its send-queue guard settles it. The answer route itself is
//! answer_http.rs.

use super::*;

pub(crate) fn prompt_changed() -> DomainStateError {
    DomainStateError {
        code: "invalidState",
        message: "The prompt changed before your answer arrived. Answer the one showing now."
            .to_string(),
    }
}

/*
CDXC:SessionChat 2026-11-25 DECISION:
User: a stale card never answers another request; an approval Ghostex cannot tie to its request
is answered in the terminal, with neither Allow nor Deny from the card (also from a card that was
already open); two requests whose text is exactly identical may answer each other.

CDXC:SessionChat 2026-11-25 WHY:
OMP's approval dialog shows its request only when the tool draws one (bash's `Command:`, an MCP
tool's `Arguments:`); for every other tool it shows just "Allow tool: <name>", the same for any two
calls of that tool, and OMP's keys answer whichever dialog is up. So the hook card's Allow and Deny
reach OMP only through a dialog that shows the card's tool and exactly its request
(`omp_permission_request_match`). A dialog that shows no unique request is answered in the
terminal, a dialog for another tool or request refuses the answer as changed, and a card that
names neither its tool nor (by its call id) the stored card cannot be told apart from another:
the thread's stored card can already be the next request's. A card relayed from the coordinator
is named by the relay's own snapshot of the card that was clicked (coordinators/relay.rs
`name_clicked_approval`, for coordinator chats whose Deny named nothing), never by the stored
card. A card that names its call is refused once the stored card is another call's. With no
dialog on screen the card's prompt is gone (changed), and a screen Ghostex could not read is
answered in the terminal: either way no key is typed, since Deny's
Escape would land in whatever OMP shows (a dialog the capture missed, or its running turn).
Since 2026-11-07 this held for Allow only; Deny went to whatever dialog was up.
SEE-ALSO: server/src/session_chat_omp_blocking.rs (`omp_shown_request`), packages/gx-chat-core/src/questions/actions.rs (`with_shown_approval`), packages/gx-chat-core/src/extras/relayed_approvals.rs (`choices`).
*/
/// The card's tool and summary once the dialog on `screen` shows exactly its request.
pub(crate) fn verify_omp_hook_answer_targets_dialog(
    session: &Value,
    params: &serde_json::Map<String, Value>,
    screen: Option<&str>,
) -> Result<(String, String), DomainStateError> {
    let answer_in_terminal = |message: &str| DomainStateError {
        code: crate::session_chat_send::ANSWER_IN_TERMINAL,
        message: message.to_string(),
    };
    let Some(screen) = screen else {
        return Err(answer_in_terminal(
            crate::session_chat_send::OMP_APPROVAL_UNREAD,
        ));
    };
    let Some(dialog) = crate::session_chat_omp_blocking::read_omp_permission_dialog(screen) else {
        return Err(prompt_changed());
    };
    let not_shown = || {
        answer_in_terminal(
            "OMP's approval doesn't show what it asks for, so Ghostex can't tell it's this card's. Answer it in the terminal.",
        )
    };
    let stored = crate::agents::session_chat_prompt_setting(session)
        .as_deref()
        .and_then(crate::session_chat::parse_stored_session_chat_prompt)
        .and_then(|stored| match stored {
            crate::session_chat::SessionChatInteractivePrompt::Approval {
                tool,
                summary,
                tool_use_id,
                ..
            } => Some((tool, summary, tool_use_id)),
            _ => None,
        });
    let shown_call = params.get("toolUseId").and_then(Value::as_str);
    let card = match params.get("promptTool").and_then(Value::as_str) {
        Some(tool) => Some((
            tool.to_string(),
            params
                .get("promptSummary")
                .and_then(Value::as_str)
                .map(str::to_string),
        )),
        // A card that names no tool (a Deny from an older chat) is the stored card only when it
        // names that card's call.
        None => stored.as_ref().and_then(|(tool, summary, tool_use_id)| {
            (tool_use_id.is_some() && shown_call == tool_use_id.as_deref())
                .then(|| (tool.clone(), summary.clone()))
        }),
    };
    if dialog.shown.is_none() {
        return Err(not_shown());
    }
    if let (Some(shown_call), Some((_, _, Some(stored_call)))) = (shown_call, stored.as_ref()) {
        if shown_call != stored_call {
            return Err(prompt_changed());
        }
    }
    let Some((tool, summary)) = card else {
        return Err(not_shown());
    };
    if crate::session_chat::normalize_session_chat_tool_name(&dialog.tool)
        != crate::session_chat::normalize_session_chat_tool_name(&tool)
    {
        return Err(prompt_changed());
    }
    // The dialog's whole request must be the card's (CDXC:SessionChat 2026-11-25 DECISION in
    // session_chat_omp_blocking.rs `omp_permission_request_match`).
    let Some(summary) = summary else {
        return Err(not_shown());
    };
    match crate::session_chat_omp_blocking::omp_permission_request_match(&dialog, &summary) {
        crate::session_chat_omp_blocking::OmpRequestMatch::Same => Ok((tool, summary)),
        crate::session_chat_omp_blocking::OmpRequestMatch::Other => Err(prompt_changed()),
        crate::session_chat_omp_blocking::OmpRequestMatch::NotUnique => Err(not_shown()),
    }
}

/*
CDXC:SessionChat 2026-11-25 WHY:
A pick of one of OMP's rows names the notice it was made on (`noticeDetectedAt`), and the answer
route checks that notice against a fresh detection (`verify_answer_targets_shown_prompt`). The
route then reads the screen once more for the keys, and OMP can have moved to another request
between those two reads (the first answered in the terminal, aborted or timed out). So the pick
is taken from that second read only while it reads as the same notice, with the same rows and
highlight, as the one the answer was verified against; the send worker then types it only while
its own read still shows that dialog, rows and highlight (`omp_row_answer_key`). A pick from a
client that names no notice (one from before 2026-11-05) has no earlier read to bind to: it is
taken from the answer's own read, as before, and still goes through the send worker's guard.

CDXC:SessionChat 2026-11-26 WHY:
The same notice includes the identity of the exact request its rows answer
(`omp_request_identity` in session_chat_omp_blocking.rs), so the detector gives a request whose
words merely read alike a `detectedAt` of its own and a pick made on the earlier one fails the
admission. A named notice without that identity ties the pick to no one request, so it is
answered in the terminal (the owner's decision above `verify_omp_hook_answer_targets_dialog`).
*/
/// The pick of OMP's row `choice_index` on `screen`. When the answer names the notice `verified`
/// it was checked against, only while `screen` shows that notice's dialog: the same notice
/// (`same_notice`: its dialog's words, its exact request and its row labels) with the same row
/// highlighted.
/// `Ok(None)`: `screen` shows no OMP dialog with that row and the answer names no notice.
pub(crate) fn omp_row_pick_on_verified_notice(
    screen: Option<&str>,
    choice_index: usize,
    verified: Option<&crate::session_chat_notice::SessionChatTerminalNotice>,
) -> Result<Option<crate::session_chat_omp_blocking::OmpRowPick>, DomainStateError> {
    let Some(verified) = verified else {
        return Ok(screen.and_then(|screen| {
            crate::session_chat_omp_blocking::omp_row_pick(screen, choice_index)
        }));
    };
    let answer_in_terminal = |message: &str| DomainStateError {
        code: crate::session_chat_send::ANSWER_IN_TERMINAL,
        message: message.to_string(),
    };
    if verified.request_identity().is_none() {
        return Err(answer_in_terminal(
            "This answer doesn't name an OMP approval Ghostex can tie to one request. Answer it in the terminal.",
        ));
    }
    let Some(screen) = screen else {
        return Err(answer_in_terminal(
            crate::session_chat_send::OMP_APPROVAL_UNREAD,
        ));
    };
    let highlight = |notice: &crate::session_chat_notice::SessionChatTerminalNotice| {
        notice.choices.iter().position(|choice| choice.selected)
    };
    crate::session_chat_notice::classify_session_chat_terminal_notice(Some("omp"), screen)
        .filter(|now| now.same_notice(Some(verified)) && highlight(now) == highlight(verified))
        .and_then(|_| crate::session_chat_omp_blocking::omp_row_pick(screen, choice_index))
        .map(Some)
        .ok_or_else(prompt_changed)
}

/// Whether `steps` answer OMP's approval through its send-queue guard.
pub(crate) fn is_omp_approval_guard(steps: &[SessionChatSendStep]) -> bool {
    steps.iter().any(|step| {
        matches!(
            step,
            SessionChatSendStep::GuardOmpApprovalRequest { .. }
                | SessionChatSendStep::GuardOmpApprovalRow(_)
        )
    })
}

/*
CDXC:SessionChat 2026-11-25 WHY:
An OMP approval's answer is settled only by its guard at the front of the send queue, which
can still type nothing (the dialog changed, or the screen could not be read). So this answer
waits for the guard's outcome: a refusal goes back to the card that sent it (it stays open,
and a relayed one is not recorded as applied), and the stored card is retired, and a relayed
answer recorded as applied, only once the keys were typed.
*/
/// Runs the OMP approval guard `steps` for `target` and waits for them: `Err` with the reason
/// when the guard typed nothing or the write failed.
pub(crate) async fn answer_through_omp_guard(
    target: &SessionChatSendTarget,
    steps: Vec<SessionChatSendStep>,
) -> Result<(), DomainStateError> {
    let error = match execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-answer",
        steps,
    )
    .await
    {
        Ok(_) => return Ok(()),
        Err(error) => error,
    };
    let code = if error.message == OMP_APPROVAL_UNREAD {
        ANSWER_IN_TERMINAL
    } else if error.failure == SessionChatSendFailure::Write {
        "internalError"
    } else {
        "invalidState"
    };
    Err(DomainStateError {
        code,
        message: error.message,
    })
}
