//! A coordinator's threads' approval cards, relayed to its own chat (`coordinatorThreads.approvals`,
//! server/src/coordinators/relay.rs): drawn under the thread's "waiting for an answer" report, in
//! the Approvals pill's menu and on the board's Needs you row, with the thread's own buttons. A
//! click sends the answer the thread's card would send, for the thread's session, naming the card
//! (`relayedFrom`), so gxserver applies it only while that card is still the thread's.
//!
//! CDXC:Coordinators 2026-11-05 DECISION:
//! User: "can we also please bring up those blocks on the sub threads so i answer them here in the coordinator and u pick whatever i picked here instead of having to switch there??". They picked all three places (Q1291 "let's do A B C pls", Q1297 the board row's buttons), Connectors cards, Claude Code and Codex permissions and folder trust (Q1294), Always allow from here with its scope in the tooltip (Q1295), and a live card that says when it was answered in the thread, expired or moved on (Q1293). The card is built from the same prompt or screen notice the thread's chat draws, with the same answer functions (questions/terminal_prompts.rs), so the buttons and what they send match the thread's own card. Only a click here sends one: there is no CLI verb, and gxserver's CLI refuses `relayedFrom`.
//! SEE-ALSO: apps/desktop/src/app/native_chat/relayed_approval.rs (desktop and web), server/src/session_chat_send/answer_http.rs (the relayed lane).

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::questions::model::{InteractivePrompt, TerminalNotice};
use crate::state::ChatState;
use crate::wire::ChatRpcMethod;

/// What this client knows of the relayed cards beyond the frames: answers in flight and sent,
/// and the last refusal per card.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RelayedApprovalsState {
    /// Answers in flight: RPC id to the card's key and the picked label.
    pub calls: BTreeMap<u64, (String, String)>,
    /// Answers gxserver applied, by card key, until a frame says the card ended.
    pub sent: BTreeMap<String, String>,
    /// The last refusal of each card, by key.
    pub errors: BTreeMap<String, String>,
}

/// The line under every open card.
const ONLY_YOU: &str = "Only you can answer this";
/// The report text a waiting thread's report starts with (server/src/coordinators/brief.rs).
const WAITING_REPORT: &str = "Ghostex thread report: waiting for an answer.";
const CODE_PREVIEW_LINES: usize = 4;

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// One button of a relayed card: what it says, how it looks, its tooltip, and the answer it sends
/// (the thread's own).
struct Choice {
    label: String,
    tone: &'static str,
    tooltip: String,
    answer: Value,
}

fn tone_of(label: &str, first: bool) -> &'static str {
    let trimmed = label.trim_start();
    if trimmed.starts_with("No") || trimmed.starts_with("Deny") || trimmed.starts_with("Don't") {
        "deny"
    } else if first {
        "primary"
    } else {
        "secondary"
    }
}

/// The tooltip that names what a lasting choice changes (Q1295).
fn scope_tooltip(kind: &str, label: &str) -> String {
    let lower = label.to_lowercase();
    if kind == "connector" && label == "Always allow" {
        return "Allows this tool in this thread's workspace from now on, so its agents run it without asking. Change it in Settings > Connectors.".to_string();
    }
    if lower.contains("don't ask again") || lower.contains("always") {
        return format!("{label}: applies to that thread's agent from now on, not just this once.");
    }
    if lower.contains("remember") {
        return "Trusts this project for every later session.".to_string();
    }
    String::new()
}

/// The card's buttons, in the thread card's order.
fn choices(card: &Value) -> Vec<Choice> {
    let kind = text(card, "kind");
    let prompt = InteractivePrompt::parse(card.get("prompt"));
    let notice = TerminalNotice::parse(card.get("notice"));
    if kind == "connector" {
        let tool_use_id = prompt
            .as_ref()
            .and_then(InteractivePrompt::tool_use_id)
            .unwrap_or_default()
            .to_string();
        let labels: Vec<String> = prompt
            .as_ref()
            .and_then(|prompt| prompt.questions().first())
            .map(|question| {
                question
                    .options
                    .iter()
                    .map(|option| option.label.clone())
                    .collect()
            })
            .unwrap_or_default();
        return labels
            .into_iter()
            .enumerate()
            .map(|(index, label)| Choice {
                tone: tone_of(&label, index == 0),
                tooltip: scope_tooltip(kind, &label),
                answer: json!({
                    "kind": "question",
                    "toolUseId": tool_use_id,
                    "selections": [{ "indices": [index] }],
                }),
                label,
            })
            .collect();
    }
    if let Some(notice) = notice.as_ref().filter(|notice| {
        !notice.visible_choices().is_empty()
            || notice.actions.as_ref().is_some_and(|a| !a.is_empty())
    }) {
        let mut out: Vec<Choice> = notice
            .visible_choices()
            .into_iter()
            .enumerate()
            .map(|(position, choice)| {
                let label =
                    crate::questions::notice_choices::without_key_hint(&choice.label).to_string();
                Choice {
                    tone: tone_of(&label, position == 0),
                    tooltip: scope_tooltip(kind, &label),
                    answer: crate::questions::terminal_prompts::terminal_notice_choice_answer(
                        Some(notice),
                        choice.index,
                    ),
                    label,
                }
            })
            .collect();
        for action in notice.actions.iter().flatten() {
            if !matches!(action.kind.as_str(), "sendKeys" | "trustAndRemember") {
                continue;
            }
            if let Some(answer) =
                crate::questions::terminal_prompts::terminal_notice_action_answer(notice, action)
            {
                out.push(Choice {
                    tone: tone_of(&action.label, false),
                    tooltip: scope_tooltip(kind, &action.label),
                    label: action.label.clone(),
                    answer,
                });
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    // A hook approval with no screen reading: the thread card's Allow and Deny.
    if let Some(InteractivePrompt::Approval {
        tool,
        summary,
        tool_use_id,
    }) = prompt
    {
        let mut allow = json!({ "kind": "approval", "approvalSend": "1", "promptTool": tool });
        if let Some(summary) = summary {
            allow["promptSummary"] = json!(summary);
        }
        if let Some(id) = tool_use_id {
            allow["toolUseId"] = json!(id);
        }
        return vec![
            Choice {
                label: "Allow".to_string(),
                tone: "primary",
                tooltip: String::new(),
                answer: allow,
            },
            Choice {
                label: "Deny".to_string(),
                tone: "deny",
                tooltip: String::new(),
                answer: json!({ "kind": "approval", "approvalSend": "" }),
            },
        ];
    }
    Vec::new()
}

/// What the card shows above its buttons: `(header, title, source, code, code label, reason)`.
fn body(card: &Value) -> (String, String, String, String, String, Value) {
    let kind = text(card, "kind");
    let prompt = InteractivePrompt::parse(card.get("prompt"));
    let raw_notice = card.get("notice").cloned().unwrap_or(Value::Null);
    let notice = TerminalNotice::parse(card.get("notice"));
    match kind {
        "connector" => {
            let question = prompt
                .as_ref()
                .and_then(|prompt| prompt.questions().first())
                .map(|question| question.question.clone())
                .unwrap_or_default();
            let (title, arguments) = match question.split_once("\n\n") {
                Some((title, rest)) => (title.trim().to_string(), rest.trim().to_string()),
                None => (question.trim().to_string(), String::new()),
            };
            (
                "Needs your approval".to_string(),
                title,
                "Connectors · Ghostex's card in that thread".to_string(),
                arguments,
                "Arguments".to_string(),
                Value::Null,
            )
        }
        "trust" => {
            let notice = notice.unwrap_or_default();
            (
                "Folder trust".to_string(),
                notice.title.clone(),
                notice.detail.clone().unwrap_or_default(),
                String::new(),
                String::new(),
                Value::Null,
            )
        }
        _ => {
            let request = raw_notice.get("permissionRequest");
            if let Some(details) = request.and_then(|request| {
                crate::questions::permission_card::presentation(
                    request,
                    notice
                        .as_ref()
                        .map(|n| n.title.as_str())
                        .unwrap_or_default(),
                )
            }) {
                return (
                    "Permission".to_string(),
                    text(&details, "title").to_string(),
                    text(&details, "source").to_string(),
                    text(&details, "code").to_string(),
                    "Command".to_string(),
                    details.get("reason").cloned().unwrap_or(Value::Null),
                );
            }
            // A Codex dialog names its command on `$ ` lines; a picker card in `approvalCommand`.
            let dialog_commands: Vec<String> = raw_notice["dialog"]["body"]
                .as_str()
                .unwrap_or_default()
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with("$ "))
                .map(|line| line.trim_start_matches("$ ").to_string())
                .collect();
            let command = if dialog_commands.is_empty() {
                text(&raw_notice, "approvalCommand").trim().to_string()
            } else {
                dialog_commands.join("\n")
            };
            match (&notice, &prompt) {
                (Some(notice), _) if !notice.title.is_empty() => (
                    "Permission".to_string(),
                    notice.title.clone(),
                    notice.detail.clone().unwrap_or_default(),
                    command,
                    "Command".to_string(),
                    Value::Null,
                ),
                (_, Some(InteractivePrompt::Approval { tool, summary, .. })) => (
                    "Permission".to_string(),
                    crate::questions::notice_tone::approval_title(
                        Some(text(card, "agent")).filter(|agent| !agent.is_empty()),
                        tool,
                    ),
                    tool.clone(),
                    summary.clone().unwrap_or_default(),
                    "Command".to_string(),
                    Value::Null,
                ),
                _ => (
                    "Permission".to_string(),
                    "The thread is asking for permission".to_string(),
                    String::new(),
                    command,
                    "Command".to_string(),
                    Value::Null,
                ),
            }
        }
    }
}

/// How the thread's "waiting for an answer" report row draws with this card: `full` (the report,
/// the card under it) while it waits, `line` (the card's one line in place of both) once the user
/// answered it here, `hidden` once it ended anywhere else.
///
/// CDXC:Coordinators 2026-11-08 DECISION:
/// User (Q1708 "Yes, remove them"): a relayed approval card disappears from the coordinator's chat (and the Approvals pill, its menu and the board) once it is answered or blocked anywhere other than here, and leaves no "Message from <thread>" row behind; one the user answered here collapses to one small line saying what they chose. The report row only carried the approval, so it goes with the card.
fn report_look(status: &str) -> &'static str {
    match status {
        "open" => "full",
        "answeredHere" => "line",
        _ => "hidden",
    }
}

/// How an ended card reads: `(tone, bold lead, rest)`.
fn settled_words(card: &Value) -> (&'static str, String, String) {
    let answer = text(card, "answer");
    let denied = tone_of(answer, false) == "deny";
    match text(card, "status") {
        "answeredHere" => (
            if denied { "bad" } else { "good" },
            format!(
                "{} here.",
                if answer.is_empty() {
                    "Answered"
                } else {
                    answer
                }
            ),
            format!(
                "The thread shows \"{} from {}\".",
                if denied { "Denied" } else { "Approved" },
                match text(card, "answeredFrom") {
                    "" => "this chat",
                    from => from,
                }
            ),
        ),
        "answeredInThread" => (
            if denied { "bad" } else { "good" },
            "Answered in the thread".to_string(),
            if answer.is_empty() {
                "Nothing left to do here.".to_string()
            } else {
                format!("{answer}. Nothing left to do here.")
            },
        ),
        "expired" => (
            "warn",
            "Expired.".to_string(),
            "Nobody answered within 30 minutes, so it didn't run.".to_string(),
        ),
        _ => (
            "muted",
            "The thread moved on.".to_string(),
            "It no longer asks this.".to_string(),
        ),
    }
}

/// The newest "waiting for an answer" report each thread sent this chat, by thread session id, so
/// its card is drawn under it.
fn report_anchors(state: &ChatState) -> BTreeMap<String, String> {
    let mut anchors = BTreeMap::new();
    for message in &state.messages.composed {
        if message.role != ghostex_gx_protocol::ChatRole::User {
            continue;
        }
        let words = crate::transcript::prose::prose_markdown(&message.blocks);
        if !words.contains(WAITING_REPORT) {
            continue;
        }
        let Some(inter) = crate::transcript::agent_message::parse_inter_agent_message(&words)
        else {
            continue;
        };
        if !inter.body.trim_start().starts_with(WAITING_REPORT) {
            continue;
        }
        // Its `Session ID`, else the last part of `Reply to: S90:P3lv0:G9a3b`.
        let session = Some(inter.session_id.trim())
            .filter(|session| !session.is_empty())
            .or_else(|| inter.reply_to.rsplit(':').next())
            .unwrap_or_default()
            .to_string();
        anchors.insert(session, message.id.clone());
    }
    anchors
}

/// The cards, open ones first: `[{ key, projectId, sessionId, threadTitle, kind, status, header,
/// title, source, code, codePreview, codeMore, codeLabel, reason, choices: [{ label, tone,
/// tooltip }], note, busy, error, settledTone, settledLead, settledText, anchorId }]`.
pub fn project(state: &ChatState) -> Value {
    let Some(cards) = state
        .session
        .coordinator_threads
        .as_ref()
        .and_then(|threads| threads.get("approvals"))
        .and_then(Value::as_array)
    else {
        return json!([]);
    };
    if cards.is_empty() {
        return json!([]);
    }
    let local = &state.extras.relayed_approvals;
    let anchors = report_anchors(state);
    let cards: Vec<Value> = cards
        .iter()
        .filter(|card| !text(card, "key").is_empty())
        .map(|card| {
            let key = text(card, "key");
            let mut status = text(card, "status").to_string();
            let mut answer = text(card, "answer").to_string();
            // An answer gxserver applied reads as answered here until the frame catches up.
            if status == "open" {
                if let Some(label) = local.sent.get(key) {
                    status = "answeredHere".to_string();
                    answer = label.clone();
                }
            }
            let mut shown = card.clone();
            shown["status"] = json!(status);
            shown["answer"] = json!(answer);
            let (header, title, source, code, code_label, reason) = body(card);
            let folded = crate::questions::permission_card::fold_code(&code, CODE_PREVIEW_LINES);
            let busy = local.calls.values().any(|(pending, _)| pending == key);
            let (settled_tone, settled_lead, settled_text) = if status == "open" {
                ("", String::new(), String::new())
            } else {
                settled_words(&shown)
            };
            json!({
                "key": key,
                "projectId": text(card, "projectId"),
                "sessionId": text(card, "sessionId"),
                "threadTitle": text(card, "title"),
                "globalRef": text(card, "globalRef"),
                "kind": text(card, "kind"),
                "status": status,
                "open": status == "open",
                "header": header,
                "title": title,
                "source": source,
                "code": code,
                "codePreview": folded.as_ref().map(|(preview, _)| preview.clone()),
                "codeMore": folded.map(|(_, more)| more),
                "report": report_look(&status),
                "codeLabel": code_label,
                "reason": reason,
                "choices": choices(card)
                    .into_iter()
                    .map(|choice| json!({
                        "label": choice.label,
                        "tone": choice.tone,
                        "tooltip": choice.tooltip,
                    }))
                    .collect::<Vec<_>>(),
                "note": ONLY_YOU,
                "busy": busy,
                "error": local.errors.get(key).cloned().unwrap_or_default(),
                "settledTone": settled_tone,
                "settledLead": settled_lead,
                "settledText": settled_text,
                "settledAt": text(card, "settledAt"),
                "askedAt": text(card, "askedAt"),
                "anchorId": anchors.get(text(card, "sessionId")).cloned().unwrap_or_default(),
            })
        })
        .collect();
    Value::Array(cards)
}

/// How many cards wait on the user, which the Approvals pill counts.
pub fn open_count(projected: &Value) -> usize {
    projected.as_array().map_or(0, |cards| {
        cards.iter().filter(|card| card["open"] == true).count()
    })
}

/// The Approvals pill: shown while a relayed card waits, red like the sidebar's approval dot.
pub fn pill(projected: &Value) -> Option<crate::document::ChatPill> {
    let open = open_count(projected);
    (open > 0).then(|| crate::document::ChatPill {
        switch: None,
        menu: Vec::new(),
        id: APPROVALS_PILL.to_string(),
        label: "Approvals".to_string(),
        count: open.to_string(),
        waiting: "approval".to_string(),
        running: false,
        aria_label: format!(
            "Approvals, {open} {} waiting for you",
            if open == 1 { "thread" } else { "threads" }
        ),
        meta: format!("{open} waiting"),
        dots: crate::extras::chat_pills::pill_dots(0, open as u64, 0),
    })
}

/// The Approvals pill's id; its menu lists the open cards.
pub const APPROVALS_PILL: &str = "approvals";

/// A click on one of a card's buttons (`{ key, choice }`): the thread's own answer, for the
/// thread's session, naming the card and this coordinator. Nothing while that card is not open or
/// already answering.
pub fn answer(state: &mut ChatState, action: &UserAction, request_id: u64) -> Option<Effect> {
    let key = action.param("key").and_then(Value::as_str)?.to_string();
    let choice = action.param("choice").and_then(Value::as_u64)? as usize;
    let card = state
        .session
        .coordinator_threads
        .as_ref()
        .and_then(|threads| threads.get("approvals"))
        .and_then(Value::as_array)?
        .iter()
        .find(|card| text(card, "key") == key && text(card, "status") == "open")?
        .clone();
    let local = &mut state.extras.relayed_approvals;
    if local.sent.contains_key(&key) || local.calls.values().any(|(pending, _)| *pending == key) {
        return None;
    }
    let picked = choices(&card).into_iter().nth(choice)?;
    let mut params = picked.answer;
    params["projectId"] = json!(text(&card, "projectId"));
    params["sessionId"] = json!(text(&card, "sessionId"));
    // The host stamps this chat's own session on every call (desktop `native_chat/state.rs`
    // `rpc`, the phone's `host.ts` `sendRpc`), so the thread rides in `relayedFrom.thread` too.
    params["relayedFrom"] = json!({
        "projectId": state.identity.project_id,
        "sessionId": state.identity.session_id,
        "thread": { "projectId": text(&card, "projectId"), "sessionId": text(&card, "sessionId") },
        "key": key,
        "label": picked.label,
    });
    local.errors.remove(&key);
    local.calls.insert(request_id, (key, picked.label));
    Some(Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AnswerSessionChatPrompt,
        params: Box::new(params),
    })
}

/// Takes the answer to [`answer`]: applied, the card reads "answered here" until gxserver's frame
/// says so; refused, the card shows why and stays open. `false` when `request_id` is not one.
pub fn settle(state: &mut ChatState, request_id: u64, answer: Result<&Value, String>) -> bool {
    let local = &mut state.extras.relayed_approvals;
    let Some((key, label)) = local.calls.remove(&request_id) else {
        return false;
    };
    match answer {
        Ok(_) => {
            local.sent.insert(key, label);
        }
        Err(message) => {
            let message = if message.trim().is_empty() {
                "Could not answer the thread's card. Try again.".to_string()
            } else {
                message
            };
            local.errors.insert(key, message);
        }
    }
    true
}

/// Forgets what this client sent for cards gxserver no longer lists open.
pub fn receive(state: &mut ChatState) {
    let open: Vec<String> = state
        .session
        .coordinator_threads
        .as_ref()
        .and_then(|threads| threads.get("approvals"))
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|card| text(card, "status") == "open")
        .map(|card| text(card, "key").to_string())
        .collect();
    let local = &mut state.extras.relayed_approvals;
    local.sent.retain(|key, _| open.contains(key));
    local.errors.retain(|key, _| open.contains(key));
}

/// A thread's chat: the answers its coordinator's chat sent for it (`relayedAnswers`), as the
/// lines drawn where its card was: `[{ text, at }]`, oldest first.
pub fn audit_lines(state: &ChatState) -> Value {
    let lines = state
        .session
        .relayed_answers
        .as_ref()
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    Value::Array(
        lines
            .iter()
            .map(|line| {
                let label = text(line, "label");
                let verb = if tone_of(label, false) == "deny" {
                    "Denied"
                } else {
                    "Approved"
                };
                let from = match text(line, "from") {
                    "" => "the coordinator",
                    from => from,
                };
                let question = text(line, "question");
                json!({
                    "text": format!("{verb} from {from}"),
                    "detail": if question.is_empty() {
                        format!("Picked \"{label}\"")
                    } else {
                        format!("Picked \"{label}\" for: {question}")
                    },
                    "tone": if verb == "Denied" { "bad" } else { "good" },
                    "at": text(line, "at"),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionKind;

    fn state_with(approvals: Value) -> ChatState {
        let mut state = ChatState::default();
        state.identity.project_id = "P1".to_string();
        state.identity.session_id = "G-coordinator".to_string();
        state.session.coordinator_threads = Some(json!({ "approvals": approvals }));
        state
    }

    fn connector_card() -> Value {
        json!({
            "key": "k1",
            "kind": "connector",
            "status": "open",
            "projectId": "P1",
            "sessionId": "G-thread",
            "title": "Outdated deps audit",
            "prompt": {
                "kind": "question",
                "toolUseId": "connectorcard:abc",
                "questions": [{
                    "question": "Allow this agent to run COMPOSIO_REMOTE_BASH_TOOL through Composio?\n\n{\n  \"command\": \"gh pr list\"\n}",
                    "options": [{"label": "Approve"}, {"label": "Always allow"}, {"label": "Deny"}],
                }],
            },
        })
    }

    fn click(key: &str, choice: u64) -> UserAction {
        UserAction {
            kind: ActionKind::AnswerRelayedApproval,
            params: json!({ "key": key, "choice": choice })
                .as_object()
                .cloned()
                .unwrap(),
        }
    }

    #[test]
    fn a_connector_card_shows_the_threads_buttons_and_arguments() {
        let state = state_with(json!([connector_card()]));
        let cards = project(&state);
        let card = &cards[0];
        assert_eq!(
            card["title"],
            "Allow this agent to run COMPOSIO_REMOTE_BASH_TOOL through Composio?"
        );
        assert!(card["code"].as_str().unwrap().contains("gh pr list"));
        let labels: Vec<_> = card["choices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["label"].clone())
            .collect();
        assert_eq!(
            labels,
            vec![json!("Approve"), json!("Always allow"), json!("Deny")]
        );
        assert!(card["choices"][1]["tooltip"]
            .as_str()
            .unwrap()
            .contains("workspace"));
        assert_eq!(card["choices"][2]["tone"], "deny");
        assert_eq!(pill(&cards).unwrap().count, "1");
    }

    #[test]
    fn a_click_sends_the_threads_own_answer_for_the_thread_naming_the_card() {
        let mut state = state_with(json!([connector_card()]));
        let Some(Effect::SendRpc { params, method, .. }) = answer(&mut state, &click("k1", 1), 9)
        else {
            panic!("a click on an open card sends");
        };
        assert_eq!(method, ChatRpcMethod::AnswerSessionChatPrompt);
        assert_eq!(params["sessionId"], "G-thread");
        assert_eq!(params["kind"], "question");
        assert_eq!(params["toolUseId"], "connectorcard:abc");
        assert_eq!(params["selections"][0]["indices"][0], 1);
        assert_eq!(params["relayedFrom"]["sessionId"], "G-coordinator");
        assert_eq!(params["relayedFrom"]["thread"]["sessionId"], "G-thread");
        assert_eq!(params["relayedFrom"]["key"], "k1");
        assert_eq!(params["relayedFrom"]["label"], "Always allow");
        // A second click while the first is in flight sends nothing.
        assert!(answer(&mut state, &click("k1", 0), 10).is_none());
        assert!(settle(&mut state, 9, Ok(&json!({}))));
        assert_eq!(project(&state)[0]["status"], "answeredHere");
    }

    #[test]
    fn a_hook_approval_without_a_screen_reading_offers_allow_and_deny() {
        let card = json!({
            "key": "k2", "kind": "permission", "status": "open",
            "projectId": "P1", "sessionId": "G-thread",
            "prompt": {"kind": "approval", "tool": "Bash", "summary": "rm -rf tmp", "toolUseId": "toolu_9"},
        });
        let mut state = state_with(json!([card]));
        let Some(Effect::SendRpc { params, .. }) = answer(&mut state, &click("k2", 0), 3) else {
            panic!("Allow sends");
        };
        assert_eq!(params["approvalSend"], "1");
        assert_eq!(params["promptTool"], "Bash");
        assert_eq!(params["promptSummary"], "rm -rf tmp");
        assert_eq!(project(&state)[0]["code"], "rm -rf tmp");
    }

    #[test]
    fn a_permission_prompt_on_screen_sends_its_rows_with_the_notice_it_was_picked_on() {
        let card = json!({
            "key": "k3", "kind": "permission", "status": "open",
            "projectId": "P1", "sessionId": "G-thread",
            "notice": {
                "kind": "permissionPrompt", "title": "Claude Code is asking for permission",
                "detectedAt": "2026-11-05T10:00:00.000Z",
                "choices": [
                    {"index": 0, "label": "Yes"},
                    {"index": 1, "label": "Yes, and don't ask again for rm commands"},
                    {"index": 2, "label": "No"},
                ],
            },
        });
        let mut state = state_with(json!([card]));
        let cards = project(&state);
        assert_eq!(cards[0]["choices"][2]["tone"], "deny");
        assert!(!cards[0]["choices"][1]["tooltip"]
            .as_str()
            .unwrap()
            .is_empty());
        let Some(Effect::SendRpc { params, .. }) = answer(&mut state, &click("k3", 2), 4) else {
            panic!("No sends");
        };
        assert_eq!(params["kind"], "terminalChoice");
        assert_eq!(params["choiceIndex"], 2);
        assert_eq!(params["noticeDetectedAt"], "2026-11-05T10:00:00.000Z");
    }

    #[test]
    fn an_ended_card_says_how_and_a_refusal_shows_on_the_open_one() {
        let mut ended = connector_card();
        ended["status"] = json!("answeredInThread");
        ended["answer"] = json!("Always allow");
        let state = state_with(json!([ended]));
        let card = &project(&state)[0];
        assert_eq!(card["settledLead"], "Answered in the thread");
        assert!(card["settledText"]
            .as_str()
            .unwrap()
            .starts_with("Always allow"));
        assert!(pill(&project(&state)).is_none());
        // Answered anywhere but here: the card and its report row leave the chat.
        assert_eq!(card["report"], "hidden");

        let mut here = connector_card();
        here["status"] = json!("answeredHere");
        here["answer"] = json!("Approve");
        let card = &project(&state_with(json!([here])))[0];
        assert_eq!(card["report"], "line");
        assert_eq!(
            project(&state_with(json!([connector_card()])))[0]["report"],
            "full"
        );

        let mut state = state_with(json!([connector_card()]));
        answer(&mut state, &click("k1", 0), 5);
        settle(
            &mut state,
            5,
            Err("That approval changed before your answer arrived.".into()),
        );
        let card = &project(&state)[0];
        assert_eq!(card["status"], "open");
        assert!(card["error"].as_str().unwrap().contains("changed"));
    }
}
