//! A cloud session as claude.ai itself reports it: its state, what it needs from the user, the
//! branch it works on and its conversation, read from Anthropic's sessions API with the Claude
//! login the session started on (the API Claude Code's own `--teleport` reads).
//!
//! CDXC:ClaudeCloud 2026-10-26 WHY: a cloud session that ended its first turn with a question (the task "SPX-123" named a ticket Claude could not find) looked failed in Ghostex: its terminal only said "Running in Claude's cloud", the row stayed idle and its chat was empty, because Ghostex read nothing back but the branch on GitHub. `GET /v1/sessions/<id>` (state, `post_turn_summary`, branch) and `/v1/sessions/<id>/events` (the conversation) answer for the account's OAuth login, so the row's status, the terminal and notifications come from them.

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;
use serde_json::Value;

use super::record::CloudRecord;

const API: &str = "https://api.anthropic.com";
const BETA: &str = "ccr-byoc-2025-07-29";
/// Pages of events read for one conversation, so a runaway session cannot hold a reader forever.
const MAX_EVENT_PAGES: usize = 20;
const EVENT_PAGE_SIZE: usize = 500;

/// The Claude login a cloud session's requests are made with.
pub(super) struct Login {
    tokens: Vec<String>,
    organization: String,
}

/// The login of the session's saved Claude account, or of the current CLI login.
pub(super) fn login_for(
    home: &Path,
    db: &Connection,
    record: &CloudRecord,
) -> Result<Login, String> {
    let (tokens, organization) = match record.raw.get("accountId").and_then(Value::as_str) {
        Some(account_id) => {
            let registry = crate::accounts::store::read(db).map_err(|error| error.message)?;
            let account = registry
                .accounts
                .iter()
                .find(|account| {
                    account.id == account_id
                        && account.provider == crate::accounts::model::Provider::Claude
                })
                .ok_or(
                    "The Claude account this cloud session started on is no longer registered.",
                )?;
            let slot = crate::accounts::claude_resets::ClaudeSlot::read(home, &account.selector)?;
            (slot.tokens(home), slot.organization().to_string())
        }
        None => crate::accounts::claude_resets::default_login_tokens(home)
            .ok_or("Claude Code is not signed in on this computer.")?,
    };
    if tokens.is_empty() {
        return Err("The Claude login of this cloud session has no current sign-in.".to_string());
    }
    Ok(Login {
        tokens,
        organization,
    })
}

fn get(login: &Login, path: &str) -> Result<Value, String> {
    for token in &login.tokens {
        let response = crate::http_agent::agent()
            .get(format!("{API}{path}"))
            .header("Authorization", format!("Bearer {token}"))
            .header("anthropic-beta", BETA)
            .header("anthropic-version", "2023-06-01")
            .header("x-organization-uuid", &login.organization)
            .header("Accept", "application/json")
            .config()
            .timeout_global(Some(Duration::from_secs(20)))
            .build()
            .call();
        match response {
            Ok(response) => {
                return crate::http_agent::read_json(response)
                    .map_err(|_| "claude.ai sent an unreadable answer.".to_string())
            }
            // An older token of the same login; the next one may be current.
            Err(ureq::Error::StatusCode(401 | 403)) => continue,
            Err(ureq::Error::StatusCode(404)) => {
                return Err("claude.ai does not know this session.".to_string())
            }
            Err(error) => return Err(format!("claude.ai did not answer ({error}).")),
        }
    }
    // Every token was refused: a kept Keychain read may hold one that was rotated since.
    crate::accounts::claude_resets::forget_keychain_reads();
    Err("claude.ai refused the session's Claude login.".to_string())
}

/// Where the session is, as claude.ai shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RemoteStatus {
    /// Starting or running a turn.
    Working,
    /// The turn ended asking the user for something; a typed reply answers it.
    NeedsInput,
    /// The turn is paused on a question or permission that only the claude.ai page can answer.
    Blocked,
    /// The turn ended.
    Idle,
    /// The session's computer could not start or stopped with an error.
    Failed,
    /// Archived on claude.ai.
    Archived,
}

impl RemoteStatus {
    /// The `pageStatus` words the row's status follows (page_status.rs).
    pub(super) fn page_status(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::NeedsInput | Self::Blocked | Self::Failed => "needsInput",
            Self::Idle | Self::Archived => "idle",
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::NeedsInput => "needsInput",
            Self::Blocked => "blocked",
            Self::Idle => "idle",
            Self::Failed => "failed",
            Self::Archived => "archived",
        }
    }
}

/// `GET /v1/sessions/<id>`, the parts Ghostex uses.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RemoteSession {
    pub(super) status: Option<RemoteStatus>,
    /// What Claude asks the user for ("paste the SPX-123 ticket title and description here").
    pub(super) needs_action: Option<String>,
    /// Claude's one-line account of where the session stands.
    pub(super) detail: Option<String>,
    /// Why the session's computer failed.
    pub(super) failure: Option<String>,
    /// The branch the session works on and pushes to.
    pub(super) branch: Option<String>,
    /// Changes whenever the session does; the terminal client reloads the conversation on it.
    pub(super) updated_at: Option<String>,
}

impl RemoteSession {
    /// The row's status line for this state, or `None` to keep the one it has.
    pub(super) fn status_line(&self) -> Option<String> {
        match self.status? {
            RemoteStatus::NeedsInput => Some(format!(
                "Needs you: {}",
                self.needs_action.as_ref().or(self.detail.as_ref())?
            )),
            RemoteStatus::Blocked => Some(format!(
                "Needs you on claude.ai: {}",
                self.needs_action.as_ref().or(self.detail.as_ref())?
            )),
            RemoteStatus::Failed => Some(match &self.failure {
                Some(failure) => format!("Failed in Claude's cloud: {failure}"),
                None => "Failed in Claude's cloud".to_string(),
            }),
            RemoteStatus::Archived => Some("Archived on claude.ai".to_string()),
            RemoteStatus::Idle => self.detail.clone(),
            // Replaces what the last turn ended with, which no longer holds.
            RemoteStatus::Working => Some("Working in Claude's cloud".to_string()),
        }
    }
}

fn clean(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// A failure as text: a message string, or an object's `message`/`reason`.
fn failure_text(value: &Value) -> Option<String> {
    clean(value)
        .or_else(|| clean(&value["message"]))
        .or_else(|| clean(&value["reason"]))
        .or_else(|| clean(&value["error"]["message"]))
}

pub(super) fn parse_session(session: &Value) -> RemoteSession {
    let summary = if session["post_turn_summary"].is_object() {
        &session["post_turn_summary"]
    } else {
        &session["external_metadata"]["post_turn_summary"]
    };
    let failure = failure_text(&session["startup_failure"]);
    let status = match session["session_status"].as_str().unwrap_or_default() {
        "running" | "pending" => Some(RemoteStatus::Working),
        "requires_action" => Some(RemoteStatus::Blocked),
        "idle" if summary["status_category"] == "need_input" => Some(RemoteStatus::NeedsInput),
        "idle" if failure.is_some() => Some(RemoteStatus::Failed),
        "idle" => Some(RemoteStatus::Idle),
        "failed" => Some(RemoteStatus::Failed),
        "archived" => Some(RemoteStatus::Archived),
        _ => None,
    };
    let branch = session["session_context"]["outcomes"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|outcome| clean(&outcome["git_info"]["branches"][0]))
        .or_else(|| {
            session["external_metadata"]["current_branches"]
                .as_object()
                .and_then(|branches| branches.values().find_map(clean))
        });
    RemoteSession {
        status,
        needs_action: clean(&summary["needs_action"]),
        detail: clean(&summary["status_detail"]).or_else(|| clean(&summary["recent_action"])),
        failure,
        branch,
        updated_at: clean(&session["updated_at"]),
    }
}

pub(super) fn read_session(login: &Login, cloud_session_id: &str) -> Result<RemoteSession, String> {
    Ok(parse_session(&get(
        login,
        &format!("/v1/sessions/{cloud_session_id}?beta=true"),
    )?))
}

/// One message of the conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Turn {
    /// `user`, `claude` or `system` (the session's computer, or a turn that ended in an error).
    pub(super) from: &'static str,
    pub(super) text: String,
}

/// The text blocks of a message's content (a string, or blocks of which only `text` is kept),
/// and the questions Claude asks with its AskUserQuestion tool.
fn message_text(content: &Value) -> Option<String> {
    if let Some(text) = clean(content) {
        return Some(text);
    }
    let parts: Vec<String> = content
        .as_array()?
        .iter()
        .filter_map(|block| match block["type"].as_str()? {
            "text" => clean(&block["text"]),
            "tool_use" if block["name"] == "AskUserQuestion" => asked_questions(&block["input"]),
            _ => None,
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

/// An AskUserQuestion call's questions and their options, one per line.
fn asked_questions(input: &Value) -> Option<String> {
    let mut lines = Vec::new();
    for question in input["questions"].as_array()? {
        lines.push(clean(&question["question"])?);
        for option in question["options"].as_array().into_iter().flatten() {
            let Some(label) = clean(&option["label"]) else {
                continue;
            };
            lines.push(match clean(&option["description"]) {
                Some(description) => format!("  - {label}: {description}"),
                None => format!("  - {label}"),
            });
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// The conversation in the events: the user's messages, Claude's replies (not its tool calls or
/// subagents), the session's computer's errors and a turn that ended in an error.
pub(super) fn conversation(events: &[Value]) -> Vec<Turn> {
    let mut turns = Vec::new();
    for event in events {
        let top_level = event["parent_tool_use_id"].is_null();
        let turn = match event["type"].as_str().unwrap_or_default() {
            "user" if top_level => message_text(&event["message"]["content"])
                .filter(|_| event["isMeta"] != true && event["isSynthetic"] != true)
                .map(|text| ("user", text)),
            "assistant" if top_level => {
                message_text(&event["message"]["content"]).map(|text| ("claude", text))
            }
            "env_manager_log" if event["data"]["level"] == "error" => {
                clean(&event["data"]["content"]).map(|text| ("system", text))
            }
            "result" if event["is_error"] == true => Some((
                "system",
                clean(&event["result"])
                    .or_else(|| {
                        event["errors"]
                            .as_array()
                            .and_then(|errors| errors.iter().find_map(failure_text))
                    })
                    .unwrap_or_else(|| "The turn ended with an error.".to_string()),
            )),
            _ => None,
        };
        if let Some((from, text)) = turn {
            turns.push(Turn { from, text });
        }
    }
    turns
}

/// Every event of the session, oldest first.
pub(super) fn read_events(login: &Login, cloud_session_id: &str) -> Result<Vec<Value>, String> {
    let mut events = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..MAX_EVENT_PAGES {
        let mut path =
            format!("/v1/sessions/{cloud_session_id}/events?beta=true&limit={EVENT_PAGE_SIZE}");
        if let Some(after) = &after {
            path.push_str(&format!("&after_id={after}"));
        }
        let page = get(login, &path)?;
        if let Some(data) = page["data"].as_array() {
            events.extend(data.iter().cloned());
        }
        after = clean(&page["last_id"]).filter(|id| {
            id.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        });
        if page["has_more"] != true || after.is_none() {
            break;
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The user's SPX-123 session as claude.ai answered on 2026-10-26: the turn ended asking for
    /// the ticket.
    fn asking_session() -> Value {
        json!({
            "session_status": "idle",
            "status_bucket": "blocked",
            "updated_at": "2026-10-26T12:09:12.614719Z",
            "post_turn_summary": {
                "needs_action": "paste the SPX-123 ticket title and description here",
                "status_category": "need_input",
                "status_detail": "SPX-123 not findable in Linear or GitHub; need its details",
            },
            "external_metadata": { "current_branches": { "": "claude/spx-123-25k31c" } },
            "session_context": { "outcomes": [{
                "type": "git_repository",
                "git_info": { "branches": ["claude/spx-123-25k31c"], "repo": "maddada/Ghostex" },
            }]},
        })
    }

    #[test]
    fn a_turn_that_ends_with_a_question_needs_the_user() {
        let session = parse_session(&asking_session());
        assert_eq!(session.status, Some(RemoteStatus::NeedsInput));
        assert_eq!(session.branch.as_deref(), Some("claude/spx-123-25k31c"));
        assert_eq!(
            session.status_line().as_deref(),
            Some("Needs you: paste the SPX-123 ticket title and description here")
        );
        assert_eq!(session.status.unwrap().page_status(), "needsInput");
    }

    #[test]
    fn statuses_follow_claude_ai() {
        let mut value = asking_session();
        value["post_turn_summary"]["status_category"] = json!("done");
        assert_eq!(parse_session(&value).status, Some(RemoteStatus::Idle));
        assert_eq!(
            parse_session(&value).status_line().as_deref(),
            Some("SPX-123 not findable in Linear or GitHub; need its details")
        );
        value["session_status"] = json!("running");
        assert_eq!(parse_session(&value).status, Some(RemoteStatus::Working));
        assert_eq!(
            parse_session(&value).status_line().as_deref(),
            Some("Working in Claude's cloud")
        );
        value["session_status"] = json!("requires_action");
        value["post_turn_summary"]["needs_action"] = json!("Approve or deny AskUserQuestion");
        assert_eq!(parse_session(&value).status, Some(RemoteStatus::Blocked));
        assert_eq!(
            parse_session(&value).status_line().as_deref(),
            Some("Needs you on claude.ai: Approve or deny AskUserQuestion")
        );
        value["session_status"] = json!("failed");
        value["startup_failure"] = json!({ "message": "Could not clone the repository" });
        assert_eq!(
            parse_session(&value).status_line().as_deref(),
            Some("Failed in Claude's cloud: Could not clone the repository")
        );
        value["session_status"] = json!("something new");
        assert_eq!(parse_session(&value).status, None);
    }

    #[test]
    fn the_branch_falls_back_to_the_current_one() {
        let mut value = asking_session();
        value["session_context"] = json!({});
        value["external_metadata"]["current_branches"] = json!({ "": "claude/other-1a2b3c" });
        assert_eq!(
            parse_session(&value).branch.as_deref(),
            Some("claude/other-1a2b3c")
        );
    }

    #[test]
    fn the_conversation_keeps_messages_and_drops_tool_traffic() {
        let events = vec![
            json!({ "type": "user", "parent_tool_use_id": null,
                    "message": { "role": "user", "content": "SPX-123" } }),
            json!({ "type": "env_manager_log",
                    "data": { "level": "info", "content": "Cloning repository" } }),
            json!({ "type": "assistant", "parent_tool_use_id": null, "message": { "content": [
                { "type": "thinking", "thinking": "…" },
                { "type": "tool_use", "name": "Bash", "input": {} },
            ]}}),
            json!({ "type": "user", "parent_tool_use_id": null, "message": { "content": [
                { "type": "tool_result", "content": "## claude/spx-123-25k31c" },
            ]}}),
            json!({ "type": "assistant", "parent_tool_use_id": "toolu_1", "message": { "content": [
                { "type": "text", "text": "a subagent's note" },
            ]}}),
            json!({ "type": "assistant", "parent_tool_use_id": null, "message": { "content": [
                { "type": "text", "text": "I couldn't start on SPX-123." },
            ]}}),
            json!({ "type": "assistant", "parent_tool_use_id": null, "message": { "content": [
                { "type": "tool_use", "name": "AskUserQuestion", "input": { "questions": [{
                    "question": "Which color do you prefer?",
                    "options": [
                        { "label": "Red", "description": "You prefer red." },
                        { "label": "Blue" },
                    ],
                }]}},
            ]}}),
            json!({ "type": "env_manager_log",
                    "data": { "level": "error", "content": "Setup script failed" } }),
            json!({ "type": "result", "is_error": true, "result": "Usage limit reached" }),
        ];
        assert_eq!(
            conversation(&events),
            vec![
                Turn {
                    from: "user",
                    text: "SPX-123".to_string()
                },
                Turn {
                    from: "claude",
                    text: "I couldn't start on SPX-123.".to_string()
                },
                Turn {
                    from: "claude",
                    text: "Which color do you prefer?\n  - Red: You prefer red.\n  - Blue"
                        .to_string(),
                },
                Turn {
                    from: "system",
                    text: "Setup script failed".to_string()
                },
                Turn {
                    from: "system",
                    text: "Usage limit reached".to_string()
                },
            ]
        );
    }
}
