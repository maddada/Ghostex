//! Approvals in the dock: the permission prompt, folder-trust question or hook Allow / Deny the
//! agent waits on is a row of the dock above the chat box (`workDock.rows`), with the card's own
//! answers as its buttons; once answered it settles to a quiet "Approved" or "Denied" row for a
//! few seconds and then leaves. A thread's "Approved from <coordinator>" does the same.
//!
//! CDXC:SessionChat 2026-11-20 DECISION:
//! User, of an "✓ Approved" bar between the dock and the chat box: "I don't like this 'approved' thingy. pls let's not show multiple bars like this above the dock chat box; the dock/dock bar should be the only one there." On the design page they picked "A. Settles in its row, then gone" (Q4243): an approval that still needs them is a dock row with Yes / Always / No, and once answered the row shows Approved for about 3 seconds and disappears, with no record kept in the chat; a thread's "Approved from <coordinator>" line gets the same treatment. This supersedes the 2026-11-08 "Approved" / "Denied" line in questions/pending_approval.rs, which stayed until the next prompt. The answers, the dimmed busy state and the refusal stay the card's (`questions::actions`), so keyboard answers and the composer's hold are unchanged; only where it is drawn moved. A notice this cannot turn into buttons keeps its card.
//!
//! CDXC:SessionChat 2026-11-20 WHY:
//! A security review of the first version found an approval could wait undrawn and read as less than it grants. So: a row exists only where it is drawn (no row while a question card takes the dock's place; the dock never stays collapsed over one; the desktop draws the card while the chat box is maximized); the row shows the whole command (a Codex command runs from its `$ ` line to the end of the dialog's body) with "Show N more lines" and the agent's warning; a button says "Yes" or "No" only for exactly Yes, Allow, "Yes, proceed" or No, every other answer keeps the agent's words, and the settled row says "Approved" or "Denied" only for those, otherwise the words picked, which supersedes the Yes / Always / No wording above; and a new row's answers wait 400 ms before taking a click (the renderers).
//! SEE-ALSO: extras/relayed_approvals.rs (`choices`, `body`: the same answers a coordinator's relayed card sends), apps/desktop/src/app/native_chat/work_cards.rs and apps/mobile/app/src/chat/native/cards/WorkCards.tsx (the rows), native_chat/composer.rs and apps/mobile/app/src/chat/native/cards/NativeChatCards.tsx (`approvalInDock` hides the card).

use serde_json::{json, Value};

use crate::questions::model::{InteractivePrompt, TerminalNotice};
use crate::questions::notice_state::notice_dismiss_key;
use crate::state::{ChatContext, ChatState};

/// How long an answered approval's row stays, settled, before it leaves.
pub const SETTLE_MS: f64 = 3_000.0;
/// A coordinator's answer older than this when it reaches the thread's chat (a chat opened later,
/// a reconnect) does not flash its row.
const RELAYED_FRESH_MS: f64 = 60_000.0;
const SETTLE_TIMER: &str = "extras.approvalSettle";

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// How Codex's approval dialogs start their heading; they reach the chat as a `codexInputBlocked`
/// screen with the dialog's rows (server/src/session_chat_codex_blocking.rs lists the same words).
const CODEX_APPROVAL_HEADINGS: [&str; 5] = [
    "Would you like to run the following command",
    "Would you like to send input to",
    "Do you want to approve network access to",
    "Would you like to grant these permissions",
    "Would you like to make the following edits",
];

/// Whether a Codex dialog's heading is an approval's: it starts with one of the headings above, or
/// ends with "needs your approval" (an MCP tool's), never merely contains the words.
fn codex_approval_heading(title: &str) -> bool {
    let title = title.trim();
    CODEX_APPROVAL_HEADINGS
        .iter()
        .any(|heading| title.starts_with(heading))
        || title.trim_end_matches('.').ends_with("needs your approval")
}

/// Whether a screen notice is an approval the dock shows as a row: `"permission"` (Claude
/// Code's permission prompt, a Codex approval dialog) or `"trust"` (a folder-trust question).
pub fn approval_kind(notice: &TerminalNotice) -> Option<&'static str> {
    match notice.kind.as_str() {
        "permissionPrompt" => Some("permission"),
        "trustPrompt" => Some("trust"),
        "codexInputBlocked" => notice
            .dialog
            .as_ref()
            .filter(|dialog| codex_approval_heading(&dialog.title))
            .map(|_| "permission"),
        _ => None,
    }
}

/// The approval card on screen as a relayed card reads it (`{kind, notice, prompt, agent}`), or
/// `None` when no approval waits or its card is not drawn.
fn open_card(state: &ChatState) -> Option<Value> {
    // A Ghostex Collab guest only sees what the agent asks, on the card drawn for guests.
    if crate::extras::collab::is_guest(state) {
        return None;
    }
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());
    let prompt = InteractivePrompt::parse(state.session.prompt.as_ref());
    // A question card takes the chat box's place and the dock's with it, so the approval keeps its
    // own card beside the question: it is never waiting without being drawn.
    if prompt.as_ref().is_some_and(InteractivePrompt::is_question)
        && crate::questions::gates::question_card_visible(state, prompt.as_ref(), notice.as_ref())
    {
        return None;
    }
    let agent = state.session.agent.clone().unwrap_or_default();
    if let Some(notice) = notice.as_ref() {
        let kind = approval_kind(notice).unwrap_or_default();
        if !kind.is_empty() && crate::questions::gates::notice_visible(state) {
            let mut card = json!({ "kind": kind, "agent": agent });
            card["notice"] = state.session.terminal_notice.clone().unwrap_or(Value::Null);
            if kind == "permission" {
                if let Some(prompt) = state.session.prompt.clone() {
                    card["prompt"] = prompt;
                }
            }
            return Some(card);
        }
    }
    if matches!(prompt, Some(InteractivePrompt::Approval { .. }))
        && crate::questions::gates::question_card_visible(state, prompt.as_ref(), notice.as_ref())
    {
        return Some(json!({
            "kind": "permission",
            "agent": agent,
            "prompt": state.session.prompt.clone().unwrap_or(Value::Null),
        }));
    }
    None
}

/// A button's word: "Yes" only for exactly Yes, Allow or "Yes, proceed", "No" only for exactly No;
/// every other answer keeps the agent's own words, so a lasting or scoped choice ("Yes, and don't
/// ask again for …") always says what it grants.
fn short_label(label: &str) -> String {
    match label.trim().to_lowercase().as_str() {
        "yes" | "allow" | "yes, proceed" => "Yes".to_string(),
        "no" => "No".to_string(),
        _ => label.to_string(),
    }
}

/// The command a Codex dialog approves: from the first `$ ` line that starts a paragraph (so a
/// wrapped Reason line can't stand in for it) to the end of the dialog's body, blank lines and all,
/// without the dialog's own indent; the dialog's body when it names no command (an edit's diff, a
/// permission's scope).
pub(crate) fn dialog_command(body: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let Some(start) = (0..lines.len()).find(|&i| {
        lines[i].trim_start().starts_with("$ ") && (i == 0 || lines[i - 1].trim().is_empty())
    }) else {
        return body.trim().to_string();
    };
    // The dialog's own indent (that of the `$ ` line) comes off every line; the script's stays.
    let indent = lines[start].len() - lines[start].trim_start().len();
    let mut command = vec![lines[start]
        .trim_start()
        .trim_start_matches("$ ")
        .trim_end()];
    command.extend(lines[start + 1..].iter().map(|line| {
        let lead = line.len() - line.trim_start().len();
        line[lead.min(indent)..].trim_end()
    }));
    command.join("\n").trim_end().to_string()
}

/// A block's first rows for its dock row, and how many wrapped rows "Show N more lines" holds back
/// (0 when it all fits): folded by wrapped rows, so one very long line is cut too
/// (`permission_card::fold_code_rows`).
pub(crate) fn block_preview(text: &str) -> (String, usize) {
    crate::questions::permission_card::fold_code_rows(text, COMMAND_PREVIEW_LINES)
        .unwrap_or_else(|| (text.to_string(), 0))
}

/// How many lines of the command its row shows before "Show N more lines".
const COMMAND_PREVIEW_LINES: usize = 4;

/// The first line of the command, with an ellipsis when there is more.
fn command_line(code: &str) -> String {
    let mut lines = code.lines().map(str::trim).filter(|line| !line.is_empty());
    match (lines.next(), lines.next()) {
        (Some(first), Some(_)) => format!("{first} \u{2026}"),
        (Some(first), None) => first.to_string(),
        _ => String::new(),
    }
}

/// The open approval's row, or `None`.
fn open_row(state: &ChatState) -> Option<Value> {
    let card = open_card(state)?;
    let choices = crate::extras::relayed_approvals::choices(&card);
    if choices.is_empty() {
        return None;
    }
    let (_, title, source, code, _, reason) = crate::extras::relayed_approvals::body(&card);
    // The answer on its way for THIS card (a reply still out for the previous prompt does not make
    // the next one busy).
    let pending = state.questions.pending_approval.as_ref().filter(|_| {
        crate::questions::pending_approval::holds_notice(state)
            || crate::questions::pending_approval::holds_prompt(state)
    });
    let picked_label = pending.map(|pending| {
        crate::questions::notice_choices::without_key_hint(&pending.label).to_string()
    });
    let buttons: Vec<Value> = choices
        .iter()
        .map(|choice| {
            let label = short_label(&choice.label);
            let tooltip = if label == choice.label {
                String::new()
            } else {
                choice.label.clone()
            };
            json!({
                "label": label,
                "tooltip": tooltip,
                "recommended": false,
                "picked": picked_label.as_deref() == Some(choice.label.as_str()),
                "command": { "type": "answer", "answer": choice.answer },
            })
        })
        .collect();
    let key = match TerminalNotice::parse(card.get("notice")) {
        Some(notice) => format!(
            "approval:{}",
            notice_dismiss_key(Some(&notice)).unwrap_or_default()
        ),
        None => format!(
            "approval:{}",
            state.questions.prompt_key.clone().unwrap_or_default()
        ),
    };
    // The whole command, every line: the row shows its first lines and "Show N more lines".
    let command = code.trim_end().trim_start_matches('\n').to_string();
    let (preview, more) = block_preview(&command);
    // Lines gxserver itself left out of a long command ("12 more lines in the terminal").
    let cut = card["notice"]
        .get("permissionRequest")
        .and_then(|request| crate::questions::permission_card::presentation(request, ""))
        .and_then(|details| {
            details
                .get("codeCut")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    // Why the agent flagged it ("Dangerous rm operation …"), on the row itself.
    let note = json!({
        "text": text(&reason, "text").trim(),
        "tone": match text(&reason, "tone") { "" => "warning", tone => tone },
    });
    // The panel a click on the row opens: the command, why it asks, where from, then the full words
    // of an answer whose button says less ("Yes, proceed" as Yes).
    let mut parts: Vec<String> = Vec::new();
    if !command.is_empty() {
        parts.push(format!("```\n{command}\n```"));
    }
    if !cut.is_empty() {
        parts.push(format!("_{cut}_"));
    }
    parts.extend([text(&note, "text").to_string(), source.trim().to_string()]);
    parts.extend(buttons.iter().filter_map(|button| {
        let tooltip = text(button, "tooltip");
        (!tooltip.is_empty()).then(|| format!("**{}**: {tooltip}", text(button, "label")))
    }));
    let body = parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let dismiss = card
        .get("notice")
        .is_some_and(Value::is_object)
        .then(|| json!({ "type": "dismissNotice" }))
        .unwrap_or(Value::Null);
    Some(json!({
        "key": key,
        "kind": "approval",
        "status": "needs",
        "title": if title.is_empty() { "Needs your approval".to_string() } else { title },
        // Drawn as a block of its own (`commandLines`), never cut without saying so: the first
        // lines, then "Show N more lines" (`commandMore`), and gxserver's own cut (`commandCut`).
        "command": command,
        "commandLines": true,
        "commandPreview": preview,
        "commandMore": more,
        "commandCut": cut,
        "note": if text(&note, "text").is_empty() { Value::Null } else { note },
        "body": body,
        "detail": [],
        "age": "",
        "buttons": buttons,
        "busy": pending.is_some(),
        // Why the answer did not land, under the title.
        "error": state.questions.notice_error.clone().unwrap_or_default(),
        "dismiss": dismiss,
        "mention": "",
        "needs": true,
        // The dock stays up for it even while it would step aside (a long draft, the Work panel
        // docked beside the chat), and never stays collapsed over it (`work_cards::dock_with`).
        "stays": true,
    }))
}

/// A settled row: `Approved` or `Denied` (and who from), quiet, no buttons.
fn settled_row(key: &str, line: &Value) -> Value {
    let denied = text(line, "tone") == "bad";
    json!({
        "key": key,
        "kind": "approvalSettled",
        "status": if denied { "dropped" } else { "finished" },
        "title": match text(line, "command") {
            "" => text(line, "text").to_string(),
            command => format!("{} \u{b7} {command}", text(line, "text")),
        },
        "detail": [],
        "age": "",
        "buttons": [],
        "settled": true,
        "needs": false,
        "stays": true,
    })
}

/// The dock's approval rows, first in the dock: the open approval, then what was just answered.
pub fn rows(state: &ChatState) -> Vec<Value> {
    let mut out = Vec::new();
    out.extend(open_row(state));
    if let Some(line) = state.questions.settled_approval.as_ref() {
        out.push(settled_row("approval-settled", line));
    }
    if state.extras.relayed_approvals.line_until.is_some() {
        if let Some(line) = crate::extras::relayed_approvals::audit_lines(state)
            .as_array()
            .and_then(|lines| lines.last())
        {
            // What it answered, after who answered it.
            let question = state
                .session
                .relayed_answers
                .as_ref()
                .and_then(Value::as_array)
                .and_then(|answers| answers.last())
                .map(|answer| command_line(text(answer, "question")))
                .unwrap_or_default();
            let mut line = line.clone();
            line["command"] = json!(question);
            out.push(settled_row("approval-relayed", &line));
        }
    }
    out
}

/// The open approval's command, one line, for its settled row.
pub fn open_command(state: &ChatState) -> String {
    open_card(state)
        .map(|card| command_line(&crate::extras::relayed_approvals::body(&card).3))
        .unwrap_or_default()
}

/// Which card the dock's approval row stands for, `"notice"` or `"prompt"`, so that card steps
/// aside; `null` when no approval is in the dock.
pub fn in_dock(state: &ChatState) -> Value {
    match open_row(state) {
        Some(row) if text(&row, "dismiss").is_empty() && !row["dismiss"].is_object() => {
            json!("prompt")
        }
        Some(_) => json!("notice"),
        None => Value::Null,
    }
}

/// Times the settled rows: each stays [`SETTLE_MS`] from the moment this chat saw it, then
/// leaves. Runs after every event (`extras::settle`).
pub fn settle(state: &mut ChatState, context: &ChatContext) {
    let now = context.now_ms;
    // This chat's own answer.
    match (
        state.questions.settled_approval.is_some(),
        state.questions.settled_until,
    ) {
        (true, None) => state.questions.settled_until = Some(now + SETTLE_MS),
        (true, Some(until)) if until <= now => {
            state.questions.settled_approval = None;
            state.questions.settled_until = None;
        }
        (false, Some(_)) => state.questions.settled_until = None,
        _ => {}
    }
    // The coordinator's newest answer for this thread: a new one that just happened shows.
    let newest = crate::extras::relayed_approvals::audit_lines(state)
        .as_array()
        .and_then(|lines| lines.last())
        .cloned();
    let relayed = &mut state.extras.relayed_approvals;
    let key = newest
        .as_ref()
        .map(|line| format!("{}|{}", text(line, "at"), text(line, "detail")));
    if key != relayed.line_key {
        relayed.line_key = key;
        // Only an answer given just now flashes: the last 12 hours' answers a chat reads when it
        // opens are old news.
        let fresh = newest.as_ref().is_some_and(|line| {
            crate::jstime::parse_iso_millis_utc(text(line, "at"))
                .is_some_and(|at| (now - at).abs() < RELAYED_FRESH_MS)
        });
        relayed.line_until = fresh.then_some(now + SETTLE_MS);
    } else if relayed.line_until.is_some_and(|until| until <= now) {
        relayed.line_until = None;
    }
    let next = [state.questions.settled_until, relayed.line_until]
        .into_iter()
        .flatten()
        .fold(None, |earliest: Option<f64>, at| {
            Some(earliest.map_or(at, |e| e.min(at)))
        });
    match next {
        Some(at) => state
            .core
            .timers
            .arm(SETTLE_TIMER, now, (at - now).max(0.0)),
        None => {
            state.core.timers.cancel(SETTLE_TIMER);
        }
    }
}
