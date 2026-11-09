//! While a call with a GBot is open: what its chat is doing while it works ("Working:" updates),
//! the Ghostex approvals the chat waits on, ending the call when GBots are turned off, and the
//! call's line in the GBot's daily log once it ends.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! "It says what it is doing while it works": a turn of a GBot's chat can run for minutes, and a call that goes quiet that long sounds dead. Every few seconds the chat's newest work (its latest tool call and its latest note to itself, read from the transcript like `read_session` reads it) becomes a "Working:" update when it changed, at most every 20 seconds and only after the turn has run for 8, so a quick answer is never talked over. Updates wait for a quiet moment like every announcement (`conversation.rs`).
//! CDXC:GBots 2026-11-09 WHY:
//! A Ghostex approval (`security/approvals.rs`, the Touch ID kind) that the GBot's chat asked for is not one of the chat's own prompts, so the announcement watcher never hears of it. This watcher reads the pending requests whose `requestedBy` is the call's chat and tells the model once per request, as something only the user's computer or phone can approve.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::mpsc;

use super::super::conversation::{Announcement, Input};
use super::super::focus::FocusScope;
use super::super::sessions::{self, VoiceState};
use super::super::tools::read_chat;
use crate::gbots::CallBot;
use crate::server::AppState;

const POLL: Duration = Duration::from_secs(4);
/// A turn says what it is doing only once it has run this long.
const FIRST_UPDATE_AFTER: Duration = Duration::from_secs(8);
/// The least time between two "Working:" updates.
const UPDATE_EVERY: Duration = Duration::from_secs(20);
/// Transcript rows read for the chat's newest work.
const ROWS: usize = 40;
/// Characters kept of the chat's newest note.
const NOTE_CHARS: usize = 240;

/// Starts the watcher of the call `tx` feeds; it stops when the call ends.
pub(in crate::voice_agent) fn watch(
    state: &Arc<AppState>,
    scope: &FocusScope,
    tx: mpsc::UnboundedSender<Input>,
) {
    let Some(bot) = scope.gbot.clone() else {
        return;
    };
    let (state, scope) = (state.clone(), scope.clone());
    super::tools::reset_sent(&scope.reference());
    tokio::spawn(async move {
        let started = Instant::now();
        let mut work = Work::default();
        let mut approvals: HashSet<String> = HashSet::new();
        loop {
            tokio::time::sleep(POLL).await;
            if tx.is_closed() {
                break;
            }
            let paths = state.paths.clone();
            let on = tokio::task::spawn_blocking(move || crate::gbots::gbots_on(&paths))
                .await
                .unwrap_or(true);
            if !on {
                let _ = tx.send(Input::GbotsOff(format!(
                    "GBots were turned off, so the call with {} ended.",
                    bot.name
                )));
                break;
            }
            for line in pending_approvals(&state, &scope, &bot, &mut approvals) {
                let _ = tx.send(Input::Announce(vec![line]));
            }
            if let Some(line) = work.next(&state, &scope, &bot).await {
                let _ = tx.send(Input::Announce(vec![line]));
            }
        }
        let minutes = started.elapsed().as_secs().div_ceil(60);
        let messages = super::tools::take_sent(&scope.reference());
        let (paths, bot_id, title) = (state.paths.clone(), bot.bot_id.clone(), scope.title.clone());
        let _ = tokio::task::spawn_blocking(move || {
            crate::gbots::log_gbot_call(&paths, &bot_id, &title, minutes, messages);
        })
        .await;
    });
}

fn announcement(
    scope: &FocusScope,
    kind: &'static str,
    text: String,
    model_text: String,
) -> Announcement {
    Announcement {
        text,
        kind,
        project_id: scope.project_id.clone(),
        session_id: scope.session_id.clone(),
        server_id: None,
        model_text: Some(model_text),
    }
}

/// The new Ghostex approvals the call's chat waits on, once each.
fn pending_approvals(
    state: &AppState,
    scope: &FocusScope,
    bot: &CallBot,
    told: &mut HashSet<String>,
) -> Vec<Announcement> {
    let pending = state
        .security
        .approvals
        .list_pending(crate::security::now_ms());
    let mut lines = Vec::new();
    for request in pending {
        let from_chat = request.requested_by.project_id.as_deref()
            == Some(scope.project_id.as_str())
            && request.requested_by.session_id.as_deref() == Some(scope.session_id.as_str());
        if !from_chat || !told.insert(request.id.clone()) {
            continue;
        }
        lines.push(announcement(
            scope,
            "gbotApproval",
            format!("{} needs your approval: {}", bot.name, request.summary),
            format!(
                "Ghostex approval: before you can go on, Ghostex needs the user's approval for this: {}. Ask the user out loud to approve or deny it on their computer or phone, where Ghostex shows it (Touch ID on a Mac). You cannot approve it in this call, and a spoken yes does not approve it.",
                request.summary
            ),
        ));
    }
    lines
}

/// The chat's turn as the watcher follows it.
#[derive(Default)]
struct Work {
    since: Option<Instant>,
    last_update: Option<Instant>,
    last_said: String,
}

impl Work {
    /// A "Working:" update when the chat has worked long enough and does something new.
    async fn next(
        &mut self,
        state: &Arc<AppState>,
        scope: &FocusScope,
        bot: &CallBot,
    ) -> Option<Announcement> {
        let all = sessions::list_sessions_async(state).await.ok()?;
        let reference = scope.reference();
        let session = all
            .iter()
            .find(|session| session.reference() == reference)?;
        if session.state != VoiceState::Working {
            self.since = None;
            self.last_said.clear();
            return None;
        }
        let now = Instant::now();
        let since = *self.since.get_or_insert(now);
        let due = now.duration_since(since) >= FIRST_UPDATE_AFTER
            && self
                .last_update
                .is_none_or(|last| now.duration_since(last) >= UPDATE_EVERY);
        if !due {
            return None;
        }
        let chat = read_chat(state, session, ROWS).await.ok()?;
        let doing = current_work(&chat)?;
        if doing == self.last_said {
            return None;
        }
        self.last_said = doing.clone();
        self.last_update = Some(now);
        Some(announcement(
            scope,
            "gbotWorking",
            format!("{} is {}", bot.name, doing.trim_end_matches('.')),
            format!("Working: your chat is {doing} Tell the user in a few words, as yourself, unless you just said the same."),
        ))
    }
}

/// What the chat's current turn is doing: its newest tool call in plain words and its newest
/// note, from the messages after the user's last one.
fn current_work(chat: &Value) -> Option<String> {
    let messages = chat.get("messages")?.as_array()?;
    let start = messages
        .iter()
        .rposition(|message| {
            crate::ghostex_cli::session_chat_transcript::effective_role(message) == "user"
        })
        .map_or(0, |index| index + 1);
    let mut tool: Option<String> = None;
    let mut note: Option<String> = None;
    for message in &messages[start..] {
        for block in message
            .get("blocks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match block.get("type").and_then(Value::as_str) {
                Some("tool-call") => {
                    tool = block.get("name").and_then(Value::as_str).map(tool_words);
                }
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        let text = text.trim();
                        if !text.is_empty() {
                            note = Some(short(text));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    match (tool, note) {
        (None, None) => None,
        (Some(tool), None) => Some(format!("{tool}.")),
        (None, Some(note)) => Some(format!("working; its latest note: \"{note}\"")),
        (Some(tool), Some(note)) => Some(format!("{tool}; its latest note: \"{note}\"")),
    }
}

fn short(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= NOTE_CHARS {
        return line;
    }
    let cut: String = line.chars().take(NOTE_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// A tool call's name as words a person says ("running a command"); never the name itself when it
/// would be read as code.
fn tool_words(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let words = if lower.starts_with("mcp__") {
        let server = lower.split("__").nth(1).unwrap_or("a connected app");
        return format!("using {}", server.replace(['_', '-'], " "));
    } else if lower.contains("websearch")
        || lower.contains("web_search")
        || lower.contains("webfetch")
        || lower.contains("fetch")
    {
        "looking things up online"
    } else if matches!(
        lower.as_str(),
        "read" | "view" | "read_file" | "notebookread"
    ) {
        "reading files"
    } else if lower.contains("edit") || lower.contains("write") || lower.contains("patch") {
        "editing files"
    } else if lower.contains("grep")
        || lower.contains("glob")
        || lower.contains("search")
        || lower == "ls"
    {
        "searching files"
    } else if lower.contains("bash")
        || lower.contains("exec")
        || lower.contains("shell")
        || lower.contains("command")
    {
        "running a command"
    } else if lower == "task" || lower == "agent" || lower.contains("subagent") {
        "handing part of the work to a helper"
    } else if lower.contains("todo") || lower.contains("plan") {
        "planning the steps"
    } else {
        "working"
    };
    words.to_string()
}
